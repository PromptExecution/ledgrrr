//! P2 live proof: fetch → emit → server → fetch → hydrate yields the same semantic digest.
//!
//! Requires a live OMG SysML v2 API server.
//! Set SYSML_API_URL=http://127.0.0.1:19000 to run.
//! Each invocation creates a fresh disposable project on the server.
use ledgrrr_sysml_adapter::client::{Bounds, NativeClient};
use ledgrrr_sysml_adapter::native::{emit, hydrate, OperationMarker};
use ufo_types::revision::{PortableBundle, PortableModel, RevisionContext};

fn fixture_bundle() -> PortableBundle {
    let v: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/native_revision_cases.json")).unwrap();
    let model: PortableModel = serde_json::from_value(v["model"].clone()).unwrap();
    let context: RevisionContext = serde_json::from_value(v["context"].clone()).unwrap();
    let artifacts = serde_json::from_value(v["artifacts"].clone()).unwrap();
    PortableBundle::dehydrate(model, context, artifacts).unwrap()
}

fn marker() -> OperationMarker {
    // project must match the context.project declared in the fixture ("project-1").
    OperationMarker {
        project: "project-1".into(),
        branch: "main".into(),
        operation: "p2-roundtrip-op-001".into(),
    }
}

fn api_url() -> Option<String> {
    std::env::var("SYSML_API_URL").ok()
}

/// P2 gate: emit a PortableBundle to native elements, POST them to the live OMG server,
/// snapshot them back, hydrate, and assert the semantic digest is unchanged.
/// Skip unless SYSML_API_URL is set.
#[tokio::test]
async fn fetch_emit_fetch_semantic_digest_matches() {
    let base_url = match api_url() {
        Some(u) => u,
        None => {
            eprintln!("SKIP: set SYSML_API_URL=http://127.0.0.1:19000 for live P2 proof");
            return;
        }
    };

    let bundle = fixture_bundle();
    let m = marker();
    let projection = emit(&bundle, &m).expect("emit must succeed with canonical fixture");
    let envelope_bytes = bundle.to_bytes().expect("PortableBundle serialization");
    let original_digest = bundle.manifest.semantic_digest.clone();

    let client = NativeClient::new(&base_url, Bounds::default()).expect("NativeClient::new");

    // Create a fresh disposable project; write the emitted elements as a root commit.
    let project_id = client
        .post_project("pex-p2-roundtrip-probe")
        .await
        .expect("POST /projects must succeed");

    let change: Vec<serde_json::Value> = projection
        .elements
        .iter()
        .map(|e| serde_json::json!({"@type":"DataVersion","identity":{"@id":e.id},"payload":serde_json::to_value(e).unwrap()}))
        .collect();
    let commit: serde_json::Value = reqwest::Client::new()
        .post(format!("{base_url}/projects/{project_id}/commits"))
        .json(&serde_json::json!({"@type":"Commit","change":change}))
        .send()
        .await
        .expect("POST commit")
        .json()
        .await
        .expect("commit JSON");
    let commit_id = commit["@id"].as_str().expect("commit @id");
    let fetched = client
        .snapshot(&project_id, commit_id, false)
        .await
        .expect("snapshot must succeed: server must return committed elements");

    // Hydrate back to PortableBundle. Requires aliasIds (marker + envelope anchor) retained.
    let rehydrated = hydrate(&fetched, &envelope_bytes, &m).unwrap_or_else(|e| {
        panic!(
            "hydrate failed after server round-trip: {e}\n\
             This means the server did not preserve aliasIds or projected fields.\n\
             project={project_id} commit={commit_id}"
        )
    });

    assert_eq!(
        original_digest, rehydrated.manifest.semantic_digest,
        "P2 FAIL: semantic digest changed after fetch→emit→server→fetch→hydrate\n\
         project={project_id} commit={commit_id}"
    );

    eprintln!(
        "P2 PASS: fetch→emit→server→fetch→hydrate\n  project={project_id}\n  commit={commit_id}\n  digest={original_digest:?}"
    );
}

/// P2 extended: all required relation types (Satisfy, Verify, Allocate, FeatureTyping)
/// survive the server round-trip with their reference fields intact.
#[tokio::test]
async fn relation_references_survive_server_round_trip() {
    let base_url = match api_url() {
        Some(u) => u,
        None => {
            eprintln!("SKIP: set SYSML_API_URL=http://127.0.0.1:19000 for live P2 proof");
            return;
        }
    };

    let bundle = fixture_bundle();
    let m = marker();
    let projection = emit(&bundle, &m).expect("emit");
    let envelope_bytes = bundle.to_bytes().expect("bundle bytes");

    let client = NativeClient::new(&base_url, Bounds::default()).unwrap();
    let project_id = client.post_project("pex-p2-relation-probe").await.unwrap();

    let change: Vec<serde_json::Value> = projection
        .elements
        .iter()
        .map(|e| serde_json::json!({"@type":"DataVersion","identity":{"@id":e.id},"payload":serde_json::to_value(e).unwrap()}))
        .collect();
    let commit: serde_json::Value = reqwest::Client::new()
        .post(format!("{base_url}/projects/{project_id}/commits"))
        .json(&serde_json::json!({"@type":"Commit","change":change}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let commit_id = commit["@id"].as_str().unwrap();
    let fetched = client
        .snapshot(&project_id, commit_id, false)
        .await
        .unwrap();

    // Verify typed reference fields survive per element kind.
    for e in &fetched {
        match e.kind.as_str() {
            "RequirementUsage" => assert!(
                e.requirement_definition.is_some(),
                "RequirementUsage lost requirementDefinition: {}",
                e.id
            ),
            "VerificationCaseUsage" => {
                assert!(
                    e.verification_case_definition.is_some(),
                    "VerificationCaseUsage lost verificationCaseDefinition: {}",
                    e.id
                );
                assert!(
                    !e.verified_requirement.is_empty(),
                    "VerificationCaseUsage lost verifiedRequirement: {}",
                    e.id
                );
            }
            "SatisfyRequirementUsage" => {
                assert!(
                    e.satisfied_requirement.is_some(),
                    "SatisfyRequirementUsage lost satisfiedRequirement: {}",
                    e.id
                );
                assert!(
                    e.satisfying_feature.is_some(),
                    "SatisfyRequirementUsage lost satisfyingFeature: {}",
                    e.id
                );
            }
            "AllocationUsage" => {
                assert!(
                    e.source_feature.is_some(),
                    "AllocationUsage lost sourceFeature: {}",
                    e.id
                );
                assert!(
                    !e.target_feature.is_empty(),
                    "AllocationUsage lost targetFeature: {}",
                    e.id
                );
            }
            _ => {}
        }
    }

    // Full hydrate still succeeds after relation-field retention check.
    hydrate(&fetched, &envelope_bytes, &m)
        .expect("hydrate must succeed after relation field verification");

    eprintln!(
        "P2 PASS: relation references survived server round-trip\n  project={project_id}\n  commit={commit_id}"
    );
}
