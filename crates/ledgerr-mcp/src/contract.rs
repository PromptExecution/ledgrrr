//! Canonical MCP transport contract for the published `ledgerr_*` surface.
//!
//! Design decision, captured here intentionally because it affects how future
//! agents extend the MCP boundary:
//! - Rust code is the only source of truth for the published MCP surface.
//! - `tools/list` schemas, operator docs, and runnable examples are generated
//!   from this module.
//! - Drift between parser, schema, and docs is a bug and should fail tests.
//!
//! Hidden compatibility aliases may continue to parse legacy shapes elsewhere,
//! but the advertised tool catalog must stay defined here.

use std::collections::BTreeMap;
use std::path::PathBuf;

use schemars::{
    schema::{InstanceType, Metadata, RootSchema, Schema, SchemaObject, SingleOrVec},
    schema_for, JsonSchema,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::AuditEntryResponse;
use crate::ToolError;

pub const DOCUMENTS_TOOL: &str = "ledgerr_documents";
pub const REVIEW_TOOL: &str = "ledgerr_review";
pub const RECONCILIATION_TOOL: &str = "ledgerr_reconciliation";
pub const WORKFLOW_TOOL: &str = "ledgerr_workflow";
pub const AUDIT_TOOL: &str = "ledgerr_audit";
pub const TAX_TOOL: &str = "ledgerr_tax";
pub const ONTOLOGY_TOOL: &str = "ledgerr_ontology";
pub const XERO_TOOL: &str = "ledgerr_xero";
pub const FOCUS_TOOL: &str = "ledgerr_focus";
pub const EVIDENCE_TOOL: &str = "ledgerr_evidence";
pub const CALENDAR_TOOL: &str = "list_calendar_events";
pub const SHAPE_TOOL: &str = "get_document_shape";
pub const SCHEMA_TOOL: &str = "ledgerr_schema";
pub const MANIFEST_TOOL: &str = "ledgerr_manifest";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolContractSpec {
    pub name: &'static str,
    pub purpose: &'static str,
    pub actions: &'static [&'static str],
}

pub const TOOL_REGISTRY: &[&str] = &[
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
    CALENDAR_TOOL,
    SHAPE_TOOL,
    SCHEMA_TOOL,
    MANIFEST_TOOL,
];

pub const PUBLISHED_TOOLS: [ToolContractSpec; 12] = [
    ToolContractSpec {
        name: DOCUMENTS_TOOL,
        purpose: "document intake (PDF, image, CSV), tagging, filesystem metadata sync",
        actions: &[
            "list_accounts",
            "pipeline_status",
            "validate_filename",
            "ingest_pdf",
            "ingest_image",
            "ingest_rows",
            "get_raw_context",
            "document_inventory",
            "apply_tags",
            "remove_tags",
            "list_tagged",
            "sync_fs_metadata",
            "normalize_filename",
        ],
    },
    ToolContractSpec {
        name: REVIEW_TOOL,
        purpose: "classification and human-review workflows",
        actions: &[
            "run_rule",
            "classify_ingested",
            "query_flags",
            "classify_transaction",
            "reconcile_excel_classification",
            "query_transactions",
            "batch_classify",
            "bulk_resolve_flags",
            "apply_mapping_bulk",
            "fetch_work_queue",
        ],
    },
    ToolContractSpec {
        name: RECONCILIATION_TOOL,
        purpose: "staged totals/postings guardrails",
        actions: &["validate", "reconcile", "commit"],
    },
    ToolContractSpec {
        name: WORKFLOW_TOOL,
        purpose: "lifecycle/HSM orchestration plus relocated plugin ops",
        actions: &["status", "transition", "resume", "plugin_info"],
    },
    ToolContractSpec {
        name: AUDIT_TOOL,
        purpose: "append-only event and audit-log views",
        actions: &["event_history", "event_replay", "query_audit_log"],
    },
    ToolContractSpec {
        name: TAX_TOOL,
        purpose: "tax summaries, evidence, ambiguity review, workbook export",
        actions: &[
            "assist",
            "evidence_chain",
            "ambiguity_review",
            "schedule_summary",
            "export_workbook",
        ],
    },
    ToolContractSpec {
        name: ONTOLOGY_TOOL,
        purpose: "ontology query/export/write operations",
        actions: &[
            "query_path",
            "export_snapshot",
            "upsert_entities",
            "upsert_edges",
        ],
    },
    ToolContractSpec {
        name: XERO_TOOL,
        purpose: "Xero accounting integration: contacts, accounts, bank accounts, entity linking",
        actions: &[
            "get_auth_url",
            "exchange_code",
            "fetch_contacts",
            "search_contacts",
            "fetch_accounts",
            "fetch_bank_accounts",
            "fetch_invoices",
            "link_entity",
            "sync_catalog",
        ],
    },
    ToolContractSpec {
        name: FOCUS_TOOL,
        purpose: "FOCUS (FinOps Cost Usage Spec) v1.3 cost/usage records, FocusDelta comparison, experiment scoring",
        actions: &[
            "append_focus_record",
            "query_focus_summary",
            "compute_focus_delta",
            "experiment_score",
        ],
    },
    ToolContractSpec {
        name: EVIDENCE_TOOL,
        purpose: "evidence traceability: provenance gaps, transaction lineage, review badges, graph summary and node queries",
        actions: &[
            "provenance_gaps",
            "trace_tx",
            "summary",
            "list_nodes",
            "node_detail",
        ],
    },
    ToolContractSpec {
        name: SCHEMA_TOOL,
        purpose: "runtime schema extensibility: register, list, remove, and inspect custom entity kinds",
        actions: &[
            "list_kinds",
            "register_kind",
            "remove_kind",
            "get_kind",
        ],
    },
    ToolContractSpec {
        name: MANIFEST_TOOL,
        purpose: "returns the full canonical viz-manifest: mapping of type IDs to their canonical Rhai DSL source strings",
        actions: &["get_manifest"],
    },
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransportRow {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    pub date: String,
    pub amount: String,
    pub description: String,
    pub source_ref: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SampleTxInput {
    pub tx_id: String,
    pub account_id: String,
    pub date: String,
    pub amount: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Eq)]
#[serde(deny_unknown_fields)]
pub struct DateRange {
    pub start: String,
    pub end: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Eq)]
#[serde(deny_unknown_fields)]
pub struct AmountRange {
    pub min: String,
    pub max: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SortField {
    Date,
    Amount,
    Description,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Eq)]
#[serde(deny_unknown_fields)]
pub struct SortSpec {
    pub field: SortField,
    pub direction: SortDirection,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Eq)]
#[serde(deny_unknown_fields)]
pub struct PaginationSpec {
    pub limit: u32,
    #[serde(default)]
    pub offset: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Eq)]
#[serde(deny_unknown_fields)]
pub struct TransactionFilters {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_range: Option<DateRange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount_range: Option<AmountRange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description_contains: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Eq)]
#[serde(deny_unknown_fields)]
pub struct TransactionRow {
    pub tx_id: String,
    pub account_id: String,
    pub date: String,
    pub amount: String,
    pub description: String,
    pub source_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReconciliationInput {
    pub source_total: String,
    pub extracted_total: String,
    pub posting_amounts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum FlexibleF64 {
    Number(f64),
    String(String),
}

impl FlexibleF64 {
    pub fn as_json(&self) -> Value {
        match self {
            Self::Number(value) => json!(value),
            Self::String(value) => json!(value),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginInfoSubcommand(pub String);

impl JsonSchema for PluginInfoSubcommand {
    fn schema_name() -> String {
        "PluginInfoSubcommand".to_string()
    }

    fn json_schema(_gen: &mut schemars::gen::SchemaGenerator) -> Schema {
        Schema::Object(SchemaObject {
            instance_type: Some(SingleOrVec::Single(Box::new(InstanceType::String))),
            metadata: Some(Box::new(Metadata {
                description: Some("Subcommand string. Known values: check, upgrade, cleanup. Unknown strings fall through to the default check behavior.".to_string()),
                ..Metadata::default()
            })),
            ..SchemaObject::default()
        })
    }

    fn is_referenceable() -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkflowPluginInfoInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subcommand: Option<PluginInfoSubcommand>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ReviewStatusInput {
    #[serde(rename = "open")]
    Open,
    #[serde(rename = "resolved")]
    Resolved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ScheduleInput {
    #[serde(rename = "ScheduleC")]
    ScheduleC,
    #[serde(rename = "ScheduleD")]
    ScheduleD,
    #[serde(rename = "ScheduleE")]
    ScheduleE,
    #[serde(rename = "Fbar")]
    Fbar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum OntologyEntityKindInput {
    Document,
    Account,
    Institution,
    Transaction,
    TaxCategory,
    EvidenceReference,
    XeroContact,
    XeroBankAccount,
    XeroInvoice,
    WorkflowTag,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OntologyEntityInput {
    pub kind: OntologyEntityKindInput,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub properties: Option<BTreeMap<String, Value>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OntologyEdgeInput {
    pub from_id: String,
    pub to_id: String,
    pub relation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<BTreeMap<String, Value>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum DocumentsArgs {
    ListAccounts,
    PipelineStatus,
    ValidateFilename {
        file_name: String,
    },
    GetRawContext {
        rkyv_ref: PathBuf,
    },
    IngestPdf {
        pdf_path: String,
        journal_path: PathBuf,
        workbook_path: PathBuf,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ontology_path: Option<PathBuf>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        raw_context_bytes: Option<Vec<u8>>,
        #[serde(default)]
        extracted_rows: Vec<TransportRow>,
    },
    IngestRows {
        journal_path: PathBuf,
        workbook_path: PathBuf,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ontology_path: Option<PathBuf>,
        rows: Vec<TransportRow>,
    },
    IngestImage {
        /// Absolute path to the image file (JPEG, PNG, WEBP, TIFF, GIF).
        image_path: String,
        /// Override doc type: "receipt", "invoice", "bank_statement", or "other".
        #[serde(default, skip_serializing_if = "Option::is_none")]
        doc_type: Option<String>,
        /// Pre-applied workflow tags (e.g. ["#receipt", "#pending-review"]).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tags: Vec<String>,
        /// If set, run LLM extraction and store result in document metadata.
        #[serde(default)]
        extract_with_llm: bool,
    },
    ApplyTags {
        /// Document ID (blake3 hash) or file path.
        doc_ref: String,
        tags: Vec<String>,
        /// If true, also write tags to filesystem metadata alongside the file.
        #[serde(default)]
        sync_fs: bool,
    },
    RemoveTags {
        doc_ref: String,
        tags: Vec<String>,
        #[serde(default)]
        sync_fs: bool,
    },
    ListTagged {
        /// Filter to documents that have ALL of these tags.
        tags: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        doc_type: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        directory: Option<PathBuf>,
    },
    SyncFsMetadata {
        /// Root directory to scan for sidecar metadata files.
        directory: PathBuf,
        #[serde(default)]
        recursive: bool,
    },
    NormalizeFilename {
        /// Current file path.
        file_path: String,
        /// Desired vendor slug (e.g. "AMEX").
        vendor: String,
        /// Account ID (e.g. "BH-CARD").
        account: String,
        /// Statement month as YYYY-MM.
        year_month: String,
        /// Document type suffix (e.g. "statement", "receipt").
        doc_type: String,
        /// If true, actually rename the file; otherwise return the proposed name only.
        #[serde(default)]
        apply: bool,
    },
    DocumentInventory {
        directory: PathBuf,
        #[serde(default)]
        recursive: bool,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        statuses: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum ReviewArgs {
    RunRule {
        rule_file: PathBuf,
        sample_tx: SampleTxInput,
    },
    ClassifyIngested {
        rule_file: PathBuf,
        review_threshold: FlexibleF64,
    },
    QueryFlags {
        year: i32,
        status: ReviewStatusInput,
    },
    ClassifyTransaction {
        tx_id: String,
        category: String,
        confidence: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
        actor: String,
    },
    ReconcileExcelClassification {
        tx_id: String,
        category: String,
        confidence: String,
        actor: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
    QueryTransactions {
        filters: TransactionFilters,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sort: Option<SortSpec>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pagination: Option<PaginationSpec>,
    },
    BatchClassify {
        request: BatchClassifyRequest,
    },
    BatchResolveFlags {
        request: BatchResolveFlagsRequest,
    },
    ApplyMappingBulk {
        request: ApplyMappingBulkRequest,
    },
    FetchQueue {
        request: FetchQueueRequest,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum ReconciliationArgs {
    Validate {
        source_total: String,
        extracted_total: String,
        posting_amounts: Vec<String>,
    },
    Reconcile {
        source_total: String,
        extracted_total: String,
        posting_amounts: Vec<String>,
    },
    Commit {
        source_total: String,
        extracted_total: String,
        posting_amounts: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkflowArgs {
    Status,
    Transition {
        target_state: String,
        target_substate: String,
    },
    Resume {
        state_marker: String,
    },
    PluginInfo {
        #[serde(flatten)]
        payload: WorkflowPluginInfoInput,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum AuditArgs {
    EventHistory {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tx_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        document_ref: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        time_start: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        time_end: Option<String>,
    },
    EventReplay {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tx_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        document_ref: Option<String>,
    },
    QueryAuditLog,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum TaxArgs {
    Assist {
        ontology_path: PathBuf,
        from_entity_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_depth: Option<usize>,
        reconciliation: ReconciliationInput,
    },
    EvidenceChain {
        ontology_path: PathBuf,
        from_entity_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tx_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        document_ref: Option<String>,
    },
    AmbiguityReview {
        ontology_path: PathBuf,
        from_entity_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_depth: Option<usize>,
        reconciliation: ReconciliationInput,
    },
    ScheduleSummary {
        year: i32,
        schedule: ScheduleInput,
    },
    ExportWorkbook {
        workbook_path: PathBuf,
    },
    // ── Tax-Lawyer Platform actions (gh#516) ──────────────────────────────
    /// Check AU R&D activity eligibility under ITAA 1997 s.355-100.
    AuRdCheckEligibility {
        lei: String,
        activity_id: String,
        activity_name: String,
        has_hypothesis: bool,
        has_technical_uncertainty: bool,
        is_systematic: bool,
        is_core: bool,
    },
    /// Classify AU R&D expenditure under s.355-305.
    AuRdClassifyExpenditure {
        lei: String,
        tx_id: String,
        category: String,
        amount_aud: String,
    },
    /// Calculate AU R&D Tax Incentive offset for an income year.
    AuRdCalculateOffset {
        lei: String,
        total_eligible_aud: String,
        is_refundable: bool,
    },
    /// Apply the IRC § 41(d) 4-part test for a US R&D activity.
    UsRdcFourPartTestCheck {
        lei: String,
        activity_id: String,
        activity_name: String,
        technical_in_nature: bool,
        permits_experimentation: bool,
        technological_uncertainty: bool,
        systematic_process: bool,
    },
    /// Check crypto transaction cost basis compliance for AU or US jurisdiction.
    CryptoCostBasisCheck {
        lei: String,
        tx_hash: String,
        tx_type: String,
        gross_proceeds: String,
        cost_basis: String,
        date: String,
        acquisition_date: Option<String>,
        jurisdiction: String,
        currency: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum OntologyArgs {
    QueryPath {
        ontology_path: PathBuf,
        from_entity_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_depth: Option<usize>,
    },
    ExportSnapshot {
        ontology_path: PathBuf,
    },
    UpsertEntities {
        ontology_path: PathBuf,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        schema_store_path: Option<PathBuf>,
        entities: Vec<OntologyEntityInput>,
    },
    UpsertEdges {
        ontology_path: PathBuf,
        edges: Vec<OntologyEdgeInput>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum XeroArgs {
    GetAuthUrl,
    ExchangeCode {
        code: String,
        state: String,
    },
    FetchContacts,
    SearchContacts {
        query: String,
    },
    FetchAccounts,
    FetchBankAccounts,
    FetchInvoices {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<String>,
    },
    LinkEntity {
        /// Local document ID or transaction ID to link.
        local_id: String,
        /// "contact", "bank_account", "account", "invoice"
        xero_entity_type: String,
        xero_id: String,
        display_name: String,
        /// If set, also write link into the ontology at this path.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ontology_path: Option<PathBuf>,
    },
    SyncCatalog {
        /// Write discovered Xero entities into the ontology store.
        ontology_path: PathBuf,
    },
}

pub fn parse_documents(arguments: &Value) -> Result<DocumentsArgs, ToolError> {
    parse_args(arguments)
}

pub fn parse_review(arguments: &Value) -> Result<ReviewArgs, ToolError> {
    parse_args(arguments)
}

pub fn parse_reconciliation(arguments: &Value) -> Result<ReconciliationArgs, ToolError> {
    parse_args(arguments)
}

pub fn parse_workflow(arguments: &Value) -> Result<WorkflowArgs, ToolError> {
    parse_args(arguments)
}

pub fn parse_audit(arguments: &Value) -> Result<AuditArgs, ToolError> {
    parse_args(arguments)
}

pub fn parse_tax(arguments: &Value) -> Result<TaxArgs, ToolError> {
    parse_args(arguments)
}

pub fn parse_ontology(arguments: &Value) -> Result<OntologyArgs, ToolError> {
    parse_args(arguments)
}

pub fn parse_xero(arguments: &Value) -> Result<XeroArgs, ToolError> {
    parse_args(arguments)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", deny_unknown_fields)]
pub enum FocusArgs {
    #[serde(rename = "append_focus_record")]
    AppendFocusRecord {
        billing_account_id: String,
        service_name: String,
        billed_cost: f64,
        effective_cost: f64,
        experiment_id: Option<String>,
        variant: Option<String>,
        agent_id: Option<String>,
    },
    #[serde(rename = "query_focus_summary")]
    QueryFocusSummary,
    #[serde(rename = "compute_focus_delta")]
    ComputeFocusDelta {
        experiment_id: String,
        control_billed: f64,
        treatment_billed: f64,
    },
    #[serde(rename = "experiment_score")]
    ExperimentScore {
        experiment_id: String,
        /// Psychometric personality label (e.g. "analyst", "explorer", "guardian").
        personality: Option<String>,
        /// Experiment variant label (e.g. "control", "treatment").
        variant: Option<String>,
    },
}

pub fn parse_focus(arguments: &Value) -> Result<FocusArgs, ToolError> {
    parse_args(arguments)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", deny_unknown_fields)]
pub enum EvidenceArgs {
    #[serde(rename = "provenance_gaps")]
    ProvenanceGaps,
    #[serde(rename = "trace_tx")]
    TraceTx { tx_id: String },
    #[serde(rename = "summary")]
    Summary,
    #[serde(rename = "list_nodes")]
    ListNodes {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        node_type: Option<String>,
    },
    #[serde(rename = "node_detail")]
    NodeDetail { node_id: String },
}

pub fn parse_evidence(arguments: &Value) -> Result<EvidenceArgs, ToolError> {
    parse_args(arguments)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", deny_unknown_fields)]
pub enum SchemaArgs {
    #[serde(rename = "list_kinds")]
    ListKinds {
        /// Path to the schema store JSON file.
        schema_path: PathBuf,
    },
    #[serde(rename = "register_kind")]
    RegisterKind {
        /// Path to the schema store JSON file.
        schema_path: PathBuf,
        /// Name for the new custom kind (must not already exist as built-in or custom).
        name: String,
        /// Optional human-readable description.
        #[serde(default)]
        description: String,
        /// Optional key-value map for attribute type hints (e.g. {"amount": "decimal"}).
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        attrs_schema: BTreeMap<String, String>,
    },
    #[serde(rename = "remove_kind")]
    RemoveKind {
        /// Path to the schema store JSON file.
        schema_path: PathBuf,
        /// Name of the custom kind to remove.
        name: String,
    },
    #[serde(rename = "get_kind")]
    GetKind {
        /// Path to the schema store JSON file.
        schema_path: PathBuf,
        /// Name of the kind to inspect (built-in or custom).
        name: String,
    },
}

pub fn parse_schema(arguments: &Value) -> Result<SchemaArgs, ToolError> {
    parse_args(arguments)
}

fn parse_args<T>(arguments: &Value) -> Result<T, ToolError>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(arguments.clone())
        .map_err(|err| ToolError::InvalidInput(format!("invalid tool arguments: {err}")))
}

pub fn tool_input_schema(name: &str) -> Value {
    match name {
        DOCUMENTS_TOOL => root_schema_to_value(schema_for!(DocumentsArgs)),
        REVIEW_TOOL => root_schema_to_value(schema_for!(ReviewArgs)),
        RECONCILIATION_TOOL => root_schema_to_value(schema_for!(ReconciliationArgs)),
        WORKFLOW_TOOL => root_schema_to_value(schema_for!(WorkflowArgs)),
        AUDIT_TOOL => root_schema_to_value(schema_for!(AuditArgs)),
        TAX_TOOL => root_schema_to_value(schema_for!(TaxArgs)),
        ONTOLOGY_TOOL => root_schema_to_value(schema_for!(OntologyArgs)),
        XERO_TOOL => root_schema_to_value(schema_for!(XeroArgs)),
        FOCUS_TOOL => root_schema_to_value(schema_for!(FocusArgs)),
        EVIDENCE_TOOL => root_schema_to_value(schema_for!(EvidenceArgs)),
        SCHEMA_TOOL => root_schema_to_value(schema_for!(SchemaArgs)),
        _ => json!({ "type": "object" }),
    }
}

/// Convert a `schemars` `RootSchema` to a `serde_json::Value` compatible with
/// the Claude API `input_schema` constraint.
///
/// # DO NOT SIMPLIFY — read before touching
///
/// The Claude API (Anthropic) hard-rejects any tool whose `input_schema` JSON
/// contains `"oneOf"`, `"anyOf"`, or `"allOf"` **at the top level**, returning
/// HTTP 400: `input_schema does not support oneOf, allOf, or anyOf at the top level`.
/// This is an API constraint, not a style preference. Ref: Anthropic tool-use docs.
///
/// `schemars` 0.8 generates a top-level `"oneOf"` for **all** Rust enums —
/// including those with `#[serde(tag = "action")]`. Each variant becomes one
/// branch. This is valid JSON Schema but is rejected by the Claude API at root.
///
/// # Why the previous `or_insert_with("type":"object")` was WRONG
///
/// The previous attempt added `"type": "object"` alongside the top-level
/// `"oneOf"`, producing `{ "type": "object", "oneOf": [...] }`. The Claude API
/// still rejects this — the constraint fires on `"oneOf"` presence at root
/// regardless of whether `"type"` is also set.
///
/// # Correct approach: flatten to a discriminated-union object
///
/// All `*Args` enums carry `#[serde(tag = "action")]`. Schemars emits a
/// `"oneOf"` where every branch is an object with `properties.action.enum`
/// containing exactly one action name. `flatten_tagged_oneof_for_claude`
/// collapses this into a single flat object:
///   - `"action"`: string enum of all action names (required)
///   - all other per-variant properties merged in as optional fields
///
/// Per-variant field exclusivity is enforced at deserialization time by serde
/// (the `#[serde(tag)]` + `deny_unknown_fields` on each enum). The schema is
/// intentionally looser — the action discriminator is preserved and tool
/// descriptions carry per-action field documentation.
fn root_schema_to_value(schema: RootSchema) -> Value {
    let mut v = serde_json::to_value(schema).expect("schema serializes");
    flatten_tagged_oneof_for_claude(&mut v);
    v
}

/// Collapse a top-level schemars `oneOf` (emitted for `#[serde(tag = "action")]`
/// enums) into a flat discriminated-union object the Claude API accepts.
///
/// # Invariant
///
/// Called only for `*Args` enums. All variants must have a
/// `properties.action.enum` array containing exactly one string — the action
/// name for that variant.
///
/// # DO NOT REMOVE OR SHORT-CIRCUIT
///
/// The Claude API returns HTTP 400 for any tool whose `input_schema` contains
/// `"oneOf"`/`"anyOf"`/`"allOf"` at the JSON root. This function is the only
/// thing standing between a valid MCP handshake and a 400 that silently
/// corrupts the entire Claude session by poisoning tool schema loading.
fn flatten_tagged_oneof_for_claude(v: &mut Value) {
    let map = match v.as_object_mut() {
        Some(m) => m,
        None => return,
    };

    // All three composition keywords are rejected by Claude API at root.
    let composition = map
        .remove("oneOf")
        .or_else(|| map.remove("anyOf"))
        .or_else(|| map.remove("allOf"));

    let variants = match composition {
        Some(Value::Array(a)) => a,
        Some(other) => {
            // Unexpected shape — restore and leave schema unchanged rather than corrupt it.
            map.insert("oneOf".to_string(), other);
            return;
        }
        None => {
            // No composition at root; ensure type is set and return.
            map.entry("type".to_string())
                .or_insert_with(|| Value::String("object".to_string()));
            return;
        }
    };

    let mut action_values: Vec<Value> = Vec::new();
    let mut merged_props: serde_json::Map<String, Value> = serde_json::Map::new();

    for variant in &variants {
        let props = match variant.get("properties").and_then(|p| p.as_object()) {
            Some(p) => p,
            None => continue,
        };
        // Collect the action discriminator value for this variant.
        if let Some(action_enum) = props
            .get("action")
            .and_then(|a| a.get("enum"))
            .and_then(|e| e.as_array())
        {
            action_values.extend(action_enum.iter().cloned());
        }
        // Merge all non-action properties; first writer wins on collision.
        for (k, val) in props {
            if k != "action" {
                merged_props.entry(k.clone()).or_insert_with(|| val.clone());
            }
        }
    }

    // Build the flat action discriminator property.
    merged_props.insert(
        "action".to_string(),
        json!({ "type": "string", "enum": action_values }),
    );

    map.insert("type".to_string(), json!("object"));
    map.insert("required".to_string(), json!(["action"]));
    map.insert("properties".to_string(), Value::Object(merged_props));
    // Remove additionalProperties only for the merged union case: the per-variant
    // constraint ("no extra fields beyond this variant's props") is invalid on the
    // flattened object that contains ALL variants' properties. For plain struct
    // schemas (no variants merged), we leave additionalProperties intact.
    // Variants were merged iff action_values is non-empty.
    if !action_values.is_empty() {
        map.remove("additionalProperties");
    }
}

pub fn generated_capability_contract_markdown() -> String {
    let mut doc = String::new();
    doc.push_str("# MCP Capability Contract (Generated)\n\n");
    doc.push_str(
        "This file is generated from `crates/ledgerr-mcp/src/contract.rs`.\n\n\
Rust code is the only source of truth for the published MCP surface. If this file drifts from the contract module, tests should fail.\n\n",
    );
    doc.push_str(&format!(
        "The default catalog is intentionally small: {} top-level `ledgerr_*` tools. Each tool uses a required `action` field so the major capability families stay visible while related operations are grouped under one top-level command.\n\n",
        PUBLISHED_TOOLS.len()
    ));
    doc.push_str("## Published MCP Tools\n\n");
    doc.push_str("| Tool | Purpose | Common actions |\n|---|---|---|\n");
    for spec in PUBLISHED_TOOLS {
        doc.push_str(&format!(
            "| `{}` | {} | {} |\n",
            spec.name,
            spec.purpose,
            spec.actions
                .iter()
                .map(|action| format!("`{action}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    doc.push_str(
        "\nThe concrete parser, action enums, field aliases, and JSON Schemas all live in [crates/ledgerr-mcp/src/contract.rs](../crates/ledgerr-mcp/src/contract.rs).\n\n\
The transport adapter in [crates/ledgerr-mcp/src/mcp_adapter.rs](../crates/ledgerr-mcp/src/mcp_adapter.rs) consumes that contract rather than re-describing it by hand.\n\n",
    );
    doc.push_str(
        "## Compatibility\n\n\
The server still accepts older `l3dg3rr_*` and proxy-style call names as hidden compatibility aliases, but they are no longer advertised in `tools/list`. Agents should use the `ledgerr_*` tools above by default.\n\n",
    );
    doc.push_str(
        "## Internal Service API\n\n\
Canonical trait:\n[TurboLedgerTools in crates/ledgerr-mcp/src/lib.rs](../crates/ledgerr-mcp/src/lib.rs#L289)\n\n\
Important distinction:\n- The MCP surface is the published `ledgerr_*` catalog defined in Rust.\n- The internal service trait remains more granular and implementation-oriented.\n\n\
API layering:\n1. `ledgerr-mcp-server` (stdio transport)\n2. `contract` (published tool families, action enums, generated schema/doc artifacts)\n3. `mcp_adapter` (dispatch + envelope shaping)\n4. `TurboLedgerService` (domain logic, guardrails, state/event/HSM ops)\n5. `ledger-core` (ingest, filename validation, classification primitives)\n\n",
    );
    doc.push_str("## Example Flow\n\n");
    doc.push_str("### Step A: initialize and list tools\n\n");
    doc.push_str("```json\n");
    doc.push_str("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"clientInfo\":{\"name\":\"demo\",\"version\":\"0.1.0\"}}}\n");
    doc.push_str("{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\",\"params\":{}}\n");
    doc.push_str("{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\",\"params\":{}}\n");
    doc.push_str("```\n\n");
    doc.push_str("### Step B: ingest a PDF through `ledgerr_documents`\n\n```json\n");
    doc.push_str(&pretty_json(&documents_ingest_pdf_example()));
    doc.push_str("\n```\n\n");
    doc.push_str("### Step C: run reconciliation commit gate\n\n```json\n");
    doc.push_str(&pretty_json(&reconciliation_commit_example()));
    doc.push_str("\n```\n\n");
    doc.push_str("### Step D: inspect workflow status and audit replay\n\n```json\n");
    doc.push_str("{\"jsonrpc\":\"2.0\",\"id\":5,\"method\":\"tools/call\",\"params\":{\"name\":\"ledgerr_workflow\",\"arguments\":{\"action\":\"status\"}}}\n");
    doc.push_str("{\"jsonrpc\":\"2.0\",\"id\":6,\"method\":\"tools/call\",\"params\":{\"name\":\"ledgerr_audit\",\"arguments\":{\"action\":\"event_replay\",\"document_ref\":\"wf-2023-01.rkyv\"}}}\n");
    doc.push_str("```\n\n");
    doc.push_str("### Step E: ask for tax evidence\n\n```json\n");
    doc.push_str(&pretty_json(&tax_evidence_chain_example()));
    doc.push_str("\n```\n\n");
    doc.push_str(
        "## Current Gaps\n\n\
Open design/roadmap gaps are tracked in:\n\
- `#20` persistent state across restart\n\
- `#21` workbook export completeness\n\
- `#22` schema/doc drift elimination\n\
- `#23` document inventory/queue\n\
- `#24` unified work queue\n\
- `#25` batch review actions\n\
- `#26` transaction query + preflight/rule preview\n",
    );
    doc
}

pub fn generated_agent_runbook_markdown() -> String {
    format!(
        "# Agent MCP Runbook (Generated)\n\n\
This file is generated from `crates/ledgerr-mcp/src/contract.rs`.\n\n\
Agent workflows must use `initialize`, `notifications/initialized`, `tools/list`, and `tools/call` over stdio.\n\n\
## Runtime Model\n\n\
The default published surface is the `ledgerr_*` catalog generated from `PUBLISHED_TOOLS`:\n\n{}\n\
Each tool requires an `action` argument.\n\n\
## Bootstrap\n\n\
From repo root:\n\n```bash\ncargo build -p ledgerr-mcp --bin ledgerr-mcp-server\n```\n\n\
## Lifecycle\n\n\
Required order:\n\n\
1. `initialize`\n\
2. `notifications/initialized`\n\
3. `tools/list`\n\
4. `tools/call`\n\n\
## Basic Happy Path\n\n```json\n{}\n{}\n{}\n{}\n```\n\n\
## Troubleshooting / Spinning Wheels\n\n```json\n{}\n{}\n{}\n```\n\n\
Expected blocked outcomes:\n\n\
- invalid workflow resume returns `HsmResumeBlocked`\n\
- imbalanced reconciliation commit returns `ReconciliationBlocked`\n\
- invalid audit time range returns `EventHistoryBlocked`\n\n\
## Suggested Test Commands\n\n```bash\ncargo test -p ledgerr-mcp --test mcp_stdio_e2e -- --nocapture\ncargo test -p ledgerr-mcp --test plugin_info_mcp_e2e -- --nocapture\nbash scripts/mcp_cli_demo.sh\nbash scripts/mcp_e2e.sh\n```\n\n\
## Notes\n\n\
- Hidden compatibility aliases still exist for older `l3dg3rr_*` and proxy-style calls, but agents should not depend on them.\n\
- Use `docs/mcp-capability-contract.md` as the concise surface map.\n",
        PUBLISHED_TOOLS
            .iter()
            .map(|spec| format!("- `{}`\n", spec.name))
            .collect::<String>(),
        compact_tool_call(DOCUMENTS_TOOL, json!({"action":"pipeline_status"})),
        compact_tool_call(DOCUMENTS_TOOL, json!({"action":"list_accounts"})),
        compact_tool_call(
            DOCUMENTS_TOOL,
            json!({
                "action":"ingest_pdf",
                "pdf_path":"WF--BH-CHK--2023-01--statement.pdf",
                "journal_path":"/tmp/demo.beancount",
                "workbook_path":"/tmp/demo.xlsx",
                "raw_context_bytes":[99,116,120],
                "extracted_rows":[{
                    "account_id":"WF-BH-CHK",
                    "date":"2023-01-15",
                    "amount":"-42.11",
                    "description":"Coffee Shop",
                    "source_ref":"wf-2023-01.rkyv"
                }]
            })
        ),
        compact_tool_call(
            DOCUMENTS_TOOL,
            json!({"action":"get_raw_context","rkyv_ref":"wf-2023-01.rkyv"})
        ),
        compact_tool_call(
            WORKFLOW_TOOL,
            json!({"action":"resume","state_marker":"invalid-checkpoint"})
        ),
        compact_tool_call(
            RECONCILIATION_TOOL,
            json!({"action":"commit","source_total":"100.00","extracted_total":"95.00","posting_amounts":["-95.00","95.00"]})
        ),
        compact_tool_call(
            AUDIT_TOOL,
            json!({"action":"event_history","time_start":"2026-12-31","time_end":"2026-01-01"})
        ),
    )
}

pub fn generated_mcp_cli_demo_script() -> String {
    "#!/usr/bin/env bash\nset -euo pipefail\n\n\
DEMO_ROOT=\"${DEMO_ROOT:-/tmp/l3dg3rr-mcp-demo-$$}\"\n\
JOURNAL_PATH=\"${JOURNAL_PATH:-$DEMO_ROOT/demo.beancount}\"\n\
WORKBOOK_PATH=\"${WORKBOOK_PATH:-$DEMO_ROOT/demo.xlsx}\"\n\
ONTOLOGY_PATH=\"${ONTOLOGY_PATH:-$DEMO_ROOT/demo.ontology.json}\"\n\
SOURCE_REF=\"${SOURCE_REF:-wf-2023-01.rkyv}\"\n\
MODE=\"${1:-basic}\"\n\n\
mkdir -p \"$DEMO_ROOT\"\n\n\
if [[ \"$MODE\" == \"basic\" ]]; then\n  \
cargo run -q -p ledgerr-mcp --bin ledgerr-mcp-server <<EOF\n\
{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"clientInfo\":{\"name\":\"demo\",\"version\":\"0.1.0\"}}}\n\
{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\",\"params\":{}}\n\
{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\",\"params\":{}}\n\
{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/call\",\"params\":{\"name\":\"ledgerr_documents\",\"arguments\":{\"action\":\"pipeline_status\"}}}\n\
{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"tools/call\",\"params\":{\"name\":\"ledgerr_documents\",\"arguments\":{\"action\":\"list_accounts\"}}}\n\
{\"jsonrpc\":\"2.0\",\"id\":5,\"method\":\"tools/call\",\"params\":{\"name\":\"ledgerr_documents\",\"arguments\":{\"action\":\"ingest_pdf\",\"pdf_path\":\"WF--BH-CHK--2023-01--statement.pdf\",\"journal_path\":\"$JOURNAL_PATH\",\"workbook_path\":\"$WORKBOOK_PATH\",\"ontology_path\":\"$ONTOLOGY_PATH\",\"raw_context_bytes\":[99,116,120],\"extracted_rows\":[{\"account_id\":\"WF-BH-CHK\",\"date\":\"2023-01-15\",\"amount\":\"-42.11\",\"description\":\"Coffee Shop\",\"source_ref\":\"$SOURCE_REF\"}]}}}\n\
{\"jsonrpc\":\"2.0\",\"id\":6,\"method\":\"tools/call\",\"params\":{\"name\":\"ledgerr_audit\",\"arguments\":{\"action\":\"event_history\"}}}\n\
{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"tools/call\",\"params\":{\"name\":\"ledgerr_ontology\",\"arguments\":{\"action\":\"export_snapshot\",\"ontology_path\":\"$ONTOLOGY_PATH\"}}}\n\
EOF\n  \
exit 0\n\
fi\n\n\
if [[ \"$MODE\" == \"spinning-wheels\" ]]; then\n  \
cargo run -q -p ledgerr-mcp --bin ledgerr-mcp-server <<'EOF'\n\
{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"clientInfo\":{\"name\":\"demo\",\"version\":\"0.1.0\"}}}\n\
{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\",\"params\":{}}\n\
{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"ledgerr_workflow\",\"arguments\":{\"action\":\"resume\",\"state_marker\":\"invalid-checkpoint\"}}}\n\
{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/call\",\"params\":{\"name\":\"ledgerr_reconciliation\",\"arguments\":{\"action\":\"commit\",\"source_total\":\"100.00\",\"extracted_total\":\"95.00\",\"posting_amounts\":[\"-95.00\",\"95.00\"]}}}\n\
{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"tools/call\",\"params\":{\"name\":\"ledgerr_audit\",\"arguments\":{\"action\":\"event_history\",\"time_start\":\"2026-12-31\",\"time_end\":\"2026-01-01\"}}}\n\
EOF\n  \
exit 0\n\
fi\n\n\
echo \"usage: $0 [basic|spinning-wheels]\" >&2\n\
exit 2\n"
        .to_string()
}

fn documents_ingest_pdf_example() -> Value {
    json!({
      "jsonrpc":"2.0",
      "id":3,
      "method":"tools/call",
      "params":{
        "name":"ledgerr_documents",
        "arguments":{
          "action":"ingest_pdf",
          "pdf_path":"WF--BH-CHK--2023-01--statement.pdf",
          "journal_path":"/tmp/demo.beancount",
          "workbook_path":"/tmp/demo.xlsx",
          "raw_context_bytes":[99,116,120],
          "extracted_rows":[{
            "account_id":"WF-BH-CHK",
            "date":"2023-01-05",
            "amount":"-42.50",
            "description":"Coffee Beans",
            "source_ref":"wf-2023-01.rkyv"
          }]
        }
      }
    })
}

fn reconciliation_commit_example() -> Value {
    json!({
      "jsonrpc":"2.0",
      "id":4,
      "method":"tools/call",
      "params":{
        "name":"ledgerr_reconciliation",
        "arguments":{
          "action":"commit",
          "source_total":"42.50",
          "extracted_total":"42.50",
          "posting_amounts":["-42.50","42.50"]
        }
      }
    })
}

fn tax_evidence_chain_example() -> Value {
    json!({
      "jsonrpc":"2.0",
      "id":7,
      "method":"tools/call",
      "params":{
        "name":"ledgerr_tax",
        "arguments":{
          "action":"evidence_chain",
          "ontology_path":"/tmp/ontology.json",
          "from_entity_id":"WF-BH-CHK",
          "document_ref":"wf-2023-01.rkyv"
        }
      }
    })
}

fn pretty_json(value: &Value) -> String {
    serde_json::to_string_pretty(value).expect("json example")
}

fn compact_tool_call(name: &str, arguments: Value) -> String {
    serde_json::to_string(&json!({ "name": name, "arguments": arguments })).expect("json line")
}

// ============================================================================
// Batch Review Operations Types (Issue #25)
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BatchMode {
    /// Stop processing on first error and revert all changes
    AllOrNothing,
    /// Continue processing even if individual operations fail
    ContinueOnError,
}

impl Default for BatchMode {
    fn default() -> Self {
        Self::ContinueOnError
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FlagResolution {
    /// Mark flag as resolved (approval)
    Approve,
    /// Mark flag as resolved (rejection)
    Reject,
    /// Escalate flag for higher-level review
    Escalate,
    /// Dismiss flag as no longer relevant
    Dismiss,
    /// Defer flag for later review
    Defer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SimilarityMatchType {
    /// Exact field match
    Exact,
    /// Substring match (target contains source)
    Substring,
    /// Prefix match (target starts with source)
    Prefix,
}

impl Default for SimilarityMatchType {
    fn default() -> Self {
        Self::Exact
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BatchClassifyRequest {
    /// List of transaction IDs to classify
    pub tx_ids: Vec<String>,
    /// Target category for all transactions
    pub category: String,
    /// Confidence value (decimal string in [0,1])
    pub confidence: String,
    /// Optional note explaining the classification
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Actor performing the classification (e.g., "agent", "user", "reviewer")
    pub actor: String,
    /// Error handling mode
    #[serde(default)]
    pub batch_mode: BatchMode,
    /// If true, don't modify state, just validate
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BatchClassifyResponse {
    /// Summary of batch execution
    pub summary: BatchSummary,
    /// Individual item results
    pub items: Vec<BatchItemResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BatchResolveFlagsRequest {
    /// List of transaction IDs with open flags to resolve
    pub tx_ids: Vec<String>,
    /// Resolution action to apply
    pub resolution: FlagResolution,
    /// Optional reason for the resolution
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Actor performing the resolution
    pub actor: String,
    /// Error handling mode
    #[serde(default)]
    pub batch_mode: BatchMode,
    /// If true, don't modify state, just validate
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BulkResolveFlagsResponse {
    /// Summary of batch execution
    pub summary: BatchSummary,
    /// Individual item results
    pub items: Vec<BatchItemResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApplyMappingBulkRequest {
    /// Source transaction ID to use as template
    pub source_tx_id: String,
    /// Fields to match on (e.g., ["description", "amount"])
    pub match_fields: Vec<String>,
    /// Similarity matching algorithm
    #[serde(default)]
    pub similarity_type: SimilarityMatchType,
    /// Target category to apply to matches
    pub target_category: String,
    /// Target confidence value (decimal string in [0,1])
    pub target_confidence: String,
    /// Actor performing the operation
    pub actor: String,
    /// Maximum number of matches to process
    #[serde(default)]
    pub max_matches: usize,
    /// Error handling mode
    #[serde(default)]
    pub batch_mode: BatchMode,
    /// If true, don't modify state, just validate
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApplyMappingBulkResponse {
    /// Summary of classification operations
    pub classification_summary: BatchSummary,
    /// Transaction IDs that matched the similarity criteria
    pub matched_tx_ids: Vec<String>,
    /// Individual item results (empty in success cases, populated on errors)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<BatchItemResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BatchSummary {
    /// Total number of transactions requested
    pub total_requested: usize,
    /// Number of transactions that succeeded
    pub succeeded: usize,
    /// Number of transactions that failed
    pub failed: usize,
    /// Number of transactions that were skipped
    pub skipped: usize,
    /// Total batch execution time in milliseconds
    pub batch_duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BatchItemResult {
    /// Transaction ID
    pub tx_id: String,
    /// Status of this item
    pub status: BatchItemStatus,
    /// Audit entries generated for this item
    pub audit_entries: Vec<AuditEntryResponse>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum BatchItemStatus {
    /// Item was processed successfully
    Succeeded,
    /// Item failed to process
    Failed {
        /// Error message
        error: String,
    },
    /// Item was skipped
    Skipped {
        /// Reason for skipping
        reason: String,
    },
}

/// ============================================================================
/// WORK QUEUE CONTRACT (Unified work items from multiple sources)
/// ============================================================================

/// Type of work queue item
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueueItemType {
    /// Review flags from transaction classification
    Flag,
    /// Tax treatment ambiguities requiring human review
    Ambiguity,
    /// Reconciliation blockers preventing commit
    Blocker,
    /// Document processing issues (failed ingest, parse errors, etc.)
    DocumentIssue,
    /// Manual changes made by human operators
    ManualChange,
}

/// Severity level of a work queue item
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum QueueSeverity {
    /// Resolved or informational items
    Low,
    /// Medium priority items
    Medium,
    /// High priority items requiring attention
    High,
    /// Critical blockers preventing normal operation
    Critical,
}

/// Status of a work queue item
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueueStatus {
    /// Item is open and requires attention
    Open,
    /// Item is currently being worked on
    InProgress,
    /// Item has been resolved
    Resolved,
    /// Item has been dismissed (not applicable or false positive)
    Dismissed,
}

/// Provenance of a work queue item (which tool/source created it)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueueProvenance {
    /// From classification review tool
    ReviewTool,
    /// From tax analysis tool
    TaxTool,
    /// From audit log (manual changes)
    AuditTool,
    /// From reconciliation tool
    ReconciliationTool,
    /// From document ingest tool
    DocumentTool,
}

/// A work queue item from any source
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QueueItem {
    /// Content hash ID (Blake3)
    pub id: String,
    /// Type of item
    pub item_type: QueueItemType,
    /// Severity level
    pub severity: QueueSeverity,
    /// ISO 8601 timestamp when item was created
    pub created_at: String,
    /// Current status
    pub status: QueueStatus,
    /// Which tool/source emitted this item
    pub provenance: QueueProvenance,
    /// Affected transaction IDs
    pub related_tx_ids: Vec<String>,
    /// Human-readable summary
    pub summary: String,
    /// Transaction ID (for transaction-related items, optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tx_id: Option<String>,
    /// Document reference (for document-related items, optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_ref: Option<String>,
    /// Type-specific metadata
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, Value>,
}

/// Request to fetch work queue items
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FetchQueueRequest {
    /// Filter by item type (None = all types)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_types: Option<Vec<QueueItemType>>,
    /// Filter by status (None = all statuses)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub statuses: Option<Vec<QueueStatus>>,
    /// Only include items updated after this ISO 8601 timestamp (None = no filter)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_after: Option<String>,
    /// Maximum number of items to return (default: 100)
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Skip first N items (default: 0)
    #[serde(default)]
    pub offset: usize,
}

fn default_limit() -> usize {
    100
}

/// Response from fetching work queue items
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FetchQueueResponse {
    /// Returned items
    pub items: Vec<QueueItem>,
    /// Total count of matching items
    pub total_count: u64,
    /// Offset used in query
    pub offset: u32,
    /// Limit used in query
    pub limit: u32,
}
