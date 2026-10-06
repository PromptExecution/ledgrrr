use ledgrrr_revision_io::*;
use ledgrrr_sysml_adapter::{
    client::{Bounds, NativeClient},
    promotion::Owner,
    server::{self, Credential, Host},
};
use serde_json::Value;
use std::sync::Arc;

#[tokio::test]
async fn authenticated_route_inventory_has_no_native_mutation_or_client_actor_controls() {
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/owner_route_cases.json")).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("owner.db");
    let actor = ActorId::new("owner").unwrap();
    let project = ProjectId::new("p").unwrap();
    let mut store = Store::open(&path).unwrap();
    store
        .bootstrap_project(
            &actor,
            &project,
            &ProjectBinding {
                provider: "native".into(),
                remote_project: "remote".into(),
                model_dialect: "SysML-v2".into(),
            },
        )
        .unwrap();
    store
        .register_branch(&actor, &project, &BranchId::new("b").unwrap(), "remote-b")
        .unwrap();
    let credential: Credential = serde_json::from_value(cases["credential"].clone()).unwrap();
    store
        .set_grant(
            &actor,
            &project,
            &credential.actor,
            Grant {
                read: true,
                propose: true,
                administer: false,
            },
        )
        .unwrap();
    let token = credential.token.clone();
    let owner_credential: Credential =
        serde_json::from_value(cases["owner_credential"].clone()).unwrap();
    let owner_token = owner_credential.token.clone();
    drop(store);
    let host = Arc::new(Host {
        owner: Owner {
            store_path: path,
            actor,
            client: NativeClient::new("http://127.0.0.1:9", Bounds::default()).unwrap(),
        },
        credentials: vec![credential, owner_credential],
        permits: tokio::sync::Semaphore::new(1),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task =
        tokio::spawn(async move { axum::serve(listener, server::router(host)).await.unwrap() });
    let client = reqwest::Client::new();
    let root = format!("http://{address}");
    let unauth = client
        .get(format!("{root}/v1/projects/p/recovery"))
        .send()
        .await
        .unwrap();
    assert_eq!(unauth.status(), 401);
    let valid = client
        .get(format!("{root}/v1/projects/p/recovery"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(valid.status(), 200);
    let denied = client
        .get(format!("{root}/v1/projects/other/recovery"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), 403);
    for prefix in cases["alternate_prefixes"].as_array().unwrap() {
        for route in cases["native_mutations"].as_array().unwrap() {
            let method =
                reqwest::Method::from_bytes(route["method"].as_str().unwrap().as_bytes()).unwrap();
            let response = client
                .request(
                    method,
                    format!(
                        "{root}{}{}",
                        prefix.as_str().unwrap(),
                        route["path"].as_str().unwrap()
                    ),
                )
                .bearer_auth(&token)
                .body("{}")
                .send()
                .await
                .unwrap();
            assert!(
                !response.status().is_success(),
                "native route admitted: {route}"
            );
        }
    }
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/native_revision_cases.json")).unwrap();
    let model: ufo_types::revision::PortableModel =
        serde_json::from_value(fixture["model"].clone()).unwrap();
    let mut context: ufo_types::revision::RevisionContext =
        serde_json::from_value(fixture["context"].clone()).unwrap();
    context.project = ProjectId::new("p").unwrap();
    let artifacts = serde_json::from_value(fixture["artifacts"].clone()).unwrap();
    let proposal = PortableBundle::dehydrate(model, context, artifacts).unwrap();
    for field in cases["intake_forbidden"].as_array().unwrap() {
        let body = serde_json::json!({"expected_head":{"kind":"empty"},"bundle":proposal,field.as_str().unwrap():"client-controlled"});
        let response = client
            .post(format!("{root}/v1/projects/p/branches/b/operations/op"))
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 400);
    }
    let duplicate = client
        .post(format!("{root}/v1/projects/p/branches/b/operations/op"))
        .bearer_auth(&token)
        .body("{\"bundle\":{},\"bundle\":{},\"expected_head\":{\"kind\":\"empty\"}}")
        .send()
        .await
        .unwrap();
    assert_eq!(duplicate.status(), 400);
    let raw = serde_json::to_string_pretty(&proposal).unwrap();
    let body = format!("{{\"expected_head\":{{\"kind\":\"empty\"}},\"bundle\":{raw}}}");
    let admitted = client
        .post(format!(
            "{root}/v1/projects/p/branches/b/operations/owner-policy"
        ))
        .bearer_auth(&token)
        .body(body)
        .send()
        .await
        .unwrap();
    assert_eq!(admitted.status(), 200);
    let admitted: StoredOperation = admitted.json().await.unwrap();
    let stored = client
        .get(format!(
            "{root}/v1/projects/p/artifacts/{}",
            admitted.raw_envelope.as_str()
        ))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    assert_eq!(stored.as_ref(), raw.as_bytes());
    let promoted = client
        .post(format!(
            "{root}/v1/projects/p/branches/b/operations/owner-policy/promote"
        ))
        .bearer_auth(&owner_token)
        .send()
        .await
        .unwrap();
    assert_eq!(promoted.status(), 503); // Provider unavailable, privileged policy admitted author correctly.
    let recovered: StoredOperation = client
        .get(format!("{root}/v1/projects/p/operations/owner-policy"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(recovered.receipt.actor.as_str(), "editor");
    assert!(matches!(
        recovered.receipt.status,
        SyncStatus::Unavailable { .. }
    ));
    task.abort();
}
