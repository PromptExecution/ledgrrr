/// Axum-based Streamable HTTP + SSE transport for the MCP server (issue #227).
///
/// Exposes `build_http_app` so the server binary and integration tests share
/// exactly the same router code.  The binary injects its own `handle_request`
/// function as the dispatch callback; tests inject a lightweight stub.
///
/// Routes:
/// - `GET  /health` — liveness probe (no auth, no state)
/// - `POST /`       — JSON-RPC dispatch; response is `application/json`
/// - `GET  /sse`    — SSE stream; receives `notifications/tools/list_changed`
///                    pushed when a POST / response carries the `_list_changed`
///                    sentinel (issue #226/#227)
use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Response, StatusCode},
    response::{
        IntoResponse,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use serde_json::{Value, json};
use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

/// Build the axum Router for the HTTP transport.
///
/// `dispatch` is the synchronous JSON-RPC handler; it is called via
/// `spawn_blocking` so it never blocks the tokio runtime.  Parameters:
/// `(request_body: Value, agent_id: Option<String>) -> Option<Value>`.
///
/// `notify_tx` is the broadcast sender for SSE events.  Pass a fresh
/// `broadcast::channel(64).0` from the server binary; tests create their own.
pub fn build_http_app<F>(
    dispatch: F,
    notify_tx: broadcast::Sender<String>,
) -> Router
where
    F: Fn(Value, Option<String>) -> Option<Value> + Send + Sync + Clone + 'static,
{
    let dispatch = Arc::new(dispatch);

    Router::new()
        .route("/health", get(|| async { (StatusCode::OK, "ok") }))
        .route(
            "/sse",
            get({
                let tx = notify_tx.clone();
                move || {
                    let rx = tx.subscribe();
                    let stream = BroadcastStream::new(rx).filter_map(|result| {
                        result.ok().map(|data| {
                            Ok::<Event, std::convert::Infallible>(Event::default().data(data))
                        })
                    });
                    async move { Sse::new(stream).keep_alive(KeepAlive::default()) }
                }
            }),
        )
        .route(
            "/",
            post({
                let tx = notify_tx.clone();
                move |headers: HeaderMap, body: String| {
                    let dispatch = Arc::clone(&dispatch);
                    let tx = tx.clone();
                    async move {
                        let agent_id = agent_id_from_headers(&headers);
                        let parsed = match serde_json::from_str::<Value>(&body) {
                            Ok(v) => v,
                            Err(e) => {
                                return Response::builder()
                                    .status(StatusCode::BAD_REQUEST)
                                    .body(Body::from(format!("invalid JSON: {e}")))
                                    .unwrap()
                                    .into_response();
                            }
                        };

                        // Run the synchronous dispatcher off the tokio thread pool.
                        let result = tokio::task::spawn_blocking(move || {
                            dispatch(parsed, agent_id)
                        })
                        .await
                        .unwrap_or(None);

                        match result {
                            Some(mut resp) => {
                                // Strip `_list_changed` sentinel and push an SSE notification.
                                if let Some(obj) = resp.as_object_mut() {
                                    if obj.remove("_list_changed").is_some() {
                                        let notif = serde_json::to_string(&json!({
                                            "jsonrpc": "2.0",
                                            "method": "notifications/tools/list_changed",
                                            "params": {}
                                        }))
                                        .unwrap_or_default();
                                        let _ = tx.send(notif);
                                    }
                                }

                                let body_str = serde_json::to_string(&resp).unwrap_or_default();
                                Response::builder()
                                    .status(StatusCode::OK)
                                    .header("content-type", "application/json")
                                    .body(Body::from(body_str))
                                    .unwrap()
                                    .into_response()
                            }
                            None => Response::builder()
                                .status(StatusCode::NO_CONTENT)
                                .body(Body::empty())
                                .unwrap()
                                .into_response(),
                        }
                    }
                }
            }),
        )
}

/// Extract calling agent identity from `X-Agent-Id` HTTP header (issue #224).
/// Case-insensitive; absent → `None` (fail-open default).
pub fn agent_id_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-agent-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    /// Minimal stub handler for transport-layer tests.
    fn stub_dispatch(req: Value, _agent_id: Option<String>) -> Option<Value> {
        let method = req.get("method").and_then(Value::as_str).unwrap_or("");
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        match method {
            "tools/list" => Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "tools": [] }
            })),
            "notifications/initialized" => None,
            // Simulate a tool call that triggers _list_changed.
            "tools/call" => Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "_list_changed": true,
                "result": { "content": [{"type": "text", "text": "ok"}] }
            })),
            other => Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": format!("method not found: {other}") }
            })),
        }
    }

    fn test_app() -> (Router, broadcast::Sender<String>) {
        let (tx, _) = broadcast::channel::<String>(16);
        let app = build_http_app(stub_dispatch, tx.clone());
        (app, tx)
    }

    #[tokio::test]
    async fn health_returns_200() {
        let (app, _) = test_app();
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
    async fn post_tools_list_returns_json() {
        let (app, _) = test_app();
        let body = serde_json::to_string(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/list"
        }))
        .unwrap();
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let ct = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(ct.contains("application/json"), "content-type was: {ct}");
    }

    #[tokio::test]
    async fn notification_returns_204() {
        let (app, _) = test_app();
        let body = serde_json::to_string(&json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }))
        .unwrap();
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn sse_endpoint_returns_event_stream_content_type() {
        let (app, _) = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/sse")
                    .header("accept", "text/event-stream")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let ct = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(
            ct.contains("text/event-stream"),
            "expected text/event-stream, got: {ct}"
        );
    }

    #[tokio::test]
    async fn post_with_list_changed_pushes_sse_event() {
        let (app, tx) = test_app();
        // Subscribe to SSE events BEFORE the POST arrives.
        let mut rx = tx.subscribe();

        let body = serde_json::to_string(&json!({
            "jsonrpc": "2.0",
            "id": 42,
            "method": "tools/call",
            "params": { "name": "ledgerr_scope", "arguments": { "action": "request_increase" } }
        }))
        .unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Response should be OK (not 204 — stub_dispatch returns Some for tools/call).
        assert_eq!(response.status(), StatusCode::OK);

        // The stub injects `_list_changed: true` in the response; the
        // transport layer must have stripped it and pushed an SSE notification.
        let notif_str = tokio::time::timeout(
            std::time::Duration::from_millis(200),
            async { rx.recv().await.expect("notification must have been sent") },
        )
        .await
        .expect("SSE notification must arrive within 200ms");

        let notif: Value = serde_json::from_str(&notif_str).unwrap();
        assert_eq!(
            notif["method"].as_str(),
            Some("notifications/tools/list_changed"),
            "unexpected notification: {notif}"
        );

        // The HTTP response body must NOT contain _list_changed (sentinel stripped).
        let resp_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let resp_json: Value = serde_json::from_slice(&resp_bytes).unwrap();
        assert!(
            resp_json.get("_list_changed").is_none(),
            "_list_changed must be stripped from the response body"
        );
    }

    #[tokio::test]
    async fn invalid_json_returns_400() {
        let (app, _) = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/")
                    .body(Body::from("not json"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn unknown_path_returns_404() {
        let (app, _) = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/not-a-route")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
