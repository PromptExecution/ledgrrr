//! Covers per-request caller identity plumbing (issue #224:
//! `LEDGERR_MCP_AGENT_ID` for stdio, `X-Agent-Id` for HTTP) and its wiring
//! into the `tools/call` governance gate (issue #225:
//! `LedgrrAgtGateway::check_tool_call`).
//!
//! Matches the layering established by `tool_visibility_ring_filter.rs`:
//! - stdio E2E: spawns the real `ledgerr-mcp-server` binary over stdio.
//! - HTTP E2E (`http-transport` feature only): spawns the real binary with
//!   `LEDGERR_MCP_TRANSPORT=http` and talks raw HTTP/1.1 over a `TcpStream`
//!   (no HTTP client crate is available as a non-optional dev-dependency;
//!   PR #223 added no HTTP-transport tests of its own to extend).
//!
//! Both transports are checked for the same three behaviors:
//! 1. No identity configured → `tools/call` is completely unenforced, byte-
//!    for-byte the pre-#224 default (a `ledgerr_reconciliation.commit` call
//!    reaches the real handler and fails on missing arguments, not on
//!    governance).
//! 2. An identity is configured and the requested action is permitted (a
//!    freshly-seen agent is auto-registered at `Ring::Standard`,  which
//!    allows `ledgerr_documents.list_accounts`) → call proceeds normally.
//! 3. An identity is configured and the action requires approval
//!    (`ledgerr_reconciliation.commit`, per `LEDGERR_POLICY_YAML`'s
//!    `ledgerr_reconciliation.commit*` approval rule) → denied with
//!    `error_type: "GovernanceDenied"`, never reaching the real handler.
//!
//! A tool family outside `AGT_GATED_TOOL_FAMILIES` (`ledgerr_budget`) stays
//! ungated even with an identity configured, matching the design doc's
//! "core group" precedent from PR #232 (`ledgerr_schema`/`ledgerr_manifest`).

mod common;

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{json, Value};

// --- stdio E2E --------------------------------------------------------

struct McpStdioClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl McpStdioClient {
    fn spawn(label: &str, agent_id: Option<&str>) -> Self {
        let server_bin = env!("CARGO_BIN_EXE_ledgerr-mcp-server");
        let mut cmd = Command::new(server_bin);
        cmd.env("LEDGERR_MCP_MANIFEST", common::stdio_test_manifest(label))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(id) = agent_id {
            cmd.env("LEDGERR_MCP_AGENT_ID", id);
        } else {
            cmd.env_remove("LEDGERR_MCP_AGENT_ID");
        }
        let mut child = cmd.spawn().expect("spawn ledgerr-mcp-server");
        let stdin = child.stdin.take().expect("server stdin");
        let stdout = BufReader::new(child.stdout.take().expect("server stdout"));
        Self {
            child,
            stdin,
            stdout,
            next_id: 1,
        }
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let payload = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let line = serde_json::to_string(&payload).expect("serialize request");
        writeln!(self.stdin, "{line}").expect("write request");
        self.stdin.flush().expect("flush request");

        let mut response = String::new();
        self.stdout
            .read_line(&mut response)
            .expect("read response line");
        serde_json::from_str::<Value>(response.trim()).expect("parse response json")
    }

    fn send_notification_initialized(&mut self) {
        let payload = json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {},
        });
        let line = serde_json::to_string(&payload).expect("serialize notification");
        writeln!(self.stdin, "{line}").expect("write notification");
        self.stdin.flush().expect("flush notification");
    }
}

impl Drop for McpStdioClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn initialize_stdio(client: &mut McpStdioClient) {
    let initialize = client.request(
        "initialize",
        json!({
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": { "name": "mcp-caller-identity-e2e", "version": "0.1.0" }
        }),
    );
    assert!(
        initialize.get("result").is_some(),
        "initialize must succeed"
    );
    client.send_notification_initialized();
}

fn call_tool(client: &mut McpStdioClient, name: &str, arguments: Value) -> Value {
    client.request(
        "tools/call",
        json!({ "name": name, "arguments": arguments }),
    )
}

fn is_error_result(response: &Value) -> bool {
    response["result"]["isError"].as_bool().unwrap_or(false)
}

fn error_type(response: &Value) -> Option<String> {
    response["result"]["content"][0]["text"]
        .as_str()
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .and_then(|payload| payload["error_type"].as_str().map(String::from))
}

#[test]
fn stdio_default_no_identity_leaves_tools_call_unenforced() {
    let mut client = McpStdioClient::spawn("caller-identity-default", None);
    initialize_stdio(&mut client);

    // Commit requires approval under the shared policy YAML, but with no
    // identity configured the governance gate must never run — the call
    // reaches the real handler and fails on missing required arguments
    // (InvalidInput), not on governance.
    let response = call_tool(
        &mut client,
        "ledgerr_reconciliation",
        json!({ "action": "commit" }),
    );
    assert!(
        is_error_result(&response),
        "missing args must still error: {response:?}"
    );
    assert_ne!(
        error_type(&response).as_deref(),
        Some("GovernanceDenied"),
        "no LEDGERR_MCP_AGENT_ID configured — governance must not run: {response:?}"
    );
}

#[test]
fn stdio_agent_id_allows_permitted_action() {
    let mut client = McpStdioClient::spawn("caller-identity-allow", Some("stdio-agent-allow"));
    initialize_stdio(&mut client);

    let response = call_tool(
        &mut client,
        "ledgerr_documents",
        json!({ "action": "list_accounts" }),
    );
    assert!(
        !is_error_result(&response),
        "a freshly-registered (Standard ring) agent must be allowed to list_accounts: {response:?}"
    );
}

#[test]
fn stdio_agent_id_denies_action_requiring_approval() {
    let mut client = McpStdioClient::spawn("caller-identity-deny", Some("stdio-agent-deny"));
    initialize_stdio(&mut client);

    let response = call_tool(
        &mut client,
        "ledgerr_reconciliation",
        json!({ "action": "commit" }),
    );
    assert!(
        is_error_result(&response),
        "commit must be denied: {response:?}"
    );
    assert_eq!(
        error_type(&response).as_deref(),
        Some("GovernanceDenied"),
        "must be denied by the governance gate specifically, before reaching the real handler: {response:?}"
    );
}

#[test]
fn stdio_agent_id_does_not_gate_non_agt_tool_family() {
    let mut client = McpStdioClient::spawn("caller-identity-core", Some("stdio-agent-core"));
    initialize_stdio(&mut client);

    // ledgerr_budget is outside AGT_GATED_TOOL_FAMILIES — it has no
    // action-pattern mapping in rings.rs/policy.rs at all (same as
    // ledgerr_schema/ledgerr_manifest, PR #232's "core group" precedent) —
    // so an identity being configured must not start gating it. Omitting
    // the required `action` tag makes the real handler fail with
    // InvalidInput; the point is that it's never reached via the
    // governance path (GovernanceDenied), proving the gate was skipped.
    let response = call_tool(&mut client, "ledgerr_budget", json!({}));
    assert_ne!(
        error_type(&response).as_deref(),
        Some("GovernanceDenied"),
        "ledgerr_budget is not AGT-gated — governance must not run: {response:?}"
    );
}

// --- HTTP E2E (http-transport feature only) ----------------------------

#[cfg(feature = "http-transport")]
mod http_e2e {
    use super::*;
    use std::io::Read;
    use std::sync::atomic::{AtomicU16, Ordering};

    static NEXT_PORT: AtomicU16 = AtomicU16::new(18100);

    fn alloc_port() -> u16 {
        NEXT_PORT.fetch_add(1, Ordering::Relaxed)
    }

    struct HttpServer {
        child: Child,
        port: u16,
    }

    impl HttpServer {
        fn spawn(label: &str) -> Self {
            let port = alloc_port();
            let server_bin = env!("CARGO_BIN_EXE_ledgerr-mcp-server");
            let child = Command::new(server_bin)
                .env("LEDGERR_MCP_MANIFEST", common::stdio_test_manifest(label))
                .env("LEDGERR_MCP_TRANSPORT", "http")
                .env("PORT", port.to_string())
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("spawn ledgerr-mcp-server (http)");
            let server = Self { child, port };
            server.wait_ready();
            server
        }

        fn wait_ready(&self) {
            for _ in 0..200 {
                if raw_http_request(self.port, "GET", "/health", &[], None).is_some() {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            panic!(
                "ledgerr-mcp-server (http) did not become ready on port {}",
                self.port
            );
        }

        fn call_tool(&self, name: &str, arguments: Value, agent_id: Option<&str>) -> Value {
            let payload = json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": { "name": name, "arguments": arguments },
            });
            let body = serde_json::to_vec(&payload).expect("serialize request");
            let headers: Vec<(String, String)> = agent_id
                .map(|id| vec![("X-Agent-Id".to_string(), id.to_string())])
                .unwrap_or_default();
            let (status, resp_body) =
                raw_http_request(self.port, "POST", "/", &headers, Some(&body))
                    .expect("http response");
            assert_eq!(status, 200, "tools/call must return HTTP 200: {resp_body}");
            serde_json::from_str(&resp_body).expect("parse JSON-RPC response body")
        }
    }

    impl Drop for HttpServer {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    /// Minimal synchronous HTTP/1.1 client over a raw `TcpStream` — no HTTP
    /// client crate is a non-optional dev-dependency of this crate. Returns
    /// `None` on connection failure (used for the readiness poll), else
    /// `Some((status_code, body))`.
    fn raw_http_request(
        port: u16,
        method: &str,
        path: &str,
        headers: &[(String, String)],
        body: Option<&[u8]>,
    ) -> Option<(u16, String)> {
        use std::net::TcpStream;

        let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
        let mut req =
            format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n");
        for (name, value) in headers {
            req.push_str(&format!("{name}: {value}\r\n"));
        }
        if let Some(b) = body {
            req.push_str(&format!(
                "Content-Type: application/json\r\nContent-Length: {}\r\n",
                b.len()
            ));
        }
        req.push_str("\r\n");

        stream.write_all(req.as_bytes()).ok()?;
        if let Some(b) = body {
            stream.write_all(b).ok()?;
        }

        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).ok()?;
        let raw = String::from_utf8_lossy(&raw).into_owned();
        let mut parts = raw.splitn(2, "\r\n\r\n");
        let head = parts.next().unwrap_or("");
        let resp_body = parts.next().unwrap_or("").to_string();
        let status = head
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|code| code.parse::<u16>().ok())?;
        Some((status, resp_body))
    }

    #[test]
    fn http_default_no_header_leaves_tools_call_unenforced() {
        let server = HttpServer::spawn("caller-identity-http-default");
        let response = server.call_tool(
            "ledgerr_reconciliation",
            json!({ "action": "commit" }),
            None,
        );
        assert!(
            is_error_result(&response),
            "missing args must still error: {response:?}"
        );
        assert_ne!(
            error_type(&response).as_deref(),
            Some("GovernanceDenied"),
            "no X-Agent-Id header — governance must not run: {response:?}"
        );
    }

    #[test]
    fn http_agent_id_header_allows_permitted_action() {
        let server = HttpServer::spawn("caller-identity-http-allow");
        let response = server.call_tool(
            "ledgerr_documents",
            json!({ "action": "list_accounts" }),
            Some("http-agent-allow"),
        );
        assert!(
            !is_error_result(&response),
            "a freshly-registered (Standard ring) agent must be allowed to list_accounts: {response:?}"
        );
    }

    #[test]
    fn http_agent_id_header_denies_action_requiring_approval() {
        let server = HttpServer::spawn("caller-identity-http-deny");
        let response = server.call_tool(
            "ledgerr_reconciliation",
            json!({ "action": "commit" }),
            Some("http-agent-deny"),
        );
        assert!(
            is_error_result(&response),
            "commit must be denied: {response:?}"
        );
        assert_eq!(
            error_type(&response).as_deref(),
            Some("GovernanceDenied"),
            "must be denied by the governance gate specifically: {response:?}"
        );
    }

    #[test]
    fn http_agent_id_is_per_request_not_process_wide() {
        // Same server process, same TCP connection lifecycle, two different
        // callers on consecutive requests — proves identity is read fresh
        // per request rather than latched from the first request seen (the
        // gap #224 exists to close, since HTTP is stateless/multi-tenant
        // per process unlike stdio's one-process-per-agent model).
        let server = HttpServer::spawn("caller-identity-http-per-request");

        let denied = server.call_tool(
            "ledgerr_reconciliation",
            json!({ "action": "commit" }),
            Some("http-agent-first"),
        );
        assert!(
            is_error_result(&denied),
            "commit must be denied: {denied:?}"
        );
        assert_eq!(error_type(&denied).as_deref(), Some("GovernanceDenied"));

        let allowed = server.call_tool(
            "ledgerr_documents",
            json!({ "action": "list_accounts" }),
            Some("http-agent-second"),
        );
        assert!(
            !is_error_result(&allowed),
            "a second, distinct caller's permitted action must still succeed: {allowed:?}"
        );
    }
}
