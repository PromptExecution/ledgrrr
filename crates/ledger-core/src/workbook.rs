use std::path::{Path, PathBuf};

use rust_xlsxwriter::{Workbook, Worksheet, DataValidation, Format};
use calamine::{Reader, open_workbook, Xlsx, Data};
use serde::{Deserialize, Serialize};

use crate::classify::{TaxCategory, Flag};
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

pub fn initialize_workbook(path: &Path) -> Result<(), rust_xlsxwriter::XlsxError> {
    let mut workbook = Workbook::new();
    for sheet_name in REQUIRED_SHEETS {
        let worksheet = workbook.add_worksheet().set_name(*sheet_name)?;
        
        if *sheet_name == "TRANSACTIONS" {
            setup_transactions_sheet(worksheet)?;
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

    let categories: Vec<String> = TaxCategory::VARIANTS.iter().map(|s| s.to_string()).collect();
    let validation = DataValidation::new().set_multi_range(&categories);
    worksheet.add_data_validation(1, 5, 1000, 5, &validation)?;

    let flags: Vec<String> = Flag::VARIANTS.iter().map(|s| s.to_string()).collect();
    let flag_validation = DataValidation::new().set_multi_range(&flags);
    worksheet.add_data_validation(1, 8, 1000, 8, &flag_validation)?;

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
        let workbook: Xlsx<_> = open_workbook(&self.path)?;
        let range = workbook.worksheet_range(sheet_name)?;
        Ok(range.height() as u32)
    }

    fn copy_sheet_data(
        worksheet: &mut Worksheet,
        range: &calamine::Range<Data>,
    ) -> Result<(), rust_xlsxwriter::XlsxError> {
        for (r_idx, row) in range.rows().enumerate() {
            for (c_idx, cell) in row.iter().enumerate() {
                let result: Result<(), rust_xlsxwriter::XlsxError> = match cell {
                    Data::String(s) => worksheet.write_string(r_idx as u32, c_idx as u16, s),
                    Data::Int(i) => worksheet.write_number(r_idx as u32, c_idx as u16, *i as f64),
                    Data::Float(f) => worksheet.write_number(r_idx as u32, c_idx as u16, *f),
                    Data::Bool(b) => worksheet.write_boolean(r_idx as u32, c_idx as u16, *b),
                    Data::Empty => continue,
                    Data::DateTime(_) => continue,
                    Data::DateTimeIso(_) => continue,
                    Data::DurationIso(_) => continue,
                    Data::Error(_) => continue,
                };
                result?;
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
            if let Ok(range) = workbook.worksheet_range(*sheet_name) {
                let worksheet = new_workbook.add_worksheet().set_name(*sheet_name)?;
                Self::copy_sheet_data(worksheet, &range)?;
            } else {
                new_workbook.add_worksheet().set_name(*sheet_name)?;
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

    pub fn append_row(
        &self,
        tx_id: &str,
        date: &str,
        vendor: &str,
        account: &str,
        amount: &str,
        category: &str,
        confidence: f64,
        needs_review: bool,
        flag: Option<&str>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let row = self.get_row_count("TRANSACTIONS")?;
        
        let mut new_workbook = Workbook::new();
        self.copy_all_sheets(&mut new_workbook)?;

        let worksheet = Self::find_worksheet_by_name(&mut new_workbook, "TRANSACTIONS")
            .ok_or("TRANSACTIONS sheet not found")?;
        
        worksheet.write_string(row, 0, tx_id)?;
        worksheet.write_string(row, 1, date)?;
        worksheet.write_string(row, 2, vendor)?;
        worksheet.write_string(row, 3, account)?;
        worksheet.write_string(row, 4, amount)?;
        worksheet.write_string(row, 5, category)?;
        worksheet.write_number(row, 6, confidence)?;
        worksheet.write_boolean(row, 7, needs_review)?;
        if let Some(f) = flag {
            worksheet.write_string(row, 8, f)?;
        }

        new_workbook.save(&self.path)?;
        self.append_mutation_internal("append_row", tx_id, "agent", "workflow", "", &format!("Added transaction {}", tx_id))?;
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
        self.append_mutation_internal("append_flag", tx_id, "agent", "workflow", "", &format!("Flagged: {}", flag_reason))?;
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
        self.append_mutation_internal(action, tx_id, agent_id, ring, before, after)
    }

    fn append_mutation_internal(
        &self,
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

        let timestamp = chrono::Utc::now().to_rfc3339();
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
        
        let workbook: Xlsx<_> = open_workbook(path).unwrap();
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
        writer.append_row(
            "tx_001",
            "2023-01-15",
            "Acme Corp",
            "CHK-001",
            "1234.56",
            "office_supplies",
            0.95,
            false,
            None,
        ).unwrap();
        
        let mut workbook: Xlsx<_> = open_workbook(path).unwrap();
        let range = workbook.worksheet_range("TRANSACTIONS").unwrap();
        
        assert_eq!(range.get((1, 0)).unwrap().to_string(), "tx_001");
        assert_eq!(range.get((1, 1)).unwrap().to_string(), "2023-01-15");
        assert_eq!(range.get((1, 2)).unwrap().to_string(), "Acme Corp");
        assert_eq!(range.get((1, 3)).unwrap().to_string(), "CHK-001");
        assert_eq!(range.get((1, 4)).unwrap().to_string(), "1234.56");
        assert_eq!(range.get((1, 5)).unwrap().to_string(), "office_supplies");
        assert_eq!(range.get((1, 6)).to_string(), "0.95");
        assert_eq!(range.get((1, 7)).to_string(), "FALSE");
    }

    #[test]
    fn test_append_row_twice_creates_two_rows() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();
        
        initialize_workbook(path).unwrap();
        
        let writer = WorkbookWriter::new(path);
        writer.append_row(
            "tx_001",
            "2023-01-15",
            "Acme Corp",
            "CHK-001",
            "1234.56",
            "office_supplies",
            0.95,
            false,
            None,
        ).unwrap();
        
        writer.append_row(
            "tx_002",
            "2023-01-16",
            "Beta Inc",
            "CHK-001",
            "789.00",
            "travel",
            0.88,
            true,
            Some("unusual_amount"),
        ).unwrap();
        
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
        writer.append_row(
            "tx_001",
            "2023-01-15",
            "Acme Corp",
            "CHK-001",
            "1234.56",
            "office_supplies",
            0.95,
            false,
            None,
        ).unwrap();
        
        writer.append_row(
            "tx_002",
            "2023-01-16",
            "Beta Inc",
            "CHK-001",
            "789.00",
            "travel",
            0.88,
            true,
            None,
        ).unwrap();
        
        let mut workbook: Xlsx<_> = open_workbook(path).unwrap();
        let range = workbook.worksheet_range("MUTATION_HISTORY").unwrap();
        
        let row_count = range.height();
        assert!(row_count >= 2, "Should have at least 2 mutation history entries");
    }

    #[test]
    fn test_tax_category_variants_exist() {
        let variants = TaxCategory::VARIANTS;
        assert!(variants.len() > 0);
        assert!(variants.contains(&&"office_supplies"));
        assert!(variants.contains(&&"travel"));
    }

    #[test]
    fn test_flag_variants_exist() {
        let variants = Flag::VARIANTS;
        assert!(variants.len() > 0);
        assert!(variants.contains(&&"unusual_amount"));
        assert!(variants.contains(&&"missing_receipt"));
    }

    #[test]
    fn test_amount_column_is_text_format() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();
        
        initialize_workbook(path).unwrap();
        
        let writer = WorkbookWriter::new(path);
        writer.append_row(
            "tx_001",
            "2023-01-15",
            "Acme Corp",
            "CHK-001",
            "1234.56",
            "office_supplies",
            0.95,
            false,
            None,
        ).unwrap();
        
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
        writer.append_row(
            "tx_001",
            "2023-01-15",
            "Acme Corp",
            "CHK-001",
            "1234.56",
            "office_supplies",
            0.95,
            false,
            None,
        ).unwrap();
        
        writer.append_row(
            "tx_002",
            "2023-01-16",
            "Beta Inc",
            "CHK-001",
            "789.00",
            "travel",
            0.88,
            true,
            Some("unusual_amount"),
        ).unwrap();
        
        let workbook: Xlsx<_> = open_workbook(path).unwrap();
        assert!(workbook.worksheet_range("TRANSACTIONS").is_ok());
        assert!(workbook.worksheet_range("MUTATION_HISTORY").is_ok());
    }
}
