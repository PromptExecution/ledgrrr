use ledgrrr_revision_io::*;
use std::collections::BTreeMap;
use ufo_types::revision::{ArtifactPath, PortableModel, RevisionContext};

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/durable_revision_cases.json")).unwrap()
}
fn actor(s: &str) -> ActorId {
    ActorId::new(s).unwrap()
}
fn project() -> ProjectId {
    ProjectId::new("project-1").unwrap()
}
fn branch() -> BranchId {
    BranchId::new("main").unwrap()
}
fn binding() -> ProjectBinding {
    serde_json::from_value(fixture()["binding"].clone()).unwrap()
}
fn bundle() -> PortableBundle {
    let f = fixture();
    let model: PortableModel = serde_json::from_value(f["model"].clone()).unwrap();
    let context: RevisionContext = serde_json::from_value(f["context"].clone()).unwrap();
    let artifacts: BTreeMap<ArtifactPath, Vec<u8>> =
        serde_json::from_value(f["artifacts"].clone()).unwrap();
    PortableBundle::dehydrate(model, context, artifacts).unwrap()
}
fn intake(id: &str) -> Intake {
    Intake {
        project: project(),
        branch: branch(),
        operation: OperationId::new(id).unwrap(),
        expected_head: ExpectedHead::Empty,
        binding: binding(),
    }
}
fn setup(path: &std::path::Path) -> Store {
    let mut s = Store::open(path).unwrap();
    s.bootstrap_project(&actor("owner"), &project(), &binding())
        .unwrap();
    s.register_branch(&actor("owner"), &project(), &branch(), "remote-main")
        .unwrap();
    s.set_grant(
        &actor("owner"),
        &project(),
        &actor("proposer"),
        Grant {
            read: true,
            propose: true,
            administer: false,
        },
    )
    .unwrap();
    s.set_grant(
        &actor("owner"),
        &project(),
        &actor("reader"),
        Grant {
            read: true,
            propose: false,
            administer: false,
        },
    )
    .unwrap();
    s
}

#[test]
fn original_and_canonical_bytes_survive_restart_and_retry() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.db");
    let mut s = setup(&path);
    let b = bundle();
    let raw = serde_json::to_vec_pretty(&b).unwrap();
    let i = intake("op-1");
    let first = s.intake(&actor("proposer"), &i, &raw).unwrap();
    assert_eq!(
        s.project_binding(&actor("reader"), &project()).unwrap(),
        binding()
    );
    assert_eq!(
        s.remote_branch(&actor("reader"), &project(), &branch())
            .unwrap(),
        "remote-main"
    );
    assert!(matches!(
        s.project_binding(&actor("denied"), &project()),
        Err(Error::Denied)
    ));
    assert!(matches!(
        s.remote_branch(&actor("denied"), &project(), &branch()),
        Err(Error::Denied)
    ));
    assert_eq!(first.receipt.status, SyncStatus::Pending);
    assert_ne!(first.raw_envelope, first.canonical_envelope);
    drop(s);
    let mut s = Store::open(&path).unwrap();
    assert_eq!(
        s.intake(&actor("proposer"), &i, &b.to_bytes().unwrap())
            .unwrap(),
        first
    );
    assert_eq!(
        s.artifact(&actor("reader"), &project(), &first.raw_envelope)
            .unwrap(),
        raw
    );
    for (digest, bytes) in &b.blobs {
        assert_eq!(
            &s.artifact(&actor("reader"), &project(), digest).unwrap(),
            bytes
        );
    }
    assert_eq!(
        s.operation(&actor("reader"), &project(), &i.operation)
            .unwrap(),
        first
    );
}

#[test]
fn immutable_identity_cases_and_denied_paths_roll_back() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(&dir.path().join("store.db"));
    let i = intake("op-identity");
    let b = bundle();
    let initial = s
        .intake(&actor("proposer"), &i, &b.to_bytes().unwrap())
        .unwrap();
    for case in fixture()["intake_cases"].as_array().unwrap() {
        let mut changed = i.clone();
        let mut model = b.clone();
        let mut a = actor("proposer");
        match case["mutation"].as_str().unwrap() {
            "actor" => a = actor("owner"),
            "branch" => {
                changed.branch = BranchId::new("other").unwrap();
                s.register_branch(&actor("owner"), &project(), &changed.branch, "remote-other")
                    .unwrap();
            }
            "expected_head" => {
                changed.expected_head = ExpectedHead::Revision(RevisionId::new("base").unwrap());
                model.manifest.context.parent = Some(RevisionId::new("base").unwrap());
            }
            "extension" => {
                model
                    .model
                    .elements
                    .values_mut()
                    .next()
                    .unwrap()
                    .extensions
                    .insert("changed".into(), serde_json::json!(true));
                model.manifest.semantic_digest = model.model.semantic_digest().unwrap();
            }
            "source_bytes" => {
                let p = ArtifactPath::new("attachments/evidence.bin").unwrap();
                let d = model.manifest.artifacts[&p].clone();
                model.blobs.remove(&d);
                let bytes = vec![0, 255, 1, 13, 10];
                let d = ArtifactDigest::of(&bytes);
                model.blobs.insert(d.clone(), bytes);
                model.manifest.artifacts.insert(p, d);
            }
            _ => panic!("unknown case"),
        }
        assert!(
            matches!(
                s.intake(&a, &changed, &model.to_bytes().unwrap()),
                Err(Error::IdentityConflict)
            ),
            "{case}"
        );
        assert_eq!(
            s.operation(&actor("reader"), &project(), &i.operation)
                .unwrap(),
            initial
        );
    }
    assert!(matches!(
        s.intake(&actor("denied"), &intake("denied"), &b.to_bytes().unwrap()),
        Err(Error::Denied)
    ));
    assert!(matches!(
        s.operation(&actor("denied"), &project(), &i.operation),
        Err(Error::Denied)
    ));
    assert!(matches!(
        s.artifact(&actor("denied"), &project(), &initial.raw_envelope),
        Err(Error::Denied)
    ));
    assert!(matches!(
        s.set_grant(&actor("reader"), &project(), &actor("denied"), Grant::OWNER),
        Err(Error::Denied)
    ));
    assert!(s
        .set_grant(
            &actor("owner"),
            &project(),
            &actor("owner"),
            Grant {
                read: true,
                propose: true,
                administer: false
            }
        )
        .is_err());
    s.set_grant(
        &actor("owner"),
        &project(),
        &actor("reader"),
        Grant {
            read: false,
            propose: false,
            administer: false,
        },
    )
    .unwrap();
    assert!(matches!(
        s.operation(&actor("reader"), &project(), &i.operation),
        Err(Error::Denied)
    ));
    let mut invalid = b.clone();
    invalid.blobs.values_mut().next().unwrap().push(42);
    assert!(s
        .intake(
            &actor("owner"),
            &intake("corrupt"),
            &serde_json::to_vec(&invalid).unwrap()
        )
        .is_err());
    assert!(matches!(
        s.operation(
            &actor("owner"),
            &project(),
            &OperationId::new("corrupt").unwrap()
        ),
        Err(Error::NotFound)
    ));
}

#[test]
fn independent_connections_race_one_intake() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.db");
    drop(setup(&path));
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let p = path.clone();
            let gate = barrier.clone();
            std::thread::spawn(move || {
                let mut s = Store::open(&p).unwrap();
                gate.wait();
                s.intake(
                    &actor("proposer"),
                    &intake("race"),
                    &bundle().to_bytes().unwrap(),
                )
                .unwrap()
            })
        })
        .collect();
    let mut values = handles.into_iter().map(|h| h.join().unwrap());
    assert_eq!(values.next(), values.next());
}

#[test]
fn future_and_unrelated_schema_fail_closed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.db");
    drop(setup(&path));
    let c = rusqlite::Connection::open(&path).unwrap();
    c.pragma_update(None, "user_version", 99).unwrap();
    drop(c);
    assert!(matches!(Store::open(&path), Err(Error::Schema)));
    let p = dir.path().join("other.db");
    let c = rusqlite::Connection::open(&p).unwrap();
    c.execute_batch("CREATE TABLE unrelated (value TEXT);INSERT INTO unrelated VALUES ('retain');")
        .unwrap();
    drop(c);
    assert!(matches!(Store::open(&p), Err(Error::Schema)));
    assert_eq!(
        rusqlite::Connection::open(p)
            .unwrap()
            .query_row("SELECT value FROM unrelated", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "retain"
    );
}

fn evidence(value: &StoredOperation) -> CommitEvidence {
    CommitEvidence {
        token: value.dispatch.clone().unwrap(),
        binding: binding(),
        expected_head: value.receipt.expected_head.clone(),
        proposal_digest: value.receipt.proposal_digest.clone(),
        actual_revision: RevisionId::new("actual-provider-42").unwrap(),
        observation: "GET actual revision and verified immutable proposal identity".into(),
    }
}
fn checkpoint() -> IndexCheckpoint {
    IndexCheckpoint {
        project: project(),
        revision: RevisionId::new("actual-provider-42").unwrap(),
        dialect: "SysML-v2".into(),
        graph_digest: ArtifactDigest::of(b"complete staged graph"),
    }
}
#[test]
fn dispatch_ambiguity_blocks_restart_and_competing_connections() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.db");
    let mut s = setup(&path);
    let one = s
        .intake(
            &actor("proposer"),
            &intake("one"),
            &bundle().to_bytes().unwrap(),
        )
        .unwrap();
    let two = s
        .intake(
            &actor("proposer"),
            &intake("two"),
            &bundle().to_bytes().unwrap(),
        )
        .unwrap();
    assert!(matches!(
        s.start_dispatch(
            &actor("proposer"),
            &project(),
            &one.receipt.operation,
            0,
            "request prepared"
        ),
        Err(Error::Denied)
    ));
    let dispatched = s
        .start_dispatch(
            &actor("owner"),
            &project(),
            &one.receipt.operation,
            0,
            "durable request identity: one",
        )
        .unwrap();
    let token = dispatched.dispatch.clone().unwrap();
    assert!(matches!(
        s.start_dispatch(
            &actor("owner"),
            &project(),
            &two.receipt.operation,
            0,
            "two"
        ),
        Err(Error::BranchBlocked)
    ));
    let ambiguous = s
        .mark_ambiguous(
            &actor("owner"),
            &token,
            dispatched.generation,
            "connection lost after send",
        )
        .unwrap();
    drop(s);
    let mut s = Store::open(&path).unwrap();
    assert_eq!(
        s.operation(&actor("reader"), &project(), &one.receipt.operation)
            .unwrap(),
        ambiguous
    );
    assert!(matches!(
        s.start_dispatch(
            &actor("owner"),
            &project(),
            &one.receipt.operation,
            ambiguous.generation,
            "resend"
        ),
        Err(Error::BranchBlocked)
    ));
    assert!(matches!(
        s.start_dispatch(
            &actor("owner"),
            &project(),
            &two.receipt.operation,
            0,
            "two"
        ),
        Err(Error::BranchBlocked)
    ));
    let e = evidence(&ambiguous);
    let committed = s
        .record_commit(&actor("owner"), ambiguous.generation, &e)
        .unwrap();
    assert_eq!(committed.actual_revision.as_ref(), Some(&e.actual_revision));
    assert!(committed.indexing_work.is_some());
    assert_eq!(
        s.record_commit(&actor("owner"), ambiguous.generation, &e)
            .unwrap(),
        committed
    );
    let later = s
        .start_dispatch(
            &actor("owner"),
            &project(),
            &two.receipt.operation,
            0,
            "two",
        )
        .unwrap();
    assert!(later.dispatch.unwrap().fence > token.fence);
    assert_eq!(
        s.recovery(&actor("reader"), &project(), None, 1)
            .unwrap()
            .len(),
        1
    );
    assert!(matches!(
        s.recovery(&actor("denied"), &project(), None, 1),
        Err(Error::Denied)
    ));
}
#[test]
fn transition_and_checkpoint_preconditions() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(&dir.path().join("store.db"));
    let op = intake("transitions");
    let pending = s
        .intake(&actor("proposer"), &op, &bundle().to_bytes().unwrap())
        .unwrap();
    assert!(matches!(
        s.publisher(&actor("proposer"), &project()),
        Err(Error::Denied)
    ));
    assert!(matches!(
        s.publisher(&actor("owner"), &project())
            .unwrap()
            .record_index(&op.operation, 0, &checkpoint()),
        Err(Error::Transition)
    ));
    assert!(matches!(
        s.start_dispatch(&actor("owner"), &project(), &op.operation, 99, "dispatch"),
        Err(Error::Stale)
    ));
    let dispatched = s
        .start_dispatch(
            &actor("owner"),
            &project(),
            &op.operation,
            pending.generation,
            "dispatch",
        )
        .unwrap();
    let good = evidence(&dispatched);
    for mutation in fixture()["commit_evidence_mutations"].as_array().unwrap() {
        let mutation = mutation.as_str().unwrap();
        let mut bad = good.clone();
        match mutation {
            "fence" => bad.token.fence += 1,
            "project" => bad.token.project = ProjectId::new("other").unwrap(),
            "digest" => bad.proposal_digest = ArtifactDigest::of(b"other"),
            "head" => bad.expected_head = ExpectedHead::Revision(RevisionId::new("wrong").unwrap()),
            "binding" => bad.binding.remote_project = "wrong".into(),
            _ => unreachable!(),
        }
        assert!(
            s.record_commit(&actor("owner"), dispatched.generation, &bad)
                .is_err(),
            "{mutation}"
        );
    }
    let committed = s
        .record_commit(&actor("owner"), dispatched.generation, &good)
        .unwrap();
    for mutation in fixture()["checkpoint_mutations"].as_array().unwrap() {
        let mutation = mutation.as_str().unwrap();
        let mut bad = checkpoint();
        match mutation {
            "project" => bad.project = ProjectId::new("wrong").unwrap(),
            "revision" => bad.revision = RevisionId::new("wrong").unwrap(),
            "dialect" => bad.dialect = "wrong".into(),
            _ => unreachable!(),
        };
        assert!(s
            .publisher(&actor("owner"), &project())
            .unwrap()
            .record_index(&op.operation, committed.generation, &bad)
            .is_err());
    }
    assert!(matches!(
        s.publisher(&actor("owner"), &project())
            .unwrap()
            .record_index(&op.operation, 0, &checkpoint()),
        Err(Error::Stale)
    ));
    let indexed = s
        .publisher(&actor("owner"), &project())
        .unwrap()
        .record_index(&op.operation, committed.generation, &checkpoint())
        .unwrap();
    assert_eq!(
        s.publisher(&actor("owner"), &project())
            .unwrap()
            .record_index(&op.operation, committed.generation, &checkpoint())
            .unwrap(),
        indexed
    );
    let mut bad = checkpoint();
    bad.graph_digest = ArtifactDigest::of(b"different");
    assert!(s
        .publisher(&actor("owner"), &project())
        .unwrap()
        .record_index(&op.operation, indexed.generation, &bad)
        .is_err());
}
#[test]
fn persisted_corruption_and_failed_sql_intake_are_not_success() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.db");
    let mut s = setup(&path);
    let op = intake("corruption");
    let v = s
        .intake(&actor("proposer"), &op, &bundle().to_bytes().unwrap())
        .unwrap();
    drop(s);
    let c = rusqlite::Connection::open(&path).unwrap();
    c.execute(
        "UPDATE artifacts SET bytes=?1 WHERE digest=?2",
        rusqlite::params![b"tampered".as_slice(), v.raw_envelope.as_str()],
    )
    .unwrap();
    drop(c);
    let mut s = Store::open(&path).unwrap();
    assert!(matches!(
        s.operation(&actor("reader"), &project(), &op.operation),
        Err(Error::Corrupt(_))
    ));
    drop(s);
    let path = dir.path().join("rollback.db");
    let mut s = setup(&path);
    let c = rusqlite::Connection::open(&path).unwrap();
    c.execute_batch("CREATE TRIGGER fail_operation BEFORE INSERT ON operations BEGIN SELECT RAISE(ABORT,'test interruption before commit'); END;").unwrap();
    drop(c);
    assert!(s
        .intake(
            &actor("proposer"),
            &intake("rollback"),
            &bundle().to_bytes().unwrap()
        )
        .is_err());
    drop(s);
    let c = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM artifacts", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM operations", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn competing_dispatch_is_single_and_old_index_does_not_release_new_work() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.db");
    let mut s = setup(&path);
    for id in ["race-a", "race-b", "race-c"] {
        s.intake(
            &actor("proposer"),
            &intake(id),
            &bundle().to_bytes().unwrap(),
        )
        .unwrap();
    }
    drop(s);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = ["race-a", "race-b"]
        .into_iter()
        .map(|id| {
            let p = path.clone();
            let gate = barrier.clone();
            std::thread::spawn(move || {
                let mut s = Store::open(p).unwrap();
                gate.wait();
                s.start_dispatch(
                    &actor("owner"),
                    &project(),
                    &OperationId::new(id).unwrap(),
                    0,
                    "request identity",
                )
            })
        })
        .collect();
    let outcomes: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(outcomes.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|r| matches!(r, Err(Error::BranchBlocked)))
            .count(),
        1
    );
    let dispatched = outcomes.into_iter().find_map(Result::ok).unwrap();
    let mut s = Store::open(path).unwrap();
    let committed = s
        .record_commit(
            &actor("owner"),
            dispatched.generation,
            &evidence(&dispatched),
        )
        .unwrap();
    let second = s
        .start_dispatch(
            &actor("owner"),
            &project(),
            &OperationId::new("race-c").unwrap(),
            0,
            "second request",
        )
        .unwrap();
    s.publisher(&actor("owner"), &project())
        .unwrap()
        .record_index(
            &committed.receipt.operation,
            committed.generation,
            &checkpoint(),
        )
        .unwrap();
    assert!(matches!(
        s.start_dispatch(
            &actor("owner"),
            &project(),
            &second.receipt.operation,
            second.generation,
            "resend"
        ),
        Err(Error::BranchBlocked)
    ));
}
#[test]
fn no_global_hash_access_and_status_rejection_before_dispatch_only() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = setup(&dir.path().join("store.db"));
    let op = intake("rejected");
    let v = s
        .intake(&actor("proposer"), &op, &bundle().to_bytes().unwrap())
        .unwrap();
    let other = ProjectId::new("project-other").unwrap();
    s.bootstrap_project(&actor("owner"), &other, &ProjectBinding { remote_project: "other-remote".into(), ..binding() })
        .unwrap();
    assert!(matches!(
        s.artifact(&actor("owner"), &other, &v.raw_envelope),
        Err(Error::NotFound)
    ));
    let conflict = SyncStatus::Conflict {
        paths: vec!["/elements/REQ-2/name".into()],
    };
    let rejected = s
        .reject_before_dispatch(
            &actor("owner"),
            &project(),
            &op.operation,
            0,
            conflict.clone(),
        )
        .unwrap();
    assert_eq!(
        s.reject_before_dispatch(&actor("owner"), &project(), &op.operation, 0, conflict)
            .unwrap(),
        rejected
    );
    assert!(matches!(
        s.start_dispatch(
            &actor("owner"),
            &project(),
            &op.operation,
            rejected.generation,
            "rejected"
        ),
        Err(Error::Transition)
    ));
    let op = intake("dispatched-rejection");
    s.intake(&actor("proposer"), &op, &bundle().to_bytes().unwrap())
        .unwrap();
    let v = s
        .start_dispatch(&actor("owner"), &project(), &op.operation, 0, "sent")
        .unwrap();
    assert!(matches!(
        s.reject_before_dispatch(
            &actor("owner"),
            &project(),
            &op.operation,
            v.generation,
            SyncStatus::Unavailable {
                reason: "network timeout".into()
            }
        ),
        Err(Error::Transition)
    ));
    assert!(matches!(
        s.mark_ambiguous(
            &actor("owner"),
            v.dispatch.as_ref().unwrap(),
            u64::MAX,
            "timeout"
        ),
        Err(Error::Stale)
    ));
}
#[test]
fn malformed_records_and_missing_artifacts_fail_closed() {
    for mode in ["receipt", "missing", "schema"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("store.db");
        let mut s = setup(&path);
        let op = intake("bad-record");
        let v = s
            .intake(&actor("proposer"), &op, &bundle().to_bytes().unwrap())
            .unwrap();
        drop(s);
        let c = rusqlite::Connection::open(&path).unwrap();
        match mode {
            "receipt" => {
                let mut record = v.clone();
                record.receipt.actor = actor("forged");
                c.execute(
                    "UPDATE operations SET record=?1",
                    [ufo_types::revision::canonical_bytes(&record).unwrap()],
                )
                .unwrap();
            }
            "missing" => {
                c.execute(
                    "DELETE FROM project_artifacts WHERE digest=?1",
                    [v.raw_envelope.as_str()],
                )
                .unwrap();
            }
            "schema" => {
                c.execute_batch("ALTER TABLE operations ADD COLUMN unknown TEXT")
                    .unwrap();
            }
            _ => unreachable!(),
        }
        drop(c);
        if mode == "schema" {
            assert!(matches!(Store::open(&path), Err(Error::Schema)));
        } else {
            let mut s = Store::open(path).unwrap();
            assert!(matches!(
                s.operation(&actor("owner"), &project(), &op.operation),
                Err(Error::Corrupt(_))
            ));
            assert!(s
                .intake(&actor("proposer"), &op, &bundle().to_bytes().unwrap())
                .is_err());
        }
    }
}

#[test]
fn interruption_child() {
    let Ok(mode) = std::env::var("REVISION_TEST_CHILD_MODE") else {
        return;
    };
    let path = std::path::PathBuf::from(std::env::var_os("REVISION_TEST_DB").unwrap());
    let ready = std::path::PathBuf::from(std::env::var_os("REVISION_TEST_READY").unwrap());
    if mode == "after" {
        let mut s = Store::open(&path).unwrap();
        s.intake(
            &actor("proposer"),
            &intake("process"),
            &bundle().to_bytes().unwrap(),
        )
        .unwrap();
    } else {
        // Interrupt a real transaction after immutable artifact insertion. The
        // SQL failure test above separately covers Store::intake's rollback.
        let mut c = rusqlite::Connection::open(&path).unwrap();
        let tx = c
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let bytes = bundle().to_bytes().unwrap();
        let d = ArtifactDigest::of(&bytes);
        tx.execute(
            "INSERT INTO artifacts(digest,bytes) VALUES (?1,?2)",
            rusqlite::params![d.as_str(), bytes],
        )
        .unwrap();
        tx.execute(
            "INSERT INTO project_artifacts(project,digest) VALUES (?1,?2)",
            rusqlite::params![project().as_str(), d.as_str()],
        )
        .unwrap();
        std::fs::write(&ready, b"transaction inserted, not committed").unwrap();
        loop {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    std::fs::write(&ready, b"public intake committed").unwrap();
    loop {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
#[test]
fn real_process_interruption_before_and_after_commit() {
    for mode in ["before", "after"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("store.db");
        drop(setup(&path));
        let ready = dir.path().join("ready");
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "interruption_child", "--nocapture"])
            .env("REVISION_TEST_CHILD_MODE", mode)
            .env("REVISION_TEST_DB", &path)
            .env("REVISION_TEST_READY", &ready)
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let start = std::time::Instant::now();
        while !ready.exists() && start.elapsed() < std::time::Duration::from_secs(10) {
            assert!(child.try_wait().unwrap().is_none());
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        if !ready.exists() {
            let _ = child.kill();
            let _ = child.wait();
            panic!("child transaction handshake timed out");
        }
        child.kill().unwrap();
        child.wait().unwrap();
        let mut s = Store::open(&path).unwrap();
        let result = s.operation(
            &actor("owner"),
            &project(),
            &OperationId::new("process").unwrap(),
        );
        if mode == "before" {
            assert!(matches!(result, Err(Error::NotFound)));
            let c = rusqlite::Connection::open(path).unwrap();
            assert_eq!(
                c.query_row("SELECT COUNT(*) FROM artifacts", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                0
            );
        } else {
            let v = result.unwrap();
            assert_eq!(
                s.artifact(&actor("owner"), &project(), &v.canonical_envelope)
                    .unwrap(),
                bundle().to_bytes().unwrap()
            );
        }
    }
}

#[test]
fn physical_project_alias_is_rejected_across_dialects_and_connections() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("physical.db");
    drop(setup(&path));
    let mut s = Store::open(&path).unwrap();
    let mut alias = binding();
    alias.model_dialect = "KerML".into();
    assert!(matches!(s.bootstrap_project(&actor("owner"), &ProjectId::new("alias").unwrap(), &alias), Err(Error::IdentityConflict)));
    drop(s);
    let mut s = Store::open(path).unwrap();
    assert!(matches!(s.bootstrap_project(&actor("owner"), &ProjectId::new("alias-reopened").unwrap(), &binding()), Err(Error::IdentityConflict)));
}
