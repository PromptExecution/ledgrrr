//! AU R&D Tax Incentive types and constraint satisfaction.
//!
//! Implements ITAA 1997 Division 355 (R&D Tax Incentive):
//! - s.355-100: Core and Supporting R&D activities (eligibility criteria)
//! - s.355-305: R&D expenditure classification
//! - s.355-100(2): Systematic hypothesis-driven experimental activities
//! - s.355-100(4): Technical uncertainty requirement
//!
//! Each type implements `Satisfies<AuRdEligibility>` from `ufo-types`,
//! producing evidence-bearing `SatisfiesResult` values suitable for
//! ATO self-assessment and audit documentation.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use ufo_types::{
    satisfies::{Constraint, Disposition, NodeId, SatisfiesResult, Satisfies},
    ufo::MomentStereotype,
    iso::{Currency, Lei},
};

// ─── Constraint markers ────────────────────────────────────────────────────

/// Constraint: ITAA 1997 s.355-100 activity eligibility.
///
/// Tests: systematic/hypothesis-driven (s.355-100(2)) +
///        technical uncertainty (s.355-100(4)).
pub struct AuRdEligibility;
impl Constraint for AuRdEligibility {}

/// Constraint: AuRdOffset self-assessment completeness.
pub struct AuRdCompliance;
impl Constraint for AuRdCompliance {}

// ─── Activity ──────────────────────────────────────────────────────────────

/// Type of R&D activity under Division 355.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityType {
    /// Core R&D: hypothesis-driven experiment with technical uncertainty (s.355-100(2)).
    Core,
    /// Supporting R&D: directly supports a Core activity (s.355-100(1)(b)).
    Supporting,
}

/// An R&D activity registered (or eligible to be registered) with AusIndustry.
///
/// UFO stereotype: `Kind` (the activity is an independent sortable entity).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuRdActivity {
    /// ISO 17442 LEI of the R&D entity conducting the activity.
    pub lei: Lei,
    /// Internal unique identifier for this activity.
    pub activity_id: String,
    /// Human-readable description submitted to AusIndustry.
    pub activity_name: String,
    /// Core or Supporting classification.
    pub activity_type: ActivityType,
    /// ANZSIC 2006 industry code of the conducting entity.
    pub anzsic_code: String,
    /// Start of the R&D income year this activity covers.
    pub period_start: NaiveDate,
    /// End of the R&D income year.
    pub period_end: NaiveDate,
    /// Whether the experiment had a documented hypothesis prior to commencement.
    pub has_hypothesis: bool,
    /// Whether the outcome was uncertain at the time of commencement.
    pub has_technical_uncertainty: bool,
    /// Whether a systematic investigative process was followed.
    pub is_systematic: bool,
}

impl AuRdActivity {
    fn evidence_node(&self) -> NodeId {
        let hash = blake3::hash(
            format!("{}:{}", self.lei, self.activity_id).as_bytes()
        ).to_hex().to_string();
        NodeId::new(format!("rnd:{hash}"))
    }
}

impl Satisfies<AuRdEligibility> for AuRdActivity {
    fn satisfies(&self, _constraint: &AuRdEligibility) -> SatisfiesResult {
        let node = self.evidence_node();

        // s.355-100(2): must be systematic + hypothesis-driven
        if !self.is_systematic || !self.has_hypothesis {
            return SatisfiesResult {
                disposition: Disposition::Violated {
                    reason: format!(
                        "Activity '{}' does not satisfy s.355-100(2): \
                         systematic={}, has_hypothesis={}",
                        self.activity_id, self.is_systematic, self.has_hypothesis
                    ),
                },
                confidence: 0.0,
                evidence_nodes: vec![node],
                ufo_category: MomentStereotype::Mode,
            };
        }

        // s.355-100(4): technical uncertainty required for Core activities
        if self.activity_type == ActivityType::Core && !self.has_technical_uncertainty {
            return SatisfiesResult {
                disposition: Disposition::Violated {
                    reason: format!(
                        "Core activity '{}' does not satisfy s.355-100(4): \
                         technical uncertainty not established",
                        self.activity_id
                    ),
                },
                confidence: 0.0,
                evidence_nodes: vec![node],
                ufo_category: MomentStereotype::Mode,
            };
        }

        let confidence = match self.activity_type {
            ActivityType::Core => 0.92,
            ActivityType::Supporting => 0.80,
        };

        SatisfiesResult {
            disposition: Disposition::Satisfied,
            confidence,
            evidence_nodes: vec![node],
            ufo_category: MomentStereotype::Relator,
        }
    }
}

// ─── Expenditure ──────────────────────────────────────────────────────────

/// Category of R&D expenditure under s.355-305 ITAA 1997.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpenditureCategory {
    /// s.355-305(1)(a): contractor/consultant fees.
    Contractor,
    /// s.355-305(1)(b): salary & wages for R&D staff.
    Salary,
    /// s.355-305(1)(c): feedstock (materials consumed in the R&D process).
    Feedstock,
    /// s.355-305(1)(d): decline in value of R&D equipment.
    DeclineInValue,
    /// Other expenditure directly incurred on R&D activities.
    Other,
}

impl ExpenditureCategory {
    /// Whether this category is directly eligible without further apportionment.
    pub fn is_directly_eligible(self) -> bool {
        !matches!(self, ExpenditureCategory::Other)
    }

    /// ITAA 1997 section reference for this expenditure type.
    pub fn section_ref(self) -> &'static str {
        match self {
            ExpenditureCategory::Contractor => "s.355-305(1)(a)",
            ExpenditureCategory::Salary => "s.355-305(1)(b)",
            ExpenditureCategory::Feedstock => "s.355-305(1)(c)",
            ExpenditureCategory::DeclineInValue => "s.355-305(1)(d)",
            ExpenditureCategory::Other => "s.355-305 (apportionment required)",
        }
    }
}

/// A single R&D expenditure item.
///
/// UFO stereotype: `Event` (a discrete financial transaction).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuRdExpenditure {
    /// Entity making the expenditure.
    pub lei: Lei,
    /// Transaction identifier from the ledger.
    pub tx_id: String,
    /// Expenditure category under s.355-305.
    pub category: ExpenditureCategory,
    /// Amount of the expenditure.
    pub amount: Decimal,
    /// Currency of the transaction.
    pub currency: Currency,
    /// Date the expenditure was incurred.
    pub date: NaiveDate,
    /// Link to the AuRdActivity this expenditure relates to.
    pub activity_id: String,
}

impl AuRdExpenditure {
    fn evidence_node(&self) -> NodeId {
        let hash = blake3::hash(
            format!("{}:{}", self.lei, self.tx_id).as_bytes()
        ).to_hex().to_string();
        NodeId::new(format!("tx:{hash}"))
    }
}

impl Satisfies<AuRdEligibility> for AuRdExpenditure {
    fn satisfies(&self, _constraint: &AuRdEligibility) -> SatisfiesResult {
        let node = self.evidence_node();

        if self.amount <= Decimal::ZERO {
            return SatisfiesResult {
                disposition: Disposition::Violated {
                    reason: format!(
                        "Expenditure {} has non-positive amount: {}",
                        self.tx_id, self.amount
                    ),
                },
                confidence: 0.0,
                evidence_nodes: vec![node],
                ufo_category: MomentStereotype::Mode,
            };
        }

        let confidence = if self.category.is_directly_eligible() { 0.95 } else { 0.60 };

        SatisfiesResult {
            disposition: Disposition::Satisfied,
            confidence,
            evidence_nodes: vec![node],
            ufo_category: MomentStereotype::Relator,
        }
    }
}

// ─── Offset ────────────────────────────────────────────────────────────────

/// AU R&D Tax Incentive offset calculation.
///
/// Offset rates (s.355-100(1)):
/// - 43.5% for aggregated turnover < AUD 20M (refundable)
/// - 38.5% for aggregated turnover ≥ AUD 20M (non-refundable)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuRdOffset {
    /// Total eligible R&D expenditure for the income year.
    pub total_eligible: Decimal,
    /// Applicable offset rate (0.435 or 0.385).
    pub offset_rate: Decimal,
    /// Estimated offset = total_eligible * offset_rate.
    pub estimated_offset: Decimal,
    /// Effective benefit after company tax adjustment.
    pub effective_benefit: Decimal,
    /// Whether this is a refundable offset (turnover < AUD 20M).
    pub is_refundable: bool,
}

impl AuRdOffset {
    /// Construct and compute derived fields.
    pub fn new(total_eligible: Decimal, is_refundable: bool) -> Self {
        let rate_str = if is_refundable { "0.435" } else { "0.385" };
        let offset_rate = Decimal::from_str_exact(rate_str).expect("static rate");
        let estimated_offset = total_eligible * offset_rate;
        // Effective benefit = offset - company tax at 25% * eligible expenditure
        let company_tax_rate = Decimal::from_str_exact("0.25").expect("static rate");
        let effective_benefit = estimated_offset - (total_eligible * company_tax_rate);
        Self {
            total_eligible,
            offset_rate,
            estimated_offset,
            effective_benefit,
            is_refundable,
        }
    }
}

impl Satisfies<AuRdCompliance> for AuRdOffset {
    fn satisfies(&self, _constraint: &AuRdCompliance) -> SatisfiesResult {
        if self.total_eligible <= Decimal::ZERO {
            return SatisfiesResult::violated("Total eligible expenditure must be positive");
        }
        if self.estimated_offset != self.total_eligible * self.offset_rate {
            return SatisfiesResult::violated(
                "estimated_offset inconsistent with total_eligible * offset_rate"
            );
        }
        SatisfiesResult::satisfied(0.98, vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use rust_decimal_macros::dec;

    fn test_lei() -> Lei {
        Lei::new("HWUPKR0MPOU8FGXBT394").unwrap_or_else(|_| {
            // Fallback: construct without validation for testing purposes
            Lei::new("HWUPKR0MPOU8FGXBT394").expect("test lei")
        })
    }

    fn core_activity() -> AuRdActivity {
        AuRdActivity {
            lei: test_lei(),
            activity_id: "ACT-001".to_string(),
            activity_name: "ML inference cost optimisation experiments".to_string(),
            activity_type: ActivityType::Core,
            anzsic_code: "7000".to_string(),
            period_start: NaiveDate::from_ymd_opt(2024, 7, 1).unwrap(),
            period_end: NaiveDate::from_ymd_opt(2025, 6, 30).unwrap(),
            has_hypothesis: true,
            has_technical_uncertainty: true,
            is_systematic: true,
        }
    }

    #[test]
    fn core_activity_satisfies_eligibility() {
        let activity = core_activity();
        let result = activity.satisfies(&AuRdEligibility);
        assert!(result.disposition.is_satisfied(), "Expected satisfied: {:?}", result.disposition);
        assert!(result.confidence >= 0.9);
    }

    #[test]
    fn core_activity_no_uncertainty_violated() {
        let mut activity = core_activity();
        activity.has_technical_uncertainty = false;
        let result = activity.satisfies(&AuRdEligibility);
        assert!(!result.disposition.is_satisfied());
        assert!(matches!(result.disposition, Disposition::Violated { .. }));
    }

    #[test]
    fn supporting_activity_no_uncertainty_ok() {
        let mut activity = core_activity();
        activity.activity_type = ActivityType::Supporting;
        activity.has_technical_uncertainty = false;
        let result = activity.satisfies(&AuRdEligibility);
        // Supporting activities don't require s.355-100(4)
        assert!(result.disposition.is_satisfied());
    }

    #[test]
    fn expenditure_contractor_eligible() {
        let exp = AuRdExpenditure {
            lei: test_lei(),
            tx_id: "TX-001".to_string(),
            category: ExpenditureCategory::Contractor,
            amount: dec!(120_000),
            currency: Currency::Aud,
            date: NaiveDate::from_ymd_opt(2024, 10, 15).unwrap(),
            activity_id: "ACT-001".to_string(),
        };
        let result = exp.satisfies(&AuRdEligibility);
        assert!(result.disposition.is_satisfied());
        assert!(result.confidence >= 0.9);
    }

    #[test]
    fn offset_refundable_43_5() {
        let offset = AuRdOffset::new(dec!(200_000), true);
        assert_eq!(offset.offset_rate, Decimal::from_str_exact("0.435").unwrap());
        let expected = dec!(200_000) * Decimal::from_str_exact("0.435").unwrap();
        assert_eq!(offset.estimated_offset, expected);
        assert!(offset.satisfies(&AuRdCompliance).disposition.is_satisfied());
    }

    #[test]
    fn expenditure_section_refs() {
        assert_eq!(ExpenditureCategory::Salary.section_ref(), "s.355-305(1)(b)");
        assert!(ExpenditureCategory::Contractor.is_directly_eligible());
        assert!(!ExpenditureCategory::Other.is_directly_eligible());
    }
}
