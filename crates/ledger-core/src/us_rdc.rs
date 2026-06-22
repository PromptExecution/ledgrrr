//! US R&D Tax Credit types and constraint satisfaction.
//!
//! Implements IRC Section 41 (Research and Experimentation Tax Credit):
//! - IRC § 41(d)(1): 4-part test for Qualified Research Activity (QRA)
//! - IRC § 41(b): Qualified Research Expenditure (QRE) categories
//! - IRC § 41(a)(1): Traditional credit method (20% of excess QREs)
//! - IRC § 41(c)(5): Alternative Simplified Credit (ASC, 14%)
//!
//! Evidence nodes follow the same Blake3-hashed identity pattern as AU R&D.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use ufo_types::{
    satisfies::{Constraint, Disposition, NodeId, SatisfiesResult, Satisfies},
    ufo::MomentStereotype,
    iso::{Currency, Lei},
};

// ─── Constraint markers ────────────────────────────────────────────────────

/// Constraint: IRC § 41(d)(1) 4-part test for Qualified Research Activity.
///
/// All four conditions must be satisfied:
/// 1. Technological in nature (§ 41(d)(1)(A))
/// 2. Permitted purpose — new/improved function, performance, reliability, quality (§ 41(d)(1)(B))
/// 3. Technological uncertainty (§ 41(d)(1)(C))
/// 4. Process of experimentation (§ 41(d)(1)(D))
pub struct UsRdcFourPartTest;
impl Constraint for UsRdcFourPartTest {}

// ─── QreActivity ───────────────────────────────────────────────────────────

/// A Qualified Research Activity under IRC § 41(d).
///
/// UFO stereotype: `Kind` (the activity is an independent sortable entity).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QreActivity {
    /// ISO 17442 LEI of the US entity claiming the credit.
    pub lei: Lei,
    /// Internal unique identifier for this activity.
    pub activity_id: String,
    /// Human-readable description of the research activity.
    pub activity_name: String,
    /// § 41(d)(1)(A): Activity relies on hard sciences, engineering, or computer science.
    pub technical_in_nature: bool,
    /// § 41(d)(1)(B): Activity aims to develop/improve function, performance, reliability, or quality.
    pub permits_experimentation: bool,
    /// § 41(d)(1)(C): Uncertainty existed about the capability, method, or design at commencement.
    pub technological_uncertainty: bool,
    /// § 41(d)(1)(D): Activity uses a systematic process of evaluation (hypotheses + testing).
    pub systematic_process: bool,
}

impl QreActivity {
    fn evidence_node(&self) -> NodeId {
        let hash = blake3::hash(
            format!("us_rdc:{}:{}", self.lei, self.activity_id).as_bytes()
        ).to_hex().to_string();
        NodeId::new(format!("rnd:{hash}"))
    }
}

impl Satisfies<UsRdcFourPartTest> for QreActivity {
    fn satisfies(&self, _constraint: &UsRdcFourPartTest) -> SatisfiesResult {
        let node = self.evidence_node();

        // All four parts are conjunctive — any failure is a violation
        if !self.technical_in_nature {
            return SatisfiesResult {
                disposition: Disposition::Violated {
                    reason: format!(
                        "Activity '{}' fails IRC § 41(d)(1)(A): not technical in nature \
                         (must rely on hard sciences, engineering, or computer science)",
                        self.activity_id
                    ),
                },
                confidence: 0.0,
                evidence_nodes: vec![node],
                ufo_category: MomentStereotype::Mode,
            };
        }

        if !self.permits_experimentation {
            return SatisfiesResult {
                disposition: Disposition::Violated {
                    reason: format!(
                        "Activity '{}' fails IRC § 41(d)(1)(B): permitted purpose not met \
                         (must seek to improve function, performance, reliability, or quality)",
                        self.activity_id
                    ),
                },
                confidence: 0.0,
                evidence_nodes: vec![node],
                ufo_category: MomentStereotype::Mode,
            };
        }

        if !self.technological_uncertainty {
            return SatisfiesResult {
                disposition: Disposition::Violated {
                    reason: format!(
                        "Activity '{}' fails IRC § 41(d)(1)(C): no technological uncertainty \
                         established at commencement",
                        self.activity_id
                    ),
                },
                confidence: 0.0,
                evidence_nodes: vec![node],
                ufo_category: MomentStereotype::Mode,
            };
        }

        if !self.systematic_process {
            return SatisfiesResult {
                disposition: Disposition::Violated {
                    reason: format!(
                        "Activity '{}' fails IRC § 41(d)(1)(D): no systematic process of \
                         experimentation documented (hypothesis → test → evaluation loop required)",
                        self.activity_id
                    ),
                },
                confidence: 0.0,
                evidence_nodes: vec![node],
                ufo_category: MomentStereotype::Mode,
            };
        }

        SatisfiesResult {
            disposition: Disposition::Satisfied,
            confidence: 0.93,
            evidence_nodes: vec![node],
            ufo_category: MomentStereotype::Relator,
        }
    }
}

// ─── QreExpenditure ────────────────────────────────────────────────────────

/// QRE category under IRC § 41(b).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QreCategory {
    /// § 41(b)(2)(A): In-house research wages (W-2 employees doing qualified research).
    Wages,
    /// § 41(b)(2)(B): Supplies used or consumed in the conduct of qualified research.
    Supplies,
    /// § 41(b)(2)(C): 65% of amounts paid to external parties for qualified research.
    ContractResearch,
    /// § 41(b)(2)(B) extended: computer cloud/HPC time directly used in qualified research.
    ComputerTime,
}

impl QreCategory {
    /// Statutory inclusion rate: 100% for Wages/Supplies/ComputerTime, 65% for Contract.
    pub fn inclusion_rate(self) -> Decimal {
        match self {
            QreCategory::ContractResearch => Decimal::from_str_exact("0.65").expect("static"),
            _ => Decimal::ONE,
        }
    }

    /// IRC § 41 subsection reference.
    pub fn section_ref(self) -> &'static str {
        match self {
            QreCategory::Wages => "IRC § 41(b)(2)(A)",
            QreCategory::Supplies => "IRC § 41(b)(2)(B)",
            QreCategory::ContractResearch => "IRC § 41(b)(2)(C) — 65% of paid amount",
            QreCategory::ComputerTime => "IRC § 41(b)(2)(B) — computer time",
        }
    }
}

/// A single Qualified Research Expenditure.
///
/// UFO stereotype: `Event` (a discrete financial transaction).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QreExpenditure {
    /// Entity incurring the expenditure.
    pub lei: Lei,
    /// Transaction identifier from the ledger.
    pub tx_id: String,
    /// QRE category determining inclusion rate.
    pub category: QreCategory,
    /// Gross amount before applying inclusion rate.
    pub amount: Decimal,
    /// Currency (typically USD for US entities).
    pub currency: Currency,
}

impl QreExpenditure {
    /// Qualified amount after applying the statutory inclusion rate.
    pub fn qualified_amount(&self) -> Decimal {
        self.amount * self.category.inclusion_rate()
    }

    fn evidence_node(&self) -> NodeId {
        let hash = blake3::hash(
            format!("us_qre:{}:{}", self.lei, self.tx_id).as_bytes()
        ).to_hex().to_string();
        NodeId::new(format!("tx:{hash}"))
    }
}

// ─── Credit calculation ────────────────────────────────────────────────────

/// IRC § 41 credit calculation method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsRdcMethod {
    /// § 41(a)(1): Traditional — 20% of excess QREs over fixed-base amount.
    Traditional,
    /// § 41(c)(5): Alternative Simplified Credit — 14% of QREs over 50% of 3-year average.
    Asc,
}

/// Aggregate credit calculation for an income year.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsRdcCredit {
    /// Calculation method selected.
    pub method: UsRdcMethod,
    /// Total qualified research expenditures for the year.
    pub total_qre: Decimal,
    /// Base amount (Traditional: fixed-base; ASC: 50% of 3-year avg QRE).
    pub base_amount: Decimal,
    /// Computed R&D tax credit amount.
    pub credit_amount: Decimal,
}

impl UsRdcCredit {
    /// Compute Traditional credit: 20% × max(0, QRE − base_amount).
    ///
    /// `base_amount` is the fixed-base amount under § 41(c).
    pub fn traditional(total_qre: Decimal, base_amount: Decimal) -> Self {
        let excess = if total_qre > base_amount { total_qre - base_amount } else { Decimal::ZERO };
        let rate = Decimal::from_str_exact("0.20").expect("static");
        Self {
            method: UsRdcMethod::Traditional,
            total_qre,
            base_amount,
            credit_amount: excess * rate,
        }
    }

    /// Compute ASC credit: 14% × max(0, QRE − 50% × avg_prior_qre).
    ///
    /// `avg_prior_qre` is the average of the 3 preceding tax years' QREs.
    pub fn asc(total_qre: Decimal, avg_prior_qre: Decimal) -> Self {
        let half_avg = avg_prior_qre / Decimal::from(2u32);
        let excess = if total_qre > half_avg { total_qre - half_avg } else { Decimal::ZERO };
        let rate = Decimal::from_str_exact("0.14").expect("static");
        Self {
            method: UsRdcMethod::Asc,
            total_qre,
            base_amount: half_avg,
            credit_amount: excess * rate,
        }
    }
}

/// Convenience function: compute best (larger) credit from both methods.
pub fn calculate_credit(
    expenditures: &[QreExpenditure],
    base_amount_traditional: Decimal,
    avg_prior_qre: Decimal,
) -> (UsRdcCredit, Vec<NodeId>) {
    let total_qre: Decimal = expenditures.iter().map(|e| e.qualified_amount()).sum();
    let evidence: Vec<NodeId> = expenditures.iter().map(|e| e.evidence_node()).collect();

    let trad = UsRdcCredit::traditional(total_qre, base_amount_traditional);
    let asc = UsRdcCredit::asc(total_qre, avg_prior_qre);

    let best = if trad.credit_amount >= asc.credit_amount { trad } else { asc };
    (best, evidence)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn test_lei() -> Lei {
        Lei::new("HWUPKR0MPOU8FGXBT394").expect("test lei")
    }

    fn qualifying_activity() -> QreActivity {
        QreActivity {
            lei: test_lei(),
            activity_id: "USACT-001".to_string(),
            activity_name: "b00t agent architecture A/B experiments".to_string(),
            technical_in_nature: true,
            permits_experimentation: true,
            technological_uncertainty: true,
            systematic_process: true,
        }
    }

    #[test]
    fn four_part_test_all_pass() {
        let result = qualifying_activity().satisfies(&UsRdcFourPartTest);
        assert!(result.disposition.is_satisfied());
        assert!(result.confidence >= 0.9);
    }

    #[test]
    fn four_part_test_not_technical() {
        let mut act = qualifying_activity();
        act.technical_in_nature = false;
        let result = act.satisfies(&UsRdcFourPartTest);
        assert!(!result.disposition.is_satisfied());
        if let Disposition::Violated { reason } = &result.disposition {
            assert!(reason.contains("41(d)(1)(A)"), "expected §41(d)(1)(A) in: {reason}");
        } else {
            panic!("expected Violated");
        }
    }

    #[test]
    fn four_part_test_no_uncertainty() {
        let mut act = qualifying_activity();
        act.technological_uncertainty = false;
        let result = act.satisfies(&UsRdcFourPartTest);
        assert!(!result.disposition.is_satisfied());
        if let Disposition::Violated { reason } = &result.disposition {
            assert!(reason.contains("41(d)(1)(C)"), "expected §41(d)(1)(C) in: {reason}");
        } else {
            panic!("expected Violated");
        }
    }

    #[test]
    fn contract_research_65_percent() {
        let exp = QreExpenditure {
            lei: test_lei(),
            tx_id: "TX-002".to_string(),
            category: QreCategory::ContractResearch,
            amount: dec!(100_000),
            currency: Currency::Usd,
        };
        assert_eq!(exp.qualified_amount(), dec!(65_000));
    }

    #[test]
    fn wages_full_inclusion() {
        let exp = QreExpenditure {
            lei: test_lei(),
            tx_id: "TX-003".to_string(),
            category: QreCategory::Wages,
            amount: dec!(200_000),
            currency: Currency::Usd,
        };
        assert_eq!(exp.qualified_amount(), dec!(200_000));
    }

    #[test]
    fn traditional_credit_calculation() {
        let credit = UsRdcCredit::traditional(dec!(500_000), dec!(300_000));
        // 20% × (500k - 300k) = 20% × 200k = 40k
        assert_eq!(credit.credit_amount, dec!(40_000));
    }

    #[test]
    fn asc_credit_calculation() {
        // 14% × (500k - 50% × 400k) = 14% × (500k - 200k) = 14% × 300k = 42k
        let credit = UsRdcCredit::asc(dec!(500_000), dec!(400_000));
        assert_eq!(credit.credit_amount, dec!(42_000));
    }

    #[test]
    fn calculate_credit_picks_best() {
        let lei = test_lei();
        let expenditures = vec![
            QreExpenditure {
                lei: lei.clone(),
                tx_id: "TX-004".to_string(),
                category: QreCategory::Wages,
                amount: dec!(500_000),
                currency: Currency::Usd,
            },
        ];
        let (credit, nodes) = calculate_credit(&expenditures, dec!(300_000), dec!(400_000));
        // ASC: 14% × (500k - 200k) = 42k > Traditional: 20% × 200k = 40k → ASC wins
        assert_eq!(credit.method, UsRdcMethod::Asc);
        assert_eq!(credit.credit_amount, dec!(42_000));
        assert_eq!(nodes.len(), 1);
    }

    #[test]
    fn section_refs_correct() {
        assert_eq!(QreCategory::Wages.section_ref(), "IRC § 41(b)(2)(A)");
        assert!(QreCategory::ContractResearch.section_ref().contains("65%"));
    }
}
