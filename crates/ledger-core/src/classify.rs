use std::path::Path;

use crate::ingest::{deterministic_tx_id, TransactionInput};
use rhai::{Dynamic, Engine, EvalAltResult, Map, Scope, AST};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

#[derive(Debug, thiserror::Error)]
pub enum ClassificationError {
    #[error("failed to read rule file: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to compile rule file: {0}")]
    Compile(#[from] rhai::ParseError),
    #[error("failed to execute rule: {0}")]
    Eval(#[from] Box<EvalAltResult>),
    #[error("invalid rule output: missing or invalid field `{0}`")]
    InvalidOutput(&'static str),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SampleTransaction {
    pub tx_id: String,
    pub account_id: String,
    pub date: String,
    pub amount: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassificationOutcome {
    pub category: String,
    pub confidence: f64,
    pub needs_review: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumString, Display, Serialize, Deserialize, strum::VariantArray)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum TaxCategory {
    OfficeSupplies,
    Travel,
    MealsAndEntertainment,
    SoftwareAndSubscriptions,
    ProfessionalServices,
    RentAndUtilities,
    MarketingAndAdvertising,
    Insurance,
    Payroll,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumString, Display, Serialize, Deserialize, strum::VariantArray)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum Flag {
    UnusualAmount,
    MissingReceipt,
    UnclearDescription,
    PotentialPersonal,
    ReviewRequired,
}

pub use crate::workbook::MutationRecord;

#[derive(Debug, Clone, PartialEq)]
pub struct ClassifiedTransaction {
    pub tx_id: String,
    pub category: String,
    pub confidence: f64,
    pub needs_review: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassificationBatch {
    pub classifications: Vec<ClassifiedTransaction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlagStatus {
    Open,
    Resolved,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewFlag {
    pub tx_id: String,
    pub year: i32,
    pub status: FlagStatus,
    pub reason: String,
    pub category: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClassificationEngine {
    flags: Vec<ReviewFlag>,
}

impl ClassificationEngine {
    pub fn run_rule_from_file(
        &self,
        rule_file: &Path,
        sample: &SampleTransaction,
    ) -> Result<ClassificationOutcome, ClassificationError> {
        let src = std::fs::read_to_string(rule_file)?;
        let engine = Engine::new();
        let ast = engine.compile(src)?;
        run_classify_fn(&engine, &ast, sample)
    }

    pub fn classify_rows_from_file(
        &mut self,
        rule_file: &Path,
        rows: &[TransactionInput],
        review_threshold: f64,
    ) -> Result<ClassificationBatch, ClassificationError> {
        let src = std::fs::read_to_string(rule_file)?;
        let engine = Engine::new();
        let ast = engine.compile(src)?;

        let mut out = Vec::new();
        for row in rows {
            let sample = SampleTransaction {
                tx_id: deterministic_tx_id(row),
                account_id: row.account_id.clone(),
                date: row.date.clone(),
                amount: row.amount.clone(),
                description: row.description.clone(),
            };
            let mut result = run_classify_fn(&engine, &ast, &sample)?;
            result.needs_review = result.needs_review || result.confidence < review_threshold;

            if result.needs_review {
                self.upsert_open_flag(
                    sample.tx_id.clone(),
                    derive_year(&sample.date),
                    result.reason.clone(),
                    result.category.clone(),
                    result.confidence,
                );
            }

            out.push(ClassifiedTransaction {
                tx_id: sample.tx_id,
                category: result.category,
                confidence: result.confidence,
                needs_review: result.needs_review,
                reason: result.reason,
            });
        }

        Ok(ClassificationBatch {
            classifications: out,
        })
    }

    pub fn query_flags(&self, year: i32, status: FlagStatus) -> Vec<ReviewFlag> {
        self.flags
            .iter()
            .filter(|flag| flag.year == year && flag.status == status)
            .cloned()
            .collect()
    }

    pub fn record_review_flag(
        &mut self,
        tx_id: String,
        date: &str,
        reason: String,
        category: String,
        confidence: f64,
    ) {
        self.upsert_open_flag(tx_id, derive_year(date), reason, category, confidence);
    }

    fn upsert_open_flag(
        &mut self,
        tx_id: String,
        year: i32,
        reason: String,
        category: String,
        confidence: f64,
    ) {
        if let Some(existing) = self
            .flags
            .iter_mut()
            .find(|flag| flag.tx_id == tx_id && flag.status == FlagStatus::Open)
        {
            existing.reason = reason;
            existing.category = category;
            existing.confidence = confidence;
            existing.year = year;
            return;
        }

        self.flags.push(ReviewFlag {
            tx_id,
            year,
            status: FlagStatus::Open,
            reason,
            category,
            confidence,
        });
    }

    /// Transition a flag from Open to Resolved. Returns `true` if the flag was found and updated.
    pub fn resolve_flag(&mut self, tx_id: &str) -> bool {
        if let Some(flag) = self
            .flags
            .iter_mut()
            .find(|f| f.tx_id == tx_id && f.status == FlagStatus::Open)
        {
            flag.status = FlagStatus::Resolved;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_flag_transitions_open_to_resolved() {
        let mut engine = ClassificationEngine::default();
        engine.record_review_flag(
            "tx-abc".to_string(),
            "2024-06-01",
            "needs review".to_string(),
            "Other".to_string(),
            0.5,
        );
        assert_eq!(engine.query_flags(2024, FlagStatus::Open).len(), 1);
        assert!(engine.resolve_flag("tx-abc"));
        assert_eq!(engine.query_flags(2024, FlagStatus::Open).len(), 0);
        assert_eq!(engine.query_flags(2024, FlagStatus::Resolved).len(), 1);
    }

    #[test]
    fn resolve_flag_returns_false_when_not_found() {
        let mut engine = ClassificationEngine::default();
        assert!(!engine.resolve_flag("no-such-tx"));
    }

    #[test]
    fn resolve_flag_ignores_already_resolved() {
        let mut engine = ClassificationEngine::default();
        engine.record_review_flag(
            "tx-xyz".to_string(),
            "2024-03-15",
            "check".to_string(),
            "Income".to_string(),
            0.7,
        );
        assert!(engine.resolve_flag("tx-xyz"));
        assert!(!engine.resolve_flag("tx-xyz"), "second resolve should return false");
    }
}

fn run_classify_fn(
    engine: &Engine,
    ast: &AST,
    sample: &SampleTransaction,
) -> Result<ClassificationOutcome, ClassificationError> {
    let mut scope = Scope::new();
    let tx_map = sample_to_map(sample);
    let output: Map = engine.call_fn(&mut scope, ast, "classify", (tx_map,))?;

    let category = map_string(&output, "category")?;
    let confidence = map_float(&output, "confidence")?;
    let needs_review = map_bool(&output, "review")?;
    let reason = map_string(&output, "reason")?;

    Ok(ClassificationOutcome {
        category,
        confidence,
        needs_review,
        reason,
    })
}

fn sample_to_map(sample: &SampleTransaction) -> Map {
    let mut tx = Map::new();
    tx.insert("tx_id".into(), Dynamic::from(sample.tx_id.clone()));
    tx.insert(
        "account_id".into(),
        Dynamic::from(sample.account_id.clone()),
    );
    tx.insert("date".into(), Dynamic::from(sample.date.clone()));
    tx.insert("amount".into(), Dynamic::from(sample.amount.clone()));
    tx.insert(
        "description".into(),
        Dynamic::from(sample.description.clone()),
    );
    tx
}

fn map_string(map: &Map, key: &'static str) -> Result<String, ClassificationError> {
    map.get(key)
        .and_then(|v| v.clone().try_cast::<String>())
        .ok_or(ClassificationError::InvalidOutput(key))
}

fn map_float(map: &Map, key: &'static str) -> Result<f64, ClassificationError> {
    map.get(key)
        .and_then(|v| v.clone().try_cast::<f64>())
        .ok_or(ClassificationError::InvalidOutput(key))
}

fn map_bool(map: &Map, key: &'static str) -> Result<bool, ClassificationError> {
    map.get(key)
        .and_then(|v| v.clone().try_cast::<bool>())
        .ok_or(ClassificationError::InvalidOutput(key))
}

fn derive_year(date: &str) -> i32 {
    date.split('-')
        .next()
        .and_then(|y| y.parse::<i32>().ok())
        .unwrap_or(0)
}
