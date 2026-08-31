//! Covers `LEDGERR_MCP_RING`-gated `tools/list` filtering (issue #222,
//! `docs/progressive-tool-scoping-design.md` §5).
//!
//! Two layers:
//! - Unit-level: `mcp_adapter::filter_tools_for_ring` directly, no server
//!   process involved.
//! - E2E: spawns the real `ledgerr-mcp-server` binary over stdio with
//!   `LEDGERR_MCP_RING` set (or unset), matching the pattern in
//!   `mcp_stdio_e2e.rs`, to prove the env var actually reaches `tools/list`.

mod common;

use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use ledgerr_mcp::mcp_adapter;
use msft_agent_gov_ledgrrr::Ring;
use serde_json::{json, Value};

fn tool_names(tools: &[Value]) -> BTreeSet<String> {
    tools
        .iter()
        .filter_map(|t| t["name"].as_str().map(String::from))
        .collect()
}

// --- Unit-level: filter_tools_for_ring ------------------------------------

#[test]
fn unfiltered_when_ring_is_none() {
    let all = mcp_adapter::tool_descriptors();
    let filtered = mcp_adapter::filter_tools_for_ring(mcp_adapter::tool_descriptors(), None);
    assert_eq!(all.len(), filtered.len());
    assert_eq!(tool_names(&all), tool_names(&filtered));
}

#[test]
fn unfiltered_for_admin_ring() {
    let all = mcp_adapter::tool_descriptors();
    let filtered =
        mcp_adapter::filter_tools_for_ring(mcp_adapter::tool_descriptors(), Some(Ring::Admin));
    assert_eq!(all.len(), filtered.len());
    assert_eq!(tool_names(&all), tool_names(&filtered));
}

#[test]
fn standard_ring_hides_reconciliation_and_xero_but_keeps_core() {
    let filtered = mcp_adapter::filter_tools_for_ring(
        mcp_adapter::tool_descriptors(),
        Some(Ring::Standard),
    );
    let names = tool_names(&filtered);

    // Core group always present.
    assert!(names.contains("ledgerr_schema"));
    assert!(names.contains("ledgerr_manifest"));

    // Ring-gated families Standard should see.
    assert!(names.contains("ledgerr_documents"));
    assert!(names.contains("ledgerr_review"));
    assert!(names.contains("ledgerr_focus"));

    // Not part of Standard's action-pattern list in rings.rs.
    assert!(!names.contains("ledgerr_reconciliation"));
    assert!(!names.contains("ledgerr_xero"));
}

#[test]
fn restricted_ring_is_a_strict_subset_of_standard() {
    let standard = tool_names(&mcp_adapter::filter_tools_for_ring(
        mcp_adapter::tool_descriptors(),
        Some(Ring::Standard),
    ));
    let restricted = tool_names(&mcp_adapter::filter_tools_for_ring(
        mcp_adapter::tool_descriptors(),
        Some(Ring::Restricted),
    ));
    assert!(restricted.is_subset(&standard));
    assert!(restricted.len() < standard.len());
    // Core group still present even at the lowest ring-gated tier.
    assert!(restricted.contains("ledgerr_schema"));
    assert!(restricted.contains("ledgerr_manifest"));
}

#[test]
fn sandboxed_ring_sees_only_the_core_group() {
    let filtered = mcp_adapter::filter_tools_for_ring(
        mcp_adapter::tool_descriptors(),
        Some(Ring::Sandboxed),
    );
    let names = tool_names(&filtered);
    assert_eq!(
        names,
        ["ledgerr_schema", "ledgerr_manifest"]
            .into_iter()
            .map(String::from)
            .collect::<BTreeSet<_>>()
    );
}

// --- E2E: real server binary over stdio -----------------------------------

struct McpStdioClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl McpStdioClient {
    fn spawn(ring: Option<&str>) -> Self {
        let server_bin = env!("CARGO_BIN_EXE_ledgerr-mcp-server");
        let mut cmd = Command::new(server_bin);
        cmd.env(
            "LEDGERR_MCP_MANIFEST",
            common::stdio_test_manifest("tool-visibility-ring-filter"),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
        if let Some(r) = ring {
            cmd.env("LEDGERR_MCP_RING", r);
        } else {
            cmd.env_remove("LEDGERR_MCP_RING");
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

fn initialize_client(client: &mut McpStdioClient) {
    let initialize = client.request(
        "initialize",
        json!({
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": { "name": "tool-visibility-ring-filter-e2e", "version": "0.1.0" }
        }),
    );
    assert!(initialize.get("result").is_some(), "initialize must succeed");
    client.send_notification_initialized();
}

fn list_tool_names(client: &mut McpStdioClient) -> BTreeSet<String> {
    let response = client.request("tools/list", json!({}));
    let tools = response["result"]["tools"]
        .as_array()
        .expect("tools/list result.tools must be an array")
        .clone();
    tool_names(&tools)
}

#[test]
fn e2e_default_env_shows_all_thirteen_tools() {
    let mut client = McpStdioClient::spawn(None);
    initialize_client(&mut client);
    let names = list_tool_names(&mut client);
    assert_eq!(
        names.len(),
        13,
        "default (no LEDGERR_MCP_RING) must be unchanged from pre-#222 behavior: {names:?}"
    );
    assert!(names.contains("ledgerr_reconciliation"));
    assert!(names.contains("ledgerr_xero"));
}

#[test]
fn e2e_restricted_ring_narrows_the_list_over_stdio() {
    let mut client = McpStdioClient::spawn(Some("restricted"));
    initialize_client(&mut client);
    let names = list_tool_names(&mut client);

    assert!(names.contains("ledgerr_schema"), "core group must survive: {names:?}");
    assert!(names.contains("ledgerr_manifest"), "core group must survive: {names:?}");
    assert!(names.contains("ledgerr_documents"));
    assert!(!names.contains("ledgerr_reconciliation"));
    assert!(!names.contains("ledgerr_xero"));
    assert!(!names.contains("ledgerr_review"));
    assert!(names.len() < 13, "restricted must be a strict narrowing: {names:?}");
}

#[test]
fn e2e_unrecognized_ring_value_falls_back_to_unfiltered() {
    let mut client = McpStdioClient::spawn(Some("superuser"));
    initialize_client(&mut client);
    let names = list_tool_names(&mut client);
    assert_eq!(
        names.len(),
        13,
        "an unrecognized LEDGERR_MCP_RING value must fail open to unfiltered, not panic or deny-all: {names:?}"
    );
}
