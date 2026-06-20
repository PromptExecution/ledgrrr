use std::path::PathBuf;

use crossbeam::channel::Sender;

use crate::{
    ClassifyIngestedRequest, ClassifyIngestedResponse, ClassifyTransactionRequest,
    ClassifyTransactionResponse, DocumentInventoryRequest, DocumentInventoryResponse,
    EventHistoryFilter, EventHistoryResponse, ExportCpaWorkbookRequest, ExportCpaWorkbookResponse,
    GetRawContextRequest, GetRawContextResponse, GetScheduleSummaryRequest,
    GetScheduleSummaryResponse, HsmResumeRequest, HsmResumeResponse, HsmStatusRequest,
    HsmStatusResponse, HsmTransitionRequest, HsmTransitionResponse, IngestImageRequest,
    IngestImageResponse, IngestPdfRequest, IngestPdfResponse, IngestStatementRowsRequest,
    IngestStatementRowsResponse, NormalizeFilenameRequest, NormalizeFilenameResponse,
    OntologyExportSnapshotRequest, OntologyExportSnapshotResponse, OntologyQueryPathRequest,
    OntologyQueryPathResponse, OntologyUpsertEdgesRequest, OntologyUpsertEdgesResponse,
    OntologyUpsertEntitiesRequest, OntologyUpsertEntitiesResponse, QueryAuditLogRequest,
    QueryAuditLogResponse, QueryFlagsRequest, QueryFlagsResponse, ReconciliationStageRequest,
    ReconciliationStageResponse, ReplayLifecycleRequest, ReplayLifecycleResponse,
    RunRhaiRuleRequest, RunRhaiRuleResponse, SyncFsMetadataRequest, SyncFsMetadataResponse,
    TaxAmbiguityReviewRequest, TaxAmbiguityReviewResponse, TaxAssistRequest, TaxAssistResponse,
    TaxEvidenceChainRequest, TaxEvidenceChainResponse, ToolError,
};

use ledger_core::filename::StatementFilename;

/// Authorization response for b00t datum delegation requests.
#[derive(Debug, Clone)]
pub struct DelegateAuthority {
    pub authorized: bool,
    pub datum_id: String,
    pub agent_id: String,
    pub task_id: String,
    /// Remaining budget after this authorization (USD).
    pub budget_remaining_usd: rust_decimal::Decimal,
    /// Opaque token b00t uses to resume after a pause.
    pub resume_token: String,
    /// If !authorized, human-readable reason.
    pub denial_reason: Option<String>,
}

/// Tool/action mapping for AGT policy enforcement.
pub struct ToolActionMapping {
    pub tool_name: &'static str,
    pub action: &'static str,
}

impl ToolActionMapping {
    /// Map a GateMessage variant to its (tool_name, action) pair for AGT policy check.
    /// Returns None for messages that don't require policy enforcement (e.g., Shutdown).
    ///
    /// **Gap 1 (PRD-10) Scope Notes**:
    /// - `edit_rhai_rule`: Admin ring - NOT YET IMPLEMENTED (no GateMessage variant exists)
    /// - `commit_workbook`: Standard ring - NOT YET IMPLEMENTED (no GateMessage variant exists)
    /// - `promote_agent`: Admin ring - NOT YET IMPLEMENTED (no GateMessage variant exists)
    ///
    /// These mappings will be added when the corresponding GateMessage variants are created
    /// in future phases (Gap 2+ for edit_rhai_rule, Gap 3+ for commit_workbook).
    pub fn from_message(msg: &GateMessage) -> Option<(&'static str, &'static str)> {
        match msg {
            GateMessage::ListAccounts { .. } => Some(("ledgerr_documents", "list_accounts")),
            GateMessage::ListAccountsTool { .. } => Some(("ledgerr_documents", "list_accounts")),
            GateMessage::DocumentInventory { .. } => Some(("ledgerr_documents", "document_inventory")),
            GateMessage::ValidateFilename { .. } => Some(("ledgerr_documents", "validate_filename")),
            GateMessage::IngestStatementRows { .. } => Some(("ledgerr_documents", "ingest_rows")),
            GateMessage::IngestPdf { .. } => Some(("ledgerr_documents", "ingest_pdf")),
            GateMessage::GetRawContext { .. } => Some(("ledgerr_documents", "get_raw_context")),
            GateMessage::RunRhaiRule { .. } => Some(("ledgerr_review", "run_rule")),
            GateMessage::ClassifyIngested { .. } => Some(("ledgerr_review", "classify_ingested")),
            GateMessage::QueryFlags { .. } => Some(("ledgerr_review", "query_flags")),
            GateMessage::ClassifyTransaction { .. } => Some(("ledgerr_review", "classify_transaction")),
            GateMessage::ReconcileExcelClassification { .. } => Some(("ledgerr_review", "reconcile_excel_classification")),
            GateMessage::QueryAuditLog { .. } => Some(("ledgerr_audit", "query_audit_log")),
            GateMessage::ExportCpaWorkbook { .. } => Some(("ledgerr_audit", "export_cpa_workbook")),
            GateMessage::GetScheduleSummary { .. } => Some(("ledgerr_tax", "get_schedule_summary")),
            GateMessage::HsmTransition { .. } => Some(("ledgerr_workflow", "transition")),
            GateMessage::HsmStatus { .. } => Some(("ledgerr_workflow", "status")),
            GateMessage::HsmResume { .. } => Some(("ledgerr_workflow", "resume")),
            GateMessage::EventHistory { .. } => Some(("ledgerr_workflow", "event_history")),
            GateMessage::ReplayLifecycle { .. } => Some(("ledgerr_workflow", "replay_lifecycle")),
            GateMessage::TaxAssist { .. } => Some(("ledgerr_tax", "tax_assist")),
            GateMessage::TaxEvidenceChain { .. } => Some(("ledgerr_tax", "tax_evidence_chain")),
            GateMessage::TaxAmbiguityReview { .. } => Some(("ledgerr_tax", "tax_ambiguity_review")),
            GateMessage::ValidateReconciliationStage { .. } => Some(("ledgerr_reconciliation", "validate")),
            GateMessage::ReconcileReconciliationStage { .. } => Some(("ledgerr_reconciliation", "reconcile")),
            GateMessage::CommitReconciliationStage { .. } => Some(("ledgerr_reconciliation", "commit")),
            GateMessage::AdjustTransaction { .. } => Some(("ledgerr_review", "adjust_transaction")),
            GateMessage::OntologyUpsertEntities { .. } => Some(("ledgerr_ontology", "upsert_entities")),
            GateMessage::OntologyUpsertEdges { .. } => Some(("ledgerr_ontology", "upsert_edges")),
            GateMessage::OntologyQueryPath { .. } => Some(("ledgerr_ontology", "query_path")),
            GateMessage::OntologyExportSnapshot { .. } => Some(("ledgerr_ontology", "export_snapshot")),
            GateMessage::IngestImage { .. } => Some(("ledgerr_documents", "ingest_image")),
            GateMessage::ApplyTags { .. } => Some(("ledgerr_documents", "apply_tags")),
            GateMessage::RemoveTags { .. } => Some(("ledgerr_documents", "remove_tags")),
            GateMessage::ListTagged { .. } => Some(("ledgerr_documents", "list_tagged")),
            GateMessage::SyncFsMetadata { .. } => Some(("ledgerr_documents", "sync_fs_metadata")),
            GateMessage::NormalizeFilename { .. } => Some(("ledgerr_documents", "normalize_filename")),
            #[cfg(feature = "xero")]
            GateMessage::XeroGetAuthUrl { .. } => Some(("ledgerr_xero", "get_auth_url")),
            #[cfg(feature = "xero")]
            GateMessage::XeroExchangeCode { .. } => Some(("ledgerr_xero", "exchange_code")),
            #[cfg(feature = "xero")]
            GateMessage::XeroFetchContacts { .. } => Some(("ledgerr_xero", "fetch_contacts")),
            #[cfg(feature = "xero")]
            GateMessage::XeroFetchAccounts { .. } => Some(("ledgerr_xero", "fetch_accounts")),
            #[cfg(feature = "xero")]
            GateMessage::XeroFetchBankAccounts { .. } => Some(("ledgerr_xero", "fetch_bank_accounts")),
            #[cfg(feature = "xero")]
            GateMessage::XeroFetchInvoices { .. } => Some(("ledgerr_xero", "fetch_invoices")),
            #[cfg(feature = "xero")]
            GateMessage::XeroLinkEntity { .. } => Some(("ledgerr_xero", "link_entity")),
            #[cfg(feature = "xero")]
            GateMessage::XeroSyncCatalog { .. } => Some(("ledgerr_xero", "sync_catalog")),
            #[cfg(feature = "b00t")]
            GateMessage::BootDatumDelegate { .. } => Some(("ledgerr_b00t", "delegate_datum")),
            GateMessage::Shutdown => None,
        }
    }
}

#[allow(clippy::large_enum_variant)]
pub enum GateMessage {
    ListAccounts {
        agent_id: String,
        reply_tx: Sender<Result<Vec<crate::AccountSummary>, ToolError>>,
    },
    ListAccountsTool {
        agent_id: String,
        request: crate::ListAccountsRequest,
        reply_tx: Sender<Result<crate::ListAccountsResponse, ToolError>>,
    },
    DocumentInventory {
        agent_id: String,
        request: DocumentInventoryRequest,
        reply_tx: Sender<Result<DocumentInventoryResponse, ToolError>>,
    },
    ValidateFilename {
        agent_id: String,
        file_name: String,
        reply_tx: Sender<Result<StatementFilename, ToolError>>,
    },
    IngestStatementRows {
        agent_id: String,
        request: IngestStatementRowsRequest,
        reply_tx: Sender<Result<IngestStatementRowsResponse, ToolError>>,
    },
    IngestPdf {
        agent_id: String,
        request: IngestPdfRequest,
        reply_tx: Sender<Result<IngestPdfResponse, ToolError>>,
    },
    GetRawContext {
        agent_id: String,
        request: GetRawContextRequest,
        reply_tx: Sender<Result<GetRawContextResponse, ToolError>>,
    },
    RunRhaiRule {
        agent_id: String,
        request: RunRhaiRuleRequest,
        reply_tx: Sender<Result<RunRhaiRuleResponse, ToolError>>,
    },
    ClassifyIngested {
        agent_id: String,
        request: ClassifyIngestedRequest,
        reply_tx: Sender<Result<ClassifyIngestedResponse, ToolError>>,
    },
    QueryFlags {
        agent_id: String,
        request: QueryFlagsRequest,
        reply_tx: Sender<Result<QueryFlagsResponse, ToolError>>,
    },
    ClassifyTransaction {
        agent_id: String,
        request: ClassifyTransactionRequest,
        reply_tx: Sender<Result<ClassifyTransactionResponse, ToolError>>,
    },
    ReconcileExcelClassification {
        agent_id: String,
        request: crate::ReconcileExcelClassificationRequest,
        reply_tx: Sender<Result<ClassifyTransactionResponse, ToolError>>,
    },
    QueryAuditLog {
        agent_id: String,
        request: QueryAuditLogRequest,
        reply_tx: Sender<Result<QueryAuditLogResponse, ToolError>>,
    },
    ExportCpaWorkbook {
        agent_id: String,
        request: ExportCpaWorkbookRequest,
        reply_tx: Sender<Result<ExportCpaWorkbookResponse, ToolError>>,
    },
    GetScheduleSummary {
        agent_id: String,
        request: GetScheduleSummaryRequest,
        reply_tx: Sender<Result<GetScheduleSummaryResponse, ToolError>>,
    },
    HsmTransition {
        agent_id: String,
        request: HsmTransitionRequest,
        reply_tx: Sender<Result<HsmTransitionResponse, ToolError>>,
    },
    HsmStatus {
        agent_id: String,
        request: HsmStatusRequest,
        reply_tx: Sender<Result<HsmStatusResponse, ToolError>>,
    },
    HsmResume {
        agent_id: String,
        request: HsmResumeRequest,
        reply_tx: Sender<Result<HsmResumeResponse, ToolError>>,
    },
    EventHistory {
        agent_id: String,
        filter: EventHistoryFilter,
        reply_tx: Sender<Result<EventHistoryResponse, ToolError>>,
    },
    ReplayLifecycle {
        agent_id: String,
        request: ReplayLifecycleRequest,
        reply_tx: Sender<Result<ReplayLifecycleResponse, ToolError>>,
    },
    TaxAssist {
        agent_id: String,
        request: TaxAssistRequest,
        reply_tx: Sender<Result<TaxAssistResponse, ToolError>>,
    },
    TaxEvidenceChain {
        agent_id: String,
        request: TaxEvidenceChainRequest,
        reply_tx: Sender<Result<TaxEvidenceChainResponse, ToolError>>,
    },
    TaxAmbiguityReview {
        agent_id: String,
        request: TaxAmbiguityReviewRequest,
        reply_tx: Sender<Result<TaxAmbiguityReviewResponse, ToolError>>,
    },
    ValidateReconciliationStage {
        agent_id: String,
        request: ReconciliationStageRequest,
        reply_tx: Sender<Result<ReconciliationStageResponse, ToolError>>,
    },
    ReconcileReconciliationStage {
        agent_id: String,
        request: ReconciliationStageRequest,
        reply_tx: Sender<Result<ReconciliationStageResponse, ToolError>>,
    },
    CommitReconciliationStage {
        agent_id: String,
        request: ReconciliationStageRequest,
        reply_tx: Sender<Result<ReconciliationStageResponse, ToolError>>,
    },
    AdjustTransaction {
        agent_id: String,
        request: ClassifyTransactionRequest,
        reply_tx: Sender<Result<ClassifyTransactionResponse, ToolError>>,
    },
    OntologyUpsertEntities {
        agent_id: String,
        request: OntologyUpsertEntitiesRequest,
        reply_tx: Sender<Result<OntologyUpsertEntitiesResponse, ToolError>>,
    },
    OntologyUpsertEdges {
        agent_id: String,
        request: OntologyUpsertEdgesRequest,
        reply_tx: Sender<Result<OntologyUpsertEdgesResponse, ToolError>>,
    },
    OntologyQueryPath {
        agent_id: String,
        request: OntologyQueryPathRequest,
        reply_tx: Sender<Result<OntologyQueryPathResponse, ToolError>>,
    },
    OntologyExportSnapshot {
        agent_id: String,
        request: OntologyExportSnapshotRequest,
        reply_tx: Sender<Result<OntologyExportSnapshotResponse, ToolError>>,
    },
    IngestImage {
        agent_id: String,
        request: IngestImageRequest,
        reply_tx: Sender<Result<IngestImageResponse, ToolError>>,
    },
    ApplyTags {
        agent_id: String,
        request: crate::ApplyTagsRequest,
        reply_tx: Sender<Result<crate::ApplyTagsResponse, ToolError>>,
    },
    RemoveTags {
        agent_id: String,
        request: crate::ApplyTagsRequest,
        reply_tx: Sender<Result<crate::ApplyTagsResponse, ToolError>>,
    },
    ListTagged {
        agent_id: String,
        request: crate::ListTaggedRequest,
        reply_tx: Sender<Result<crate::ListTaggedResponse, ToolError>>,
    },
    SyncFsMetadata {
        agent_id: String,
        request: SyncFsMetadataRequest,
        reply_tx: Sender<Result<SyncFsMetadataResponse, ToolError>>,
    },
    NormalizeFilename {
        agent_id: String,
        request: NormalizeFilenameRequest,
        reply_tx: Sender<Result<NormalizeFilenameResponse, ToolError>>,
    },
    #[cfg(feature = "xero")]
    XeroGetAuthUrl {
        agent_id: String,
        reply_tx: Sender<Result<String, ToolError>>,
    },
    #[cfg(feature = "xero")]
    XeroExchangeCode {
        agent_id: String,
        code: String,
        state: String,
        reply_tx: Sender<Result<serde_json::Value, ToolError>>,
    },
    #[cfg(feature = "xero")]
    XeroFetchContacts {
        agent_id: String,
        search: Option<String>,
        reply_tx: Sender<Result<serde_json::Value, ToolError>>,
    },
    #[cfg(feature = "xero")]
    XeroFetchAccounts {
        agent_id: String,
        reply_tx: Sender<Result<serde_json::Value, ToolError>>,
    },
    #[cfg(feature = "xero")]
    XeroFetchBankAccounts {
        agent_id: String,
        reply_tx: Sender<Result<serde_json::Value, ToolError>>,
    },
    #[cfg(feature = "xero")]
    XeroFetchInvoices {
        agent_id: String,
        status: Option<String>,
        reply_tx: Sender<Result<serde_json::Value, ToolError>>,
    },
    #[cfg(feature = "xero")]
    XeroLinkEntity {
        agent_id: String,
        local_id: String,
        xero_entity_type: String,
        xero_id: String,
        display_name: String,
        ontology_path: Option<PathBuf>,
        reply_tx: Sender<Result<serde_json::Value, ToolError>>,
    },
    #[cfg(feature = "xero")]
    XeroSyncCatalog {
        agent_id: String,
        ontology_path: PathBuf,
        reply_tx: Sender<Result<serde_json::Value, ToolError>>,
    },
    #[cfg(feature = "b00t")]
    BootDatumDelegate {
        agent_id: String,
        datum_id: String,
        task_id: String,
        /// Caller's estimate of cost — ledgrrr validates against budget.
        estimated_cost_usd: rust_decimal::Decimal,
        reply_tx: crossbeam::channel::Sender<Result<DelegateAuthority, ToolError>>,
    },
    Shutdown,
}
