//! # ledgerr-focus — FOCUS (FinOps Cost Usage Specification) v1.3
//!
//! Typed cost/usage records conforming to the FOCUS v1.3 schema.
//! Implements the Cost and Usage dataset with Arrow RecordBatch serde
//! and gRPC transport contract.
//!
//! ## FOCUS v1.3 alignment
//! - CostAndUsage dataset: BilledCost, EffectiveCost, ConsumedQuantity, ChargeCategory, etc.
//! - ContractCommitment dataset: ContractCommitmentCost, ContractCommitmentID, etc.
//! - Custom columns via `x_` prefix for experiment-specific extensions
//! - All monetary values: `rust_decimal::Decimal`
//!
//! ## ledgerr extensions (x_ custom columns)
//! - `x_ExperimentId` — experiment identifier
//! - `x_Variant` — experiment variant (control/treatment)
//! - `x_Personality` — psychometric personality label
//! - `x_ExperimentScore` — overall experiment score (0.0–1.0)
//! - `x_AgentId` — executing agent identifier
//! - `x_ReasoningReview` — reasoning reviewer verdict
//!
//! ## Transport
//! - [`CostAndUsageBatch`]: FOCUS Cost and Usage Arrow RecordBatch
//! - [`ContractCommitmentBatch`]: FOCUS Contract Commitment Arrow RecordBatch
//! - gRPC via tonic + Apache Arrow DataFrames (DataFusion compatible)

use chrono::{DateTime, Utc};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Version ──────────────────────────────────────────────────────────────────

pub const FOCUS_SPEC_VERSION: &str = "1.3";

// ── Dimension constants (FOCUS v1.3 Cost and Usage dataset) ──────────────────

/// Standard FOCUS charge categories
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChargeCategory {
    Usage,
    Purchase,
    Tax,
    Credit,
    Adjustment,
}

impl ChargeCategory {
    pub const ALL: &'static [Self; 5] = &[
        Self::Usage,
        Self::Purchase,
        Self::Tax,
        Self::Credit,
        Self::Adjustment,
    ];
}

/// Standard FOCUS charge classes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChargeClass {
    Correction,
}

/// Standard FOCUS charge frequencies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChargeFrequency {
    OneTime,
    Recurring,
    UsageBased,
}

/// Standard FOCUS pricing categories
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PricingCategory {
    Standard,
    Tiered,
    Volume,
    Spot,
    Reserved,
    SavingsPlan,
    CommitmentDiscount,
    Contracted,
}

// ── FOCUS Cost and Usage dataset row ─────────────────────────────────────────

/// A single row in the FOCUS Cost and Usage dataset (v1.3).
///
/// Maps to one row in an Arrow RecordBatch.
/// All monetary fields use `rust_decimal::Decimal` per FOCUS spec.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CostAndUsageRow {
    // ── Mandatory columns ───────────────────────────────────────────────
    pub billing_account_id: String,
    pub billing_account_name: Option<String>,
    pub billing_currency: String,
    pub billing_period_start: DateTime<Utc>,
    pub billing_period_end: DateTime<Utc>,
    pub charge_period_start: DateTime<Utc>,
    pub charge_period_end: DateTime<Utc>,
    pub charge_category: ChargeCategory,
    pub charge_frequency: ChargeFrequency,
    pub billed_cost: Decimal,
    pub effective_cost: Decimal,
    pub service_provider_name: String,
    pub service_name: String,
    pub sku_id: String,

    // ── Conditional columns ─────────────────────────────────────────────
    pub billing_account_type: Option<String>,
    pub charge_class: Option<ChargeClass>,
    pub charge_description: Option<String>,
    pub commitment_discount_id: Option<String>,
    pub commitment_discount_name: Option<String>,
    pub commitment_discount_category: Option<String>,
    pub commitment_discount_type: Option<String>,
    pub commitment_discount_status: Option<String>,
    pub commitment_discount_quantity: Option<Decimal>,
    pub commitment_discount_unit: Option<String>,
    pub consumed_quantity: Option<Decimal>,
    pub consumed_unit: Option<String>,
    pub contracted_cost: Option<Decimal>,
    pub contracted_unit_price: Option<Decimal>,
    pub invoice_id: Option<String>,
    pub invoice_issuer_name: Option<String>,
    pub list_cost: Option<Decimal>,
    pub list_unit_price: Option<Decimal>,
    pub pricing_category: Option<PricingCategory>,
    pub pricing_quantity: Option<Decimal>,
    pub pricing_unit: Option<String>,
    pub region_id: Option<String>,
    pub region_name: Option<String>,
    pub resource_id: Option<String>,
    pub resource_name: Option<String>,
    pub resource_type: Option<String>,
    pub service_category: Option<String>,
    pub service_subcategory: Option<String>,
    pub sku_meter: Option<String>,
    pub sku_price_id: Option<String>,
    pub sku_price_details: Option<String>,
    pub sub_account_id: Option<String>,
    pub sub_account_name: Option<String>,
    pub sub_account_type: Option<String>,
    pub availability_zone: Option<String>,
    pub capacity_reservation_id: Option<String>,
    pub capacity_reservation_status: Option<String>,
    pub host_provider_name: Option<String>,

    // ── Tags (key-value metadata) ───────────────────────────────────────
    pub tags: HashMap<String, String>,

    // ── ledgerr custom columns (x_ prefix per FOCUS extensibility) ──────
    pub x_experiment_id: Option<String>,
    pub x_variant: Option<String>,
    pub x_personality: Option<String>,
    pub x_experiment_score: Option<Decimal>,
    pub x_agent_id: Option<String>,
    pub x_reasoning_review: Option<String>,
}

impl CostAndUsageRow {
    /// Summary convenience for experiment tracking
    pub fn experiment_summary(&self) -> String {
        format!(
            "exp={} variant={} billed={:.2} effective={:.2} qty={}",
            self.x_experiment_id.as_deref().unwrap_or("-"),
            self.x_variant.as_deref().unwrap_or("-"),
            self.billed_cost,
            self.effective_cost,
            self.consumed_quantity
                .map(|q| format!("{q:.2}"))
                .unwrap_or_else(|| "0".into()),
        )
    }
}

// ── FOCUS Contract Commitment dataset row (v1.3) ─────────────────────────────

/// A single row in the FOCUS Contract Commitment dataset.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContractCommitmentRow {
    pub billing_currency: String,
    pub contract_commitment_id: String,
    pub contract_commitment_cost: Decimal,
    pub contract_commitment_category: String,
    pub contract_commitment_description: String,
    pub contract_commitment_period_start: DateTime<Utc>,
    pub contract_commitment_period_end: DateTime<Utc>,
    pub contract_commitment_quantity: Decimal,
    pub contract_commitment_type: String,
    pub contract_commitment_unit: String,
    pub contract_id: String,
    pub contract_period_start: DateTime<Utc>,
    pub contract_period_end: DateTime<Utc>,
    pub service_provider_name: String,
}

// ── FocusDelta — diff between two FOCUS record sets ──────────────────────────

/// Represents the difference between two sets of FOCUS records.
/// Used by the reasoning reviewer to compare control vs treatment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FocusDelta {
    pub experiment_id: String,
    pub control_billed_cost: Decimal,
    pub treatment_billed_cost: Decimal,
    pub control_effective_cost: Decimal,
    pub treatment_effective_cost: Decimal,
    pub delta_billed: Decimal,
    pub delta_effective: Decimal,
    pub dimension_deltas: HashMap<String, Decimal>,
    pub recommendation: String,
}

// ── ExperimentScore — ledgrrr extension ──────────────────────────────────────

/// Scored experiment outcome. Uses the `x_` FOCUS custom column convention.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExperimentScore {
    pub experiment_id: String,
    pub variant: String,
    pub personality: Option<String>,
    pub status: String,
    /// Scored dimensions: roi, cost, time, accuracy, utility, risk
    pub scores: HashMap<String, Decimal>,
    pub duration_ms: u64,
    pub token_cost: u64,
    pub reasoning: String,
}

// ── Personality profiles for psychometric experiments ────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PersonalityProfile {
    pub label: String,
    pub traits: HashMap<String, Decimal>,
}

impl PersonalityProfile {
    pub fn analyst() -> Self {
        let mut traits = HashMap::new();
        traits.insert("conscientiousness".into(), dec!(0.85));
        traits.insert("openness".into(), dec!(0.60));
        traits.insert("extraversion".into(), dec!(0.30));
        traits.insert("agreeableness".into(), dec!(0.50));
        traits.insert("neuroticism".into(), dec!(0.20));
        Self {
            label: "analyst".into(),
            traits,
        }
    }

    pub fn explorer() -> Self {
        let mut traits = HashMap::new();
        traits.insert("conscientiousness".into(), dec!(0.55));
        traits.insert("openness".into(), dec!(0.90));
        traits.insert("extraversion".into(), dec!(0.70));
        traits.insert("agreeableness".into(), dec!(0.60));
        traits.insert("neuroticism".into(), dec!(0.35));
        Self {
            label: "explorer".into(),
            traits,
        }
    }

    pub fn guardian() -> Self {
        let mut traits = HashMap::new();
        traits.insert("conscientiousness".into(), dec!(0.95));
        traits.insert("openness".into(), dec!(0.30));
        traits.insert("extraversion".into(), dec!(0.40));
        traits.insert("agreeableness".into(), dec!(0.75));
        traits.insert("neuroticism".into(), dec!(0.50));
        Self {
            label: "guardian".into(),
            traits,
        }
    }

    pub fn all() -> Vec<Self> {
        vec![Self::analyst(), Self::explorer(), Self::guardian()]
    }
}

// ── t00n TOON format serializer (always available) ───────────────────────────

pub mod t00n;

// ── Arrow RecordBatch serde (feature-gated behind "arrow-serde") ─────────────

#[cfg(feature = "arrow-serde")]
pub mod arrow_serde {
    use super::*;
    use arrow::array::{
        ArrayRef, Decimal128Array, Float64Array, StringArray, TimestampNanosecondArray,
    };
    use arrow::datatypes::{DataType, Field, Schema, SchemaRef, TimeUnit};
    use arrow::record_batch::RecordBatch;
    use std::sync::Arc;

    /// Precision and scale used for FOCUS monetary Decimal128 columns.
    /// Scale 9 preserves nanosecond-level decimal precision (9 decimal places).
    pub const DECIMAL_PRECISION: u8 = 38;
    pub const DECIMAL_SCALE: i8 = 9;

    /// Convert a `rust_decimal::Decimal` to `i128` at the Arrow Decimal128 scale.
    fn decimal_to_i128(d: &Decimal) -> i128 {
        let src_scale = d.scale() as i64;
        let dst_scale = DECIMAL_SCALE as i64;
        let mantissa = d.mantissa();
        let diff = dst_scale - src_scale;
        if diff >= 0 {
            mantissa.saturating_mul(10i128.pow(diff as u32))
        } else {
            mantissa / 10i128.pow((-diff) as u32)
        }
    }

    /// Build a minimal Arrow Schema for the FOCUS Cost and Usage dataset.
    ///
    /// This schema covers the mandatory columns and ledgerr x_ extension columns.
    /// The full FOCUS v1.3 Cost and Usage dataset contains additional conditional
    /// columns not included here.
    pub fn core_cost_and_usage_schema() -> SchemaRef {
        let ts_ns = DataType::Timestamp(TimeUnit::Nanosecond, None);
        let dec128 = DataType::Decimal128(DECIMAL_PRECISION, DECIMAL_SCALE);
        Arc::new(Schema::new(vec![
            // Mandatory
            Field::new("BillingAccountId", DataType::Utf8, false),
            Field::new("BillingAccountName", DataType::Utf8, true),
            Field::new("BillingCurrency", DataType::Utf8, false),
            Field::new("BillingPeriodStart", ts_ns.clone(), false),
            Field::new("BillingPeriodEnd", ts_ns.clone(), false),
            Field::new("ChargePeriodStart", ts_ns.clone(), false),
            Field::new("ChargePeriodEnd", ts_ns, false),
            Field::new("ChargeCategory", DataType::Utf8, false),
            Field::new("ChargeFrequency", DataType::Utf8, false),
            Field::new("BilledCost", dec128.clone(), false),
            Field::new("EffectiveCost", dec128, false),
            Field::new("ServiceProviderName", DataType::Utf8, false),
            Field::new("ServiceName", DataType::Utf8, false),
            Field::new("SkuId", DataType::Utf8, false),
            // ledgerr custom columns (x_ prefix)
            Field::new("x_ExperimentId", DataType::Utf8, true),
            Field::new("x_Variant", DataType::Utf8, true),
            Field::new("x_Personality", DataType::Utf8, true),
            Field::new("x_ExperimentScore", DataType::Float64, true),
            Field::new("x_AgentId", DataType::Utf8, true),
            Field::new("x_ReasoningReview", DataType::Utf8, true),
        ]))
    }

    /// Convert a minimal set of FOCUS CostAndUsageRow fields to an Arrow RecordBatch.
    ///
    /// Covers mandatory columns and ledgerr x_ extensions. Nullable string fields
    /// are represented as proper Arrow nulls (not empty strings).
    pub fn core_rows_to_batch(rows: &[CostAndUsageRow]) -> RecordBatch {
        let schema = core_cost_and_usage_schema();
        let batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(StringArray::from(
                    rows.iter()
                        .map(|r| r.billing_account_id.as_str())
                        .collect::<Vec<_>>(),
                )) as ArrayRef,
                Arc::new(StringArray::from(
                    rows.iter()
                        .map(|r| r.billing_account_name.as_deref())
                        .collect::<Vec<Option<&str>>>(),
                )),
                Arc::new(StringArray::from(
                    rows.iter()
                        .map(|r| r.billing_currency.as_str())
                        .collect::<Vec<_>>(),
                )),
                Arc::new(TimestampNanosecondArray::from(
                    rows.iter()
                        .map(|r| r.billing_period_start.timestamp_nanos_opt())
                        .collect::<Vec<_>>(),
                )),
                Arc::new(TimestampNanosecondArray::from(
                    rows.iter()
                        .map(|r| r.billing_period_end.timestamp_nanos_opt())
                        .collect::<Vec<_>>(),
                )),
                Arc::new(TimestampNanosecondArray::from(
                    rows.iter()
                        .map(|r| r.charge_period_start.timestamp_nanos_opt())
                        .collect::<Vec<_>>(),
                )),
                Arc::new(TimestampNanosecondArray::from(
                    rows.iter()
                        .map(|r| r.charge_period_end.timestamp_nanos_opt())
                        .collect::<Vec<_>>(),
                )),
                Arc::new(StringArray::from(
                    rows.iter()
                        .map(|r| format!("{:?}", r.charge_category))
                        .collect::<Vec<_>>(),
                )),
                Arc::new(StringArray::from(
                    rows.iter()
                        .map(|r| format!("{:?}", r.charge_frequency))
                        .collect::<Vec<_>>(),
                )),
                Arc::new(
                    Decimal128Array::from(
                        rows.iter()
                            .map(|r| decimal_to_i128(&r.billed_cost))
                            .collect::<Vec<_>>(),
                    )
                    .with_precision_and_scale(DECIMAL_PRECISION, DECIMAL_SCALE)
                    .expect("BilledCost Decimal128 precision/scale is valid"),
                ),
                Arc::new(
                    Decimal128Array::from(
                        rows.iter()
                            .map(|r| decimal_to_i128(&r.effective_cost))
                            .collect::<Vec<_>>(),
                    )
                    .with_precision_and_scale(DECIMAL_PRECISION, DECIMAL_SCALE)
                    .expect("EffectiveCost Decimal128 precision/scale is valid"),
                ),
                Arc::new(StringArray::from(
                    rows.iter()
                        .map(|r| r.service_provider_name.as_str())
                        .collect::<Vec<_>>(),
                )),
                Arc::new(StringArray::from(
                    rows.iter()
                        .map(|r| r.service_name.as_str())
                        .collect::<Vec<_>>(),
                )),
                Arc::new(StringArray::from(
                    rows.iter().map(|r| r.sku_id.as_str()).collect::<Vec<_>>(),
                )),
                Arc::new(StringArray::from(
                    rows.iter()
                        .map(|r| r.x_experiment_id.as_deref())
                        .collect::<Vec<Option<&str>>>(),
                )),
                Arc::new(StringArray::from(
                    rows.iter()
                        .map(|r| r.x_variant.as_deref())
                        .collect::<Vec<Option<&str>>>(),
                )),
                Arc::new(StringArray::from(
                    rows.iter()
                        .map(|r| r.x_personality.as_deref())
                        .collect::<Vec<Option<&str>>>(),
                )),
                Arc::new(Float64Array::from(
                    rows.iter()
                        .map(|r| r.x_experiment_score.and_then(|s| s.to_f64()))
                        .collect::<Vec<Option<f64>>>(),
                )),
                Arc::new(StringArray::from(
                    rows.iter()
                        .map(|r| r.x_agent_id.as_deref())
                        .collect::<Vec<Option<&str>>>(),
                )),
                Arc::new(StringArray::from(
                    rows.iter()
                        .map(|r| r.x_reasoning_review.as_deref())
                        .collect::<Vec<Option<&str>>>(),
                )),
            ],
        )
        .expect("FOCUS core CostAndUsage Arrow RecordBatch construction failed");
        batch
    }
}

// ── Error types ──────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum FocusError {
    #[error("Invalid FOCUS record: {0}")]
    InvalidRecord(String),
    #[cfg(feature = "arrow-serde")]
    #[error("Arrow serialization: {0}")]
    ArrowError(#[from] arrow::error::ArrowError),
    #[error("Decimal conversion: {0}")]
    DecimalError(#[from] rust_decimal::Error),
}

// ── Macros ───────────────────────────────────────────────────────────────────

macro_rules! dec {
    ($val:literal) => {
        rust_decimal::Decimal::try_from($val).unwrap_or(rust_decimal::Decimal::ZERO)
    };
}
pub(crate) use dec;

// ── Utility ──────────────────────────────────────────────────────────────────

pub fn compute_focus_delta(
    control: &[CostAndUsageRow],
    treatment: &[CostAndUsageRow],
    dimension_scores_control: &HashMap<String, Decimal>,
    dimension_scores_treatment: &HashMap<String, Decimal>,
    experiment_id: &str,
) -> FocusDelta {
    let control_billed: Decimal = control.iter().map(|r| r.billed_cost).sum();
    let treatment_billed: Decimal = treatment.iter().map(|r| r.billed_cost).sum();
    let control_effective: Decimal = control.iter().map(|r| r.effective_cost).sum();
    let treatment_effective: Decimal = treatment.iter().map(|r| r.effective_cost).sum();

    let mut dimension_deltas = HashMap::new();
    for (dim, _) in dimension_scores_control.iter() {
        let c = dimension_scores_control
            .get(dim)
            .copied()
            .unwrap_or(Decimal::ZERO);
        let t = dimension_scores_treatment
            .get(dim)
            .copied()
            .unwrap_or(Decimal::ZERO);
        let delta = match dim.as_str() {
            "risk" | "cost" | "time" => c - t,
            _ => t - c,
        };
        dimension_deltas.insert(dim.clone(), delta);
    }

    let c_roi = dimension_scores_control
        .get("roi")
        .copied()
        .unwrap_or(Decimal::ZERO);
    let t_roi = dimension_scores_treatment
        .get("roi")
        .copied()
        .unwrap_or(Decimal::ZERO);
    let recommendation = if t_roi > c_roi {
        "treatment"
    } else {
        "control"
    };

    FocusDelta {
        experiment_id: experiment_id.to_string(),
        control_billed_cost: control_billed,
        treatment_billed_cost: treatment_billed,
        control_effective_cost: control_effective,
        treatment_effective_cost: treatment_effective,
        delta_billed: treatment_billed - control_billed,
        delta_effective: treatment_effective - control_effective,
        dimension_deltas,
        recommendation: recommendation.to_string(),
    }
}

impl FocusDelta {
    pub fn to_focus_cli(&self) -> String {
        let dims: Vec<String> = self
            .dimension_deltas
            .iter()
            .map(|(k, v)| format!("{k}={v:.2}"))
            .collect();
        format!(
            "ledgrrr focus delta --experiment={} --control_billed={:.2} --treatment_billed={:.2} --delta_billed={:.2} --recommend={} --dims=\"{}\"",
            self.experiment_id,
            self.control_billed_cost,
            self.treatment_billed_cost,
            self.delta_billed,
            self.recommendation,
            dims.join(","),
        )
    }
}

pub fn format_focus_cli(row: &CostAndUsageRow) -> String {
    format!(
        "ledgrrr focus append --billing_account={} --service={} --billed={:.2} --effective={:.2} --cat={:?} --exp={} --variant={}",
        row.billing_account_id,
        row.service_name,
        row.billed_cost,
        row.effective_cost,
        row.charge_category,
        row.x_experiment_id.as_deref().unwrap_or(""),
        row.x_variant.as_deref().unwrap_or(""),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::prelude::FromPrimitive;

    fn sample_row(experiment_id: &str, variant: &str, cost: f64) -> CostAndUsageRow {
        CostAndUsageRow {
            billing_account_id: "b00t-hive".into(),
            billing_account_name: Some("b00t Hive".into()),
            billing_currency: "FOCUS".into(),
            billing_period_start: DateTime::from_timestamp_nanos(0),
            billing_period_end: DateTime::from_timestamp_nanos(1_000_000_000),
            charge_period_start: DateTime::from_timestamp_nanos(0),
            charge_period_end: DateTime::from_timestamp_nanos(1_000_000_000),
            charge_category: ChargeCategory::Usage,
            charge_frequency: ChargeFrequency::UsageBased,
            billed_cost: Decimal::from_f64(cost).unwrap_or_default(),
            effective_cost: Decimal::from_f64(cost * 0.85).unwrap_or_default(),
            service_provider_name: "ledgrrr".into(),
            service_name: "experiment-eval".into(),
            sku_id: "exp-eval-sm0l".into(),
            billing_account_type: None,
            charge_class: None,
            charge_description: None,
            commitment_discount_id: None,
            commitment_discount_name: None,
            commitment_discount_category: None,
            commitment_discount_type: None,
            commitment_discount_status: None,
            commitment_discount_quantity: None,
            commitment_discount_unit: None,
            consumed_quantity: Some(Decimal::from_f64(1.0).unwrap()),
            consumed_unit: Some("experiment".into()),
            contracted_cost: None,
            contracted_unit_price: None,
            invoice_id: None,
            invoice_issuer_name: None,
            list_cost: None,
            list_unit_price: None,
            pricing_category: None,
            pricing_quantity: None,
            pricing_unit: None,
            region_id: None,
            region_name: None,
            resource_id: None,
            resource_name: None,
            resource_type: None,
            service_category: Some("AI Inference".into()),
            service_subcategory: None,
            sku_meter: None,
            sku_price_id: None,
            sku_price_details: None,
            sub_account_id: None,
            sub_account_name: None,
            sub_account_type: None,
            availability_zone: None,
            capacity_reservation_id: None,
            capacity_reservation_status: None,
            host_provider_name: None,
            tags: HashMap::new(),
            x_experiment_id: Some(experiment_id.into()),
            x_variant: Some(variant.into()),
            x_personality: Some("analyst".into()),
            x_experiment_score: Some(Decimal::from_f64(0.85).unwrap()),
            x_agent_id: Some(format!("sm0l-{variant}")),
            x_reasoning_review: Some("performed adequately".into()),
        }
    }

    #[test]
    fn test_cost_and_usage_row_creation() {
        let row = sample_row("exp-001", "control", 100.0);
        assert_eq!(row.billing_account_id, "b00t-hive");
        assert_eq!(row.charge_category, ChargeCategory::Usage);
        assert_eq!(row.x_experiment_id, Some("exp-001".into()));
        assert_eq!(row.x_variant, Some("control".into()));
    }

    #[test]
    fn test_experiment_summary() {
        let row = sample_row("exp-001", "control", 100.0);
        let summary = row.experiment_summary();
        assert!(summary.contains("exp=exp-001"));
        assert!(summary.contains("variant=control"));
        assert!(summary.contains("billed=100"));
    }

    #[test]
    fn test_format_focus_cli() {
        let row = sample_row("exp-001", "treatment", 150.0);
        let cli = format_focus_cli(&row);
        assert!(cli.starts_with("ledgrrr focus append"));
        assert!(cli.contains("--billing_account=b00t-hive"));
        assert!(cli.contains("--exp=exp-001"));
        assert!(cli.contains("--variant=treatment"));
    }

    #[test]
    fn test_compute_focus_delta() {
        let control = vec![sample_row("exp-001", "control", 100.0)];
        let treatment = vec![sample_row("exp-001", "treatment", 150.0)];

        let mut cs = HashMap::new();
        let mut ts = HashMap::new();
        cs.insert("roi".into(), dec!(0.5));
        ts.insert("roi".into(), dec!(0.8));

        let delta = compute_focus_delta(&control, &treatment, &cs, &ts, "exp-001");
        assert_eq!(delta.experiment_id, "exp-001");
        assert!(delta.delta_billed > dec!(0)); // treatment cost more
        assert_eq!(delta.recommendation, "treatment"); // treatment roi higher
    }

    #[test]
    fn test_personality_profiles() {
        let profiles = PersonalityProfile::all();
        assert_eq!(profiles.len(), 3);
        assert!(profiles.iter().any(|p| p.label == "analyst"));
        assert!(profiles.iter().any(|p| p.label == "explorer"));
    }

    #[test]
    fn test_charge_category_enum() {
        assert_eq!(ChargeCategory::ALL.len(), 5);
        assert_eq!(format!("{:?}", ChargeCategory::Usage), "Usage");
        assert_eq!(format!("{:?}", ChargeCategory::Purchase), "Purchase");
    }

    #[cfg(feature = "arrow-serde")]
    #[test]
    fn test_focus_arrow_record_batch() {
        use arrow_serde::*;
        let rows = vec![
            sample_row("exp-001", "control", 100.0),
            sample_row("exp-001", "treatment", 150.0),
        ];
        let batch = core_rows_to_batch(&rows);
        assert_eq!(batch.num_rows(), 2);
        assert_eq!(batch.num_columns(), 20);

        let schema = batch.schema();
        assert_eq!(schema.field(0).name(), "BillingAccountId");
        assert_eq!(schema.field(14).name(), "x_ExperimentId");
        assert_eq!(schema.field(19).name(), "x_ReasoningReview");
    }
}
