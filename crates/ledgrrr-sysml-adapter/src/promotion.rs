//! Serialized provider effects; a durable authorization is never a resend token.
use crate::{
    client::NativeClient,
    native::{self, NativeElement, OperationMarker},
};
use ledgrrr_revision_io::*;
use std::path::PathBuf;
use ufo_types::revision::{merge_models, MergeOutcome, PortableModel};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Store(#[from] ledgrrr_revision_io::Error),
    #[error(transparent)]
    Native(#[from] native::Error),
    #[error(transparent)]
    Transport(#[from] crate::client::Error),
    #[error(transparent)]
    Revision(#[from] ufo_types::revision::RevisionError),
    #[error("authority unavailable: {0}")]
    Unavailable(String),
}
pub type Result<T> = std::result::Result<T, Error>;

pub struct Owner {
    pub store_path: PathBuf,
    pub actor: ActorId,
    pub client: NativeClient,
}
impl Owner {
    pub fn store(&self) -> Result<Store> {
        Ok(Store::open(&self.store_path)?)
    }

    /// Every snapshot is complete and verified against its immutable owner envelope.
    pub async fn export(
        &self,
        actor: &ActorId,
        project: &ProjectId,
        revision: &str,
    ) -> Result<PortableBundle> {
        let mut store = self.store()?;
        let binding = store.project_binding(actor, project)?;
        let elements = self
            .client
            .snapshot(&binding.remote_project, revision, false)
            .await?;
        let reference = native::marker_from_elements(&elements)?;
        if reference.marker.project != project.as_str() {
            return Err(Error::Unavailable("foreign project marker".into()));
        }
        let operation = store.operation(
            &self.actor,
            project,
            &OperationId::new(&reference.marker.operation)?,
        )?;
        if operation.actual_revision.as_ref().map(RevisionId::as_str) != Some(revision) {
            return Err(Error::Unavailable(
                "revision has no accepted owner observation".into(),
            ));
        }
        let bytes = store.artifact(actor, project, &reference.envelope)?;
        Ok(native::hydrate(&elements, &bytes, &reference.marker)?)
    }

    async fn bound_snapshot(
        &self,
        store: &mut Store,
        project: &ProjectId,
        branch: &BranchId,
        remote_project: &str,
        revision: &str,
    ) -> Result<(Option<PortableBundle>, Vec<NativeElement>)> {
        let elements = self
            .client
            .snapshot(remote_project, revision, false)
            .await?;
        if elements.is_empty() {
            if self
                .client
                .commit(remote_project, revision)
                .await?
                .parent
                .is_some()
            {
                return Err(Error::Unavailable(
                    "empty non-root revision invalidates owner authority".into(),
                ));
            }
            return Ok((None, elements));
        }
        let reference = native::marker_from_elements(&elements)?;
        if reference.marker.project != project.as_str()
            || reference.marker.branch != branch.as_str()
        {
            return Err(Error::Unavailable("unexplained external writer".into()));
        }
        let prior = store.operation(
            &self.actor,
            project,
            &OperationId::new(&reference.marker.operation)?,
        )?;
        if prior.actual_revision.as_ref().map(RevisionId::as_str) != Some(revision) {
            return Err(Error::Unavailable(
                "head lacks accepted owner observation".into(),
            ));
        }
        let bytes = store.artifact(&self.actor, project, &reference.envelope)?;
        Ok((
            Some(native::hydrate(&elements, &bytes, &reference.marker)?),
            elements,
        ))
    }

    pub async fn promote(
        &self,
        requester: &ActorId,
        project: &ProjectId,
        operation: &OperationId,
    ) -> Result<StoredOperation> {
        let mut store = self.store()?;
        // Read authorization applies to returned receipt; propose permission was checked at intake.
        let original = store.operation(requester, project, operation)?;
        if original.receipt.actor != *requester && *requester != self.actor {
            return Err(ledgrrr_revision_io::Error::Denied.into());
        }
        let binding = store.project_binding(requester, project)?;
        let retry = Intake {
            project: project.clone(),
            branch: original.receipt.branch.clone(),
            operation: operation.clone(),
            expected_head: original.receipt.expected_head.clone(),
            binding,
        };
        let bytes = store.artifact(requester, project, &original.raw_envelope)?;
        store.intake(&original.receipt.actor, &retry, &bytes)?; // Recheck current propose grant, including revocation.

        if original.dispatch.is_some() || original.receipt.status != SyncStatus::Pending {
            return Ok(original);
        }
        let reserved = store.reserve(
            &self.actor,
            project,
            operation,
            original.generation,
            "native owner reservation before head fetch",
        )?;
        let token = reserved
            .dispatch
            .as_ref()
            .ok_or_else(|| Error::Unavailable("missing reservation".into()))?
            .clone();
        let prepared_result = self.prepare_candidate(&mut store, &reserved).await;
        let (candidate, current, existing, elements) = match prepared_result {
            Ok(Some(v)) => v,
            Ok(None) => {
                return store
                    .operation(requester, project, operation)
                    .map_err(Into::into)
            }
            Err(e) => {
                store.cancel_reserved(
                    &self.actor,
                    &token,
                    reserved.generation,
                    SyncStatus::Unavailable {
                        reason: e.to_string(),
                    },
                )?;
                return Err(e);
            }
        };
        let marker = OperationMarker {
            project: project.as_str().into(),
            branch: reserved.receipt.branch.as_str().into(),
            operation: operation.as_str().into(),
        };
        let projection_marker = if existing {
            native::marker_from_elements(&elements)?.marker
        } else {
            marker.clone()
        };
        let projection = match native::emit(&candidate, &projection_marker) {
            Ok(p) => p,
            Err(e) => {
                store.cancel_reserved(
                    &self.actor,
                    &token,
                    reserved.generation,
                    SyncStatus::Unavailable {
                        reason: e.to_string(),
                    },
                )?;
                return Err(e.into());
            }
        };
        let prepared = store.prepare(
            &self.actor,
            &token,
            reserved.generation,
            &candidate.to_bytes()?,
            projection.projection_digest.clone(),
            projection.identity_digest.clone(),
        )?;
        let binding = store.project_binding(&self.actor, project)?;
        if existing {
            let revision = current
                .ok_or_else(|| Error::Unavailable("no-op without existing revision".into()))?;
            let evidence = evidence(
                &prepared,
                &binding,
                &revision,
                "complete exact native/full-envelope observed no-op",
            )?;
            return Ok(store.record_observed(&self.actor, prepared.generation, &evidence)?);
        }
        let remote_branch = store.remote_branch(&self.actor, project, &prepared.receipt.branch)?;
        // Authority never released after authorization, including response/cancellation uncertainty.
        let sent = store.authorize_send(&self.actor, &token, prepared.generation, true)?;
        let new_ids: std::collections::BTreeSet<_> =
            projection.elements.iter().map(|e| e.id.as_str()).collect();
        let deleted = elements
            .iter()
            .filter(|e| !new_ids.contains(e.id.as_str()))
            .map(|e| e.id.clone())
            .collect::<Vec<_>>();
        let remote = self
            .client
            .create_commit(
                &binding.remote_project,
                &remote_branch,
                current.as_deref(),
                &projection.elements,
                &deleted,
            )
            .await;
        let commit = match remote {
            Ok(c) => c,
            Err(_) => {
                return Ok(store.mark_ambiguous(
                    &self.actor,
                    &token,
                    sent.generation,
                    "native create outcome unknown; reconcile without resend",
                )?)
            }
        };
        if std::env::var_os("OWNER_TEST_CRASH_AFTER_CREATE").is_some() {
            // Trusted subprocess harness only; no request can select this hook.
            std::process::exit(86);
        }
        if std::env::var_os("OWNER_TEST_AMBIGUOUS_AFTER_CREATE").is_some() {
            return Ok(store.mark_ambiguous(
                &self.actor,
                &token,
                sent.generation,
                "trusted harness dropped create response; reconcile without resend",
            )?);
        }
        let proof = self
            .verify_result(
                &binding.remote_project,
                &remote_branch,
                &commit.id,
                &current,
                &candidate,
                &marker,
            )
            .await;
        match proof {
            Ok(())=>Ok(store.record_commit(&self.actor,sent.generation,&evidence(&sent,&binding,&commit.id,"independent exact commit parent, branch reachability and full native/envelope identity")?)?),
            Err(_)=>Ok(store.mark_ambiguous(&self.actor,&token,sent.generation,"response revision failed independent acceptance proof")?),
        }
    }

    async fn prepare_candidate(
        &self,
        store: &mut Store,
        reserved: &StoredOperation,
    ) -> Result<Option<(PortableBundle, Option<String>, bool, Vec<NativeElement>)>> {
        let project = &reserved.receipt.project;
        let branch = &reserved.receipt.branch;
        let binding = store.project_binding(&self.actor, project)?;
        let remote_branch = store.remote_branch(&self.actor, project, branch)?;
        let head = self
            .client
            .head(&binding.remote_project, &remote_branch)
            .await?;
        let ours = PortableBundle::from_bytes(&store.artifact(
            &self.actor,
            project,
            &reserved.canonical_envelope,
        )?)?;
        // A wiped or external head that the owner has never observed is the
        // "no prior state" base for this operation; treat it as if the native
        // branch were empty so the accepted commit can rebase the next revision.
        // Two error shapes cover this case from bound_snapshot:
        //   - Store::NotFound when the head's marker operation is absent
        //   - Unavailable("head lacks accepted owner observation") when the
        //     marker operation is present but its actual_revision doesn't match
        // The expected_head branch below still rejects an unknown *base* revision
        // because the client is asserting a specific parent that we cannot verify.
        let (theirs, elements) = if let Some(h) = &head {
            match self
                .bound_snapshot(store, project, branch, &binding.remote_project, h)
                .await
            {
                Ok(snapshot) => snapshot,
                Err(Error::Store(ledgrrr_revision_io::Error::NotFound)) => (None, vec![]),
                Err(Error::Unavailable(ref msg))
                    if msg == "head lacks accepted owner observation" =>
                {
                    (None, vec![])
                }
                Err(e) => return Err(e),
            }
        } else {
            (None, vec![])
        };
        // Full original envelope equality includes source/library/toolchain/context and bytes.
        if let Some(t) = &theirs {
            if ours.to_bytes()? == t.to_bytes()? {
                return Ok(Some((ours, head, true, elements)));
            }
        }
        let expected = match &reserved.receipt.expected_head {
            ExpectedHead::Empty => None,
            ExpectedHead::Revision(r) => Some(r.as_str()),
        };
        let mut candidate = ours.clone();
        if expected != head.as_deref() {
            let base = if let Some(r) = expected {
                self.bound_snapshot(store, project, branch, &binding.remote_project, r)
                    .await?
                    .0
            } else {
                None
            };
            let empty = PortableModel {
                elements: Default::default(),
                relations: Default::default(),
            };
            let base_model = base.as_ref().map_or(&empty, |b| &b.model);
            let theirs_model = theirs.as_ref().map_or(&empty, |b| &b.model);
            if let (Some(b), Some(t)) = (&base, &theirs) {
                if !same_environment(b, &ours)
                    || !same_environment(b, t)
                    || b.manifest.artifacts != ours.manifest.artifacts
                    || b.manifest.artifacts != t.manifest.artifacts
                {
                    store.cancel_reserved(
                        &self.actor,
                        reserved
                            .dispatch
                            .as_ref()
                            .ok_or_else(|| Error::Unavailable("missing reservation".into()))?,
                        reserved.generation,
                        SyncStatus::Conflict {
                            paths: vec!["/manifest/context-or-artifacts".into()],
                        },
                    )?;
                    return Ok(None);
                }
            }
            match merge_models(base_model, &ours.model, theirs_model)? {
                MergeOutcome::Merged { model, .. } => {
                    candidate.model = model;
                    candidate.manifest.semantic_digest = candidate.model.semantic_digest()?;
                }
                MergeOutcome::Conflicted { conflicts } => {
                    store.cancel_with_conflicts(
                        &self.actor,
                        reserved
                            .dispatch
                            .as_ref()
                            .ok_or_else(|| Error::Unavailable("missing reservation".into()))?,
                        reserved.generation,
                        &conflicts,
                    )?;
                    return Ok(None);
                }
            }
        }
        candidate.manifest.context.parent = head.as_ref().map(RevisionId::new).transpose()?;
        candidate.validate()?;
        Ok(Some((candidate, head, false, elements)))
    }

    async fn verify_result(
        &self,
        remote_project: &str,
        remote_branch: &str,
        revision: &str,
        parent: &Option<String>,
        candidate: &PortableBundle,
        marker: &OperationMarker,
    ) -> Result<()> {
        let commit = self.client.commit(remote_project, revision).await?;
        if &commit.parent != parent {
            return Err(Error::Unavailable(
                "actual promotion parent mismatch".into(),
            ));
        }
        let history = self.client.history(remote_project, remote_branch).await?;
        if !history.iter().any(|c| c.id == revision) {
            return Err(Error::Unavailable(
                "commit unreachable from bound branch".into(),
            ));
        }
        let native = self
            .client
            .snapshot(remote_project, revision, false)
            .await?;
        let hydrated = native::hydrate(&native, &candidate.to_bytes()?, marker)?;
        if hydrated != *candidate {
            return Err(Error::Unavailable("accepted content mismatch".into()));
        }
        if self
            .client
            .head(remote_project, remote_branch)
            .await?
            .as_deref()
            != Some(revision)
        {
            return Err(Error::Unavailable(
                "unexplained head advance invalidates owner authority".into(),
            ));
        }
        Ok(())
    }

    pub async fn reconcile(
        &self,
        requester: &ActorId,
        project: &ProjectId,
        operation: &OperationId,
    ) -> Result<StoredOperation> {
        let mut store = self.store()?;
        let value = store.operation(requester, project, operation)?;
        if value.receipt.actor != *requester && *requester != self.actor {
            return Err(ledgrrr_revision_io::Error::Denied.into());
        }
        if !matches!(
            value.receipt.status,
            SyncStatus::Pending | SyncStatus::Ambiguous { .. }
        ) {
            return Ok(value);
        }
        if !value.send_authorized {
            if let Some(token) = &value.dispatch {
                return Ok(store.cancel_reserved(&self.actor,token,value.generation,SyncStatus::Unavailable{reason:"never authorized preparation cancelled; no remote request was permitted".into()})?);
            }
            return Ok(value);
        }
        let prepared = value.prepared.as_ref().ok_or_else(|| {
            Error::Unavailable("legacy dispatch requires adapter-specific proof".into())
        })?;
        let candidate = PortableBundle::from_bytes(&store.artifact(
            &self.actor,
            project,
            &prepared.envelope,
        )?)?;
        let binding = store.project_binding(&self.actor, project)?;
        let remote_branch = store.remote_branch(&self.actor, project, &value.receipt.branch)?;
        let marker = OperationMarker {
            project: project.as_str().into(),
            branch: value.receipt.branch.as_str().into(),
            operation: operation.as_str().into(),
        };
        let parent = match &prepared.parent {
            ExpectedHead::Empty => None,
            ExpectedHead::Revision(r) => Some(r.as_str().to_owned()),
        };
        let history = self
            .client
            .history(&binding.remote_project, &remote_branch)
            .await?;
        let mut matches = vec![];
        for commit in history {
            if commit.parent != parent {
                continue;
            }
            let elements = self
                .client
                .snapshot(&binding.remote_project, &commit.id, false)
                .await?;
            let Ok(reference) = native::marker_from_elements(&elements) else {
                continue;
            };
            if reference.marker == marker
                && reference.envelope == prepared.envelope
                && native::hydrate(&elements, &candidate.to_bytes()?, &marker).is_ok()
            {
                matches.push(commit.id);
            }
        }
        if matches.len() == 1 {
            return Ok(store.record_commit(&self.actor,value.generation,&evidence(&value,&binding,&matches[0],"restart complete reachable history/exact parent/native/envelope proof; never resent")?)?);
        }
        if value.receipt.status == SyncStatus::Pending {
            return Ok(store.mark_ambiguous(
                &self.actor,
                value
                    .dispatch
                    .as_ref()
                    .ok_or_else(|| Error::Unavailable("missing dispatched reservation".into()))?,
                value.generation,
                "history has zero or multiple exact results; branch remains blocking",
            )?);
        }
        Ok(value)
    }
}
fn same_environment(a: &PortableBundle, b: &PortableBundle) -> bool {
    let a = &a.manifest.context;
    let b = &b.manifest.context;
    a.project == b.project
        && a.model_dialect == b.model_dialect
        && a.library_revisions == b.library_revisions
        && a.source_revisions == b.source_revisions
        && a.toolchain == b.toolchain
}
fn evidence(
    value: &StoredOperation,
    binding: &ProjectBinding,
    revision: &str,
    observation: &str,
) -> Result<CommitEvidence> {
    Ok(CommitEvidence {
        token: value
            .dispatch
            .clone()
            .ok_or_else(|| Error::Unavailable("missing dispatch token".into()))?,
        binding: binding.clone(),
        expected_head: value.receipt.expected_head.clone(),
        proposal_digest: value.receipt.proposal_digest.clone(),
        actual_revision: RevisionId::new(revision)?,
        observation: observation.into(),
    })
}
