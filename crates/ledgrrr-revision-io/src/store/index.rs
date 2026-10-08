//! Durable revision projection ownership; provider observations remain trusted adapter input.
use super::*;
use serde::{Deserialize, Serialize};
use ufo_types::revision::RevisionGraphDescriptor;

pub const DEFAULT_PROJECTION: &str = "urn:ledgrrr:revision-projection:1";
const MAX_GRAPH: usize = 64 * 1024 * 1024;
const MAX_ATTEMPTS: u64 = 32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexJobToken {
    pub project: ProjectId,
    pub revision: RevisionId,
    pub projection_schema: String,
    pub worker: String,
    pub fence: u64,
}
#[derive(Debug, Clone)]
pub struct IndexWork {
    pub token: IndexJobToken,
    pub candidate: PortableBundle,
    pub descriptor: Option<RevisionGraphDescriptor>,
}
#[derive(Debug, Clone, Serialize)]
pub struct IndexState {
    pub model_revision: Option<RevisionId>,
    pub available_graph: Option<RevisionGraphDescriptor>,
    pub unavailable_reason: Option<String>,
}

fn now() -> Result<i64> {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| Error::Invalid(e.to_string()))?
        .as_millis();
    i64::try_from(n).map_err(|_| Error::Invalid("clock out of range".into()))
}
fn descriptor(
    c: &Connection,
    p: &ProjectId,
    r: &RevisionId,
    schema: &str,
    published: bool,
) -> Result<Option<RevisionGraphDescriptor>> {
    let raw: Option<Vec<u8>>=c.query_row("SELECT descriptor FROM index_jobs WHERE project=?1 AND revision=?2 AND projection_schema=?3 AND (?4=0 OR state='published')",params![p.as_str(),r.as_str(),schema,published],|r|r.get(0)).optional()?.flatten();
    let result: Option<RevisionGraphDescriptor> = raw.map(|v| decode(&v)).transpose()?;
    if let Some(g) = &result {
        if g.checkpoint.project != *p
            || g.checkpoint.revision != *r
            || g.projection_schema != schema
            || g.checkpoint.dialect != project_binding(c, p)?.model_dialect
            || g.accepted_candidate_digest != candidate(c, p, r)?.bundle_digest()?
        {
            return Err(Error::Corrupt("graph descriptor accepted identity".into()));
        }
    }
    Ok(result)
}
fn candidate(c: &Connection, p: &ProjectId, r: &RevisionId) -> Result<PortableBundle> {
    let (envelope,digest):(String,String)=c.query_row("SELECT envelope,candidate_digest FROM accepted_revisions WHERE project=?1 AND revision=?2",params![p.as_str(),r.as_str()],|r|Ok((r.get(0)?,r.get(1)?))).optional()?.ok_or(Error::NotFound)?;
    let b = PortableBundle::from_bytes(&artifact(c, p, &ArtifactDigest::try_from(envelope)?)?)?;
    if b.bundle_digest()?.as_str() != digest || b.manifest.context.project != *p {
        return Err(Error::Corrupt("accepted manifest identity".into()));
    }
    Ok(b)
}
pub(super) fn enqueue_accepted(c: &Connection, value: &mut StoredOperation) -> Result<()> {
    let Some(prepared) = &value.prepared else {
        return Ok(());
    }; // legacy accepted identity is explicitly unavailable
    let p = &value.receipt.project;
    let r = value.actual_revision.as_ref().ok_or(Error::Transition)?;
    let prior:Option<(String,String)>=c.query_row("SELECT envelope,candidate_digest FROM accepted_revisions WHERE project=?1 AND revision=?2",params![p.as_str(),r.as_str()],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    if let Some((envelope, digest)) = prior {
        if envelope != prepared.envelope.as_str() || digest != prepared.candidate_digest.as_str() {
            return Err(Error::IdentityConflict);
        }
    } else {
        c.execute("INSERT INTO accepted_revisions(project,revision,branch,parent,envelope,candidate_digest) VALUES (?1,?2,?3,?4,?5,?6)",params![p.as_str(),r.as_str(),value.receipt.branch.as_str(),match &prepared.parent {ExpectedHead::Empty=>None,ExpectedHead::Revision(v)=>Some(v.as_str())},prepared.envelope.as_str(),prepared.candidate_digest.as_str()])?;
    }
    let seq: i64 = c.query_row(
        "SELECT sequence FROM accepted_revisions WHERE project=?1 AND revision=?2",
        params![p.as_str(), r.as_str()],
        |r| r.get(0),
    )?;
    c.execute("INSERT INTO branch_index(project,branch,model_revision,model_sequence) VALUES (?1,?2,?3,?4) ON CONFLICT(project,branch) DO UPDATE SET model_revision=excluded.model_revision,model_sequence=excluded.model_sequence,unavailable_reason=NULL WHERE excluded.model_sequence>=branch_index.model_sequence",params![p.as_str(),value.receipt.branch.as_str(),r.as_str(),seq])?;
    c.execute("INSERT INTO index_jobs(project,revision,projection_schema,dialect,state) VALUES (?1,?2,?3,?4,'queued') ON CONFLICT DO NOTHING",params![p.as_str(),r.as_str(),DEFAULT_PROJECTION,project_binding(c,p)?.model_dialect])?;
    if let Some(g) = descriptor(c, p, r, DEFAULT_PROJECTION, true)? {
        verify_graph(c, &g)?;
        value.receipt.status = SyncStatus::Indexed {
            checkpoint: g.checkpoint,
        };
    }
    Ok(())
}
pub(super) fn migrate_accepted(c: &Connection) -> Result<()> {
    let rows = {
        let mut q = c.prepare("SELECT record FROM operations ORDER BY rowid")?;
        let rows = q
            .query_map([], |r| r.get::<_, Vec<u8>>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows
    };
    let mut operations = rows
        .iter()
        .map(|raw| decode::<StoredOperation>(raw))
        .collect::<Result<Vec<_>>>()?;
    operations.sort_by_key(|op| {
        (
            op.receipt.project.clone(),
            op.receipt.branch.clone(),
            op.dispatch.as_ref().map_or(0, |t| t.fence),
        )
    });
    for mut op in operations {
        if op.actual_revision.is_some() {
            enqueue_accepted(c, &mut op)?;
        }
    }
    Ok(())
}
pub(super) fn published_checkpoint(c: &Connection, checkpoint: &IndexCheckpoint) -> Result<bool> {
    let rows = {
        let mut q=c.prepare("SELECT descriptor FROM index_jobs WHERE project=?1 AND revision=?2 AND state='published'")?;
        let rows = q
            .query_map(
                params![checkpoint.project.as_str(), checkpoint.revision.as_str()],
                |r| r.get::<_, Vec<u8>>(0),
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows
    };
    for raw in rows {
        let g: RevisionGraphDescriptor = decode(&raw)?;
        if g.checkpoint == *checkpoint {
            verify_graph(c, &g)?;
            return Ok(true);
        }
    }
    Ok(false)
}
fn check_job(c: &Connection, t: &IndexJobToken) -> Result<String> {
    let row:Option<(String,u64,Option<String>,i64)>=c.query_row("SELECT state,fence,worker,lease_until FROM index_jobs WHERE project=?1 AND revision=?2 AND projection_schema=?3",params![t.project.as_str(),t.revision.as_str(),t.projection_schema],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
    let (state, fence, worker, until) = row.ok_or(Error::NotFound)?;
    if fence != t.fence
        || worker.as_deref() != Some(t.worker.as_str())
        || until < now()?
        || !matches!(state.as_str(), "claimed" | "sealed")
    {
        return Err(Error::Stale);
    }
    Ok(state)
}
impl Store {
    pub fn claim_index(
        &mut self,
        actor: &ActorId,
        p: &ProjectId,
        revision: Option<&RevisionId>,
        schema: &str,
        worker: &str,
        lease_ms: u64,
    ) -> Result<Option<IndexWork>> {
        valid_text(schema)?;
        valid_text(worker)?;
        if lease_ms == 0 || lease_ms > 300_000 {
            return Err(Error::Invalid("index lease bounds".into()));
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        authorize(&tx, actor, p, "administer")?;
        let clock = now()?;
        // A newly selected projection schema is a distinct immutable job.
        tx.execute("INSERT INTO index_jobs(project,revision,projection_schema,dialect,state) SELECT project,revision,?2,?3,'queued' FROM accepted_revisions WHERE project=?1 ON CONFLICT DO NOTHING",params![p.as_str(),schema,project_binding(&tx,p)?.model_dialect])?;
        let row:Option<(String,u64)>=tx.query_row("SELECT j.revision,j.fence FROM index_jobs j JOIN accepted_revisions a ON a.project=j.project AND a.revision=j.revision WHERE j.project=?1 AND j.projection_schema=?2 AND (?3 IS NULL OR j.revision=?3) AND j.state!='published' AND (j.state='queued' OR j.lease_until<?4) AND j.attempts<?5 ORDER BY a.sequence LIMIT 1",params![p.as_str(),schema,revision.map(RevisionId::as_str),clock,MAX_ATTEMPTS],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        let Some((r, fence)) = row else {
            tx.commit()?;
            return Ok(None);
        };
        let r = RevisionId::new(r)?;
        let fence = fence.checked_add(1).ok_or(Error::Stale)?;
        tx.execute("UPDATE index_jobs SET state=CASE WHEN descriptor IS NULL THEN 'claimed' ELSE 'sealed' END,worker=?4,fence=?5,lease_until=?6,attempts=attempts+1 WHERE project=?1 AND revision=?2 AND projection_schema=?3",params![p.as_str(),r.as_str(),schema,worker,fence,clock+lease_ms as i64])?;
        let work = IndexWork {
            token: IndexJobToken {
                project: p.clone(),
                revision: r.clone(),
                projection_schema: schema.into(),
                worker: worker.into(),
                fence,
            },
            candidate: candidate(&tx, p, &r)?,
            descriptor: descriptor(&tx, p, &r, schema, false)?,
        };
        tx.commit()?;
        Ok(Some(work))
    }
    pub fn seal_index(
        &mut self,
        actor: &ActorId,
        t: &IndexJobToken,
        g: &RevisionGraphDescriptor,
        bytes: &[u8],
    ) -> Result<()> {
        if bytes.is_empty() || bytes.len() > MAX_GRAPH {
            return Err(Error::Invalid("graph bounds".into()));
        }
        g.validate()?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        authorize(&tx, actor, &t.project, "administer")?;
        check_job(&tx, t)?;
        if g.checkpoint.project != t.project
            || g.checkpoint.revision != t.revision
            || g.projection_schema != t.projection_schema
            || g.checkpoint.dialect != project_binding(&tx, &t.project)?.model_dialect
            || g.accepted_candidate_digest
                != candidate(&tx, &t.project, &t.revision)?.bundle_digest()?
            || g.artifact_digest != ArtifactDigest::of(bytes)
        {
            return Err(Error::IdentityConflict);
        }
        let hex = |s: &str| {
            s.as_bytes()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        };
        let graph_iri = format!(
            "urn:ledgrrr:revision:1:graph:{}:{}",
            hex(t.project.as_str()),
            hex(t.revision.as_str())
        );
        let graph = oxigraph::model::NamedNode::new(graph_iri.clone())
            .map_err(|e| Error::Invalid(e.to_string()))?;
        let mut required = std::collections::BTreeMap::from([
            (
                "urn:ledgrrr:revision:1:projection_schema",
                g.projection_schema.as_str(),
            ),
            ("urn:ledgrrr:revision:1:project", t.project.as_str()),
            ("urn:ledgrrr:revision:1:revision", t.revision.as_str()),
            (
                "urn:ledgrrr:revision:1:accepted_candidate_digest",
                g.accepted_candidate_digest.as_str(),
            ),
        ]);
        let mut seen = std::collections::BTreeSet::new();
        let mut count = 0u64;
        for quad in
            oxigraph::io::RdfParser::from_format(oxigraph::io::RdfFormat::NQuads).for_slice(bytes)
        {
            let quad = quad.map_err(|e| Error::Invalid(e.to_string()))?;
            if quad.graph_name != graph.clone().into()
                || !matches!(
                    quad.subject,
                    oxigraph::model::NamedOrBlankNode::NamedNode(_)
                )
                || matches!(quad.object, oxigraph::model::Term::BlankNode(_))
            {
                return Err(Error::IdentityConflict);
            }
            if !seen.insert(quad.to_string()) {
                return Err(Error::Invalid("duplicate graph quad".into()));
            }
            if quad.subject == graph.clone().into() {
                if let Some(expected) = required.get(quad.predicate.as_str()) {
                    if quad.object != oxigraph::model::Literal::new_simple_literal(*expected).into()
                    {
                        return Err(Error::IdentityConflict);
                    }
                    required.remove(quad.predicate.as_str());
                }
            }
            count = count
                .checked_add(1)
                .ok_or(Error::Invalid("quad bounds".into()))?;
            if count > 1_000_000 {
                return Err(Error::Invalid("quad bounds".into()));
            }
        }
        if !required.is_empty() {
            return Err(Error::IdentityConflict);
        }
        if count != g.quad_count {
            return Err(Error::IdentityConflict);
        }
        if let Some(prior) = descriptor(&tx, &t.project, &t.revision, &t.projection_schema, false)?
        {
            if prior != *g {
                return Err(Error::IdentityConflict);
            }
        }
        tx.execute(
            "INSERT INTO graph_artifacts VALUES (?1,?2) ON CONFLICT(digest) DO NOTHING",
            params![g.artifact_digest.as_str(), bytes],
        )?;
        let stored: Vec<u8> = tx.query_row(
            "SELECT bytes FROM graph_artifacts WHERE digest=?1",
            [g.artifact_digest.as_str()],
            |r| r.get(0),
        )?;
        if stored != bytes {
            return Err(Error::Corrupt("immutable graph mismatch".into()));
        }
        tx.execute("UPDATE index_jobs SET state='sealed',descriptor=?4 WHERE project=?1 AND revision=?2 AND projection_schema=?3",params![t.project.as_str(),t.revision.as_str(),t.projection_schema,canonical_bytes(g)?])?;
        tx.commit()?;
        Ok(())
    }
    pub fn publish_index(
        &mut self,
        actor: &ActorId,
        t: &IndexJobToken,
    ) -> Result<RevisionGraphDescriptor> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        authorize(&tx, actor, &t.project, "administer")?;
        if check_job(&tx, t)? != "sealed" {
            return Err(Error::Transition);
        }
        let g = descriptor(&tx, &t.project, &t.revision, &t.projection_schema, false)?
            .ok_or(Error::Transition)?;
        verify_graph(&tx, &g)?;
        tx.execute("UPDATE index_jobs SET state='published' WHERE project=?1 AND revision=?2 AND projection_schema=?3",params![t.project.as_str(),t.revision.as_str(),t.projection_schema])?;
        let ops = {
            let mut q = tx.prepare("SELECT operation FROM operations WHERE project=?1")?;
            let rows = q
                .query_map([t.project.as_str()], |r| r.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows
        };
        for id in ops {
            let mut op = load_operation(&tx, &t.project, &OperationId::new(id)?)?;
            if op.actual_revision.as_ref() == Some(&t.revision)
                && matches!(op.receipt.status, SyncStatus::ModelCommitted { .. })
            {
                if op.prepared.as_ref().map(|p| &p.candidate_digest)
                    != Some(&g.accepted_candidate_digest)
                {
                    return Err(Error::IdentityConflict);
                }
                op.receipt.status = SyncStatus::Indexed {
                    checkpoint: g.checkpoint.clone(),
                };
                update_operation(&tx, &mut op)?;
            }
        }
        let seq: i64 = tx.query_row(
            "SELECT sequence FROM accepted_revisions WHERE project=?1 AND revision=?2",
            params![t.project.as_str(), t.revision.as_str()],
            |r| r.get(0),
        )?;
        let branches = {
            let mut q=tx.prepare("SELECT branch,model_revision FROM branch_index WHERE project=?1 AND model_revision IS NOT NULL AND unavailable_reason IS NULL")?;
            let rows = q
                .query_map([t.project.as_str()], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows
        };
        for (branch, head) in branches {
            if is_ancestor(&tx, &t.project, &t.revision, &RevisionId::new(head)?)? {
                tx.execute("INSERT INTO branch_checkpoints VALUES (?1,?2,?3,?4,?5) ON CONFLICT(project,branch,projection_schema) DO UPDATE SET revision=excluded.revision,sequence=excluded.sequence WHERE excluded.sequence>branch_checkpoints.sequence",params![t.project.as_str(),branch,t.projection_schema,t.revision.as_str(),seq])?;
            }
        }
        #[cfg(test)]
        publication_test_pause()?;
        tx.commit()?;
        Ok(g)
    }
    pub fn graph(
        &mut self,
        actor: &ActorId,
        p: &ProjectId,
        r: &RevisionId,
        schema: &str,
    ) -> Result<Option<(RevisionGraphDescriptor, Vec<u8>)>> {
        let tx = self.connection.transaction()?;
        authorize(&tx, actor, p, "read")?;
        let result = descriptor(&tx, p, r, schema, true)?
            .map(|g| verify_graph(&tx, &g).map(|bytes| (g, bytes)))
            .transpose()?;
        tx.commit()?;
        Ok(result)
    }
    pub fn accepted_bundle(
        &mut self,
        actor: &ActorId,
        p: &ProjectId,
        r: &RevisionId,
    ) -> Result<PortableBundle> {
        let tx = self.connection.transaction()?;
        authorize(&tx, actor, p, "read")?;
        let b = candidate(&tx, p, r)?;
        tx.commit()?;
        Ok(b)
    }
    pub fn graph_descriptor(
        &mut self,
        actor: &ActorId,
        p: &ProjectId,
        r: &RevisionId,
        schema: &str,
    ) -> Result<Option<RevisionGraphDescriptor>> {
        let tx = self.connection.transaction()?;
        authorize(&tx, actor, p, "read")?;
        let g = descriptor(&tx, p, r, schema, false)?;
        tx.commit()?;
        Ok(g)
    }
    pub fn index_state(
        &mut self,
        actor: &ActorId,
        p: &ProjectId,
        b: &BranchId,
        schema: &str,
    ) -> Result<IndexState> {
        self.index_state_inner(actor, p, b, schema, true)
    }
    /// Read publication identities without requiring the branch-current artifact.
    /// Exact historical queries must validate their selected artifact separately.
    pub fn index_state_identity(
        &mut self,
        actor: &ActorId,
        p: &ProjectId,
        b: &BranchId,
        schema: &str,
    ) -> Result<IndexState> {
        self.index_state_inner(actor, p, b, schema, false)
    }
    fn index_state_inner(
        &mut self,
        actor: &ActorId,
        p: &ProjectId,
        b: &BranchId,
        schema: &str,
        verify: bool,
    ) -> Result<IndexState> {
        let tx = self.connection.transaction()?;
        authorize(&tx, actor, p, "read")?;
        let row:Option<(Option<String>,Option<String>)>=tx.query_row("SELECT model_revision,unavailable_reason FROM branch_index WHERE project=?1 AND branch=?2",params![p.as_str(),b.as_str()],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        let (model, reason) = row.unwrap_or((None, None));
        let revision:Option<String>=tx.query_row("SELECT revision FROM branch_checkpoints WHERE project=?1 AND branch=?2 AND projection_schema=?3",params![p.as_str(),b.as_str(),schema],|r|r.get(0)).optional()?;
        let available = revision
            .map(RevisionId::new)
            .transpose()?
            .map(|r| descriptor(&tx, p, &r, schema, false))
            .transpose()?
            .flatten();
        if let Some(g) = available.as_ref().filter(|_| verify) {
            verify_graph(&tx, g)?;
        }
        let state = IndexState {
            model_revision: model.map(RevisionId::new).transpose()?,
            available_graph: available,
            unavailable_reason: reason,
        };
        tx.commit()?;
        Ok(state)
    }
    pub fn observe_head(
        &mut self,
        actor: &ActorId,
        p: &ProjectId,
        b: &BranchId,
        r: Option<&RevisionId>,
        reason: Option<&str>,
    ) -> Result<()> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        authorize(&tx, actor, p, "administer")?;
        let seq: Option<i64> = r
            .map(|r| {
                tx.query_row(
                    "SELECT sequence FROM accepted_revisions WHERE project=?1 AND revision=?2",
                    params![p.as_str(), r.as_str()],
                    |r| r.get(0),
                )
                .optional()
            })
            .transpose()?
            .flatten();
        tx.execute("INSERT INTO branch_index VALUES (?1,?2,?3,?4,?5) ON CONFLICT(project,branch) DO UPDATE SET model_revision=excluded.model_revision,model_sequence=excluded.model_sequence,unavailable_reason=excluded.unavailable_reason",params![p.as_str(),b.as_str(),r.map(RevisionId::as_str),seq.unwrap_or(0),reason])?;
        tx.commit()?;
        Ok(())
    }
    pub fn is_ancestor(
        &mut self,
        actor: &ActorId,
        p: &ProjectId,
        ancestor: &RevisionId,
        descendant: &RevisionId,
    ) -> Result<bool> {
        let tx = self.connection.transaction()?;
        authorize(&tx, actor, p, "read")?;
        let result = is_ancestor(&tx, p, ancestor, descendant)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn remove_projection(
        &mut self,
        actor: &ActorId,
        p: &ProjectId,
        r: &RevisionId,
        schema: &str,
    ) -> Result<()> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        authorize(&tx, actor, p, "administer")?;
        let g = descriptor(&tx, p, r, schema, false)?.ok_or(Error::NotFound)?;
        tx.execute(
            "DELETE FROM graph_artifacts WHERE digest=?1",
            [g.artifact_digest.as_str()],
        )?;
        tx.execute("UPDATE index_jobs SET state='queued',worker=NULL,lease_until=0,attempts=0 WHERE project=?1 AND revision=?2 AND projection_schema=?3",params![p.as_str(),r.as_str(),schema])?;
        tx.commit()?;
        Ok(())
    }
}
fn verify_graph(c: &Connection, g: &RevisionGraphDescriptor) -> Result<Vec<u8>> {
    g.validate()?;
    let bytes: Vec<u8> = c
        .query_row(
            "SELECT bytes FROM graph_artifacts WHERE digest=?1",
            [g.artifact_digest.as_str()],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| Error::Corrupt("published graph artifact missing".into()))?;
    if ArtifactDigest::of(&bytes) != g.artifact_digest {
        return Err(Error::Corrupt("published graph artifact digest".into()));
    }
    Ok(bytes)
}
fn is_ancestor(c: &Connection, p: &ProjectId, a: &RevisionId, d: &RevisionId) -> Result<bool> {
    let mut next = Some(d.as_str().to_owned());
    let mut seen = std::collections::BTreeSet::new();
    while let Some(r) = next {
        if r == a.as_str() {
            return Ok(true);
        }
        if seen.len() >= 10000 || !seen.insert(r.clone()) {
            return Err(Error::Corrupt("accepted ancestry bound/cycle".into()));
        }
        next = c
            .query_row(
                "SELECT parent FROM accepted_revisions WHERE project=?1 AND revision=?2",
                params![p.as_str(), r],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
    }
    Ok(false)
}
#[cfg(test)]
fn publication_test_pause() -> Result<()> {
    if let Some(path) = std::env::var_os("REVISION_PUBLICATION_TEST_READY") {
        std::fs::write(path, b"publication transaction uncommitted")
            .map_err(|e| Error::Invalid(e.to_string()))?;
        loop {
            std::thread::park();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(
        path: &Path,
    ) -> (
        Store,
        StoredOperation,
        IndexWork,
        RevisionGraphDescriptor,
        Vec<u8>,
    ) {
        let f: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/durable_revision_cases.json"
        ))
        .unwrap();
        let bundle = PortableBundle::dehydrate(
            serde_json::from_value(f["model"].clone()).unwrap(),
            serde_json::from_value(f["context"].clone()).unwrap(),
            serde_json::from_value(f["artifacts"].clone()).unwrap(),
        )
        .unwrap();
        let actor = ActorId::new("owner").unwrap();
        let project = bundle.manifest.context.project.clone();
        let branch = BranchId::new("main").unwrap();
        let binding: ProjectBinding = serde_json::from_value(f["binding"].clone()).unwrap();
        let mut s = Store::open(path).unwrap();
        s.bootstrap_project(&actor, &project, &binding).unwrap();
        s.register_branch(&actor, &project, &branch, "native-main")
            .unwrap();
        let intake = Intake {
            project: project.clone(),
            branch,
            operation: OperationId::new("actual-accepted").unwrap(),
            expected_head: ExpectedHead::Empty,
            binding: binding.clone(),
        };
        let op = s
            .intake(&actor, &intake, &bundle.to_bytes().unwrap())
            .unwrap();
        let op = s
            .reserve(
                &actor,
                &project,
                &intake.operation,
                op.generation,
                "reserve",
            )
            .unwrap();
        let token = op.dispatch.clone().unwrap();
        let op = s
            .prepare(
                &actor,
                &token,
                op.generation,
                &bundle.to_bytes().unwrap(),
                ArtifactDigest::of(b"native"),
                ArtifactDigest::of(b"identity"),
            )
            .unwrap();
        let op = s
            .authorize_send(&actor, &token, op.generation, true)
            .unwrap();
        let revision = RevisionId::new("actual-provider-r").unwrap();
        let op = s
            .record_commit(
                &actor,
                op.generation,
                &CommitEvidence {
                    token,
                    binding,
                    expected_head: ExpectedHead::Empty,
                    proposal_digest: op.receipt.proposal_digest.clone(),
                    actual_revision: revision.clone(),
                    observation: "verified acceptance".into(),
                },
            )
            .unwrap();
        let work = s
            .claim_index(
                &actor,
                &project,
                Some(&revision),
                DEFAULT_PROJECTION,
                "subprocess-test",
                30000,
            )
            .unwrap()
            .unwrap();
        let hex = |s: &str| s.bytes().map(|v| format!("{v:02x}")).collect::<String>();
        let iri = format!(
            "urn:ledgrrr:revision:1:graph:{}:{}",
            hex(project.as_str()),
            hex(revision.as_str())
        );
        let digest = bundle.bundle_digest().unwrap();
        let bytes = [
            ("projection_schema", DEFAULT_PROJECTION),
            ("project", project.as_str()),
            ("revision", revision.as_str()),
            ("accepted_candidate_digest", digest.as_str()),
        ]
        .iter()
        .map(|(k, v)| {
            format!(
                "<{iri}> <urn:ledgrrr:revision:1:{k}> {} <{iri}> .\n",
                serde_json::to_string(v).unwrap()
            )
        })
        .collect::<String>()
        .into_bytes();
        let g = RevisionGraphDescriptor {
            checkpoint: IndexCheckpoint {
                project,
                revision,
                dialect: bundle.manifest.context.model_dialect.clone(),
                graph_digest: ArtifactDigest::of(&bytes),
            },
            projection_schema: DEFAULT_PROJECTION.into(),
            accepted_candidate_digest: digest,
            artifact_digest: ArtifactDigest::of(&bytes),
            quad_count: 4,
        };
        (s, op, work, g, bytes)
    }
    #[test]
    fn publication_child() {
        if let Some(path) = std::env::var_os("INDEX_TEST_DB") {
            let token: IndexJobToken = serde_json::from_slice(
                &std::fs::read(std::env::var_os("INDEX_TEST_TOKEN").unwrap()).unwrap(),
            )
            .unwrap();
            Store::open(path)
                .unwrap()
                .publish_index(&ActorId::new("owner").unwrap(), &token)
                .unwrap();
        }
    }
    #[test]
    fn sqlite_progress_deadline_interrupts_actual_long_evaluation() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("owner.db");
        drop(Store::open(&path).unwrap());
        let store = Store::open_bounded(&path, Duration::from_millis(25)).unwrap();
        let started = std::time::Instant::now();
        let result=store.connection.query_row::<i64,_,_>("WITH RECURSIVE numbers(x) AS (VALUES(0) UNION ALL SELECT x+1 FROM numbers WHERE x<100000000) SELECT sum(x) FROM numbers",[],|r|r.get(0));
        assert!(
            matches!(result,Err(rusqlite::Error::SqliteFailure(error,_)) if error.code==rusqlite::ErrorCode::OperationInterrupted)
        );
        assert!(started.elapsed() < Duration::from_millis(300));
    }
    fn kill_at_ready(child: &mut std::process::Child, ready: &Path) {
        let start = std::time::Instant::now();
        while !ready.exists() && start.elapsed() < Duration::from_secs(10) {
            assert!(child.try_wait().unwrap().is_none());
            std::thread::sleep(Duration::from_millis(10));
        }
        if !ready.exists() {
            let _ = child.kill();
            let _ = child.wait();
            panic!("subprocess transaction handshake timed out");
        }
        child.kill().unwrap();
        child.wait().unwrap();
    }
    #[test]
    fn killed_publication_transaction_exposes_no_partial_checkpoint() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("owner.db");
        let ready = d.path().join("ready");
        let tokenfile = d.path().join("token.json");
        let (mut s, op, work, g, bytes) = fixture(&path);
        let actor = ActorId::new("owner").unwrap();
        s.seal_index(&actor, &work.token, &g, &bytes).unwrap();
        std::fs::write(&tokenfile, canonical_bytes(&work.token).unwrap()).unwrap();
        drop(s);
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "store::index::tests::publication_child",
                "--nocapture",
            ])
            .env("INDEX_TEST_DB", &path)
            .env("INDEX_TEST_TOKEN", &tokenfile)
            .env("REVISION_PUBLICATION_TEST_READY", &ready)
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        kill_at_ready(&mut child, &ready);
        let mut s = Store::open(&path).unwrap();
        assert!(s
            .graph(
                &actor,
                &g.checkpoint.project,
                &g.checkpoint.revision,
                DEFAULT_PROJECTION
            )
            .unwrap()
            .is_none());
        assert_eq!(
            s.operation(&actor, &op.receipt.project, &op.receipt.operation)
                .unwrap(),
            op
        );
        assert_eq!(s.publish_index(&actor, &work.token).unwrap(), g);
        assert_eq!(
            s.graph(
                &actor,
                &g.checkpoint.project,
                &g.checkpoint.revision,
                DEFAULT_PROJECTION
            )
            .unwrap()
            .unwrap()
            .1,
            bytes
        );
        drop(s);
        Store::open(path).unwrap();
    }
    #[test]
    fn killed_actual_v2_migration_rolls_back_and_replays_candidate() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("owner.db");
        let ready = d.path().join("ready");
        let (s, op, work, _, _) = fixture(&path);
        drop(s);
        let c = Connection::open(&path).unwrap();
        c.execute_batch("DROP TABLE branch_checkpoints; DROP TABLE branch_index; DROP TABLE graph_artifacts; DROP TABLE index_jobs; DROP TABLE accepted_revisions; PRAGMA user_version=2;").unwrap();
        c.execute(
            "UPDATE metadata SET schema_digest=?1",
            [ArtifactDigest::of(SCHEMA_V2.as_bytes()).as_str()],
        )
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
        kill_at_ready(&mut child, &ready);
        let c = Connection::open(&path).unwrap();
        assert_eq!(
            c.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
                .unwrap(),
            2
        );
        let expected = Connection::open_in_memory().unwrap();
        expected.execute_batch(SCHEMA_V2).unwrap();
        assert_eq!(schema_shape(&c).unwrap(), schema_shape(&expected).unwrap());
        drop(c);
        let mut s = Store::open(&path).unwrap();
        let actor = ActorId::new("owner").unwrap();
        assert_eq!(
            s.operation(&actor, &op.receipt.project, &op.receipt.operation)
                .unwrap(),
            op
        );
        let replay = s
            .claim_index(
                &actor,
                &op.receipt.project,
                op.actual_revision.as_ref(),
                DEFAULT_PROJECTION,
                "replay",
                10000,
            )
            .unwrap()
            .unwrap();
        assert_eq!(replay.candidate, work.candidate);
    }
}
