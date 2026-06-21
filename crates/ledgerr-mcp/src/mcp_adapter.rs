//! Legacy MCP adapter — direct TurboLedgerService dispatch.
//!
//! This module contains the original dispatch layer where each MCP tool
//! call goes directly through `&TurboLedgerService` method calls.  It is
//! gated behind the `legacy` feature flag for historical reference.
//!
//! New code should route through `actor::ServiceHandle` instead.
//!
//! ──ℹ── Historical snapshot ──ℹ──
//! This code was the primary tool dispatch from 2025-Q3 through 2026-Q2.
//! It was replaced by the actor/gate channel system in PRD-7 Phase 0-4.

use std::collections::BTreeMap;
use std::path::PathBuf;
#[cfg(feature = "b00t")]
use std::sync::OnceLock;

use ledger_core::ingest::{deterministic_tx_id, TransactionInput};
#[cfg(feature = "b00t")]
use ledgerr_mcp_core::ToolDescriptor;
use serde::Serialize;
use serde_json::{json, Value};

use crate::{
    contract::{
        self, AuditArgs, DocumentsArgs, EvidenceArgs, OntologyArgs, ReconciliationArgs, ReviewArgs,
        TaxArgs, WorkflowArgs,
        BatchClassifyRequest, BatchResolveFlagsRequest, ApplyMappingBulkRequest,
        BatchItemStatus,
    },
        FetchQueueRequest,
        QueueItemType, QueueStatus,
    ClassifyIngestedRequest, ClassifyTransactionRequest, DocumentInventoryRequest,
    DocumentQueueStatusRequest, EventHistoryFilter, ExportCpaWorkbookRequest, FlagStatusRequest,
    GetRawContextRequest, GetScheduleSummaryRequest, HsmResumeRequest, HsmStatusRequest,
    HsmTransitionRequest, IngestPdfRequest, IngestStatementRowsRequest, ListAccountsRequest,
    OntologyExportSnapshotRequest, OntologyQueryPathRequest, OntologyUpsertEdgesRequest,
    OntologyUpsertEntitiesRequest, QueryAuditLogRequest, QueryFlagsRequest, QueryTransactionsRequest,
    ReconcileExcelClassificationRequest, ReconciliationStageRequest, ReplayLifecycleRequest,
    RunRhaiRuleRequest, SampleTxRequest, ScheduleKindRequest, TaxAmbiguityReviewRequest,
    TaxAssistRequest, TaxEvidenceChainRequest, ToolError, TurboLedgerService, TurboLedgerTools,
};

// Global provider registry — set once at startup by the server binary.
// Feature-gated because the registry type comes from ledgerr-mcp-core (b00t feature).
#[cfg(feature = "b00t")]
static GLOBAL_PROVIDER_REGISTRY: OnceLock<ledgerr_mcp_core::McpProviderRegistry> = OnceLock::new();

/// Register the global provider registry for external tool dispatch.
/// Called once at startup by the server binary.
#[cfg(feature = "b00t")]
pub fn set_global_provider_registry(registry: ledgerr_mcp_core::McpProviderRegistry) {
    let _ = GLOBAL_PROVIDER_REGISTRY.set(registry);
}

/// Return all external provider tool descriptors for inclusion in tools/list.
#[cfg(feature = "b00t")]
fn external_tool_descriptors() -> Vec<Value> {
    let Some(registry) = GLOBAL_PROVIDER_REGISTRY.get() else {
        return Vec::new();
    };
    registry
        .all_tool_descriptors()
        .into_iter()
        .map(|td: ToolDescriptor| json!({ "name": td.name, "inputSchema": td.input_schema }))
        .collect()
}

#[cfg(not(feature = "b00t"))]
fn external_tool_descriptors() -> Vec<Value> {
    Vec::new()
}

// Public re-exports are always available (they're just constants).
pub use crate::contract::{
    AUDIT_TOOL, DOCUMENTS_TOOL, EVIDENCE_TOOL, FOCUS_TOOL, ONTOLOGY_TOOL, RECONCILIATION_TOOL,
    REVIEW_TOOL, TAX_TOOL, WORKFLOW_TOOL, XERO_TOOL,
};

// ── Default dispatch ──────────────────────────────────────────────────────────
// These always-compiled functions provide the core dispatch surface.  When the
// `legacy` feature is active they are joined by the original direct-dispatch
// functions (tool_names, tool_names_for, tool_input_schema, handle_*_tool, etc.)
// which are compiled alongside these.

/// Non-legacy tool descriptors: returns built-in + external provider tools.
pub fn tool_descriptors() -> Vec<Value> {
    let mut tools: Vec<Value> = BUILTIN_TOOL_NAMES
        .iter()
        .map(|name| {
            let schema = builtin_tool_input_schema(name);
            json!({ "name": name, "inputSchema": schema })
        })
        .collect();
    let ext_tools = external_tool_descriptors();
    for ext in ext_tools {
        let ext_name = ext["name"].as_str().unwrap_or("");
        if !tools.iter().any(|t| t["name"].as_str() == Some(ext_name)) {
            tools.push(ext);
        }
    }
    tools
}

pub fn handle_focus_tool(arguments: &Value) -> Value {
    use crate::contract::parse_focus;
    use crate::focus_tool::{self, FocusToolInput, FocusToolRecord};
    use std::io::Write;

    let request = match parse_focus(arguments) {
        Ok(r) => r,
        Err(err) => return error_envelope(&err),
    };

    match request {
        crate::contract::FocusArgs::AppendFocusRecord {
            billing_account_id,
            service_name,
            billed_cost,
            effective_cost,
            experiment_id,
            variant,
            agent_id,
        } => {
            let record = FocusToolRecord {
                billing_account_id,
                service_name,
                billed_cost,
                effective_cost,
                experiment_id: experiment_id.clone(),
                variant: variant.clone(),
                agent_id: agent_id.clone(),
            };
            let input = FocusToolInput {
                action: "append_focus_record".into(),
                records: vec![record.clone()],
                experiment_id: experiment_id.clone(),
                personality: None,
            };
            match focus_tool::handle_focus_tool(input) {
                Ok(output) => {
                    // Persist the appended record (not the tool output) to JSONL.
                    // Path defaults to temp dir; override with FOCUS_SIDECAR_PATH env var.
                    // TODO: derive path from manifest/workbook path when available in context.
                    let sidecar_path = std::env::var("FOCUS_SIDECAR_PATH")
                        .map(std::path::PathBuf::from)
                        .unwrap_or_else(|_| std::env::temp_dir().join("focus_records.jsonl"));
                    match serde_json::to_string(&record) {
                        Ok(serialized) => {
                            match std::fs::OpenOptions::new()
                                .create(true)
                                .append(true)
                                .open(&sidecar_path)
                            {
                                Ok(mut f) => {
                                    if let Err(e) = writeln!(f, "{serialized}") {
                                        tracing::warn!(
                                            path = %sidecar_path.display(),
                                            err = %e,
                                            "focus_records JSONL write failed"
                                        );
                                    }
                                }
                                Err(e) => tracing::warn!(
                                    path = %sidecar_path.display(),
                                    err = %e,
                                    "focus_records JSONL open failed"
                                ),
                            }
                        }
                        Err(e) => tracing::warn!(err = %e, "focus record serialization failed"),
                    }
                    json!({
                        "content": [text_content(json!(output))],
                        "isError": false
                    })
                }
                Err(err) => error_envelope(&ToolError::Internal(err)),
            }
        }
        crate::contract::FocusArgs::QueryFocusSummary => {
            let input = FocusToolInput {
                action: "query_focus_summary".into(),
                records: vec![],
                experiment_id: None,
                personality: None,
            };
            match focus_tool::handle_focus_tool(input) {
                Ok(output) => json!({ "content": [text_content(json!(output))], "isError": false }),
                Err(err) => error_envelope(&ToolError::Internal(err)),
            }
        }
        crate::contract::FocusArgs::ComputeFocusDelta {
            experiment_id,
            control_billed,
            treatment_billed,
        } => {
            let input = FocusToolInput {
                action: "compute_focus_delta".into(),
                records: vec![
                    FocusToolRecord {
                        billing_account_id: "ledgrrr".into(),
                        service_name: "experiment-eval".into(),
                        billed_cost: control_billed,
                        effective_cost: control_billed * 0.85,
                        experiment_id: Some(experiment_id.clone()),
                        variant: Some("control".into()),
                        agent_id: None,
                    },
                    FocusToolRecord {
                        billing_account_id: "ledgrrr".into(),
                        service_name: "experiment-eval".into(),
                        billed_cost: treatment_billed,
                        effective_cost: treatment_billed * 0.85,
                        experiment_id: Some(experiment_id.clone()),
                        variant: Some("treatment".into()),
                        agent_id: None,
                    },
                ],
                experiment_id: Some(experiment_id),
                personality: None,
            };
            match focus_tool::handle_focus_tool(input) {
                Ok(output) => json!({ "content": [text_content(json!(output))], "isError": false }),
                Err(err) => error_envelope(&ToolError::Internal(err)),
            }
        }
        crate::contract::FocusArgs::ExperimentScore {
            experiment_id,
            personality,
            variant: _variant,
        } => {
            let input = FocusToolInput {
                action: "experiment_score".into(),
                records: vec![],
                experiment_id: Some(experiment_id),
                personality,
            };
            match focus_tool::handle_focus_tool(input) {
                Ok(output) => json!({ "content": [text_content(json!(output))], "isError": false }),
                Err(err) => error_envelope(&ToolError::Internal(err)),
            }
        }
    }
}

/// Hardcoded list of published tool names (always available).
const BUILTIN_TOOL_NAMES: &[&str] = &[
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
];

fn builtin_tool_input_schema(name: &str) -> Value {
    crate::contract::tool_input_schema(name)
}

/// Returns the names of all top-level published tools.
/// When the `legacy` feature is inactive this is derived from `BUILTIN_TOOL_NAMES`.
#[cfg(not(feature = "legacy"))]
pub fn tool_names() -> Vec<String> {
    BUILTIN_TOOL_NAMES.iter().map(|s| s.to_string()).collect()
}

/// Hook for external MCP providers.  Dispatches via the global provider registry.
#[cfg(feature = "b00t")]
pub fn handle_external_tool(tool_name: &str, arguments: &Value) -> Value {
    let Some(reg) = GLOBAL_PROVIDER_REGISTRY.get() else {
        return unknown_tool_result(tool_name);
    };
    match reg.call_tool("", tool_name, arguments.clone()) {
        Ok(result) => json!({
            "content": [text_content(result)],
            "isError": false
        }),
        Err(_) => unknown_tool_result(tool_name),
    }
}

#[cfg(not(feature = "b00t"))]
pub fn handle_external_tool(tool_name: &str, _arguments: &Value) -> Value {
    unknown_tool_result(tool_name)
}

// ── Legacy dispatch (cfg-gated) ───────────────────────────────────────────────
// The functions below are the original direct-dispatch path.  They are kept for
// historical reference and are only compiled when the `legacy` feature is active.

#[cfg(feature = "legacy")]
#[allow(clippy::vec_init_then_push)]
pub fn tool_names() -> Vec<String> {
    let mut features = Vec::new();

    #[cfg(feature = "core")]
    features.push("core");
    #[cfg(feature = "events")]
    features.push("events");
    #[cfg(feature = "reconciliation")]
    features.push("reconciliation");
    #[cfg(feature = "hsm")]
    features.push("hsm");
    #[cfg(feature = "ontology")]
    features.push("ontology");
    #[cfg(feature = "classification")]
    features.push("classification");
    #[cfg(feature = "audit")]
    features.push("audit");
    #[cfg(feature = "tax")]
    features.push("tax");
    #[cfg(feature = "xero")]
    features.push("xero");

    if features.is_empty() {
        features.push("core");
    }

    tool_names_for(&features)
}

pub fn tool_names_for(features: &[&str]) -> Vec<String> {
    let mut tools = Vec::new();

    let want_core = features.is_empty() || features.contains(&"core");
    if want_core {
        tools.push(DOCUMENTS_TOOL.to_string());
        tools.push(REVIEW_TOOL.to_string());
        tools.push(RECONCILIATION_TOOL.to_string());
        tools.push(WORKFLOW_TOOL.to_string());
        tools.push(AUDIT_TOOL.to_string());
        tools.push(TAX_TOOL.to_string());
        tools.push(ONTOLOGY_TOOL.to_string());
        tools.push(XERO_TOOL.to_string());
        tools.push(EVIDENCE_TOOL.to_string());
        tools.push(FOCUS_TOOL.to_string());
    }
    if features.contains(&"classification") {
        tools.push(REVIEW_TOOL.to_string());
    }
    if features.contains(&"reconciliation") {
        tools.push(RECONCILIATION_TOOL.to_string());
    }
    if features.contains(&"events") || features.contains(&"audit") {
        tools.push(AUDIT_TOOL.to_string());
    }
    if features.contains(&"tax") {
        tools.push(TAX_TOOL.to_string());
    }
    if features.contains(&"ontology") {
        tools.push(ONTOLOGY_TOOL.to_string());
    }
    if features.contains(&"xero") {
        tools.push(XERO_TOOL.to_string());
    }

    tools.sort();
    tools.dedup();
    tools
}

/// Returns the JSON Schema for the input arguments of a named tool.
#[cfg(feature = "legacy")]
pub fn tool_input_schema(name: &str) -> Value {
    contract::tool_input_schema(name)
}

#[cfg(feature = "legacy")]
pub fn handle_list_accounts(service: &TurboLedgerService) -> Value {
    match service.list_accounts_tool(ListAccountsRequest) {
        Ok(response) => {
            let accounts = response
                .accounts
                .into_iter()
                .map(|account| json!({ "account_id": account.account_id }))
                .collect::<Vec<_>>();
            json!({
                "content": [text_content(json!({ "accounts": accounts }))],
                "isError": false
            })
        }
        Err(err) => error_envelope(&err),
    }
}

#[cfg(feature = "legacy")]
pub fn handle_document_inventory(service: &TurboLedgerService, arguments: &Value) -> Value {
    let request = match parse_document_inventory_request(arguments) {
        Ok(request) => request,
        Err(err) => return error_envelope(&err),
    };

    match service.document_inventory_tool(request) {
        Ok(response) => {
            let documents = response
                .documents
                .into_iter()
                .map(|document| {
                    json!({
                        "file_name": document.file_name,
                        "document_path": document.document_path,
                        "raw_context_ref": document.raw_context_ref,
                        "status": document.status.as_str(),
                        "blocked_reason": document.blocked_reason,
                        "next_hint": document.next_hint,
                        "vendor": document.vendor,
                        "account_id": document.account_id,
                        "year_month": document.year_month,
                        "document_type": document.document_type,
                        "ingested_tx_ids": document.ingested_tx_ids,
                    })
                })
                .collect::<Vec<_>>();
            let status_counts = document_status_counts(&documents);
            json!({
                "content": [text_content(json!({
                    "documents": documents,
                    "summary": {
                        "total_documents": status_counts.values().copied().sum::<usize>(),
                        "status_counts": status_counts,
                    }
                }))],
                "isError": false
            })
        }
        Err(err) => error_envelope(&err),
    }
}

#[cfg(feature = "legacy")]
pub fn handle_get_raw_context(service: &TurboLedgerService, arguments: &Value) -> Value {
    let request = match parse_get_raw_context_request(arguments) {
        Ok(request) => request,
        Err(err) => return error_envelope(&err),
    };

    match service.get_raw_context(request) {
        Ok(response) => json!({
            "content": [text_content(json!({ "bytes": response.bytes }))],
            "isError": false
        }),
        Err(err) => error_envelope(&err),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PipelineStatusResponse {
    pub status: String,
    pub blockers: Vec<String>,
    pub next_hint: String,
}

#[cfg(feature = "legacy")]
pub fn get_pipeline_status(
    manifest_ready: bool,
    rustledger_ready: bool,
    docling_ready: bool,
    mut blockers: Vec<String>,
) -> PipelineStatusResponse {
    if !manifest_ready {
        blockers.push("manifest_unavailable".to_string());
    }
    if !rustledger_ready {
        blockers.push("rustledger_unreachable".to_string());
    }
    if !docling_ready {
        blockers.push("docling_unreachable".to_string());
    }
    blockers.sort();
    blockers.dedup();

    if blockers.is_empty() {
        PipelineStatusResponse {
            status: "ready".to_string(),
            blockers,
            next_hint: "call_proxy_ingest_pdf".to_string(),
        }
    } else {
        PipelineStatusResponse {
            status: "blocked".to_string(),
            blockers,
            next_hint: "resolve_blockers_then_retry".to_string(),
        }
    }
}

#[cfg(feature = "legacy")]
pub fn handle_pipeline_status(
    manifest_ready: bool,
    rustledger_ready: bool,
    docling_ready: bool,
    blockers: Vec<String>,
) -> Value {
    let status = get_pipeline_status(manifest_ready, rustledger_ready, docling_ready, blockers);
    json!({
        "content": [text_content(json!({
            "status": status.status,
            "blockers": status.blockers,
            "next_hint": status.next_hint,
        }))],
        "isError": false
    })
}

#[cfg(feature = "legacy")]
pub fn rows_to_json_with_provenance(
    provider: &str,
    backend_tool: &str,
    backend_version: Option<&str>,
    backend_call_id: Option<&str>,
    rows: Vec<TransactionInput>,
) -> Vec<Value> {
    rows.into_iter()
        .map(|row| {
            let account_id = row.account_id;
            let currency = infer_currency(&account_id);
            json!({
                "account": account_id,
                "date": row.date,
                "amount": row.amount,
                "description": row.description,
                "currency": currency,
                "source_ref": row.source_ref,
                "provider": provider,
                "backend_tool": backend_tool,
                "backend_version": backend_version,
                "backend_call_id": backend_call_id,
            })
        })
        .collect()
}

fn text_content(payload: Value) -> Value {
    json!({ "type": "text", "text": payload.to_string() })
}

fn error_envelope(err: &ToolError) -> Value {
    json!({
        "content": [text_content(error_payload(err))],
        "isError": true
    })
}

pub fn error_payload(error: &ToolError) -> Value {
    match error {
        ToolError::InvalidInput(message) => json!({
            "isError": true,
            "error_type": "InvalidInput",
            "message": message,
        }),
        ToolError::Internal(message) => json!({
            "isError": true,
            "error_type": "Internal",
            "message": message,
        }),
        ToolError::PolicyDenied(reason) => json!({
            "isError": true,
            "error_type": "PolicyDenied",
            "message": reason,
        }),
        ToolError::RateLimited { retry_after_secs } => json!({
            "isError": true,
            "error_type": "RateLimited",
            "message": format!("Rate limited. Retry after {} seconds", retry_after_secs),
            "retry_after_secs": retry_after_secs,
        }),
    }
}

/// Return a well-formed MCP error envelope for an unknown tool name.
///
/// Not gated behind any feature flag because it is used by both the `legacy`
/// dispatch path and the `b00t` external-provider path.
pub fn unknown_tool_result(tool_name: &str) -> Value {
    json!({
        "content": [text_content(json!({
                "isError": true,
                "error_type": "InvalidInput",
                "message": format!("unknown tool: {tool_name}")
            }))],
        "isError": true
    })
}

fn handle_plugin_info(arguments: &Value) -> Value {
    let payload = crate::plugin_info::handle(arguments);
    json!({
        "content": [text_content(payload)],
        "isError": false
    })
}

#[cfg(feature = "legacy")]
pub fn handle_documents_tool(service: &TurboLedgerService, arguments: &Value) -> Value {
    let request = match contract::parse_documents(arguments) {
        Ok(request) => request,
        Err(err) => return error_envelope(&err),
    };

    match request {
        DocumentsArgs::ListAccounts => handle_list_accounts(service),
        DocumentsArgs::GetRawContext { rkyv_ref } => {
            handle_get_raw_context(service, &json!({ "rkyv_ref": rkyv_ref }))
        }
        DocumentsArgs::PipelineStatus => handle_pipeline_status(true, true, true, Vec::new()),
        DocumentsArgs::ValidateFilename { file_name } => {
            match service.validate_source_filename(&file_name) {
                Ok(parsed) => json!({
                    "content": [text_content(json!({
                        "vendor": parsed.vendor,
                        "account": parsed.account,
                        "year": parsed.year,
                        "month": parsed.month,
                        "doc_type": parsed.doc_type,
                    }))],
                    "isError": false
                }),
                Err(err) => error_envelope(&err),
            }
        }
        DocumentsArgs::IngestPdf {
            pdf_path,
            journal_path,
            workbook_path,
            ontology_path,
            raw_context_bytes,
            extracted_rows,
        } => handle_ingest_pdf(
            service,
            &json!({
                "pdf_path": pdf_path,
                "journal_path": journal_path,
                "workbook_path": workbook_path,
                "ontology_path": ontology_path,
                "raw_context_bytes": raw_context_bytes,
                "extracted_rows": extracted_rows,
            }),
            None,
        ),
        DocumentsArgs::IngestRows {
            journal_path,
            workbook_path,
            ontology_path,
            rows,
        } => handle_ingest_statement_rows(
            service,
            &json!({
                "journal_path": journal_path,
                "workbook_path": workbook_path,
                "ontology_path": ontology_path,
                "rows": rows,
            }),
            None,
        ),
        DocumentsArgs::DocumentInventory {
            directory,
            recursive,
            statuses,
        } => {
            let status_filter: Vec<DocumentQueueStatusRequest> = statuses
                .into_iter()
                .filter_map(|s| DocumentQueueStatusRequest::parse(s.as_str()))
                .collect();
            handle_document_inventory(
                service,
                &json!({
                    "directory": directory,
                    "recursive": recursive,
                    "statuses": status_filter.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                }),
            )
        }
        DocumentsArgs::IngestImage {
            image_path,
            doc_type,
            tags,
            extract_with_llm,
        } => {
            use crate::IngestImageRequest;
            match service.ingest_image_tool(IngestImageRequest {
                image_path,
                doc_type,
                tags,
                extract_with_llm,
            }) {
                Ok(r) => json!({
                    "content": [text_content(json!({
                        "doc_id": r.doc_id,
                        "file_name": r.file_name,
                        "doc_type": r.doc_type,
                        "tags": r.tags,
                        "llm_extracted": r.llm_extracted,
                    }))],
                    "isError": false
                }),
                Err(e) => error_envelope(&e),
            }
        }
        DocumentsArgs::ApplyTags {
            doc_ref,
            tags,
            sync_fs,
        } => {
            use crate::ApplyTagsRequest;
            match service.apply_tags_tool(ApplyTagsRequest {
                doc_ref,
                tags,
                sync_fs,
            }) {
                Ok(r) => json!({
                    "content": [text_content(json!({"doc_id": r.doc_id, "tags": r.tags}))],
                    "isError": false
                }),
                Err(e) => error_envelope(&e),
            }
        }
        DocumentsArgs::RemoveTags {
            doc_ref,
            tags,
            sync_fs,
        } => {
            use crate::ApplyTagsRequest;
            match service.remove_tags_tool(ApplyTagsRequest {
                doc_ref,
                tags,
                sync_fs,
            }) {
                Ok(r) => json!({
                    "content": [text_content(json!({"doc_id": r.doc_id, "tags": r.tags}))],
                    "isError": false
                }),
                Err(e) => error_envelope(&e),
            }
        }
        DocumentsArgs::ListTagged {
            tags,
            doc_type,
            directory,
        } => {
            use crate::ListTaggedRequest;
            match service.list_tagged_tool(ListTaggedRequest {
                tags,
                doc_type,
                directory,
            }) {
                Ok(r) => json!({
                    "content": [text_content(json!({
                        "documents": r.documents.iter().map(|d| json!({
                            "doc_id": d.doc_id,
                            "file_name": d.file_name,
                            "doc_type": d.doc_type,
                            "tags": d.tags,
                            "status": d.status,
                        })).collect::<Vec<_>>(),
                        "count": r.documents.len(),
                    }))],
                    "isError": false
                }),
                Err(e) => error_envelope(&e),
            }
        }
        DocumentsArgs::SyncFsMetadata {
            directory,
            recursive,
        } => {
            use crate::SyncFsMetadataRequest;
            match service.sync_fs_metadata_tool(SyncFsMetadataRequest {
                directory,
                recursive,
            }) {
                Ok(r) => json!({
                    "content": [text_content(json!({
                        "files_scanned": r.files_scanned,
                        "files_synced": r.files_synced,
                    }))],
                    "isError": false
                }),
                Err(e) => error_envelope(&e),
            }
        }
        DocumentsArgs::NormalizeFilename {
            file_path,
            vendor,
            account,
            year_month,
            doc_type,
            apply,
        } => {
            use crate::NormalizeFilenameRequest;
            match service.normalize_filename_tool(NormalizeFilenameRequest {
                file_path,
                vendor,
                account,
                year_month,
                doc_type,
                apply,
            }) {
                Ok(r) => json!({
                    "content": [text_content(json!({
                        "proposed_name": r.proposed_name,
                        "original_name": r.original_name,
                        "renamed": r.renamed,
                    }))],
                    "isError": false
                }),
                Err(e) => error_envelope(&e),
            }
        }
    }
}

#[cfg(feature = "legacy")]
pub fn handle_xero_tool(service: &TurboLedgerService, arguments: &Value) -> Value {
    use contract::{parse_xero, XeroArgs};

    let request = match parse_xero(arguments) {
        Ok(r) => r,
        Err(e) => return error_envelope(&e),
    };

    let result: Result<serde_json::Value, crate::ToolError> = match request {
        XeroArgs::GetAuthUrl => service
            .xero_get_auth_url()
            .map(|url| json!({ "auth_url": url })),
        XeroArgs::ExchangeCode { code, state } => service.xero_exchange_code(code, state),
        XeroArgs::FetchContacts => service.xero_fetch_contacts(None),
        XeroArgs::SearchContacts { query } => service.xero_fetch_contacts(Some(query.as_str())),
        XeroArgs::FetchAccounts => service.xero_fetch_accounts(),
        XeroArgs::FetchBankAccounts => service.xero_fetch_bank_accounts(),
        XeroArgs::FetchInvoices { status } => service.xero_fetch_invoices(status.as_deref()),
        XeroArgs::LinkEntity {
            local_id,
            xero_entity_type,
            xero_id,
            display_name,
            ontology_path,
        } => service.xero_link_entity(
            local_id,
            xero_entity_type,
            xero_id,
            display_name,
            ontology_path,
        ),
        XeroArgs::SyncCatalog { ontology_path } => service.xero_sync_catalog(ontology_path),
    };

    match result {
        Ok(payload) => json!({
            "content": [text_content(payload)],
            "isError": false
        }),
        Err(e) => error_envelope(&e),
    }
}

#[cfg(feature = "legacy")]
pub fn handle_review_tool(service: &TurboLedgerService, arguments: &Value) -> Value {
    let request = match contract::parse_review(arguments) {
        Ok(request) => request,
        Err(err) => return error_envelope(&err),
    };

    match request {
        ReviewArgs::RunRule {
            rule_file,
            sample_tx,
        } => {
            let request = match parse_run_rhai_rule_request(&json!({
                "rule_file": rule_file,
                "sample_tx": sample_tx,
            })) {
                Ok(request) => request,
                Err(err) => return error_envelope(&err),
            };
            match service.run_rhai_rule(request) {
                Ok(response) => json!({
                    "content": [text_content(json!({
                        "category": response.category,
                        "confidence": response.confidence,
                        "review": response.review,
                        "reason": response.reason,
                    }))],
                    "isError": false
                }),
                Err(err) => error_envelope(&err),
            }
        }
        ReviewArgs::ClassifyIngested {
            rule_file,
            review_threshold,
        } => handle_classify_ingested(
            service,
            &json!({
                "rule_file": rule_file,
                "review_threshold": review_threshold.as_json(),
            }),
        ),
        ReviewArgs::QueryFlags { year, status } => handle_query_flags(
            service,
            &json!({
                "year": year,
                "status": match status {
                    crate::contract::ReviewStatusInput::Open => "open",
                    crate::contract::ReviewStatusInput::Resolved => "resolved",
                }
            }),
        ),
        ReviewArgs::ClassifyTransaction {
            tx_id,
            category,
            confidence,
            note,
            actor,
        } => handle_classify_transaction(
            service,
            &json!({
                "tx_id": tx_id,
                "category": category,
                "confidence": confidence,
                "note": note,
                "actor": actor,
            }),
        ),
        ReviewArgs::ReconcileExcelClassification {
            tx_id,
            category,
            confidence,
            actor,
            note,
        } => handle_reconcile_excel_classification(
            service,
            &json!({
                "tx_id": tx_id,
                "category": category,
                "confidence": confidence,
                "actor": actor,
                "note": note,
            }),
        ),
        ReviewArgs::QueryTransactions { filters, sort, pagination } => {
            match service.query_transactions(QueryTransactionsRequest {
                filters,
                sort,
                pagination,
            }) {
                Ok(response) => json!({
                    "content": [text_content(json!({
                        "transactions": response.transactions,
                        "total_count": response.total_count,
                    }))],
                    "isError": false
                }),
                Err(err) => error_envelope(&err),
            }
        }
        ReviewArgs::BatchClassify {
            request,
        } => handle_batch_classify(
            service,
            &json!({ "request": request }),
        ),
        ReviewArgs::BatchResolveFlags {
            request,
        } => handle_bulk_resolve_flags(
            service,
            &json!({ "request": request }),
        ),
        ReviewArgs::ApplyMappingBulk {
            request,
        } => handle_apply_mapping_bulk(
            service,
            &json!({ "request": request }),
        ),
        ReviewArgs::FetchQueue { request } => handle_fetch_queue(
            service,
            &serde_json::json!({ "item_types": request.item_types, "statuses": request.statuses, "updated_after": request.updated_after, "limit": request.limit, "offset": request.offset }),
        ),
    }
}

#[cfg(feature = "legacy")]
pub fn handle_reconciliation_tool(service: &TurboLedgerService, arguments: &Value) -> Value {
    let request = match contract::parse_reconciliation(arguments) {
        Ok(request) => request,
        Err(err) => return error_envelope(&err),
    };

    match request {
        ReconciliationArgs::Validate {
            source_total,
            extracted_total,
            posting_amounts,
        } => dispatch_reconciliation(
            service,
            "validate",
            &json!({
                "source_total": source_total,
                "extracted_total": extracted_total,
                "posting_amounts": posting_amounts,
            }),
        ),
        ReconciliationArgs::Reconcile {
            source_total,
            extracted_total,
            posting_amounts,
        } => dispatch_reconciliation(
            service,
            "reconcile",
            &json!({
                "source_total": source_total,
                "extracted_total": extracted_total,
                "posting_amounts": posting_amounts,
            }),
        ),
        ReconciliationArgs::Commit {
            source_total,
            extracted_total,
            posting_amounts,
        } => dispatch_reconciliation(
            service,
            "commit",
            &json!({
                "source_total": source_total,
                "extracted_total": extracted_total,
                "posting_amounts": posting_amounts,
            }),
        ),
    }
}

#[cfg(feature = "legacy")]
pub fn handle_workflow_tool(service: &TurboLedgerService, arguments: &Value) -> Value {
    let request = match contract::parse_workflow(arguments) {
        Ok(request) => request,
        Err(err) => return error_envelope(&err),
    };

    match request {
        WorkflowArgs::Status => dispatch_hsm(service, "status", &json!({})),
        WorkflowArgs::Transition {
            target_state,
            target_substate,
        } => dispatch_hsm(
            service,
            "transition",
            &json!({
                "target_state": target_state,
                "target_substate": target_substate,
            }),
        ),
        WorkflowArgs::Resume { state_marker } => dispatch_hsm(
            service,
            "resume",
            &json!({
                "state_marker": state_marker,
            }),
        ),
        WorkflowArgs::PluginInfo { payload } => handle_plugin_info(&json!({
            "subcommand": payload.subcommand.map(|value| value.0),
        })),
    }
}

#[cfg(feature = "legacy")]
pub fn handle_audit_tool(service: &TurboLedgerService, arguments: &Value) -> Value {
    let request = match contract::parse_audit(arguments) {
        Ok(request) => request,
        Err(err) => return error_envelope(&err),
    };

    match request {
        AuditArgs::EventHistory {
            tx_id,
            document_ref,
            time_start,
            time_end,
        } => handle_event_history(
            service,
            &json!({
                "tx_id": tx_id,
                "document_ref": document_ref,
                "time_start": time_start,
                "time_end": time_end,
            }),
        ),
        AuditArgs::EventReplay {
            tx_id,
            document_ref,
        } => handle_event_replay(
            service,
            &json!({
                "tx_id": tx_id,
                "document_ref": document_ref,
            }),
        ),
        AuditArgs::QueryAuditLog => handle_query_audit_log(service, &json!({})),
    }
}

#[cfg(feature = "legacy")]
pub fn handle_tax_tool(service: &TurboLedgerService, arguments: &Value) -> Value {
    let request = match contract::parse_tax(arguments) {
        Ok(request) => request,
        Err(err) => return error_envelope(&err),
    };

    match request {
        TaxArgs::Assist {
            ontology_path,
            from_entity_id,
            max_depth,
            reconciliation,
        } => handle_tax_assist(
            service,
            &json!({
                "ontology_path": ontology_path,
                "from_entity_id": from_entity_id,
                "max_depth": max_depth,
                "reconciliation": reconciliation,
            }),
        ),
        TaxArgs::EvidenceChain {
            ontology_path,
            from_entity_id,
            tx_id,
            document_ref,
        } => handle_tax_evidence_chain(
            service,
            &json!({
                "ontology_path": ontology_path,
                "from_entity_id": from_entity_id,
                "tx_id": tx_id,
                "document_ref": document_ref,
            }),
        ),
        TaxArgs::AmbiguityReview {
            ontology_path,
            from_entity_id,
            max_depth,
            reconciliation,
        } => handle_tax_ambiguity_review(
            service,
            &json!({
                "ontology_path": ontology_path,
                "from_entity_id": from_entity_id,
                "max_depth": max_depth,
                "reconciliation": reconciliation,
            }),
        ),
        TaxArgs::ScheduleSummary { year, schedule } => handle_get_schedule_summary(
            service,
            &json!({
                "year": year,
                "schedule": match schedule {
                    crate::contract::ScheduleInput::ScheduleC => "ScheduleC",
                    crate::contract::ScheduleInput::ScheduleD => "ScheduleD",
                    crate::contract::ScheduleInput::ScheduleE => "ScheduleE",
                    crate::contract::ScheduleInput::Fbar => "Fbar",
                }
            }),
        ),
        TaxArgs::ExportWorkbook { workbook_path } => handle_export_cpa_workbook(
            service,
            &json!({
                "workbook_path": workbook_path,
            }),
        ),
    }
}

#[cfg(feature = "legacy")]
pub fn handle_ontology_tool(service: &TurboLedgerService, arguments: &Value) -> Value {
    let request = match contract::parse_ontology(arguments) {
        Ok(request) => request,
        Err(err) => return error_envelope(&err),
    };

    match request {
        OntologyArgs::QueryPath {
            ontology_path,
            from_entity_id,
            max_depth,
        } => handle_ontology_query_path(
            service,
            &json!({
                "ontology_path": ontology_path,
                "from_entity_id": from_entity_id,
                "max_depth": max_depth,
            }),
        ),
        OntologyArgs::ExportSnapshot { ontology_path } => handle_ontology_export_snapshot(
            service,
            &json!({
                "ontology_path": ontology_path,
            }),
        ),
        OntologyArgs::UpsertEntities {
            ontology_path,
            entities,
        } => handle_ontology_upsert_entities(
            service,
            &json!({
                "ontology_path": ontology_path,
                "entities": entities,
            }),
        ),
        OntologyArgs::UpsertEdges {
            ontology_path,
            edges,
        } => handle_ontology_upsert_edges(
            service,
            &json!({
                "ontology_path": ontology_path,
                "edges": edges,
            }),
        ),
    }
}

#[cfg(feature = "legacy")]
pub fn protocol_method_not_found(id: Value, method: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": -32601,
            "message": format!("method not found: {method}")
        }
    })
}

fn unknown_tool_action_result(tool_name: &str, action: &str) -> Value {
    json!({
        "content": [text_content(json!({
            "isError": true,
            "error_type": "InvalidInput",
            "message": format!("unknown action `{action}` for tool `{tool_name}`")
        }))],
        "isError": true
    })
}

fn parse_ingest_pdf_request(arguments: &Value) -> Result<IngestPdfRequest, ToolError> {
    let pdf_path = required_str(arguments, "pdf_path")?.to_string();
    let journal_path = PathBuf::from(required_str(arguments, "journal_path")?);
    let workbook_path = PathBuf::from(required_str(arguments, "workbook_path")?);
    let ontology_path = arguments
        .get("ontology_path")
        .and_then(Value::as_str)
        .map(PathBuf::from);
    let raw_context_bytes = parse_optional_bytes(arguments.get("raw_context_bytes"))?;
    let extracted_rows = match arguments.get("extracted_rows") {
        None | Some(Value::Null) => Vec::new(),
        some => parse_rows(some, "extracted_rows")?,
    };

    Ok(IngestPdfRequest {
        pdf_path,
        journal_path,
        workbook_path,
        ontology_path,
        raw_context_bytes,
        extracted_rows,
    })
}

fn parse_ingest_statement_rows_request(
    arguments: &Value,
) -> Result<IngestStatementRowsRequest, ToolError> {
    let journal_path = PathBuf::from(required_str(arguments, "journal_path")?);
    let workbook_path = PathBuf::from(required_str(arguments, "workbook_path")?);
    let ontology_path = arguments
        .get("ontology_path")
        .and_then(Value::as_str)
        .map(PathBuf::from);
    let rows = parse_rows(arguments.get("rows"), "rows")?;

    Ok(IngestStatementRowsRequest {
        journal_path,
        workbook_path,
        ontology_path,
        rows,
    })
}

#[cfg(feature = "legacy")]
pub fn handle_ingest_pdf<T: TurboLedgerTools>(
    service: &T,
    arguments: &Value,
    backend_call_id: Option<String>,
) -> Value {
    let request = match parse_ingest_pdf_request(arguments) {
        Ok(request) => request,
        Err(err) => return error_envelope(&err),
    };

    let canonical_rows = rows_to_json_with_provenance(
        "docling",
        "ingest_pdf",
        Some(env!("CARGO_PKG_VERSION")),
        backend_call_id.as_deref(),
        request.extracted_rows.clone(),
    );

    match service.ingest_pdf(request.clone()) {
        Ok(response) => {
            let tx_ids = if response.tx_ids.is_empty() {
                request
                    .extracted_rows
                    .iter()
                    .map(deterministic_tx_id)
                    .collect::<Vec<_>>()
            } else {
                response.tx_ids
            };
            json!({
                "content": [text_content(json!({
                        "inserted_count": response.inserted_count,
                        "tx_ids": tx_ids,
                        "canonical_rows": canonical_rows,
                    }))],
                "isError": false
            })
        }
        Err(err) => error_envelope(&err),
    }
}

#[cfg(feature = "legacy")]
pub fn handle_ingest_statement_rows<T: TurboLedgerTools>(
    service: &T,
    arguments: &Value,
    backend_call_id: Option<String>,
) -> Value {
    let request = match parse_ingest_statement_rows_request(arguments) {
        Ok(request) => request,
        Err(err) => return error_envelope(&err),
    };

    let canonical_rows = rows_to_json_with_provenance(
        "rustledger",
        "ingest_statement_rows",
        Some(env!("CARGO_PKG_VERSION")),
        backend_call_id.as_deref(),
        request.rows.clone(),
    );

    match service.ingest_statement_rows(request.clone()) {
        Ok(response) => {
            let tx_ids = if response.tx_ids.is_empty() {
                request
                    .rows
                    .iter()
                    .map(deterministic_tx_id)
                    .collect::<Vec<_>>()
            } else {
                response.tx_ids
            };
            json!({
                "content": [text_content(json!({
                        "inserted_count": response.inserted_count,
                        "tx_ids": tx_ids,
                        "canonical_rows": canonical_rows,
                        "provider": "rustledger",
                        "backend_tool": "ingest_statement_rows",
                    }))],
                "isError": false
            })
        }
        Err(err) => error_envelope(&err),
    }
}

#[cfg(feature = "legacy")]
pub fn handle_ontology_query_path(service: &TurboLedgerService, arguments: &Value) -> Value {
    let request = match parse_ontology_query_path_request(arguments) {
        Ok(request) => request,
        Err(err) => return error_envelope(&err),
    };

    match service.ontology_query_path_tool(request) {
        Ok(response) => json!({
            "content": [text_content(json!({
                    "nodes": response.nodes,
                    "edges": response.edges,
                }))],
            "isError": false
        }),
        Err(err) => error_envelope(&err),
    }
}

#[cfg(feature = "legacy")]
pub fn handle_ontology_export_snapshot(service: &TurboLedgerService, arguments: &Value) -> Value {
    let ontology_path = match parse_ontology_path(arguments) {
        Ok(path) => path,
        Err(err) => return error_envelope(&err),
    };

    match service.ontology_export_snapshot(OntologyExportSnapshotRequest { ontology_path }) {
        Ok(response) => json!({
            "content": [text_content(json!({
                    "entities": response.entities,
                    "edges": response.edges,
                    "snapshot": {
                        "entity_count": response.entity_count,
                        "edge_count": response.edge_count,
                    }
                }))],
            "isError": false
        }),
        Err(err) => error_envelope(&err),
    }
}

#[cfg(feature = "legacy")]
pub fn dispatch_reconciliation(
    service: &TurboLedgerService,
    tool_name: &str,
    arguments: &Value,
) -> Value {
    let request = match parse_reconciliation_stage_request(arguments) {
        Ok(request) => request,
        Err(err) => return error_envelope(&err),
    };

    let response = match tool_name {
        "validate" => service.validate_reconciliation_stage_tool(request),
        "reconcile" => service.reconcile_reconciliation_stage_tool(request),
        "commit" => service.commit_reconciliation_stage_tool(request),
        _ => {
            return unknown_tool_action_result(RECONCILIATION_TOOL, tool_name);
        }
    };

    match response {
        Ok(stage_response) => {
            let blocked = stage_response.status == "blocked";
            let stage = stage_response.stage;
            let status = stage_response.status;
            let stage_marker = stage_response.stage_marker;
            let blocked_reasons = stage_response.blocked_reasons;
            let diagnostics = stage_response
                .diagnostics
                .into_iter()
                .map(|diag| json!({ "key": diag.key, "message": diag.message }))
                .collect::<Vec<_>>();

            let payload = if blocked {
                json!({
                    "isError": true,
                    "error_type": "ReconciliationBlocked",
                    "message": format!("{stage} blocked by reconciliation guardrails"),
                    "stage": stage,
                    "status": status,
                    "stage_marker": stage_marker,
                    "blocked_reasons": blocked_reasons,
                    "diagnostics": diagnostics,
                })
            } else {
                json!({
                    "stage": stage,
                    "status": status,
                    "stage_marker": stage_marker,
                    "blocked_reasons": blocked_reasons,
                    "diagnostics": diagnostics,
                })
            };

            json!({
                "content": [text_content(payload)],
                "isError": blocked
            })
        }
        Err(err) => error_envelope(&err),
    }
}

#[cfg(feature = "legacy")]
pub fn dispatch_hsm(service: &TurboLedgerService, tool_name: &str, arguments: &Value) -> Value {
    match tool_name {
        "transition" => {
            let request = match parse_hsm_transition_request(arguments) {
                Ok(request) => request,
                Err(err) => return error_envelope(&err),
            };

            match service.hsm_transition_tool(request) {
                Ok(response) => {
                    let blocked = response.status == "blocked";
                    let payload = if blocked {
                        json!({
                            "isError": true,
                            "error_type": "HsmTransitionBlocked",
                            "message": "hsm transition blocked by lifecycle guard",
                            "state": response.state,
                            "substate": response.substate,
                            "status": response.status,
                            "guard_reason": response.guard_reason,
                            "transition_evidence": response.transition_evidence,
                            "state_marker": response.state_marker,
                        })
                    } else {
                        json!({
                            "state": response.state,
                            "substate": response.substate,
                            "status": response.status,
                            "guard_reason": response.guard_reason,
                            "transition_evidence": response.transition_evidence,
                            "state_marker": response.state_marker,
                        })
                    };
                    json!({
                        "content": [text_content(payload)],
                        "isError": blocked
                    })
                }
                Err(err) => error_envelope(&err),
            }
        }
        "status" => match service.hsm_status_tool(HsmStatusRequest) {
            Ok(response) => json!({
                "content": [text_content(json!({
                        "state": response.state,
                        "substate": response.substate,
                        "display_state": response.display_state,
                        "next_hint": response.next_hint,
                        "resume_hint": response.resume_hint,
                        "blockers": response.blockers,
                    }))],
                "isError": false
            }),
            Err(err) => error_envelope(&err),
        },
        "resume" => {
            let request = match parse_hsm_resume_request(arguments) {
                Ok(request) => request,
                Err(err) => return error_envelope(&err),
            };

            match service.hsm_resume_tool(request) {
                Ok(response) => {
                    let blocked = !response.resumed;
                    let payload = if blocked {
                        json!({
                            "isError": true,
                            "error_type": "HsmResumeBlocked",
                            "message": "hsm resume blocked by checkpoint guard",
                            "resumed": response.resumed,
                            "resume_from": response.resume_from,
                            "resume_hint": response.resume_hint,
                            "blockers": response.blockers,
                        })
                    } else {
                        json!({
                            "resumed": response.resumed,
                            "resume_from": response.resume_from,
                            "resume_hint": response.resume_hint,
                            "blockers": response.blockers,
                        })
                    };
                    json!({
                        "content": [text_content(payload)],
                        "isError": blocked
                    })
                }
                Err(err) => error_envelope(&err),
            }
        }
        _ => unknown_tool_action_result(WORKFLOW_TOOL, tool_name),
    }
}

#[cfg(feature = "legacy")]
pub fn handle_event_history(service: &TurboLedgerService, arguments: &Value) -> Value {
    let filter = match parse_event_history_filter(arguments) {
        Ok(filter) => filter,
        Err(err) => return error_envelope(&err),
    };

    match service.event_history(filter.clone()) {
        Ok(response) => {
            let events = response
                .events
                .into_iter()
                .map(|event| {
                    json!({
                        "event_id": event.event_id,
                        "sequence": event.sequence,
                        "event_type": event.event_type,
                        "tx_id": event.tx_id,
                        "document_ref": event.document_ref,
                        "occurred_at": event.occurred_at,
                        "payload": event.payload,
                        "identity_inputs": event.identity_inputs,
                    })
                })
                .collect::<Vec<_>>();

            json!({
                "content": [text_content(json!({
                        "filter": {
                            "tx_id": filter.tx_id,
                            "document_ref": filter.document_ref,
                            "time_start": filter.time_start,
                            "time_end": filter.time_end,
                        },
                        "events": events,
                    }))],
                "isError": false
            })
        }
        Err(ToolError::InvalidInput(message))
            if message.contains("time_start must be <= time_end") =>
        {
            json!({
                "content": [text_content(json!({
                        "isError": true,
                        "error_type": "EventHistoryBlocked",
                        "reason": "time_range_invalid",
                        "message": message,
                    }))],
                "isError": true
            })
        }
        Err(err) => error_envelope(&err),
    }
}

#[cfg(feature = "legacy")]
pub fn handle_event_replay(service: &TurboLedgerService, arguments: &Value) -> Value {
    let request = match parse_replay_lifecycle_request(arguments) {
        Ok(request) => request,
        Err(err) => return error_envelope(&err),
    };

    match service.replay_lifecycle(request) {
        Ok(response) => json!({
            "content": [text_content(json!({
                    "reconstructed_state": response.reconstructed_state,
                    "event_count": response.event_count,
                    "diagnostics": response.diagnostics,
                    "filter": {
                        "tx_id": response.filter.tx_id,
                        "document_ref": response.filter.document_ref,
                        "time_start": response.filter.time_start,
                        "time_end": response.filter.time_end,
                    }
                }))],
            "isError": false
        }),
        Err(err) => error_envelope(&err),
    }
}

#[cfg(feature = "legacy")]
pub fn handle_tax_assist(service: &TurboLedgerService, arguments: &Value) -> Value {
    let request = match parse_tax_assist_request(arguments) {
        Ok(request) => request,
        Err(err) => return error_envelope(&err),
    };

    match service.tax_assist_tool(request) {
        Ok(response) => {
            let blocked = response.status == "blocked";
            let payload = if blocked {
                json!({
                    "isError": true,
                    "error_type": "TaxAssistBlocked",
                    "reason": response.blocked_reasons.first().cloned().unwrap_or_default(),
                    "status": response.status,
                    "stage_marker": response.stage_marker,
                    "blocked_reasons": response.blocked_reasons,
                    "summary": response.summary,
                    "schedule_rows": response.schedule_rows,
                    "fbar_rows": response.fbar_rows,
                    "ambiguity": response.ambiguity,
                })
            } else {
                json!({
                    "status": response.status,
                    "stage_marker": response.stage_marker,
                    "blocked_reasons": response.blocked_reasons,
                    "summary": response.summary,
                    "schedule_rows": response.schedule_rows,
                    "fbar_rows": response.fbar_rows,
                    "ambiguity": response.ambiguity,
                })
            };
            json!({
                "content": [text_content(payload)],
                "isError": blocked
            })
        }
        Err(err) => error_envelope(&err),
    }
}

#[cfg(feature = "legacy")]
pub fn handle_tax_evidence_chain(service: &TurboLedgerService, arguments: &Value) -> Value {
    let request = match parse_tax_evidence_chain_request(arguments) {
        Ok(request) => request,
        Err(err) => return error_envelope(&err),
    };

    match service.tax_evidence_chain_tool(request) {
        Ok(response) => json!({
            "content": [text_content(json!({
                    "source": response.source,
                    "events": response.events,
                    "current_state": response.current_state,
                    "ambiguity": response.ambiguity,
                }))],
            "isError": false
        }),
        Err(err) => error_envelope(&err),
    }
}

#[cfg(feature = "legacy")]
pub fn handle_tax_ambiguity_review(service: &TurboLedgerService, arguments: &Value) -> Value {
    let request = match parse_tax_ambiguity_review_request(arguments) {
        Ok(request) => request,
        Err(err) => return error_envelope(&err),
    };

    match service.tax_ambiguity_review_tool(request) {
        Ok(response) => {
            let blocked = response.status == "blocked";
            let payload = if blocked {
                json!({
                    "isError": true,
                    "error_type": "TaxAmbiguityReviewBlocked",
                    "reason": response.blocked_reasons.first().cloned().unwrap_or_default(),
                    "status": response.status,
                    "stage_marker": response.stage_marker,
                    "blocked_reasons": response.blocked_reasons,
                    "ambiguity": response.ambiguity,
                })
            } else {
                json!({
                    "status": response.status,
                    "stage_marker": response.stage_marker,
                    "blocked_reasons": response.blocked_reasons,
                    "ambiguity": response.ambiguity,
                })
            };
            json!({
                "content": [text_content(payload)],
                "isError": blocked
            })
        }
        Err(err) => error_envelope(&err),
    }
}

#[cfg(feature = "legacy")]
pub fn handle_classify_ingested(service: &TurboLedgerService, arguments: &Value) -> Value {
    let request = match parse_classify_ingested_request(arguments) {
        Ok(request) => request,
        Err(err) => return error_envelope(&err),
    };

    match service.classify_ingested(request) {
        Ok(response) => {
            let classifications = response
                .classifications
                .into_iter()
                .map(|c| {
                    json!({
                        "tx_id": c.tx_id,
                        "category": c.category,
                        "confidence": c.confidence,
                        "needs_review": c.needs_review,
                        "reason": c.reason,
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "content": [text_content(json!({ "classifications": classifications }))],
                "isError": false
            })
        }
        Err(err) => error_envelope(&err),
    }
}

#[cfg(feature = "legacy")]
pub fn handle_query_flags(service: &TurboLedgerService, arguments: &Value) -> Value {
    let request = match parse_query_flags_request(arguments) {
        Ok(request) => request,
        Err(err) => return error_envelope(&err),
    };

    match service.query_flags(request) {
        Ok(response) => {
            let flags = response
                .flags
                .into_iter()
                .map(|f| {
                    json!({
                        "tx_id": f.tx_id,
                        "year": f.year,
                        "status": match f.status {
                            FlagStatusRequest::Open => "open",
                            FlagStatusRequest::Resolved => "resolved",
                        },
                        "reason": f.reason,
                        "category": f.category,
                        "confidence": f.confidence,
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "content": [text_content(json!({ "flags": flags }))],
                "isError": false
            })
        }
        Err(err) => error_envelope(&err),
    }
}

#[cfg(feature = "legacy")]
pub fn handle_query_audit_log(service: &TurboLedgerService, arguments: &Value) -> Value {
    let _request = match parse_query_audit_log_request(arguments) {
        Ok(request) => request,
        Err(err) => return error_envelope(&err),
    };

    match service.query_audit_log(QueryAuditLogRequest) {
        Ok(response) => {
            let entries = response
                .entries
                .into_iter()
                .map(|e| {
                    json!({
                        "timestamp": e.timestamp,
                        "actor": e.actor,
                        "tx_id": e.tx_id,
                        "field": e.field,
                        "old_value": e.old_value,
                        "new_value": e.new_value,
                        "note": e.note,
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "content": [text_content(json!({ "entries": entries }))],
                "isError": false
            })
        }
        Err(err) => error_envelope(&err),
    }
}

#[cfg(feature = "legacy")]
pub fn handle_classify_transaction(service: &TurboLedgerService, arguments: &Value) -> Value {
    let request = match parse_classify_transaction_request(arguments) {
        Ok(request) => request,
        Err(err) => return error_envelope(&err),
    };

    match service.classify_transaction(request) {
        Ok(response) => {
            let audit_entries = response
                .audit_entries
                .into_iter()
                .map(|e| {
                    json!({
                        "timestamp": e.timestamp,
                        "actor": e.actor,
                        "tx_id": e.tx_id,
                        "field": e.field,
                        "old_value": e.old_value,
                        "new_value": e.new_value,
                        "note": e.note,
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "content": [text_content(json!({
                        "tx_id": response.tx_id,
                        "category": response.category,
                        "confidence": response.confidence,
                        "audit_entries": audit_entries,
                    }))],
                "isError": false
            })
        }
        Err(err) => error_envelope(&err),
    }
}

#[cfg(feature = "legacy")]
pub fn handle_reconcile_excel_classification(
    service: &TurboLedgerService,
    arguments: &Value,
) -> Value {
    let request = match parse_reconcile_excel_classification_request(arguments) {
        Ok(request) => request,
        Err(err) => return error_envelope(&err),
    };

    match service.reconcile_excel_classification(request) {
        Ok(response) => {
            let audit_entries = response
                .audit_entries
                .into_iter()
                .map(|e| {
                    json!({
                        "timestamp": e.timestamp,
                        "actor": e.actor,
                        "tx_id": e.tx_id,
                        "field": e.field,
                        "old_value": e.old_value,
                        "new_value": e.new_value,
                        "note": e.note,
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "content": [text_content(json!({
                        "tx_id": response.tx_id,
                        "category": response.category,
                        "confidence": response.confidence,
                        "audit_entries": audit_entries,
                    }))],
                "isError": false
            })
        }
        Err(err) => error_envelope(&err),
    }
}

pub fn handle_batch_classify(service: &TurboLedgerService, arguments: &Value) -> Value {
    let request_obj = match arguments.get("request") {
        Some(req) => req,
        None => return error_envelope(&ToolError::InvalidInput("missing 'request' field".to_string())),
    };
    
    let request = match serde_json::from_value::<BatchClassifyRequest>(request_obj.clone()) {
        Ok(req) => req,
        Err(err) => return error_envelope(&ToolError::InvalidInput(format!("invalid BatchClassifyRequest: {}", err))),
    };

    match service.batch_classify(request) {
        Ok(response) => {
            let items_json: Vec<Value> = response.items
                .into_iter()
                .map(|item| {
                    let audit_entries_json: Vec<Value> = item.audit_entries
                        .into_iter()
                        .map(|e| json!({
                            "timestamp": e.timestamp,
                            "actor": e.actor,
                            "tx_id": e.tx_id,
                            "field": e.field,
                            "old_value": e.old_value,
                            "new_value": e.new_value,
                            "note": e.note,
                        }))
                        .collect();
                    
                    {
                        let mut obj = serde_json::Map::new();
                        obj.insert("tx_id".to_string(), json!(item.tx_id));
                        
                        match item.status {
                            BatchItemStatus::Succeeded => {
                                obj.insert("status".to_string(), json!("succeeded"));
                            }
                            BatchItemStatus::Failed { error } => {
                                obj.insert("status".to_string(), json!("failed"));
                                obj.insert("error".to_string(), json!(error));
                            }
                            BatchItemStatus::Skipped { reason } => {
                                obj.insert("status".to_string(), json!("skipped"));
                                obj.insert("reason".to_string(), json!(reason));
                            }
                        }
                        
                        obj.insert("audit_entries".to_string(), json!(audit_entries_json));
                        json!(obj)
                    }
                })
                .collect();
            
            json!({
                "content": [text_content(json!({
                    "summary": {
                        "total_requested": response.summary.total_requested,
                        "succeeded": response.summary.succeeded,
                        "failed": response.summary.failed,
                        "skipped": response.summary.skipped,
                        "batch_duration_ms": response.summary.batch_duration_ms,
                    },
                    "items": items_json,
                }))],
                "isError": false
            })
        }
        Err(err) => error_envelope(&err),
    }
}

pub fn handle_bulk_resolve_flags(service: &TurboLedgerService, arguments: &Value) -> Value {
    let request_obj = match arguments.get("request") {
        Some(req) => req,
        None => return error_envelope(&ToolError::InvalidInput("missing 'request' field".to_string())),
    };
    
    let request = match serde_json::from_value::<BatchResolveFlagsRequest>(request_obj.clone()) {
        Ok(req) => req,
        Err(err) => return error_envelope(&ToolError::InvalidInput(format!("invalid BatchResolveFlagsRequest: {}", err))),
    };

    match service.bulk_resolve_flags(request) {
        Ok(response) => {
            let items_json: Vec<Value> = response.items
                .into_iter()
                .map(|item| {
                    let audit_entries_json: Vec<Value> = item.audit_entries
                        .into_iter()
                        .map(|e| json!({
                            "timestamp": e.timestamp,
                            "actor": e.actor,
                            "tx_id": e.tx_id,
                            "field": e.field,
                            "old_value": e.old_value,
                            "new_value": e.new_value,
                            "note": e.note,
                        }))
                        .collect();
                    
                    {
                        let mut obj = serde_json::Map::new();
                        obj.insert("tx_id".to_string(), json!(item.tx_id));
                        
                        match item.status {
                            BatchItemStatus::Succeeded => {
                                obj.insert("status".to_string(), json!("succeeded"));
                            }
                            BatchItemStatus::Failed { error } => {
                                obj.insert("status".to_string(), json!("failed"));
                                obj.insert("error".to_string(), json!(error));
                            }
                            BatchItemStatus::Skipped { reason } => {
                                obj.insert("status".to_string(), json!("skipped"));
                                obj.insert("reason".to_string(), json!(reason));
                            }
                        }
                        
                        obj.insert("audit_entries".to_string(), json!(audit_entries_json));
                        json!(obj)
                    }
                })
                .collect();
            
            json!({
                "content": [text_content(json!({
                    "summary": {
                        "total_requested": response.summary.total_requested,
                        "succeeded": response.summary.succeeded,
                        "failed": response.summary.failed,
                        "skipped": response.summary.skipped,
                        "batch_duration_ms": response.summary.batch_duration_ms,
                    },
                    "items": items_json,
                }))],
                "isError": false
            })
        }
        Err(err) => error_envelope(&err),
    }
}

pub fn handle_apply_mapping_bulk(service: &TurboLedgerService, arguments: &Value) -> Value {
    let request_obj = match arguments.get("request") {
        Some(req) => req,
        None => return error_envelope(&ToolError::InvalidInput("missing 'request' field".to_string())),
    };
    
    let request = match serde_json::from_value::<ApplyMappingBulkRequest>(request_obj.clone()) {
        Ok(req) => req,
        Err(err) => return error_envelope(&ToolError::InvalidInput(format!("invalid ApplyMappingBulkRequest: {}", err))),
    };

    match service.apply_mapping_bulk(request) {
        Ok(response) => {
            let items_json: Vec<Value> = response.items
                .into_iter()
                .map(|item| {
                    let audit_entries_json: Vec<Value> = item.audit_entries
                        .into_iter()
                        .map(|e| json!({
                            "timestamp": e.timestamp,
                            "actor": e.actor,
                            "tx_id": e.tx_id,
                            "field": e.field,
                            "old_value": e.old_value,
                            "new_value": e.new_value,
                            "note": e.note,
                        }))
                        .collect();
                    
                    {
                        let mut obj = serde_json::Map::new();
                        obj.insert("tx_id".to_string(), json!(item.tx_id));
                        
                        match item.status {
                            BatchItemStatus::Succeeded => {
                                obj.insert("status".to_string(), json!("succeeded"));
                            }
                            BatchItemStatus::Failed { error } => {
                                obj.insert("status".to_string(), json!("failed"));
                                obj.insert("error".to_string(), json!(error));
                            }
                            BatchItemStatus::Skipped { reason } => {
                                obj.insert("status".to_string(), json!("skipped"));
                                obj.insert("reason".to_string(), json!(reason));
                            }
                        }
                        
                        obj.insert("audit_entries".to_string(), json!(audit_entries_json));
                        json!(obj)
                    }
                })
                .collect();
            
            json!({
                "content": [text_content(json!({
                    "classification_summary": {
                        "total_requested": response.classification_summary.total_requested,
                        "succeeded": response.classification_summary.succeeded,
                        "failed": response.classification_summary.failed,
                        "skipped": response.classification_summary.skipped,
                        "batch_duration_ms": response.classification_summary.batch_duration_ms,
                    },
                    "matched_tx_ids": response.matched_tx_ids,
                    "items": items_json,
                }))],
                "isError": false
            })
        }
        Err(err) => error_envelope(&err),
    }
}

#[cfg(feature = "legacy")]
pub fn handle_get_schedule_summary(service: &TurboLedgerService, arguments: &Value) -> Value {
    let request = match parse_get_schedule_summary_request(arguments) {
        Ok(request) => request,
        Err(err) => return error_envelope(&err),
    };

    match service.get_schedule_summary(request) {
        Ok(response) => {
            let schedule_str = match response.schedule {
                ScheduleKindRequest::ScheduleC => "ScheduleC",
                ScheduleKindRequest::ScheduleD => "ScheduleD",
                ScheduleKindRequest::ScheduleE => "ScheduleE",
                ScheduleKindRequest::Fbar => "Fbar",
            };
            let lines = response
                .lines
                .into_iter()
                .map(|l| {
                    json!({
                        "key": l.key,
                        "total": l.total,
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "content": [text_content(json!({
                        "year": response.year,
                        "schedule": schedule_str,
                        "total": response.total,
                        "lines": lines,
                    }))],
                "isError": false
            })
        }
        Err(err) => error_envelope(&err),
    }
}

#[cfg(feature = "legacy")]
pub fn handle_export_cpa_workbook(service: &TurboLedgerService, arguments: &Value) -> Value {
    let request = match parse_export_cpa_workbook_request(arguments) {
        Ok(request) => request,
        Err(err) => return error_envelope(&err),
    };

    match service.export_cpa_workbook(request) {
        Ok(response) => json!({
            "content": [text_content(json!({ "sheets_written": response.sheets_written }))],
            "isError": false
        }),
        Err(err) => error_envelope(&err),
    }
}

/// Parse FetchQueueRequest from MCP arguments
fn parse_fetch_queue_request(arguments: &Value) -> Result<FetchQueueRequest, ToolError> {
    let item_types = parse_optional_queue_item_types(arguments.get("item_types"))?;
    let statuses = parse_optional_queue_statuses(arguments.get("statuses"))?;
    let updated_after = optional_str(arguments, "updated_after");
    let limit = arguments
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(100) as usize;
    let offset = arguments
        .get("offset")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    
    Ok(FetchQueueRequest {
        item_types,
        statuses,
        updated_after,
        limit,
        offset,
    })
}

/// Parse optional queue item types array
fn parse_optional_queue_item_types(value: Option<&Value>) -> Result<Option<Vec<QueueItemType>>, ToolError> {
    match value {
        None => Ok(None),
        Some(v) => {
            let items = v.as_array()
                .ok_or_else(|| ToolError::InvalidInput("item_types must be an array".to_string()))?;
            let result = items.iter()
                .map(|item| {
                    let s = item.as_str()
                        .ok_or_else(|| ToolError::InvalidInput("item_types must contain strings".to_string()))?;
                    match s {
                        "flag" => Ok(QueueItemType::Flag),
                        "ambiguity" => Ok(QueueItemType::Ambiguity),
                        "blocker" => Ok(QueueItemType::Blocker),
                        "document_issue" => Ok(QueueItemType::DocumentIssue),
                        "manual_change" => Ok(QueueItemType::ManualChange),
                        _ => Err(ToolError::InvalidInput(format!("Unknown item_type: {}", s))),
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Some(result))
        }
    }
}

/// Parse optional queue statuses array
fn parse_optional_queue_statuses(value: Option<&Value>) -> Result<Option<Vec<QueueStatus>>, ToolError> {
    match value {
        None => Ok(None),
        Some(v) => {
            let items = v.as_array()
                .ok_or_else(|| ToolError::InvalidInput("statuses must be an array".to_string()))?;
            let result = items.iter()
                .map(|item| {
                    let s = item.as_str()
                        .ok_or_else(|| ToolError::InvalidInput("statuses must contain strings".to_string()))?;
                    match s {
                        "open" => Ok(QueueStatus::Open),
                        "in_progress" => Ok(QueueStatus::InProgress),
                        "resolved" => Ok(QueueStatus::Resolved),
                        "dismissed" => Ok(QueueStatus::Dismissed),
                        _ => Err(ToolError::InvalidInput(format!("Unknown status: {}", s))),
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Some(result))
        }
    }
}

/// Handle fetch_work_queue tool call
#[cfg(feature = "legacy")]
pub fn handle_fetch_queue(service: &TurboLedgerService, arguments: &Value) -> Value {
    let request = match parse_fetch_queue_request(arguments) {
        Ok(request) => request,
        Err(err) => return error_envelope(&err),
    };

    match service.fetch_work_queue(request) {
        Ok(response) => {
            let items = response.items.into_iter().map(|item| {
                json!({
                    "id": item.id,
                    "item_type": item.item_type,
                    "severity": item.severity,
                    "created_at": item.created_at,
                    "status": item.status,
                    "provenance": item.provenance,
                    "related_tx_ids": item.related_tx_ids,
                    "summary": item.summary,
                    "tx_id": item.tx_id,
                    "document_ref": item.document_ref,
                    "metadata": item.metadata,
                })
            }).collect::<Vec<_>>();

            json!({
                "content": [text_content(json!({
                    "items": items,
                    "total_count": response.total_count,
                    "offset": response.offset,
                    "limit": response.limit,
                }))],
            })
        }
        Err(err) => error_envelope(&err),
    }
}

#[cfg(feature = "legacy")]
pub fn handle_ontology_upsert_entities(service: &TurboLedgerService, arguments: &Value) -> Value {
    let request = match parse_ontology_upsert_entities_request(arguments) {
        Ok(request) => request,
        Err(err) => return error_envelope(&err),
    };

    match service.ontology_upsert_entities_tool(request) {
        Ok(response) => json!({
            "content": [text_content(json!({ "upserted": response.inserted_count }))],
            "isError": false
        }),
        Err(err) => error_envelope(&err),
    }
}

#[cfg(feature = "legacy")]
pub fn handle_ontology_upsert_edges(service: &TurboLedgerService, arguments: &Value) -> Value {
    let request = match parse_ontology_upsert_edges_request(arguments) {
        Ok(request) => request,
        Err(err) => return error_envelope(&err),
    };

    match service.ontology_upsert_edges_tool(request) {
        Ok(response) => json!({
            "content": [text_content(json!({ "upserted": response.inserted_count }))],
            "isError": false
        }),
        Err(err) => error_envelope(&err),
    }
}

fn infer_currency(account_id: &str) -> String {
    let upper = account_id.to_ascii_uppercase();
    if upper.contains("BTC") {
        "BTC".to_string()
    } else {
        "USD".to_string()
    }
}

fn required_str<'a>(obj: &'a Value, key: &str) -> Result<&'a str, ToolError> {
    obj.get(key).and_then(Value::as_str).ok_or_else(|| {
        ToolError::InvalidInput(format!("missing or invalid `{key}` in tool arguments"))
    })
}

fn parse_ontology_path(arguments: &Value) -> Result<PathBuf, ToolError> {
    Ok(PathBuf::from(required_str(arguments, "ontology_path")?))
}

fn parse_get_raw_context_request(arguments: &Value) -> Result<GetRawContextRequest, ToolError> {
    Ok(GetRawContextRequest {
        rkyv_ref: PathBuf::from(required_str(arguments, "rkyv_ref")?),
    })
}

fn parse_document_inventory_request(
    arguments: &Value,
) -> Result<DocumentInventoryRequest, ToolError> {
    let directory = PathBuf::from(required_str(arguments, "directory")?);
    let recursive = arguments
        .get("recursive")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let statuses = match arguments.get("statuses") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| {
                let raw = item.as_str().ok_or_else(|| {
                    ToolError::InvalidInput(
                        "`statuses` must contain only string status names".to_string(),
                    )
                })?;
                DocumentQueueStatusRequest::parse(raw).ok_or_else(|| {
                    ToolError::InvalidInput(format!(
                        "`statuses` contains unsupported value `{raw}`"
                    ))
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
        _ => {
            return Err(ToolError::InvalidInput(
                "`statuses` must be an array of status names".to_string(),
            ))
        }
    };
    Ok(DocumentInventoryRequest {
        directory,
        recursive,
        statuses,
    })
}

fn document_status_counts(documents: &[Value]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for document in documents {
        if let Some(status) = document.get("status").and_then(Value::as_str) {
            *counts.entry(status.to_string()).or_insert(0) += 1;
        }
    }
    counts
}

fn parse_ontology_query_path_request(
    arguments: &Value,
) -> Result<OntologyQueryPathRequest, ToolError> {
    let ontology_path = parse_ontology_path(arguments)?;
    let from_entity_id = required_str(arguments, "from_entity_id")?.to_string();
    let max_depth = match arguments.get("max_depth") {
        None | Some(Value::Null) => None,
        Some(Value::Number(num)) => {
            let raw = num.as_u64().ok_or_else(|| {
                ToolError::InvalidInput("`max_depth` must be a non-negative integer".to_string())
            })?;
            Some(usize::try_from(raw).map_err(|_| {
                ToolError::InvalidInput("`max_depth` is too large for this platform".to_string())
            })?)
        }
        _ => {
            return Err(ToolError::InvalidInput(
                "`max_depth` must be null or a non-negative integer".to_string(),
            ))
        }
    };

    Ok(OntologyQueryPathRequest {
        ontology_path,
        from_entity_id,
        max_depth,
    })
}

fn parse_reconciliation_stage_request(
    arguments: &Value,
) -> Result<ReconciliationStageRequest, ToolError> {
    let source_total = required_str(arguments, "source_total")?.to_string();
    let extracted_total = required_str(arguments, "extracted_total")?.to_string();
    let posting_amounts = parse_string_array(arguments.get("posting_amounts"), "posting_amounts")?;
    Ok(ReconciliationStageRequest {
        source_total,
        extracted_total,
        posting_amounts,
    })
}

fn parse_hsm_transition_request(arguments: &Value) -> Result<HsmTransitionRequest, ToolError> {
    let target_state = required_str(arguments, "target_state")?.to_string();
    let target_substate = required_str(arguments, "target_substate")?.to_string();
    Ok(HsmTransitionRequest {
        target_state,
        target_substate,
    })
}

fn parse_hsm_resume_request(arguments: &Value) -> Result<HsmResumeRequest, ToolError> {
    let state_marker = required_str(arguments, "state_marker")?.to_string();
    Ok(HsmResumeRequest { state_marker })
}

fn parse_event_history_filter(arguments: &Value) -> Result<EventHistoryFilter, ToolError> {
    Ok(EventHistoryFilter {
        tx_id: optional_str(arguments, "tx_id"),
        document_ref: optional_str(arguments, "document_ref"),
        time_start: optional_str(arguments, "time_start"),
        time_end: optional_str(arguments, "time_end"),
    })
}

fn parse_replay_lifecycle_request(arguments: &Value) -> Result<ReplayLifecycleRequest, ToolError> {
    Ok(ReplayLifecycleRequest {
        tx_id: optional_str(arguments, "tx_id"),
        document_ref: optional_str(arguments, "document_ref"),
    })
}

fn parse_tax_assist_request(arguments: &Value) -> Result<TaxAssistRequest, ToolError> {
    let ontology_path = PathBuf::from(required_str(arguments, "ontology_path")?);
    let from_entity_id = required_str(arguments, "from_entity_id")?.to_string();
    let max_depth = parse_optional_max_depth(arguments.get("max_depth"))?;
    let reconciliation = parse_nested_reconciliation_request(arguments)?;
    Ok(TaxAssistRequest {
        ontology_path,
        from_entity_id,
        max_depth,
        reconciliation,
    })
}

fn parse_tax_evidence_chain_request(
    arguments: &Value,
) -> Result<TaxEvidenceChainRequest, ToolError> {
    let ontology_path = PathBuf::from(required_str(arguments, "ontology_path")?);
    let from_entity_id = required_str(arguments, "from_entity_id")?.to_string();
    let tx_id = optional_str(arguments, "tx_id");
    let document_ref = optional_str(arguments, "document_ref");
    Ok(TaxEvidenceChainRequest {
        ontology_path,
        from_entity_id,
        tx_id,
        document_ref,
    })
}

fn parse_tax_ambiguity_review_request(
    arguments: &Value,
) -> Result<TaxAmbiguityReviewRequest, ToolError> {
    let ontology_path = PathBuf::from(required_str(arguments, "ontology_path")?);
    let from_entity_id = required_str(arguments, "from_entity_id")?.to_string();
    let max_depth = parse_optional_max_depth(arguments.get("max_depth"))?;
    let reconciliation = parse_nested_reconciliation_request(arguments)?;
    Ok(TaxAmbiguityReviewRequest {
        ontology_path,
        from_entity_id,
        max_depth,
        reconciliation,
    })
}

fn parse_nested_reconciliation_request(
    arguments: &Value,
) -> Result<ReconciliationStageRequest, ToolError> {
    let reconciliation = arguments.get("reconciliation").ok_or_else(|| {
        ToolError::InvalidInput("missing or invalid `reconciliation` in tool arguments".to_string())
    })?;
    parse_reconciliation_stage_request(reconciliation)
}

fn parse_optional_max_depth(value: Option<&Value>) -> Result<Option<usize>, ToolError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(num)) => {
            let raw = num.as_u64().ok_or_else(|| {
                ToolError::InvalidInput("`max_depth` must be a non-negative integer".to_string())
            })?;
            let depth = usize::try_from(raw).map_err(|_| {
                ToolError::InvalidInput("`max_depth` is too large for this platform".to_string())
            })?;
            Ok(Some(depth))
        }
        _ => Err(ToolError::InvalidInput(
            "missing or invalid `max_depth` in tool arguments".to_string(),
        )),
    }
}

fn parse_optional_bytes(value: Option<&Value>) -> Result<Option<Vec<u8>>, ToolError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| {
                let num = item.as_u64().ok_or_else(|| {
                    ToolError::InvalidInput(
                        "raw_context_bytes must be an array of bytes".to_string(),
                    )
                })?;
                u8::try_from(num).map_err(|_| {
                    ToolError::InvalidInput(
                        "raw_context_bytes values must be in range 0..=255".to_string(),
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some),
        _ => Err(ToolError::InvalidInput(
            "raw_context_bytes must be null or an array of bytes".to_string(),
        )),
    }
}

fn parse_rows(value: Option<&Value>, field_name: &str) -> Result<Vec<TransactionInput>, ToolError> {
    let rows = value
        .and_then(Value::as_array)
        .ok_or_else(|| ToolError::InvalidInput(format!("missing or invalid `{field_name}`")))?;

    rows.iter()
        .map(|row| {
            Ok(TransactionInput {
                account_id: row
                    .get("account_id")
                    .and_then(Value::as_str)
                    .or_else(|| row.get("account").and_then(Value::as_str))
                    .ok_or_else(|| {
                        ToolError::InvalidInput(
                            "missing or invalid `account_id` in tool arguments".to_string(),
                        )
                    })?
                    .to_string(),
                date: required_str(row, "date")?.to_string(),
                amount: required_str(row, "amount")?.to_string(),
                description: required_str(row, "description")?.to_string(),
                source_ref: required_str(row, "source_ref")?.to_string(),
            })
        })
        .collect()
}

fn optional_str(obj: &Value, key: &str) -> Option<String> {
    obj.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn parse_string_array(value: Option<&Value>, field_name: &str) -> Result<Vec<String>, ToolError> {
    let items = value
        .and_then(Value::as_array)
        .ok_or_else(|| ToolError::InvalidInput(format!("missing or invalid `{field_name}`")))?;

    items
        .iter()
        .map(|item| {
            item.as_str().map(ToString::to_string).ok_or_else(|| {
                ToolError::InvalidInput(format!("`{field_name}` must contain strings"))
            })
        })
        .collect()
}

fn parse_classify_ingested_request(
    arguments: &Value,
) -> Result<ClassifyIngestedRequest, ToolError> {
    let rule_file = PathBuf::from(required_str(arguments, "rule_file")?);
    let review_threshold = match arguments.get("review_threshold") {
        Some(Value::String(s)) => s.parse::<f64>().map_err(|_| {
            ToolError::InvalidInput("review_threshold must be a valid f64".to_string())
        })?,
        Some(Value::Number(n)) => n.as_f64().ok_or_else(|| {
            ToolError::InvalidInput("review_threshold must be a valid number".to_string())
        })?,
        _ => {
            return Err(ToolError::InvalidInput(
                "missing or invalid `review_threshold` in tool arguments".to_string(),
            ))
        }
    };
    Ok(ClassifyIngestedRequest {
        rule_file,
        review_threshold,
    })
}

fn parse_run_rhai_rule_request(arguments: &Value) -> Result<RunRhaiRuleRequest, ToolError> {
    let rule_file = PathBuf::from(required_str(arguments, "rule_file")?);
    let sample_tx = arguments.get("sample_tx").ok_or_else(|| {
        ToolError::InvalidInput("missing or invalid `sample_tx` in tool arguments".to_string())
    })?;
    Ok(RunRhaiRuleRequest {
        rule_file,
        sample_tx: SampleTxRequest {
            tx_id: required_str(sample_tx, "tx_id")?.to_string(),
            account_id: required_str(sample_tx, "account_id")?.to_string(),
            date: required_str(sample_tx, "date")?.to_string(),
            amount: required_str(sample_tx, "amount")?.to_string(),
            description: required_str(sample_tx, "description")?.to_string(),
        },
    })
}

fn parse_query_flags_request(arguments: &Value) -> Result<QueryFlagsRequest, ToolError> {
    let year = match arguments.get("year") {
        Some(Value::Number(n)) => n
            .as_i64()
            .ok_or_else(|| ToolError::InvalidInput("year must be a valid integer".to_string()))?
            as i32,
        _ => {
            return Err(ToolError::InvalidInput(
                "missing or invalid `year` in tool arguments".to_string(),
            ))
        }
    };
    let status = match arguments.get("status").and_then(Value::as_str) {
        Some("open") => FlagStatusRequest::Open,
        Some("resolved") => FlagStatusRequest::Resolved,
        _ => {
            return Err(ToolError::InvalidInput(
                "missing or invalid `status` in tool arguments (must be 'open' or 'resolved')"
                    .to_string(),
            ))
        }
    };
    Ok(QueryFlagsRequest { year, status })
}

fn parse_query_audit_log_request(_arguments: &Value) -> Result<QueryAuditLogRequest, ToolError> {
    Ok(QueryAuditLogRequest)
}

fn parse_classify_transaction_request(
    arguments: &Value,
) -> Result<ClassifyTransactionRequest, ToolError> {
    let tx_id = required_str(arguments, "tx_id")?.to_string();
    let category = required_str(arguments, "category")?.to_string();
    let confidence = required_str(arguments, "confidence")?.to_string();
    let note = optional_str(arguments, "note");
    let actor = required_str(arguments, "actor")?.to_string();
    Ok(ClassifyTransactionRequest {
        tx_id,
        category,
        confidence,
        note,
        actor,
    })
}

fn parse_reconcile_excel_classification_request(
    arguments: &Value,
) -> Result<ReconcileExcelClassificationRequest, ToolError> {
    let tx_id = required_str(arguments, "tx_id")?.to_string();
    let category = required_str(arguments, "category")?.to_string();
    let confidence = required_str(arguments, "confidence")?.to_string();
    let note = optional_str(arguments, "note");
    let actor = required_str(arguments, "actor")?.to_string();
    Ok(ReconcileExcelClassificationRequest {
        tx_id,
        category,
        confidence,
        note,
        actor,
    })
}

fn parse_get_schedule_summary_request(
    arguments: &Value,
) -> Result<GetScheduleSummaryRequest, ToolError> {
    let year = match arguments.get("year") {
        Some(Value::Number(n)) => n
            .as_i64()
            .ok_or_else(|| ToolError::InvalidInput("year must be a valid integer".to_string()))?
            as i32,
        _ => {
            return Err(ToolError::InvalidInput(
                "missing or invalid `year` in tool arguments".to_string(),
            ))
        }
    };
    let schedule = match arguments.get("schedule").and_then(Value::as_str) {
        Some("ScheduleC") => ScheduleKindRequest::ScheduleC,
        Some("ScheduleD") => ScheduleKindRequest::ScheduleD,
        Some("ScheduleE") => ScheduleKindRequest::ScheduleE,
        Some("Fbar") => ScheduleKindRequest::Fbar,
        _ => {
            return Err(ToolError::InvalidInput(
                "missing or invalid `schedule` in tool arguments (must be 'ScheduleC', 'ScheduleD', 'ScheduleE', or 'Fbar')".to_string(),
            ))
        }
    };
    Ok(GetScheduleSummaryRequest { year, schedule })
}

fn parse_export_cpa_workbook_request(
    arguments: &Value,
) -> Result<ExportCpaWorkbookRequest, ToolError> {
    let workbook_path = PathBuf::from(required_str(arguments, "workbook_path")?);
    Ok(ExportCpaWorkbookRequest { workbook_path })
}

fn parse_ontology_upsert_entities_request(
    arguments: &Value,
) -> Result<OntologyUpsertEntitiesRequest, ToolError> {
    let ontology_path = parse_ontology_path(arguments)?;
    let entities_json = arguments
        .get("entities")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            ToolError::InvalidInput("missing or invalid `entities` in tool arguments".to_string())
        })?;
    let entities = entities_json
        .iter()
        .map(|e| {
            let kind = parse_ontology_entity_kind(e.get("kind").and_then(Value::as_str))?;
            let mut attrs = std::collections::BTreeMap::new();
            if let Some(id) = e.get("id").and_then(Value::as_str) {
                attrs.insert("id".to_string(), id.to_string());
            }
            if let Some(label) = e.get("label").and_then(Value::as_str) {
                attrs.insert("label".to_string(), label.to_string());
            }
            if let Some(obj) = e.get("properties").and_then(Value::as_object) {
                for (k, v) in obj {
                    attrs.insert(k.clone(), v.to_string());
                }
            }
            Ok::<crate::OntologyEntityInput, ToolError>(crate::OntologyEntityInput { kind, attrs })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(OntologyUpsertEntitiesRequest {
        ontology_path,
        entities,
    })
}

fn parse_ontology_upsert_edges_request(
    arguments: &Value,
) -> Result<OntologyUpsertEdgesRequest, ToolError> {
    let ontology_path = parse_ontology_path(arguments)?;
    let edges_json = arguments
        .get("edges")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            ToolError::InvalidInput("missing or invalid `edges` in tool arguments".to_string())
        })?;
    let edges = edges_json
        .iter()
        .map(|e| {
            let from = required_str(e, "from_id").map(|s| s.to_string())?;
            let to = required_str(e, "to_id").map(|s| s.to_string())?;
            let relation = required_str(e, "relation").map(|s| s.to_string())?;
            let provenance = e
                .get("provenance")
                .and_then(Value::as_object)
                .map(|obj| {
                    obj.iter()
                        .map(|(k, v)| (k.clone(), v.to_string()))
                        .collect::<std::collections::BTreeMap<_, _>>()
                })
                .unwrap_or_default();
            Ok::<crate::OntologyEdgeInput, ToolError>(crate::OntologyEdgeInput {
                from,
                to,
                relation,
                provenance,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(OntologyUpsertEdgesRequest {
        ontology_path,
        edges,
    })
}

fn parse_ontology_entity_kind(raw: Option<&str>) -> Result<crate::OntologyEntityKind, ToolError> {
    match raw {
        Some("Document") => Ok(crate::OntologyEntityKind::Document),
        Some("Account") => Ok(crate::OntologyEntityKind::Account),
        Some("Institution") => Ok(crate::OntologyEntityKind::Institution),
        Some("Transaction") => Ok(crate::OntologyEntityKind::Transaction),
        Some("TaxCategory") => Ok(crate::OntologyEntityKind::TaxCategory),
        Some("EvidenceReference") => Ok(crate::OntologyEntityKind::EvidenceReference),
        Some("XeroContact") => Ok(crate::OntologyEntityKind::XeroContact),
        Some("XeroBankAccount") => Ok(crate::OntologyEntityKind::XeroBankAccount),
        Some("XeroInvoice") => Ok(crate::OntologyEntityKind::XeroInvoice),
        Some("WorkflowTag") => Ok(crate::OntologyEntityKind::WorkflowTag),
        _ => Err(ToolError::InvalidInput(
            "missing or invalid `kind` in entity (must be Document, Account, Institution, Transaction, TaxCategory, EvidenceReference, XeroContact, XeroBankAccount, XeroInvoice, or WorkflowTag)".to_string(),
        )),
    }
}

#[cfg(feature = "legacy")]
/// Parse a user-supplied node type string into the typed `arc_kit_au::NodeType`.
///
/// Accepts both canonical snake_case names and the single-char prefixes used in
/// `NodeId` strings so operators can use either form interchangeably.
fn parse_evidence_node_type(s: &str) -> Option<arc_kit_au::NodeType> {
    use arc_kit_au::NodeType;
    Some(match s {
        "source_doc" | "doc" => NodeType::SourceDoc,
        "extracted_row" | "row" => NodeType::ExtractedRow,
        "transaction" | "tx" => NodeType::Transaction,
        "classification" | "cls" => NodeType::Classification,
        "model_proposal" | "prop" => NodeType::ModelProposal,
        "operator_approval" | "approval" => NodeType::OperatorApproval,
        "workbook_row" | "wb" => NodeType::WorkbookRow,
        "validation_issue" | "vi" => NodeType::ValidationIssue,
        _ => return None,
    })
}

#[cfg(feature = "legacy")]
/// Canonical snake_case label for an `arc_kit_au::NodeType` (MCP output surface).
fn evidence_node_type_label(nt: arc_kit_au::NodeType) -> &'static str {
    use arc_kit_au::NodeType;
    match nt {
        NodeType::SourceDoc => "source_doc",
        NodeType::ExtractedRow => "extracted_row",
        NodeType::Transaction => "transaction",
        NodeType::Classification => "classification",
        NodeType::ModelProposal => "model_proposal",
        NodeType::OperatorApproval => "operator_approval",
        NodeType::WorkbookRow => "workbook_row",
        NodeType::ValidationIssue => "validation_issue",
        NodeType::Unknown => "unknown",
    }
}

#[cfg(feature = "legacy")]
pub fn handle_evidence_tool(service: &TurboLedgerService, arguments: &Value) -> Value {
    use crate::contract::parse_evidence;

    let request = match parse_evidence(arguments) {
        Ok(r) => r,
        Err(err) => return error_envelope(&err),
    };

    match request {
        EvidenceArgs::ProvenanceGaps => {
            use arc_kit_au::ProvenanceScanner;
            let evidence = match service.evidence.lock() {
                Ok(e) => e,
                Err(_) => {
                    return error_envelope(&ToolError::Internal(
                        "evidence mutex poisoned".to_string(),
                    ))
                }
            };
            let gaps = evidence.find_missing_provenance();
            let gap_jsons: Vec<_> = gaps
                .iter()
                .map(|g| {
                    json!({
                        "tx_id": g.tx_id,
                        "has_source": g.has_source,
                        "has_classification": g.has_classification,
                        "has_approval": g.has_approval,
                        "has_export": g.has_export,
                        "is_critical": g.is_critical(),
                        "missing": g.missing.iter().map(|m| m.to_string()).collect::<Vec<_>>(),
                    })
                })
                .collect();
            json!({
                "content": [text_content(json!({
                    "action": "provenance_gaps",
                    "gaps": gap_jsons,
                    "count": gaps.len(),
                }))],
                "isError": false
            })
        }
        EvidenceArgs::TraceTx { tx_id } => {
            use arc_kit_au::EvidenceTracer;
            let evidence = match service.evidence.lock() {
                Ok(e) => e,
                Err(_) => {
                    return error_envelope(&ToolError::Internal(
                        "evidence mutex poisoned".to_string(),
                    ))
                }
            };
            match evidence.trace_transaction(&tx_id) {
                Some(chain) => {
                    use arc_kit_au::ProvenanceBadge;
                    let badge = ProvenanceBadge::from(&chain);
                    json!({
                        "content": [text_content(json!({
                            "action": "trace_tx",
                            "tx_id": tx_id,
                            "provenance_badge": badge.label(),
                            "has_complete_provenance": chain.has_complete_provenance(),
                            "source_count": chain.source_count(),
                            "proposal_count": chain.proposal_count(),
                            "approval_count": chain.approval_count(),
                            "export_count": chain.export_count(),
                            "missing": chain.missing_elements(),
                        }))],
                        "isError": false
                    })
                }
                None => json!({
                    "content": [text_content(json!({
                        "action": "trace_tx",
                        "tx_id": tx_id,
                        "provenance_badge": "not_found",
                        "message": "No evidence chain found for this transaction.",
                    }))],
                    "isError": false
                }),
            }
        }
        EvidenceArgs::Summary => {
            let evidence = match service.evidence.lock() {
                Ok(e) => e,
                Err(_) => {
                    return error_envelope(&ToolError::Internal(
                        "evidence mutex poisoned".to_string(),
                    ))
                }
            };
            // Single pass over all nodes to build per-type counts.
            let mut counts: std::collections::HashMap<&'static str, usize> =
                std::collections::HashMap::new();
            for node in evidence.all_nodes() {
                *counts
                    .entry(evidence_node_type_label(node.node_type()))
                    .or_insert(0) += 1;
            }
            let node_counts = json!({
                "source_docs":        counts.get("source_doc").copied().unwrap_or(0),
                "extracted_rows":     counts.get("extracted_row").copied().unwrap_or(0),
                "transactions":       counts.get("transaction").copied().unwrap_or(0),
                "classifications":    counts.get("classification").copied().unwrap_or(0),
                "model_proposals":    counts.get("model_proposal").copied().unwrap_or(0),
                "operator_approvals": counts.get("operator_approval").copied().unwrap_or(0),
                "workbook_rows":      counts.get("workbook_row").copied().unwrap_or(0),
                "validation_issues":  counts.get("validation_issue").copied().unwrap_or(0),
            });
            let wq = evidence.work_queue_summary();
            json!({
                "content": [text_content(json!({
                    "action": "summary",
                    "total_nodes": evidence.node_count(),
                    "total_edges": evidence.edge_count(),
                    "node_counts": node_counts,
                    "work_queue": {
                        "total_transactions":    wq.total_transactions,
                        "ready_to_review":       wq.ready_to_review,
                        "blocked":               wq.blocked,
                        "exported":              wq.exported,
                        "with_validation_issues":wq.with_validation_issues,
                    },
                }))],
                "isError": false
            })
        }
        EvidenceArgs::ListNodes { node_type } => {
            let evidence = match service.evidence.lock() {
                Ok(e) => e,
                Err(_) => {
                    return error_envelope(&ToolError::Internal(
                        "evidence mutex poisoned".to_string(),
                    ))
                }
            };
            let nodes: Vec<&arc_kit_au::EvidenceNode> = match node_type {
                Some(ref nt) => match parse_evidence_node_type(nt) {
                    Some(parsed) => evidence.nodes_of_type(parsed),
                    None => {
                        return json!({
                            "content": [text_content(json!({
                                "error": format!(
                                    "Unknown node type: {nt}. Valid types: \
                                     source_doc, extracted_row, transaction, classification, \
                                     model_proposal, operator_approval, workbook_row, validation_issue"
                                ),
                            }))],
                            "isError": true,
                        })
                    }
                },
                None => evidence.all_nodes().iter().collect(),
            };
            let node_summaries: Vec<_> = nodes
                .iter()
                .map(|n| {
                    json!({
                        "node_id":   n.node_id().to_string(),
                        "node_type": evidence_node_type_label(n.node_type()),
                    })
                })
                .collect();
            json!({
                "content": [text_content(json!({
                    "action": "list_nodes",
                    "count":  node_summaries.len(),
                    "nodes":  node_summaries,
                }))],
                "isError": false
            })
        }
        EvidenceArgs::NodeDetail { node_id } => {
            let evidence = match service.evidence.lock() {
                Ok(e) => e,
                Err(_) => {
                    return error_envelope(&ToolError::Internal(
                        "evidence mutex poisoned".to_string(),
                    ))
                }
            };
            let id = arc_kit_au::NodeId(node_id.clone());
            match evidence.get_node(&id) {
                Some(node) => json!({
                    "content": [text_content(json!({
                        "action":  "node_detail",
                        "node_id": node_id,
                        "node":    node,
                    }))],
                    "isError": false
                }),
                None => json!({
                    "content": [text_content(json!({
                        "action":  "node_detail",
                        "node_id": node_id,
                        "error":   "Node not found in evidence graph",
                    }))],
                    "isError": true,
                }),
            }
        }
    }
}
