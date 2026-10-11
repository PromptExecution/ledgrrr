//! Document shape classification — determines the bank/vendor and statement format
//! from a document before extraction. Distinct from transaction classification.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::document::DocType;

/// Known statement vendors. Extend as new institutions are onboarded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StatementVendor {
    WellsFargo,
    Chase,
    BankOfAmerica,
    Hsbc,        // international
    Anz,         // AU
    Commbank,    // AU Commonwealth Bank
    WestpacAu,   // AU Westpac
    Interactive, // Interactive Brokers
    Coinbase,
    Kraken,
    Generic,
    Unknown,
}

impl StatementVendor {
    /// Return the jurisdiction implied by this vendor.
    pub fn jurisdiction(&self) -> crate::legal::Jurisdiction {
        use crate::legal::Jurisdiction;
        match self {
            Self::WellsFargo
            | Self::Chase
            | Self::BankOfAmerica
            | Self::Interactive
            | Self::Coinbase
            | Self::Kraken => Jurisdiction::US,
            Self::Hsbc => Jurisdiction::UK,
            Self::Anz | Self::Commbank | Self::WestpacAu => Jurisdiction::AU,
            Self::Generic | Self::Unknown => Jurisdiction::US,
        }
    }

    /// Return a stable string key used in rule files and filenames.
    pub fn slug(&self) -> &'static str {
        match self {
            Self::WellsFargo => "wellsfargo",
            Self::Chase => "chase",
            Self::BankOfAmerica => "bankofamerica",
            Self::Hsbc => "hsbc",
            Self::Anz => "anz",
            Self::Commbank => "commbank",
            Self::WestpacAu => "westpac-au",
            Self::Interactive => "interactive",
            Self::Coinbase => "coinbase",
            Self::Kraken => "kraken",
            Self::Generic => "generic",
            Self::Unknown => "unknown",
        }
    }
}

/// The inferred shape of a bank statement document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentShape {
    pub vendor: StatementVendor,
    pub account_type: String, // "checking", "savings", "brokerage", "crypto"
    pub statement_format: String, // "csv_ofx", "pdf_tabular", "xlsx_native", "csv_generic"
    pub column_map: HashMap<String, String>, // canonical → source_header
    pub date_format: Option<String>, // e.g. "%m/%d/%Y"
    pub currency: String,     // "USD", "AUD", "EUR"
    pub confidence: f64,
    pub signals: Vec<String>, // which signals fired
}

impl DocumentShape {
    pub fn unknown() -> Self {
        Self {
            vendor: StatementVendor::Unknown,
            account_type: String::new(),
            statement_format: String::new(),
            column_map: HashMap::new(),
            date_format: None,
            currency: "USD".to_string(),
            confidence: 0.0,
            signals: Vec::new(),
        }
    }

    pub fn is_classified(&self) -> bool {
        self.confidence > 0.0
    }
}

// ---------------------------------------------------------------------------
// Internal classifier state
// ---------------------------------------------------------------------------

struct Classifier<'a> {
    doc_type: &'a DocType,
    filename: &'a str,
    filename_lower: String,
    sample_content: &'a str,
    vendor: StatementVendor,
    account_type: String,
    statement_format: String,
    column_map: HashMap<String, String>,
    date_format: Option<String>,
    currency: String,
    confidence: f64,
    signals: Vec<String>,
}

impl<'a> Classifier<'a> {
    fn new(doc_type: &'a DocType, filename: &'a str, sample_content: &'a str) -> Self {
        Self {
            doc_type,
            filename,
            filename_lower: filename.to_ascii_lowercase(),
            sample_content,
            vendor: StatementVendor::Unknown,
            account_type: String::new(),
            statement_format: String::new(),
            column_map: HashMap::new(),
            date_format: None,
            currency: "USD".to_string(),
            confidence: 0.0,
            signals: Vec::new(),
        }
    }

    fn add_signal(&mut self, sig: impl Into<String>, confidence_bump: f64) {
        self.signals.push(sig.into());
        self.confidence = (self.confidence + confidence_bump).min(1.0);
    }

    // -----------------------------------------------------------------------
    // Step 1: parse VENDOR--ACCOUNT--YYYY-MM--DOCTYPE filename convention
    // -----------------------------------------------------------------------
    fn classify_filename_convention(&mut self) {
        let parts: Vec<&str> = self.filename.split("--").collect();
        if parts.len() >= 3 {
            let vendor_slug = parts[0].to_ascii_lowercase();
            if let Some(v) = vendor_from_slug(&vendor_slug) {
                self.vendor = v;
                self.add_signal(format!("filename-convention vendor={vendor_slug}"), 0.5);
                // account type from second segment if present
                if parts.len() >= 2 {
                    self.account_type = parts[1].to_ascii_lowercase();
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Step 2: keyword matches in filename
    // -----------------------------------------------------------------------
    fn classify_filename_keywords(&mut self) {
        // Capture by value to avoid holding a borrow of self while calling add_signal
        let fl = self.filename_lower.clone();

        let checks: &[(&str, StatementVendor, &str)] = &[
            (
                "wellsfargo",
                StatementVendor::WellsFargo,
                "filename-keyword:wellsfargo",
            ),
            ("wf_", StatementVendor::WellsFargo, "filename-keyword:wf_"),
            ("chase", StatementVendor::Chase, "filename-keyword:chase"),
            ("jpmc", StatementVendor::Chase, "filename-keyword:jpmc"),
            ("hsbc", StatementVendor::Hsbc, "filename-keyword:hsbc"),
            ("anz", StatementVendor::Anz, "filename-keyword:anz"),
            (
                "commbank",
                StatementVendor::Commbank,
                "filename-keyword:commbank",
            ),
            ("cba", StatementVendor::Commbank, "filename-keyword:cba"),
            (
                "westpac",
                StatementVendor::WestpacAu,
                "filename-keyword:westpac",
            ),
            (
                "interactive",
                StatementVendor::Interactive,
                "filename-keyword:interactive",
            ),
            (
                "coinbase",
                StatementVendor::Coinbase,
                "filename-keyword:coinbase",
            ),
            ("kraken", StatementVendor::Kraken, "filename-keyword:kraken"),
            (
                "bankofamerica",
                StatementVendor::BankOfAmerica,
                "filename-keyword:bankofamerica",
            ),
            (
                "boa",
                StatementVendor::BankOfAmerica,
                "filename-keyword:boa",
            ),
        ];

        for (needle, vendor, signal) in checks {
            if fl.contains(needle) && self.vendor == StatementVendor::Unknown {
                self.vendor = vendor.clone();
                self.add_signal(*signal, 0.4);
            }
        }

        // Crypto hints → account type
        if (fl.contains("coinbase") || fl.contains("kraken")) && self.account_type.is_empty() {
            self.account_type = "crypto".to_string();
        }
    }

    // -----------------------------------------------------------------------
    // Step 3: content-based vendor detection
    // -----------------------------------------------------------------------
    fn classify_content_vendor(&mut self) {
        let content_lower = self.sample_content.to_ascii_lowercase();

        // Chase CSV header
        if self
            .sample_content
            .contains("Transaction Date,Post Date,Description,Amount")
            || self
                .sample_content
                .contains("Transaction Date,Post Date,Description,Category,Type,Amount")
        {
            if self.vendor == StatementVendor::Unknown {
                self.vendor = StatementVendor::Chase;
            }
            self.add_signal("content:chase-csv-header", 0.5);
            self.statement_format = "csv_generic".to_string();
        }

        // Wells Fargo CSV
        if content_lower.contains("wells fargo") {
            if self.vendor == StatementVendor::Unknown {
                self.vendor = StatementVendor::WellsFargo;
            }
            self.add_signal("content:wellsfargo-text", 0.4);
        }
        if self.sample_content.contains("Date,Amount,") && content_lower.contains("wells fargo") {
            self.add_signal("content:wellsfargo-csv-header", 0.3);
            self.statement_format = "csv_generic".to_string();
        }

        // AU signals
        if content_lower.contains("bsb") {
            self.add_signal("content:bsb-code", 0.3);
            self.currency = "AUD".to_string();
            if self.vendor == StatementVendor::Unknown {
                self.vendor = StatementVendor::Generic;
            }
        }

        // Currency clue in content
        if content_lower.contains("aud") || content_lower.contains("australian dollar") {
            self.currency = "AUD".to_string();
            self.add_signal("content:aud-currency", 0.2);
        }

        // IBAN / BIC / SWIFT → international, likely HSBC or Generic
        if content_lower.contains("iban")
            || content_lower.contains(" bic ")
            || content_lower.contains("swift")
        {
            self.add_signal("content:international-banking-codes", 0.3);
            if self.vendor == StatementVendor::Unknown {
                self.vendor = StatementVendor::Hsbc;
            }
        }

        // Specific institution text
        if content_lower.contains("commbank") || content_lower.contains("commonwealth bank") {
            if self.vendor == StatementVendor::Unknown {
                self.vendor = StatementVendor::Commbank;
            }
            self.add_signal("content:commbank-text", 0.4);
        }

        if content_lower.contains("westpac") {
            if self.vendor == StatementVendor::Unknown {
                self.vendor = StatementVendor::WestpacAu;
            }
            self.add_signal("content:westpac-text", 0.4);
        }
    }

    // -----------------------------------------------------------------------
    // Step 4: date format detection
    // -----------------------------------------------------------------------
    fn detect_date_format(&mut self) {
        // Check for ISO date 2024-01-15 style first
        if has_date_pattern(self.sample_content, DatePattern::Iso) {
            self.date_format = Some("%Y-%m-%d".to_string());
            self.add_signal("date-format:iso", 0.1);
        } else if has_date_pattern(self.sample_content, DatePattern::UsSlash) {
            self.date_format = Some("%m/%d/%Y".to_string());
            self.add_signal("date-format:us-slash", 0.1);
        } else if has_date_pattern(self.sample_content, DatePattern::AuSlash) {
            self.date_format = Some("%d/%m/%Y".to_string());
            self.add_signal("date-format:au-slash", 0.1);
            // AU date style reinforces AUD
            if self.currency == "USD" {
                self.currency = "AUD".to_string();
            }
        }
    }

    // -----------------------------------------------------------------------
    // Step 5: CSV column map inference
    // -----------------------------------------------------------------------
    fn infer_column_map(&mut self) {
        if !matches!(
            self.doc_type,
            DocType::SpreadsheetCsv | DocType::SpreadsheetXlsx
        ) {
            return;
        }

        // Take the first non-empty line as the header
        let header_line = self
            .sample_content
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("");

        let cols: Vec<&str> = header_line.split(',').collect();
        for col in &cols {
            let trimmed = col.trim().trim_matches('"');
            let lower = trimmed.to_ascii_lowercase();
            let canonical = match lower.as_str() {
                "date" | "transaction date" | "post date" | "trans date" | "value date"
                | "transaction_date" => Some("date"),
                "amount" | "debit" | "credit" | "debit amount" | "credit amount"
                | "transaction amount" => Some("amount"),
                "description"
                | "memo"
                | "narrative"
                | "transaction description"
                | "trans description"
                | "details" => Some("description"),
                "balance" | "running balance" | "available balance" => Some("balance"),
                "category" | "type" | "transaction type" => Some("category"),
                _ => None,
            };
            if let Some(canon) = canonical {
                self.column_map
                    .entry(canon.to_string())
                    .or_insert_with(|| trimmed.to_string());
            }
        }

        if !self.column_map.is_empty() {
            self.add_signal(format!("csv-column-map:{}", self.column_map.len()), 0.1);
        }
    }

    // -----------------------------------------------------------------------
    // Step 6: format fallback
    // -----------------------------------------------------------------------
    fn infer_statement_format(&mut self) {
        if !self.statement_format.is_empty() {
            return;
        }
        self.statement_format = match self.doc_type {
            DocType::SpreadsheetCsv => "csv_generic".to_string(),
            DocType::SpreadsheetXlsx => "xlsx_native".to_string(),
            DocType::Pdf | DocType::BankStatement => "pdf_tabular".to_string(),
            _ => "unknown".to_string(),
        };
    }

    // -----------------------------------------------------------------------
    // Step 7: AU vendor → AUD currency
    // -----------------------------------------------------------------------
    fn reconcile_au_currency(&mut self) {
        match &self.vendor {
            StatementVendor::Anz | StatementVendor::Commbank | StatementVendor::WestpacAu => {
                self.currency = "AUD".to_string();
            }
            _ => {}
        }
    }

    // -----------------------------------------------------------------------
    // Step 8: Generic CSV with no other signal
    // -----------------------------------------------------------------------
    fn handle_generic_csv_fallback(&mut self) {
        if self.vendor == StatementVendor::Unknown
            && matches!(self.doc_type, DocType::SpreadsheetCsv)
        {
            self.vendor = StatementVendor::Generic;
            self.statement_format = "csv_generic".to_string();
            self.add_signal("fallback:generic-csv", 0.1);
        }
    }

    fn build(self) -> DocumentShape {
        DocumentShape {
            vendor: self.vendor,
            account_type: self.account_type,
            statement_format: self.statement_format,
            column_map: self.column_map,
            date_format: self.date_format,
            currency: self.currency,
            confidence: self.confidence,
            signals: self.signals,
        }
    }
}

// ---------------------------------------------------------------------------
// Date pattern helpers
// ---------------------------------------------------------------------------

enum DatePattern {
    Iso,     // 2024-01-15
    UsSlash, // 01/15/2024
    AuSlash, // 15/01/2024 (day > 12 in first position gives this away)
}

fn has_date_pattern(content: &str, pattern: DatePattern) -> bool {
    // Simple scan — look for 10-char substrings that match the shape
    match pattern {
        DatePattern::Iso => {
            // YYYY-MM-DD: first four chars are a plausible year
            content.as_bytes().windows(10).any(|w| {
                if w[4] == b'-' && w[7] == b'-' {
                    w[0..4].iter().all(|c| c.is_ascii_digit())
                        && w[5..7].iter().all(|c| c.is_ascii_digit())
                        && w[8..10].iter().all(|c| c.is_ascii_digit())
                } else {
                    false
                }
            })
        }
        DatePattern::UsSlash => {
            // MM/DD/YYYY — look for d{1,2}/d{1,2}/d{4}
            content.as_bytes().windows(10).any(|w| {
                if w[2] == b'/' && w[5] == b'/' {
                    // month 01-12
                    let m1 = w[0] as char;
                    let m2 = w[1] as char;
                    let d1 = w[3] as char;
                    let d2 = w[4] as char;
                    let year_ok = w[6..10].iter().all(|c| c.is_ascii_digit());
                    m1.is_ascii_digit() && m2.is_ascii_digit()
                        && d1.is_ascii_digit() && d2.is_ascii_digit()
                        && year_ok
                        // Disambiguate from AU: if day part > 12 it cannot be US month
                        && !(w[0] == b'1' && w[1] > b'2' || w[0] == b'2' || w[0] == b'3')
                } else {
                    false
                }
            })
        }
        DatePattern::AuSlash => {
            // DD/MM/YYYY — day > 12 in position 0-1 distinguishes from US
            content.as_bytes().windows(10).any(|w| {
                if w[2] == b'/' && w[5] == b'/' {
                    let day_tens = w[0];
                    let day_units = w[1];
                    let year_ok = w[6..10].iter().all(|c| c.is_ascii_digit());
                    let is_valid_day_tens = (b'0'..=b'3').contains(&day_tens);
                    let unit_digit = day_units.is_ascii_digit();
                    // AU style if day tens > 1 (i.e. 20-31) or tens == 1 and units > 2 (13-19)
                    let looks_au = (day_tens == b'1' && day_units > b'2')
                        || (day_tens == b'2')
                        || (day_tens == b'3' && (day_units == b'0' || day_units == b'1'));
                    year_ok && is_valid_day_tens && unit_digit && looks_au
                } else {
                    false
                }
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Slug → vendor helper
// ---------------------------------------------------------------------------

fn vendor_from_slug(slug: &str) -> Option<StatementVendor> {
    match slug {
        "wellsfargo" | "wf" => Some(StatementVendor::WellsFargo),
        "chase" | "jpmc" => Some(StatementVendor::Chase),
        "bankofamerica" | "boa" => Some(StatementVendor::BankOfAmerica),
        "hsbc" => Some(StatementVendor::Hsbc),
        "anz" => Some(StatementVendor::Anz),
        "commbank" | "cba" => Some(StatementVendor::Commbank),
        "westpac" | "westpacau" => Some(StatementVendor::WestpacAu),
        "interactive" => Some(StatementVendor::Interactive),
        "coinbase" => Some(StatementVendor::Coinbase),
        "kraken" => Some(StatementVendor::Kraken),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Classify a document's shape from its type, filename, and a sample of its
/// text content (first ~2 KB from Docling extraction).
///
/// This is the deterministic fast path — no LLM required.
pub fn classify_document_shape(
    doc_type: &DocType,
    filename: &str,
    sample_content: &str,
) -> DocumentShape {
    let mut c = Classifier::new(doc_type, filename, sample_content);

    c.classify_filename_convention();
    c.classify_filename_keywords();
    c.classify_content_vendor();
    c.detect_date_format();
    c.infer_column_map();
    c.infer_statement_format();
    c.reconcile_au_currency();
    c.handle_generic_csv_fallback();

    c.build()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wellsfargo_filename_keyword() {
        let shape =
            classify_document_shape(&DocType::SpreadsheetCsv, "wellsfargo_checking_2024.csv", "");
        assert_eq!(shape.vendor, StatementVendor::WellsFargo);
        assert!(shape.confidence > 0.0);
    }

    #[test]
    fn wellsfargo_wf_prefix() {
        let shape = classify_document_shape(&DocType::SpreadsheetCsv, "wf_savings_jan2024.csv", "");
        assert_eq!(shape.vendor, StatementVendor::WellsFargo);
    }

    #[test]
    fn chase_csv_header_detection() {
        let header = "Transaction Date,Post Date,Description,Amount\n01/15/2024,01/16/2024,AMAZON.COM,-42.99\n";
        let shape = classify_document_shape(&DocType::SpreadsheetCsv, "statement.csv", header);
        assert_eq!(shape.vendor, StatementVendor::Chase);
        // Column map should have date and amount
        assert!(
            shape.column_map.contains_key("date"),
            "expected 'date' key in column_map, got {:?}",
            shape.column_map
        );
        assert!(
            shape.column_map.contains_key("amount"),
            "expected 'amount' key, got {:?}",
            shape.column_map
        );
    }

    #[test]
    fn au_filename_vendor_and_currency() {
        let shape =
            classify_document_shape(&DocType::Pdf, "anz--checking--2024-03--statement.pdf", "");
        assert_eq!(shape.vendor, StatementVendor::Anz);
        assert_eq!(shape.currency, "AUD");
    }

    #[test]
    fn commbank_filename_keyword() {
        let shape = classify_document_shape(
            &DocType::SpreadsheetCsv,
            "commbank_transactions_2024.csv",
            "",
        );
        assert_eq!(shape.vendor, StatementVendor::Commbank);
        assert_eq!(shape.currency, "AUD");
    }

    #[test]
    fn unknown_content_returns_unknown() {
        let shape = classify_document_shape(
            &DocType::Pdf,
            "mystery_document.pdf",
            "some random text with no bank signals",
        );
        assert_eq!(shape.vendor, StatementVendor::Unknown);
        assert_eq!(shape.confidence, 0.0);
    }

    #[test]
    fn vendor_account_filename_convention_parse() {
        let shape =
            classify_document_shape(&DocType::Pdf, "chase--checking--2024-01--statement.pdf", "");
        assert_eq!(shape.vendor, StatementVendor::Chase);
        assert_eq!(shape.account_type, "checking");
    }

    #[test]
    fn generic_csv_fallback() {
        let shape = classify_document_shape(
            &DocType::SpreadsheetCsv,
            "transactions_export.csv",
            "Date,Description,Amount\n",
        );
        assert_eq!(shape.vendor, StatementVendor::Generic);
        assert_eq!(shape.statement_format, "csv_generic");
    }

    #[test]
    fn iban_signals_international() {
        let shape = classify_document_shape(
            &DocType::Pdf,
            "statement.pdf",
            "Your IBAN: GB29NWBK60161331926819\nBIC: NWBKGB2L\n",
        );
        assert!(shape.signals.iter().any(|s| s.contains("international")));
    }

    #[test]
    fn date_format_iso_detected() {
        let shape = classify_document_shape(
            &DocType::SpreadsheetCsv,
            "data.csv",
            "date,amount\n2024-03-15,-50.00\n",
        );
        assert_eq!(shape.date_format, Some("%Y-%m-%d".to_string()));
    }

    #[test]
    fn date_format_us_slash_detected() {
        let shape = classify_document_shape(
            &DocType::SpreadsheetCsv,
            "chase.csv",
            "Transaction Date,Post Date,Description,Amount\n01/15/2024,01/16/2024,STORE,-10.00\n",
        );
        assert_eq!(shape.date_format, Some("%m/%d/%Y".to_string()));
    }

    #[test]
    fn vendor_slug_roundtrip() {
        assert_eq!(StatementVendor::WellsFargo.slug(), "wellsfargo");
        assert_eq!(StatementVendor::Anz.slug(), "anz");
        assert_eq!(StatementVendor::Unknown.slug(), "unknown");
    }
}
