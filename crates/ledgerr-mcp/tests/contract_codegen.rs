use std::fs;
use std::path::PathBuf;

use ledgerr_mcp::contract::{
    self, DocumentsArgs, WorkflowArgs, AUDIT_TOOL, DOCUMENTS_TOOL, EVIDENCE_TOOL, FOCUS_TOOL,
    ONTOLOGY_TOOL, RECONCILIATION_TOOL, REVIEW_TOOL, TAX_TOOL, WORKFLOW_TOOL, XERO_TOOL,
};
use serde_json::json;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace crates dir")
        .parent()
        .expect("repo root")
        .to_path_buf()
}

#[test]
fn contract_docs_are_generated_from_rust_source() {
    let root = repo_root();
    let checked_in = fs::read_to_string(root.join("docs/mcp-capability-contract.md"))
        .expect("capability contract doc");
    let generated = contract::generated_capability_contract_markdown();
    assert_eq!(checked_in, generated);
}

#[test]
fn runbook_is_generated_from_rust_source() {
    let root = repo_root();
    let checked_in =
        fs::read_to_string(root.join("docs/agent-mcp-runbook.md")).expect("agent runbook");
    let generated = contract::generated_agent_runbook_markdown();
    assert_eq!(checked_in, generated);
}

#[test]
fn demo_script_is_generated_from_rust_source() {
    let root = repo_root();
    let checked_in =
        fs::read_to_string(root.join("scripts/mcp_cli_demo.sh")).expect("mcp cli demo script");
    let generated = contract::generated_mcp_cli_demo_script();
    assert_eq!(checked_in, generated);
}

#[test]
fn documents_contract_accepts_legacy_account_alias() {
    let parsed = contract::parse_documents(&json!({
        "action": "ingest_rows",
        "journal_path": "/tmp/demo.beancount",
        "workbook_path": "/tmp/demo.xlsx",
        "rows": [{
            "account": "WF-BH-CHK",
            "date": "2023-01-15",
            "amount": "-42.11",
            "description": "Coffee Shop",
            "source_ref": "wf-2023-01.rkyv"
        }]
    }))
    .expect("documents args parse");

    match parsed {
        DocumentsArgs::IngestRows {
            ontology_path,
            rows,
            ..
        } => {
            assert_eq!(ontology_path, None);
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].account.as_deref(), Some("WF-BH-CHK"));
            assert_eq!(rows[0].account_id, None);
        }
        other => panic!("unexpected variant: {other:?}"),
    }
}

#[test]
fn documents_contract_accepts_optional_ontology_path_for_ingest() {
    let rows = contract::parse_documents(&json!({
        "action": "ingest_rows",
        "journal_path": "/tmp/demo.beancount",
        "workbook_path": "/tmp/demo.xlsx",
        "ontology_path": "/tmp/demo.ontology.json",
        "rows": [{
            "account_id": "WF-BH-CHK",
            "date": "2023-01-15",
            "amount": "-42.11",
            "description": "Coffee Shop",
            "source_ref": "wf-2023-01.rkyv"
        }]
    }))
    .expect("documents ingest rows args parse");

    match rows {
        DocumentsArgs::IngestRows { ontology_path, .. } => {
            assert_eq!(
                ontology_path,
                Some(PathBuf::from("/tmp/demo.ontology.json"))
            );
        }
        other => panic!("unexpected variant: {other:?}"),
    }

    let pdf = contract::parse_documents(&json!({
        "action": "ingest_pdf",
        "pdf_path": "WF--BH-CHK--2023-01--statement.pdf",
        "journal_path": "/tmp/demo.beancount",
        "workbook_path": "/tmp/demo.xlsx",
        "ontology_path": "/tmp/demo.ontology.json",
        "raw_context_bytes": [99, 116, 120],
        "extracted_rows": []
    }))
    .expect("documents ingest pdf args parse");

    match pdf {
        DocumentsArgs::IngestPdf { ontology_path, .. } => {
            assert_eq!(
                ontology_path,
                Some(PathBuf::from("/tmp/demo.ontology.json"))
            );
        }
        other => panic!("unexpected variant: {other:?}"),
    }
}

#[test]
fn review_contract_accepts_string_review_threshold() {
    let parsed = contract::parse_review(&json!({
        "action": "classify_ingested",
        "rule_file": "/tmp/rules.rhai",
        "review_threshold": "0.91"
    }))
    .expect("review args parse");

    match parsed {
        ledgerr_mcp::contract::ReviewArgs::ClassifyIngested {
            review_threshold, ..
        } => {
            assert_eq!(review_threshold.as_json(), json!("0.91"));
        }
        other => panic!("unexpected variant: {other:?}"),
    }
}

#[test]
fn published_tool_schema_generation_stays_wired_to_all_visible_tools() {
    for tool in [
        DOCUMENTS_TOOL,
        REVIEW_TOOL,
        RECONCILIATION_TOOL,
        WORKFLOW_TOOL,
        AUDIT_TOOL,
        TAX_TOOL,
        ONTOLOGY_TOOL,
        XERO_TOOL,
        FOCUS_TOOL,
        EVIDENCE_TOOL,
    ] {
        let schema = contract::tool_input_schema(tool);
        assert!(
            schema.is_object(),
            "schema for {tool} should serialize as an object"
        );
    }
}

/// Regression test: Claude API rejects `oneOf`/`anyOf`/`allOf` at the top level
/// of a tool's `input_schema` with HTTP 400. schemars 0.8 generates top-level
/// `oneOf` for ALL Rust enums including those with `#[serde(tag = "action")]`.
///
/// `flatten_tagged_oneof_for_claude` in `contract.rs` collapses this into a flat
/// discriminated-union object. DO NOT remove or weaken this test — the broken
/// `or_insert_with("type":"object")` approach would also pass `is_object()` above
/// but would still emit a `oneOf` at root that the Claude API rejects.
///
/// If this test fails after a refactor, read the doc-comment on
/// `flatten_tagged_oneof_for_claude` in `crates/ledgerr-mcp/src/contract.rs`
/// before touching the schema generation path.
#[test]
fn all_tool_schemas_are_claude_api_compatible_no_root_composition_keywords() {
    for tool in [
        DOCUMENTS_TOOL,
        REVIEW_TOOL,
        RECONCILIATION_TOOL,
        WORKFLOW_TOOL,
        AUDIT_TOOL,
        TAX_TOOL,
        ONTOLOGY_TOOL,
        XERO_TOOL,
        FOCUS_TOOL,
        EVIDENCE_TOOL,
    ] {
        let schema = contract::tool_input_schema(tool);
        for keyword in ["oneOf", "anyOf", "allOf"] {
            assert!(
                schema.get(keyword).is_none(),
                "schema for {tool} must not contain '{keyword}' at root — \
                 Claude API rejects input_schema with top-level composition keywords (HTTP 400). \
                 See flatten_tagged_oneof_for_claude in contract.rs before modifying."
            );
        }
        assert_eq!(
            schema.get("type").and_then(|t| t.as_str()),
            Some("object"),
            "schema for {tool} must have type=object at root"
        );
        assert!(
            schema
                .get("properties")
                .and_then(|p| p.get("action"))
                .is_some(),
            "schema for {tool} must have an 'action' property (action discriminator)"
        );
        assert_eq!(
            schema.get("required").and_then(|r| r.as_array()).map(|a| {
                a.iter()
                    .any(|v| v.as_str() == Some("action"))
            }),
            Some(true),
            "schema for {tool} must require the 'action' field"
        );
    }
}

#[test]
fn workflow_contract_allows_unknown_plugin_subcommand_for_postel_boundary_behavior() {
    let parsed = contract::parse_workflow(&json!({
        "action": "plugin_info",
        "subcommand": "nonsense"
    }))
    .expect("workflow args parse");

    match parsed {
        WorkflowArgs::PluginInfo { payload } => {
            assert_eq!(
                payload.subcommand.map(|value| value.0),
                Some("nonsense".to_string())
            );
        }
        other => panic!("unexpected variant: {other:?}"),
    }
}


