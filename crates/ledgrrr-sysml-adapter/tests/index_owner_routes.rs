use ledgrrr_revision_io::*;
use ledgrrr_sysml_adapter::{
    client::{Bounds, NativeClient},
    promotion::Owner,
    query::QueryEngine,
    server::{self, Credential, Host},
};
use std::sync::Arc;
use ufo_types::revision::{
    QueryUnavailableReason, RevisionQueryOutcome, RevisionQueryRequest, RevisionQueryResponse,
};

#[tokio::test]
async fn query_boundary_authenticates_validates_and_reports_unavailable_truthfully() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("owner.db");
    let owner = ActorId::new("owner").unwrap();
    let project = ProjectId::new("p").unwrap();
    let mut store = Store::open(&path).unwrap();
    store
        .bootstrap_project(
            &owner,
            &project,
            &ProjectBinding {
                provider: "native".into(),
                remote_project: "remote-project".into(),
                model_dialect: "SysML-v2".into(),
            },
        )
        .unwrap();
    store
        .register_branch(
            &owner,
            &project,
            &BranchId::new("b").unwrap(),
            "remote-branch",
        )
        .unwrap();
    drop(store);
    let token = "query-owner-token-at-least-thirty-two-bytes";
    let host = Arc::new(Host {
        owner: Owner {
            store_path: path,
            actor: owner.clone(),
            client: NativeClient::new("http://127.0.0.1:9", Bounds::default()).unwrap(),
        },
        credentials: vec![Credential {
            token: token.into(),
            actor: owner,
        }],
        permits: tokio::sync::Semaphore::new(4),
        query_engine: QueryEngine::new(Default::default())
            .unwrap()
            .with_worker_path(env!("CARGO_BIN_EXE_revision-owner")),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task =
        tokio::spawn(async move { axum::serve(listener, server::router(host)).await.unwrap() });
    let client = reqwest::Client::new();
    let root = format!("http://{address}");
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/index_route_cases.json")).unwrap();
    let route = format!("{root}/v1/projects/p/branches/b/query");
    assert_eq!(
        client
            .post(&route)
            .json(&fixture["request"])
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    for case in fixture["invalid"].as_array().unwrap() {
        let mut request = fixture["request"].clone();
        request[case["field"].as_str().unwrap()] = case["value"].clone();
        let response = client
            .post(&route)
            .bearer_auth(token)
            .json(&request)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 400, "{case}");
    }
    let mut foreign = fixture["request"].clone();
    foreign["project"] = serde_json::json!("other");
    assert_eq!(
        client
            .post(format!("{root}/v1/projects/other/branches/b/query"))
            .bearer_auth(token)
            .json(&foreign)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        client
            .get(format!(
                "{root}/v1/projects/p/revisions/not-indexed/checkpoint"
            ))
            .bearer_auth(token)
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    assert_eq!(
        client
            .get(format!(
                "{root}/v1/projects/other/revisions/not-indexed/checkpoint"
            ))
            .bearer_auth(token)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    let response = client
        .post(&route)
        .bearer_auth(token)
        .json(&fixture["request"])
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let response: RevisionQueryResponse = response.json().await.unwrap();
    let request: RevisionQueryRequest = serde_json::from_value(fixture["request"].clone()).unwrap();
    response.validate_for(&request).unwrap();
    assert!(matches!(
        response.outcome,
        RevisionQueryOutcome::Unavailable {
            reason: QueryUnavailableReason::ExternalRevision,
            ..
        }
    ));
    task.abort();
    let _ = task.await;
}
