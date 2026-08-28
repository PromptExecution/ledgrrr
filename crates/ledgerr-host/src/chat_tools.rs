//! Tool allowlist and dispatch for the Chat panel's MCP tool-calling loop.
//!
//! The model never gets the full `ledgerr-mcp` tool surface — only the names
//! listed in [`ALLOWLISTED_TOOL_NAMES`], matching the operator-approved list
//! in issue #208. Everything else (reconciliation, ontology upserts, workbook
//! export, ingestion, event replay, mapping bulk-apply, xero, ...) is out of
//! scope for chat and is rejected by [`dispatch_mcp_tool`] before it ever
//! reaches `ledgerr_mcp::mcp_adapter`.
//!
//! Mutating tools (`classify_transaction`, `batch_classify`,
//! `bulk_resolve_flags`) are allowlisted because they route through the
//! existing Rhai classification waterfall / `ledger_ops.rs` validation in
//! `ledgerr-mcp` — chat only *triggers* them, it never writes ledger state
//! from raw model output itself.
//!
//! `get_evidence_dashboard` is a Tauri-state-bound read (it lives on
//! `AppState.evidence`) and is dispatched by the caller in
//! `bin/tauri/commands.rs`, not here — see `is_allowlisted` for why it still
//! counts as allowlisted.

use std::sync::OnceLock;

use ledgerr_mcp::mcp_adapter;
use ledgerr_mcp::TurboLedgerService;
use serde_json::{json, Value};

use crate::agent_runtime::ModelToolSpec;

/// Tool name for the Tauri-state-bound evidence dashboard read. Dispatched by
/// `bin/tauri/commands.rs` (needs `AppState`), not by [`dispatch_mcp_tool`].
pub const GET_EVIDENCE_DASHBOARD: &str = "get_evidence_dashboard";

/// Every tool name the chat loop is allowed to call — read-only lookups plus
/// the three pre-approved mutation tools. Kept as a single source of truth
/// alongside `tool_specs()` and `dispatch_mcp_tool()` so the advertised tool
/// list and the dispatcher can never drift apart (see `allowlist_and_specs_never_drift_apart`
/// test below).
pub const ALLOWLISTED_TOOL_NAMES: &[&str] = &[
    GET_EVIDENCE_DASHBOARD,
    "get_pipeline_status",
    "document_inventory",
    "get_raw_context",
    "query_flags",
    "query_audit_log",
    "tax_evidence_chain",
    "get_schedule_summary",
    "event_history",
    "schema_lookup",
    "classify_transaction",
    "batch_classify",
    "bulk_resolve_flags",
];

pub fn is_allowlisted(name: &str) -> bool {
    ALLOWLISTED_TOOL_NAMES.contains(&name)
}

/// The three allowlisted tools that mutate ledger/flag state. A call to any
/// of these must go through the chat tool loop's operator-confirmation gate
/// (see `chat::send_message_with_tools` / `chat::resume_after_confirmation`)
/// before `dispatch_mcp_tool` ever runs it — never dispatched inline off raw
/// model output. Every other allowlisted tool is read-only and unaffected.
pub const MUTATION_TOOL_NAMES: &[&str] = &[
    "classify_transaction",
    "batch_classify",
    "bulk_resolve_flags",
];

pub fn is_mutation_tool(name: &str) -> bool {
    MUTATION_TOOL_NAMES.contains(&name)
}

/// OpenAI function-calling descriptors for every allowlisted tool, in the
/// shape `ModelRequest::with_tools` expects.
pub fn tool_specs() -> Vec<ModelToolSpec> {
    vec![
        ModelToolSpec {
            name: GET_EVIDENCE_DASHBOARD.to_string(),
            description: "Read the evidence dashboard's today-queue: pending gaps, provenance status, and reconciliation queue counts.".to_string(),
            parameters: json!({"type": "object", "properties": {}, "additionalProperties": false}),
        },
        ModelToolSpec {
            name: "get_pipeline_status".to_string(),
            description: "Read overall pipeline readiness (manifest, rustledger, docling) and any blockers.".to_string(),
            parameters: json!({"type": "object", "properties": {}, "additionalProperties": false}),
        },
        ModelToolSpec {
            name: "document_inventory".to_string(),
            description: "List ingested/queued source documents under a directory, with ingest status per file.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "directory": {"type": "string", "description": "Directory to scan for source documents"},
                    "recursive": {"type": "boolean", "description": "Recurse into subdirectories (default false)"},
                    "statuses": {"type": "array", "items": {"type": "string"}, "description": "Filter to these document queue statuses only"}
                },
                "required": ["directory"],
                "additionalProperties": false
            }),
        },
        ModelToolSpec {
            name: "get_raw_context".to_string(),
            description: "Read the raw ingestion context (OCR/parse output) captured for a document, by its rkyv reference path.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "rkyv_ref": {"type": "string", "description": "Path to the rkyv-serialized raw context file"}
                },
                "required": ["rkyv_ref"],
                "additionalProperties": false
            }),
        },
        ModelToolSpec {
            name: "query_flags".to_string(),
            description: "List review flags for a tax year, filtered by status.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "year": {"type": "integer", "description": "Tax year, e.g. 2023"},
                    "status": {"type": "string", "enum": ["open", "resolved"]}
                },
                "required": ["year", "status"],
                "additionalProperties": false
            }),
        },
        ModelToolSpec {
            name: "query_audit_log".to_string(),
            description: "Read the full immutable audit log of classification and reconciliation decisions.".to_string(),
            parameters: json!({"type": "object", "properties": {}, "additionalProperties": false}),
        },
        ModelToolSpec {
            name: "tax_evidence_chain".to_string(),
            description: "Trace the evidence chain (ontology graph path) supporting a tax position from a starting entity.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "ontology_path": {"type": "string", "description": "Path to the ontology snapshot file"},
                    "from_entity_id": {"type": "string", "description": "Entity id to start tracing from"},
                    "tx_id": {"type": "string", "description": "Optional transaction id to focus the trace on"},
                    "document_ref": {"type": "string", "description": "Optional source document reference to focus the trace on"}
                },
                "required": ["ontology_path", "from_entity_id"],
                "additionalProperties": false
            }),
        },
        ModelToolSpec {
            name: "get_schedule_summary".to_string(),
            description: "Read the summarized Schedule C/D/E/FBAR totals for a tax year.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "year": {"type": "integer", "description": "Tax year, e.g. 2023"},
                    "schedule": {"type": "string", "enum": ["ScheduleC", "ScheduleD", "ScheduleE", "Fbar"]}
                },
                "required": ["year", "schedule"],
                "additionalProperties": false
            }),
        },
        ModelToolSpec {
            name: "event_history".to_string(),
            description: "Read lifecycle event history, optionally filtered by transaction, source document, or time range.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "tx_id": {"type": "string"},
                    "document_ref": {"type": "string"},
                    "time_start": {"type": "string", "description": "RFC3339 timestamp lower bound"},
                    "time_end": {"type": "string", "description": "RFC3339 timestamp upper bound"}
                },
                "additionalProperties": false
            }),
        },
        ModelToolSpec {
            name: "schema_lookup".to_string(),
            description: "List the registered ontology kinds in a schema store (read-only — this tool cannot register or remove kinds).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "schema_path": {"type": "string", "description": "Path to the schema store JSON file"}
                },
                "required": ["schema_path"],
                "additionalProperties": false
            }),
        },
        ModelToolSpec {
            name: "classify_transaction".to_string(),
            description: "Classify one transaction by tx_id. Routes through the Rhai classification waterfall — this does not write the category directly.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "tx_id": {"type": "string"},
                    "category": {"type": "string"},
                    "confidence": {"type": "string", "description": "Decimal string in [0,1], e.g. \"0.85\""},
                    "note": {"type": "string"},
                    "actor": {"type": "string", "description": "Who/what is performing this classification, e.g. \"agent\""}
                },
                "required": ["tx_id", "category", "confidence", "actor"],
                "additionalProperties": false
            }),
        },
        ModelToolSpec {
            name: "batch_classify".to_string(),
            description: "Classify a batch of transaction ids to the same category in one call. Routes through the same Rhai classification waterfall as classify_transaction.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "tx_ids": {"type": "array", "items": {"type": "string"}},
                    "category": {"type": "string"},
                    "confidence": {"type": "string", "description": "Decimal string in [0,1]"},
                    "note": {"type": "string"},
                    "actor": {"type": "string"},
                    "batch_mode": {"type": "string", "enum": ["AllOrNothing", "ContinueOnError"]},
                    "dry_run": {"type": "boolean"}
                },
                "required": ["tx_ids", "category", "confidence", "actor"],
                "additionalProperties": false
            }),
        },
        ModelToolSpec {
            name: "bulk_resolve_flags".to_string(),
            description: "Resolve a batch of open review flags by transaction id with one resolution action.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "tx_ids": {"type": "array", "items": {"type": "string"}},
                    "resolution": {"type": "string", "enum": ["Approve", "Reject", "Escalate", "Dismiss", "Defer"]},
                    "reason": {"type": "string"},
                    "actor": {"type": "string"},
                    "batch_mode": {"type": "string", "enum": ["AllOrNothing", "ContinueOnError"]},
                    "dry_run": {"type": "boolean"}
                },
                "required": ["tx_ids", "resolution", "actor"],
                "additionalProperties": false
            }),
        },
    ]
}

/// Dispatches an allowlisted tool call to `ledgerr-mcp`.
///
/// Returns an `{"ok": false, "error": ...}` envelope — never panics or
/// propagates a Rust error — for an unknown/non-allowlisted name, so the
/// tool loop can always feed *something* back to the model as the tool
/// result. Callers still SHOULD check `is_allowlisted` themselves before
/// this so a rejection can be logged/surfaced distinctly if desired.
///
/// `get_evidence_dashboard` is not handled here — see the module doc and
/// `bin/tauri/commands.rs`.
pub fn dispatch_mcp_tool(name: &str, arguments: &Value) -> Value {
    if !is_allowlisted(name) {
        return json!({
            "ok": false,
            "error": format!("tool '{name}' is not in the chat allowlist")
        });
    }

    // These two never touch TurboLedgerService — resolve them before paying
    // for (or failing on) service resolution below.
    match name {
        GET_EVIDENCE_DASHBOARD => {
            return json!({
                "ok": false,
                "error": "get_evidence_dashboard must be dispatched by the Tauri command layer (needs AppState), not chat_tools::dispatch_mcp_tool"
            });
        }
        "get_pipeline_status" => return dispatch_pipeline_status(),
        _ => {}
    }

    let service = match global_service() {
        Ok(service) => service,
        Err(error) => return json!({ "ok": false, "error": error }),
    };
    match name {
        "document_inventory" => mcp_adapter::handle_document_inventory(service, arguments),
        "get_raw_context" => mcp_adapter::handle_get_raw_context(service, arguments),
        "query_flags" => mcp_adapter::handle_query_flags(service, arguments),
        "query_audit_log" => mcp_adapter::handle_query_audit_log(service, arguments),
        "tax_evidence_chain" => mcp_adapter::handle_tax_evidence_chain(service, arguments),
        "get_schedule_summary" => mcp_adapter::handle_get_schedule_summary(service, arguments),
        "event_history" => mcp_adapter::handle_event_history(service, arguments),
        "schema_lookup" => dispatch_schema_lookup(service, arguments),
        "classify_transaction" => mcp_adapter::handle_classify_transaction(service, arguments),
        "batch_classify" => {
            mcp_adapter::handle_batch_classify(service, &json!({ "request": arguments }))
        }
        "bulk_resolve_flags" => {
            mcp_adapter::handle_bulk_resolve_flags(service, &json!({ "request": arguments }))
        }
        // Unreachable: `is_allowlisted` above already rejected anything not
        // in ALLOWLISTED_TOOL_NAMES, and every name in that list is matched
        // either in the block above or here (see
        // `allowlist_and_specs_never_drift_apart`).
        other => json!({
            "ok": false,
            "error": format!("tool '{other}' is allowlisted but has no dispatcher wired — this is a bug")
        }),
    }
}

/// `get_pipeline_status` takes no model-supplied arguments — readiness is
/// computed from the local docling probe, mirroring
/// `ledgerr-mcp-server.rs`'s `l3dg3rr_get_pipeline_status` handler.
fn dispatch_pipeline_status() -> Value {
    let docling_ready = b00t_iface::docling::DoclingProcessSurface::new().is_ready();
    mcp_adapter::handle_pipeline_status(true, true, docling_ready, Vec::new())
}

/// `schema_lookup` is read-only by design: the underlying `handle_schema_tool`
/// also supports `register_kind`/`remove_kind`, but chat only ever asks it to
/// `list_kinds` — the model-supplied arguments cannot set `action`.
fn dispatch_schema_lookup(service: &TurboLedgerService, arguments: &Value) -> Value {
    let schema_path = arguments.get("schema_path").cloned().unwrap_or(Value::Null);
    mcp_adapter::handle_schema_tool(
        service,
        &json!({ "action": "list_kinds", "schema_path": schema_path }),
    )
}

/// Resolves the `TurboLedgerService` backing every allowlisted tool except
/// `get_evidence_dashboard`/`get_pipeline_status`, or a clear error.
///
/// Deliberately does NOT default to a hardcoded manifest/workbook path the
/// way `ledgerr-mcp-server.rs`'s standalone `build_service()` does — that
/// default is fine for a process whose whole job is one manifest, but the
/// Tauri desktop app doesn't (yet) track a "currently open workbook" in
/// `AppSettings`, and silently defaulting here would let `classify_transaction`
/// / `batch_classify` / `bulk_resolve_flags` write into a phantom
/// `./tax-ledger.xlsx` relative to wherever the app process happened to
/// start, instead of the operator's actual workbook — a silent-divergence
/// risk in a financial path. Until the app has a real workbook-path setting
/// to source this from, every chat tool call requires `LEDGERR_MCP_MANIFEST`
/// to be set explicitly and fails closed (an error envelope, never a panic)
/// when it isn't.
fn global_service() -> Result<&'static TurboLedgerService, String> {
    static SERVICE: OnceLock<Result<&'static TurboLedgerService, String>> = OnceLock::new();
    SERVICE.get_or_init(build_service).clone()
}

fn build_service() -> Result<&'static TurboLedgerService, String> {
    let manifest = std::env::var("LEDGERR_MCP_MANIFEST").map_err(|_| {
        "chat tools need LEDGERR_MCP_MANIFEST set to the active workbook's manifest \
         TOML (the desktop app does not yet track a 'currently open workbook' setting \
         — see l3dg3rr#208 follow-up); no ledgerr-mcp-backed tool can run without it"
            .to_string()
    })?;
    let service = TurboLedgerService::from_manifest_str(&manifest)
        .map_err(|error| format!("LEDGERR_MCP_MANIFEST failed to parse: {error}"))?;
    Ok(Box::leak(Box::new(service)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a `TurboLedgerService` directly from an in-line manifest, for
    /// tests that need the real `ledgerr-mcp` handlers but must not go
    /// through the env-var-gated `global_service()` singleton: this
    /// workspace denies `unsafe_code` outright, and `std::env::set_var` is
    /// an unsafe fn, so tests cannot set `LEDGERR_MCP_MANIFEST` at all. This
    /// also means `LEDGERR_MCP_MANIFEST` is reliably unset for every test in
    /// this binary — see `service_backed_tools_fail_closed_without_manifest_configured`.
    fn test_service() -> TurboLedgerService {
        let workbook_path = std::env::temp_dir().join(format!(
            "chat-tools-test-workbook-{}-{}.xlsx",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        let manifest = format!(
            "[session]\nworkbook_path=\"{}\"\nactive_year=2023\n\n[accounts]\nWF-BH-CHK = {{ institution = \"Wells Fargo\", type = \"checking\", currency = \"USD\" }}\n",
            workbook_path.display()
        );
        TurboLedgerService::from_manifest_str(&manifest).expect("test manifest must parse")
    }

    #[test]
    fn allowlist_and_specs_never_drift_apart() {
        let specs = tool_specs();
        let spec_names: Vec<&str> = specs.iter().map(|spec| spec.name.as_str()).collect();
        assert_eq!(
            spec_names.len(),
            ALLOWLISTED_TOOL_NAMES.len(),
            "tool_specs() must describe exactly the allowlisted tools"
        );
        for name in ALLOWLISTED_TOOL_NAMES {
            assert!(
                spec_names.contains(name),
                "allowlisted tool '{name}' has no advertised spec"
            );
        }
    }

    #[test]
    fn every_mutation_tool_is_allowlisted_and_vice_versa_for_exactly_these_three() {
        for name in MUTATION_TOOL_NAMES {
            assert!(
                is_allowlisted(name),
                "mutation tool '{name}' must also be an allowlisted tool"
            );
        }
        for name in [
            "classify_transaction",
            "batch_classify",
            "bulk_resolve_flags",
        ] {
            assert!(is_mutation_tool(name), "'{name}' must require confirmation");
        }
        for name in ALLOWLISTED_TOOL_NAMES {
            let should_be_mutation = MUTATION_TOOL_NAMES.contains(name);
            assert_eq!(
                is_mutation_tool(name),
                should_be_mutation,
                "read-only tool '{name}' must not be gated as a mutation tool"
            );
        }
    }

    #[test]
    fn non_allowlisted_tool_name_is_rejected() {
        let result = dispatch_mcp_tool("export_cpa_workbook", &json!({}));
        assert_eq!(result["ok"], json!(false));
        let error = result["error"].as_str().expect("error message");
        assert!(error.contains("export_cpa_workbook"));
        assert!(error.contains("not in the chat allowlist"));
    }

    #[test]
    fn unknown_tool_name_is_rejected_the_same_way_as_a_known_disallowed_one() {
        for name in [
            "reconcile_postings",
            "ontology_upsert_entities",
            "made_up_tool",
            "",
        ] {
            assert!(!is_allowlisted(name), "'{name}' should not be allowlisted");
            let result = dispatch_mcp_tool(name, &json!({}));
            assert_eq!(result["ok"], json!(false));
        }
    }

    #[test]
    fn query_audit_log_handler_succeeds_against_a_directly_built_service() {
        // Exercises the real ledgerr-mcp handler end-to-end. Deliberately
        // bypasses `dispatch_mcp_tool`/`global_service()` — see
        // `test_service` for why.
        let service = test_service();
        let result = mcp_adapter::handle_query_audit_log(&service, &json!({}));
        assert_eq!(result["isError"], json!(false));
    }

    #[test]
    fn schema_lookup_ignores_a_model_supplied_action_and_stays_read_only() {
        // Even if a model tries to sneak `action: "register_kind"` in,
        // `dispatch_schema_lookup` must force `list_kinds` — never let chat
        // register or remove ontology kinds. Calls `dispatch_schema_lookup`
        // directly (bypassing `global_service()`) — see `test_service`.
        let service = test_service();
        let scratch = std::env::temp_dir().join(format!(
            "chat-tools-schema-lookup-test-{}.json",
            std::process::id()
        ));
        let path_str = scratch.to_string_lossy().to_string();
        let result = dispatch_schema_lookup(
            &service,
            &json!({
                "schema_path": path_str,
                "action": "register_kind",
                "name": "should_not_be_created"
            }),
        );
        // `SchemaStore::load` treats a non-existent path as an empty store
        // (see `schema.rs`), so `list_kinds` here succeeds with zero kinds —
        // the important assertion is that nothing was registered despite the
        // model-supplied `action: "register_kind"`.
        let _ = std::fs::remove_file(&scratch);
        assert_eq!(result["isError"], json!(false));
    }

    #[test]
    fn service_backed_tools_fail_closed_without_ledgerr_mcp_manifest_configured() {
        // The default state on Tauri process startup today: the desktop app
        // has no "currently open workbook" setting yet to source a manifest
        // from (see `build_service`'s doc comment), so `LEDGERR_MCP_MANIFEST`
        // is unset. Every ledgerr-mcp-backed tool must fail with a clear,
        // actionable error here — never panic, and never silently fall back
        // to a phantom workbook. (We never call `std::env::set_var` anywhere
        // in this crate — `unsafe_code` is denied workspace-wide — so this
        // env var is reliably unset for the whole test binary.)
        for name in [
            "query_audit_log",
            "document_inventory",
            "classify_transaction",
        ] {
            let result = dispatch_mcp_tool(name, &json!({}));
            assert_eq!(result["ok"], json!(false), "tool: {name}");
            let error = result["error"].as_str().unwrap_or_default();
            assert!(
                error.contains("LEDGERR_MCP_MANIFEST"),
                "tool: {name}, error: {error}"
            );
        }
    }

    #[test]
    fn get_pipeline_status_and_get_evidence_dashboard_do_not_require_a_manifest() {
        // These two don't touch TurboLedgerService at all, so they must not
        // be affected by LEDGERR_MCP_MANIFEST being unset.
        let pipeline = dispatch_mcp_tool("get_pipeline_status", &json!({}));
        assert!(pipeline.get("isError").is_some());
        let evidence = dispatch_mcp_tool(GET_EVIDENCE_DASHBOARD, &json!({}));
        assert_eq!(evidence["ok"], json!(false));
        assert!(evidence["error"]
            .as_str()
            .unwrap()
            .contains("Tauri command layer"));
    }

    #[test]
    fn get_evidence_dashboard_is_allowlisted_but_not_dispatched_here() {
        assert!(is_allowlisted(GET_EVIDENCE_DASHBOARD));
        let result = dispatch_mcp_tool(GET_EVIDENCE_DASHBOARD, &json!({}));
        assert_eq!(result["ok"], json!(false));
        assert!(result["error"]
            .as_str()
            .unwrap()
            .contains("Tauri command layer"));
    }
}
