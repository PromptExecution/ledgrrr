use ledgrrr_revision_io::*;
use std::collections::BTreeMap;
use ufo_types::revision::{ArtifactPath, PortableModel, RevisionContext, RevisionGraphDescriptor};

fn a() -> ActorId {
    ActorId::new("owner").unwrap()
}
fn p() -> ProjectId {
    ProjectId::new("project-1").unwrap()
}
fn b() -> BranchId {
    BranchId::new("main").unwrap()
}
fn candidate(parent: Option<&str>) -> PortableBundle {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/durable_revision_cases.json")).unwrap();
    let model: PortableModel = serde_json::from_value(f["model"].clone()).unwrap();
    let mut context: RevisionContext = serde_json::from_value(f["context"].clone()).unwrap();
    context.parent = parent.map(|v| RevisionId::new(v).unwrap());
    let artifacts: BTreeMap<ArtifactPath, Vec<u8>> =
        serde_json::from_value(f["artifacts"].clone()).unwrap();
    PortableBundle::dehydrate(model, context, artifacts).unwrap()
}
fn setup(path: &std::path::Path) -> Store {
    let mut s = Store::open(path).unwrap();
    let binding: ProjectBinding = serde_json::from_value(
        serde_json::from_str::<serde_json::Value>(include_str!(
            "fixtures/durable_revision_cases.json"
        ))
        .unwrap()["binding"]
            .clone(),
    )
    .unwrap();
    s.bootstrap_project(&a(), &p(), &binding).unwrap();
    s.register_branch(&a(), &p(), &b(), "remote-main").unwrap();
    s
}
fn accept(s: &mut Store, op: &str, revision: &str, parent: Option<&str>) -> StoredOperation {
    let bundle = candidate(parent);
    let i = Intake {
        project: p(),
        branch: b(),
        operation: OperationId::new(op).unwrap(),
        expected_head: parent
            .map(|v| ExpectedHead::Revision(RevisionId::new(v).unwrap()))
            .unwrap_or(ExpectedHead::Empty),
        binding: s.project_binding(&a(), &p()).unwrap(),
    };
    let initial = s.intake(&a(), &i, &bundle.to_bytes().unwrap()).unwrap();
    let reserved = s
        .reserve(&a(), &p(), &i.operation, initial.generation, "reserve")
        .unwrap();
    let token = reserved.dispatch.as_ref().unwrap();
    let prepared = s
        .prepare(
            &a(),
            token,
            reserved.generation,
            &bundle.to_bytes().unwrap(),
            ArtifactDigest::of(b"native"),
            ArtifactDigest::of(b"identities"),
        )
        .unwrap();
    let sent = s
        .authorize_send(&a(), token, prepared.generation, true)
        .unwrap();
    s.record_commit(
        &a(),
        sent.generation,
        &CommitEvidence {
            token: token.clone(),
            binding: i.binding,
            expected_head: i.expected_head,
            proposal_digest: initial.receipt.proposal_digest,
            actual_revision: RevisionId::new(revision).unwrap(),
            observation: "actual adapter acceptance".into(),
        },
    )
    .unwrap()
}
fn graph(revision: &str, digest: ArtifactDigest) -> (RevisionGraphDescriptor, Vec<u8>) {
    let hex = |s: &str| s.bytes().map(|v| format!("{v:02x}")).collect::<String>();
    let iri = format!(
        "urn:ledgrrr:revision:1:graph:{}:{}",
        hex(p().as_str()),
        hex(revision)
    );
    let bytes = [
        ("projection_schema", "test-projection-1"),
        ("project", p().as_str()),
        ("revision", revision),
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
    (
        RevisionGraphDescriptor {
            checkpoint: IndexCheckpoint {
                project: p(),
                revision: RevisionId::new(revision).unwrap(),
                dialect: candidate(None).manifest.context.model_dialect,
                graph_digest: ArtifactDigest::of(&bytes),
            },
            projection_schema: "test-projection-1".into(),
            accepted_candidate_digest: digest,
            artifact_digest: ArtifactDigest::of(&bytes),
            quad_count: 4,
        },
        bytes,
    )
}

#[test]
fn checkpoint_requires_sealed_fenced_graph_and_retains_accepted_input() {
    let d = tempfile::tempdir().unwrap();
    let mut s = setup(&d.path().join("owner.db"));
    let op = accept(&mut s, "op-a", "actual-a", None);
    let (descriptor, bytes) = graph(
        "actual-a",
        op.prepared.as_ref().unwrap().candidate_digest.clone(),
    );
    assert!(s
        .publisher(&a(), &p())
        .unwrap()
        .record_index(&op.receipt.operation, op.generation, &descriptor.checkpoint)
        .is_err());
    let job = s
        .claim_index(
            &a(),
            &p(),
            Some(&descriptor.checkpoint.revision),
            "test-projection-1",
            "worker",
            10000,
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        job.candidate.bundle_digest().unwrap(),
        descriptor.accepted_candidate_digest
    );
    s.seal_index(&a(), &job.token, &descriptor, &bytes).unwrap();
    s.publish_index(&a(), &job.token).unwrap();
    assert_eq!(
        s.graph(
            &a(),
            &p(),
            &descriptor.checkpoint.revision,
            "test-projection-1"
        )
        .unwrap()
        .unwrap(),
        (descriptor.clone(), bytes)
    );
    assert_eq!(
        s.operation(&a(), &p(), &op.receipt.operation)
            .unwrap()
            .receipt
            .status,
        SyncStatus::Indexed {
            checkpoint: descriptor.checkpoint
        }
    );
}

#[test]
fn out_of_order_jobs_do_not_rewind_and_rebuild_uses_retained_candidate() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("owner.db");
    let mut s = setup(&path);
    let old = accept(&mut s, "old", "r-old", None);
    let new = accept(&mut s, "new", "r-new", Some("r-old"));
    let old_job = s
        .claim_index(
            &a(),
            &p(),
            Some(&RevisionId::new("r-old").unwrap()),
            "test-projection-1",
            "old-worker",
            10000,
        )
        .unwrap()
        .unwrap();
    let new_job = s
        .claim_index(
            &a(),
            &p(),
            Some(&RevisionId::new("r-new").unwrap()),
            "test-projection-1",
            "new-worker",
            10000,
        )
        .unwrap()
        .unwrap();
    for (op, job, r) in [(&new, &new_job, "r-new"), (&old, &old_job, "r-old")] {
        let (g, bytes) = graph(r, op.prepared.as_ref().unwrap().candidate_digest.clone());
        s.seal_index(&a(), &job.token, &g, &bytes).unwrap();
        s.publish_index(&a(), &job.token).unwrap();
    }
    let state = s
        .index_state(&a(), &p(), &b(), "test-projection-1")
        .unwrap();
    assert_eq!(
        state.model_revision,
        Some(RevisionId::new("r-new").unwrap())
    );
    assert_eq!(
        state.available_graph.as_ref().unwrap().checkpoint.revision,
        RevisionId::new("r-new").unwrap()
    );
    assert!(s
        .is_ancestor(
            &a(),
            &p(),
            &RevisionId::new("r-old").unwrap(),
            &RevisionId::new("r-new").unwrap()
        )
        .unwrap());
    let before = state.available_graph.unwrap();
    s.remove_projection(&a(), &p(), &before.checkpoint.revision, "test-projection-1")
        .unwrap();
    assert!(s
        .graph(&a(), &p(), &before.checkpoint.revision, "test-projection-1")
        .unwrap()
        .is_none());
    drop(s);
    let mut s = Store::open(path).unwrap();
    let job = s
        .claim_index(
            &a(),
            &p(),
            Some(&before.checkpoint.revision),
            "test-projection-1",
            "rebuild",
            10000,
        )
        .unwrap()
        .unwrap();
    let (after, bytes) = graph("r-new", job.candidate.bundle_digest().unwrap());
    assert_eq!(after, before);
    s.seal_index(&a(), &job.token, &after, &bytes).unwrap();
    s.publish_index(&a(), &job.token).unwrap();
}

#[test]
fn expired_fence_denied_grant_and_corrupt_graph_cannot_publish_or_read() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("owner.db");
    let mut s = setup(&path);
    let op = accept(&mut s, "lease", "lease-r", None);
    let r = RevisionId::new("lease-r").unwrap();
    let first = s
        .claim_index(&a(), &p(), Some(&r), "test-projection-1", "first", 1)
        .unwrap()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    let next = s
        .claim_index(
            &a(),
            &p(),
            Some(&r),
            "test-projection-1",
            "replacement",
            10000,
        )
        .unwrap()
        .unwrap();
    assert!(next.token.fence > first.token.fence);
    let (g, bytes) = graph(
        "lease-r",
        op.prepared.as_ref().unwrap().candidate_digest.clone(),
    );
    assert!(matches!(
        s.seal_index(&a(), &first.token, &g, &bytes),
        Err(Error::Stale)
    ));
    let denied = ActorId::new("denied").unwrap();
    assert!(matches!(
        s.seal_index(&denied, &next.token, &g, &bytes),
        Err(Error::Denied)
    ));
    let mut forged = g.clone();
    forged.accepted_candidate_digest = ArtifactDigest::of(b"stale intake");
    assert!(matches!(
        s.seal_index(&a(), &next.token, &forged, &bytes),
        Err(Error::IdentityConflict)
    ));
    let invalid = b"this is not NQuads";
    let mut bad = g.clone();
    bad.artifact_digest = ArtifactDigest::of(invalid);
    bad.checkpoint.graph_digest = bad.artifact_digest.clone();
    assert!(s.seal_index(&a(), &next.token, &bad, invalid).is_err());
    s.seal_index(&a(), &next.token, &g, &bytes).unwrap();
    s.publish_index(&a(), &next.token).unwrap();
    assert!(matches!(
        s.graph(&denied, &p(), &r, "test-projection-1"),
        Err(Error::Denied)
    ));
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute(
        "UPDATE graph_artifacts SET bytes=?1",
        [b"corrupt".as_slice()],
    )
    .unwrap();
    drop(db);
    assert!(matches!(
        s.graph(&a(), &p(), &r, "test-projection-1"),
        Err(Error::Corrupt(_))
    ));
}

#[test]
fn noop_receipts_share_durable_job_and_published_evidence() {
    let d = tempfile::tempdir().unwrap();
    let mut s = setup(&d.path().join("owner.db"));
    let first = accept(&mut s, "first", "same-r", None);
    let r = RevisionId::new("same-r").unwrap();
    let job = s
        .claim_index(
            &a(),
            &p(),
            Some(&r),
            "urn:ledgrrr:revision-projection:1",
            "worker",
            10000,
        )
        .unwrap()
        .unwrap();
    let (mut g, bytes) = graph(
        "same-r",
        first.prepared.as_ref().unwrap().candidate_digest.clone(),
    );
    g.projection_schema = job.token.projection_schema.clone();
    let bytes = String::from_utf8(bytes)
        .unwrap()
        .replace("test-projection-1", &g.projection_schema)
        .into_bytes();
    g.artifact_digest = ArtifactDigest::of(&bytes);
    g.checkpoint.graph_digest = g.artifact_digest.clone();
    s.seal_index(&a(), &job.token, &g, &bytes).unwrap();
    s.publish_index(&a(), &job.token).unwrap();
    let next = accept(&mut s, "observed-existing", "same-r", None);
    assert_eq!(
        next.receipt.status,
        SyncStatus::Indexed {
            checkpoint: g.checkpoint
        }
    );
    assert!(s
        .claim_index(
            &a(),
            &p(),
            Some(&r),
            "urn:ledgrrr:revision-projection:1",
            "other",
            10000
        )
        .unwrap()
        .is_none());
    let db = rusqlite::Connection::open(d.path().join("owner.db")).unwrap();
    assert_eq!(
        db.query_row::<i64, _, _>("SELECT count(*) FROM accepted_revisions", [], |r| r.get(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row::<i64, _, _>("SELECT count(*) FROM index_jobs", [], |r| r.get(0))
            .unwrap(),
        1
    );
}

#[test]
fn bounded_store_open_does_not_outlive_locked_database_deadline() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("owner.db");
    drop(setup(&path));
    let blocker = rusqlite::Connection::open(&path).unwrap();
    blocker.execute_batch("BEGIN IMMEDIATE").unwrap();
    let started = std::time::Instant::now();
    assert!(Store::open_bounded(&path, std::time::Duration::from_millis(25)).is_err());
    assert!(started.elapsed() < std::time::Duration::from_millis(300));
    blocker.execute_batch("ROLLBACK").unwrap();
    Store::open(path).unwrap();
}

#[test]
fn invalid_lease_requests_do_not_consume_durable_work() {
    let d = tempfile::tempdir().unwrap();
    let mut store = setup(&d.path().join("owner.db"));
    let accepted = accept(&mut store, "bounded-lease", "bounded-r", None);
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/index_jobs.json")).unwrap();
    for case in fixture["lease_cases"].as_array().unwrap() {
        let result = store.claim_index(
            &a(),
            &p(),
            accepted.actual_revision.as_ref(),
            fixture["schema"].as_str().unwrap(),
            fixture["worker"].as_str().unwrap(),
            case["lease_ms"].as_u64().unwrap(),
        );
        if case["valid"].as_bool().unwrap() {
            assert!(result.unwrap().is_some());
        } else {
            assert!(matches!(result, Err(Error::Invalid(_))));
        }
    }
}
