use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::{error::Error, fmt};

use crate::journal::{append_entries, JournalTransaction};
use crate::workbook::{materialize_tx_projection, TxProjectionRow};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionInput {
    pub account_id: String,
    pub date: String,
    pub amount: String,
    pub description: String,
    pub source_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IngestedTransaction {
    pub tx_id: String,
    pub source_ref: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IngestedLedger {
    seen: BTreeSet<String>,
    projection_rows: Vec<TxProjectionRow>,
}

impl IngestedLedger {
    pub fn ingest(&mut self, rows: &[TransactionInput]) -> Vec<IngestedTransaction> {
        let mut out = Vec::new();

        for row in rows {
            let tx_id = deterministic_tx_id(row);
            if self.seen.insert(tx_id.clone()) {
                out.push(IngestedTransaction {
                    tx_id,
                    source_ref: row.source_ref.clone(),
                });
            }
        }

        out
    }

    pub fn ingest_to_journal(
        &mut self,
        rows: &[TransactionInput],
        journal_path: &Path,
    ) -> Result<Vec<IngestedTransaction>, std::io::Error> {
        let inserted = self.ingest(rows);
        let entries: Vec<JournalTransaction> = inserted
            .iter()
            .filter_map(|tx| {
                rows.iter()
                    .find(|row| deterministic_tx_id(row) == tx.tx_id)
                    .map(JournalTransaction::from_input)
            })
            .collect();
        append_entries(journal_path, &entries)?;
        Ok(inserted)
    }

    pub fn ingest_to_journal_and_workbook(
        &mut self,
        rows: &[TransactionInput],
        journal_path: &Path,
        workbook_path: &Path,
    ) -> Result<Vec<IngestedTransaction>, std::io::Error> {
        let inserted = self.ingest_to_journal(rows, journal_path)?;
        if inserted.is_empty() {
            return Ok(inserted);
        }

        let mut by_id = BTreeMap::<String, &TransactionInput>::new();
        for row in rows {
            by_id.insert(deterministic_tx_id(row), row);
        }

        for tx in &inserted {
            if let Some(row) = by_id.get(&tx.tx_id) {
                self.projection_rows.push(TxProjectionRow {
                    tx_id: tx.tx_id.clone(),
                    account_id: row.account_id.clone(),
                    date: row.date.clone(),
                    amount: row.amount.clone(),
                    description: row.description.clone(),
                    source_ref: row.source_ref.clone(),
                });
            }
        }

        materialize_tx_projection(workbook_path, &self.projection_rows)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        Ok(inserted)
    }
}

pub fn deterministic_tx_id(row: &TransactionInput) -> String {
    let canonical = format!(
        "{}|{}|{}|{}",
        row.account_id.trim().to_ascii_uppercase(),
        row.date.trim(),
        row.amount.trim(),
        row.description.trim().to_ascii_lowercase(),
    );
    blake3::hash(canonical.as_bytes()).to_hex().to_string()
}

impl From<&TransactionInput> for crate::pipeline::DocumentFields {
    fn from(row: &TransactionInput) -> Self {
        Self {
            amount: row.amount.trim().parse().ok(),
            ..Self::default()
        }
      
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentFieldsParseError {
    pub field: String,
    pub value: String,
}

impl fmt::Display for DocumentFieldsParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "failed to parse {} from transaction input: '{}'",
            self.field, self.value
        )
    }
}

impl Error for DocumentFieldsParseError {}

impl TryFrom<&TransactionInput> for crate::pipeline::DocumentFields {
    type Error = DocumentFieldsParseError;

    fn try_from(row: &TransactionInput) -> Result<Self, Self::Error> {
        let amount_text = row.amount.trim();
        let amount = amount_text
            .parse()
            .map_err(|_| DocumentFieldsParseError {
                field: "amount".to_string(),
                value: amount_text.to_string(),
            })?;
        Ok(Self {
            amount: Some(amount),
            ..Self::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transaction_input_to_document_fields_parses_amount() {
        let row = TransactionInput {
            account_id: "acc".to_string(),
            date: "2026-01-01".to_string(),
            amount: "12.34".to_string(),
            description: "test".to_string(),
            source_ref: "src".to_string(),
        };

        let fields = crate::pipeline::DocumentFields::try_from(&row).expect("valid decimal");
        assert_eq!(
            fields.amount,
            Some(rust_decimal::Decimal::from_str_exact("12.34").expect("valid decimal"))
        );
    }

    #[test]
    fn transaction_input_to_document_fields_rejects_invalid_amount() {
        let row = TransactionInput {
            account_id: "acc".to_string(),
            date: "2026-01-01".to_string(),
            amount: "not-a-decimal".to_string(),
            description: "test".to_string(),
            source_ref: "src".to_string(),
        };

        let err = crate::pipeline::DocumentFields::try_from(&row).expect_err("invalid decimal");
        assert_eq!(
            err,
            DocumentFieldsParseError {
                field: "amount".to_string(),
                value: "not-a-decimal".to_string(),
            }
        );
    }
}
