use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Mutex;
use blake3;

use ledger_core::classify::{ClassificationEngine, FlagStatus, SampleTransaction};
use ledger_core::document::{DocType, DocumentRecord, DocumentStatus, XeroLink};
use ledger_core::filename::{FilenameError, StatementFilename};
use ledger_core::fs_meta::{FsMetadata, MetadataBackend, SidecarBackend};
use ledger_core::ingest::{deterministic_tx_id, IngestedLedger, TransactionInput};
use ledger_core::manifest::Manifest;
use ledger_core::tags::{parse_tags, Tag};
use ledger_core::workbook::REQUIRED_SHEETS;
use rust_decimal::Decimal;
use rust_xlsxwriter::Workbook;
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

#[cfg(feature = "llm")]
use ledgerr_llm::{LlmClient, LlmConfig};
#[cfg(feature = "xero")]
use xero_service::XeroService;

pub mod actor;
pub mod batch_executor;
use crate::batch_executor::BatchExecutor;
pub mod calendar_tool;
pub mod contract;
pub mod events;
pub mod focus_tool;
pub mod gate;
pub mod hsm;
pub mod mcp_adapter;
pub mod ontology;
pub mod plugin_info;
#[cfg(feature = "b00t")]
pub mod provider;
#[cfg(feature = "b00t")]
pub mod providers;
pub mod reconciliation;
pub mod shape_tool;
pub mod tax_assist;
pub mod xero_service;
pub use calendar_tool::{list_calendar_events, CalendarEventRow, ListCalendarEventsRequest};
pub use events::{
    AppendEventResult, EventHistoryFilter, EventHistoryResponse, InMemoryLifecycleEventStore,
    LifecycleEvent, LifecycleEventStore, ReplayProjection,
};
pub use hsm::{
    HsmMachine, HsmResumeRequest, HsmResumeResponse, HsmStatusRequest, HsmStatusResponse,
    HsmTransitionRequest, HsmTransitionResponse,
};
pub use ontology::{
    OntologyEdge, OntologyEdgeInput, OntologyEntity, OntologyEntityInput, OntologyEntityKind,
    OntologyQueryPathRequest, OntologyQueryPathResponse, OntologyStore, OntologyUpsertEdgesRequest,
    OntologyUpsertEdgesResponse, OntologyUpsertEntitiesRequest, OntologyUpsertEntitiesResponse,
};
pub use reconciliation::{
    commit_stage, reconcile_stage, validate_stage, ReconciliationDiagnostic,
    ReconciliationStageRequest, ReconciliationStageResponse,
};
pub use shape_tool::{get_document_shape, GetDocumentShapeRequest};
pub use tax_assist::{
    TaxAmbiguityRecord, TaxAmbiguityReviewRequest, TaxAmbiguityReviewResponse, TaxAssistRequest,
    TaxAssistResponse, TaxAssistSummary, TaxEvidenceChainRequest, TaxEvidenceChainResponse,
    TaxEvidenceCurrentState, TaxEvidenceEvent, TaxEvidenceRow, TaxEvidenceSource,
};
pub use contract::{
    TransactionFilters, DateRange, AmountRange, SortDirection, SortField, SortSpec,
    PaginationSpec, TransactionRow as TransactionRowResponse,
    BatchClassifyRequest, BatchClassifyResponse,
    BatchResolveFlagsRequest, BulkResolveFlagsResponse,
    ApplyMappingBulkRequest, ApplyMappingBulkResponse,
    BatchMode, FlagResolution, SimilarityMatchType,
    BatchSummary, BatchItemResult, BatchItemStatus,
    FetchQueueRequest, FetchQueueResponse,
    QueueItem, QueueItemType, QueueSeverity, QueueStatus, QueueProvenance,
};
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountSummary {
    pub account_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListAccountsRequest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListAccountsResponse {
    pub accounts: Vec<AccountSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestStatementRowsRequest {
    pub journal_path: PathBuf,
    pub workbook_path: PathBuf,
    pub ontology_path: Option<PathBuf>,
    pub rows: Vec<TransactionInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestStatementRowsResponse {
    pub inserted_count: usize,
    pub tx_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestPdfRequest {
    pub pdf_path: String,
    pub journal_path: PathBuf,
    pub workbook_path: PathBuf,
    pub ontology_path: Option<PathBuf>,
    pub raw_context_bytes: Option<Vec<u8>>,
    pub extracted_rows: Vec<TransactionInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestPdfResponse {
    pub inserted_count: usize,
    pub tx_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetRawContextRequest {
    pub rkyv_ref: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetRawContextResponse {
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DocumentQueueStatusRequest {
    InvalidName,
    Ready,
    Ingested,
}

impl DocumentQueueStatusRequest {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidName => "invalid_name",
            Self::Ready => "ready",
            Self::Ingested => "ingested",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "invalid_name" => Some(Self::InvalidName),
            "ready" => Some(Self::Ready),
            "ingested" => Some(Self::Ingested),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentInventoryRequest {
    pub directory: PathBuf,
    pub recursive: bool,
    pub statuses: Vec<DocumentQueueStatusRequest>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentRecordResponse {
    pub file_name: String,
    pub document_path: String,
    pub raw_context_ref: String,
    pub status: DocumentQueueStatusRequest,
    pub blocked_reason: Option<String>,
    pub next_hint: String,
    pub vendor: Option<String>,
    pub account_id: Option<String>,
    pub year_month: Option<String>,
    pub document_type: Option<String>,
    pub ingested_tx_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentInventoryResponse {
    pub documents: Vec<DocumentRecordResponse>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SampleTxRequest {
    pub tx_id: String,
    pub account_id: String,
    pub date: String,
    pub amount: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRhaiRuleRequest {
    pub rule_file: PathBuf,
    pub sample_tx: SampleTxRequest,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunRhaiRuleResponse {
    pub category: String,
    pub confidence: f64,
    pub review: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassifyIngestedRequest {
    pub rule_file: PathBuf,
    pub review_threshold: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassifiedTxResponse {
    pub tx_id: String,
    pub category: String,
    pub confidence: f64,
    pub needs_review: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassifyIngestedResponse {
    pub classifications: Vec<ClassifiedTxResponse>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlagStatusRequest {
    Open,
    Resolved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryFlagsRequest {
    pub year: i32,
    pub status: FlagStatusRequest,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FlagRecordResponse {
    pub tx_id: String,
    pub year: i32,
    pub status: FlagStatusRequest,
    pub reason: String,
    pub category: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QueryFlagsResponse {
    pub flags: Vec<FlagRecordResponse>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryTransactionsRequest {
    pub filters: TransactionFilters,
    pub sort: Option<SortSpec>,
    pub pagination: Option<PaginationSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryTransactionsResponse {
    pub transactions: Vec<TransactionRowResponse>,
    pub total_count: usize,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifyTransactionRequest {
    pub tx_id: String,
    pub category: String,
    pub confidence: String,
    pub note: Option<String>,
    pub actor: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconcileExcelClassificationRequest {
    pub tx_id: String,
    pub category: String,
    pub confidence: String,
    pub actor: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryAuditLogRequest;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AuditEntryResponse {
    pub timestamp: String,
    pub actor: String,
    pub tx_id: String,
    pub field: String,
    pub old_value: Option<String>,
    pub new_value: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifyTransactionResponse {
    pub tx_id: String,
    pub category: String,
    pub confidence: String,
    pub audit_entries: Vec<AuditEntryResponse>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReplayLifecycleRequest {
    pub tx_id: Option<String>,
    pub document_ref: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReplayLifecycleResponse {
    pub reconstructed_state: String,
    pub event_count: usize,
    pub diagnostics: Vec<String>,
    pub filter: EventHistoryFilter,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryAuditLogResponse {
    pub entries: Vec<AuditEntryResponse>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportCpaWorkbookRequest {
    pub workbook_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportCpaWorkbookResponse {
    pub sheets_written: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OntologyExportSnapshotRequest {
    pub ontology_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct OntologyExportSnapshotResponse {
    pub entities: Vec<OntologyEntity>,
    pub edges: Vec<OntologyEdge>,
    pub entity_count: usize,
    pub edge_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleKindRequest {
    ScheduleC,
    ScheduleD,
    ScheduleE,
    Fbar,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetScheduleSummaryRequest {
    pub year: i32,
    pub schedule: ScheduleKindRequest,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleLineResponse {
    pub key: String,
    pub total: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GetScheduleSummaryResponse {
    pub year: i32,
    pub schedule: ScheduleKindRequest,
    pub total: f64,
    pub lines: Vec<ScheduleLineResponse>,
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("internal error: {0}")]
    Internal(String),
    #[error("policy denied: {0}")]
    PolicyDenied(String),
    #[error("rate limited: retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },
}

impl From<FilenameError> for ToolError {
    fn from(value: FilenameError) -> Self {
        Self::InvalidInput(value.to_string())
    }
}

pub trait TurboLedgerTools {
    fn list_accounts(&self) -> Result<Vec<AccountSummary>, ToolError>;
    fn document_inventory(
        &self,
        request: DocumentInventoryRequest,
    ) -> Result<DocumentInventoryResponse, ToolError>;
    fn validate_source_filename(&self, file_name: &str) -> Result<StatementFilename, ToolError>;
    fn ingest_statement_rows(
        &self,
        request: IngestStatementRowsRequest,
    ) -> Result<IngestStatementRowsResponse, ToolError>;
    fn ingest_pdf(&self, request: IngestPdfRequest) -> Result<IngestPdfResponse, ToolError>;
    fn get_raw_context(
        &self,
        request: GetRawContextRequest,
    ) -> Result<GetRawContextResponse, ToolError>;
    fn run_rhai_rule(&self, request: RunRhaiRuleRequest) -> Result<RunRhaiRuleResponse, ToolError>;
    fn classify_ingested(
        &self,
        request: ClassifyIngestedRequest,
    ) -> Result<ClassifyIngestedResponse, ToolError>;
    fn query_flags(&self, request: QueryFlagsRequest) -> Result<QueryFlagsResponse, ToolError>;
    fn query_transactions(&self, request: QueryTransactionsRequest) -> Result<QueryTransactionsResponse, ToolError>;
    fn classify_transaction(
        &self,
        request: ClassifyTransactionRequest,
    ) -> Result<ClassifyTransactionResponse, ToolError>;
    fn reconcile_excel_classification(
        &self,
        request: ReconcileExcelClassificationRequest,
    ) -> Result<ClassifyTransactionResponse, ToolError>;
    fn query_audit_log(
        &self,
        request: QueryAuditLogRequest,
    ) -> Result<QueryAuditLogResponse, ToolError>;
    fn export_cpa_workbook(
        &self,
        request: ExportCpaWorkbookRequest,
    ) -> Result<ExportCpaWorkbookResponse, ToolError>;
    fn get_schedule_summary(
        &self,
        request: GetScheduleSummaryRequest,
    ) -> Result<GetScheduleSummaryResponse, ToolError>;
    fn batch_classify(
        &self,
        request: BatchClassifyRequest,
    ) -> Result<BatchClassifyResponse, ToolError>;
    fn bulk_resolve_flags(
        &self,
        request: BatchResolveFlagsRequest,
    ) -> Result<BulkResolveFlagsResponse, ToolError>;
    fn apply_mapping_bulk(
        &self,
        request: ApplyMappingBulkRequest,
    ) -> Result<ApplyMappingBulkResponse, ToolError>;
    fn fetch_work_queue(
        &self,
        request: FetchQueueRequest,
    ) -> Result<FetchQueueResponse, ToolError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredClassification {
    category: String,
    confidence: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuditEntry {
    timestamp: String,
    actor: String,
    tx_id: String,
    field: String,
    old_value: Option<String>,
    new_value: String,
    note: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ClassificationState {
    tx_rows: BTreeMap<String, TransactionInput>,
    classifications: BTreeMap<String, StoredClassification>,
    audit_log: Vec<AuditEntry>,
    engine: ClassificationEngine,
}

const PERSISTED_STATE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedServiceState {
    version: u32,
    ingest_state: IngestedLedger,
    classification_state: ClassificationState,
    lifecycle_events: InMemoryLifecycleEventStore,
    hsm_state: HsmMachine,
}

impl Default for PersistedServiceState {
    fn default() -> Self {
        Self {
            version: PERSISTED_STATE_VERSION,
            ingest_state: IngestedLedger::default(),
            classification_state: ClassificationState::default(),
            lifecycle_events: InMemoryLifecycleEventStore::default(),
            hsm_state: HsmMachine::default(),
        }
    }
}

pub struct TurboLedgerService {
    manifest: Manifest,
    ingest_state: Mutex<IngestedLedger>,
    classification_state: Mutex<ClassificationState>,
    lifecycle_events: Mutex<InMemoryLifecycleEventStore>,
    hsm_state: Mutex<HsmMachine>,
    /// In-memory registry: doc_id → DocumentRecord. Persisted as a JSON sidecar.
    document_registry: Mutex<BTreeMap<String, DocumentRecord>>,
    /// Evidence graph for provenance tracking (arc-kit-au).
    pub(crate) evidence: Mutex<arc_kit_au::EvidenceGraph>,
    #[cfg(feature = "xero")]
    xero: XeroService,
    #[cfg(feature = "llm")]
    llm: Option<LlmClient>,
}
/// Apply transaction filters to a set of transactions joined with classifications
fn apply_transaction_filters<'a>(
    tx_rows: &'a BTreeMap<String, TransactionInput>,
    classifications: &'a BTreeMap<String, StoredClassification>,
    filters: &TransactionFilters,
) -> Result<Vec<(String, &'a TransactionInput, Option<&'a StoredClassification>)>, ToolError> {
    let mut results: Vec<_> = tx_rows.iter()
        .filter_map(|(tx_id, tx)| {
            // Join with classification data
            let classification = classifications.get(tx_id);
            Some((tx_id.clone(), tx, classification))
        })
        .collect();

    // Apply filters
    if let Some(ref account_id) = filters.account_id {
        results.retain(|(_, tx, _)| tx.account_id == *account_id);
    }

    if let Some(ref date_range) = filters.date_range {
        results.retain(|(_, tx, _)| {
            tx.date >= date_range.start && tx.date <= date_range.end
        });
    }

    if let Some(ref category) = filters.category {
        results.retain(|(_, _, classification)| {
            classification.map_or(false, |c| c.category.to_lowercase() == category.to_lowercase())
        });
    }

    if let Some(ref amount_range) = filters.amount_range {
        let min = Decimal::from_str(&amount_range.min)
            .map_err(|e| ToolError::InvalidInput(format!("invalid amount_range.min: {}", e)))?;
        let max = Decimal::from_str(&amount_range.max)
            .map_err(|e| ToolError::InvalidInput(format!("invalid amount_range.max: {}", e)))?;
        results.retain(|(_, tx, _)| {
            let amount = Decimal::from_str(&tx.amount).unwrap_or(Decimal::ZERO);
            amount >= min && amount <= max
        });
    }

    if let Some(ref source_ref) = filters.source_ref {
        results.retain(|(_, tx, _)| {
            tx.source_ref.to_lowercase().contains(&source_ref.to_lowercase())
        });
    }

    if let Some(ref desc) = filters.description_contains {
        results.retain(|(_, tx, _)| {
            tx.description.to_lowercase().contains(&desc.to_lowercase())
        });
    }

    Ok(results)
}

/// Apply sorting to a set of transactions
fn apply_transaction_sort<'a>(
    mut transactions: Vec<(String, &'a TransactionInput, Option<&'a StoredClassification>)>,
    sort_spec: &Option<SortSpec>,
) -> Vec<(String, &'a TransactionInput, Option<&'a StoredClassification>)> {
    let sort_spec = sort_spec.as_ref().map_or(
        &SortSpec { 
            field: SortField::Date, 
            direction: SortDirection::Desc 
        },
        |s| s
    );
    
    transactions.sort_by(|a, b| match sort_spec.field {
        SortField::Date => {
            let ord = a.1.date.cmp(&b.1.date);
            if sort_spec.direction == SortDirection::Desc { ord.reverse() } else { ord }
        }
        SortField::Amount => {
            let a_amt = Decimal::from_str(&a.1.amount).unwrap_or(Decimal::ZERO);
            let b_amt = Decimal::from_str(&b.1.amount).unwrap_or(Decimal::ZERO);
            let ord = a_amt.cmp(&b_amt);
            if sort_spec.direction == SortDirection::Desc { ord.reverse() } else { ord }
        }
        SortField::Description => {
            let ord = a.1.description.cmp(&b.1.description);
            if sort_spec.direction == SortDirection::Desc { ord.reverse() } else { ord }
        }
    });
    
    transactions
}


impl TurboLedgerService {
    pub fn from_manifest_str(src: &str) -> Result<Self, ToolError> {
        let manifest = Manifest::parse(src).map_err(|e| ToolError::InvalidInput(e.to_string()))?;
        let persisted =
            load_persisted_state(std::path::Path::new(&manifest.session.workbook_path))?;

        // Load document registry from sidecar if present.
        let registry =
            load_document_registry(std::path::Path::new(&manifest.session.workbook_path));

        #[cfg(feature = "xero")]
        let xero = {
            use ledgerr_xero::XeroConfig;
            let token_path = std::path::Path::new(&manifest.session.workbook_path)
                .with_extension("xero-tokens.json");
            let config = XeroConfig {
                client_id: std::env::var("XERO_CLIENT_ID").unwrap_or_default(),
                client_secret: std::env::var("XERO_CLIENT_SECRET").unwrap_or_default(),
                redirect_port: std::env::var("XERO_REDIRECT_PORT")
                    .ok()
                    .and_then(|p| p.parse().ok())
                    .unwrap_or(8080),
                scopes: Vec::new(),
            };
            XeroService::new(config, token_path)
        };

        #[cfg(feature = "llm")]
        let llm = LlmClient::new(LlmConfig::from_env()).ok();

        let initial_evidence_path = {
            let service_path = std::path::Path::new(&manifest.session.workbook_path);
            persisted_state_path(service_path).with_extension("evidence.json")
        };

        Ok(Self {
            manifest,
            ingest_state: Mutex::new(persisted.ingest_state),
            classification_state: Mutex::new(persisted.classification_state),
            lifecycle_events: Mutex::new(persisted.lifecycle_events),
            hsm_state: Mutex::new(persisted.hsm_state),
            document_registry: Mutex::new(registry),
            evidence: Mutex::new(
                arc_kit_au::EvidenceStore::new(initial_evidence_path)
                    .load()
                    .unwrap_or_else(|_| arc_kit_au::EvidenceGraph::new()),
            ),
            #[cfg(feature = "xero")]
            xero,
            #[cfg(feature = "llm")]
            llm,
        })
    }

    /// Spawn the service behind a channel actor, returning a handle.
    pub fn spawn_actor(self) -> crate::actor::ServiceHandle {
        crate::actor::spawn_actor(self)
    }

    pub fn workbook_path(&self) -> &std::path::Path {
        std::path::Path::new(&self.manifest.session.workbook_path)
    }

    fn state_sidecar_path(&self) -> PathBuf {
        persisted_state_path(self.workbook_path())
    }

    /// Canonical evidence graph path, always derived from the workbook path.
    /// Centralizes path computation so callers do not recompute divergently.
    fn evidence_path(&self) -> PathBuf {
        self.state_sidecar_path().with_extension("evidence.json")
    }

    fn snapshot_persisted_state(&self) -> Result<PersistedServiceState, ToolError> {
        // Persist all restart-visible state as one snapshot so idempotency, audit,
        // lifecycle replay, and HSM resume move together. Partial silent resets would
        // be worse than a hard failure in this accountant-facing workflow.
        let ingest_state = self
            .ingest_state
            .lock()
            .map_err(|_| ToolError::Internal("ingest lock poisoned".to_string()))?
            .clone();
        let classification_state = self
            .classification_state
            .lock()
            .map_err(|_| ToolError::Internal("classification lock poisoned".to_string()))?
            .clone();
        let lifecycle_events = self
            .lifecycle_events
            .lock()
            .map_err(|_| ToolError::Internal("events lock poisoned".to_string()))?
            .clone();
        let hsm_state = self
            .hsm_state
            .lock()
            .map_err(|_| ToolError::Internal("hsm lock poisoned".to_string()))?
            .clone();
        Ok(PersistedServiceState {
            version: PERSISTED_STATE_VERSION,
            ingest_state,
            classification_state,
            lifecycle_events,
            hsm_state,
        })
    }

    fn persist_state(&self) -> Result<(), ToolError> {
        let snapshot = self.snapshot_persisted_state()?;
        persist_state_to_path(&self.state_sidecar_path(), &snapshot)?;
        // Persist evidence graph alongside the main state.
        if let Ok(evidence) = self.evidence.lock() {
            if let Err(e) = arc_kit_au::EvidenceStore::new(self.evidence_path()).save(&evidence) {
                tracing::warn!(error = %e, "evidence graph persistence failed");
            }
        }
        Ok(())
    }

    pub fn list_accounts_tool(
        &self,
        _request: ListAccountsRequest,
    ) -> Result<ListAccountsResponse, ToolError> {
        Ok(ListAccountsResponse {
            accounts: self.list_accounts()?,
        })
    }

    pub fn document_inventory_tool(
        &self,
        request: DocumentInventoryRequest,
    ) -> Result<DocumentInventoryResponse, ToolError> {
        self.document_inventory(request)
    }

    pub fn ontology_upsert_entities(
        &self,
        request: OntologyUpsertEntitiesRequest,
    ) -> Result<OntologyUpsertEntitiesResponse, ToolError> {
        let mut store = OntologyStore::load(&request.ontology_path)?;
        let response = store.upsert_entities(request.entities)?;
        store.persist(&request.ontology_path)?;
        Ok(response)
    }

    pub fn ontology_upsert_entities_tool(
        &self,
        request: OntologyUpsertEntitiesRequest,
    ) -> Result<OntologyUpsertEntitiesResponse, ToolError> {
        self.ontology_upsert_entities(request)
    }

    pub fn ontology_upsert_edges(
        &self,
        request: OntologyUpsertEdgesRequest,
    ) -> Result<OntologyUpsertEdgesResponse, ToolError> {
        let mut store = OntologyStore::load(&request.ontology_path)?;
        let response = store.upsert_edges(request.edges)?;
        store.persist(&request.ontology_path)?;
        Ok(response)
    }

    pub fn ontology_upsert_edges_tool(
        &self,
        request: OntologyUpsertEdgesRequest,
    ) -> Result<OntologyUpsertEdgesResponse, ToolError> {
        self.ontology_upsert_edges(request)
    }

    pub fn ontology_query_path(
        &self,
        request: OntologyQueryPathRequest,
    ) -> Result<OntologyQueryPathResponse, ToolError> {
        let store = OntologyStore::load(&request.ontology_path)?;
        store.query_path(&request.from_entity_id, request.max_depth)
    }

    pub fn ontology_query_path_tool(
        &self,
        request: OntologyQueryPathRequest,
    ) -> Result<OntologyQueryPathResponse, ToolError> {
        self.ontology_query_path(request)
    }

    pub fn ontology_export_snapshot(
        &self,
        request: OntologyExportSnapshotRequest,
    ) -> Result<OntologyExportSnapshotResponse, ToolError> {
        let store = OntologyStore::load(&request.ontology_path)?;
        Ok(OntologyExportSnapshotResponse {
            entity_count: store.entities.len(),
            edge_count: store.edges.len(),
            entities: store.entities,
            edges: store.edges,
        })
    }

    pub fn validate_reconciliation_stage_tool(
        &self,
        request: ReconciliationStageRequest,
    ) -> Result<ReconciliationStageResponse, ToolError> {
        validate_stage(&request)
    }

    pub fn reconcile_reconciliation_stage_tool(
        &self,
        request: ReconciliationStageRequest,
    ) -> Result<ReconciliationStageResponse, ToolError> {
        reconcile_stage(&request)
    }

    pub fn commit_reconciliation_stage_tool(
        &self,
        request: ReconciliationStageRequest,
    ) -> Result<ReconciliationStageResponse, ToolError> {
        commit_stage(&request)
    }

    pub fn hsm_transition_tool(
        &self,
        request: HsmTransitionRequest,
    ) -> Result<HsmTransitionResponse, ToolError> {
        let requested = hsm::parse_node(&request.target_state, &request.target_substate)
            .ok_or_else(|| {
                ToolError::InvalidInput(
                    "target_state/target_substate must match lifecycle vocabulary".to_string(),
                )
            })?;

        let mut hsm = self
            .hsm_state
            .lock()
            .map_err(|_| ToolError::Internal("hsm lock poisoned".to_string()))?;
        let current = hsm.current;
        if hsm::allowed_next_node(current) == Some(requested) {
            hsm.current = requested;
            hsm.last_valid_checkpoint = hsm::checkpoint_marker(requested);
            let response = hsm::transition_advanced_response(requested);
            drop(hsm);
            self.persist_state()?;
            return Ok(response);
        }

        Ok(hsm::transition_blocked_response(current, requested))
    }

    pub fn hsm_status_tool(
        &self,
        _request: HsmStatusRequest,
    ) -> Result<HsmStatusResponse, ToolError> {
        let hsm = self
            .hsm_state
            .lock()
            .map_err(|_| ToolError::Internal("hsm lock poisoned".to_string()))?;
        Ok(hsm::status_response(hsm.current, Vec::new()))
    }

    pub fn hsm_resume_tool(
        &self,
        request: HsmResumeRequest,
    ) -> Result<HsmResumeResponse, ToolError> {
        let mut hsm = self
            .hsm_state
            .lock()
            .map_err(|_| ToolError::Internal("hsm lock poisoned".to_string()))?;
        let Some(resume_node) = hsm::parse_checkpoint_marker(&request.state_marker) else {
            return Ok(hsm::resume_response(
                hsm.current,
                false,
                vec!["checkpoint_invalid".to_string()],
            ));
        };

        if request.state_marker != hsm.last_valid_checkpoint {
            return Ok(hsm::resume_response(
                hsm.current,
                false,
                vec!["checkpoint_unknown".to_string()],
            ));
        }

        hsm.current = resume_node;
        let response = hsm::resume_response(hsm.current, true, Vec::new());
        drop(hsm);
        self.persist_state()?;
        Ok(response)
    }

    pub fn adjust_transaction(
        &self,
        request: ClassifyTransactionRequest,
    ) -> Result<ClassifyTransactionResponse, ToolError> {
        self.apply_classification_action(request, "adjustment")
    }

    pub fn event_history(
        &self,
        filter: EventHistoryFilter,
    ) -> Result<EventHistoryResponse, ToolError> {
        self.lifecycle_events
            .lock()
            .map_err(|_| ToolError::Internal("events lock poisoned".to_string()))?
            .list_events(filter)
    }

    pub fn replay_lifecycle(
        &self,
        request: ReplayLifecycleRequest,
    ) -> Result<ReplayLifecycleResponse, ToolError> {
        let filter = EventHistoryFilter {
            tx_id: request.tx_id,
            document_ref: request.document_ref,
            time_start: None,
            time_end: None,
        };
        let history = self.event_history(filter.clone())?;
        let projection = events::reconstruct_lifecycle(&history.events);
        Ok(ReplayLifecycleResponse {
            reconstructed_state: projection.reconstructed_state,
            event_count: projection.event_count,
            diagnostics: projection.diagnostics,
            filter,
        })
    }

    pub fn tax_assist_tool(
        &self,
        request: TaxAssistRequest,
    ) -> Result<TaxAssistResponse, ToolError> {
        let stage = self.reconcile_reconciliation_stage_tool(request.reconciliation)?;
        let path = if stage.status == "passed" {
            let ontology_path = request.ontology_path.clone();
            let mut path = self.ontology_query_path_tool(OntologyQueryPathRequest {
                ontology_path: ontology_path.clone(),
                from_entity_id: request.from_entity_id.clone(),
                max_depth: request.max_depth,
            })?;
            let store = OntologyStore::load(&ontology_path)?;
            let entity_lookup = store
                .entities
                .iter()
                .map(|node| (node.id.clone(), node.clone()))
                .collect::<BTreeMap<_, _>>();
            let mut existing_edge_ids = path
                .edges
                .iter()
                .map(|edge| edge.id.clone())
                .collect::<BTreeSet<_>>();
            let mut existing_node_ids = path
                .nodes
                .iter()
                .map(|node| node.id.clone())
                .collect::<BTreeSet<_>>();
            for edge in store
                .edges
                .into_iter()
                .filter(|edge| edge.from == request.from_entity_id)
            {
                if existing_edge_ids.insert(edge.id.clone()) {
                    if !existing_node_ids.contains(&edge.to) {
                        if let Some(node) = entity_lookup.get(&edge.to) {
                            path.nodes.push(node.clone());
                            existing_node_ids.insert(node.id.clone());
                        }
                    }
                    path.edges.push(edge);
                }
            }
            Some(path)
        } else {
            None
        };
        Ok(tax_assist::build_tax_assist_response(
            &request.from_entity_id,
            stage,
            path,
        ))
    }

    pub fn tax_evidence_chain_tool(
        &self,
        request: TaxEvidenceChainRequest,
    ) -> Result<TaxEvidenceChainResponse, ToolError> {
        let normalized_tx_id = request
            .tx_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let normalized_document_ref = request
            .document_ref
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        let path = self.ontology_query_path_tool(OntologyQueryPathRequest {
            ontology_path: request.ontology_path,
            from_entity_id: request.from_entity_id.clone(),
            max_depth: None,
        })?;
        let history_filter = EventHistoryFilter {
            tx_id: normalized_tx_id.clone(),
            document_ref: normalized_document_ref.clone(),
            time_start: None,
            time_end: None,
        };
        let events = self.event_history(history_filter.clone())?;
        let replay = self.replay_lifecycle(ReplayLifecycleRequest {
            tx_id: history_filter.tx_id,
            document_ref: history_filter.document_ref,
        })?;

        let mut ambiguity = path
            .edges
            .iter()
            .filter(|edge| edge.relation == "ambiguity")
            .map(|edge| TaxAmbiguityRecord {
                tx_id: normalized_tx_id.clone().or_else(|| Some(edge.from.clone())),
                review_state: "needs_review".to_string(),
                reason: "ambiguous_tax_treatment".to_string(),
                provenance_refs: edge
                    .provenance
                    .iter()
                    .filter_map(|(key, value)| {
                        if key.contains("source") || key.contains("ref") {
                            Some(value.clone())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>(),
            })
            .collect::<Vec<_>>();
        ambiguity.sort_by(|a, b| {
            a.tx_id
                .cmp(&b.tx_id)
                .then_with(|| a.review_state.cmp(&b.review_state))
                .then_with(|| a.reason.cmp(&b.reason))
        });
        let mut provenance_refs = path
            .edges
            .iter()
            .flat_map(|edge| edge.provenance.iter())
            .filter_map(|(key, value)| {
                if key.contains("source") || key.contains("ref") {
                    Some(value.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        provenance_refs.sort();
        provenance_refs.dedup();
        let mut node_ids = path
            .nodes
            .into_iter()
            .map(|node| node.id)
            .collect::<Vec<_>>();
        node_ids.sort();
        let mut edge_ids = path
            .edges
            .into_iter()
            .map(|edge| edge.id)
            .collect::<Vec<_>>();
        edge_ids.sort();
        let source = TaxEvidenceSource {
            from_entity_id: request.from_entity_id,
            node_ids,
            edge_ids,
            provenance_refs,
        };
        Ok(tax_assist::build_tax_evidence_chain_response(
            source, events, replay, ambiguity,
        ))
    }

    pub fn tax_ambiguity_review_tool(
        &self,
        request: TaxAmbiguityReviewRequest,
    ) -> Result<TaxAmbiguityReviewResponse, ToolError> {
        let stage = self.reconcile_reconciliation_stage_tool(request.reconciliation)?;
        let path = if stage.status == "passed" {
            self.ontology_query_path_tool(OntologyQueryPathRequest {
                ontology_path: request.ontology_path,
                from_entity_id: request.from_entity_id,
                max_depth: request.max_depth,
            })?
        } else {
            OntologyQueryPathResponse {
                nodes: Vec::new(),
                edges: Vec::new(),
            }
        };
        let assist = tax_assist::build_tax_assist_response("", stage.clone(), Some(path));
        Ok(tax_assist::build_tax_ambiguity_review_response(
            stage,
            assist.ambiguity,
        ))
    }

    fn append_lifecycle_event(
        &self,
        event_type: &str,
        tx_id: Option<String>,
        document_ref: Option<String>,
        payload: BTreeMap<String, String>,
    ) -> Result<AppendEventResult, ToolError> {
        self.lifecycle_events
            .lock()
            .map_err(|_| ToolError::Internal("events lock poisoned".to_string()))?
            .append_event(event_type, tx_id, document_ref, payload)
    }

    fn apply_classification_action(
        &self,
        request: ClassifyTransactionRequest,
        event_type: &str,
    ) -> Result<ClassifyTransactionResponse, ToolError> {
        let (response, tx_row) = {
            let mut classification = self
                .classification_state
                .lock()
                .map_err(|_| ToolError::Internal("classification lock poisoned".to_string()))?;

            let tx_row = classification
                .tx_rows
                .get(&request.tx_id)
                .cloned()
                .ok_or_else(|| ToolError::InvalidInput("unknown tx_id".to_string()))?;

            validate_invariants(&tx_row, &request.tx_id, &request.category)?;
            let confidence = parse_confidence(&request.confidence)?;

            let old = classification.classifications.get(&request.tx_id).cloned();
            let mut new_entries = Vec::new();
            let timestamp = now_timestamp();

            if old.as_ref().map(|c| c.category.as_str()) != Some(request.category.as_str()) {
                let entry = AuditEntry {
                    timestamp: timestamp.clone(),
                    actor: request.actor.clone(),
                    tx_id: request.tx_id.clone(),
                    field: "category".to_string(),
                    old_value: old.as_ref().map(|c| c.category.clone()),
                    new_value: request.category.clone(),
                    note: request.note.clone(),
                };
                classification.audit_log.push(entry.clone());
                new_entries.push(to_audit_response(entry));
            }

            if old.as_ref().map(|c| c.confidence) != Some(confidence) {
                let entry = AuditEntry {
                    timestamp,
                    actor: request.actor.clone(),
                    tx_id: request.tx_id.clone(),
                    field: "confidence".to_string(),
                    old_value: old.as_ref().map(|c| c.confidence.to_string()),
                    new_value: confidence.to_string(),
                    note: request.note.clone(),
                };
                classification.audit_log.push(entry.clone());
                new_entries.push(to_audit_response(entry));
            }

            classification.classifications.insert(
                request.tx_id.clone(),
                StoredClassification {
                    category: request.category.clone(),
                    confidence,
                },
            );

            if confidence < Decimal::from_str("0.80").expect("valid decimal literal")
                || request.category.eq_ignore_ascii_case("uncategorized")
            {
                classification.engine.record_review_flag(
                    request.tx_id.clone(),
                    &tx_row.date,
                    "manual classification requires review".to_string(),
                    request.category.clone(),
                    confidence.to_string().parse::<f64>().unwrap_or(0.0),
                );

                // Emit ValidationIssue evidence for low-confidence classifications.
                if let Ok(mut evidence) = self.evidence.lock() {
                    let mut builder = arc_kit_au::EvidenceBuilder::new(&mut evidence);
                    let issue = arc_kit_au::node::ValidationIssue {
                        tx_id: request.tx_id.clone(),
                        rule: "confidence_threshold".to_string(),
                        severity: "recoverable".to_string(),
                        message: format!(
                            "confidence {} below 0.80 threshold for category '{}'",
                            confidence, request.category
                        ),
                        actor: request.actor.clone(),
                        raised_at: chrono::Utc::now(),
                        resolved: false,
                    };
                    builder.ensure_validation_issue(issue);
                }
            }

            (
                ClassifyTransactionResponse {
                    tx_id: request.tx_id.clone(),
                    category: request.category.clone(),
                    confidence: confidence.to_string(),
                    audit_entries: new_entries,
                },
                tx_row,
            )
        };

        let mut payload = BTreeMap::new();
        payload.insert("actor".to_string(), request.actor.clone());
        payload.insert("category".to_string(), request.category.clone());
        payload.insert("confidence".to_string(), request.confidence.clone());
        payload.insert("date".to_string(), tx_row.date.clone());
        payload.insert("note".to_string(), request.note.clone().unwrap_or_default());
        self.append_lifecycle_event(
            event_type,
            Some(request.tx_id.clone()),
            Some(tx_row.source_ref.clone()),
            payload,
        )?;
        self.persist_state()?;

        // Emit classification evidence via idempotent builder.
        {
            let mut evidence = self
                .evidence
                .lock()
                .map_err(|_| ToolError::Internal("evidence lock poisoned".to_string()))?;
            let mut builder = arc_kit_au::EvidenceBuilder::new(&mut evidence);
            let cls = arc_kit_au::node::Classification {
                tx_id: request.tx_id.clone(),
                category: request.category.clone(),
                sub_category: None,
                confidence: arc_kit_au::Confidence::from(
                    request.confidence.parse::<f64>().unwrap_or(0.0),
                ),
                rule_used: None,
                actor: request.actor.clone(),
                classified_at: chrono::Utc::now(),
                note: request.note.clone(),
            };
            builder.ensure_classification(cls);
        }

        Ok(response)
    }

    fn emit_ingest_evidence(&self, row: &TransactionInput, tx_id: &str) -> Result<(), ToolError> {
        use arc_kit_au::node::{ExtractedRow, SourceDoc, Transaction};
        use chrono::Utc;

        let mut evidence = self
            .evidence
            .lock()
            .map_err(|_| ToolError::Internal("evidence lock poisoned".to_string()))?;

        // Create source document evidence.
        let source_ref = &row.source_ref;
        let filename = std::path::Path::new(source_ref)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(source_ref);
        let doc = SourceDoc {
            filename: filename.to_string(),
            vendor: String::new(),
            account_id: row.account_id.clone(),
            statement_date: row.date.clone(),
            document_type: "statement".to_string(),
            content_hash: blake3::hash(source_ref.as_bytes()).to_hex().to_string(),
            ingested_at: Utc::now(),
            raw_context_path: Some(source_ref.clone()),
        };

        // Use the idempotent builder — fails gracefully with tracing::warn!.
        let mut builder = arc_kit_au::EvidenceBuilder::new(&mut evidence);
        let doc_id = doc.node_id();
        builder.ensure_document(doc);

        // Create extracted row evidence. Clone doc_id for row reference since NodeId is move.
        let doc_ref = doc_id.clone();
        let amount = rust_decimal::Decimal::from_str_exact(&row.amount).map_err(|_| {
            ToolError::InvalidInput(format!("bad amount in evidence emission: {}", row.amount))
        })?;
        let ext_row = ExtractedRow {
            account_id: row.account_id.clone(),
            date: row.date.clone(),
            amount,
            description: row.description.clone(),
            source_document: doc_ref,
            extraction_confidence: arc_kit_au::Confidence::from(1.0),
        };
        let row_ids = builder.ensure_extracted_rows(&doc_id, vec![ext_row]);

        // Create transaction evidence.
        let tx = Transaction {
            tx_id: tx_id.to_string(),
            account_id: row.account_id.clone(),
            date: row.date.clone(),
            amount: row.amount.clone(),
            description: row.description.clone(),
            source_rows: row_ids.clone(),
        };
        builder.ensure_transaction(tx, &row_ids);

        Ok(())
    }
}

// ============================================================================
// WORK QUEUE TRANSFORMATION FUNCTIONS
// ============================================================================

/// Convert a flag record to a queue item
fn flag_to_queue_item(flag: &FlagRecordResponse) -> QueueItem {
    let id = blake3::hash(format!("flag:{}:{}", flag.tx_id, flag.year).as_bytes()).to_hex();
    let severity = match flag.status {
        FlagStatusRequest::Open => QueueSeverity::High,
        FlagStatusRequest::Resolved => QueueSeverity::Low,
    };
    let created_at = "1970-01-01T00:00:00Z".to_string(); // No timestamp in FlagRecordResponse
    
    QueueItem {
        id: id.to_string(),
        item_type: QueueItemType::Flag,
        severity,
        created_at,
        status: match flag.status {
            FlagStatusRequest::Open => QueueStatus::Open,
            FlagStatusRequest::Resolved => QueueStatus::Resolved,
        },
        provenance: QueueProvenance::ReviewTool,
        related_tx_ids: vec![flag.tx_id.clone()],
        summary: flag.reason.clone(),
        tx_id: Some(flag.tx_id.clone()),
        document_ref: None,
        metadata: BTreeMap::new(),
    }
}


/// Convert a lifecycle event (manual change) to a queue item
fn manual_change_to_queue_item(event: &LifecycleEvent) -> QueueItem {
    let id = format!("manual:{}", event.event_id);
    
    QueueItem {
        id,
        item_type: QueueItemType::ManualChange,
        severity: QueueSeverity::Low,
        created_at: event.occurred_at.clone(),
        status: QueueStatus::Resolved,
        provenance: QueueProvenance::AuditTool,
        summary: format!("Manual change: {}", event.event_type),
        related_tx_ids: event.tx_id.clone().map(|id| vec![id]).unwrap_or_default(),
        tx_id: event.tx_id.clone(),
        document_ref: event.document_ref.clone(),
        metadata: BTreeMap::new(),
    }
}



impl TurboLedgerTools for TurboLedgerService {
    fn list_accounts(&self) -> Result<Vec<AccountSummary>, ToolError> {
        let out = self
            .manifest
            .list_account_ids()
            .into_iter()
            .map(|account_id| AccountSummary { account_id })
            .collect();
        Ok(out)
    }

    fn document_inventory(
        &self,
        request: DocumentInventoryRequest,
    ) -> Result<DocumentInventoryResponse, ToolError> {
        // Queue discovery is intentionally derived on demand from the filesystem plus
        // known ingested artifacts. That keeps the first cut deterministic and avoids
        // introducing claim/prioritization state before the queue semantics settle.
        let directory =
            resolve_document_inventory_directory(self.workbook_path(), &request.directory)?;
        let known_source_refs = self
            .classification_state
            .lock()
            .map_err(|_| ToolError::Internal("classification lock poisoned".to_string()))?
            .tx_rows
            .iter()
            .map(|(tx_id, row)| (tx_id.clone(), PathBuf::from(&row.source_ref)))
            .collect::<Vec<_>>();
        let mut documents = collect_document_paths(&directory, request.recursive)?
            .into_iter()
            .map(|path| build_document_record(self, &known_source_refs, path))
            .collect::<Result<Vec<_>, _>>()?;
        documents.retain(|document| {
            request.statuses.is_empty() || request.statuses.contains(&document.status)
        });
        documents.sort_by(|left, right| {
            document_status_rank(left.status)
                .cmp(&document_status_rank(right.status))
                .then_with(|| left.file_name.cmp(&right.file_name))
                .then_with(|| left.document_path.cmp(&right.document_path))
        });
        Ok(DocumentInventoryResponse { documents })
    }

    fn validate_source_filename(&self, file_name: &str) -> Result<StatementFilename, ToolError> {
        Ok(StatementFilename::parse(file_name)?)
    }

    fn ingest_statement_rows(
        &self,
        request: IngestStatementRowsRequest,
    ) -> Result<IngestStatementRowsResponse, ToolError> {
        let inserted = {
            let mut state = self
                .ingest_state
                .lock()
                .map_err(|_| ToolError::Internal("ingest lock poisoned".to_string()))?;
            state
                .ingest_to_journal_and_workbook(
                    &request.rows,
                    &request.journal_path,
                    &request.workbook_path,
                )
                .map_err(|e| ToolError::Internal(e.to_string()))?
        };

        let mut by_id = BTreeMap::<String, TransactionInput>::new();
        for row in &request.rows {
            by_id.insert(deterministic_tx_id(row), row.clone());
        }
        let mut classification = self
            .classification_state
            .lock()
            .map_err(|_| ToolError::Internal("classification lock poisoned".to_string()))?;
        for tx in &inserted {
            if let Some(row) = by_id.get(&tx.tx_id) {
                classification.tx_rows.insert(tx.tx_id.clone(), row.clone());
            }
        }
        drop(classification);

        if let Some(raw_ontology_path) = request.ontology_path.as_deref() {
            let allowed_base = request
                .workbook_path
                .parent()
                .ok_or_else(|| {
                    ToolError::InvalidInput(
                        "workbook_path must have a parent directory".to_string(),
                    )
                })?
                .to_path_buf();
            if raw_ontology_path
                .components()
                .any(|c| c == std::path::Component::ParentDir)
            {
                return Err(ToolError::InvalidInput(format!(
                    "ontology_path '{}' contains path traversal components",
                    raw_ontology_path.display()
                )));
            }
            let resolved_ontology_path = if raw_ontology_path.is_absolute() {
                if !raw_ontology_path.starts_with(&allowed_base) {
                    return Err(ToolError::InvalidInput(format!(
                        "ontology_path '{}' resolves outside the allowed directory",
                        raw_ontology_path.display()
                    )));
                }
                raw_ontology_path.to_path_buf()
            } else {
                allowed_base.join(raw_ontology_path)
            };
            emit_ingest_ontology_edges(&resolved_ontology_path, &request.rows)?;
        }

        for row in &request.rows {
            let tx_id = deterministic_tx_id(row);
            let mut payload = BTreeMap::new();
            payload.insert("account_id".to_string(), row.account_id.clone());
            payload.insert("amount".to_string(), row.amount.clone());
            payload.insert("date".to_string(), row.date.clone());
            payload.insert("description".to_string(), row.description.clone());
            payload.insert(
                "inserted".to_string(),
                inserted.iter().any(|tx| tx.tx_id == tx_id).to_string(),
            );
            self.append_lifecycle_event(
                "ingest",
                Some(tx_id),
                Some(row.source_ref.clone()),
                payload,
            )?;
        }
        self.persist_state()?;

        // Emit evidence nodes for ingested rows.
        for row in &request.rows {
            let tx_id = deterministic_tx_id(row);
            // Only emit for newly inserted transactions.
            if inserted.iter().any(|tx| tx.tx_id == tx_id) {
                if let Err(e) = self.emit_ingest_evidence(row, &tx_id) {
                    tracing::warn!(error = %e, tx_id, "evidence emission failed during ingest");
                }
            }
        }

        let tx_ids = inserted
            .iter()
            .map(|row| row.tx_id.clone())
            .collect::<Vec<_>>();
        Ok(IngestStatementRowsResponse {
            inserted_count: tx_ids.len(),
            tx_ids,
        })
    }

    fn ingest_pdf(&self, request: IngestPdfRequest) -> Result<IngestPdfResponse, ToolError> {
        let file_name = std::path::Path::new(&request.pdf_path)
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                ToolError::InvalidInput("pdf_path must have a valid filename".to_string())
            })?;
        let _parsed = self.validate_source_filename(file_name)?;

        // Derive the allowed base directory from the workbook path to prevent path traversal.
        let allowed_base = request
            .workbook_path
            .parent()
            .ok_or_else(|| {
                ToolError::InvalidInput("workbook_path must have a parent directory".to_string())
            })?
            .to_path_buf();

        for row in &request.extracted_rows {
            let source_path = std::path::Path::new(&row.source_ref);
            let resolved = if source_path.is_absolute() {
                // Absolute paths are allowed only if they reside within the allowed base directory.
                // Reject any `..` components that could escape the base via lexical traversal.
                if source_path
                    .components()
                    .any(|c| c == std::path::Component::ParentDir)
                {
                    return Err(ToolError::InvalidInput(format!(
                        "source_ref '{}' contains path traversal components",
                        row.source_ref
                    )));
                }
                if !source_path.starts_with(&allowed_base) {
                    return Err(ToolError::InvalidInput(format!(
                        "source_ref '{}' resolves outside the allowed directory",
                        row.source_ref
                    )));
                }
                source_path.to_path_buf()
            } else {
                // Relative paths must not contain `..` components.
                if source_path
                    .components()
                    .any(|c| c == std::path::Component::ParentDir)
                {
                    return Err(ToolError::InvalidInput(format!(
                        "source_ref '{}' contains path traversal components",
                        row.source_ref
                    )));
                }
                allowed_base.join(source_path)
            };
            if resolved.exists() {
                continue;
            }
            if let Some(parent) = resolved.parent() {
                std::fs::create_dir_all(parent).map_err(|e| ToolError::Internal(e.to_string()))?;
            }
            let bytes = request.raw_context_bytes.as_deref().ok_or_else(|| {
                ToolError::InvalidInput(
                    "raw_context_bytes required when source_ref file does not exist".to_string(),
                )
            })?;
            std::fs::write(&resolved, bytes).map_err(|e| ToolError::Internal(e.to_string()))?;
        }

        let response = self.ingest_statement_rows(IngestStatementRowsRequest {
            journal_path: request.journal_path,
            workbook_path: request.workbook_path,
            ontology_path: request.ontology_path,
            rows: request.extracted_rows,
        })?;
        Ok(IngestPdfResponse {
            inserted_count: response.inserted_count,
            tx_ids: response.tx_ids,
        })
    }

    fn get_raw_context(
        &self,
        request: GetRawContextRequest,
    ) -> Result<GetRawContextResponse, ToolError> {
        let allowed_base = self
            .workbook_path()
            .parent()
            .ok_or_else(|| {
                ToolError::InvalidInput("workbook_path must have a parent directory".to_string())
            })?
            .to_path_buf();

        let rkyv_path = &request.rkyv_ref;
        if rkyv_path
            .components()
            .any(|c| c == std::path::Component::ParentDir)
        {
            return Err(ToolError::InvalidInput(format!(
                "rkyv_ref '{}' contains path traversal components",
                rkyv_path.display()
            )));
        }
        let resolved = if rkyv_path.is_absolute() {
            if !rkyv_path.starts_with(&allowed_base) {
                return Err(ToolError::InvalidInput(format!(
                    "rkyv_ref '{}' resolves outside the allowed directory",
                    rkyv_path.display()
                )));
            }
            rkyv_path.to_path_buf()
        } else {
            allowed_base.join(rkyv_path)
        };

        let bytes = std::fs::read(&resolved).map_err(|e| ToolError::Internal(e.to_string()))?;
        Ok(GetRawContextResponse { bytes })
    }

    fn run_rhai_rule(&self, request: RunRhaiRuleRequest) -> Result<RunRhaiRuleResponse, ToolError> {
        let sample = SampleTransaction {
            tx_id: request.sample_tx.tx_id,
            account_id: request.sample_tx.account_id,
            date: request.sample_tx.date,
            amount: request.sample_tx.amount,
            description: request.sample_tx.description,
        };
        let classification = self
            .classification_state
            .lock()
            .map_err(|_| ToolError::Internal("classification lock poisoned".to_string()))?
            .engine
            .run_rule_from_file(&request.rule_file, &sample)
            .map_err(|e| ToolError::InvalidInput(e.to_string()))?;

        Ok(RunRhaiRuleResponse {
            category: classification.category,
            confidence: classification.confidence,
            review: classification.needs_review,
            reason: classification.reason,
        })
    }

    fn classify_ingested(
        &self,
        request: ClassifyIngestedRequest,
    ) -> Result<ClassifyIngestedResponse, ToolError> {
        let mut classification = self
            .classification_state
            .lock()
            .map_err(|_| ToolError::Internal("classification lock poisoned".to_string()))?;

        let rows = classification.tx_rows.values().cloned().collect::<Vec<_>>();
        let batch = classification
            .engine
            .classify_rows_from_file(&request.rule_file, &rows, request.review_threshold)
            .map_err(|e| ToolError::InvalidInput(e.to_string()))?;

        let timestamp = now_timestamp();
        let mut results = Vec::with_capacity(batch.classifications.len());
        for c in batch.classifications {
            let confidence = Decimal::try_from(c.confidence).unwrap_or(Decimal::ZERO);
            let old = classification.classifications.get(&c.tx_id).cloned();
            // Emit audit entries for every change, including first classifications (old_value: None).
            if old.as_ref().map(|e| e.category.as_str()) != Some(c.category.as_str()) {
                classification.audit_log.push(AuditEntry {
                    timestamp: timestamp.clone(),
                    actor: "rhai-rule".to_string(),
                    tx_id: c.tx_id.clone(),
                    field: "category".to_string(),
                    old_value: old.as_ref().map(|e| e.category.clone()),
                    new_value: c.category.clone(),
                    note: Some(c.reason.clone()),
                });
            }
            if old.as_ref().map(|e| e.confidence) != Some(confidence) {
                classification.audit_log.push(AuditEntry {
                    timestamp: timestamp.clone(),
                    actor: "rhai-rule".to_string(),
                    tx_id: c.tx_id.clone(),
                    field: "confidence".to_string(),
                    old_value: old.as_ref().map(|e| e.confidence.to_string()),
                    new_value: confidence.to_string(),
                    note: Some(c.reason.clone()),
                });
            }
            classification.classifications.insert(
                c.tx_id.clone(),
                StoredClassification {
                    category: c.category.clone(),
                    confidence,
                },
            );
            results.push(ClassifiedTxResponse {
                tx_id: c.tx_id,
                category: c.category,
                confidence: c.confidence,
                needs_review: c.needs_review,
                reason: c.reason,
            });
        }
        drop(classification);
        self.persist_state()?;

        Ok(ClassifyIngestedResponse {
            classifications: results,
        })
    }

    fn query_flags(&self, request: QueryFlagsRequest) -> Result<QueryFlagsResponse, ToolError> {
        let status = match request.status {
            FlagStatusRequest::Open => FlagStatus::Open,
            FlagStatusRequest::Resolved => FlagStatus::Resolved,
        };
        let flags = self
            .classification_state
            .lock()
            .map_err(|_| ToolError::Internal("classification lock poisoned".to_string()))?
            .engine
            .query_flags(request.year, status);

        Ok(QueryFlagsResponse {
            flags: flags
                .into_iter()
                .map(|f| FlagRecordResponse {
                    tx_id: f.tx_id,
                    year: f.year,
                    status: match f.status {
                        FlagStatus::Open => FlagStatusRequest::Open,
                        FlagStatus::Resolved => FlagStatusRequest::Resolved,
                    },
                    reason: f.reason,
                    category: f.category,
                    confidence: f.confidence,
                })
                .collect(),
        })
    }

    fn query_transactions(&self, request: QueryTransactionsRequest) -> Result<QueryTransactionsResponse, ToolError> {
        // Lock the classification state
        let classification = self
            .classification_state
            .lock()
            .map_err(|_| ToolError::Internal("classification lock poisoned".to_string()))?;

        // Apply filters
        let mut filtered = apply_transaction_filters(&classification.tx_rows, &classification.classifications, &request.filters)?;

        // Get total count before pagination
        let total_count = filtered.len();

        // Apply sorting
        filtered = apply_transaction_sort(filtered, &request.sort);

        // Apply pagination
        let pagination = request.pagination.unwrap_or(PaginationSpec {
            limit: 100,
            offset: 0,
        });
        let limit = pagination.limit.min(1000) as usize; // Max 1000 per page
        let offset = pagination.offset as usize;
        let paginated: Vec<_> = if offset < filtered.len() {
            filtered.into_iter().skip(offset).take(limit).collect()
        } else {
            Vec::new()
        };

        // Build response rows
        let transactions: Vec<TransactionRowResponse> = paginated.into_iter()
            .map(|(tx_id, tx, classification)| TransactionRowResponse {
                tx_id: tx_id.clone(),
                account_id: tx.account_id.clone(),
                date: tx.date.clone(),
                amount: tx.amount.clone(),
                description: tx.description.clone(),
                source_ref: tx.source_ref.clone(),
                category: classification.map(|c| c.category.clone()),
                confidence: classification.map(|c| c.confidence.to_string()),
            })
            .collect();

        Ok(QueryTransactionsResponse {
            transactions,
            total_count,
        })
    }

    fn classify_transaction(
        &self,
        request: ClassifyTransactionRequest,
    ) -> Result<ClassifyTransactionResponse, ToolError> {
        self.apply_classification_action(request, "classification")
    }

    fn reconcile_excel_classification(
        &self,
        request: ReconcileExcelClassificationRequest,
    ) -> Result<ClassifyTransactionResponse, ToolError> {
        self.apply_classification_action(
            ClassifyTransactionRequest {
                tx_id: request.tx_id,
                category: request.category,
                confidence: request.confidence,
                note: request.note,
                actor: request.actor,
            },
            "reconciliation",
        )
    }

    fn query_audit_log(
        &self,
        _request: QueryAuditLogRequest,
    ) -> Result<QueryAuditLogResponse, ToolError> {
        let entries = self
            .classification_state
            .lock()
            .map_err(|_| ToolError::Internal("classification lock poisoned".to_string()))?
            .audit_log
            .clone();
        Ok(QueryAuditLogResponse {
            entries: entries.into_iter().map(to_audit_response).collect(),
        })
    }

    fn export_cpa_workbook(
        &self,
        request: ExportCpaWorkbookRequest,
    ) -> Result<ExportCpaWorkbookResponse, ToolError> {
        let active_year = self.manifest.session.active_year;

        // Clone the data needed for export while holding the lock, then release
        // it before the (potentially slow) workbook build and disk write.
        let (tx_rows, classifications, audit_log, open_flags, resolved_flags) = {
            let classification = self
                .classification_state
                .lock()
                .map_err(|_| ToolError::Internal("classification lock poisoned".to_string()))?;
            let tx_rows = classification.tx_rows.clone();
            let classifications = classification.classifications.clone();
            let audit_log = classification.audit_log.clone();
            let open_flags = classification
                .engine
                .query_flags(active_year as i32, FlagStatus::Open);
            let resolved_flags = classification
                .engine
                .query_flags(active_year as i32, FlagStatus::Resolved);
            (
                tx_rows,
                classifications,
                audit_log,
                open_flags,
                resolved_flags,
            )
        };

        // Workbook export is an artifact projection, not a mutable source of truth.
        // Rebuild the full accountant-facing workbook from canonical service state on
        // every export so the handoff file stays consistent with the declared contract.
        let mut workbook = Workbook::new();
        for sheet_name in REQUIRED_SHEETS {
            workbook
                .add_worksheet()
                .set_name(*sheet_name)
                .map_err(map_xlsx)?;
        }
        let mut sheets_written: usize = REQUIRED_SHEETS.len();

        let mut categories = BTreeSet::new();
        categories.insert("Uncategorized".to_string());
        for entry in classifications.values() {
            categories.insert(entry.category.clone());
        }

        {
            let meta_sheet = workbook
                .worksheet_from_name("META.config")
                .map_err(map_xlsx)?;
            meta_sheet
                .write_string(0, 0, "workbook_path")
                .map_err(map_xlsx)?;
            meta_sheet
                .write_string(0, 1, "active_year")
                .map_err(map_xlsx)?;
            meta_sheet
                .write_string(1, 0, &self.manifest.session.workbook_path)
                .map_err(map_xlsx)?;
            meta_sheet
                .write_number(1, 1, f64::from(active_year))
                .map_err(map_xlsx)?;
        }

        {
            let account_sheet = workbook
                .worksheet_from_name("ACCT.registry")
                .map_err(map_xlsx)?;
            account_sheet
                .write_string(0, 0, "account_id")
                .map_err(map_xlsx)?;
            account_sheet
                .write_string(0, 1, "institution")
                .map_err(map_xlsx)?;
            account_sheet
                .write_string(0, 2, "account_type")
                .map_err(map_xlsx)?;
            account_sheet
                .write_string(0, 3, "currency")
                .map_err(map_xlsx)?;
            for (idx, (account_id, account)) in self.manifest.accounts.iter().enumerate() {
                let row = (idx + 1) as u32;
                account_sheet
                    .write_string(row, 0, account_id)
                    .map_err(map_xlsx)?;
                account_sheet
                    .write_string(row, 1, &account.institution)
                    .map_err(map_xlsx)?;
                account_sheet
                    .write_string(row, 2, &account.account_type)
                    .map_err(map_xlsx)?;
                account_sheet
                    .write_string(row, 3, &account.currency)
                    .map_err(map_xlsx)?;
            }
        }

        {
            let cat_sheet = workbook
                .worksheet_from_name("CAT.taxonomy")
                .map_err(map_xlsx)?;
            cat_sheet.write_string(0, 0, "category").map_err(map_xlsx)?;
            for (idx, category) in categories.iter().enumerate() {
                cat_sheet
                    .write_string((idx + 1) as u32, 0, category)
                    .map_err(map_xlsx)?;
            }
        }

        let mut by_account = BTreeMap::<String, Vec<(String, TransactionInput)>>::new();
        for (tx_id, row) in &tx_rows {
            by_account
                .entry(row.account_id.clone())
                .or_default()
                .push((tx_id.clone(), row.clone()));
        }

        for (account, rows) in by_account {
            let sheet_name = format!("TX.{account}");
            let ws = workbook
                .add_worksheet()
                .set_name(sheet_name)
                .map_err(map_xlsx)?;
            sheets_written += 1;
            ws.write_string(0, 0, "tx_id").map_err(map_xlsx)?;
            ws.write_string(0, 1, "date").map_err(map_xlsx)?;
            ws.write_string(0, 2, "amount").map_err(map_xlsx)?;
            ws.write_string(0, 3, "description").map_err(map_xlsx)?;
            ws.write_string(0, 4, "category").map_err(map_xlsx)?;
            ws.write_string(0, 5, "confidence").map_err(map_xlsx)?;
            ws.write_string(0, 6, "source_ref").map_err(map_xlsx)?;

            for (idx, (tx_id, row)) in rows.into_iter().enumerate() {
                let line = (idx + 1) as u32;
                let classified = classifications.get(&tx_id);
                ws.write_string(line, 0, tx_id).map_err(map_xlsx)?;
                ws.write_string(line, 1, &row.date).map_err(map_xlsx)?;
                ws.write_string(line, 2, &row.amount).map_err(map_xlsx)?;
                ws.write_string(line, 3, &row.description)
                    .map_err(map_xlsx)?;
                ws.write_string(
                    line,
                    4,
                    classified
                        .map(|c| c.category.as_str())
                        .unwrap_or("Uncategorized"),
                )
                .map_err(map_xlsx)?;
                ws.write_string(
                    line,
                    5,
                    classified
                        .map(|c| c.confidence.to_string())
                        .unwrap_or_else(|| "0.0".to_string()),
                )
                .map_err(map_xlsx)?;
                ws.write_string(line, 6, &row.source_ref)
                    .map_err(map_xlsx)?;
            }
        }

        {
            let ws = workbook
                .worksheet_from_name("FLAGS.open")
                .map_err(map_xlsx)?;
            ws.write_string(0, 0, "tx_id").map_err(map_xlsx)?;
            ws.write_string(0, 1, "reason").map_err(map_xlsx)?;
            for (idx, flag) in open_flags.iter().enumerate() {
                ws.write_string((idx + 1) as u32, 0, &flag.tx_id)
                    .map_err(map_xlsx)?;
                ws.write_string((idx + 1) as u32, 1, &flag.reason)
                    .map_err(map_xlsx)?;
            }
        }
        {
            let ws = workbook
                .worksheet_from_name("FLAGS.resolved")
                .map_err(map_xlsx)?;
            ws.write_string(0, 0, "tx_id").map_err(map_xlsx)?;
            ws.write_string(0, 1, "reason").map_err(map_xlsx)?;
            for (idx, flag) in resolved_flags.iter().enumerate() {
                ws.write_string((idx + 1) as u32, 0, &flag.tx_id)
                    .map_err(map_xlsx)?;
                ws.write_string((idx + 1) as u32, 1, &flag.reason)
                    .map_err(map_xlsx)?;
            }
        }

        write_schedule_sheet(
            &mut workbook,
            "SCHED.C",
            &build_schedule_summary_from_classification(
                &tx_rows,
                &classifications,
                active_year as i32,
                ScheduleKindRequest::ScheduleC,
            ),
        )?;
        write_schedule_sheet(
            &mut workbook,
            "SCHED.D",
            &build_schedule_summary_from_classification(
                &tx_rows,
                &classifications,
                active_year as i32,
                ScheduleKindRequest::ScheduleD,
            ),
        )?;
        write_schedule_sheet(
            &mut workbook,
            "SCHED.E",
            &build_schedule_summary_from_classification(
                &tx_rows,
                &classifications,
                active_year as i32,
                ScheduleKindRequest::ScheduleE,
            ),
        )?;
        write_schedule_sheet(
            &mut workbook,
            "FBAR.accounts",
            &build_schedule_summary_from_classification(
                &tx_rows,
                &classifications,
                active_year as i32,
                ScheduleKindRequest::Fbar,
            ),
        )?;

        {
            let audit_sheet = workbook
                .worksheet_from_name("AUDIT.log")
                .map_err(map_xlsx)?;
            audit_sheet
                .write_string(0, 0, "timestamp")
                .map_err(map_xlsx)?;
            audit_sheet.write_string(0, 1, "actor").map_err(map_xlsx)?;
            audit_sheet.write_string(0, 2, "tx_id").map_err(map_xlsx)?;
            audit_sheet.write_string(0, 3, "field").map_err(map_xlsx)?;
            audit_sheet
                .write_string(0, 4, "old_value")
                .map_err(map_xlsx)?;
            audit_sheet
                .write_string(0, 5, "new_value")
                .map_err(map_xlsx)?;
            audit_sheet.write_string(0, 6, "note").map_err(map_xlsx)?;

            for (idx, entry) in audit_log.iter().enumerate() {
                let row = (idx + 1) as u32;
                audit_sheet
                    .write_string(row, 0, &entry.timestamp)
                    .map_err(map_xlsx)?;
                audit_sheet
                    .write_string(row, 1, &entry.actor)
                    .map_err(map_xlsx)?;
                audit_sheet
                    .write_string(row, 2, &entry.tx_id)
                    .map_err(map_xlsx)?;
                audit_sheet
                    .write_string(row, 3, &entry.field)
                    .map_err(map_xlsx)?;
                audit_sheet
                    .write_string(row, 4, entry.old_value.as_deref().unwrap_or(""))
                    .map_err(map_xlsx)?;
                audit_sheet
                    .write_string(row, 5, &entry.new_value)
                    .map_err(map_xlsx)?;
                audit_sheet
                    .write_string(row, 6, entry.note.as_deref().unwrap_or(""))
                    .map_err(map_xlsx)?;
            }
        }

        workbook.save(&request.workbook_path).map_err(map_xlsx)?;

        // Emit WorkbookRow evidence for each classified transaction
        for (tx_id, stored_cls) in &classifications {
            let wb_row = arc_kit_au::node::WorkbookRow {
                tx_id: tx_id.clone(),
                sheet_name: "Transactions".to_string(),
                row_index: 0,
                category: stored_cls.category.clone(),
                amount: String::new(),
                exported_at: chrono::Utc::now(),
            };
            if let Ok(mut evidence) = self.evidence.lock() {
                let mut builder = arc_kit_au::EvidenceBuilder::new(&mut evidence);
                builder.ensure_workbook_row(wb_row);
            }
        }

        Ok(ExportCpaWorkbookResponse { sheets_written })
    }

    fn get_schedule_summary(
        &self,
        request: GetScheduleSummaryRequest,
    ) -> Result<GetScheduleSummaryResponse, ToolError> {
        let classification = self
            .classification_state
            .lock()
            .map_err(|_| ToolError::Internal("classification lock poisoned".to_string()))?;
        Ok(build_schedule_summary_from_classification(
            &classification.tx_rows,
            &classification.classifications,
            request.year,
            request.schedule,
        ))
    }

    // TODO: Rollback guidance for all-or-nothing mode failures:
    // When batch_mode=AllOrNothing and failures occur, operators should:
    // 1. Re-query affected tx_ids via query_transactions
    // 2. Manually reverse classifications using classify_transaction with original category
    // This is intentional trade-off vs full transactional rollback implementation
    fn batch_classify(
        &self,
        request: BatchClassifyRequest,
    ) -> Result<BatchClassifyResponse, ToolError> {
        let classification = self
            .classification_state
            .lock()
            .map_err(|_| ToolError::Internal("classification lock poisoned".to_string()))?;

        let dry_run = request.dry_run;
        let batch_mode = request.batch_mode;
        // Validate confidence at entry; the parsed value is not used directly — the raw string
        // is forwarded to apply_classification_action which re-parses it per-item.
        let _confidence = parse_confidence(&request.confidence)?;
        drop(classification);

        if dry_run {
            // Dry run mode: return all items as skipped
            let items: Vec<BatchItemResult> = request
                .tx_ids
                .iter()
                .map(|tx_id| BatchItemResult {
                    tx_id: tx_id.clone(),
                    status: BatchItemStatus::Skipped {
                        reason: "dry_run".to_string(),
                    },
                    audit_entries: vec![],
                })
                .collect();

            let summary = BatchSummary {
                total_requested: request.tx_ids.len(),
                succeeded: 0,
                failed: 0,
                skipped: request.tx_ids.len(),
                batch_duration_ms: 0,
            };

            return Ok(BatchClassifyResponse { summary, items });
        }

        // Normal mode: execute operations and collect both items and summary
        let start = std::time::Instant::now();
        let total_requested = request.tx_ids.len();
        let mut succeeded = 0;
        let mut failed = 0;
        let mut items = Vec::new();

        for tx_id in &request.tx_ids {
            match self.apply_classification_action(
                ClassifyTransactionRequest {
                    tx_id: tx_id.to_string(),
                    category: request.category.clone(),
                    confidence: request.confidence.clone(),
                    note: request.note.clone(),
                    actor: request.actor.clone(),
                },
                "classification",
            ) {
                Ok(response) => {
                    succeeded += 1;
                    items.push(BatchItemResult {
                        tx_id: response.tx_id,
                        status: BatchItemStatus::Succeeded,
                        audit_entries: response.audit_entries,
                    });
                }
                Err(e) => {
                    failed += 1;
                    items.push(BatchItemResult {
                        tx_id: tx_id.clone(),
                        status: BatchItemStatus::Failed { error: e.to_string() },
                        audit_entries: vec![],
                    });
                    
                    if batch_mode == BatchMode::AllOrNothing {
                        break;
                    }
                }
            }
        }

        let duration = start.elapsed().as_millis() as u64;
        let summary = BatchSummary {
            total_requested,
            succeeded,
            failed,
            skipped: 0,
            batch_duration_ms: duration,
        };

        Ok(BatchClassifyResponse { summary, items })
    }

    // TODO: Rollback guidance for all-or-nothing mode failures

    // Simplified flag resolution - only supports Open -> Resolved transitions
    // TODO: Rollback guidance for all-or-nothing mode failures
    // TODO: Flag resolution requires ledger-core update to expose flag resolution API
    // Current ClassificationEngine only supports Open and Resolved states
    // and does not provide a public method to resolve flags
    fn bulk_resolve_flags(
        &self,
        request: BatchResolveFlagsRequest,
    ) -> Result<BulkResolveFlagsResponse, ToolError> {
        if !request.dry_run {
            return Err(ToolError::Internal(
                "bulk_resolve_flags requires ledger-core update: ClassificationEngine needs a public flag resolution method".to_string()
            ));
        }

        // Dry run implementation - just return skipped items
        let items: Vec<BatchItemResult> = request
            .tx_ids
            .iter()
            .map(|tx_id| BatchItemResult {
                tx_id: tx_id.clone(),
                status: BatchItemStatus::Skipped {
                    reason: "dry_run".to_string(),
                },
                audit_entries: vec![],
            })
            .collect();

        Ok(BulkResolveFlagsResponse {
            summary: BatchSummary {
                total_requested: request.tx_ids.len(),
                succeeded: 0,
                failed: 0,
                skipped: request.tx_ids.len(),
                batch_duration_ms: 0,
            },
            items,
        })
    }
    fn apply_mapping_bulk(
        &self,
        request: ApplyMappingBulkRequest,
    ) -> Result<ApplyMappingBulkResponse, ToolError> {
        let classification = self
            .classification_state
            .lock()
            .map_err(|_| ToolError::Internal("classification lock poisoned".to_string()))?;

        let dry_run = request.dry_run;
        let batch_mode = request.batch_mode;

        // Get source transaction
        let source_category = classification
            .classifications
            .get(&request.source_tx_id)
            .ok_or_else(|| ToolError::InvalidInput("source_tx_id not classified".to_string()))?
            .category
            .clone();

        let _source_row = classification
            .tx_rows
            .get(&request.source_tx_id)
            .ok_or_else(|| ToolError::InvalidInput("source_tx_id not found".to_string()))?;

        drop(classification);


        // TODO: Build transient index for O(n) lookup (deferred optimization)
        // For now, O(n²) search through all transactions

        let mut matches = Vec::new();
        {
            let classification = self
                .classification_state
                .lock()
                .map_err(|_| ToolError::Internal("classification lock poisoned".to_string()))?;

            for (tx_id, row) in &classification.tx_rows {
                if tx_id == &request.source_tx_id {
                    continue; // Skip source
                }

                // Check similarity based on match_fields
                let is_similar = request.match_fields.iter().any(|field| {
                    match field.as_str() {
                        "description" => match request.similarity_type {
                            SimilarityMatchType::Exact => row.description.to_lowercase()
                                == source_category.to_lowercase(),
                            SimilarityMatchType::Substring => row.description
                                .to_lowercase()
                                .contains(&source_category.to_lowercase()),
                            SimilarityMatchType::Prefix => row.description.to_lowercase()
                                .starts_with(&source_category.to_lowercase()),
                        },
                        "category" => row.description.to_lowercase() == source_category.to_lowercase(),
                        "amount" => {
                            let amt1 = Decimal::from_str(&row.amount).unwrap();
                            let amt2 = rust_decimal::Decimal::ZERO;
                            amt1 == amt2
                        }
                        _ => false,
                    }
                });

                if is_similar {
                    matches.push(tx_id.clone());
                }
            }
        }

        // Sort matches and limit by max_matches
        matches.truncate(request.max_matches);

        let match_count = matches.len();

        // Use BatchExecutor to apply classifications
        let classification_summary = if dry_run {
            // Return empty summary (no state mutations)
            BatchSummary {
                total_requested: match_count,
                succeeded: 0,
                failed: 0,
                skipped: match_count,
                batch_duration_ms: 0,
            }
        } else {
            BatchExecutor::execute_batch(
                matches.clone(),
                batch_mode,
                false, // dry_run=false
                |tx_id| {
                    // Reuse apply_classification_action
                    let response = self.apply_classification_action(
                        ClassifyTransactionRequest {
                            tx_id: tx_id.to_string(),
                            category: request.target_category.clone(),
                            confidence: request.target_confidence.clone(),
                            note: Some(format!("bulk applied from {}", request.source_tx_id)),
                            actor: request.actor.clone(),
                        },
                        "bulk_mapping",
                    )?;

                    Ok(BatchItemResult {
                        tx_id: response.tx_id,
                        status: BatchItemStatus::Succeeded,
                        audit_entries: response.audit_entries,
                    })
                },
            )?
        };

        Ok(ApplyMappingBulkResponse {
            classification_summary,
            matched_tx_ids: matches,
            items: vec![], // Populated by caller from audit log
        })
    }

    fn fetch_work_queue(
        &self,
        request: FetchQueueRequest,
    ) -> Result<FetchQueueResponse, ToolError> {
        let classification = self.classification_state.lock()
            .map_err(|_| ToolError::Internal("classification lock poisoned".to_string()))?;
        
        // Phase 1: Gather data from all sources
        let mut items = Vec::new();
        
        // 1. Query flags (open + resolved across known transaction years)
        if request.item_types.as_ref().map_or(true, |v| v.is_empty() || v.contains(&QueueItemType::Flag)) {
            let years: BTreeSet<i32> = classification
                .tx_rows
                .values()
                .map(|tx| derive_year(&tx.date))
                .filter(|year| (1900..=9999).contains(year))
                .collect();

            for year in years {
                let open_flags = classification
                    .engine
                    .query_flags(year, FlagStatus::Open);
                for flag in open_flags {
                    let flag_resp = FlagRecordResponse {
                        tx_id: flag.tx_id,
                        year: flag.year,
                        status: FlagStatusRequest::Open,
                        reason: flag.reason,
                        category: flag.category,
                        confidence: flag.confidence,
                    };
                    items.push(flag_to_queue_item(&flag_resp));
                }

                let resolved_flags = classification
                    .engine
                    .query_flags(year, FlagStatus::Resolved);
                for flag in resolved_flags {
                    let flag_resp = FlagRecordResponse {
                        tx_id: flag.tx_id,
                        year: flag.year,
                        status: FlagStatusRequest::Resolved,
                        reason: flag.reason,
                        category: flag.category,
                        confidence: flag.confidence,
                    };
                    items.push(flag_to_queue_item(&flag_resp));
                }
            }
        }
        
        drop(classification);
        
        // 2. Query tax ambiguities (if item_types includes Ambiguity)
        // Note: This requires a full reconciliation stage which may be expensive
        // For now, we return empty if requested (can be enhanced later)
        if request.item_types.as_ref().map_or(false, |v| v.contains(&QueueItemType::Ambiguity)) {
            // TODO: Implement tax ambiguity query
            // This would require running a reconciliation stage
        }
        
        // 3. Query manual changes from audit log (if item_types includes ManualChange)
        if request.item_types.as_ref().map_or(true, |v| v.is_empty() || v.contains(&QueueItemType::ManualChange)) {
            let lifecycle = self.lifecycle_events.lock()
                .map_err(|_| ToolError::Internal("events lock poisoned".to_string()))?;
            
            // Get all events (filtering by time_end would require changes to EventHistoryFilter)
            let filter = EventHistoryFilter {
                tx_id: None,
                document_ref: None,
                time_start: None,
                time_end: None,
            };
            let events = lifecycle.list_events(filter).map_err(|_| {
                ToolError::Internal("Failed to query event history".to_string())
            })?.events;
            
            for event in events {
                // Filter for manual changes (non-agent adjustments/classifications)
                let actor = event.payload.get("actor")
                    .cloned()
                    .unwrap_or("unknown".to_string());
                if actor != "agent" && (event.event_type == "adjustment" || event.event_type == "classification") {
                    items.push(manual_change_to_queue_item(&event));
                }
            }
        }
        
        // 4. Query reconciliation blockers (if item_types includes Blocker)
        // Note: This requires running reconciliation which may be expensive
        // For now, we return empty if requested (can be enhanced later)
        if request.item_types.as_ref().map_or(false, |v| v.contains(&QueueItemType::Blocker)) {
            // TODO: Implement blocker query
            // This would require running a reconciliation stage
        }
        
        // 5. Query document issues (if item_types includes DocumentIssue)
        // Note: This would require checking document registry for failed ingests
        // For now, we return empty if requested (can be enhanced later)
        if request.item_types.as_ref().map_or(false, |v| v.contains(&QueueItemType::DocumentIssue)) {
            // TODO: Implement document issue query
            // This would require checking document registry
        }
        
        // Apply status filter
        if let Some(ref statuses) = request.statuses {
            if !statuses.is_empty() {
                items.retain(|item| statuses.contains(&item.status));
            }
        }
        
        // Apply updated_after filter
        if let Some(ref after) = request.updated_after {
            items.retain(|item| item.created_at.as_str() > after.as_str());
        }
        
        // Phase 2: Sort by default (created_at descending)
        items.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        
        // Phase 3: Paginate
        let total_count = items.len() as u64;
        let limit = request.limit;
        let offset = request.offset as usize;
        
        let paginated: Vec<QueueItem> = items
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect();
        
        Ok(FetchQueueResponse {
            items: paginated,
            total_count,
            offset: request.offset as u32,
            limit: limit as u32,
        })
    }
}

fn parse_confidence(input: &str) -> Result<Decimal, ToolError> {
    let confidence = Decimal::from_str(input)
        .map_err(|_| ToolError::InvalidInput("confidence must be a valid decimal".to_string()))?;
    if confidence < Decimal::ZERO || confidence > Decimal::ONE {
        return Err(ToolError::InvalidInput(
            "confidence must be between 0 and 1".to_string(),
        ));
    }
    Ok(confidence)
}

fn validate_invariants(
    row: &TransactionInput,
    tx_id: &str,
    category: &str,
) -> Result<(), ToolError> {
    if category.trim().is_empty() {
        return Err(ToolError::InvalidInput(
            "category must not be empty".to_string(),
        ));
    }
    Decimal::from_str(row.amount.trim())
        .map_err(|_| ToolError::InvalidInput("invalid amount decimal".to_string()))?;

    if deterministic_tx_id(row) != tx_id {
        return Err(ToolError::InvalidInput(
            "tx_id invariant violation: deterministic hash mismatch".to_string(),
        ));
    }

    let parts: Vec<&str> = row.date.split('-').collect();
    if parts.len() != 3
        || parts[0].parse::<u32>().is_err()
        || parts[1].parse::<u32>().is_err()
        || parts[2].parse::<u32>().is_err()
    {
        return Err(ToolError::InvalidInput(
            "schema invariant violation: date must be YYYY-MM-DD".to_string(),
        ));
    }
    Ok(())
}

fn now_timestamp() -> String {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => format!("{}", d.as_secs()),
        Err(_) => "0".to_string(),
    }
}

fn resolve_document_inventory_directory(
    workbook_path: &std::path::Path,
    directory: &std::path::Path,
) -> Result<PathBuf, ToolError> {
    if directory
        .components()
        .any(|component| component == std::path::Component::ParentDir)
    {
        return Err(ToolError::InvalidInput(
            "directory must not contain parent traversal components".to_string(),
        ));
    }

    let resolved = if directory.is_absolute() {
        directory.to_path_buf()
    } else {
        let base = workbook_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(std::path::Path::to_path_buf)
            .unwrap_or(std::env::current_dir().map_err(|e| ToolError::Internal(e.to_string()))?);
        base.join(directory)
    };

    if !resolved.is_dir() {
        return Err(ToolError::InvalidInput(format!(
            "directory '{}' does not exist or is not a directory",
            resolved.display()
        )));
    }
    Ok(resolved)
}

fn collect_document_paths(
    directory: &std::path::Path,
    recursive: bool,
) -> Result<Vec<PathBuf>, ToolError> {
    let mut documents = Vec::new();
    collect_document_paths_into(directory, recursive, &mut documents)?;
    documents.sort();
    Ok(documents)
}

fn collect_document_paths_into(
    directory: &std::path::Path,
    recursive: bool,
    documents: &mut Vec<PathBuf>,
) -> Result<(), ToolError> {
    let mut entries = std::fs::read_dir(directory)
        .map_err(|e| ToolError::Internal(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ToolError::Internal(e.to_string()))?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            if recursive {
                collect_document_paths_into(&path, true, documents)?;
            }
            continue;
        }
        let is_pdf = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("pdf"))
            .unwrap_or(false);
        if is_pdf {
            documents.push(path);
        }
    }

    Ok(())
}

fn build_document_record(
    service: &TurboLedgerService,
    known_source_refs: &[(String, PathBuf)],
    document_path: PathBuf,
) -> Result<DocumentRecordResponse, ToolError> {
    let file_name = document_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            ToolError::InvalidInput("document path must have a UTF-8 filename".to_string())
        })?
        .to_string();

    match service.validate_source_filename(&file_name) {
        Ok(parsed) => {
            let raw_context_ref = document_path.with_extension("rkyv");
            let ingested_tx_ids = known_source_refs
                .iter()
                .filter(|(_, source_ref)| source_ref_matches(source_ref, &raw_context_ref))
                .map(|(tx_id, _)| tx_id.clone())
                .collect::<Vec<_>>();
            let status = if ingested_tx_ids.is_empty() {
                DocumentQueueStatusRequest::Ready
            } else {
                DocumentQueueStatusRequest::Ingested
            };

            Ok(DocumentRecordResponse {
                file_name,
                document_path: document_path.display().to_string(),
                raw_context_ref: raw_context_ref.display().to_string(),
                status,
                blocked_reason: None,
                next_hint: if ingested_tx_ids.is_empty() {
                    "call_proxy_ingest_pdf".to_string()
                } else {
                    "review_existing_rows".to_string()
                },
                vendor: Some(parsed.vendor),
                account_id: Some(parsed.account),
                year_month: Some(format!("{:04}-{:02}", parsed.year, parsed.month)),
                document_type: Some(parsed.doc_type),
                ingested_tx_ids,
            })
        }
        Err(_) => Ok(DocumentRecordResponse {
            file_name,
            document_path: document_path.display().to_string(),
            raw_context_ref: document_path.with_extension("rkyv").display().to_string(),
            status: DocumentQueueStatusRequest::InvalidName,
            blocked_reason: Some("invalid_contract_name".to_string()),
            next_hint: "rename_then_retry".to_string(),
            vendor: None,
            account_id: None,
            year_month: None,
            document_type: None,
            ingested_tx_ids: Vec::new(),
        }),
    }
}

fn source_ref_matches(source_ref: &std::path::Path, expected: &std::path::Path) -> bool {
    let source_canonical = std::fs::canonicalize(source_ref).ok();
    let expected_canonical = std::fs::canonicalize(expected).ok();
    source_canonical.as_ref() == expected_canonical.as_ref()
        || source_ref == expected
        || source_ref.file_name() == expected.file_name()
}

fn document_status_rank(status: DocumentQueueStatusRequest) -> u8 {
    match status {
        DocumentQueueStatusRequest::Ingested => 0,
        DocumentQueueStatusRequest::Ready => 1,
        DocumentQueueStatusRequest::InvalidName => 2,
    }
}

fn persisted_state_path(workbook_path: &std::path::Path) -> PathBuf {
    PathBuf::from(format!("{}.ledgerr-state.json", workbook_path.display()))
}

fn load_persisted_state(
    workbook_path: &std::path::Path,
) -> Result<PersistedServiceState, ToolError> {
    let sidecar_path = persisted_state_path(workbook_path);
    if !sidecar_path.exists() {
        return Ok(PersistedServiceState::default());
    }

    let bytes = std::fs::read(&sidecar_path).map_err(|e| {
        ToolError::Internal(format!(
            "failed to read persisted state '{}': {e}",
            sidecar_path.display()
        ))
    })?;
    let state: PersistedServiceState = serde_json::from_slice(&bytes).map_err(|e| {
        ToolError::Internal(format!(
            "failed to parse persisted state '{}': {e}",
            sidecar_path.display()
        ))
    })?;
    if state.version != PERSISTED_STATE_VERSION {
        return Err(ToolError::Internal(format!(
            "unsupported persisted state version {} in '{}'",
            state.version,
            sidecar_path.display()
        )));
    }
    Ok(state)
}

fn persist_state_to_path(
    sidecar_path: &std::path::Path,
    state: &PersistedServiceState,
) -> Result<(), ToolError> {
    if let Some(parent) = sidecar_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).map_err(|e| {
            ToolError::Internal(format!(
                "failed to create persisted state directory '{}': {e}",
                parent.display()
            ))
        })?;
    }
    let bytes = serde_json::to_vec_pretty(state)
        .map_err(|e| ToolError::Internal(format!("failed to serialize persisted state: {e}")))?;
    // Write to a sibling temp file first, then rename atomically so a mid-write
    // crash never leaves a truncated/corrupt sidecar that causes startup to fail closed.
    let tmp_path: std::path::PathBuf = {
        let mut tmp_os_string = sidecar_path.as_os_str().to_os_string();
        tmp_os_string.push(".tmp");
        tmp_os_string.into()
    };
    std::fs::write(&tmp_path, &bytes).map_err(|e| {
        ToolError::Internal(format!(
            "failed to write persisted state temp file '{}': {e}",
            tmp_path.display()
        ))
    })?;
    std::fs::rename(&tmp_path, sidecar_path).map_err(|e| {
        ToolError::Internal(format!(
            "failed to rename persisted state '{}' to '{}': {e}",
            tmp_path.display(),
            sidecar_path.display()
        ))
    })
}

fn to_audit_response(entry: AuditEntry) -> AuditEntryResponse {
    AuditEntryResponse {
        timestamp: entry.timestamp,
        actor: entry.actor,
        tx_id: entry.tx_id,
        field: entry.field,
        old_value: entry.old_value,
        new_value: entry.new_value,
        note: entry.note,
    }
}

fn map_xlsx(err: rust_xlsxwriter::XlsxError) -> ToolError {
    ToolError::Internal(err.to_string())
}

fn derive_year(date: &str) -> i32 {
    date.split('-')
        .next()
        .and_then(|y| y.parse::<i32>().ok())
        .unwrap_or(0)
}

fn schedule_for_category(category: &str) -> Option<ScheduleKindRequest> {
    let category = category.to_ascii_lowercase();
    if category.contains("crypto") || category.contains("capital") || category.contains("baddebt") {
        return Some(ScheduleKindRequest::ScheduleD);
    }
    if category.contains("rent") || category.contains("property") {
        return Some(ScheduleKindRequest::ScheduleE);
    }
    if category != "uncategorized" {
        return Some(ScheduleKindRequest::ScheduleC);
    }
    None
}

fn write_schedule_sheet(
    workbook: &mut Workbook,
    sheet_name: &str,
    summary: &GetScheduleSummaryResponse,
) -> Result<(), ToolError> {
    let sheet = workbook.worksheet_from_name(sheet_name).map_err(map_xlsx)?;
    sheet.write_string(0, 0, "key").map_err(map_xlsx)?;
    sheet.write_string(0, 1, "total").map_err(map_xlsx)?;
    for (idx, line) in summary.lines.iter().enumerate() {
        let row = (idx + 1) as u32;
        sheet.write_string(row, 0, &line.key).map_err(map_xlsx)?;
        sheet.write_number(row, 1, line.total).map_err(map_xlsx)?;
    }
    Ok(())
}

fn build_schedule_summary_from_classification(
    tx_rows: &BTreeMap<String, TransactionInput>,
    classifications: &BTreeMap<String, StoredClassification>,
    year: i32,
    schedule: ScheduleKindRequest,
) -> GetScheduleSummaryResponse {
    let mut grouped = BTreeMap::<String, Decimal>::new();
    for (tx_id, row) in tx_rows {
        if derive_year(&row.date) != year {
            continue;
        }
        let amount = match Decimal::from_str(row.amount.trim()) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let key = match schedule {
            ScheduleKindRequest::Fbar => row.account_id.clone(),
            _ => {
                let category = classifications
                    .get(tx_id)
                    .map(|c| c.category.clone())
                    .unwrap_or_else(|| "Uncategorized".to_string());
                if schedule_for_category(&category) != Some(schedule) {
                    continue;
                }
                category
            }
        };

        if schedule == ScheduleKindRequest::Fbar {
            let abs_amount = amount.abs();
            let current = grouped.entry(key).or_insert(Decimal::ZERO);
            if abs_amount > *current {
                *current = abs_amount;
            }
        } else {
            *grouped.entry(key).or_insert(Decimal::ZERO) += amount;
        }
    }

    let lines = grouped
        .into_iter()
        .map(|(key, total)| ScheduleLineResponse {
            key,
            total: decimal_to_f64(total),
        })
        .collect::<Vec<_>>();
    let total = lines.iter().map(|line| line.total).sum::<f64>();
    GetScheduleSummaryResponse {
        year,
        schedule,
        total,
        lines,
    }
}

fn decimal_to_f64(value: Decimal) -> f64 {
    value.to_string().parse::<f64>().unwrap_or(0.0)
}

// ── Document registry persistence ─────────────────────────────────────────────

fn doc_registry_path(workbook: &std::path::Path) -> PathBuf {
    workbook.with_extension("docs.json")
}

fn load_document_registry(workbook: &std::path::Path) -> BTreeMap<String, DocumentRecord> {
    let path = doc_registry_path(workbook);
    if !path.exists() {
        return BTreeMap::new();
    }
    match std::fs::read_to_string(&path) {
        Err(e) => {
            tracing::warn!(path = %path.display(), err = %e, "failed to read document registry; starting empty");
            BTreeMap::new()
        }
        Ok(raw) => match serde_json::from_str(&raw) {
            Ok(registry) => registry,
            Err(e) => {
                tracing::warn!(path = %path.display(), err = %e, "failed to parse document registry; starting empty");
                BTreeMap::new()
            }
        },
    }
}

fn save_document_registry(
    workbook: &std::path::Path,
    registry: &BTreeMap<String, DocumentRecord>,
) -> Result<(), ToolError> {
    let path = doc_registry_path(workbook);
    let json =
        serde_json::to_string_pretty(registry).map_err(|e| ToolError::Internal(e.to_string()))?;
    std::fs::write(path, json).map_err(|e| ToolError::Internal(e.to_string()))
}

pub fn default_ontology_path_for_workbook(workbook: &std::path::Path) -> PathBuf {
    let file_name = workbook
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("ledger.xlsx");
    workbook.with_file_name(format!("{file_name}.ontology.json"))
}

fn emit_ingest_ontology_edges(
    ontology_path: &std::path::Path,
    rows: &[TransactionInput],
) -> Result<(), ToolError> {
    if rows.is_empty() {
        return Ok(());
    }

    let mut store = OntologyStore::load(ontology_path)?;

    for row in rows {
        let tx_id = deterministic_tx_id(row);
        let mut doc_attrs = BTreeMap::new();
        doc_attrs.insert("source_ref".to_string(), row.source_ref.clone());

        let mut tx_attrs = BTreeMap::new();
        tx_attrs.insert("tx_id".to_string(), tx_id.clone());
        tx_attrs.insert("account_id".to_string(), row.account_id.clone());
        tx_attrs.insert("date".to_string(), row.date.clone());
        tx_attrs.insert("amount".to_string(), row.amount.clone());
        tx_attrs.insert("description".to_string(), row.description.clone());

        let entity_ids = store
            .upsert_entities(vec![
                OntologyEntityInput {
                    kind: OntologyEntityKind::Document,
                    attrs: doc_attrs,
                },
                OntologyEntityInput {
                    kind: OntologyEntityKind::Transaction,
                    attrs: tx_attrs,
                },
            ])?
            .entity_ids;

        let mut provenance = BTreeMap::new();
        provenance.insert("emitter".to_string(), "ingest_statement_rows".to_string());
        provenance.insert("source_ref".to_string(), row.source_ref.clone());
        provenance.insert("tx_id".to_string(), tx_id);

        store.upsert_edges(vec![OntologyEdgeInput {
            from: entity_ids[0].clone(),
            to: entity_ids[1].clone(),
            relation: "documents_transaction".to_string(),
            provenance,
        }])?;
    }

    store.persist(ontology_path)
}

// ── New TurboLedgerService methods ────────────────────────────────────────────

/// Request/response types for new FDKMS tools.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestImageRequest {
    pub image_path: String,
    pub doc_type: Option<String>,
    pub tags: Vec<String>,
    pub extract_with_llm: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestImageResponse {
    pub doc_id: String,
    pub file_name: String,
    pub doc_type: String,
    pub tags: Vec<String>,
    pub llm_extracted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyTagsRequest {
    pub doc_ref: String,
    pub tags: Vec<String>,
    pub sync_fs: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyTagsResponse {
    pub doc_id: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListTaggedRequest {
    pub tags: Vec<String>,
    pub doc_type: Option<String>,
    pub directory: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListTaggedResponse {
    pub documents: Vec<DocumentSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentSummary {
    pub doc_id: String,
    pub file_name: String,
    pub doc_type: String,
    pub tags: Vec<String>,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncFsMetadataRequest {
    pub directory: PathBuf,
    pub recursive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncFsMetadataResponse {
    pub files_scanned: usize,
    pub files_synced: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizeFilenameRequest {
    pub file_path: String,
    pub vendor: String,
    pub account: String,
    pub year_month: String,
    pub doc_type: String,
    pub apply: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizeFilenameResponse {
    pub proposed_name: String,
    pub original_name: String,
    pub renamed: bool,
}

impl TurboLedgerService {
    /// Ingest an image document, compute its blake3 ID, apply tags, optionally run LLM extraction.
    pub fn ingest_image_tool(
        &self,
        request: IngestImageRequest,
    ) -> Result<IngestImageResponse, ToolError> {
        // Apply the same path traversal guardrails as ingest_pdf.
        let allowed_base = self
            .workbook_path()
            .parent()
            .ok_or_else(|| {
                ToolError::InvalidInput("workbook_path must have a parent directory".to_string())
            })?
            .to_path_buf();

        let raw_path = std::path::Path::new(&request.image_path);
        let resolved = if raw_path.is_absolute() {
            if raw_path
                .components()
                .any(|c| c == std::path::Component::ParentDir)
            {
                return Err(ToolError::InvalidInput(format!(
                    "image_path '{}' contains path traversal components",
                    request.image_path
                )));
            }
            if !raw_path.starts_with(&allowed_base) {
                return Err(ToolError::InvalidInput(format!(
                    "image_path '{}' resolves outside the allowed directory",
                    request.image_path
                )));
            }
            raw_path.to_path_buf()
        } else {
            if raw_path
                .components()
                .any(|c| c == std::path::Component::ParentDir)
            {
                return Err(ToolError::InvalidInput(format!(
                    "image_path '{}' contains path traversal components",
                    request.image_path
                )));
            }
            allowed_base.join(raw_path)
        };
        let path = resolved.as_path();
        let bytes = std::fs::read(path).map_err(|e| ToolError::Internal(e.to_string()))?;
        let doc_id = ledger_core::document::document_id_from_bytes(&bytes);
        let doc_type = request
            .doc_type
            .as_deref()
            .map(|s| match s {
                "receipt" => DocType::Receipt,
                "invoice" => DocType::Invoice,
                "bank_statement" => DocType::BankStatement,
                _ => DocType::from_path(path),
            })
            .unwrap_or_else(|| DocType::from_path(path));

        let doc_type_str = format!("{doc_type:?}");
        let (validated_tags, _) = parse_tags(&request.tags);

        let mut record = DocumentRecord::new(doc_id.clone(), request.image_path.clone(), doc_type);
        record.status = DocumentStatus::Indexed;

        let mut llm_extracted = false;

        // Apply tags (including auto-tag based on doc type).
        for tag in &validated_tags {
            record.add_tag(tag.clone());
        }
        if record.doc_type.is_image()
            && !request
                .tags
                .iter()
                .any(|t| t.contains("receipt") || t.contains("invoice"))
        {
            if let Ok(t) = Tag::new(ledger_core::tags::TAG_RECEIPT) {
                record.add_tag(t);
            }
        }

        // LLM extraction (requires llm feature).
        #[cfg(feature = "llm")]
        if request.extract_with_llm {
            if let Some(llm) = &self.llm {
                let mime = record.doc_type.mime_type();
                if let Ok(extraction) = llm.extract_receipt_bytes(&bytes, mime) {
                    if let Some(vendor) = &extraction.vendor_name {
                        record.metadata.insert(
                            "vendor_name".into(),
                            serde_json::Value::String(vendor.clone()),
                        );
                    }
                    if let Some(date) = &extraction.date {
                        record
                            .metadata
                            .insert("date".into(), serde_json::Value::String(date.clone()));
                    }
                    if let Some(total) = extraction.total_amount {
                        record
                            .metadata
                            .insert("total_amount".into(), serde_json::json!(total.to_string()));
                    }
                    for tag_str in &extraction.suggested_tags {
                        if let Ok(t) = Tag::new(tag_str) {
                            record.add_tag(t);
                        }
                    }
                    llm_extracted = true;
                    // Mark OCR complete.
                    if let Ok(t) = Tag::new(ledger_core::tags::TAG_OCR_COMPLETE) {
                        record.add_tag(t);
                    }
                }
            } else {
                return Err(ToolError::InvalidInput(
                    "extract_with_llm requested but LLM backend is not configured".into(),
                ));
            }
        }
        #[cfg(not(feature = "llm"))]
        if request.extract_with_llm {
            return Err(ToolError::InvalidInput(
                "extract_with_llm requested but this build does not include the llm feature".into(),
            ));
        }

        // Write filesystem sidecar.
        let fs_meta = FsMetadata {
            doc_id: doc_id.clone(),
            tags: record.tags.iter().map(|t| t.as_str().to_string()).collect(),
            status: "indexed".into(),
            indexed_at: Some(chrono::Utc::now().to_rfc3339()),
            ..Default::default()
        };
        if let Err(e) = SidecarBackend.write(path, &fs_meta) {
            tracing::warn!(path = %path.display(), err = %e, "failed to write fs metadata sidecar");
        }

        let tags_out: Vec<String> = record.tags.iter().map(|t| t.as_str().to_string()).collect();
        let mut registry = self
            .document_registry
            .lock()
            .map_err(|_| ToolError::Internal("document registry lock poisoned".into()))?;
        registry.insert(doc_id.clone(), record);
        save_document_registry(self.workbook_path(), &registry)?;

        Ok(IngestImageResponse {
            doc_id,
            file_name: path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string(),
            doc_type: doc_type_str,
            tags: tags_out,
            llm_extracted,
        })
    }

    /// Apply tags to an existing document in the registry.
    pub fn apply_tags_tool(
        &self,
        request: ApplyTagsRequest,
    ) -> Result<ApplyTagsResponse, ToolError> {
        let (new_tags, _) = parse_tags(&request.tags);
        let mut registry = self
            .document_registry
            .lock()
            .map_err(|_| ToolError::Internal("document registry lock poisoned".into()))?;

        // Accept either a doc_id or a file path as doc_ref.
        let doc_id = if registry.contains_key(&request.doc_ref) {
            request.doc_ref.clone()
        } else {
            // Try to match by file_name.
            registry
                .iter()
                .find(|(_, r)| r.file_path == request.doc_ref || r.file_name == request.doc_ref)
                .map(|(id, _)| id.clone())
                .ok_or_else(|| {
                    ToolError::InvalidInput(format!("document not found: {}", request.doc_ref))
                })?
        };

        let record = registry
            .get_mut(&doc_id)
            .ok_or_else(|| ToolError::Internal("registry inconsistency".into()))?;

        for tag in &new_tags {
            record.add_tag(tag.clone());
        }

        if request.sync_fs {
            let fs_meta = FsMetadata {
                doc_id: doc_id.clone(),
                tags: record.tags.iter().map(|t| t.as_str().to_string()).collect(),
                status: format!("{:?}", record.status).to_ascii_lowercase(),
                ..Default::default()
            };
            let _ = SidecarBackend.write(std::path::Path::new(&record.file_path), &fs_meta);
        }

        let tags_out: Vec<String> = record.tags.iter().map(|t| t.as_str().to_string()).collect();
        let _ = save_document_registry(self.workbook_path(), &registry);

        Ok(ApplyTagsResponse {
            doc_id,
            tags: tags_out,
        })
    }

    /// Remove tags from a document.
    pub fn remove_tags_tool(
        &self,
        request: ApplyTagsRequest, // same shape; reuse
    ) -> Result<ApplyTagsResponse, ToolError> {
        let mut registry = self
            .document_registry
            .lock()
            .map_err(|_| ToolError::Internal("document registry lock poisoned".into()))?;

        let doc_id = if registry.contains_key(&request.doc_ref) {
            request.doc_ref.clone()
        } else {
            registry
                .iter()
                .find(|(_, r)| r.file_path == request.doc_ref || r.file_name == request.doc_ref)
                .map(|(id, _)| id.clone())
                .ok_or_else(|| {
                    ToolError::InvalidInput(format!("document not found: {}", request.doc_ref))
                })?
        };

        let record = registry
            .get_mut(&doc_id)
            .ok_or_else(|| ToolError::Internal("registry inconsistency".into()))?;

        for raw in &request.tags {
            record.remove_tag(raw);
        }

        if request.sync_fs {
            let fs_meta = FsMetadata {
                doc_id: doc_id.clone(),
                tags: record.tags.iter().map(|t| t.as_str().to_string()).collect(),
                status: format!("{:?}", record.status).to_ascii_lowercase(),
                ..Default::default()
            };
            let _ = SidecarBackend.write(std::path::Path::new(&record.file_path), &fs_meta);
        }

        let tags_out: Vec<String> = record.tags.iter().map(|t| t.as_str().to_string()).collect();
        save_document_registry(self.workbook_path(), &registry)?;

        Ok(ApplyTagsResponse {
            doc_id,
            tags: tags_out,
        })
    }

    /// List documents matching all the given tags.
    pub fn list_tagged_tool(
        &self,
        request: ListTaggedRequest,
    ) -> Result<ListTaggedResponse, ToolError> {
        let filter_tags: Vec<String> = request
            .tags
            .iter()
            .map(|t| Tag::normalize(t).as_str().to_string())
            .collect();

        let registry = self
            .document_registry
            .lock()
            .map_err(|_| ToolError::Internal("document registry lock poisoned".into()))?;

        let documents: Vec<DocumentSummary> = registry
            .values()
            .filter(|r| {
                let doc_tags: Vec<&str> = r.tags.iter().map(|t| t.as_str()).collect();
                filter_tags.iter().all(|ft| doc_tags.contains(&ft.as_str()))
            })
            .filter(|r| {
                request.doc_type.as_deref().is_none_or(|dt| {
                    format!("{:?}", r.doc_type)
                        .to_ascii_lowercase()
                        .contains(dt)
                })
            })
            .filter(|r| {
                // If a directory filter was provided, only include records under that directory.
                request
                    .directory
                    .as_deref()
                    .is_none_or(|dir| std::path::Path::new(&r.file_path).starts_with(dir))
            })
            .map(|r| DocumentSummary {
                doc_id: r.doc_id.clone(),
                file_name: r.file_name.clone(),
                doc_type: format!("{:?}", r.doc_type),
                tags: r.tags.iter().map(|t| t.as_str().to_string()).collect(),
                status: format!("{:?}", r.status).to_ascii_lowercase(),
            })
            .collect();

        Ok(ListTaggedResponse { documents })
    }

    /// Scan a directory for sidecar metadata and sync discovered docs into the registry.
    pub fn sync_fs_metadata_tool(
        &self,
        request: SyncFsMetadataRequest,
    ) -> Result<SyncFsMetadataResponse, ToolError> {
        // Constrain to workbook_path.parent() to prevent arbitrary directory traversal.
        let allowed_base = self
            .workbook_path()
            .parent()
            .ok_or_else(|| {
                ToolError::InvalidInput("workbook_path must have a parent directory".to_string())
            })?
            .to_path_buf();

        let scan_dir = &request.directory;
        if scan_dir
            .components()
            .any(|c| c == std::path::Component::ParentDir)
        {
            return Err(ToolError::InvalidInput(
                "directory contains path traversal components".into(),
            ));
        }
        let resolved_dir = if scan_dir.is_absolute() {
            if !scan_dir.starts_with(&allowed_base) {
                return Err(ToolError::InvalidInput(format!(
                    "directory '{}' resolves outside the allowed base",
                    scan_dir.display()
                )));
            }
            scan_dir.clone()
        } else {
            allowed_base.join(scan_dir)
        };

        let found = ledger_core::fs_meta::scan_directory(&resolved_dir, request.recursive)
            .map_err(|e| ToolError::Internal(e.to_string()))?;

        let files_scanned = found.len();
        let mut files_synced = 0usize;

        let mut registry = self
            .document_registry
            .lock()
            .map_err(|_| ToolError::Internal("document registry lock poisoned".into()))?;

        for (path, meta) in found {
            if meta.doc_id.is_empty() {
                continue;
            }
            let doc_type = DocType::from_path(&path);
            let (tags, _) = parse_tags(&meta.tags);
            let mut record = DocumentRecord::new(
                meta.doc_id.clone(),
                path.to_string_lossy().to_string(),
                doc_type,
            );
            record.tags = tags;
            record.status = DocumentStatus::Indexed;
            registry.insert(meta.doc_id.clone(), record);
            files_synced += 1;
        }

        save_document_registry(self.workbook_path(), &registry)?;

        Ok(SyncFsMetadataResponse {
            files_scanned,
            files_synced,
        })
    }

    /// Propose (or apply) a normalized `VENDOR--ACCOUNT--YYYY-MM--DOCTYPE.ext` filename.
    pub fn normalize_filename_tool(
        &self,
        request: NormalizeFilenameRequest,
    ) -> Result<NormalizeFilenameResponse, ToolError> {
        // Sanitize each component to remove path separators and traversal sequences.
        fn sanitize_component(s: &str) -> Result<String, ToolError> {
            let trimmed = s.trim();
            // Reject any path separator characters or ParentDir components.
            if trimmed.contains('/') || trimmed.contains('\\') {
                return Err(ToolError::InvalidInput(format!(
                    "filename component '{trimmed}' contains path separator characters"
                )));
            }
            if std::path::Path::new(trimmed)
                .components()
                .any(|c| c == std::path::Component::ParentDir)
            {
                return Err(ToolError::InvalidInput(format!(
                    "filename component '{trimmed}' contains path traversal sequences"
                )));
            }
            Ok(trimmed.to_ascii_uppercase())
        }

        let vendor = sanitize_component(&request.vendor)?;
        let account = sanitize_component(&request.account)?;
        let year_month = sanitize_component(&request.year_month)?;
        let doc_type_part = sanitize_component(&request.doc_type)?;

        // Constrain file_path to workbook_path.parent().
        let allowed_base = self
            .workbook_path()
            .parent()
            .ok_or_else(|| {
                ToolError::InvalidInput("workbook_path must have a parent directory".to_string())
            })?
            .to_path_buf();

        let raw_path = std::path::Path::new(&request.file_path);
        let resolved_path = if raw_path.is_absolute() {
            if raw_path
                .components()
                .any(|c| c == std::path::Component::ParentDir)
            {
                return Err(ToolError::InvalidInput(format!(
                    "file_path '{}' contains path traversal components",
                    request.file_path
                )));
            }
            if !raw_path.starts_with(&allowed_base) {
                return Err(ToolError::InvalidInput(format!(
                    "file_path '{}' resolves outside the allowed directory",
                    request.file_path
                )));
            }
            raw_path.to_path_buf()
        } else {
            if raw_path
                .components()
                .any(|c| c == std::path::Component::ParentDir)
            {
                return Err(ToolError::InvalidInput(format!(
                    "file_path '{}' contains path traversal components",
                    request.file_path
                )));
            }
            allowed_base.join(raw_path)
        };
        let path = resolved_path.as_path();

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("pdf");
        let original_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();

        let proposed_name = format!(
            "{}--{}--{}--{}.{}",
            vendor, account, year_month, doc_type_part, ext
        );

        let mut renamed = false;
        if request.apply && path.exists() {
            let new_path = path.with_file_name(&proposed_name);
            std::fs::rename(path, &new_path).map_err(|e| ToolError::Internal(e.to_string()))?;
            renamed = true;
        }

        Ok(NormalizeFilenameResponse {
            proposed_name,
            original_name,
            renamed,
        })
    }

    // ── Xero tool methods (delegated to XeroService) ──────────────────────────

    #[cfg(feature = "xero")]
    pub fn xero_get_auth_url(&self) -> Result<String, ToolError> {
        self.xero.get_auth_url()
    }

    #[cfg(feature = "xero")]
    pub fn xero_exchange_code(
        &self,
        code: String,
        state: String,
    ) -> Result<serde_json::Value, ToolError> {
        self.xero.exchange_code(code, state)
    }

    #[cfg(feature = "xero")]
    pub fn xero_fetch_contacts(
        &self,
        search: Option<&str>,
    ) -> Result<serde_json::Value, ToolError> {
        let refs = self.xero.fetch_contacts(search)?;
        serde_json::to_value(&refs).map_err(|e| ToolError::Internal(e.to_string()))
    }

    #[cfg(feature = "xero")]
    pub fn xero_fetch_accounts(&self) -> Result<serde_json::Value, ToolError> {
        let refs = self.xero.fetch_accounts()?;
        serde_json::to_value(&refs).map_err(|e| ToolError::Internal(e.to_string()))
    }

    #[cfg(feature = "xero")]
    pub fn xero_fetch_bank_accounts(&self) -> Result<serde_json::Value, ToolError> {
        let refs = self.xero.fetch_bank_accounts()?;
        serde_json::to_value(&refs).map_err(|e| ToolError::Internal(e.to_string()))
    }

    #[cfg(feature = "xero")]
    pub fn xero_fetch_invoices(
        &self,
        status: Option<&str>,
    ) -> Result<serde_json::Value, ToolError> {
        self.xero.fetch_invoices(status)
    }

    #[cfg(feature = "xero")]
    pub fn xero_link_entity(
        &self,
        local_id: String,
        xero_entity_type: String,
        xero_id: String,
        display_name: String,
        ontology_path: Option<std::path::PathBuf>,
    ) -> Result<serde_json::Value, ToolError> {
        use ledger_core::document::XeroEntityType;
        let entity_type = match xero_entity_type.as_str() {
            "contact" => XeroEntityType::Contact,
            "bank_account" => XeroEntityType::BankAccount,
            "account" => XeroEntityType::Account,
            "invoice" => XeroEntityType::Invoice,
            "bank_transaction" => XeroEntityType::BankTransaction,
            other => {
                return Err(ToolError::InvalidInput(format!(
                    "unknown xero entity type: {other}"
                )))
            }
        };

        let link = XeroLink {
            entity_type,
            xero_id: xero_id.clone(),
            display_name: display_name.clone(),
            linked_at: chrono::Utc::now().to_rfc3339(),
        };

        let mut registry = self
            .document_registry
            .lock()
            .map_err(|_| ToolError::Internal("document registry lock poisoned".into()))?;

        if let Some(record) = registry.get_mut(&local_id) {
            record.add_xero_link(link.clone());
            // Add #xero-linked tag.
            if let Ok(t) = Tag::new(ledger_core::tags::TAG_XERO_LINKED) {
                record.add_tag(t);
            }
            let _ = save_document_registry(self.workbook_path(), &registry);
        }

        // Optionally wire into ontology.
        if let Some(ont_path) = ontology_path {
            drop(registry); // release lock before ontology I/O
            let mut store = OntologyStore::load(&ont_path).unwrap_or_default();
            let kind = match xero_entity_type.as_str() {
                "contact" => OntologyEntityKind::XeroContact,
                "bank_account" => OntologyEntityKind::XeroBankAccount,
                "invoice" => OntologyEntityKind::XeroInvoice,
                _ => OntologyEntityKind::Account,
            };
            let mut attrs = std::collections::BTreeMap::new();
            attrs.insert("xero_id".into(), xero_id.clone());
            attrs.insert("display_name".into(), display_name.clone());
            attrs.insert("local_id".into(), local_id.clone());
            let _ = store.upsert_entities(vec![OntologyEntityInput { kind, attrs }]);
            let _ = store.persist(&ont_path);
        }

        Ok(serde_json::json!({
            "linked": true,
            "local_id": local_id,
            "xero_entity_type": xero_entity_type,
            "xero_id": xero_id,
            "display_name": display_name,
        }))
    }

    #[cfg(feature = "xero")]
    pub fn xero_sync_catalog(
        &self,
        ontology_path: std::path::PathBuf,
    ) -> Result<serde_json::Value, ToolError> {
        let mut store = OntologyStore::load(&ontology_path).unwrap_or_default();
        self.xero.sync_catalog(&mut store, &ontology_path)
    }
}

// ── Feature-gated stub methods (no-op when features are disabled) ─────────────

#[cfg(not(feature = "xero"))]
impl TurboLedgerService {
    pub fn xero_get_auth_url(&self) -> Result<String, ToolError> {
        Err(ToolError::Internal(
            "ledgerr-mcp built without 'xero' feature".into(),
        ))
    }

    pub fn xero_exchange_code(
        &self,
        _code: String,
        _state: String,
    ) -> Result<serde_json::Value, ToolError> {
        Err(ToolError::Internal(
            "ledgerr-mcp built without 'xero' feature".into(),
        ))
    }

    pub fn xero_fetch_contacts(
        &self,
        _search: Option<&str>,
    ) -> Result<serde_json::Value, ToolError> {
        Err(ToolError::Internal(
            "ledgerr-mcp built without 'xero' feature".into(),
        ))
    }

    pub fn xero_fetch_accounts(&self) -> Result<serde_json::Value, ToolError> {
        Err(ToolError::Internal(
            "ledgerr-mcp built without 'xero' feature".into(),
        ))
    }

    pub fn xero_fetch_bank_accounts(&self) -> Result<serde_json::Value, ToolError> {
        Err(ToolError::Internal(
            "ledgerr-mcp built without 'xero' feature".into(),
        ))
    }

    pub fn xero_fetch_invoices(
        &self,
        _status: Option<&str>,
    ) -> Result<serde_json::Value, ToolError> {
        Err(ToolError::Internal(
            "ledgerr-mcp built without 'xero' feature".into(),
        ))
    }

    pub fn xero_link_entity(
        &self,
        _local_id: String,
        _xero_entity_type: String,
        _xero_id: String,
        _display_name: String,
        _ontology_path: Option<std::path::PathBuf>,
    ) -> Result<serde_json::Value, ToolError> {
        Err(ToolError::Internal(
            "ledgerr-mcp built without 'xero' feature".into(),
        ))
    }

    pub fn xero_sync_catalog(
        &self,
        _ontology_path: std::path::PathBuf,
    ) -> Result<serde_json::Value, ToolError> {
        Err(ToolError::Internal(
            "ledgerr-mcp built without 'xero' feature".into(),
        ))
    }
}
