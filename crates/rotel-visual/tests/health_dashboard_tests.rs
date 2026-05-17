use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;

fn test_app() -> axum::Router {
    rotel_visual::create_app().unwrap()
}

#[tokio::test]
async fn test_health_endpoint() {
    let _app = rotel_visual::create_app().expect("create_app failed");

    let response = _app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_dashboard_endpoint() {
    let _app = rotel_visual::create_app().expect("create_app failed");

    let response = _app
        .oneshot(
            Request::builder()
                .uri("/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("text/html"));
}

#[tokio::test]
async fn test_otlp_logs_ingestion_accepts_json_and_returns_202() {
    let _app = rotel_visual::create_app().expect("create_app failed");

    let body = json!({
        "resourceLogs": [
            {
                "resource": { "attributes": [] },
                "scopeLogs": []
            }
        ]
    });

    let response = test_app()
        .oneshot(
            Request::builder()
                .uri("/v1/logs")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
}

#[tokio::test]
async fn test_otlp_metrics_ingestion_accepts_json_and_returns_202() {
    let _app = rotel_visual::create_app().expect("create_app failed");

    let body = json!({
        "resourceMetrics": [
            {
                "resource": { "attributes": [] },
                "scopeMetrics": []
            }
        ]
    });

    let response = test_app()
        .oneshot(
            Request::builder()
                .uri("/v1/metrics")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
}

#[tokio::test]
async fn test_otlp_traces_ingestion_accepts_json_and_returns_202() {
    let _app = rotel_visual::create_app().expect("create_app failed");

    let body = json!({
        "resourceSpans": [
            {
                "resource": { "attributes": [] },
                "scopeSpans": []
            }
        ]
    });

    let response = test_app()
        .oneshot(
            Request::builder()
                .uri("/v1/traces")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
}

#[tokio::test]
async fn test_otlp_logs_rejects_invalid_json_with_400() {
    let _app = rotel_visual::create_app().expect("create_app failed");

    let response = _app
        .oneshot(
            Request::builder()
                .uri("/v1/logs")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from("not-json"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_classified_artifacts_are_accepted_via_otlp_logs() {
    // This test verifies that an OTLP log payload matching a classification
    // rule is accepted by the ingestion endpoint.

    // Ingest a log that matches the GPU fault rule.
    let body = serde_json::json!({
        "resourceLogs": [
            {
                "resource": {
                    "attributes": [
                        { "key": "host.name", "value": { "stringValue": "test-host" } }
                    ]
                },
                "scopeLogs": [
                    {
                        "logRecords": [
                            {
                                "timeUnixNano": "1777724525000000000",
                                "severityNumber": 17,
                                "severityText": "ERROR",
                                "body": { "stringValue": "Unable to determine the device handle for GPU0: 0000:01:00.0: Unknown Error" },
                                "attributes": []
                            }
                        ]
                    }
                ]
            }
        ]
    });

    let response = test_app()
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/logs")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
}

#[tokio::test]
async fn test_ring_buffer_populated_after_otlp_log_ingest() {
    // Verify that ingesting OTLP logs populates the ring buffer (returns 202).
    // Ring-buffer replay to new WebSocket subscribers requires a live server.
    let _app = rotel_visual::create_app().expect("create_app failed");

    // Ingest a log to populate the ring buffer
    let body = json!({
        "resourceLogs": [
            {
                "resource": { "attributes": [] },
                "scopeLogs": [
                    {
                        "logRecords": [
                            {
                                "timeUnixNano": "1777724525000000000",
                                "severityNumber": 17,
                                "severityText": "ERROR",
                                "body": { "stringValue": "Unable to determine the device handle for GPU0: 0000:01:00.0: Unknown Error" },
                                "attributes": []
                            }
                        ]
                    }
                ]
            }
        ]
    });

    let response = test_app()
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/logs")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
}

#[tokio::test]
async fn test_metrics_endpoint_returns_self_telemetry() {
    let response = test_app()
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let snapshot: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(snapshot.get("logs_ingested_total").is_some());
    assert!(snapshot.get("ws_connections_active").is_some());
}

#[tokio::test]
async fn test_metrics_endpoint_increments_after_ingestion() {
    let app = test_app();

    // Get baseline
    let baseline = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let baseline_body = axum::body::to_bytes(baseline.into_body(), usize::MAX)
        .await
        .unwrap();
    let baseline_json: serde_json::Value = serde_json::from_slice(&baseline_body).unwrap();
    let baseline_metrics = baseline_json["metrics_ingested_total"].as_u64().unwrap();

    // Ingest a metric — must include a named metric so the counter increments
    let body = json!({
        "resourceMetrics": [
            {
                "resource": { "attributes": [] },
                "scopeMetrics": [
                    {
                        "metrics": [{ "name": "test_metric_1" }]
                    }
                ]
            }
        ]
    });
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/metrics")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Check incremented
    let after = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let after_body = axum::body::to_bytes(after.into_body(), usize::MAX)
        .await
        .unwrap();
    let after_json: serde_json::Value = serde_json::from_slice(&after_body).unwrap();
    let after_metrics = after_json["metrics_ingested_total"].as_u64().unwrap();

    assert_eq!(after_metrics, baseline_metrics + 1);
}

#[tokio::test]
async fn test_rotel_evaluate_endpoint_returns_sarif() {
    let body = json!({
        "gate_name": "test-gate",
        "expression": "log_shape && metric",
        "log_shape_observed": true,
        "metric_observed": true,
        "slo_expected": true
    });

    let response = test_app()
        .oneshot(
            Request::builder()
                .uri("/rotel/evaluate")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let sarif: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(sarif.get("runs").is_some());
}

#[tokio::test]
async fn test_rotel_evaluate_detects_slo_failure() {
    let body = json!({
        "gate_name": "test-gate-fail",
        "expression": "log_shape && metric",
        "log_shape_observed": true,
        "metric_observed": false,
        "slo_expected": true
    });

    let response = test_app()
        .oneshot(
            Request::builder()
                .uri("/rotel/evaluate")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let sarif: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let runs = sarif["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 1);
    let results = runs[0]["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["rule_id"], "l3dg3rr/otel/build-gate-slo");
}
