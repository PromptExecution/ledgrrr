use crate::*;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::de::DeserializeOwned;
use std::{path::Path, time::Duration};
use ufo_types::revision::canonical_bytes;

const SCHEMA: &str = include_str!("schema.sql");
const SCHEMA_V1: &str = include_str!("schema-v1.sql");
const SCHEMA_V2: &str = include_str!("schema-v2.sql");
const APP_ID: i64 = 0x4c525649;
const MAX_ENVELOPE: usize = 32 * 1024 * 1024;

/// On-disk SQLite owner. Use only on a local filesystem. The host owns database
/// file permissions; any process with direct write access is a trusted owner.
pub struct Store {
    connection: Connection,
}
mod index;
pub use index::{IndexJobToken, IndexState, IndexWork};

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
    if !binding
        .provider
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_' | b'.'))
    {
        return Err(Error::Invalid(
            "provider must be a canonical configured lowercase handle".into(),
        ));
    }
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
    let raw =
        artifact(c, project, &value.raw_envelope).map_err(|e| Error::Corrupt(e.to_string()))?;
    let canonical = artifact(c, project, &value.canonical_envelope)
        .map_err(|e| Error::Corrupt(e.to_string()))?;
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
    let expected_parent = match &receipt.expected_head {
        ExpectedHead::Empty => None,
        ExpectedHead::Revision(r) => Some(r),
    };
    if bundle.manifest.context.parent.as_ref() != expected_parent {
        return Err(Error::Corrupt(
            "expected head differs from proposal parent".into(),
        ));
    }
    if let Some(p) = &value.prepared {
        let candidate = PortableBundle::from_bytes(&artifact(c, project, &p.envelope)?)?;
        let parent = candidate
            .manifest
            .context
            .parent
            .clone()
            .map_or(ExpectedHead::Empty, ExpectedHead::Revision);
        if candidate.bundle_digest() != Ok(p.candidate_digest.clone())
            || parent != p.parent
            || candidate.manifest.context.project != *project
        {
            return Err(Error::Corrupt("prepared identity".into()));
        }
        for (digest, bytes) in &candidate.blobs {
            if artifact(c, project, digest)? != *bytes {
                return Err(Error::Corrupt("prepared source bytes".into()));
            }
        }
    }
    if let Some(digest) = &value.conflict_evidence {
        let conflicts: Vec<ufo_types::revision::Conflict> = decode(&artifact(c, project, digest)?)?;
        if !matches!(&receipt.status,SyncStatus::Conflict{paths} if *paths==conflicts.iter().map(|c|c.path.clone()).collect::<Vec<_>>())
        {
            return Err(Error::Corrupt("conflict evidence identity".into()));
        }
    }
    validate_state(&value)?;
    if let Some(token) = &value.dispatch {
        let (fence, active): (u64, Option<String>) = c.query_row(
            "SELECT fence,active_operation FROM branches WHERE project=?1 AND branch=?2",
            params![project.as_str(), receipt.branch.as_str()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if fence < token.fence
            || (matches!(
                receipt.status,
                SyncStatus::Pending | SyncStatus::Ambiguous { .. }
            ) && (fence != token.fence || active.as_deref() != Some(operation.as_str())))
        {
            return Err(Error::Corrupt("dispatch reservation identity".into()));
        }
    }
    if let Some(e) = &value.commit_evidence {
        if e.binding != binding
            || Some(&e.token) != value.dispatch.as_ref()
            || e.expected_head != receipt.expected_head
            || e.proposal_digest != receipt.proposal_digest
            || Some(&e.actual_revision) != value.actual_revision.as_ref()
        {
            return Err(Error::Corrupt("commit evidence identity".into()));
        }
        let expected_work = ArtifactDigest::of(&canonical_bytes(&(
            "ledgrrr:index-work:1",
            &receipt.project,
            &receipt.operation,
            &e.actual_revision,
            value
                .prepared
                .as_ref()
                .map_or(&receipt.proposal_digest, |p| &p.candidate_digest),
        ))?);
        if value.indexing_work.as_ref() != Some(&expected_work) {
            return Err(Error::Corrupt("indexing work identity".into()));
        }
    } else if value.actual_revision.is_some() {
        return Err(Error::Corrupt("missing commit evidence".into()));
    }
    if let SyncStatus::Indexed { checkpoint } = &receipt.status {
        if checkpoint.dialect != binding.model_dialect {
            return Err(Error::Corrupt("checkpoint dialect".into()));
        }
    }
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
            if value.actual_revision.is_some()
                || value.indexing_work.is_some()
                || value.commit_evidence.is_some()
            {
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
    pub fn project_binding(
        &mut self,
        actor: &ActorId,
        project: &ProjectId,
    ) -> Result<ProjectBinding> {
        let tx = self.connection.transaction()?;
        authorize(&tx, actor, project, "read")?;
        let binding = project_binding(&tx, project)?;
        tx.commit()?;
        Ok(binding)
    }

    pub fn remote_branch(
        &mut self,
        actor: &ActorId,
        project: &ProjectId,
        branch: &BranchId,
    ) -> Result<String> {
        let tx = self.connection.transaction()?;
        authorize(&tx, actor, project, "read")?;
        let handle: String = tx
            .query_row(
                "SELECT remote_branch FROM branches WHERE project=?1 AND branch=?2",
                params![project.as_str(), branch.as_str()],
                |r| r.get(0),
            )
            .optional()?
            .ok_or(Error::NotFound)?;
        valid_text(&handle).map_err(|e| Error::Corrupt(e.to_string()))?;
        tx.commit()?;
        Ok(handle)
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_inner(path, None)
    }
    /// Read requests retain this deadline through subsequent SQLite work.
    pub fn open_bounded(path: impl AsRef<Path>, timeout: Duration) -> Result<Self> {
        if timeout.is_zero() {
            return Err(Error::Invalid("expired store deadline".into()));
        }
        Self::open_inner(path, Some(timeout))
    }
    /// Refresh the remaining request budget after provider I/O.
    pub fn bound_work_for(&mut self, timeout: Duration) -> Result<()> {
        if timeout.is_zero() {
            return Err(Error::Invalid("expired store deadline".into()));
        }
        self.connection
            .busy_timeout(timeout.min(Duration::from_millis(3000)))?;
        let deadline = std::time::Instant::now() + timeout;
        self.connection
            .progress_handler(1000, Some(move || std::time::Instant::now() >= deadline));
        Ok(())
    }
    fn open_inner(path: impl AsRef<Path>, timeout: Option<Duration>) -> Result<Self> {
        if path.as_ref() == Path::new(":memory:") {
            return Err(Error::Invalid("on-disk local database required".into()));
        }
        let mut c = Connection::open(path)?;
        c.busy_timeout(
            timeout
                .unwrap_or(Duration::from_millis(3000))
                .min(Duration::from_millis(3000)),
        )?;
        if let Some(timeout) = timeout {
            let deadline = std::time::Instant::now() + timeout;
            c.progress_handler(1000, Some(move || std::time::Instant::now() >= deadline));
        }
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
            tx.pragma_update(None, "user_version", 3)?;
        } else {
            if app != APP_ID || !matches!(version, 1..=3) {
                return Err(Error::Schema);
            }
            let expected = Connection::open_in_memory()?;
            expected.execute_batch(match version {
                1 => SCHEMA_V1,
                2 => SCHEMA_V2,
                _ => SCHEMA,
            })?;
            if shape != schema_shape(&expected)? {
                return Err(Error::Schema);
            }
            let digest: String = tx.query_row(
                "SELECT schema_digest FROM metadata WHERE singleton=1",
                [],
                |r| r.get(0),
            )?;
            if digest
                != ArtifactDigest::of(match version {
                    1 => SCHEMA_V1.as_bytes(),
                    2 => SCHEMA_V2.as_bytes(),
                    _ => SCHEMA.as_bytes(),
                })
                .as_str()
            {
                return Err(Error::Schema);
            }
            if version == 1 {
                // Ordered atomic migration: reject collisions before changing any data.
                let bindings = {
                    let mut q =
                        tx.prepare("SELECT project,binding FROM projects ORDER BY project")?;
                    let rows = q
                        .query_map([], |r| {
                            Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
                        })?
                        .collect::<std::result::Result<Vec<_>, _>>()?;
                    rows
                };
                let mut seen = std::collections::BTreeSet::new();
                for (_, raw) in &bindings {
                    let b: ProjectBinding = decode(raw)?;
                    validate_binding(&b)?;
                    if !seen.insert((b.provider, b.remote_project)) {
                        return Err(Error::IdentityConflict);
                    }
                }
                tx.execute_batch(SCHEMA_V2.strip_prefix(SCHEMA_V1).ok_or(Error::Schema)?)?;
                for (project, raw) in bindings {
                    let b: ProjectBinding = decode(&raw)?;
                    tx.execute(
                        "INSERT INTO physical_projects VALUES (?1,?2,?3)",
                        params![b.provider, b.remote_project, project],
                    )?;
                }
                let records = {
                    let mut q = tx.prepare("SELECT project,operation,record FROM operations")?;
                    let rows = q
                        .query_map([], |r| {
                            Ok((
                                r.get::<_, String>(0)?,
                                r.get::<_, String>(1)?,
                                r.get::<_, Vec<u8>>(2)?,
                            ))
                        })?
                        .collect::<std::result::Result<Vec<_>, _>>()?;
                    rows
                };
                for (project, operation, raw) in records {
                    let mut value: StoredOperation = decode(&raw)?;
                    // v1 dispatch always meant send permission: unresolved legacy work stays fenced.
                    if value.dispatch.is_some() {
                        value.send_authorized = true;
                    }
                    tx.execute(
                        "UPDATE operations SET record=?1 WHERE project=?2 AND operation=?3",
                        params![canonical_bytes(&value)?, project, operation],
                    )?;
                }
                // Unit-test binary only: kill the actual migration after all row writes,
                // before schema metadata/commit. No production environment hook exists.
                #[cfg(test)]
                migration_test_pause()?;
                tx.execute(
                    "UPDATE metadata SET schema_digest=?1",
                    [ArtifactDigest::of(SCHEMA_V2.as_bytes()).as_str()],
                )?;
                tx.pragma_update(None, "user_version", 2)?;
            }
            if version <= 2 {
                tx.execute_batch(SCHEMA.strip_prefix(SCHEMA_V2).ok_or(Error::Schema)?)?;
                index::migrate_accepted(&tx)?;
                #[cfg(test)]
                migration_test_pause()?;
                tx.execute(
                    "UPDATE metadata SET schema_digest=?1",
                    [ArtifactDigest::of(SCHEMA.as_bytes()).as_str()],
                )?;
                tx.pragma_update(None, "user_version", 3)?;
            }
        }
        // Validate physical mappings even on reopen; raw DB writers are trusted but corruption fails closed.
        {
            let mut q = tx.prepare("SELECT project FROM projects ORDER BY project")?;
            let ids = q
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            for id in ids {
                let b = project_binding(&tx, &ProjectId::new(&id)?)?;
                let physical:Option<String>=tx.query_row("SELECT project FROM physical_projects WHERE provider=?1 AND remote_project=?2",params![b.provider,b.remote_project],|r|r.get(0)).optional()?;
                if physical.as_deref() != Some(id.as_str()) {
                    return Err(Error::Corrupt("physical project binding".into()));
                }
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
            let occupied:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM physical_projects WHERE provider=?1 AND remote_project=?2)",params![binding.provider,binding.remote_project],|r|r.get(0))?;
            if occupied {
                return Err(Error::IdentityConflict);
            }
            tx.execute(
                "INSERT INTO projects(project,binding,bootstrap_owner) VALUES (?1,?2,?3)",
                params![project.as_str(), canonical_bytes(binding)?, owner.as_str()],
            )?;
            tx.execute(
                "INSERT INTO physical_projects VALUES (?1,?2,?3)",
                params![binding.provider, binding.remote_project, project.as_str()],
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
            commit_evidence: None,
            prepared: None,
            send_authorized: false,
            observed_existing: false,
            conflict_evidence: None,
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

fn update_operation(c: &Connection, value: &mut StoredOperation) -> Result<()> {
    validate_state(value)?;
    let prior = value.generation;
    value.generation = value.generation.checked_add(1).ok_or(Error::Stale)?;
    let changed=c.execute("UPDATE operations SET generation=?1,record=?2 WHERE project=?3 AND operation=?4 AND generation=?5",params![value.generation,canonical_bytes(value)?,value.receipt.project.as_str(),value.receipt.operation.as_str(),prior])?;
    if changed != 1 {
        return Err(Error::Stale);
    }
    Ok(())
}
fn check_token(c: &Connection, value: &StoredOperation, token: &DispatchToken) -> Result<()> {
    if value.dispatch.as_ref() != Some(token) {
        return Err(Error::Stale);
    }
    let (fence, active): (u64, Option<String>) = c.query_row(
        "SELECT fence,active_operation FROM branches WHERE project=?1 AND branch=?2",
        params![token.project.as_str(), token.branch.as_str()],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if fence != token.fence || active.as_deref() != Some(token.operation.as_str()) {
        return Err(Error::Stale);
    }
    Ok(())
}
impl Store {
    /// Call before a provider request. A second invocation never authorizes
    /// sending that request again, even for an identical operation or worker.
    pub fn start_dispatch(
        &mut self,
        actor: &ActorId,
        project: &ProjectId,
        operation: &OperationId,
        generation: u64,
        observation: &str,
    ) -> Result<StoredOperation> {
        let value = self.reserve(actor, project, operation, generation, observation)?;
        self.authorize_send(
            actor,
            value.dispatch.as_ref().ok_or(Error::Transition)?,
            value.generation,
            false,
        )
    }

    pub fn reserve(
        &mut self,
        actor: &ActorId,
        project: &ProjectId,
        operation: &OperationId,
        generation: u64,
        observation: &str,
    ) -> Result<StoredOperation> {
        valid_text(observation)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        authorize(&tx, actor, project, "administer")?;
        let mut value = load_operation(&tx, project, operation)?;
        let (fence, active): (u64, Option<String>) = tx.query_row(
            "SELECT fence,active_operation FROM branches WHERE project=?1 AND branch=?2",
            params![project.as_str(), value.receipt.branch.as_str()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if let Some(active) = active {
            load_operation(&tx, project, &OperationId::new(active)?)
                .map_err(|e| Error::Corrupt(e.to_string()))?;
            return Err(Error::BranchBlocked);
        }
        let unresolved:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM operations WHERE project=?1 AND branch=?2 AND json_extract(record,'$.dispatch') IS NOT NULL AND json_extract(record,'$.receipt.status.kind') IN ('pending','ambiguous'))",params![project.as_str(),value.receipt.branch.as_str()],|r|r.get(0))?;
        if unresolved {
            return Err(Error::Corrupt("missing branch reservation".into()));
        }
        if value.dispatch.is_some() {
            return Err(Error::BranchBlocked);
        }
        if value.generation != generation {
            return Err(Error::Stale);
        }
        if value.receipt.status != SyncStatus::Pending {
            return Err(Error::Transition);
        }
        let fence = fence.checked_add(1).ok_or(Error::Stale)?;
        value.dispatch = Some(DispatchToken {
            project: project.clone(),
            branch: value.receipt.branch.clone(),
            operation: operation.clone(),
            fence,
        });
        value.dispatch_evidence = Some(observation.to_owned());
        tx.execute(
            "UPDATE branches SET fence=?1,active_operation=?2 WHERE project=?3 AND branch=?4",
            params![
                fence,
                operation.as_str(),
                project.as_str(),
                value.receipt.branch.as_str()
            ],
        )?;
        update_operation(&tx, &mut value)?;
        tx.commit()?;
        Ok(value)
    }
    /// Persist validated candidate and blobs while retaining the original intake.
    pub fn prepare(
        &mut self,
        actor: &ActorId,
        token: &DispatchToken,
        generation: u64,
        bytes: &[u8],
        projection_digest: ArtifactDigest,
        identity_digest: ArtifactDigest,
    ) -> Result<StoredOperation> {
        let candidate = PortableBundle::from_bytes(bytes)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        authorize(&tx, actor, &token.project, "administer")?;
        let mut value = load_operation(&tx, &token.project, &token.operation)?;
        check_token(&tx, &value, token)?;
        if value.generation != generation {
            return Err(Error::Stale);
        }
        if value.send_authorized
            || value.prepared.is_some()
            || value.receipt.status != SyncStatus::Pending
        {
            return Err(Error::Transition);
        }
        if candidate.manifest.context.project != token.project
            || candidate.manifest.context.model_dialect
                != project_binding(&tx, &token.project)?.model_dialect
        {
            return Err(Error::IdentityConflict);
        }
        let parent = candidate
            .manifest
            .context
            .parent
            .clone()
            .map_or(ExpectedHead::Empty, ExpectedHead::Revision);
        let envelope = save_artifact(&tx, &token.project, &candidate.to_bytes()?)?;
        for bytes in candidate.blobs.values() {
            save_artifact(&tx, &token.project, bytes)?;
        }
        value.prepared = Some(PreparedCandidate {
            envelope,
            candidate_digest: candidate.bundle_digest()?,
            parent,
            projection_digest,
            identity_digest,
        });
        update_operation(&tx, &mut value)?;
        tx.commit()?;
        Ok(value)
    }

    /// Exactly one durable send authorization. Repeated calls never grant another send.
    pub fn authorize_send(
        &mut self,
        actor: &ActorId,
        token: &DispatchToken,
        generation: u64,
        require_prepared: bool,
    ) -> Result<StoredOperation> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        authorize(&tx, actor, &token.project, "administer")?;
        let mut value = load_operation(&tx, &token.project, &token.operation)?;
        check_token(&tx, &value, token)?;
        if value.generation != generation {
            return Err(Error::Stale);
        }
        if value.send_authorized
            || value.receipt.status != SyncStatus::Pending
            || (require_prepared && value.prepared.is_none())
        {
            return Err(Error::Transition);
        }
        value.send_authorized = true;
        update_operation(&tx, &mut value)?;
        tx.commit()?;
        Ok(value)
    }

    /// A reserved but never authorized operation can safely release its branch.
    pub fn cancel_reserved(
        &mut self,
        actor: &ActorId,
        token: &DispatchToken,
        generation: u64,
        status: SyncStatus,
    ) -> Result<StoredOperation> {
        self.cancel_reserved_inner(actor, token, generation, status, None)
    }
    pub fn cancel_with_conflicts(
        &mut self,
        actor: &ActorId,
        token: &DispatchToken,
        generation: u64,
        conflicts: &[ufo_types::revision::Conflict],
    ) -> Result<StoredOperation> {
        if conflicts.is_empty() || conflicts.len() > 256 {
            return Err(Error::Invalid("bounded nonempty conflicts required".into()));
        }
        let status = SyncStatus::Conflict {
            paths: conflicts.iter().map(|c| c.path.clone()).collect(),
        };
        self.cancel_reserved_inner(actor, token, generation, status, Some(conflicts))
    }
    fn cancel_reserved_inner(
        &mut self,
        actor: &ActorId,
        token: &DispatchToken,
        generation: u64,
        status: SyncStatus,
        conflicts: Option<&[ufo_types::revision::Conflict]>,
    ) -> Result<StoredOperation> {
        if !matches!(
            status,
            SyncStatus::Conflict { .. } | SyncStatus::Unavailable { .. }
        ) {
            return Err(Error::Transition);
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        authorize(&tx, actor, &token.project, "administer")?;
        let mut value = load_operation(&tx, &token.project, &token.operation)?;
        check_token(&tx, &value, token)?;
        if value.generation != generation {
            return Err(Error::Stale);
        }
        if value.send_authorized || value.receipt.status != SyncStatus::Pending {
            return Err(Error::Transition);
        }
        if let Some(conflicts) = conflicts {
            value.conflict_evidence = Some(save_artifact(
                &tx,
                &token.project,
                &canonical_bytes(&conflicts)?,
            )?);
        }
        value.receipt.status = status;
        update_operation(&tx, &mut value)?;
        tx.execute("UPDATE branches SET active_operation=NULL WHERE project=?1 AND branch=?2 AND fence=?3 AND active_operation=?4",params![token.project.as_str(),token.branch.as_str(),token.fence,token.operation.as_str()])?;
        tx.commit()?;
        Ok(value)
    }

    /// Bind a fully verified existing revision, without authorizing a remote request.
    pub fn record_observed(
        &mut self,
        actor: &ActorId,
        generation: u64,
        evidence: &CommitEvidence,
    ) -> Result<StoredOperation> {
        self.record_commit_inner(actor, generation, evidence, true)
    }

    pub fn mark_ambiguous(
        &mut self,
        actor: &ActorId,
        token: &DispatchToken,
        generation: u64,
        reason: &str,
    ) -> Result<StoredOperation> {
        valid_text(reason)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        authorize(&tx, actor, &token.project, "administer")?;
        let mut value = load_operation(&tx, &token.project, &token.operation)?;
        check_token(&tx, &value, token)?;
        let target = SyncStatus::Ambiguous {
            reason: reason.into(),
        };
        if value.receipt.status == target && generation.checked_add(1) == Some(value.generation) {
            tx.commit()?;
            return Ok(value);
        }
        if value.generation != generation {
            return Err(Error::Stale);
        }
        if value.receipt.status != SyncStatus::Pending {
            return Err(Error::Transition);
        }
        value.receipt.status = target;
        update_operation(&tx, &mut value)?;
        tx.commit()?;
        Ok(value)
    }
    /// Accept evidence only from the trusted authenticated provider worker. An
    /// ambiguous request is resolved by this exact observed identity, never by
    /// assuming a lease expiry means a remote create did not happen.
    pub fn record_commit(
        &mut self,
        actor: &ActorId,
        generation: u64,
        evidence: &CommitEvidence,
    ) -> Result<StoredOperation> {
        self.record_commit_inner(actor, generation, evidence, false)
    }
    fn record_commit_inner(
        &mut self,
        actor: &ActorId,
        generation: u64,
        evidence: &CommitEvidence,
        observed: bool,
    ) -> Result<StoredOperation> {
        valid_text(&evidence.observation)?;
        let token = &evidence.token;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        authorize(&tx, actor, &token.project, "administer")?;
        let mut value = load_operation(&tx, &token.project, &token.operation)?;
        if value.commit_evidence.as_ref() == Some(evidence)
            && generation.checked_add(1) == Some(value.generation)
        {
            tx.commit()?;
            return Ok(value);
        }
        if value.generation != generation {
            return Err(Error::Stale);
        }
        check_token(&tx, &value, token)?;
        if !matches!(
            value.receipt.status,
            SyncStatus::Pending | SyncStatus::Ambiguous { .. }
        ) {
            return Err(Error::Transition);
        }
        if evidence.binding != project_binding(&tx, &token.project)?
            || evidence.expected_head != value.receipt.expected_head
            || evidence.proposal_digest != value.receipt.proposal_digest
        {
            return Err(Error::IdentityConflict);
        }
        if observed {
            if value.send_authorized || value.prepared.is_none() {
                return Err(Error::Transition);
            }
            value.observed_existing = true;
        }
        if !value.send_authorized && !value.observed_existing {
            return Err(Error::Transition);
        }
        value.actual_revision = Some(evidence.actual_revision.clone());
        value.indexing_work = Some(ArtifactDigest::of(&canonical_bytes(&(
            "ledgrrr:index-work:1",
            &value.receipt.project,
            &value.receipt.operation,
            &evidence.actual_revision,
            value
                .prepared
                .as_ref()
                .map_or(&value.receipt.proposal_digest, |p| &p.candidate_digest),
        ))?));
        value.commit_evidence = Some(evidence.clone());
        value.receipt.status = SyncStatus::ModelCommitted {
            revision: evidence.actual_revision.clone(),
        };
        index::enqueue_accepted(&tx, &mut value)?;
        update_operation(&tx, &mut value)?;
        tx.execute("UPDATE branches SET active_operation=NULL WHERE project=?1 AND branch=?2 AND fence=?3 AND active_operation=?4",params![token.project.as_str(),token.branch.as_str(),token.fence,token.operation.as_str()])?;
        tx.commit()?;
        Ok(value)
    }
    /// Safe only before dispatch. After sending a request its outcome must be
    /// reconciled, not relabelled unavailable/conflicting to release a branch.
    pub fn reject_before_dispatch(
        &mut self,
        actor: &ActorId,
        project: &ProjectId,
        operation: &OperationId,
        generation: u64,
        status: SyncStatus,
    ) -> Result<StoredOperation> {
        match &status {
            SyncStatus::Conflict { paths } if !paths.is_empty() && paths.len() <= 256 => {
                for p in paths {
                    valid_text(p)?;
                }
            }
            SyncStatus::Unavailable { reason } => valid_text(reason)?,
            _ => return Err(Error::Transition),
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        authorize(&tx, actor, project, "administer")?;
        let mut value = load_operation(&tx, project, operation)?;
        if value.receipt.status == status && generation.checked_add(1) == Some(value.generation) {
            tx.commit()?;
            return Ok(value);
        }
        if value.generation != generation {
            return Err(Error::Stale);
        }
        if value.dispatch.is_some() || value.receipt.status != SyncStatus::Pending {
            return Err(Error::Transition);
        }
        value.receipt.status = status;
        update_operation(&tx, &mut value)?;
        tx.commit()?;
        Ok(value)
    }
    /// Only the host's authenticated administrator may obtain this capability.
    /// Keep it inside the trusted graph publisher; do not expose it as a request
    /// receipt-update API. Identity checks cannot prove graph completeness.
    pub fn publisher<'a>(
        &'a mut self,
        actor: &ActorId,
        project: &ProjectId,
    ) -> Result<IndexPublisher<'a>> {
        authorize(&self.connection, actor, project, "administer")?;
        Ok(IndexPublisher {
            store: self,
            actor: actor.clone(),
            project: project.clone(),
        })
    }
}

pub struct IndexPublisher<'a> {
    store: &'a mut Store,
    actor: ActorId,
    project: ProjectId,
}
impl IndexPublisher<'_> {
    /// Records trusted publication evidence for this immutable operation only.
    /// It does not advance any branch pointer or publish/query an external graph.
    pub fn record_index(
        &mut self,
        operation: &OperationId,
        generation: u64,
        checkpoint: &IndexCheckpoint,
    ) -> Result<StoredOperation> {
        let tx = self
            .store
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        authorize(&tx, &self.actor, &self.project, "administer")?;
        let mut value = load_operation(&tx, &self.project, operation)?;
        if checkpoint.project != self.project
            || value.actual_revision.as_ref() != Some(&checkpoint.revision)
            || project_binding(&tx, &self.project)?.model_dialect != checkpoint.dialect
        {
            return Err(Error::Transition);
        }
        let target = SyncStatus::Indexed {
            checkpoint: checkpoint.clone(),
        };
        if value.receipt.status == target && generation.checked_add(1) == Some(value.generation) {
            tx.commit()?;
            return Ok(value);
        }
        if value.generation != generation {
            return Err(Error::Stale);
        }
        if !matches!(value.receipt.status, SyncStatus::ModelCommitted { .. }) {
            return Err(Error::Transition);
        }
        // Legacy compatibility API may only attach already-published sealed evidence.
        if !index::published_checkpoint(&tx, checkpoint)? {
            return Err(Error::Transition);
        }
        value.receipt.status = target;
        update_operation(&tx, &mut value)?;
        tx.commit()?;
        Ok(value)
    }
}

#[cfg(test)]
fn migration_test_pause() -> Result<()> {
    if let Some(ready) = std::env::var_os("REVISION_MIGRATION_TEST_READY") {
        std::fs::write(ready, b"migration rows written, transaction uncommitted")
            .map_err(|e| Error::Invalid(e.to_string()))?;
        loop {
            std::thread::park();
        }
    }
    Ok(())
}

#[cfg(test)]
mod migration_tests {
    use super::*;
    #[test]
    fn migration_process_child() {
        if let Some(path) = std::env::var_os("REVISION_MIGRATION_TEST_DATABASE") {
            Store::open(path).unwrap();
        }
    }
    #[test]
    fn killed_actual_v1_upgrade_rolls_back_and_reopens_without_loss() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("migration.db");
        let ready = directory.path().join("ready");
        let f: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/durable_revision_cases.json"
        ))
        .unwrap();
        let bundle = PortableBundle::dehydrate(
            serde_json::from_value(f["model"].clone()).unwrap(),
            serde_json::from_value(f["context"].clone()).unwrap(),
            serde_json::from_value(f["artifacts"].clone()).unwrap(),
        )
        .unwrap();
        let owner = ActorId::new("owner").unwrap();
        let project = bundle.manifest.context.project.clone();
        let branch = BranchId::new("main").unwrap();
        let operation = OperationId::new("migration-crash").unwrap();
        let binding: ProjectBinding = serde_json::from_value(f["binding"].clone()).unwrap();
        let mut store = Store::open(&path).unwrap();
        store.bootstrap_project(&owner, &project, &binding).unwrap();
        store
            .register_branch(&owner, &project, &branch, "remote-main")
            .unwrap();
        let intake = Intake {
            project: project.clone(),
            branch,
            operation: operation.clone(),
            expected_head: ExpectedHead::Empty,
            binding,
        };
        let original = store
            .intake(&owner, &intake, &bundle.to_bytes().unwrap())
            .unwrap();
        let sent = store
            .start_dispatch(
                &owner,
                &project,
                &operation,
                original.generation,
                "legacy request authorized",
            )
            .unwrap();
        let ambiguous = store
            .mark_ambiguous(
                &owner,
                sent.dispatch.as_ref().unwrap(),
                sent.generation,
                "legacy remote result unknown",
            )
            .unwrap();
        drop(store);
        let c = Connection::open(&path).unwrap();
        c.execute_batch("DROP TABLE branch_checkpoints; DROP TABLE branch_index; DROP TABLE graph_artifacts; DROP TABLE index_jobs; DROP TABLE accepted_revisions; DROP TABLE physical_projects; PRAGMA user_version=1;")
            .unwrap();
        c.execute(
            "UPDATE metadata SET schema_digest=?1",
            [ArtifactDigest::of(SCHEMA_V1.as_bytes()).as_str()],
        )
        .unwrap();
        let mut legacy = ambiguous.clone();
        legacy.send_authorized = false;
        let legacy_bytes = canonical_bytes(&legacy).unwrap();
        c.execute("UPDATE operations SET record=?1", [&legacy_bytes])
            .unwrap();
        let artifact_count: i64 = c
            .query_row("SELECT COUNT(*) FROM artifacts", [], |r| r.get(0))
            .unwrap();
        drop(c);
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "store::migration_tests::migration_process_child",
                "--nocapture",
            ])
            .env("REVISION_MIGRATION_TEST_DATABASE", &path)
            .env("REVISION_MIGRATION_TEST_READY", &ready)
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let started = std::time::Instant::now();
        while !ready.exists() && started.elapsed() < Duration::from_secs(10) {
            assert!(child.try_wait().unwrap().is_none());
            std::thread::sleep(Duration::from_millis(10));
        }
        if !ready.exists() {
            let _ = child.kill();
            let _ = child.wait();
            panic!("actual migration handshake timed out");
        }
        child.kill().unwrap();
        child.wait().unwrap();
        let c = Connection::open(&path).unwrap();
        assert_eq!(
            c.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
                .unwrap(),
            1
        );
        assert_eq!(schema_shape(&c).unwrap(), {
            let known = Connection::open_in_memory().unwrap();
            known.execute_batch(SCHEMA_V1).unwrap();
            schema_shape(&known).unwrap()
        });
        assert_eq!(
            c.query_row::<Vec<u8>, _, _>("SELECT record FROM operations", [], |r| r.get(0))
                .unwrap(),
            legacy_bytes
        );
        assert_eq!(
            c.query_row::<i64, _, _>("SELECT COUNT(*) FROM artifacts", [], |r| r.get(0))
                .unwrap(),
            artifact_count
        );
        drop(c);
        let mut upgraded = Store::open(&path).unwrap();
        assert_eq!(
            upgraded.operation(&owner, &project, &operation).unwrap(),
            ambiguous
        );
        assert_eq!(
            upgraded
                .artifact(&owner, &project, &original.raw_envelope)
                .unwrap(),
            bundle.to_bytes().unwrap()
        );
        assert!(matches!(
            upgraded.start_dispatch(
                &owner,
                &project,
                &operation,
                ambiguous.generation,
                "unsafe retry"
            ),
            Err(Error::BranchBlocked)
        ));
        drop(upgraded);
        Store::open(path).unwrap();
    }
}
