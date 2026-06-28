use std::io::{self, BufRead, Write};
use std::sync::OnceLock;

use ledgerr_mcp::mcp_adapter;
use serde_json::{json, Value};

#[cfg(feature = "b00t")]
use ledgerr_mcp_core::McpProviderRegistry;
#[cfg(feature = "b00t")]
use ledgerr_mcp::providers::definitions::register_default_providers;

fn main() {
    // Pre-warm: construct and spawn the service actor on startup so all
    // tool calls route through the channel-based gate system.  The raw
    // service reference stays available for the existing mcp_adapter path;
    // future phases will retire the raw adapter in favor of actor dispatch.
    let _ = global_raw_service();

    #[cfg(feature = "b00t")]
    initialize_providers();

    serve(io::stdin().lock(), io::stdout());
}

#[cfg(feature = "b00t")]
fn initialize_providers() {
    let mut registry = McpProviderRegistry::new();
    register_default_providers(&mut registry, None, None);
    let results = registry.initialize_all();
    for (name, result) in &results {
        match result {
            Ok(info) => tracing::info!(provider = %name, tools = info.tools.len(), "external provider registered"),
            Err(e) => tracing::warn!(provider = %name, error = %e, "external provider init failed"),
        }
    }
    // Store in both the local static (for handle_external_tool) and
    // the mcp_adapter global (for tools/list merging).
    ledgerr_mcp::mcp_adapter::set_global_provider_registry(registry);
}



fn serve<R: BufRead, W: Write>(reader: R, mut writer: W) {
    for line in reader.lines() {
        let Ok(raw) = line else { continue };
        let Ok(request) = serde_json::from_str::<Value>(&raw) else {
            continue;
        };
        if let Some(response) = handle_request(request) {
            if let Ok(serialized) = serde_json::to_string(&response) {
                let _ = writeln!(writer, "{serialized}");
                let _ = writer.flush();
            }
        }
    }
}

fn handle_request(request: Value) -> Option<Value> {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");

    match method {
        "initialize" => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "ledgerr-mcp",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        })),
        "notifications/initialized" => None,
        "tools/list" => {
            let tools = mcp_adapter::tool_descriptors();
            Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "tools": tools }
            }))
        }
        "tools/call" => {
            let params = request.get("params").cloned().unwrap_or(Value::Null);
            let tool_name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let result = match tool_name {
                mcp_adapter::DOCUMENTS_TOOL => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_documents_tool(global_raw_service(), &arguments)
                }
                mcp_adapter::REVIEW_TOOL => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_review_tool(global_raw_service(), &arguments)
                }
                mcp_adapter::RECONCILIATION_TOOL => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_reconciliation_tool(global_raw_service(), &arguments)
                }
                mcp_adapter::WORKFLOW_TOOL => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_workflow_tool(global_raw_service(), &arguments)
                }
                mcp_adapter::AUDIT_TOOL => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_audit_tool(global_raw_service(), &arguments)
                }
                mcp_adapter::TAX_TOOL => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_tax_tool(global_raw_service(), &arguments)
                }
                mcp_adapter::ONTOLOGY_TOOL => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_ontology_tool(global_raw_service(), &arguments)
                }
                mcp_adapter::XERO_TOOL => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_xero_tool(global_raw_service(), &arguments)
                }
                mcp_adapter::EVIDENCE_TOOL => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_evidence_tool(global_raw_service(), &arguments)
                }
                "l3dg3rr_list_accounts" => mcp_adapter::handle_list_accounts(global_raw_service()),
                "l3dg3rr_get_pipeline_status" => {
                    mcp_adapter::handle_pipeline_status(true, true, true, Vec::new())
                }
                "proxy_docling_ingest_pdf" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_ingest_pdf(
                        global_raw_service(),
                        &arguments,
                        Some(format!("mcp-call-{id}")),
                    )
                }
                "proxy_rustledger_ingest_statement_rows" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_ingest_statement_rows(
                        global_raw_service(),
                        &arguments,
                        Some(format!("mcp-call-{id}")),
                    )
                }
                "l3dg3rr_get_raw_context" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_get_raw_context(global_raw_service(), &arguments)
                }
                "l3dg3rr_ontology_query_path" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_ontology_query_path(global_raw_service(), &arguments)
                }
                "l3dg3rr_ontology_export_snapshot" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_ontology_export_snapshot(global_raw_service(), &arguments)
                }
                "l3dg3rr_validate_reconciliation" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::dispatch_reconciliation(global_raw_service(), "validate", &arguments)
                }
                "l3dg3rr_reconcile_postings" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::dispatch_reconciliation(global_raw_service(), "reconcile", &arguments)
                }
                "l3dg3rr_commit_guarded" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::dispatch_reconciliation(global_raw_service(), "commit", &arguments)
                }
                "l3dg3rr_hsm_transition" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::dispatch_hsm(global_raw_service(), "transition", &arguments)
                }
                "l3dg3rr_hsm_status" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::dispatch_hsm(global_raw_service(), "status", &arguments)
                }
                "l3dg3rr_hsm_resume" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::dispatch_hsm(global_raw_service(), "resume", &arguments)
                }
                "l3dg3rr_event_history" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_event_history(global_raw_service(), &arguments)
                }
                "l3dg3rr_event_replay" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_event_replay(global_raw_service(), &arguments)
                }
                "l3dg3rr_tax_assist" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_tax_assist(global_raw_service(), &arguments)
                }
                "l3dg3rr_tax_evidence_chain" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_tax_evidence_chain(global_raw_service(), &arguments)
                }
                "l3dg3rr_tax_ambiguity_review" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_tax_ambiguity_review(global_raw_service(), &arguments)
                }
                "l3dg3rr_classify_ingested" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_classify_ingested(global_raw_service(), &arguments)
                }
                "l3dg3rr_query_flags" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_query_flags(global_raw_service(), &arguments)
                }
                "l3dg3rr_query_audit_log" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_query_audit_log(global_raw_service(), &arguments)
                }
                "l3dg3rr_classify_transaction" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_classify_transaction(global_raw_service(), &arguments)
                }
                "l3dg3rr_reconcile_excel_classification" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_reconcile_excel_classification(global_raw_service(), &arguments)
                }
                "l3dg3rr_get_schedule_summary" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_get_schedule_summary(global_raw_service(), &arguments)
                }
                "l3dg3rr_export_cpa_workbook" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_export_cpa_workbook(global_raw_service(), &arguments)
                }
                "l3dg3rr_ontology_upsert_entities" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_ontology_upsert_entities(global_raw_service(), &arguments)
                }
                "l3dg3rr_ontology_upsert_edges" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_ontology_upsert_edges(global_raw_service(), &arguments)
                }
                "l3dg3rr_plugin_info" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_workflow_tool(
                        global_raw_service(),
                        &json!({ "action": "plugin_info", "subcommand": arguments.get("subcommand").cloned().unwrap_or(Value::String("check".to_string())) }),
                    )
                }
                _ => {
                    #[cfg(feature = "b00t")] {
                        // Registry is accessed internally by handle_external_tool
                        // via mcp_adapter's GLOBAL_PROVIDER_REGISTRY.
                        let ext_args = params.get("arguments").cloned().unwrap_or(Value::Null);
                        let dummy = ledgerr_mcp_core::McpProviderRegistry::new();
                        mcp_adapter::handle_external_tool(&dummy, tool_name, &ext_args)
                    }
                    #[cfg(not(feature = "b00t"))] {
                        mcp_adapter::unknown_tool_result(tool_name)
                    }
                }
            };
            Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": result
            }))
        }
        _ => Some(mcp_adapter::protocol_method_not_found(id, method)),
    }
}

/// Build the service, spawn an actor, and leak a raw reference for the adapter path.
fn build_service() -> (&'static ledgerr_mcp::TurboLedgerService, ledgerr_mcp::actor::ServiceHandle) {
    let manifest = std::env::var("LEDGERR_MCP_MANIFEST").unwrap_or_else(|_| {
        "[session]\nworkbook_path=\"tax-ledger.xlsx\"\nactive_year=2023\n\n[accounts]\nWF-BH-CHK = { institution = \"Wells Fargo\", type = \"checking\", currency = \"USD\" }\n".to_string()
    });
    let service = ledgerr_mcp::TurboLedgerService::from_manifest_str(&manifest)
        .expect("default manifest must parse");
    let handle = service.spawn_actor();
    let raw = Box::new(ledgerr_mcp::TurboLedgerService::from_manifest_str(&manifest)
        .expect("default manifest must parse"));
    let leaked = Box::leak(raw);
    (leaked, handle)
}

fn global_raw_service() -> &'static ledgerr_mcp::TurboLedgerService {
    static PAIR: OnceLock<(&'static ledgerr_mcp::TurboLedgerService, ledgerr_mcp::actor::ServiceHandle)> = OnceLock::new();
    PAIR.get_or_init(|| build_service()).0
}
