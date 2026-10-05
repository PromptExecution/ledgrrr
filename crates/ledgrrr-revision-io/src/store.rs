use crate::*;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::de::DeserializeOwned;
use std::{path::Path, time::Duration};
use ufo_types::revision::canonical_bytes;

const SCHEMA: &str = include_str!("schema.sql");
const APP_ID: i64 = 0x4c525649;
const MAX_ENVELOPE: usize = 32 * 1024 * 1024;

/// On-disk SQLite owner. Use only on a local filesystem. The host owns database
/// file permissions; any process with direct write access is a trusted owner.
pub struct Store {
    connection: Connection,
}

fn decode<T: DeserializeOwned + Serialize>(bytes: &[u8]) -> Result<T> {
    let value: T = serde_json::from_slice(bytes).map_err(|e| Error::Corrupt(e.to_string()))?;
    if canonical_bytes(&value)? != bytes {
        return Err(Error::Corrupt("noncanonical record".into()));
    }
    Ok(value)
}
fn valid_text(value: &str) -> Result<()> {
    if value.is_empty() || value.len() > 4096 || value.chars().any(char::is_control) {
        return Err(Error::Invalid(
            "empty, overlong or control-bearing handle".into(),
        ));
    }
    Ok(())
}
fn validate_binding(binding: &ProjectBinding) -> Result<()> {
    valid_text(&binding.provider)?;
    valid_text(&binding.remote_project)?;
    valid_text(&binding.model_dialect)
}
fn schema_shape(c: &Connection) -> Result<Vec<(String, String)>> {
    let mut q =
        c.prepare("SELECT name,sql FROM sqlite_schema WHERE sql IS NOT NULL ORDER BY name")?;
    let rows = q
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?;
    Ok(rows)
}
fn authorize(c: &Connection, actor: &ActorId, project: &ProjectId, permission: &str) -> Result<()> {
    let g = c
        .query_row(
            "SELECT read,propose,administer FROM grants WHERE project=?1 AND actor=?2",
            params![project.as_str(), actor.as_str()],
            |r| {
                Ok(Grant {
                    read: r.get(0)?,
                    propose: r.get(1)?,
                    administer: r.get(2)?,
                })
            },
        )
        .optional()?;
    let allowed = g.is_some_and(|g| match permission {
        "read" => g.read,
        "propose" => g.propose,
        "administer" => g.administer,
        _ => false,
    });
    if allowed {
        Ok(())
    } else {
        Err(Error::Denied)
    }
}
fn project_binding(c: &Connection, project: &ProjectId) -> Result<ProjectBinding> {
    let raw: Vec<u8> = c
        .query_row(
            "SELECT binding FROM projects WHERE project=?1",
            [project.as_str()],
            |r| r.get(0),
        )
        .optional()?
        .ok_or(Error::NotFound)?;
    let b = decode(&raw)?;
    validate_binding(&b).map_err(|e| Error::Corrupt(e.to_string()))?;
    Ok(b)
}
fn artifact(c: &Connection, project: &ProjectId, digest: &ArtifactDigest) -> Result<Vec<u8>> {
    let bytes:Vec<u8>=c.query_row("SELECT a.bytes FROM artifacts a JOIN project_artifacts p ON p.digest=a.digest WHERE p.project=?1 AND a.digest=?2",params![project.as_str(),digest.as_str()],|r|r.get(0)).optional()?.ok_or(Error::NotFound)?;
    if ArtifactDigest::of(&bytes) != *digest {
        return Err(Error::Corrupt("artifact digest".into()));
    }
    Ok(bytes)
}
fn save_artifact(c: &Connection, project: &ProjectId, bytes: &[u8]) -> Result<ArtifactDigest> {
    let d = ArtifactDigest::of(bytes);
    c.execute(
        "INSERT INTO artifacts(digest,bytes) VALUES (?1,?2) ON CONFLICT(digest) DO NOTHING",
        params![d.as_str(), bytes],
    )?;
    let stored: Vec<u8> = c.query_row(
        "SELECT bytes FROM artifacts WHERE digest=?1",
        [d.as_str()],
        |r| r.get(0),
    )?;
    if stored != bytes {
        return Err(Error::Corrupt("immutable artifact mismatch".into()));
    }
    c.execute(
        "INSERT INTO project_artifacts(project,digest) VALUES (?1,?2) ON CONFLICT DO NOTHING",
        params![project.as_str(), d.as_str()],
    )?;
    Ok(d)
}
fn load_operation(
    c: &Connection,
    project: &ProjectId,
    operation: &OperationId,
) -> Result<StoredOperation> {
    let row=c.query_row("SELECT branch,actor,expected_head,proposal_digest,generation,record FROM operations WHERE project=?1 AND operation=?2",params![project.as_str(),operation.as_str()],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,Vec<u8>>(2)?,r.get::<_,String>(3)?,r.get::<_,u64>(4)?,r.get::<_,Vec<u8>>(5)?))).optional()?.ok_or(Error::NotFound)?;
    let value: StoredOperation = decode(&row.5)?;
    let receipt = &value.receipt;
    if receipt.project != *project
        || receipt.operation != *operation
        || receipt.branch.as_str() != row.0
        || receipt.actor.as_str() != row.1
        || canonical_bytes(&receipt.expected_head)? != row.2
        || receipt.proposal_digest.as_str() != row.3
        || value.generation != row.4
    {
        return Err(Error::Corrupt("operation columns disagree".into()));
    }
    let raw = artifact(c, project, &value.raw_envelope)?;
    let canonical = artifact(c, project, &value.canonical_envelope)?;
    let bundle = PortableBundle::from_bytes(&raw).map_err(|e| Error::Corrupt(e.to_string()))?;
    if bundle.to_bytes()? != canonical
        || bundle.bundle_digest()? != receipt.proposal_digest
        || bundle.manifest.context.project != *project
    {
        return Err(Error::Corrupt("proposal identity".into()));
    }
    for (digest, bytes) in &bundle.blobs {
        if artifact(c, project, digest)? != *bytes {
            return Err(Error::Corrupt("source bytes".into()));
        }
    }
    let binding = project_binding(c, project)?;
    if binding.model_dialect != bundle.manifest.context.model_dialect {
        return Err(Error::Corrupt("model dialect".into()));
    }
    validate_state(&value)?;
    Ok(value)
}
fn validate_state(value: &StoredOperation) -> Result<()> {
    if let Some(t) = &value.dispatch {
        if t.project != value.receipt.project
            || t.branch != value.receipt.branch
            || t.operation != value.receipt.operation
            || t.fence == 0
            || value.dispatch_evidence.is_none()
        {
            return Err(Error::Corrupt("dispatch identity".into()));
        }
    } else if value.dispatch_evidence.is_some() {
        return Err(Error::Corrupt("dispatch evidence without token".into()));
    }
    match &value.receipt.status {
        SyncStatus::Pending | SyncStatus::Unavailable { .. } | SyncStatus::Conflict { .. } => {
            if value.actual_revision.is_some() || value.indexing_work.is_some() {
                return Err(Error::Corrupt("uncommitted revision".into()));
            }
        }
        SyncStatus::Ambiguous { .. } => {
            if value.dispatch.is_none()
                || value.actual_revision.is_some()
                || value.indexing_work.is_some()
            {
                return Err(Error::Corrupt("ambiguous dispatch".into()));
            }
        }
        SyncStatus::ModelCommitted { revision } => {
            if value.actual_revision.as_ref() != Some(revision)
                || value.indexing_work.is_none()
                || value.dispatch.is_none()
            {
                return Err(Error::Corrupt("committed evidence".into()));
            }
        }
        SyncStatus::Indexed { checkpoint } => {
            if checkpoint.project != value.receipt.project
                || value.actual_revision.as_ref() != Some(&checkpoint.revision)
                || value.indexing_work.is_none()
                || value.dispatch.is_none()
            {
                return Err(Error::Corrupt("checkpoint evidence".into()));
            }
        }
    }
    Ok(())
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        if path.as_ref() == Path::new(":memory:") {
            return Err(Error::Invalid("on-disk local database required".into()));
        }
        let mut c = Connection::open(path)?;
        c.busy_timeout(Duration::from_millis(3000))?;
        c.pragma_update(None, "foreign_keys", true)?;
        // Check identity before changing journal settings on a foreign database.
        let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let app: i64 = tx.pragma_query_value(None, "application_id", |r| r.get(0))?;
        let version: i64 = tx.pragma_query_value(None, "user_version", |r| r.get(0))?;
        let shape = schema_shape(&tx)?;
        if shape.is_empty() && app == 0 && version == 0 {
            tx.execute_batch(SCHEMA)?;
            tx.execute(
                "INSERT INTO metadata(singleton,schema_digest) VALUES (1,?1)",
                [ArtifactDigest::of(SCHEMA.as_bytes()).as_str()],
            )?;
            tx.pragma_update(None, "application_id", APP_ID)?;
            tx.pragma_update(None, "user_version", 1)?;
        } else {
            if app != APP_ID || version != 1 {
                return Err(Error::Schema);
            }
            let expected = Connection::open_in_memory()?;
            expected.execute_batch(SCHEMA)?;
            if shape != schema_shape(&expected)? {
                return Err(Error::Schema);
            }
            let digest: String = tx.query_row(
                "SELECT schema_digest FROM metadata WHERE singleton=1",
                [],
                |r| r.get(0),
            )?;
            if digest != ArtifactDigest::of(SCHEMA.as_bytes()).as_str() {
                return Err(Error::Schema);
            }
        }
        tx.commit()?;
        let journal: String =
            c.pragma_update_and_check(None, "journal_mode", "WAL", |r| r.get(0))?;
        c.pragma_update(None, "synchronous", "FULL")?;
        let full: i64 = c.pragma_query_value(None, "synchronous", |r| r.get(0))?;
        let fk: i64 = c.pragma_query_value(None, "foreign_keys", |r| r.get(0))?;
        if journal != "wal" || full != 2 || fk != 1 {
            return Err(Error::Schema);
        }
        let check: String = c.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        if check != "ok" {
            return Err(Error::Corrupt(check));
        }
        let bad_fk: Option<String> = c
            .query_row("PRAGMA foreign_key_check", [], |r| r.get(0))
            .optional()?;
        if bad_fk.is_some() {
            return Err(Error::Corrupt("foreign key integrity".into()));
        }
        Ok(Self { connection: c })
    }

    /// Explicit host-controlled provisioning. Only absent projects can be
    /// created; repeating bootstrap requires the original owner and binding.
    /// Exposing this method as a client endpoint would bypass authentication.
    pub fn bootstrap_project(
        &mut self,
        owner: &ActorId,
        project: &ProjectId,
        binding: &ProjectBinding,
    ) -> Result<()> {
        validate_binding(binding)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing: Option<String> = tx
            .query_row(
                "SELECT bootstrap_owner FROM projects WHERE project=?1",
                [project.as_str()],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(original) = existing {
            authorize(&tx, owner, project, "administer")?;
            if original != owner.as_str() || project_binding(&tx, project)? != *binding {
                return Err(Error::IdentityConflict);
            }
        } else {
            tx.execute(
                "INSERT INTO projects(project,binding,bootstrap_owner) VALUES (?1,?2,?3)",
                params![project.as_str(), canonical_bytes(binding)?, owner.as_str()],
            )?;
            tx.execute(
                "INSERT INTO grants(project,actor,read,propose,administer) VALUES (?1,?2,1,1,1)",
                params![project.as_str(), owner.as_str()],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn register_branch(
        &mut self,
        actor: &ActorId,
        project: &ProjectId,
        branch: &BranchId,
        remote_branch: &str,
    ) -> Result<()> {
        valid_text(remote_branch)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        authorize(&tx, actor, project, "administer")?;
        let existing: Option<String> = tx
            .query_row(
                "SELECT remote_branch FROM branches WHERE project=?1 AND branch=?2",
                params![project.as_str(), branch.as_str()],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(old) = existing {
            if old != remote_branch {
                return Err(Error::IdentityConflict);
            }
        } else {
            tx.execute(
                "INSERT INTO branches(project,branch,remote_branch) VALUES (?1,?2,?3)",
                params![project.as_str(), branch.as_str(), remote_branch],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn set_grant(
        &mut self,
        actor: &ActorId,
        project: &ProjectId,
        target: &ActorId,
        grant: Grant,
    ) -> Result<()> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        authorize(&tx, actor, project, "administer")?;
        let existing: bool = tx
            .query_row(
                "SELECT administer FROM grants WHERE project=?1 AND actor=?2",
                params![project.as_str(), target.as_str()],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(false);
        if existing && !grant.administer {
            let admins: i64 = tx.query_row(
                "SELECT COUNT(*) FROM grants WHERE project=?1 AND administer=1",
                [project.as_str()],
                |r| r.get(0),
            )?;
            if admins <= 1 {
                return Err(Error::Invalid(
                    "last administrator cannot be removed".into(),
                ));
            }
        }
        tx.execute("INSERT INTO grants(project,actor,read,propose,administer) VALUES (?1,?2,?3,?4,?5) ON CONFLICT(project,actor) DO UPDATE SET read=excluded.read,propose=excluded.propose,administer=excluded.administer",params![project.as_str(),target.as_str(),grant.read,grant.propose,grant.administer])?;
        tx.commit()?;
        Ok(())
    }

    pub fn intake(
        &mut self,
        actor: &ActorId,
        intake: &Intake,
        raw: &[u8],
    ) -> Result<StoredOperation> {
        if raw.len() > MAX_ENVELOPE {
            return Err(Error::Invalid("envelope exceeds 32 MiB".into()));
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        authorize(&tx, actor, &intake.project, "propose")?;
        if project_binding(&tx, &intake.project)? != intake.binding {
            return Err(Error::IdentityConflict);
        }
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM branches WHERE project=?1 AND branch=?2)",
            params![intake.project.as_str(), intake.branch.as_str()],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(Error::NotFound);
        }
        let bundle = PortableBundle::from_bytes(raw)?;
        let parent = match &intake.expected_head {
            ExpectedHead::Empty => None,
            ExpectedHead::Revision(r) => Some(r),
        };
        if bundle.manifest.context.project != intake.project
            || bundle.manifest.context.model_dialect != intake.binding.model_dialect
            || bundle.manifest.context.parent.as_ref() != parent
        {
            return Err(Error::Invalid(
                "bundle context does not match intake".into(),
            ));
        }
        let receipt = OperationReceipt {
            operation: intake.operation.clone(),
            actor: actor.clone(),
            project: intake.project.clone(),
            branch: intake.branch.clone(),
            expected_head: intake.expected_head.clone(),
            proposal_digest: bundle.bundle_digest()?,
            status: SyncStatus::Pending,
        };
        match load_operation(&tx, &intake.project, &intake.operation) {
            Ok(existing) => {
                let mut identity = existing.receipt.clone();
                identity.status = SyncStatus::Pending;
                if identity != receipt {
                    return Err(Error::IdentityConflict);
                }
                tx.commit()?;
                return Ok(existing);
            }
            Err(Error::NotFound) => {}
            Err(e) => return Err(e),
        }
        let raw_envelope = save_artifact(&tx, &intake.project, raw)?;
        let canonical_envelope = save_artifact(&tx, &intake.project, &bundle.to_bytes()?)?;
        for bytes in bundle.blobs.values() {
            save_artifact(&tx, &intake.project, bytes)?;
        }
        let value = StoredOperation {
            receipt,
            generation: 0,
            raw_envelope,
            canonical_envelope,
            dispatch: None,
            dispatch_evidence: None,
            actual_revision: None,
            indexing_work: None,
        };
        tx.execute("INSERT INTO operations(project,operation,branch,actor,expected_head,proposal_digest,generation,record) VALUES (?1,?2,?3,?4,?5,?6,0,?7)",params![intake.project.as_str(),intake.operation.as_str(),intake.branch.as_str(),actor.as_str(),canonical_bytes(&intake.expected_head)?,value.receipt.proposal_digest.as_str(),canonical_bytes(&value)?])?;
        tx.commit()?;
        Ok(value)
    }

    pub fn operation(
        &mut self,
        actor: &ActorId,
        project: &ProjectId,
        operation: &OperationId,
    ) -> Result<StoredOperation> {
        let tx = self.connection.transaction()?;
        authorize(&tx, actor, project, "read")?;
        let value = load_operation(&tx, project, operation)?;
        tx.commit()?;
        Ok(value)
    }
    pub fn artifact(
        &mut self,
        actor: &ActorId,
        project: &ProjectId,
        digest: &ArtifactDigest,
    ) -> Result<Vec<u8>> {
        let tx = self.connection.transaction()?;
        authorize(&tx, actor, project, "read")?;
        let bytes = artifact(&tx, project, digest)?;
        tx.commit()?;
        Ok(bytes)
    }
    /// Bounded deterministic page; callers supply the last returned operation ID.
    pub fn recovery(
        &mut self,
        actor: &ActorId,
        project: &ProjectId,
        after: Option<&OperationId>,
        limit: u32,
    ) -> Result<Vec<StoredOperation>> {
        if !(1..=256).contains(&limit) {
            return Err(Error::Invalid("recovery limit must be 1..256".into()));
        }
        let tx = self.connection.transaction()?;
        authorize(&tx, actor, project, "read")?;
        let ids = {
            let mut q=tx.prepare("SELECT operation FROM operations WHERE project=?1 AND operation>?2 ORDER BY operation LIMIT ?3")?;
            let rows = q
                .query_map(
                    params![
                        project.as_str(),
                        after.map_or("", OperationId::as_str),
                        limit
                    ],
                    |r| r.get::<_, String>(0),
                )?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows
        };
        let values = ids
            .into_iter()
            .map(|id| load_operation(&tx, project, &OperationId::new(id)?))
            .collect::<Result<_>>()?;
        tx.commit()?;
        Ok(values)
    }
}
