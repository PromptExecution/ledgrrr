mod common;

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{json, Value};

struct McpStdioClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl McpStdioClient {
    fn spawn() -> Self {
        let server_bin = env!("CARGO_BIN_EXE_ledgerr-mcp-server");
        let mut child = Command::new(server_bin)
            .env(
                "LEDGERR_MCP_MANIFEST",
                common::stdio_test_manifest("mcp-stdio-e2e"),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn turbo-mcp-server");
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
        self.send_and_read(payload)
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

    fn send_and_read(&mut self, payload: Value) -> Value {
        let line = serde_json::to_string(&payload).expect("serialize request");
        writeln!(self.stdin, "{line}").expect("write request");
        self.stdin.flush().expect("flush request");

        let mut response = String::new();
        self.stdout
            .read_line(&mut response)
            .expect("read response line");
        serde_json::from_str::<Value>(response.trim()).expect("parse response json")
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
            "clientInfo": { "name": "mcp-stdio-e2e", "version": "0.1.0" }
        }),
    );
    assert!(
        initialize.get("result").is_some(),
        "initialize must succeed"
    );
    client.send_notification_initialized();
}

fn build_ingest_arguments(base_dir: &std::path::Path) -> Value {
    json!({
        "action": "ingest_pdf",
        "pdf_path": "WF--BH-CHK--2023-01--statement.pdf",
        "journal_path": base_dir.join("ledger.beancount").display().to_string(),
        "workbook_path": base_dir.join("tax-ledger.xlsx").display().to_string(),
        "raw_context_bytes": [99, 116, 120],
        "extracted_rows": [
            {
                "account_id": "WF-BH-CHK",
                "date": "2023-01-15",
                "amount": "-42.11",
                "description": "Coffee Shop",
                "source_ref": base_dir
                    .join("WF--BH-CHK--2023-01--statement.rkyv")
                    .display()
                    .to_string()
            }
        ]
    })
}

fn build_rustledger_rows_arguments(base_dir: &std::path::Path) -> Value {
    json!({
        "action": "ingest_rows",
        "journal_path": base_dir.join("ledger.beancount").display().to_string(),
        "workbook_path": base_dir.join("tax-ledger.xlsx").display().to_string(),
        "rows": [
            {
                "account_id": "WF-BH-CHK",
                "date": "2023-01-15",
                "amount": "-42.11",
                "description": "Coffee Shop",
                "source_ref": base_dir
                    .join("WF--BH-CHK--2023-01--statement.rkyv")
                    .display()
                    .to_string()
            }
        ]
    })
}

// DOC-01: ingest path must be executable through MCP tools/call only.

fn parse_response_payload(response: &serde_json::Value) -> serde_json::Value {
    let text = response["content"][0]["text"].as_str().unwrap_or("null");
    serde_json::from_str(text).unwrap_or(serde_json::Value::Null)
}

#[test]
fn doc_01_mcp_only_ingest_via_tools_call() {
    let mut client = McpStdioClient::spawn();
    initialize_client(&mut client);

    let tools = client.request("tools/list", json!({}));
    let tool_names = tools["result"]["tools"]
        .as_array()
        .expect("tools list")
        .iter()
        .filter_map(|entry| entry.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert_eq!(tool_names.len(), 10);
    assert!(tool_names.contains(&"ledgerr_documents"));

    let tempdir = tempfile::tempdir().expect("tempdir");
    let call = client.request(
        "tools/call",
        json!({
            "name": "ledgerr_documents",
            "arguments": build_ingest_arguments(tempdir.path())
        }),
    );

    assert_eq!(call["result"]["isError"], Value::Bool(false));
    assert_eq!(
        parse_response_payload(&call["result"])["inserted_count"],
        json!(1)
    );
    assert!(
        parse_response_payload(&call["result"])["tx_ids"]
            .as_array()
            .expect("tx_ids array")
            .len()
            == 1
    );
}

// DOC-02: canonical + provenance mapping must be deterministic in MCP payloads.
#[test]
fn doc_02_canonical_mapping_and_provenance_fields_over_transport() {
    let mut client = McpStdioClient::spawn();
    initialize_client(&mut client);

    let tempdir = tempfile::tempdir().expect("tempdir");
    let call = client.request(
        "tools/call",
        json!({
            "name": "ledgerr_documents",
            "arguments": build_ingest_arguments(tempdir.path())
        }),
    );

    let p = parse_response_payload(&call["result"]);
    let canonical = &p["canonical_rows"][0];
    assert!(canonical.get("account").is_some());
    assert!(canonical.get("date").is_some());
    assert!(canonical.get("amount").is_some());
    assert!(canonical.get("description").is_some());
    assert!(canonical.get("currency").is_some());
    assert!(canonical.get("source_ref").is_some());
    assert!(canonical.get("provider").is_some());
    assert!(canonical.get("backend_tool").is_some());
    assert!(canonical.get("backend_version").is_some());
    assert!(canonical.get("backend_call_id").is_some());
}

// DOC-03: replaying identical source through MCP remains idempotent with stable tx IDs.
#[test]
fn doc_03_replay_idempotent_with_stable_tx_ids_over_mcp() {
    let mut client = McpStdioClient::spawn();
    initialize_client(&mut client);
    let tempdir = tempfile::tempdir().expect("tempdir");

    let first = client.request(
        "tools/call",
        json!({
            "name": "ledgerr_documents",
            "arguments": build_ingest_arguments(tempdir.path())
        }),
    );
    let second = client.request(
        "tools/call",
        json!({
            "name": "ledgerr_documents",
            "arguments": build_ingest_arguments(tempdir.path())
        }),
    );

    assert_eq!(
        parse_response_payload(&first["result"])["inserted_count"],
        json!(1)
    );
    assert_eq!(
        parse_response_payload(&second["result"])["inserted_count"],
        json!(0)
    );

    let fp = parse_response_payload(&first["result"]);
    let first_ids = fp["tx_ids"].as_array().expect("first tx ids");
    let sp = parse_response_payload(&second["result"]);
    let second_ids = sp["tx_ids"].as_array().expect("second tx ids");
    assert_eq!(first_ids, second_ids);
}

// DOC-01/02/03 (D-03): Rustledger passthrough must be callable via MCP tools/call only.
#[test]
fn rustledger_proxy_ingest_statement_rows_over_transport() {
    let mut client = McpStdioClient::spawn();
    initialize_client(&mut client);

    let tools = client.request("tools/list", json!({}));
    let tool_names = tools["result"]["tools"]
        .as_array()
        .expect("tools list")
        .iter()
        .filter_map(|entry| entry.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert!(tool_names.contains(&"ledgerr_documents"));

    let tempdir = tempfile::tempdir().expect("tempdir");
    let first = client.request(
        "tools/call",
        json!({
            "name": "ledgerr_documents",
            "arguments": build_rustledger_rows_arguments(tempdir.path())
        }),
    );
    let second = client.request(
        "tools/call",
        json!({
            "name": "ledgerr_documents",
            "arguments": build_rustledger_rows_arguments(tempdir.path())
        }),
    );

    assert_eq!(first["result"]["isError"], Value::Bool(false));
    assert_eq!(
        parse_response_payload(&first["result"])["inserted_count"],
        json!(1)
    );
    assert_eq!(
        parse_response_payload(&second["result"])["inserted_count"],
        json!(0)
    );

    let fp2 = parse_response_payload(&first["result"]);
    let first_ids = fp2["tx_ids"].as_array().expect("first tx ids");
    let sp2 = parse_response_payload(&second["result"]);
    let second_ids = sp2["tx_ids"].as_array().expect("second tx ids");
    assert_eq!(first_ids, second_ids);

    let canonical = &fp2["canonical_rows"][0];
    assert_eq!(canonical["provider"], json!("rustledger"));
    assert_eq!(canonical["backend_tool"], json!("ingest_statement_rows"));
    assert!(canonical.get("account").is_some());
    assert!(canonical.get("date").is_some());
    assert!(canonical.get("amount").is_some());
    assert!(canonical.get("description").is_some());
    assert!(canonical.get("currency").is_some());
    assert!(canonical.get("source_ref").is_some());
    assert!(canonical.get("backend_version").is_some());
    assert!(canonical.get("backend_call_id").is_some());
}

#[test]
fn mcp_lists_and_calls_accounts_and_raw_context_tools() {
    let mut client = McpStdioClient::spawn();
    initialize_client(&mut client);

    let tools = client.request("tools/list", json!({}));
    let tool_names = tools["result"]["tools"]
        .as_array()
        .expect("tools list")
        .iter()
        .filter_map(|entry| entry.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert!(tool_names.contains(&"ledgerr_documents"));

    let list_accounts = client.request(
        "tools/call",
        json!({
            "name": "ledgerr_documents",
            "arguments": { "action": "list_accounts" }
        }),
    );
    assert_eq!(list_accounts["result"]["isError"], Value::Bool(false));
    let accounts_p = parse_response_payload(&list_accounts["result"]);
    let accounts = accounts_p["accounts"].as_array().expect("accounts array");
    assert!(!accounts.is_empty());
    assert!(accounts
        .iter()
        .any(|entry| entry["account_id"] == json!("WF-BH-CHK")));

    let tempdir = tempfile::tempdir().expect("tempdir");
    let ingest_args = build_ingest_arguments(tempdir.path());
    let source_ref = ingest_args["extracted_rows"][0]["source_ref"]
        .as_str()
        .expect("source_ref")
        .to_string();
    let ingest = client.request(
        "tools/call",
        json!({
            "name": "ledgerr_documents",
            "arguments": ingest_args
        }),
    );
    assert_eq!(ingest["result"]["isError"], Value::Bool(false));

    let raw_context = client.request(
        "tools/call",
        json!({
            "name": "ledgerr_documents",
            "arguments": { "action": "get_raw_context", "rkyv_ref": source_ref }
        }),
    );
    assert_eq!(raw_context["result"]["isError"], Value::Bool(false));
    assert_eq!(
        parse_response_payload(&raw_context["result"])["bytes"],
        json!([99, 116, 120])
    );
}
