mod common;

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use ledgerr_mcp::{
    mcp_adapter, OntologyEdgeInput, OntologyEntityInput, OntologyEntityKind,
    OntologyExportSnapshotRequest, OntologyStore, TurboLedgerService,
};
use serde_json::{json, Value};

const ONTOLOGY_QUERY_TOOL: &str = "l3dg3rr_ontology_query_path";
const ONTOLOGY_EXPORT_TOOL: &str = "l3dg3rr_ontology_export_snapshot";

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
                common::stdio_test_manifest("ontology-mcp"),
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
            "clientInfo": { "name": "ontology-mcp-e2e", "version": "0.1.0" }
        }),
    );
    assert!(
        initialize.get("result").is_some(),
        "initialize must succeed"
    );
    client.send_notification_initialized();
}

fn seed_ontology(path: &std::path::Path) -> (String, String, String, String) {
    let mut store = OntologyStore::default();

    let entities = store
        .upsert_entities(vec![
            OntologyEntityInput {
                kind: OntologyEntityKind::Document,
                attrs: {
                    let mut attrs = BTreeMap::new();
                    attrs.insert("source_ref".to_string(), "wf-statement.pdf".to_string());
                    attrs
                },
                custom_kind: None,
            },
            OntologyEntityInput {
                kind: OntologyEntityKind::Transaction,
                attrs: {
                    let mut attrs = BTreeMap::new();
                    attrs.insert("tx_id".to_string(), "tx-001".to_string());
                    attrs
                },
                custom_kind: None,
            },
            OntologyEntityInput {
                kind: OntologyEntityKind::TaxCategory,
                attrs: {
                    let mut attrs = BTreeMap::new();
                    attrs.insert("category".to_string(), "OfficeSupplies".to_string());
                    attrs
                },
                custom_kind: None,
            },
            OntologyEntityInput {
                kind: OntologyEntityKind::EvidenceReference,
                attrs: {
                    let mut attrs = BTreeMap::new();
                    attrs.insert("rkyv_ref".to_string(), "wf-ctx.rkyv".to_string());
                    attrs
                },
                custom_kind: None,
            },
        ], None)
        .expect("seed entities");

    let doc_id = entities.entity_ids[0].clone();
    let tx_id = entities.entity_ids[1].clone();
    let tax_id = entities.entity_ids[2].clone();
    let evidence_id = entities.entity_ids[3].clone();

    store
        .upsert_edges(vec![
            OntologyEdgeInput {
                from: tx_id.clone(),
                to: tax_id.clone(),
                relation: "links_tax_category".to_string(),
                provenance: BTreeMap::new(),
            },
            OntologyEdgeInput {
                from: doc_id.clone(),
                to: tx_id.clone(),
                relation: "documents_transaction".to_string(),
                provenance: BTreeMap::new(),
            },
            OntologyEdgeInput {
                from: tx_id.clone(),
                to: evidence_id.clone(),
                relation: "links_evidence".to_string(),
                provenance: BTreeMap::new(),
            },
        ])
        .expect("seed edges");

    store.persist(path).expect("persist seed ontology");

    (doc_id, tx_id, evidence_id, tax_id)
}

// ONTO-03 (D-03): tools/list advertises ontology query/export transport surfaces.

fn parse_response_payload(response: &serde_json::Value) -> serde_json::Value {
    let text = response["content"][0]["text"].as_str().unwrap_or("null");
    serde_json::from_str(text).unwrap_or(serde_json::Value::Null)
}

#[test]
fn onto_03_tools_list_advertises_ontology_tool() {
    let mut client = McpStdioClient::spawn();
    initialize_client(&mut client);

    let tools = client.request("tools/list", json!({}));
    let tool_names = tools["result"]["tools"]
        .as_array()
        .expect("tools list")
        .iter()
        .filter_map(|entry| entry.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();

    assert!(tool_names.contains(&"ledgerr_ontology"));
}

// ONTO-03 (D-03): ontology query and export return deterministic concise payloads over transport.
#[test]
fn onto_03_tools_call_query_and_export_snapshot_payloads_are_deterministic() {
    let mut client = McpStdioClient::spawn();
    initialize_client(&mut client);

    let tempdir = tempfile::tempdir().expect("tempdir");
    let ontology_path = tempdir.path().join("ontology.json");
    let (doc_id, tx_id, evidence_id, tax_id) = seed_ontology(&ontology_path);

    let query = client.request(
        "tools/call",
        json!({
            "name": ONTOLOGY_QUERY_TOOL,
            "arguments": {
                "ontology_path": ontology_path.display().to_string(),
                "from_entity_id": doc_id,
                "max_depth": 4
            }
        }),
    );

    assert_eq!(query["result"]["isError"], Value::Bool(false));

    let query_json = parse_response_payload(&query["result"]);
    let node_ids = query_json["nodes"]
        .as_array()
        .expect("nodes array")
        .iter()
        .map(|node| node["id"].as_str().expect("node id").to_string())
        .collect::<Vec<_>>();
    assert_eq!(node_ids, vec![doc_id, tx_id, evidence_id, tax_id]);

    let export = client.request(
        "tools/call",
        json!({
            "name": ONTOLOGY_EXPORT_TOOL,
            "arguments": {
                "ontology_path": ontology_path.display().to_string()
            }
        }),
    );

    assert_eq!(export["result"]["isError"], Value::Bool(false));

    let export_json = parse_response_payload(&export["result"]);
    let entity_kinds = export_json["entities"]
        .as_array()
        .expect("entities array")
        .iter()
        .map(|entity| entity["kind"].as_str().expect("kind").to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        entity_kinds,
        vec![
            "document".to_string(),
            "transaction".to_string(),
            "tax_category".to_string(),
            "evidence_reference".to_string(),
        ]
    );

    let edge_relations = export_json["edges"]
        .as_array()
        .expect("edges array")
        .iter()
        .map(|edge| edge["relation"].as_str().expect("relation").to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        edge_relations,
        vec![
            "documents_transaction".to_string(),
            "links_evidence".to_string(),
            "links_tax_category".to_string(),
        ]
    );
}

// ONTO-03 (D-03): repeated ontology export for unchanged inputs remains byte-for-byte stable.
#[test]
fn onto_03_export_snapshot_stable_json_serialization_over_transport() {
    let mut client = McpStdioClient::spawn();
    initialize_client(&mut client);

    let tempdir = tempfile::tempdir().expect("tempdir");
    let ontology_path = tempdir.path().join("ontology.json");
    let _ = seed_ontology(&ontology_path);

    let first = client.request(
        "tools/call",
        json!({
            "name": ONTOLOGY_EXPORT_TOOL,
            "arguments": {
                "ontology_path": ontology_path.display().to_string()
            }
        }),
    );
    let second = client.request(
        "tools/call",
        json!({
            "name": ONTOLOGY_EXPORT_TOOL,
            "arguments": {
                "ontology_path": ontology_path.display().to_string()
            }
        }),
    );

    assert_eq!(first["result"]["isError"], Value::Bool(false));
    assert_eq!(second["result"]["isError"], Value::Bool(false));

    let first_payload = serde_json::to_string(&parse_response_payload(&first["result"]))
        .expect("serialize first payload");
    let second_payload = serde_json::to_string(&parse_response_payload(&second["result"]))
        .expect("serialize second payload");

    assert_eq!(first_payload, second_payload);
}

// ONTO-03 (D-03): ontology_export_snapshot routes through TurboLedgerService, not OntologyStore directly.
#[test]
fn onto_03_export_snapshot_routes_through_service() {
    let test_manifest = format!(
        "{}\n[accounts]\nWF-BH-CHK = {{ institution = \"Wells Fargo\", type = \"checking\", currency = \"USD\" }}\n",
        common::manifest_for_workbook(&common::unique_workbook_path("ontology-mcp"), 2023)
    );
    let service =
        TurboLedgerService::from_manifest_str(&test_manifest).expect("manifest must parse");

    let tempdir = tempfile::tempdir().expect("tempdir");
    let ontology_path = tempdir.path().join("ontology.json");
    let _ = seed_ontology(&ontology_path);

    // Call via service method directly and verify response struct fields.
    let response = service
        .ontology_export_snapshot(OntologyExportSnapshotRequest {
            ontology_path: ontology_path.clone(),
        })
        .expect("ontology_export_snapshot must succeed");

    assert_eq!(response.entity_count, response.entities.len());
    assert_eq!(response.edge_count, response.edges.len());
    assert_eq!(response.entity_count, 4, "seed produces 4 entities");
    assert_eq!(response.edge_count, 3, "seed produces 3 edges");
    assert!(!response.entities.is_empty());
    assert!(!response.edges.is_empty());

    // Call via JSON handler and check the isError: false shape.
    let args = json!({ "ontology_path": ontology_path.display().to_string() });
    let result = mcp_adapter::handle_ontology_export_snapshot(&service, &args);

    assert_eq!(result["isError"], Value::Bool(false));
    let json_payload = parse_response_payload(&result);
    assert!(json_payload["entities"].is_array());
    assert!(json_payload["edges"].is_array());
    assert_eq!(
        json_payload["snapshot"]["entity_count"].as_u64().unwrap(),
        4
    );
    assert_eq!(json_payload["snapshot"]["edge_count"].as_u64().unwrap(), 3);
}
