//! Task 6 (`docs/systems-modeling-registry-rescope.md` §6): wires
//! Requirement/Decision/Cost into the `ledgerr_evidence` MCP tool's
//! contract. Covers the new `import_requirement`/`record_decision`/
//! `record_cost` actions, the `list_nodes` filter extension, and the
//! `summary` node-count extension.

mod common;

use ledgerr_mcp::mcp_adapter::handle_evidence_tool;
use ledgerr_mcp::TurboLedgerService;
use serde_json::json;

fn service() -> TurboLedgerService {
    let workbook_path = common::unique_workbook_path("evidence-rdc");
    TurboLedgerService::from_manifest_str(&common::manifest_for_workbook(&workbook_path, 2023))
        .expect("manifest")
}

#[test]
fn import_requirement_then_list_nodes_and_node_detail() {
    let svc = service();

    let import = handle_evidence_tool(
        &svc,
        &json!({
            "action": "import_requirement",
            "requirement_id": "PO-3-1",
            "title": "toolchain risk mitigation",
            "rationale": "Specify which tools mitigate identified risks.",
            "source": "NIST SSDF 1.1",
            "status": "active",
        }),
    );
    assert_eq!(import["isError"], json!(false));
    let node_id = import["content"][0]["text"]
        .as_str()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(t).ok())
        .expect("parsed content")["node_id"]
        .as_str()
        .expect("node_id")
        .to_string();
    assert!(node_id.starts_with("req:"), "got {node_id}");

    // Re-importing the identical requirement is idempotent (content-hash dedup), not an error.
    let reimport = handle_evidence_tool(
        &svc,
        &json!({
            "action": "import_requirement",
            "requirement_id": "PO-3-1",
            "title": "toolchain risk mitigation",
            "rationale": "Specify which tools mitigate identified risks.",
            "source": "NIST SSDF 1.1",
            "status": "active",
        }),
    );
    assert_eq!(reimport["isError"], json!(false));

    let list = handle_evidence_tool(
        &svc,
        &json!({ "action": "list_nodes", "node_type": "requirement" }),
    );
    let text = list["content"][0]["text"].as_str().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(text).unwrap();
    assert_eq!(parsed["count"], json!(1), "re-import must not duplicate the node");

    let detail = handle_evidence_tool(
        &svc,
        &json!({ "action": "node_detail", "node_id": node_id }),
    );
    assert_eq!(detail["isError"], json!(false));

    let summary = handle_evidence_tool(&svc, &json!({ "action": "summary" }));
    let summary_text = summary["content"][0]["text"].as_str().unwrap();
    let summary_parsed: serde_json::Value = serde_json::from_str(summary_text).unwrap();
    assert_eq!(summary_parsed["node_counts"]["requirements"], json!(1));
}

#[test]
fn record_decision_and_record_cost_are_content_hashed_and_queryable() {
    let svc = service();

    let decision = handle_evidence_tool(
        &svc,
        &json!({
            "action": "record_decision",
            "decision_id": "DEC-001",
            "subject": "Adopt sysml-derive over LinkML",
            "rationale": "Rust-first fit, no correctness surprise",
            "decided_by": "brianh",
        }),
    );
    assert_eq!(decision["isError"], json!(false));

    let cost = handle_evidence_tool(
        &svc,
        &json!({
            "action": "record_cost",
            "cost_id": "COST-001",
            "subject": "GPU training run",
            "amount": "42.50",
            "currency": "USD",
            "recorded_by": "brianh",
        }),
    );
    assert_eq!(cost["isError"], json!(false));

    let list_dec = handle_evidence_tool(
        &svc,
        &json!({ "action": "list_nodes", "node_type": "decision" }),
    );
    let dec_text = list_dec["content"][0]["text"].as_str().unwrap();
    let dec_parsed: serde_json::Value = serde_json::from_str(dec_text).unwrap();
    assert_eq!(dec_parsed["count"], json!(1));

    let list_cost = handle_evidence_tool(
        &svc,
        &json!({ "action": "list_nodes", "node_type": "cost" }),
    );
    let cost_text = list_cost["content"][0]["text"].as_str().unwrap();
    let cost_parsed: serde_json::Value = serde_json::from_str(cost_text).unwrap();
    assert_eq!(cost_parsed["count"], json!(1));

    let summary = handle_evidence_tool(&svc, &json!({ "action": "summary" }));
    let summary_text = summary["content"][0]["text"].as_str().unwrap();
    let summary_parsed: serde_json::Value = serde_json::from_str(summary_text).unwrap();
    assert_eq!(summary_parsed["node_counts"]["decisions"], json!(1));
    assert_eq!(summary_parsed["node_counts"]["costs"], json!(1));
}

#[test]
fn list_nodes_rejects_unknown_type_with_full_valid_list() {
    let svc = service();
    let result = handle_evidence_tool(
        &svc,
        &json!({ "action": "list_nodes", "node_type": "bogus" }),
    );
    assert_eq!(result["isError"], json!(true));
    let text = result["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("requirement"));
    assert!(text.contains("decision"));
    assert!(text.contains("cost"));
}
