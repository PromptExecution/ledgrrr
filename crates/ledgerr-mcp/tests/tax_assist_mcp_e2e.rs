mod common;

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use ledgerr_mcp::ontology::{edge_content_hash, entity_content_hash};
use ledgerr_mcp::{OntologyEdge, OntologyEntity, OntologyEntityKind, OntologyStore};
use serde_json::{json, Value};

const TAX_ASSIST_TOOL: &str = "l3dg3rr_tax_assist";
const TAX_EVIDENCE_CHAIN_TOOL: &str = "l3dg3rr_tax_evidence_chain";
const TAX_AMBIGUITY_REVIEW_TOOL: &str = "l3dg3rr_tax_ambiguity_review";

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
                common::stdio_test_manifest("tax-assist-mcp"),
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
            "clientInfo": { "name": "tax-assist-mcp-e2e", "version": "0.1.0" }
        }),
    );
    assert!(
        initialize.get("result").is_some(),
        "initialize must succeed"
    );
    client.send_notification_initialized();
}

fn write_tax_ontology(path: &std::path::Path) -> String {
    let mut tx_attrs = BTreeMap::new();
    tx_attrs.insert("tx_id".to_string(), "tx-e2e-01".to_string());
    tx_attrs.insert("amount".to_string(), "-42.11".to_string());
    let mut doc_attrs = BTreeMap::new();
    doc_attrs.insert("doc_ref".to_string(), "source/wf-2023-01.rkyv".to_string());
    let mut tax_attrs = BTreeMap::new();
    tax_attrs.insert("code".to_string(), "OfficeSupplies".to_string());

    let tx_id = entity_content_hash(OntologyEntityKind::Transaction, &tx_attrs);
    let doc_id = entity_content_hash(OntologyEntityKind::Document, &doc_attrs);
    let tax_id = entity_content_hash(OntologyEntityKind::TaxCategory, &tax_attrs);

    let mut schedule_prov = BTreeMap::new();
    schedule_prov.insert(
        "source_ref".to_string(),
        "source/wf-2023-01.rkyv".to_string(),
    );
    let mut fbar_prov = BTreeMap::new();
    fbar_prov.insert(
        "source_ref".to_string(),
        "source/wf-2023-01.rkyv".to_string(),
    );
    let mut ambiguity_prov = BTreeMap::new();
    ambiguity_prov.insert(
        "source_ref".to_string(),
        "source/wf-2023-01.rkyv".to_string(),
    );
    ambiguity_prov.insert("reason".to_string(), "classification_conflict".to_string());

    let store = OntologyStore {
        artifacts: vec![
            OntologyEntity {
                id: tx_id.clone(),
                kind: OntologyEntityKind::Transaction,
                attrs: tx_attrs,
            },
            OntologyEntity {
                id: doc_id.clone(),
                kind: OntologyEntityKind::Document,
                attrs: doc_attrs,
            },
            OntologyEntity {
                id: tax_id.clone(),
                kind: OntologyEntityKind::TaxCategory,
                attrs: tax_attrs,
            },
        ],
        relations: vec![
            OntologyEdge {
                id: edge_content_hash(&tx_id, &tax_id, "schedule_c", &schedule_prov),
                from: tx_id.clone(),
                to: tax_id.clone(),
                relation: "schedule_c".to_string(),
                provenance: schedule_prov,
            },
            OntologyEdge {
                id: edge_content_hash(&tx_id, &doc_id, "fbar_reportable", &fbar_prov),
                from: tx_id.clone(),
                to: doc_id,
                relation: "fbar_reportable".to_string(),
                provenance: fbar_prov,
            },
            OntologyEdge {
                id: edge_content_hash(&tx_id, &tax_id, "ambiguity", &ambiguity_prov),
                from: tx_id.clone(),
                to: tax_id,
                relation: "ambiguity".to_string(),
                provenance: ambiguity_prov,
            },
        ],
    };
    let payload = serde_json::to_string_pretty(&store).expect("serialize store");
    std::fs::write(path, payload).expect("write ontology");
    tx_id
}

fn ingest_one_row(client: &mut McpStdioClient) {
    let ingest = client.request(
        "tools/call",
        json!({
            "name": "proxy_rustledger_ingest_statement_rows",
            "arguments": {
                "journal_path": "/tmp/tax-assist-mcp-e2e-journal.beancount",
                "workbook_path": "/tmp/tax-assist-mcp-e2e.xlsx",
                "rows": [{
                    "account": "WF-BH-CHK",
                    "date": "2023-01-15",
                    "amount": "-42.11",
                    "description": "Coffee Shop",
                    "source_ref": "source/wf-2023-01.rkyv"
                }]
            }
        }),
    );
    assert_eq!(ingest["result"]["isError"], Value::Bool(false));
}

fn parse_response_payload(response: &serde_json::Value) -> serde_json::Value {
    let text = response["content"][0]["text"].as_str().unwrap_or("null");
    serde_json::from_str(text).unwrap_or(serde_json::Value::Null)
}

#[test]
fn taxa_mcp_tools_list_advertises_tax_tool() {
    let mut client = McpStdioClient::spawn();
    initialize_client(&mut client);

    let tools = client.request("tools/list", json!({}));
    let tool_names = tools["result"]["tools"]
        .as_array()
        .expect("tools list")
        .iter()
        .filter_map(|entry| entry.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();

    assert!(tool_names.contains(&"ledgerr_tax"));
}

#[test]
fn taxa_mcp_tools_call_return_deterministic_tax_assist_and_evidence_chain_sections() {
    let mut client = McpStdioClient::spawn();
    initialize_client(&mut client);
    ingest_one_row(&mut client);
    let temp = tempfile::tempdir().expect("tempdir");
    let ontology_path = temp.path().join("ontology.json");
    let from_entity_id = write_tax_ontology(&ontology_path);

    let assist = client.request(
        "tools/call",
        json!({
            "name": TAX_ASSIST_TOOL,
            "arguments": {
                "ontology_path": ontology_path.display().to_string(),
                "from_entity_id": from_entity_id,
                "max_depth": 4,
                "reconciliation": {
                    "source_total": "100.00",
                    "extracted_total": "100.00",
                    "posting_amounts": ["-100.00", "100.00"]
                }
            }
        }),
    );
    assert_eq!(assist["result"]["isError"], Value::Bool(false));
    let assist_payload = parse_response_payload(&assist["result"]);
    assert!(assist_payload.get("summary").is_some());
    assert!(assist_payload.get("schedule_rows").is_some());
    assert!(assist_payload.get("fbar_rows").is_some());
    assert!(assist_payload.get("ambiguity").is_some());

    let chain = client.request(
        "tools/call",
        json!({
            "name": TAX_EVIDENCE_CHAIN_TOOL,
            "arguments": {
                "ontology_path": ontology_path.display().to_string(),
                "from_entity_id": assist_payload["summary"]["source_entity_id"],
                "document_ref": "source/wf-2023-01.rkyv"
            }
        }),
    );
    assert_eq!(chain["result"]["isError"], Value::Bool(false));
    let chain_payload = parse_response_payload(&chain["result"]);
    assert!(chain_payload.get("source").is_some());
    assert!(chain_payload.get("events").is_some());
    assert!(chain_payload.get("current_state").is_some());
}

#[test]
fn taxa_mcp_ambiguity_review_payload_includes_provenance_and_review_state() {
    let mut client = McpStdioClient::spawn();
    initialize_client(&mut client);
    let temp = tempfile::tempdir().expect("tempdir");
    let ontology_path = temp.path().join("ontology.json");
    let from_entity_id = write_tax_ontology(&ontology_path);

    let response = client.request(
        "tools/call",
        json!({
            "name": TAX_AMBIGUITY_REVIEW_TOOL,
            "arguments": {
                "ontology_path": ontology_path.display().to_string(),
                "from_entity_id": from_entity_id,
                "max_depth": 4,
                "reconciliation": {
                    "source_total": "100.00",
                    "extracted_total": "100.00",
                    "posting_amounts": ["-100.00", "100.00"]
                }
            }
        }),
    );
    assert_eq!(response["result"]["isError"], Value::Bool(false));
    let payload = parse_response_payload(&response["result"]);
    assert_eq!(payload["status"], json!("review_ready"));
    assert_eq!(
        payload["ambiguity"][0]["review_state"],
        json!("needs_review")
    );
    assert_eq!(
        payload["ambiguity"][0]["provenance_refs"],
        json!(["source/wf-2023-01.rkyv"])
    );
}
