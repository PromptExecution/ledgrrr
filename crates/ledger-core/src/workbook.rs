use std::path::{Path, PathBuf};

use rust_xlsxwriter::{Workbook, Worksheet, DataValidation, Format};
use calamine::{Reader, open_workbook, Xlsx, Data};
use serde::{Deserialize, Serialize};

use crate::classify::{TaxCategory, Flag};
use crate::validation::{CommitGate, Disposition, Issue, MetaCtx};
use crate::attest::{Attested, AttestationSpec};
use ledger_attest::attested;
use strum::VariantArray;

pub const REQUIRED_SHEETS: &[&str] = &[
    "TRANSACTIONS",
    "FLAGS.open",
    "FLAGS.resolved",
    "MUTATION_HISTORY",
    "META.config",
    "ACCT.registry",
    "CAT.taxonomy",
    "SCHED.C",
    "SCHED.D",
    "SCHED.E",
    "FBAR.accounts",
    "AUDIT.log",
];
const TRANSACTIONS_SHEET: &str = "TRANSACTIONS";

#[derive(Debug, Clone)]
pub struct TransactionRow<'a> {
    pub tx_id: &'a str,
    pub date: &'a str,
    pub vendor: &'a str,
    pub account: &'a str,
    pub amount: &'a str,
    pub category: &'a str,
    pub confidence: f64,
    pub needs_review: bool,
    pub flag: Option<&'a str>,
}

impl<'a> TransactionRow<'a> {
    pub fn new(
        tx_id: &'a str,
        date: &'a str,
        vendor: &'a str,
        account: &'a str,
        amount: &'a str,
        category: &'a str,
        confidence: f64,
        needs_review: bool,
        flag: Option<&'a str>,
    ) -> Self {
        Self { tx_id, date, vendor, account, amount, category, confidence, needs_review, flag }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MutationRecord {
    pub timestamp: String,
    pub tx_id: String,
    pub agent_id: String,
    pub ring: String,
    pub action: String,
    pub before: String,
    pub after: String,
}

pub fn initialize_workbook(path: &Path) -> Result<(), rust_xlsxwriter::XlsxError> {
    let mut workbook = Workbook::new();
    for sheet_name in REQUIRED_SHEETS {
        let worksheet = workbook.add_worksheet().set_name(*sheet_name)?;
        
        if *sheet_name == TRANSACTIONS_SHEET {
            setup_transactions_sheet(worksheet)?;
        } else if *sheet_name == "AUDIT.log" {
            setup_audit_sheet(worksheet)?;
        }
    }
    workbook.save(path)
}

fn setup_transactions_sheet(worksheet: &mut Worksheet) -> Result<(), rust_xlsxwriter::XlsxError> {
    worksheet.write_string(0, 0, "tx_id")?;
    worksheet.write_string(0, 1, "date")?;
    worksheet.write_string(0, 2, "vendor")?;
    worksheet.write_string(0, 3, "account")?;
    worksheet.write_string(0, 4, "amount")?;
    worksheet.write_string(0, 5, "category")?;
    worksheet.write_string(0, 6, "confidence")?;
    worksheet.write_string(0, 7, "needs_review")?;
    worksheet.write_string(0, 8, "flag")?;

    let text_format = Format::new().set_num_format("@");
    worksheet.set_column_format(4, &text_format)?;

    let categories: Vec<String> = TaxCategory::VARIANTS.iter().map(|c| c.to_string()).collect();
    let validation = DataValidation::new().allow_list_strings(&categories)?;
    worksheet.add_data_validation(1, 5, 1000, 5, &validation)?;

    let flags: Vec<String> = Flag::VARIANTS.iter().map(|f| f.to_string()).collect();
    let flag_validation = DataValidation::new().allow_list_strings(&flags)?;
    worksheet.add_data_validation(1, 8, 1000, 8, &flag_validation)?;

    Ok(())
}

fn setup_audit_sheet(worksheet: &mut Worksheet) -> Result<(), rust_xlsxwriter::XlsxError> {
    worksheet.write_string(0, 0, "entry_id")?;
    worksheet.write_string(0, 1, "constraint_score")?;
    worksheet.write_string(0, 2, "legal_result")?;
    worksheet.write_string(0, 3, "disposition")?;
    worksheet.write_string(0, 4, "accumulated_confidence")?;
    worksheet.write_string(0, 5, "stage_trace_json")?;
    worksheet.write_string(0, 6, "flags")?;
    worksheet.write_string(0, 7, "invoice_arithmetic_ok")?;
    worksheet.write_string(0, 8, "commit_gate")?;
    // Keep stage_trace_json narrow so it doesn't overwhelm the CPA view
    worksheet.set_column_width(5, 8)?;
    Ok(())
}

pub struct WorkbookWriter {
    path: PathBuf,
}

impl WorkbookWriter {
    pub fn new(path: &Path) -> Self {
        WorkbookWriter {
            path: path.to_path_buf(),
        }
    }

    fn get_row_count(&self, sheet_name: &str) -> Result<u32, Box<dyn std::error::Error>> {
        let mut workbook: Xlsx<_> = open_workbook(&self.path)?;
        let range = workbook.worksheet_range(sheet_name)?;
        Ok(range.height() as u32)
    }

    pub fn get_existing_tx_ids(&self) -> Result<std::collections::HashSet<String>, Box<dyn std::error::Error>> {
        let mut workbook: Xlsx<_> = open_workbook(&self.path)?;
        let range = workbook.worksheet_range("TRANSACTIONS")?;
        let mut tx_ids = std::collections::HashSet::new();

        for row in range.rows().skip(1) {
            if let Some(Data::String(tx_id)) = row.get(0) {
                tx_ids.insert(tx_id.clone());
            }
        }

        Ok(tx_ids)
    }

    fn copy_sheet_data(
        worksheet: &mut Worksheet,
        range: &calamine::Range<Data>,
    ) -> Result<(), rust_xlsxwriter::XlsxError> {
        for (r_idx, row) in range.rows().enumerate() {
            for (c_idx, cell) in row.iter().enumerate() {
                let should_skip = matches!(cell,
                    Data::Empty | Data::DateTime(_) | Data::DateTimeIso(_) | Data::DurationIso(_) | Data::Error(_)
                );

                if should_skip {
                    continue;
                }

                match cell {
                    Data::String(s) => worksheet.write_string(r_idx as u32, c_idx as u16, s)?,
                    Data::Int(i) => worksheet.write_number(r_idx as u32, c_idx as u16, *i as f64)?,
                    Data::Float(f) => worksheet.write_number(r_idx as u32, c_idx as u16, *f)?,
                    Data::Bool(b) => worksheet.write_boolean(r_idx as u32, c_idx as u16, *b)?,
                    _ => unreachable!(),
                };
            }
        }
        Ok(())
    }

    fn copy_all_sheets(
        &self,
        new_workbook: &mut Workbook,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut workbook: Xlsx<_> = open_workbook(&self.path)?;
        
        for sheet_name in REQUIRED_SHEETS {
            let worksheet = new_workbook.add_worksheet().set_name(*sheet_name)?;
            if *sheet_name == TRANSACTIONS_SHEET {
                setup_transactions_sheet(worksheet)?;
            } else if *sheet_name == "AUDIT.log" {
                setup_audit_sheet(worksheet)?;
            }
            if let Ok(range) = workbook.worksheet_range(*sheet_name) {
                Self::copy_sheet_data(worksheet, &range)?;
            }
        }
        
        Ok(())
    }

    fn find_worksheet_by_name<'a>(
        workbook: &'a mut Workbook,
        name: &str,
    ) -> Option<&'a mut Worksheet> {
        workbook.worksheets_mut().iter_mut().find(|w| w.name() == name)
    }

    pub fn append_row(&self, row: TransactionRow<'_>) -> Result<(), Box<dyn std::error::Error>> {
        let row_count = self.get_row_count(TRANSACTIONS_SHEET)?;
        
        let mut new_workbook = Workbook::new();
        self.copy_all_sheets(&mut new_workbook)?;

        let worksheet = Self::find_worksheet_by_name(&mut new_workbook, TRANSACTIONS_SHEET)
            .ok_or("TRANSACTIONS sheet not found")?;
        
        worksheet.write_string(row_count, 0, row.tx_id)?;
        worksheet.write_string(row_count, 1, row.date)?;
        worksheet.write_string(row_count, 2, row.vendor)?;
        worksheet.write_string(row_count, 3, row.account)?;
        worksheet.write_string(row_count, 4, row.amount)?;
        worksheet.write_string(row_count, 5, row.category)?;
        worksheet.write_number(row_count, 6, row.confidence)?;
        worksheet.write_boolean(row_count, 7, row.needs_review)?;
        if let Some(f) = row.flag {
            worksheet.write_string(row_count, 8, f)?;
        }

        new_workbook.save(&self.path)?;
        self.append_mutation_internal(None, "append_row", row.tx_id, "agent", "workflow", "", &format!("Added transaction {}", row.tx_id))?;
        Ok(())
    }

    pub fn append_flag(
        &self,
        tx_id: &str,
        date: &str,
        vendor: &str,
        account: &str,
        amount: &str,
        category: &str,
        confidence: f64,
        flag_reason: &str,
        flagged_by: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let row = self.get_row_count("FLAGS.open")?;
        
        let mut new_workbook = Workbook::new();
        self.copy_all_sheets(&mut new_workbook)?;

        let worksheet = Self::find_worksheet_by_name(&mut new_workbook, "FLAGS.open")
            .ok_or("FLAGS.open sheet not found")?;
        
        worksheet.write_string(row, 0, tx_id)?;
        worksheet.write_string(row, 1, date)?;
        worksheet.write_string(row, 2, vendor)?;
        worksheet.write_string(row, 3, account)?;
        worksheet.write_string(row, 4, amount)?;
        worksheet.write_string(row, 5, category)?;
        worksheet.write_number(row, 6, confidence)?;
        worksheet.write_string(row, 7, flag_reason)?;
        worksheet.write_string(row, 8, flagged_by)?;

        new_workbook.save(&self.path)?;
        self.append_mutation_internal(None, "append_flag", tx_id, "agent", "workflow", "", &format!("Flagged: {}", flag_reason))?;
        Ok(())
    }

    pub fn append_audit_row(
        &self,
        row: &AuditRow,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let row_idx = self.get_row_count("AUDIT.log")?;

        let mut new_workbook = Workbook::new();
        self.copy_all_sheets(&mut new_workbook)?;

        let worksheet = Self::find_worksheet_by_name(&mut new_workbook, "AUDIT.log")
            .ok_or("AUDIT.log sheet not found")?;

        worksheet.write_string(row_idx, 0, &row.entry_id)?;
        worksheet.write_number(row_idx, 1, row.constraint_score as f64)?;
        worksheet.write_string(row_idx, 2, &row.legal_result)?;
        worksheet.write_string(row_idx, 3, &row.disposition)?;
        worksheet.write_number(row_idx, 4, row.accumulated_confidence as f64)?;
        worksheet.write_string(row_idx, 5, &row.stage_trace_json)?;
        worksheet.write_string(row_idx, 6, &row.flags)?;
        worksheet.write_boolean(row_idx, 7, row.invoice_arithmetic_ok)?;
        worksheet.write_string(row_idx, 8, &row.commit_gate)?;

        new_workbook.save(&self.path)?;
        Ok(())
    }

    pub fn append_mutation(
        &self,
        timestamp: &str,
        tx_id: &str,
        agent_id: &str,
        ring: &str,
        action: &str,
        before: &str,
        after: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if timestamp.is_empty() {
            return Err("timestamp cannot be empty".into());
        }
        self.append_mutation_internal(Some(timestamp), action, tx_id, agent_id, ring, before, after)
    }

    /// Convenience wrapper: construct a [`MutationRecord`] and pass it here instead of 7 `&str` args.
    pub fn append_mutation_record(
        &self,
        record: &MutationRecord,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.append_mutation(
            &record.timestamp,
            &record.tx_id,
            &record.agent_id,
            &record.ring,
            &record.action,
            &record.before,
            &record.after,
        )
    }

    fn append_mutation_internal(
        &self,
        timestamp: Option<&str>,
        action: &str,
        tx_id: &str,
        agent_id: &str,
        ring: &str,
        before: &str,
        after: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let row = self.get_row_count("MUTATION_HISTORY")?;
        
        let mut new_workbook = Workbook::new();
        self.copy_all_sheets(&mut new_workbook)?;

        let timestamp = timestamp
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
        let worksheet = Self::find_worksheet_by_name(&mut new_workbook, "MUTATION_HISTORY")
            .ok_or("MUTATION_HISTORY sheet not found")?;
        
        worksheet.write_string(row, 0, &timestamp)?;
        worksheet.write_string(row, 1, tx_id)?;
        worksheet.write_string(row, 2, agent_id)?;
        worksheet.write_string(row, 3, ring)?;
        worksheet.write_string(row, 4, action)?;
        worksheet.write_string(row, 5, before)?;
        worksheet.write_string(row, 6, after)?;

        new_workbook.save(&self.path)?;
        Ok(())
    }
}

/// One audit row per committed transaction for the AUDIT.log sheet.
#[attested("audit_row_entry_id_deterministic")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRow {
    pub entry_id: String,
    pub constraint_score: f32,
    pub legal_result: String,
    pub disposition: String,
    pub accumulated_confidence: f32,
    pub stage_trace_json: String,
    pub flags: String,
    pub invoice_arithmetic_ok: bool,
    pub commit_gate: String,
}

impl AuditRow {
    pub fn new(
        document_id: &str,
        source_ref: &str,
        constraint_score: f32,
        issues: &[Issue],
        meta: &MetaCtx,
        invoice_arithmetic_ok: bool,
        gate: &CommitGate,
    ) -> Self {
        let entry_id = {
            let input = format!("{document_id}|{source_ref}");
            blake3::hash(input.as_bytes()).to_hex().to_string()
        };

        let legal_result = issues
            .iter()
            .find(|i| i.code == "legal_violation")
            .map(|i| i.code.clone())
            .unwrap_or_else(|| "ok".to_string());

        let disposition = issues
            .iter()
            .map(|i| i.disposition)
            .max_by_key(|d| match d {
                Disposition::Unrecoverable => 2,
                Disposition::Recoverable => 1,
                Disposition::Advisory => 0,
            })
            .map(|d| format!("{d:?}").to_ascii_lowercase())
            .unwrap_or_else(|| "ok".to_string());

        let stage_trace_json = serde_json::to_string(&meta.stage_trace)
            .unwrap_or_else(|_| "[]".to_string());

        let flags = meta
            .flags
            .iter()
            .map(|f| f.to_string())
            .collect::<Vec<_>>()
            .join(",");

        let commit_gate = match gate {
            CommitGate::Approved { .. } => "Approved",
            CommitGate::PendingOperator { .. } => "PendingOperator",
            CommitGate::Blocked { .. } => "Blocked",
        }
        .to_string();

        Self {
            entry_id,
            constraint_score,
            legal_result,
            disposition,
            accumulated_confidence: meta.accumulated_confidence,
            stage_trace_json,
            flags,
            invoice_arithmetic_ok,
            commit_gate,
        }
    }
}

impl Attested for AuditRow {
    fn attestation_spec() -> AttestationSpec {
        AttestationSpec {
            invariant: "audit_row_entry_id_deterministic",
            z3_predicate: None,
            kasuari_description: Some("entry_id = blake3(document_id | source_ref) — same inputs always produce the same hash"),
            kani_module: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TxProjectionRow {
    pub tx_id: String,
    pub account_id: String,
    pub date: String,
    pub amount: String,
    pub description: String,
    pub source_ref: String,
}

pub fn materialize_tx_projection(
    path: &Path,
    rows: &[TxProjectionRow],
) -> Result<(), rust_xlsxwriter::XlsxError> {
    let mut workbook = Workbook::new();
    for sheet_name in REQUIRED_SHEETS {
        workbook.add_worksheet().set_name(*sheet_name)?;
    }

    let mut grouped = std::collections::BTreeMap::<String, Vec<&TxProjectionRow>>::new();
    for row in rows {
        grouped.entry(row.account_id.clone()).or_default().push(row);
    }

    for (account_id, account_rows) in grouped {
        let sheet_name = format!("TX.{}", account_id);
        let worksheet = workbook.add_worksheet().set_name(sheet_name)?;
        worksheet.write_string(0, 0, "tx_id")?;
        worksheet.write_string(0, 1, "date")?;
        worksheet.write_string(0, 2, "amount")?;
        worksheet.write_string(0, 3, "description")?;
        worksheet.write_string(0, 4, "source_ref")?;

        for (idx, row) in account_rows.into_iter().enumerate() {
            let r = (idx + 1) as u32;
            worksheet.write_string(r, 0, &row.tx_id)?;
            worksheet.write_string(r, 1, &row.date)?;
            worksheet.write_string(r, 2, &row.amount)?;
            worksheet.write_string(r, 3, &row.description)?;
            worksheet.write_string(r, 4, &row.source_ref)?;
        }
    }

    workbook.save(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_initialize_workbook_creates_required_sheets() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();
        
        initialize_workbook(path).unwrap();
        
        let mut workbook: Xlsx<_> = open_workbook(path).unwrap();
        for sheet_name in REQUIRED_SHEETS {
            assert!(workbook.worksheet_range(sheet_name).is_ok(),
                "Sheet {} should exist", sheet_name);
        }
    }

    #[test]
    fn test_transactions_sheet_has_headers_and_validation() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();
        
        initialize_workbook(path).unwrap();
        
        let mut workbook: Xlsx<_> = open_workbook(path).unwrap();
        let range = workbook.worksheet_range("TRANSACTIONS").unwrap();
        
        assert_eq!(range.get((0, 0)).unwrap().to_string(), "tx_id");
        assert_eq!(range.get((0, 1)).unwrap().to_string(), "date");
        assert_eq!(range.get((0, 5)).unwrap().to_string(), "category");
        assert_eq!(range.get((0, 8)).unwrap().to_string(), "flag");
    }

    #[test]
    fn test_append_row_writes_transaction_data() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();
        
        initialize_workbook(path).unwrap();
        
        let writer = WorkbookWriter::new(path);
        writer.append_row(TransactionRow::new(
            "tx_001",
            "2023-01-15",
            "Acme Corp",
            "CHK-001",
            "1234.56",
            "office_supplies",
            0.95,
            false,
            None,
        )).unwrap();
        
        let mut workbook: Xlsx<_> = open_workbook(path).unwrap();
        let range = workbook.worksheet_range("TRANSACTIONS").unwrap();
        
        assert_eq!(range.get((1, 0)).unwrap().to_string(), "tx_001");
        assert_eq!(range.get((1, 1)).unwrap().to_string(), "2023-01-15");
        assert_eq!(range.get((1, 2)).unwrap().to_string(), "Acme Corp");
        assert_eq!(range.get((1, 3)).unwrap().to_string(), "CHK-001");
        assert_eq!(range.get((1, 4)).unwrap().to_string(), "1234.56");
        assert_eq!(range.get((1, 5)).unwrap().to_string(), "office_supplies");
        assert_eq!(range.get((1, 6)).unwrap().to_string(), "0.95");
        assert_eq!(range.get((1, 7)).unwrap().to_string(), "false");
    }

    #[test]
    fn test_append_row_twice_creates_two_rows() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();
        
        initialize_workbook(path).unwrap();
        
        let writer = WorkbookWriter::new(path);
        writer.append_row(TransactionRow::new(
            "tx_001",
            "2023-01-15",
            "Acme Corp",
            "CHK-001",
            "1234.56",
            "office_supplies",
            0.95,
            false,
            None,
        )).unwrap();
        
        writer.append_row(TransactionRow::new(
            "tx_002",
            "2023-01-16",
            "Beta Inc",
            "CHK-001",
            "789.00",
            "travel",
            0.88,
            true,
            Some("unusual_amount"),
        )).unwrap();
        
        let mut workbook: Xlsx<_> = open_workbook(path).unwrap();
        let range = workbook.worksheet_range("TRANSACTIONS").unwrap();
        
        assert_eq!(range.get((1, 0)).unwrap().to_string(), "tx_001");
        assert_eq!(range.get((2, 0)).unwrap().to_string(), "tx_002");
    }

    #[test]
    fn test_append_flag_writes_to_flags_sheet() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();
        
        initialize_workbook(path).unwrap();
        
        let writer = WorkbookWriter::new(path);
        writer.append_flag(
            "tx_001",
            "2023-01-15",
            "Acme Corp",
            "CHK-001",
            "99999.99",
            "other",
            0.5,
            "Unusually large amount",
            "agent-001",
        ).unwrap();
        
        let mut workbook: Xlsx<_> = open_workbook(path).unwrap();
        let range = workbook.worksheet_range("FLAGS.open").unwrap();
        
        assert_eq!(range.get((0, 0)).unwrap().to_string(), "tx_001");
        assert_eq!(range.get((0, 7)).unwrap().to_string(), "Unusually large amount");
        assert_eq!(range.get((0, 8)).unwrap().to_string(), "agent-001");
    }

    #[test]
    fn test_mutation_history_is_append_only() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();
        
        initialize_workbook(path).unwrap();
        
        let writer = WorkbookWriter::new(path);
        writer.append_row(TransactionRow::new(
            "tx_001",
            "2023-01-15",
            "Acme Corp",
            "CHK-001",
            "1234.56",
            "office_supplies",
            0.95,
            false,
            None,
        )).unwrap();
        
        writer.append_row(TransactionRow::new(
            "tx_002",
            "2023-01-16",
            "Beta Inc",
            "CHK-001",
            "789.00",
            "travel",
            0.88,
            true,
            None,
        )).unwrap();
        
        let mut workbook: Xlsx<_> = open_workbook(path).unwrap();
        let range = workbook.worksheet_range("MUTATION_HISTORY").unwrap();
        
        let row_count = range.height();
        assert!(row_count >= 2, "Should have at least 2 mutation history entries");
    }

    #[test]
    fn test_append_mutation_uses_explicit_timestamp() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();

        initialize_workbook(path).unwrap();

        let writer = WorkbookWriter::new(path);
        writer.append_mutation(
            "2026-05-10T11:43:55Z",
            "tx_001",
            "agent-001",
            "workflow",
            "adjust_transaction",
            "before",
            "after",
        ).unwrap();

        let mut workbook: Xlsx<_> = open_workbook(path).unwrap();
        let range = workbook.worksheet_range("MUTATION_HISTORY").unwrap();

        assert_eq!(range.get((0, 0)).unwrap().to_string(), "2026-05-10T11:43:55Z");
        assert_eq!(range.get((0, 4)).unwrap().to_string(), "adjust_transaction");
    }

    #[test]
    fn test_tax_category_variants_exist() {
        let variants = TaxCategory::VARIANTS;
        assert!(variants.len() > 0);
        let variant_strings: Vec<String> = variants.iter().map(|c| c.to_string()).collect();
        assert!(variant_strings.contains(&"office_supplies".to_string()));
        assert!(variant_strings.contains(&"travel".to_string()));
    }

    #[test]
    fn test_flag_variants_exist() {
        let variants = Flag::VARIANTS;
        assert!(variants.len() > 0);
        let variant_strings: Vec<String> = variants.iter().map(|f| f.to_string()).collect();
        assert!(variant_strings.contains(&"unusual_amount".to_string()));
        assert!(variant_strings.contains(&"missing_receipt".to_string()));
    }

    #[test]
    fn test_amount_column_is_text_format() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();
        
        initialize_workbook(path).unwrap();
        
        let writer = WorkbookWriter::new(path);
        writer.append_row(TransactionRow::new(
            "tx_001",
            "2023-01-15",
            "Acme Corp",
            "CHK-001",
            "1234.56",
            "office_supplies",
            0.95,
            false,
            None,
        )).unwrap();
        
        let mut workbook: Xlsx<_> = open_workbook(path).unwrap();
        let range = workbook.worksheet_range("TRANSACTIONS").unwrap();
        
        let amount_cell = range.get((1, 4)).unwrap();
        let amount_str = amount_cell.to_string();
        assert_eq!(amount_str, "1234.56");
    }

    #[test]
    fn test_initialize_followed_by_two_appends_produces_valid_xlsx() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();

        initialize_workbook(path).unwrap();

        let writer = WorkbookWriter::new(path);
        writer.append_row(TransactionRow::new(
            "tx_001",
            "2023-01-15",
            "Acme Corp",
            "CHK-001",
            "1234.56",
            "office_supplies",
            0.95,
            false,
            None,
        )).unwrap();

        writer.append_row(TransactionRow::new(
            "tx_002",
            "2023-01-16",
            "Beta Inc",
            "CHK-001",
            "789.00",
            "travel",
            0.88,
            true,
            Some("unusual_amount"),
        )).unwrap();

        let mut workbook: Xlsx<_> = open_workbook(path).unwrap();
        assert!(workbook.worksheet_range("TRANSACTIONS").is_ok());
        assert!(workbook.worksheet_range("MUTATION_HISTORY").is_ok());
    }

    #[test]
    fn test_decimal_amount_stored_as_string_not_float() {
        use rust_decimal::Decimal;

        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();

        initialize_workbook(path).unwrap();

        let amount = Decimal::from_str_exact("1234.56").unwrap();
        let amount_str = amount.to_string();

        let writer = WorkbookWriter::new(path);
        writer.append_row(TransactionRow::new(
            "tx_001",
            "2023-01-15",
            "Test Vendor",
            "CHK-001",
            &amount_str,
            "office_supplies",
            0.95,
            false,
            None,
        )).unwrap();

        let mut workbook: Xlsx<_> = open_workbook(path).unwrap();
        let range = workbook.worksheet_range("TRANSACTIONS").unwrap();

        let amount_cell = range.get((1, 4)).unwrap();
        match amount_cell {
            Data::String(s) => assert_eq!(s, "1234.56"),
            Data::Float(_) => panic!("Amount should be stored as string, not float"),
            Data::Int(_) => panic!("Amount should be stored as string, not int"),
            _ => panic!("Unexpected cell type"),
        }
    }
    #[test]
    fn test_audit_row_legal_violation_result() {
        use crate::validation::{CommitGate, Disposition, Issue, IssueSource, MetaCtx};
        let issues = vec![Issue {
            code: "legal_violation".to_string(),
            message: "foreign SaaS should have BASEXCLUDED tax code".to_string(),
            field: None,
            disposition: Disposition::Unrecoverable,
            source: IssueSource::TypeCheck,
        }];
        let meta = MetaCtx::default();
        let gate = CommitGate::Blocked { issues: issues.clone() };
        let row = AuditRow::new("doc1", "WF--BH--2026-01", 0.0, &issues, &meta, true, &gate);
        assert_eq!(row.legal_result, "legal_violation");
        assert_eq!(row.commit_gate, "Blocked");
        assert_eq!(row.disposition, "unrecoverable");
    }

    #[test]
    fn test_audit_row_approved_gate() {
        use crate::validation::{CommitGate, MetaCtx};
        let gate = CommitGate::Approved { confidence: 0.95 };
        let meta = MetaCtx::default();
        let row = AuditRow::new("doc2", "WF--BH--2026-01", 0.95, &[], &meta, true, &gate);
        assert_eq!(row.commit_gate, "Approved");
        assert_eq!(row.legal_result, "ok");
        assert_eq!(row.disposition, "ok");
    }

    #[test]
    fn test_audit_row_stage_trace_json_is_valid() {
        use crate::validation::{CommitGate, MetaCtx};
        let mut meta = MetaCtx::default();
        meta = meta.advance("validate", 0.9, &[]);
        meta = meta.advance("verify_legal", 1.0, &[]);
        let gate = CommitGate::Approved { confidence: 0.9 };
        let row = AuditRow::new("doc3", "src", 1.0, &[], &meta, false, &gate);
        let parsed: serde_json::Value = serde_json::from_str(&row.stage_trace_json)
            .expect("stage_trace_json must be valid JSON");
        assert!(parsed.is_array());
        assert_eq!(parsed.as_array().unwrap().len(), 2);
    }
}
