use ledgrrr_sysml_adapter::{
    projection::{project_bundle, source_requirement_query, unverified_requirements_query},
    query::{QueryEngine, QueryLimits},
};
use ufo_types::revision::*;
fn engine(limits: QueryLimits) -> Result<QueryEngine, ledgrrr_sysml_adapter::query::QueryError> {
    QueryEngine::new(limits).map(|e| e.with_worker_path(env!("CARGO_BIN_EXE_revision-owner")))
}
fn fixture() -> PortableBundle {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/revision_projection.json")).unwrap();
    PortableBundle::dehydrate(
        serde_json::from_value(f["model"].clone()).unwrap(),
        serde_json::from_value(f["context"].clone()).unwrap(),
        serde_json::from_value(f["artifacts"].clone()).unwrap(),
    )
    .unwrap()
}
fn request(query: impl Into<String>) -> RevisionQueryRequest {
    RevisionQueryRequest {
        project: ProjectId::new("project-1").unwrap(),
        branch: BranchId::new("main").unwrap(),
        selector: RevisionQuerySelector::Exact {
            revision: RevisionId::new("accepted").unwrap(),
        },
        query: query.into(),
        deadline_ms: 2000,
    }
}
#[tokio::test]
async fn exact_discovery_returns_evidence_paths_and_unverified_requirements() {
    let b = fixture();
    let p = project_bundle(&b, &RevisionId::new("accepted").unwrap()).unwrap();
    let engine = engine(QueryLimits::default()).unwrap();
    for query in [unverified_requirements_query(), source_requirement_query()] {
        let r = engine
            .evaluate(&request(query), &p.descriptor, &p.nquads)
            .await
            .unwrap();
        let RevisionQueryResults::Select { rows, .. } = r else {
            panic!()
        };
        assert!(!rows.is_empty());
    }
    let r=engine.evaluate(&request("SELECT ?x ?absent WHERE { VALUES ?x {\"01.2300\" \"\"} OPTIONAL {<urn:none> <urn:none> ?absent}}"),&p.descriptor,&p.nquads).await.unwrap();
    let RevisionQueryResults::Select { rows, .. } = r else {
        panic!()
    };
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| !r.contains_key("absent")));
}
#[tokio::test]
async fn forbidden_ast_and_tampered_artifacts_fail_without_execution() {
    let p = project_bundle(&fixture(), &RevisionId::new("accepted").unwrap()).unwrap();
    let engine = engine(QueryLimits::default()).unwrap();
    let f: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/revision_projection.json")).unwrap();
    for q in f["forbidden_queries"].as_array().unwrap() {
        assert!(
            engine
                .evaluate(&request(q.as_str().unwrap()), &p.descriptor, &p.nquads)
                .await
                .is_err(),
            "{q}"
        );
    }
    let mut bytes = p.nquads.clone();
    bytes.push(b' ');
    assert!(engine
        .evaluate(&request("ASK {}"), &p.descriptor, &bytes)
        .await
        .is_err());
    assert_eq!(engine.active_workers(), 0);
}
fn empty_projection() -> ledgrrr_sysml_adapter::projection::ProjectedGraph {
    let mut b = fixture();
    b.model.elements.clear();
    b.model.relations.clear();
    b.manifest.semantic_digest = b.model.semantic_digest().unwrap();
    project_bundle(&b, &RevisionId::new("accepted").unwrap()).unwrap()
}
fn cartesian() -> String {
    let values = (0..250)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    (0..5)
        .map(|i| format!("VALUES ?v{i} {{{values}}}"))
        .collect::<Vec<_>>()
        .join(" ")
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn expensive_prefirstrow_and_lazy_iteration_cancel_and_join_before_capacity_reuse() {
    let p = empty_projection();
    let engine = engine(QueryLimits {
        max_workers: 1,
        ..Default::default()
    })
    .unwrap();
    for q in [
        format!(
            "SELECT (COUNT(*) AS ?count) WHERE {{ {} FILTER(?v0+?v1+?v2+?v3+?v4>=0) }}",
            cartesian()
        ),
        format!(
            "SELECT ?v0 ?v1 ?v2 ?v3 ?v4 WHERE {{ {} FILTER(?v0+?v1+?v2+?v3+?v4<0) }}",
            cartesian()
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let (case, q) = q;
        let mut r = request(q);
        r.deadline_ms = 150;
        let prior_started = engine.evaluations_started();
        let prior_killed = engine.killed_workers();
        let prior_joined = engine.joined_workers();
        let prior_lazy = engine.lazy_evaluations_started();
        let started = std::time::Instant::now();
        let error = engine
            .evaluate(&r, &p.descriptor, &p.nquads)
            .await
            .unwrap_err();
        assert_eq!(error.reason(), QueryUnavailableReason::DeadlineExceeded);
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
        assert_eq!(engine.active_workers(), 0);
        assert!(
            engine.evaluations_started() > prior_started,
            "deadline before engine execute does not prove engine cancellation"
        );
        assert!(
            engine.killed_workers() > prior_killed,
            "noncooperative engine must be killed and reaped"
        );
        assert!(engine.joined_workers() > prior_joined);
        if case == 1 {
            assert!(
                engine.lazy_evaluations_started() > prior_lazy,
                "must exercise lazy results"
            );
        }
        assert!(engine
            .evaluate(&request("ASK {}"), &p.descriptor, &p.nquads)
            .await
            .is_ok());
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dataset_scan_observes_real_engine_token_cancellation() {
    let p = project_bundle(&fixture(), &RevisionId::new("accepted").unwrap()).unwrap();
    let engine = engine(QueryLimits {
        max_workers: 1,
        ..Default::default()
    })
    .unwrap();
    let mut r=request(format!("SELECT ?s WHERE {{ ?s ?p ?o FILTER(CONTAINS(REPLACE(STR(?s),\".\",\"{}\"),\"definitely-absent\")) }}", "a".repeat(8192)));
    r.deadline_ms = 1000;
    let e = engine
        .evaluate(&r, &p.descriptor, &p.nquads)
        .await
        .unwrap_err();
    assert_eq!(e.reason(), QueryUnavailableReason::DeadlineExceeded);
    assert!(engine.evaluations_started() > 0);
    assert!(
        engine.evaluations_cancelled() > 0,
        "storage scan must surface engine Cancelled, not only supervisor deadline"
    );
    assert_eq!(
        engine.killed_workers(),
        0,
        "cooperative scan should finish before hard kill"
    );
    assert_eq!(engine.active_workers(), 0);
    assert_eq!(engine.joined_workers(), 1);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn disconnect_cancels_worker_and_capacity_returns_without_detached_evaluation() {
    let p = empty_projection();
    let engine = engine(QueryLimits {
        max_workers: 1,
        ..Default::default()
    })
    .unwrap();
    let task_engine = engine.clone();
    let task_p = p.clone();
    let query = format!(
        "SELECT (COUNT(*) AS ?n) WHERE {{ {} FILTER(?v0+?v1+?v2+?v3+?v4>=0) }}",
        cartesian()
    );
    let task = tokio::spawn(async move {
        task_engine
            .evaluate(&request(query), &task_p.descriptor, &task_p.nquads)
            .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while engine.evaluations_started() == 0 {
            tokio::task::yield_now().await
        }
    })
    .await
    .unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(engine
        .evaluate(&request("ASK {}"), &p.descriptor, &p.nquads)
        .await
        .is_ok());
    assert_eq!(engine.active_workers(), 0);
    assert!(engine.joined_workers() >= 2);
}
#[tokio::test]
async fn configured_bounds_cover_input_ast_graph_rows_output_and_deadlines() {
    let p = empty_projection();
    for (limits, q) in [
        (
            QueryLimits {
                max_query_bytes: 5,
                ..Default::default()
            },
            "SELECT * WHERE {}",
        ),
        (
            QueryLimits {
                max_ast_nodes: 1,
                ..Default::default()
            },
            "SELECT * WHERE {?s ?p ?o}",
        ),
        (
            QueryLimits {
                max_ast_depth: 1,
                ..Default::default()
            },
            "SELECT ?s WHERE {?s ?p ?o FILTER(?s=?o)}",
        ),
        (
            QueryLimits {
                max_rows: 1,
                ..Default::default()
            },
            "SELECT ?s WHERE {VALUES ?s {1 2}}",
        ),
        (
            QueryLimits {
                max_result_bytes: 30,
                ..Default::default()
            },
            "SELECT ?s WHERE {VALUES ?s {\"0123456789\"}}",
        ),
        (
            QueryLimits {
                max_graph_bytes: 10,
                ..Default::default()
            },
            "ASK {}",
        ),
        (
            QueryLimits {
                max_quads: 1,
                ..Default::default()
            },
            "ASK {}",
        ),
    ] {
        let engine = engine(limits).unwrap();
        assert_eq!(
            engine
                .evaluate(&request(q), &p.descriptor, &p.nquads)
                .await
                .unwrap_err()
                .reason(),
            QueryUnavailableReason::CapacityExceeded
        );
        assert_eq!(engine.active_workers(), 0);
    }
    let engine = engine(QueryLimits {
        max_deadline_ms: 1,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        engine
            .evaluate(&request("ASK {}"), &p.descriptor, &p.nquads)
            .await
            .unwrap_err()
            .reason(),
        QueryUnavailableReason::DeadlineExceeded
    );
    let mut req = request("ASK {}");
    req.project = ProjectId::new("other").unwrap();
    assert!(engine
        .evaluate(&req, &p.descriptor, &p.nquads)
        .await
        .is_err());
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn worker_sets_safe_address_space_and_cpu_limits_before_reading_input() {
    use std::process::Stdio;
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_revision-owner"))
        .arg("query-worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let pid = child.id().unwrap();
    let observed = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let limits = std::fs::read_to_string(format!("/proc/{pid}/limits")).unwrap();
            let bound = |name: &str, value: &str| {
                limits
                    .lines()
                    .find(|l| l.starts_with(name))
                    .is_some_and(|line| {
                        let fields: Vec<_> = line[name.len()..].split_whitespace().collect();
                        fields.first() == Some(&value) && fields.get(1) == Some(&value)
                    })
            };
            if bound("Max cpu time", "6") && bound("Max address space", "1073741824") {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    child.kill().await.unwrap();
    child.wait().await.unwrap();
    assert!(
        observed.is_ok(),
        "actual worker must apply AS/CPU soft+hard caps before stdin completes"
    );
    assert!(
        !std::path::Path::new(&format!("/proc/{pid}")).exists(),
        "worker must be reaped"
    );
}
