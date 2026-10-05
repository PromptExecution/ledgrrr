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
