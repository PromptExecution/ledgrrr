use ledger_core::ingest::TransactionInput;

// DOC-01: MCP transport boundary should expose the reduced top-level capability surface.
#[test]
fn doc_01_mcp_boundary_tool_catalog_exposes_reduced_top_level_surface() {
    let tools = ledgerr_mcp::mcp_adapter::tool_names();

    assert_eq!(tools.len(), 10);
    assert!(tools.contains(&"ledgerr_documents".to_string()));
    assert!(tools.contains(&"ledgerr_review".to_string()));
    assert!(tools.contains(&"ledgerr_reconciliation".to_string()));
    assert!(tools.contains(&"ledgerr_workflow".to_string()));
    assert!(tools.contains(&"ledgerr_audit".to_string()));
    assert!(tools.contains(&"ledgerr_tax".to_string()));
    assert!(tools.contains(&"ledgerr_ontology".to_string()));
    assert!(tools.contains(&"ledgerr_xero".to_string()));
    assert!(tools.contains(&"ledgerr_evidence".to_string()));
    assert!(tools.contains(&"ledgerr_focus".to_string()));
    // MCP lifecycle methods (tools/list, tools/call) are JSON-RPC methods, not tools —
    // they must NOT appear in the tool catalog per MCP spec.
    assert!(!tools.contains(&"tools/list".to_string()));
    assert!(!tools.contains(&"tools/call".to_string()));
}

// DOC-02 (D-02, D-04): Canonical rows + provenance fields must be deterministic.
#[test]
fn doc_02_normalized_rows_include_canonical_and_provenance_fields() {
    let rows = vec![TransactionInput {
        account_id: "WF-BH-CHK".to_string(),
        date: "2023-01-15".to_string(),
        amount: "-42.11".to_string(),
        description: "Coffee Shop".to_string(),
        source_ref: "2023-taxes/WF--BH-CHK--2023-01--statement.rkyv".to_string(),
    }];

    let normalized = ledgerr_mcp::mcp_adapter::rows_to_json_with_provenance(
        "rustledger",
        "ingest_statement_rows",
        Some("1.0.0"),
        Some("call-001"),
        rows,
    );

    assert_eq!(normalized.len(), 1);
    let entry = &normalized[0];

    assert!(entry.get("account").is_some());
    assert!(entry.get("date").is_some());
    assert!(entry.get("amount").is_some());
    assert!(entry.get("description").is_some());
    assert!(entry.get("currency").is_some());
    assert!(entry.get("source_ref").is_some());
    assert!(entry.get("provider").is_some());
    assert!(entry.get("backend_tool").is_some());
    assert!(entry.get("backend_version").is_some());
    assert!(entry.get("backend_call_id").is_some());
}

// DOC-02 (D-04): Stable enum-like status + blockers + next_hint contract.
#[test]
fn doc_02_pipeline_status_shape_is_deterministic_and_concise() {
    let status = ledgerr_mcp::mcp_adapter::get_pipeline_status(
        true,
        true,
        false,
        vec!["docling_unreachable".to_string()],
    );

    assert_eq!(status.status, "blocked");
    assert_eq!(status.blockers, vec!["docling_unreachable".to_string()]);
    assert_eq!(status.next_hint, "resolve_blockers_then_retry");
}

// DOC-01: documents surface remains explicitly advertised in the reduced catalog.
#[test]
fn doc_01_documents_tool_name_is_exact_and_callable_target() {
    let tools = ledgerr_mcp::mcp_adapter::tool_names();
    assert!(tools.iter().any(|name| name == "ledgerr_documents"));
}
