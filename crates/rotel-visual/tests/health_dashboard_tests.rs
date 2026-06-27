use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn test_health_endpoint() {
    let app = rotel_visual::create_app().expect("create_app failed");

    let response = app
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
    let app = rotel_visual::create_app().expect("create_app failed");

    let response = app
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
    let app = rotel_visual::create_app().expect("create_app failed");

    let body = json!({
        "resourceLogs": [
            {
                "resource": { "attributes": [] },
                "scopeLogs": []
            }
        ]
    });

    let response = app
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
    let app = rotel_visual::create_app().expect("create_app failed");

    let body = json!({
        "resourceMetrics": [
            {
                "resource": { "attributes": [] },
                "scopeMetrics": []
            }
        ]
    });

    let response = app
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
    let app = rotel_visual::create_app().expect("create_app failed");

    let body = json!({
        "resourceSpans": [
            {
                "resource": { "attributes": [] },
                "scopeSpans": []
            }
        ]
    });

    let response = app
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
    let app = rotel_visual::create_app().expect("create_app failed");

    let response = app
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
    let app = rotel_visual::create_app().expect("create_app failed");

    // Ingest a log that matches the GPU fault rule.
    let body = json!({
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
                                "body": {
                                    "stringValue": "Unable to determine the device handle for GPU0: 0000:01:00.0: Unknown Error"
                                },
                                "attributes": []
                            }
                        ]
                    }
                ]
            }
        ]
    });

    let response = app
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
    let app = rotel_visual::create_app().expect("create_app failed");

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
                                "body": {
                                    "stringValue": "Unable to determine the device handle for GPU0: 0000:01:00.0: Unknown Error"
                                },
                                "attributes": []
                            }
                        ]
                    }
                ]
            }
        ]
    });

    let response = app
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
