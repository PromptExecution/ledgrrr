//! Internal remote-pilot interface for `--timeout`/test-debug mode.
//!
//! Speaks the same JSON-RPC `tools/list` / `tools/call` shape as
//! `ledgerr-mcp-server.rs` (POST the whole request body to `/`, same field
//! names) so the same tooling an LLM already uses to drive `ledgerr-mcp`
//! can drive this app too — evaluate JS / screenshot / switch panels via
//! the WebView2 CDP endpoint (`TAURI_CDP_PORT`), read back what's been sent
//! so far, and check the remaining `--timeout` countdown. Only starts when
//! `--timeout <seconds>` is passed on the command line; a normal end-user
//! launch never opens this port.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{json, Value};

pub const REMOTE_PILOT_ADDR: &str = "127.0.0.1:15117";
const PANEL_IDS: [&str; 6] = ["chat", "logs", "dash", "settings", "docs", "viz"];

#[derive(Debug, Clone, Serialize)]
pub struct RemoteCommandLogEntry {
    pub tool: String,
    pub arguments: Value,
    pub result: Value,
    pub elapsed_ms: u128,
}

#[derive(Default)]
pub struct RemotePilotState {
    pub command_log: Mutex<Vec<RemoteCommandLogEntry>>,
    pub started_at: Mutex<Option<Instant>>,
    pub timeout: Mutex<Option<Duration>>,
}

impl RemotePilotState {
    pub fn arm_timeout(&self, timeout: Duration) {
        *self.started_at.lock().unwrap() = Some(Instant::now());
        *self.timeout.lock().unwrap() = Some(timeout);
    }

    pub fn remaining(&self) -> Option<Duration> {
        let started = (*self.started_at.lock().unwrap())?;
        let timeout = (*self.timeout.lock().unwrap())?;
        Some(timeout.saturating_sub(started.elapsed()))
    }
}

/// Starts the remote-pilot HTTP server on a background thread. Safe to call
/// multiple times only in the sense that a second call will fail to bind —
/// callers should only invoke this once, guarded by `--timeout` being set.
pub fn spawn(state: Arc<RemotePilotState>) -> std::io::Result<()> {
    let listener = TcpListener::bind(REMOTE_PILOT_ADDR)?;
    listener.set_nonblocking(true)?;
    thread::Builder::new()
        .name("ledgerr-remote-pilot".to_string())
        .spawn(move || serve_loop(listener, state))?;
    Ok(())
}

fn serve_loop(listener: TcpListener, state: Arc<RemotePilotState>) {
    loop {
        match listener.accept() {
            Ok((stream, _)) => handle_stream(stream, &state),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(25));
            }
            Err(_) => break,
        }
    }
}

fn handle_stream(mut stream: TcpStream, state: &Arc<RemotePilotState>) {
    let mut buffer = Vec::with_capacity(8192);
    let mut chunk = [0_u8; 4096];
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buffer.extend_from_slice(&chunk[..n]);
                if let Some(header_end) = find_header_end(&buffer) {
                    let headers = String::from_utf8_lossy(&buffer[..header_end]);
                    let content_length = parse_content_length(&headers).unwrap_or(0);
                    if buffer.len() >= header_end + 4 + content_length {
                        break;
                    }
                }
            }
            Err(_) => break,
        }
    }
    let response = route(&buffer, state);
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn parse_content_length(headers: &str) -> Option<usize> {
    headers.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("content-length")
            .then(|| value.trim().parse().ok())
            .flatten()
    })
}

fn json_response(status: u16, body: &Value) -> String {
    let body = serde_json::to_string(body).unwrap_or_else(|_| "{}".to_string());
    let reason = if status == 200 { "OK" } else { "Bad Request" };
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
}

fn route(raw: &[u8], state: &Arc<RemotePilotState>) -> String {
    let Some(header_end) = find_header_end(raw) else {
        return json_response(400, &json!({"error": "invalid request"}));
    };
    let body = &raw[header_end + 4..];
    let Ok(request) = serde_json::from_slice::<Value>(body) else {
        let _ = std::fs::write(std::env::temp_dir().join("host-tauri-remote-pilot-badrequest.txt"), raw);
        return json_response(400, &json!({"error": "invalid json body"}));
    };
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    match method {
        "tools/list" => json_response(200, &json!({"tools": tool_descriptors()})),
        "tools/call" => {
            let params = request.get("params").cloned().unwrap_or(Value::Null);
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            let start = Instant::now();
            let result = dispatch(&name, &arguments, state);
            let elapsed_ms = start.elapsed().as_millis();
            state.command_log.lock().unwrap().push(RemoteCommandLogEntry {
                tool: name,
                arguments: arguments.clone(),
                result: result.clone(),
                elapsed_ms,
            });
            json_response(200, &json!({"content": result}))
        }
        other => json_response(400, &json!({"error": format!("unknown method: {other}")})),
    }
}

fn tool_descriptors() -> Value {
    json!([
        {
            "name": "remote_evaluate_js",
            "description": "Evaluate a JS expression in the app's webview via CDP Runtime.evaluate",
            "inputSchema": {"type": "object", "properties": {"expression": {"type": "string"}}, "required": ["expression"]}
        },
        {
            "name": "remote_screenshot",
            "description": "Capture a PNG screenshot of the app's webview via CDP Page.captureScreenshot and write it to a local path",
            "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}
        },
        {
            "name": "remote_show_panel",
            "description": "Switch the sidebar to the named panel",
            "inputSchema": {"type": "object", "properties": {"panel": {"type": "string", "enum": PANEL_IDS}}, "required": ["panel"]}
        },
        {
            "name": "remote_get_logs",
            "description": "Return every remote-pilot command received this session",
            "inputSchema": {"type": "object", "properties": {}}
        },
        {
            "name": "remote_remaining_timeout",
            "description": "Seconds left before --timeout auto-exit, or null if --timeout wasn't set",
            "inputSchema": {"type": "object", "properties": {}}
        },
    ])
}

fn dispatch(name: &str, args: &Value, state: &Arc<RemotePilotState>) -> Value {
    match name {
        "remote_evaluate_js" => {
            let expression = args.get("expression").and_then(Value::as_str).unwrap_or_default();
            match cdp_evaluate(expression) {
                Ok(value) => json!({"ok": true, "value": value}),
                Err(error) => json!({"ok": false, "error": error}),
            }
        }
        "remote_screenshot" => {
            let path = args.get("path").and_then(Value::as_str).unwrap_or("screenshot.png");
            match cdp_screenshot(path) {
                Ok(bytes) => json!({"ok": true, "path": path, "bytes": bytes}),
                Err(error) => json!({"ok": false, "error": error}),
            }
        }
        "remote_show_panel" => {
            let panel = args.get("panel").and_then(Value::as_str).unwrap_or_default();
            match PANEL_IDS.iter().position(|p| *p == panel) {
                Some(index) => {
                    let expression = format!(
                        "document.querySelectorAll('.nav-item[data-panel-index]')[{index}].click(); 'ok'"
                    );
                    match cdp_evaluate(&expression) {
                        Ok(value) => json!({"ok": true, "value": value}),
                        Err(error) => json!({"ok": false, "error": error}),
                    }
                }
                None => json!({"ok": false, "error": format!("unknown panel: {panel}; valid: {PANEL_IDS:?}")}),
            }
        }
        "remote_get_logs" => {
            let log = state.command_log.lock().unwrap();
            json!({"ok": true, "commands": &*log})
        }
        "remote_remaining_timeout" => {
            json!({"ok": true, "seconds_remaining": state.remaining().map(|d| d.as_secs())})
        }
        other => json!({"ok": false, "error": format!("unknown tool: {other}")}),
    }
}

fn cdp_port() -> Result<u16, String> {
    std::env::var("TAURI_CDP_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|p| *p > 0)
        .ok_or_else(|| {
            "TAURI_CDP_PORT not set — launch with \
             WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=<port> and \
             TAURI_CDP_PORT=<port> to enable CDP-backed remote-pilot tools"
                .to_string()
        })
}

fn cdp_first_target_ws_url(port: u16) -> Result<String, String> {
    let url = format!("http://127.0.0.1:{port}/json");
    let response = reqwest::blocking::get(&url).map_err(|e| e.to_string())?;
    let targets: Vec<Value> = response.json().map_err(|e| e.to_string())?;
    targets
        .into_iter()
        .find_map(|t| {
            t.get("webSocketDebuggerUrl")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .ok_or_else(|| "no CDP targets found — is the webview still open?".to_string())
}

fn cdp_evaluate(expression: &str) -> Result<Value, String> {
    let port = cdp_port()?;
    let ws_url = cdp_first_target_ws_url(port)?;
    let expression = expression.to_string();
    let runtime = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    runtime.block_on(cdp_roundtrip(
        ws_url,
        json!({
            "id": 1,
            "method": "Runtime.evaluate",
            "params": {"expression": expression, "returnByValue": true, "awaitPromise": true}
        }),
    ))
}

fn cdp_screenshot(path: &str) -> Result<u64, String> {
    let port = cdp_port()?;
    let ws_url = cdp_first_target_ws_url(port)?;
    let path = path.to_string();
    let runtime = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    let result = runtime.block_on(cdp_roundtrip(
        ws_url,
        json!({"id": 2, "method": "Page.captureScreenshot", "params": {"format": "png"}}),
    ))?;
    let data_b64 = result
        .get("data")
        .and_then(Value::as_str)
        .ok_or("no screenshot data in CDP response")?;
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_b64)
        .map_err(|e| e.to_string())?;
    std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    Ok(bytes.len() as u64)
}

/// One request/response round trip over a fresh CDP websocket connection.
/// Returns the `result` object of the reply matching the request's `id`.
async fn cdp_roundtrip(ws_url: String, request: Value) -> Result<Value, String> {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let want_id = request.get("id").and_then(Value::as_i64);
    let (mut ws, _) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .map_err(|e| e.to_string())?;
    ws.send(Message::Text(request.to_string().into()))
        .await
        .map_err(|e| e.to_string())?;
    loop {
        let msg = ws
            .next()
            .await
            .ok_or("CDP connection closed before response")?
            .map_err(|e| e.to_string())?;
        let Message::Text(text) = msg else { continue };
        let value: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        if value.get("id").and_then(Value::as_i64) == want_id {
            if let Some(error) = value.get("error") {
                return Err(error.to_string());
            }
            return Ok(value.get("result").cloned().unwrap_or(Value::Null));
        }
    }
}

/// Writes the full session dump (chat history + review log, as Debug text —
/// neither type implements Serialize — plus every remote-pilot command
/// received) to a timestamped file under the OS temp dir and returns its
/// path. Called when `--timeout` fires.
pub fn dump_session_log(state: &RemotePilotState, history_debug: &str, review_log_debug: &str) -> PathBuf {
    let pid = std::process::id();
    let path = std::env::temp_dir().join(format!("host-tauri-session-dump-{pid}.json"));
    let commands = state.command_log.lock().unwrap();
    let dump = json!({
        "pid": pid,
        "chat_history_debug": history_debug,
        "review_log_debug": review_log_debug,
        "remote_commands": &*commands,
    });
    let _ = std::fs::write(&path, serde_json::to_string_pretty(&dump).unwrap_or_default());
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_header_end_locates_terminator() {
        assert_eq!(find_header_end(b"POST / HTTP/1.1\r\n\r\n{}"), Some(16));
        assert_eq!(find_header_end(b"no terminator here"), None);
    }

    #[test]
    fn parse_content_length_is_case_insensitive_and_tolerant() {
        assert_eq!(
            parse_content_length("POST / HTTP/1.1\r\nContent-Length: 42\r\n"),
            Some(42)
        );
        assert_eq!(
            parse_content_length("POST / HTTP/1.1\r\ncontent-length:  7\r\n"),
            Some(7)
        );
        assert_eq!(parse_content_length("POST / HTTP/1.1\r\n"), None);
        assert_eq!(
            parse_content_length("POST / HTTP/1.1\r\nContent-Length: not-a-number\r\n"),
            None
        );
    }

    #[test]
    fn json_response_sets_status_and_content_length() {
        let body = json!({"ok": true});
        let response = json_response(200, &body);
        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
        let expected_len = serde_json::to_string(&body).unwrap().len();
        assert!(response.contains(&format!("Content-Length: {expected_len}\r\n")));
        assert!(response.ends_with(&serde_json::to_string(&body).unwrap()));

        let error_response = json_response(400, &json!({"error": "bad"}));
        assert!(error_response.starts_with("HTTP/1.1 400 Bad Request\r\n"));
    }

    #[test]
    fn remote_pilot_state_remaining_before_and_after_arming() {
        let state = RemotePilotState::default();
        assert_eq!(state.remaining(), None, "unarmed state has no countdown");

        state.arm_timeout(Duration::from_secs(60));
        let remaining = state.remaining().expect("armed state has a countdown");
        assert!(
            remaining <= Duration::from_secs(60) && remaining > Duration::from_secs(55),
            "expected ~60s remaining immediately after arming, got {remaining:?}"
        );
    }

    #[test]
    fn tool_descriptors_lists_all_five_tools() {
        let tools = tool_descriptors();
        let names: Vec<&str> = tools
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            vec![
                "remote_evaluate_js",
                "remote_screenshot",
                "remote_show_panel",
                "remote_get_logs",
                "remote_remaining_timeout",
            ]
        );
    }

    fn http_request(body: &str) -> Vec<u8> {
        format!(
            "POST / HTTP/1.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        )
        .into_bytes()
    }

    #[test]
    fn route_tools_list_returns_all_tools() {
        let state = Arc::new(RemotePilotState::default());
        let raw = http_request(r#"{"method":"tools/list"}"#);
        let response = route(&raw, &state);
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("remote_screenshot"));
        assert!(response.contains("remote_remaining_timeout"));
    }

    #[test]
    fn route_rejects_missing_header_terminator() {
        let state = Arc::new(RemotePilotState::default());
        let response = route(b"not even a real http request", &state);
        assert!(response.starts_with("HTTP/1.1 400"));
        assert!(response.contains("invalid request"));
    }

    #[test]
    fn route_rejects_invalid_json_body() {
        let state = Arc::new(RemotePilotState::default());
        let raw = http_request("not json");
        let response = route(&raw, &state);
        assert!(response.starts_with("HTTP/1.1 400"));
        assert!(response.contains("invalid json body"));
    }

    #[test]
    fn route_rejects_unknown_method() {
        let state = Arc::new(RemotePilotState::default());
        let raw = http_request(r#"{"method":"tools/frobnicate"}"#);
        let response = route(&raw, &state);
        assert!(response.starts_with("HTTP/1.1 400"));
        assert!(response.contains("unknown method"));
    }

    #[test]
    fn dispatch_unknown_tool_reports_ok_false() {
        let state = Arc::new(RemotePilotState::default());
        let result = dispatch("not_a_real_tool", &json!({}), &state);
        assert_eq!(result["ok"], json!(false));
        assert!(result["error"].as_str().unwrap().contains("not_a_real_tool"));
    }

    #[test]
    fn dispatch_remote_show_panel_rejects_unknown_panel() {
        let state = Arc::new(RemotePilotState::default());
        let result = dispatch("remote_show_panel", &json!({"panel": "nonexistent"}), &state);
        assert_eq!(result["ok"], json!(false));
        assert!(result["error"].as_str().unwrap().contains("unknown panel"));
    }

    #[test]
    fn dispatch_remote_remaining_timeout_reflects_state() {
        let state = Arc::new(RemotePilotState::default());
        assert_eq!(
            dispatch("remote_remaining_timeout", &json!({}), &state)["seconds_remaining"],
            json!(null)
        );
        state.arm_timeout(Duration::from_secs(30));
        let seconds_remaining = dispatch("remote_remaining_timeout", &json!({}), &state)
            ["seconds_remaining"]
            .as_u64()
            .unwrap();
        assert!(seconds_remaining <= 30);
    }

    #[test]
    fn dispatch_remote_get_logs_reflects_prior_calls_via_route() {
        // Route (not dispatch directly) so the command actually gets logged —
        // logging happens in route()'s tools/call branch, not in dispatch().
        let state = Arc::new(RemotePilotState::default());
        let _ = route(&http_request(r#"{"method":"tools/list"}"#), &state);
        let _ = route(
            &http_request(r#"{"method":"tools/call","params":{"name":"remote_remaining_timeout","arguments":{}}}"#),
            &state,
        );
        let logs_response = route(
            &http_request(r#"{"method":"tools/call","params":{"name":"remote_get_logs","arguments":{}}}"#),
            &state,
        );
        assert!(logs_response.contains("remote_remaining_timeout"));
        assert_eq!(state.command_log.lock().unwrap().len(), 2);
    }

    #[test]
    fn cdp_port_reports_a_clear_error_when_unset() {
        std::env::remove_var("TAURI_CDP_PORT");
        let error = cdp_port().expect_err("TAURI_CDP_PORT should be unset in this test process");
        assert!(error.contains("TAURI_CDP_PORT"));
    }

    #[test]
    fn dump_session_log_writes_expected_fields() {
        let state = RemotePilotState::default();
        state.command_log.lock().unwrap().push(RemoteCommandLogEntry {
            tool: "remote_get_logs".to_string(),
            arguments: json!({}),
            result: json!({"ok": true}),
            elapsed_ms: 5,
        });
        let path = dump_session_log(&state, "[]", "ReviewLog { entries: [] }");
        let contents = std::fs::read_to_string(&path).expect("dump file should exist");
        let _ = std::fs::remove_file(&path);
        let parsed: Value = serde_json::from_str(&contents).unwrap();
        assert_eq!(parsed["chat_history_debug"], json!("[]"));
        assert_eq!(parsed["remote_commands"][0]["tool"], json!("remote_get_logs"));
    }
}
