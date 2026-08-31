//! Integration tests for `LlmClient` against a hand-rolled HTTP mock server
//! standing in for `b00t-server`'s OpenAI-compatible `/v1/chat/completions`
//! endpoint (l3dg3rr#212).
//!
//! Deliberately NOT using a mocking crate (wiremock/mockito/httpmock aren't
//! present anywhere in this workspace) — a real `std::net::TcpListener`
//! parsing a real HTTP/1.1 request off the wire is the thing least likely to
//! hide an actual wire-format mismatch between `LlmClient` and b00t-server:
//! the request path, the `Authorization: Bearer <key>` header, and the JSON
//! body shape are all asserted against bytes that actually went over a
//! socket, not against a mock library's internal call log.
//!
//! `LlmClient` uses `reqwest::blocking`, so the mock server runs
//! synchronously on its own thread and speaks raw HTTP/1.1 — no tokio
//! runtime needed here either.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

use ledgerr_llm::{LlmClient, LlmConfig, LlmError};
use rust_decimal::Decimal;
use serde_json::Value;

struct CapturedRequest {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: String,
}

/// Reads one HTTP/1.1 request off `stream`: request line, headers (lowercased
/// keys), and body sized by `Content-Length`. No chunked-encoding support —
/// `reqwest`'s blocking client with a small JSON body never sends chunked.
fn read_request(stream: &mut TcpStream) -> CapturedRequest {
    let mut raw = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        stream.read_exact(&mut byte).expect("read request headers");
        raw.push(byte[0]);
        if raw.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    let header_text = String::from_utf8_lossy(&raw).to_string();
    let mut lines = header_text.lines();
    let request_line = lines.next().expect("request line");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().expect("method").to_string();
    let path = parts.next().expect("path").to_string();

    let mut headers = HashMap::new();
    let mut content_length = 0usize;
    for line in lines {
        if line.is_empty() {
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            let k = k.trim().to_ascii_lowercase();
            let v = v.trim().to_string();
            if k == "content-length" {
                content_length = v.parse().unwrap_or(0);
            }
            headers.insert(k, v);
        }
    }

    let mut body_bytes = vec![0u8; content_length];
    if content_length > 0 {
        stream.read_exact(&mut body_bytes).expect("read request body");
    }
    let body = String::from_utf8(body_bytes).expect("utf8 body");

    CapturedRequest {
        method,
        path,
        headers,
        body,
    }
}

fn write_response(stream: &mut TcpStream, status: u16, body: &str) {
    let status_text = match status {
        200 => "OK",
        401 => "Unauthorized",
        500 => "Internal Server Error",
        _ => "Error",
    };
    let resp = format!(
        "HTTP/1.1 {status} {status_text}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(resp.as_bytes()).expect("write response");
    stream.flush().expect("flush response");
}

/// Starts a one-shot mock server on an OS-assigned local port. Accepts a
/// single connection, hands the captured request to `respond` and writes
/// back whatever HTTP status/body it returns. Returns the base URL
/// (`http://127.0.0.1:<port>`, no trailing slash / no `/v1`) and a
/// `JoinHandle` the test should join to propagate any server-side panic
/// (e.g. an assertion on the captured request) as a real test failure.
fn spawn_mock_server(
    respond: impl FnOnce(&CapturedRequest) -> (u16, String) + Send + 'static,
) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
    let addr = listener.local_addr().expect("local_addr");
    let base_url = format!("http://{addr}");

    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept connection");
        let req = read_request(&mut stream);
        let (status, body) = respond(&req);
        write_response(&mut stream, status, &body);
    });

    (base_url, handle)
}

fn config_for(base_url: &str, api_key: &str) -> LlmConfig {
    LlmConfig {
        api_key: api_key.to_string(),
        model: "phi-4-mini-reasoning".to_string(),
        base_url: Some(base_url.to_string()),
        temperature: 0.0,
    }
}

fn chat_completion_response(content_json: &str) -> String {
    serde_json::json!({
        "id": "chatcmpl-mock",
        "object": "chat.completion",
        "model": "phi-4-mini-reasoning",
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": content_json},
            "finish_reason": "stop"
        }]
    })
    .to_string()
}

#[test]
fn classify_transaction_sends_expected_wire_format_and_parses_the_response() {
    let classification_json = serde_json::json!({
        "category": "Meals",
        "sub_category": "Coffee",
        "confidence": 0.92,
        "reasoning": "Vendor name matches a known coffee shop.",
        "suggested_tags": ["#meals", "#coffee"]
    })
    .to_string();

    let (base_url, handle) = spawn_mock_server(move |req| {
        // ── Path: b00t-server mounts OpenAI-compatible routes at the root ──
        assert_eq!(req.method, "POST");
        assert_eq!(req.path, "/v1/chat/completions");

        // ── Auth: bearer_auth must carry the provisioned b00t-server key ──
        assert_eq!(
            req.headers.get("authorization").map(String::as_str),
            Some("Bearer b00t-sk-test-wire-format")
        );

        // ── Body: OpenAI chat-completions shape ──
        let body: Value = serde_json::from_str(&req.body).expect("request body is valid JSON");
        assert_eq!(body["model"], "phi-4-mini-reasoning");
        assert_eq!(body["temperature"], 0.0);
        let messages = body["messages"].as_array().expect("messages array");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["role"], "system");
        assert!(
            messages[0]["content"]
                .as_str()
                .unwrap()
                .contains("categorization assistant"),
            "system prompt should be the classify prompt"
        );
        assert_eq!(messages[1]["role"], "user");
        let user_text = messages[1]["content"][0]["text"].as_str().unwrap();
        assert!(user_text.contains("Coffee House"));
        assert!(user_text.contains("4.50"));

        (200, chat_completion_response(&classification_json))
    });

    let config = config_for(&base_url, "b00t-sk-test-wire-format");
    let client = LlmClient::new(config).expect("client construction");

    let result = client
        .classify_transaction("Coffee House", Decimal::new(450, 2))
        .expect("classify_transaction should succeed against the mock server");

    assert_eq!(result.category, "Meals");
    assert_eq!(result.sub_category.as_deref(), Some("Coffee"));
    assert_eq!(result.suggested_tags, vec!["#meals", "#coffee"]);

    handle.join().expect("mock server thread should not panic");
}

#[test]
fn extract_receipt_bytes_sends_vision_content_and_parses_the_response() {
    let receipt_json = serde_json::json!({
        "vendor_name": "Coffee House",
        "date": "2026-04-18",
        "total_amount": 12.50,
        "currency": "USD",
        "subtotal": 11.36,
        "tax_amount": 1.14,
        "line_items": [{"description": "Latte", "quantity": 1.0, "unit_price": 5.00, "amount": 5.00}],
        "suggested_category": "Meals",
        "suggested_tags": ["#receipt", "#meals"],
        "confidence": 0.95,
        "raw_text": null
    })
    .to_string();

    let (base_url, handle) = spawn_mock_server(move |req| {
        assert_eq!(req.path, "/v1/chat/completions");
        assert_eq!(
            req.headers.get("authorization").map(String::as_str),
            Some("Bearer b00t-sk-test-vision")
        );

        let body: Value = serde_json::from_str(&req.body).expect("request body is valid JSON");
        let messages = body["messages"].as_array().expect("messages array");
        let user_content = messages[1]["content"].as_array().expect("user content array");
        assert_eq!(user_content[0]["type"], "image_url");
        let url = user_content[0]["image_url"]["url"].as_str().unwrap();
        assert!(
            url.starts_with("data:image/png;base64,"),
            "image_url should carry a base64 data URI, got: {url}"
        );
        assert_eq!(user_content[1]["type"], "text");

        (200, chat_completion_response(&receipt_json))
    });

    let config = config_for(&base_url, "b00t-sk-test-vision");
    let client = LlmClient::new(config).expect("client construction");

    // Minimal valid 1x1 PNG.
    let png_bytes: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
        0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8,
        0xCF, 0xC0, 0x00, 0x00, 0x00, 0x03, 0x00, 0x01, 0x18, 0xDD, 0x8D, 0xB0, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    let result = client
        .extract_receipt_bytes(png_bytes, "image/png")
        .expect("extract_receipt_bytes should succeed against the mock server");

    assert_eq!(result.vendor_name.as_deref(), Some("Coffee House"));
    assert_eq!(result.line_items.len(), 1);

    handle.join().expect("mock server thread should not panic");
}

#[test]
fn api_error_status_surfaces_as_llm_error_api_error_with_body() {
    let (base_url, handle) = spawn_mock_server(|_req| {
        (
            401,
            serde_json::json!({"error": {"message": "invalid b00t-server key"}}).to_string(),
        )
    });

    let config = config_for(&base_url, "b00t-sk-invalid");
    let client = LlmClient::new(config).expect("client construction");

    let err = client
        .classify_transaction("Some vendor", Decimal::new(100, 2))
        .expect_err("a 401 response must surface as an error, never a silent default");

    match err {
        LlmError::ApiError { status, message } => {
            assert_eq!(status, 401);
            assert!(message.contains("invalid b00t-server key"));
        }
        other => panic!("expected LlmError::ApiError, got: {other:?}"),
    }

    handle.join().expect("mock server thread should not panic");
}
