//! Composable ledger operation interface.
//!
//! [`LedgerOperation`] is the stable trait; concrete implementations below are
//! either stubs (returning `NotImplemented`) or functional. The skeleton bodies
//! document the intended logic so a reader can see what each operation would do.

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;

// `BusinessCalendar` is defined in `calendar`, which imports from here — use a
// forward-reference via the module path; actual Arc usage is behind `Option`.
use crate::calendar::{BusinessCalendar, ScheduledEvent};
use crate::classify::ClassifiedTransaction;

#[cfg(feature = "cedar-policy")]
use tracing as _;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum LedgerOpError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("operation not implemented: {0}")]
    NotImplemented(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("classification failed: {0}")]
    Classification(String),
    #[error("workbook error: {0}")]
    Workbook(String),
    #[error("external process failed: {0}")]
    ExternalProcessFailed(String),
    #[error("subprocess timeout after 120 seconds")]
    Timeout,
}

/// Row-level error from PDF ingest subprocess.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestRowError {
    pub tx_id: Option<String>,
    pub row_index: usize,
    pub error: String,
}

// ---------------------------------------------------------------------------
// Operation kind — carried in scheduled events
// ---------------------------------------------------------------------------

/// Discriminated union of operation kinds, used in [`crate::calendar::ScheduledEvent`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OperationKind {
    IngestStatement { source_glob: String },
    ClassifyTransactions { rule_dir: String },
    ReconcileAccount { account_id: String },
    ExportWorkbook { output_path: String },
    GenerateAuditTrail { year: i32 },
    CheckTaxDeadline { deadline_id: String },
}

// ---------------------------------------------------------------------------
// Execution context
// ---------------------------------------------------------------------------

/// Execution context passed to every operation.
#[derive(Clone)]
pub struct OperationContext {
    pub working_dir: PathBuf,
    pub rules_dir: PathBuf,
    pub calendar: Option<Arc<BusinessCalendar>>,
    pub dry_run: bool,
    /// Optional single-file input path for `IngestStatementOp`.
    pub input_path: Option<PathBuf>,
    /// Pre-classified transactions for `ExportWorkbookOp`.
    pub classified_transactions: Vec<ClassifiedTransaction>,
    /// Optional workbook path for `PdfIngestOp`.
    pub workbook_path: Option<PathBuf>,
    /// AGT gateway for `CedarGateOp` compliance checks.
    #[cfg(feature = "cedar-policy")]
    pub gateway: Option<Arc<msft_agent_gov_ledgrrr::LedgrrAgtGateway>>,
}

impl OperationContext {
    pub fn new(working_dir: PathBuf, rules_dir: PathBuf) -> Self {
        Self {
            working_dir,
            rules_dir,
            calendar: None,
            dry_run: false,
            input_path: None,
            classified_transactions: Vec::new(),
            workbook_path: None,
            #[cfg(feature = "cedar-policy")]
            gateway: None,
        }
    }

    pub fn with_input_path(mut self, path: PathBuf) -> Self {
        self.input_path = Some(path);
        self
    }

    pub fn with_classified_transactions(mut self, txs: Vec<ClassifiedTransaction>) -> Self {
        self.classified_transactions = txs;
        self
    }

    #[cfg(feature = "cedar-policy")]
    pub fn with_gateway(
        mut self,
        gateway: Arc<msft_agent_gov_ledgrrr::LedgrrAgtGateway>,
    ) -> Self {
        self.gateway = Some(gateway);
        self
    }

    pub fn with_calendar(mut self, cal: Arc<BusinessCalendar>) -> Self {
        self.calendar = Some(cal);
        self
    }

    pub fn dry_run(mut self) -> Self {
        self.dry_run = true;
        self
    }
}

// ---------------------------------------------------------------------------
// Operation result
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationResult {
    pub operation_id: String,
    pub success: bool,
    pub items_processed: usize,
    pub items_flagged: usize,
    pub issues: Vec<String>,
    pub duration_ms: u64,
    pub row_errors: Vec<IngestRowError>,
}

impl OperationResult {
    pub fn success(id: impl Into<String>, items: usize) -> Self {
        Self {
            operation_id: id.into(),
            success: true,
            items_processed: items,
            items_flagged: 0,
            issues: Vec::new(),
            duration_ms: 0,
            row_errors: Vec::new(),
        }
    }

    pub fn failure(id: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            operation_id: id.into(),
            success: false,
            items_processed: 0,
            items_flagged: 0,
            issues: vec![reason.into()],
            duration_ms: 0,
            row_errors: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Core trait
// ---------------------------------------------------------------------------

/// Core trait for all ledger operations.
pub trait LedgerOperation: Send + Sync {
    fn id(&self) -> &str;
    fn description(&self) -> &str;
    /// Whether running the same operation twice is safe (e.g. ingest is idempotent via Blake3).
    fn is_idempotent(&self) -> bool {
        false
    }
    fn execute(&self, ctx: &OperationContext) -> Result<OperationResult, LedgerOpError>;
}

// ---------------------------------------------------------------------------
// Concrete operations
// ---------------------------------------------------------------------------

/// Ingest all statement files matching a glob pattern.
pub struct IngestStatementOp {
    pub source_glob: String,
    pub vendor_hint: Option<String>,
}

impl LedgerOperation for IngestStatementOp {
    fn id(&self) -> &str {
        "ingest-statement"
    }

    fn description(&self) -> &str {
        "Ingest statement files matching a glob pattern into the ledger"
    }

    fn is_idempotent(&self) -> bool {
        // Blake3 content-hash IDs prevent duplicate records on re-ingest
        true
    }

    fn execute(&self, ctx: &OperationContext) -> Result<OperationResult, LedgerOpError> {
        use crate::document::DocType;
        use crate::document_shape::classify_document_shape;
        use crate::ingest::{IngestedLedger, TransactionInput};
        use calamine::{open_workbook_auto, Reader};

        let input_path = ctx.input_path.as_ref().ok_or_else(|| {
            LedgerOpError::InvalidInput("input_path not set in context".to_string())
        })?;

        let filename = input_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        let doc_type = DocType::from_path(input_path);

        // Read a small sample for shape classification (first 2 KB of the file
        // for CSV; not applicable for XLSX — just use the filename).
        let sample_content = if matches!(doc_type, DocType::SpreadsheetCsv) {
            std::fs::read_to_string(input_path)
                .map(|s| s.chars().take(2048).collect::<String>())
                .unwrap_or_default()
        } else {
            String::new()
        };

        let shape = classify_document_shape(&doc_type, filename, &sample_content);

        // Resolve canonical column names from the shape's column_map.
        // column_map: canonical → source_header. We need to find the
        // 0-based column index for "date", "amount", "description".
        //
        // For XLSX/CSV via calamine, we scan the header row.
        let mut workbook = open_workbook_auto(input_path)
            .map_err(|e| LedgerOpError::InvalidInput(format!("calamine open: {e}")))?;

        let sheet_names = workbook.sheet_names().to_vec();
        let first_sheet = sheet_names
            .first()
            .cloned()
            .ok_or_else(|| LedgerOpError::InvalidInput("no sheets in file".to_string()))?;

        let range = workbook
            .worksheet_range(&first_sheet)
            .map_err(|e| LedgerOpError::InvalidInput(format!("calamine range: {e}")))?;

        let mut rows_iter = range.rows();

        // Read header row to build column-index map
        let header_row = match rows_iter.next() {
            Some(h) => h,
            None => {
                return Ok(OperationResult::success("ingest-statement", 0));
            }
        };

        // Build header → index map from the actual file
        let header_map: std::collections::HashMap<String, usize> = header_row
            .iter()
            .enumerate()
            .filter_map(|(i, cell)| {
                let s = cell.to_string().trim().to_ascii_lowercase();
                if s.is_empty() {
                    None
                } else {
                    Some((s, i))
                }
            })
            .collect();

        // Resolve canonical names through shape.column_map → actual header name → index
        let find_col = |canon: &str| -> Option<usize> {
            // First try shape column_map canonical → source_header → index
            if let Some(source_header) = shape.column_map.get(canon) {
                let lower = source_header.to_ascii_lowercase();
                if let Some(&idx) = header_map.get(&lower) {
                    return Some(idx);
                }
            }
            // Fallback: direct canonical name match in header
            header_map.get(canon).copied()
        };

        let date_col = find_col("date");
        let amount_col = find_col("amount");
        let desc_col = find_col("description");

        // Derive account_id from filename (vendor slug or filename stem)
        let account_id = input_path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.split("--").next().unwrap_or(s).to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let mut transactions: Vec<TransactionInput> = Vec::new();

        for row in rows_iter {
            let get_cell = |col: Option<usize>| -> String {
                col.and_then(|i| row.get(i))
                    .map(|c| c.to_string().trim().to_string())
                    .unwrap_or_default()
            };

            let date = get_cell(date_col);
            let amount = get_cell(amount_col);
            let description = get_cell(desc_col);

            // Skip empty rows
            if date.is_empty() && amount.is_empty() && description.is_empty() {
                continue;
            }

            transactions.push(TransactionInput {
                account_id: account_id.clone(),
                date,
                amount,
                description,
                source_ref: filename.to_string(),
            });
        }

        let count = transactions.len();
        let mut ledger = IngestedLedger::default();
        ledger.ingest(&transactions);

        Ok(OperationResult::success("ingest-statement", count))
    }
}

/// Run the Rhai classification waterfall over unclassified transactions.
pub struct ClassifyTransactionsOp {
    pub rule_dir: PathBuf,
    pub review_threshold: f64,
    pub account_filter: Option<String>,
}

impl LedgerOperation for ClassifyTransactionsOp {
    fn id(&self) -> &str {
        "classify-transactions"
    }

    fn description(&self) -> &str {
        "Run the Rhai rule waterfall over unclassified transactions"
    }

    fn execute(&self, _ctx: &OperationContext) -> Result<OperationResult, LedgerOpError> {
        // Intended logic:
        //   1. Load all `.rhai` rule files from `self.rule_dir` via RuleRegistry
        //   2. Fetch unclassified transactions from ledger store
        //      (optionally filtered by `self.account_filter`)
        //   3. For each transaction:
        //      a. Run ClassificationEngine.classify(transaction)
        //      b. If confidence >= review_threshold → write classification
        //      c. If confidence < threshold → flag for human review
        //   4. Persist updated classifications back to the store
        //   5. Return processed/flagged counts and any rule errors
        Err(LedgerOpError::NotImplemented(
            "ClassifyTransactionsOp: Rhai engine integration not yet wired".to_string(),
        ))
    }
}

/// Reconcile a single account against external source (Xero stub).
pub struct ReconcileAccountOp {
    pub account_id: String,
    pub dry_run: bool,
}

impl LedgerOperation for ReconcileAccountOp {
    fn id(&self) -> &str {
        "reconcile-account"
    }

    fn description(&self) -> &str {
        "Reconcile a single account against Xero or another external source"
    }

    fn execute(&self, ctx: &OperationContext) -> Result<OperationResult, LedgerOpError> {
        // Intended logic:
        //   1. Load local transactions for `self.account_id` from ledger store
        //   2. Fetch corresponding transactions from Xero API (ledgerr-xero crate)
        //   3. Match transactions by date/amount/description heuristics
        //   4. Flag unmatched items on either side
        //   5. If !self.dry_run && !ctx.dry_run → write reconciliation status
        //   6. Return matched/unmatched counts and issues
        let _ = ctx; // suppress unused warning while stubbed
        Err(LedgerOpError::NotImplemented(format!(
            "ReconcileAccountOp: Xero integration not yet wired (account={})",
            self.account_id
        )))
    }
}

/// Write the current ledger state to an Excel workbook.
pub struct ExportWorkbookOp {
    pub output_path: PathBuf,
    pub include_flags: bool,
}

impl LedgerOperation for ExportWorkbookOp {
    fn id(&self) -> &str {
        "export-workbook"
    }

    fn description(&self) -> &str {
        "Write the current ledger state to an Excel workbook"
    }

    fn execute(&self, ctx: &OperationContext) -> Result<OperationResult, LedgerOpError> {
        use crate::workbook::TxProjectionRow;
        use rust_xlsxwriter::Workbook;

        let txs = &ctx.classified_transactions;

        if ctx.dry_run {
            return Ok(OperationResult::success("export-workbook", txs.len()));
        }

        // Route transactions to sheet groups by category
        let mut sched_c: Vec<TxProjectionRow> = Vec::new();
        let mut sched_d: Vec<TxProjectionRow> = Vec::new();
        let mut sched_e: Vec<TxProjectionRow> = Vec::new();
        let mut fbar: Vec<TxProjectionRow> = Vec::new();
        let mut flags_open: Vec<TxProjectionRow> = Vec::new();

        for tx in txs {
            let row = TxProjectionRow {
                tx_id: tx.tx_id.clone(),
                account_id: String::new(), // not carried in ClassifiedTransaction
                date: String::new(),
                amount: String::new(),
                description: String::new(),
                source_ref: tx.reason.clone(),
            };

            if tx.needs_review {
                if tx.category == "ForeignIncome" {
                    fbar.push(row.clone());
                }
                flags_open.push(row);
                continue;
            }

            match tx.category.as_str() {
                "SelfEmployment" => sched_c.push(row),
                "CapitalGain" | "CryptoGain" | "CryptoLoss" => sched_d.push(row),
                "RentalIncome" => sched_e.push(row),
                _ => {} // Other categories not yet routed to a specific sheet
            }
        }

        // Materialize the workbook with all required sheets
        let mut workbook = Workbook::new();
        for sheet_name in crate::workbook::REQUIRED_SHEETS {
            workbook
                .add_worksheet()
                .set_name(*sheet_name)
                .map_err(|e| LedgerOpError::Workbook(e.to_string()))?;
        }

        // Write each sheet group
        let write_sheet = |wb: &mut Workbook,
                           sheet_name: &str,
                           rows: &[TxProjectionRow]|
         -> Result<(), LedgerOpError> {
            let ws = wb
                .worksheet_from_name(sheet_name)
                .map_err(|e| LedgerOpError::Workbook(e.to_string()))?;
            ws.write_string(0, 0, "tx_id")
                .map_err(|e| LedgerOpError::Workbook(e.to_string()))?;
            ws.write_string(0, 1, "category")
                .map_err(|e| LedgerOpError::Workbook(e.to_string()))?;
            ws.write_string(0, 2, "reason")
                .map_err(|e| LedgerOpError::Workbook(e.to_string()))?;
            for (idx, row) in rows.iter().enumerate() {
                let r = (idx + 1) as u32;
                ws.write_string(r, 0, &row.tx_id)
                    .map_err(|e| LedgerOpError::Workbook(e.to_string()))?;
                ws.write_string(r, 2, &row.source_ref)
                    .map_err(|e| LedgerOpError::Workbook(e.to_string()))?;
            }
            Ok(())
        };

        write_sheet(&mut workbook, "SCHED.C", &sched_c)?;
        write_sheet(&mut workbook, "SCHED.D", &sched_d)?;
        write_sheet(&mut workbook, "SCHED.E", &sched_e)?;
        write_sheet(&mut workbook, "FBAR.accounts", &fbar)?;
        write_sheet(&mut workbook, "FLAGS.open", &flags_open)?;

        workbook
            .save(&self.output_path)
            .map_err(|e| LedgerOpError::Workbook(e.to_string()))?;

        let total = sched_c.len() + sched_d.len() + sched_e.len() + fbar.len() + flags_open.len();
        Ok(OperationResult::success("export-workbook", total))
    }
}

/// Generate a full audit trail document.
pub struct GenerateAuditTrailOp {
    pub output_path: PathBuf,
    pub year: i32,
}

impl LedgerOperation for GenerateAuditTrailOp {
    fn id(&self) -> &str {
        "generate-audit-trail"
    }

    fn description(&self) -> &str {
        "Generate a CPA-auditable audit trail document for a tax year"
    }

    fn execute(&self, _ctx: &OperationContext) -> Result<OperationResult, LedgerOpError> {
        // Intended logic:
        //   1. Query all mutation events for `self.year` from audit log
        //   2. Serialize to a structured JSON/XLSX audit document
        //   3. Include: ingest timestamps, classification changes, reconciliation
        //      outcomes, human review sign-offs
        //   4. Write to `self.output_path`
        Err(LedgerOpError::NotImplemented(format!(
            "GenerateAuditTrailOp: audit trail export not yet wired (year={})",
            self.year
        )))
    }
}

/// Check a tax deadline and emit an advisory issue if it is approaching.
pub struct CheckTaxDeadlineOp {
    pub deadline_id: String,
    pub warn_days_before: u32,
}

impl LedgerOperation for CheckTaxDeadlineOp {
    fn id(&self) -> &str {
        &self.deadline_id
    }

    fn description(&self) -> &str {
        "Check a scheduled tax deadline and emit advisory issues if approaching"
    }

    fn execute(&self, ctx: &OperationContext) -> Result<OperationResult, LedgerOpError> {
        // Intended logic:
        //   1. Look up `self.deadline_id` in `ctx.calendar`
        //   2. Compute next due date via BusinessCalendar::next_due
        //   3. If today + warn_days_before >= due_date → emit advisory issue
        //   4. Return result with issue text if approaching
        //
        // For now, just return success if calendar is not available.
        let _calendar = &ctx.calendar;

        // TODO: Implement full calendar lookup when calendar integration is complete
        Ok(OperationResult::success("check-tax-deadline", 0))
    }
}

/// Ingest a PDF statement file via the `reqif-opa-mcp` Python sidecar.
///
/// This op is a Phase 2 stub. See the TODO below for the intended implementation.
pub struct PdfIngestOp {
    pub input_path: PathBuf,
    pub rule_dir: PathBuf,
    pub workbook_path: PathBuf,
}

impl LedgerOperation for PdfIngestOp {
    fn id(&self) -> &str {
        "pdf-ingest"
    }

    fn description(&self) -> &str {
        "Ingest a PDF statement file via the reqif-opa-mcp Python sidecar (phase-2)"
    }

    fn is_idempotent(&self) -> bool {
        // Blake3 content-hash IDs prevent duplicate records on re-ingest
        true
    }

    fn execute(&self, _ctx: &OperationContext) -> Result<OperationResult, LedgerOpError> {
        use crate::classify::ClassificationEngine;
        use crate::document::DocType;
        use crate::ingest::TransactionInput;
        use crate::rule_registry::{ReqIfCandidate, RuleRegistry};
        use crate::workbook::WorkbookWriter;
        use std::time::Duration;

        let filename = self
            .input_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| LedgerOpError::InvalidInput("invalid filename".to_string()))?;

        let doc_type = DocType::from_path(&self.input_path);
        if !matches!(doc_type, DocType::Pdf) {
            return Err(LedgerOpError::InvalidInput(format!(
                "expected PDF file, got: {:?}",
                doc_type
            )));
        }

        // Use tokio runtime for async subprocess with timeout
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|e| LedgerOpError::ExternalProcessFailed(format!("runtime creation failed: {e}")))?;
        let input_path = self.input_path.to_string_lossy().into_owned();

        let output = runtime.block_on(async {
            let timeout_duration = Duration::from_secs(120);

            tokio::time::timeout(timeout_duration, async {
                let mut child = tokio::process::Command::new("reqif-opa-mcp")
                    .args([
                        "ingest",
                        "--file",
                        &input_path,
                        "--output",
                        "ndjson",
                    ])
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()
                    .map_err(|e| LedgerOpError::ExternalProcessFailed(format!("spawn failed: {e}")))?;

                let stdout = child.stdout.take().ok_or_else(|| {
                    LedgerOpError::ExternalProcessFailed("stdout not captured".to_string())
                })?;

                let stderr = child.stderr.take().ok_or_else(|| {
                    LedgerOpError::ExternalProcessFailed("stderr not captured".to_string())
                })?;

                let stdout_bytes = tokio::time::timeout(timeout_duration, async {
                    let mut buf = Vec::new();
                    use tokio::io::AsyncReadExt;
                    let mut reader = tokio::io::BufReader::new(stdout);
                    reader.read_to_end(&mut buf).await
                        .map_err(|e| LedgerOpError::Io(e))?;
                    Ok::<_, LedgerOpError>(buf)
                })
                .await
                .map_err(|_| LedgerOpError::Timeout)??;

                let stderr_bytes = tokio::time::timeout(timeout_duration, async {
                    let mut buf = Vec::new();
                    use tokio::io::AsyncReadExt;
                    let mut reader = tokio::io::BufReader::new(stderr);
                    reader.read_to_end(&mut buf).await
                        .map_err(|e| LedgerOpError::Io(e))?;
                    Ok::<_, LedgerOpError>(buf)
                })
                .await
                .map_err(|_| LedgerOpError::Timeout)??;

                let status = tokio::time::timeout(timeout_duration, child.wait())
                    .await
                    .map_err(|_| LedgerOpError::Timeout)??;

                Ok::<_, LedgerOpError>((status, stdout_bytes, stderr_bytes))
            })
            .await
            .map_err(|_| LedgerOpError::Timeout)?
        })?;

        if !output.0.success() {
            let stderr = String::from_utf8_lossy(&output.2);
            return Err(LedgerOpError::ExternalProcessFailed(format!(
                "exit code {}: {}",
                output.0, stderr
            )));
        }

        let stdout = String::from_utf8_lossy(&output.1);
        let mut candidates = Vec::new();
        let mut row_errors = Vec::new();

        for (line_num, line) in stdout.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }

            match serde_json::from_str::<ReqIfCandidate>(line) {
                Ok(candidate) => candidates.push(candidate),
                Err(e) => {
                    row_errors.push(IngestRowError {
                        tx_id: None,
                        row_index: line_num + 1,
                        error: format!("parse error: {e}"),
                    });
                }
            }
        }

        if candidates.is_empty() {
            return Ok(OperationResult {
                operation_id: "pdf-ingest".to_string(),
                success: true,
                items_processed: 0,
                items_flagged: 0,
                issues: Vec::new(),
                duration_ms: 0,
                row_errors,
            });
        }

        let mut engine = ClassificationEngine::default();
        let registry = RuleRegistry::load_from_dir(&self.rule_dir).map_err(|e| {
            LedgerOpError::InvalidInput(format!("failed to load rules: {e}"))
        })?;

        // Get existing tx_ids from workbook for deduplication
        let writer = WorkbookWriter::new(&self.workbook_path);
        let mut existing_tx_ids = writer.get_existing_tx_ids()
            .unwrap_or_else(|_| std::collections::HashSet::new());

        let mut processed = 0;
        let mut flagged = 0;

        for (candidate_index, candidate) in candidates.iter().enumerate() {
            let tx_input = TransactionInput {
                account_id: candidate.key.clone(),
                date: candidate.section.clone(),
                amount: candidate.confidence.to_string(),
                description: candidate.text.clone(),
                source_ref: filename.to_string(),
            };

            let tx_id = crate::ingest::deterministic_tx_id(&tx_input);

            // Blake3 deduplication: skip if tx_id already exists
            if existing_tx_ids.contains(&tx_id) {
                continue;
            }

            let sample = crate::classify::SampleTransaction {
                tx_id: tx_id.clone(),
                account_id: tx_input.account_id.clone(),
                date: tx_input.date.clone(),
                amount: tx_input.amount.clone(),
                description: tx_input.description.clone(),
            };

            match registry.classify_waterfall(&mut engine, &sample) {
                Ok(outcome) => {
                    processed += 1;
                    if outcome.needs_review {
                        flagged += 1;
                    }

                    // Persist classified transaction to workbook
                    writer.append_row(
                        &tx_id,
                        &tx_input.date,
                        &candidate.key,
                        &tx_input.account_id,
                        &tx_input.amount,
                        &outcome.category,
                        outcome.confidence,
                        outcome.needs_review,
                        None,
                    ).map_err(|e| LedgerOpError::Workbook(format!("failed to persist {}: {}", tx_id, e)))?;
                    existing_tx_ids.insert(tx_id);
                }
                Err(e) => {
                    row_errors.push(IngestRowError {
                        tx_id: Some(tx_id.clone()),
                        row_index: candidate_index + 1,
                        error: format!("classification failed: {e}"),
                    });
                }
            }
        }

        Ok(OperationResult {
            operation_id: "pdf-ingest".to_string(),
            success: row_errors.is_empty(),
            items_processed: processed,
            items_flagged: flagged,
            issues: if row_errors.is_empty() {
                Vec::new()
            } else {
                vec![format!("{} rows had errors", row_errors.len())]
            },
            duration_ms: 0,
            row_errors,
        })
    }
}

/// Gate classified transactions through AGT compliance before workbook commit.
///
/// Replaces OpaGateOp. Uses `LedgrrAgtGateway::compliance_report()` to determine
/// whether transactions should proceed to the workbook or be flagged for review.
#[cfg(feature = "cedar-policy")]
pub struct CedarGateOp;

#[cfg(feature = "cedar-policy")]
impl LedgerOperation for CedarGateOp {
    fn id(&self) -> &str {
        "cedar-gate"
    }

    fn description(&self) -> &str {
        "Gate classified transactions through AGT compliance before workbook commit"
    }

    fn execute(&self, ctx: &OperationContext) -> Result<OperationResult, LedgerOpError> {
        use msft_agent_gov_ledgrrr::ComplianceGrade;

        let gw = ctx.gateway.as_ref().ok_or_else(|| {
            LedgerOpError::InvalidInput(
                "CedarGateOp requires OperationContext.gateway to be set".to_string(),
            )
        })?;

        let report = gw.compliance_report();

        match report.grade {
            ComplianceGrade::Full => {
                tracing::info!("compliance gate passed: Full grade");
                Ok(OperationResult::success(
                    "cedar-gate",
                    ctx.classified_transactions.len(),
                ))
            }
            ComplianceGrade::Partial => {
                tracing::warn!("compliance gate partial: some controls unsatisfied");
                Ok(OperationResult {
                    operation_id: "cedar-gate".to_string(),
                    success: true,
                    items_processed: 0,
                    items_flagged: ctx.classified_transactions.len(),
                    issues: vec!["Compliance grade: Partial - transactions flagged".to_string()],
                    duration_ms: 0,
                    row_errors: Vec::new(),
                })
            }
            ComplianceGrade::Unknown => {
                tracing::warn!("compliance gate unknown: no attestations recorded");
                Ok(OperationResult {
                    operation_id: "cedar-gate".to_string(),
                    success: true,
                    items_processed: 0,
                    items_flagged: ctx.classified_transactions.len(),
                    issues: vec!["Compliance grade: Unknown - transactions flagged".to_string()],
                    duration_ms: 0,
                    row_errors: Vec::new(),
                })
            }
        }
    }
}

/// No-op fallback for OpaGateOp when cedar-policy feature is disabled.
///
/// Preserves existing behavior and allows tests to pass without the feature.
#[cfg(not(feature = "cedar-policy"))]
pub struct CedarGateOp;

#[cfg(not(feature = "cedar-policy"))]
impl LedgerOperation for CedarGateOp {
    fn id(&self) -> &str {
        "cedar-gate"
    }

    fn description(&self) -> &str {
        "No-op compliance gate (cedar-policy feature disabled)"
    }

    fn execute(&self, ctx: &OperationContext) -> Result<OperationResult, LedgerOpError> {
        Ok(OperationResult::success(
            "cedar-gate",
            ctx.classified_transactions.len(),
        ))
    }
}

// ---------------------------------------------------------------------------
// Dispatcher
// ---------------------------------------------------------------------------

/// Collects and runs multiple [`LedgerOperation`] instances.
#[derive(Default)]
pub struct OperationDispatcher {
    ops: Vec<Box<dyn LedgerOperation>>,
}

impl OperationDispatcher {
    pub fn new() -> Self {
        Self { ops: Vec::new() }
    }

    pub fn register(&mut self, op: Box<dyn LedgerOperation>) -> &mut Self {
        self.ops.push(op);
        self
    }

    /// Create a dispatcher from a slice of scheduled events.
    ///
    /// Each event's `operation` field is converted to a concrete operation struct
    /// and registered with the dispatcher.
    pub fn from_scheduled_events(events: &[ScheduledEvent]) -> Self {
        let mut dispatcher = Self::new();

        for event in events {
            let op: Box<dyn LedgerOperation> = match &event.operation {
                OperationKind::CheckTaxDeadline { deadline_id } => Box::new(CheckTaxDeadlineOp {
                    deadline_id: deadline_id.clone(),
                    warn_days_before: 30,
                }),
                OperationKind::IngestStatement { source_glob } => Box::new(IngestStatementOp {
                    source_glob: source_glob.clone(),
                    vendor_hint: None,
                }),
                OperationKind::ClassifyTransactions { rule_dir } => {
                    Box::new(ClassifyTransactionsOp {
                        rule_dir: PathBuf::from(rule_dir),
                        review_threshold: 0.8,
                        account_filter: None,
                    })
                }
                OperationKind::ReconcileAccount { account_id } => Box::new(ReconcileAccountOp {
                    account_id: account_id.clone(),
                    dry_run: false,
                }),
                OperationKind::ExportWorkbook { output_path } => Box::new(ExportWorkbookOp {
                    output_path: PathBuf::from(output_path),
                    include_flags: true,
                }),
                OperationKind::GenerateAuditTrail { year } => Box::new(GenerateAuditTrailOp {
                    output_path: PathBuf::from(format!("audit-trail-{}.xlsx", year)),
                    year: *year,
                }),
            };

            dispatcher.ops.push(op);
        }

        dispatcher
    }

    /// Run every registered operation and collect results.
    pub fn run_all(&self, ctx: &OperationContext) -> Vec<Result<OperationResult, LedgerOpError>> {
        self.ops.iter().map(|op| op.execute(ctx)).collect()
    }

    /// Run the first operation whose `id()` matches, returning `None` if not found.
    pub fn run_by_id(
        &self,
        id: &str,
        ctx: &OperationContext,
    ) -> Option<Result<OperationResult, LedgerOpError>> {
        self.ops
            .iter()
            .find(|op| op.id() == id)
            .map(|op| op.execute(ctx))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_ctx() -> OperationContext {
        OperationContext::new(PathBuf::from("/tmp/working"), PathBuf::from("/tmp/rules"))
    }

    #[test]
    fn operation_result_success_constructor() {
        let r = OperationResult::success("test-op", 42);
        assert!(r.success);
        assert_eq!(r.items_processed, 42);
        assert_eq!(r.operation_id, "test-op");
        assert!(r.issues.is_empty());
        assert!(r.row_errors.is_empty());
    }

    #[test]
    fn operation_result_failure_constructor() {
        let r = OperationResult::failure("test-op", "something broke");
        assert!(!r.success);
        assert_eq!(r.issues.len(), 1);
        assert!(r.issues[0].contains("something broke"));
    }

    #[test]
    fn operation_context_new() {
        let ctx = OperationContext::new(PathBuf::from("/work"), PathBuf::from("/rules"));
        assert_eq!(ctx.working_dir, PathBuf::from("/work"));
        assert_eq!(ctx.rules_dir, PathBuf::from("/rules"));
        assert!(!ctx.dry_run);
        assert!(ctx.calendar.is_none());
    }

    #[test]
    fn operation_context_builder_dry_run() {
        let ctx = OperationContext::new(PathBuf::from("/w"), PathBuf::from("/r")).dry_run();
        assert!(ctx.dry_run);
    }

    #[test]
    fn dispatcher_register_and_find_by_id() {
        let mut dispatcher = OperationDispatcher::new();
        dispatcher.register(Box::new(CheckTaxDeadlineOp {
            deadline_id: "us-q1".to_string(),
            warn_days_before: 30,
        }));

        let ctx = test_ctx();
        let result = dispatcher.run_by_id("us-q1", &ctx);
        assert!(result.is_some(), "should find operation by its deadline_id");
    }

    #[test]
    fn dispatcher_run_by_id_not_found_returns_none() {
        let dispatcher = OperationDispatcher::new();
        let ctx = test_ctx();
        let result = dispatcher.run_by_id("nonexistent", &ctx);
        assert!(result.is_none());
    }

    #[test]
    fn check_tax_deadline_returns_success() {
        let op = CheckTaxDeadlineOp {
            deadline_id: "us-annual".to_string(),
            warn_days_before: 30,
        };
        let ctx = test_ctx();
        let result = op.execute(&ctx);
        match result {
            Ok(op_result) => {
                assert!(op_result.success);
                assert_eq!(op_result.operation_id, "check-tax-deadline");
            }
            other => panic!("expected success, got {other:?}"),
        }
    }

    #[test]
    fn dispatcher_run_all_collects_results() {
        let mut dispatcher = OperationDispatcher::new();
        dispatcher.register(Box::new(CheckTaxDeadlineOp {
            deadline_id: "us-q1".to_string(),
            warn_days_before: 14,
        }));
        dispatcher.register(Box::new(CheckTaxDeadlineOp {
            deadline_id: "us-annual".to_string(),
            warn_days_before: 30,
        }));
        let ctx = test_ctx();
        let results = dispatcher.run_all(&ctx);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn ingest_op_is_idempotent() {
        let op = IngestStatementOp {
            source_glob: "statements/*.pdf".to_string(),
            vendor_hint: None,
        };
        assert!(op.is_idempotent());
    }

    #[test]
    fn pdf_ingest_op_is_idempotent() {
        let op = PdfIngestOp {
            input_path: PathBuf::from("/tmp/test.pdf"),
            rule_dir: PathBuf::from("/tmp/rules"),
            workbook_path: PathBuf::from("/tmp/workbook.xlsx"),
        };
        assert!(op.is_idempotent());
    }

    #[test]
    fn pdf_ingest_op_rejects_non_pdf() {
        let op = PdfIngestOp {
            input_path: PathBuf::from("/tmp/test.csv"),
            rule_dir: PathBuf::from("/tmp/rules"),
            workbook_path: PathBuf::from("/tmp/workbook.xlsx"),
        };
        let ctx = OperationContext::new(
            PathBuf::from("/tmp"),
            PathBuf::from("/tmp/rules"),
        );

        let result = op.execute(&ctx);
        assert!(result.is_err());
        match result {
            Err(LedgerOpError::InvalidInput(msg)) => {
                assert!(msg.contains("expected PDF"));
            }
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }
}
