/// Install-path end-to-end test.
///
/// Spawns the actual `ledgerr-mcp-server` binary (the same artifact that gets
/// bundled into the .mcpb and installed by Claude Desktop), runs the full MCP
/// JSON-RPC startup sequence over stdin/stdout, and verifies:
///
///   1. The binary starts and responds to `initialize`.
///   2. `tools/list` returns the expected published catalog (≥17 tools).
///   3. A real tool call (`ledgerr_documents` / `pipeline_status`) succeeds.
///
/// This test proves the user-facing install path works end-to-end — not just
/// library-level correctness, but the binary-on-disk + stdio transport layer.
mod common;

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{json, Value};

struct InstallClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl InstallClient {
    fn spawn() -> Self {
        let server_bin = env!("CARGO_BIN_EXE_ledgerr-mcp-server");
        let mut child = Command::new(server_bin)
            .env(
                "LEDGERR_MCP_MANIFEST",
                common::stdio_test_manifest("install-e2e"),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap_or_else(|e| panic!("failed to spawn {server_bin}: {e}"));
        let stdin = child.stdin.take().expect("server stdin");
        let stdout = BufReader::new(child.stdout.take().expect("server stdout"));
        Self {
            child,
            stdin,
            stdout,
            next_id: 1,
        }
    }

    fn send(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let payload = json!({"jsonrpc":"2.0","id":id,"method":method,"params":params});
        let line = serde_json::to_string(&payload).expect("serialize");
        writeln!(self.stdin, "{line}").expect("write");
        self.stdin.flush().expect("flush");
        let mut buf = String::new();
        self.stdout.read_line(&mut buf).expect("read");
        serde_json::from_str::<Value>(buf.trim()).expect("parse json")
    }

    fn notify_initialized(&mut self) {
        let payload =
            json!({"jsonrpc":"2.0","method":"notifications/initialized","params":{}});
        let line = serde_json::to_string(&payload).expect("serialize");
        writeln!(self.stdin, "{line}").expect("write");
        self.stdin.flush().expect("flush");
    }
}

impl Drop for InstallClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Minimum published tools the catalog must contain.
const PUBLISHED_TOOLS: &[&str] = &[
    "ledgerr_documents",
    "ledgerr_review",
    "ledgerr_reconciliation",
    "ledgerr_workflow",
    "ledgerr_audit",
    "ledgerr_tax",
    "ledgerr_ontology",
    "ledgerr_xero",
    "ledgerr_focus",
    "ledgerr_evidence",
    "ledgerr_schema",
    "ledgerr_manifest",
    "ledgerr_budget",
    "ledgerr_gcp_billing",
    "ledgerr_scope",
    "ledgerr_validate_process",
];

#[test]
fn install_path_binary_spawns_and_serves_full_catalog() {
    let mut client = InstallClient::spawn();

    // Step 1: initialize handshake
    let init = client.send(
        "initialize",
        json!({
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": {"name": "install-e2e", "version": "0.1.0"}
        }),
    );
    assert!(
        init.get("result").is_some(),
        "initialize failed: {init}"
    );
    client.notify_initialized();

    // Step 2: tools/list — verify full catalog is published
    let tools_resp = client.send("tools/list", json!({}));
    let tools = tools_resp["result"]["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("tools/list returned no array: {tools_resp}"));
    let names: Vec<&str> = tools
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();

    for expected in PUBLISHED_TOOLS {
        assert!(
            names.contains(expected),
            "published tool {expected:?} missing from tools/list; got: {names:?}"
        );
    }

    // Step 3: real tool call — pipeline_status over the stdio transport
    let resp = client.send(
        "tools/call",
        json!({"name":"ledgerr_documents","arguments":{"action":"pipeline_status"}}),
    );
    assert!(
        resp.get("result").is_some(),
        "ledgerr_documents/pipeline_status failed: {resp}"
    );

    let content = &resp["result"]["content"];
    assert!(
        content.is_array() && !content.as_array().unwrap().is_empty(),
        "pipeline_status returned empty content: {resp}"
    );
}
