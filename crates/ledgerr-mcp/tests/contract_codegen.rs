use std::fs;
use std::path::PathBuf;

use ledgerr_mcp::contract::{
    self, DocumentsArgs, WorkflowArgs, AUDIT_TOOL, DOCUMENTS_TOOL, ONTOLOGY_TOOL,
    RECONCILIATION_TOOL, REVIEW_TOOL, TAX_TOOL, WORKFLOW_TOOL,
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
    ] {
        let schema = contract::tool_input_schema(tool);
        assert!(
            schema.is_object(),
            "schema for {tool} should serialize as an object"
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


