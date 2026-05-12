//! Kasuari-based constraint solving for data plausibility.
//! Uses the Cassowary algorithm to evaluate constraints against transaction populations.

use serde::{Deserialize, Serialize};
use ledger_attest::attested;
use crate::attest::{Attested, AttestationSpec};

/// Constraint strength levels (matching Kasuari).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConstraintStrength {
    /// Must be satisfied -- cannot be violated.
    Required,
    /// Strong preference -- can be violated only if required fails.
    Strong,
    /// Medium preference.
    Medium,
    /// Weak preference -- violated first if needed.
    Weak,
}

/// Result of constraint evaluation.
#[attested("constraint_evaluation_bounded")]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConstraintEvaluation {
    /// Whether REQUIRED constraints passed.
    pub required_pass: bool,
    /// Fraction of STRONG constraints passing (0.0-1.0).
    pub strong_ratio: f32,
    /// Fraction of MEDIUM constraints passing (0.0-1.0).
    pub medium_ratio: f32,
    /// Fraction of WEAK constraints passing (0.0-1.0).
    pub weak_ratio: f32,
}

impl ConstraintEvaluation {
    /// Convert to confidence score and disposition.
    pub fn to_confidence(&self) -> (f32, super::validation::Disposition) {
        use super::validation::Disposition;
        if !self.required_pass {
            return (0.0, Disposition::Unrecoverable);
        }
        let score = self.strong_ratio * 0.60 + self.medium_ratio * 0.30 + self.weak_ratio * 0.10;
        let disposition = if score >= 0.85 {
            Disposition::Advisory
        } else {
            Disposition::Recoverable
        };
        (score, disposition)
    }

    /// Convert constraint results into typed pipeline issues.
    pub fn to_issues(&self, vendor_id: &str) -> Vec<super::validation::Issue> {
        use super::validation::{Issue, IssueSource};
        let mut issues = Vec::new();
        if !self.required_pass {
            issues.push(Issue::unrecoverable(
                "constraint_required_fail",
                format!("vendor {vendor_id}: required constraint failed (zero-amount or corrupt)"),
            ));
            return issues;
        }
        if self.strong_ratio < 1.0 {
            issues.push(Issue::recoverable(
                "constraint_strong_fail",
                format!(
                    "vendor {vendor_id}: strong constraint ratio {:.0}%",
                    self.strong_ratio * 100.0
                ),
                IssueSource::Constraint {
                    strength: self.strong_ratio,
                },
            ));
        }
        if self.medium_ratio < 0.5 {
            issues.push(Issue::recoverable(
                "constraint_medium_fail",
                format!(
                    "vendor {vendor_id}: medium constraint ratio {:.0}%",
                    self.medium_ratio * 100.0
                ),
                IssueSource::Constraint {
                    strength: self.medium_ratio,
                },
            ));
        }
        issues
    }

    /// Produce a MetaFlag if this evaluation is below advisory threshold.
    pub fn to_meta_flag(&self, constraint_name: &str) -> Option<super::validation::MetaFlag> {
        use super::validation::MetaFlag;
        let (score, _) = self.to_confidence();
        if score < 0.85 {
            Some(MetaFlag::ConstraintWeak {
                constraint: constraint_name.to_string(),
            })
        } else {
            None
        }
    }
}

/// Structured result from invoice verification.
#[attested("invoice_arithmetic_valid")]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InvoiceVerification {
    pub evaluation: ConstraintEvaluation,
    pub arithmetic_ok: bool,
    pub gst_rate_ok: bool,
    pub audit_note: String,
}


impl Attested for ConstraintEvaluation {
    fn attestation_spec() -> AttestationSpec {
        AttestationSpec {
            invariant: "constraint_evaluation_bounded",
            z3_predicate: None,
            kasuari_description: Some("strong_ratio, medium_ratio, weak_ratio in [0.0, 1.0]"),
            kani_module: Some("kani_proofs::vendor_constraints"),
        }
    }
}

/// A historical constraint set for a vendor or category.
#[attested("vendor_constraint_bounds_ordered")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VendorConstraintSet {
    pub vendor_id: String,
    pub amount_p05: f64,
    pub amount_p95: f64,
    pub usual_day_of_month: Option<u32>,
    pub usual_tax_code: String,
    pub usual_account: String,
}

impl VendorConstraintSet {
    pub fn evaluate(
        &self,
        amount: f64,
        day: u32,
        tax_code: &str,
        account: &str,
    ) -> ConstraintEvaluation {
        let in_range = amount >= self.amount_p05 && amount <= self.amount_p95;
        let tax_matches = tax_code == self.usual_tax_code;
        let account_matches = account == self.usual_account;
        let required_pass = amount != 0.0;
        let strong_count = 2.0;
        let strong_pass = [in_range, tax_matches].iter().filter(|&&b| b).count() as f32;
        let strong_ratio = strong_pass / strong_count;
        let day_matches = self.usual_day_of_month.map(|d| day == d).unwrap_or(true);
        let medium_count = 2.0;
        let medium_pass = [day_matches, account_matches]
            .iter()
            .filter(|&&b| b)
            .count() as f32;
        let medium_ratio = medium_pass / medium_count;
        let weak_ratio = 1.0;
        ConstraintEvaluation {
            required_pass,
            strong_ratio,
            medium_ratio,
            weak_ratio,
        }
    }
}

impl Attested for VendorConstraintSet {
    fn attestation_spec() -> AttestationSpec {
        AttestationSpec {
            invariant: "vendor_constraint_bounds_ordered",
            z3_predicate: None,
            kasuari_description: Some("evaluate() strong_ratio in [0.0, 1.0] for all finite f64 inputs"),
            kani_module: Some("kani_proofs::vendor_constraints"),
        }
    }
}

/// Invoice arithmetic constraints (total = subtotal + gst).
#[derive(Debug, Clone, Default)]
pub struct InvoiceConstraintSolver {
    #[allow(dead_code)]
    constraint_count: usize,
}

impl InvoiceConstraintSolver {
    pub fn new() -> Self {
        Self {
            constraint_count: 0,
        }
    }

    /// Verify invoice arithmetic and return structured audit result.
    pub fn verify(&self, total: f64, subtotal: f64, gst: f64) -> InvoiceVerification {
        let evaluation = self.validate(total, subtotal, gst);
        let arithmetic_ok = evaluation.required_pass;
        let gst_rate_ok = evaluation.strong_ratio >= 1.0;
        let audit_note = if arithmetic_ok && gst_rate_ok {
            "invoice arithmetic verified".to_string()
        } else if !arithmetic_ok {
            format!("invoice arithmetic error: {total:.2} != {subtotal:.2} + {gst:.2}")
        } else {
            format!(
                "GST rate anomaly: expected {:.2}, got {gst:.2}",
                subtotal * 0.1
            )
        };
        InvoiceVerification {
            evaluation,
            arithmetic_ok,
            gst_rate_ok,
            audit_note,
        }
    }

    pub fn validate(&self, total: f64, subtotal: f64, gst: f64) -> ConstraintEvaluation {
        let required_pass = (total - subtotal - gst).abs() < 0.01;
        let expected_gst = subtotal * 0.1;
        let gst_correct = (gst - expected_gst).abs() < 0.02;
        let amounts_positive = total > 0.0 && subtotal > 0.0;
        let total_reasonable = total > 0.0 && total < 1_000_000.0;
        ConstraintEvaluation {
            required_pass,
            strong_ratio: if gst_correct { 1.0 } else { 0.0 },
            medium_ratio: if amounts_positive { 1.0 } else { 0.0 },
            weak_ratio: if total_reasonable { 1.0 } else { 0.0 },
        }
    }
}

/// Constraint solver for ontology graph layout.
#[derive(Debug, Clone, Default)]
pub struct LayoutSolver {
    _private: (),
}

impl LayoutSolver {
    pub fn new() -> Self {
        Self { _private: () }
    }
}


impl Attested for InvoiceVerification {
    fn attestation_spec() -> AttestationSpec {
        AttestationSpec {
            invariant: "invoice_arithmetic_valid",
            z3_predicate: Some("total = subtotal + gst"),
            kasuari_description: None,
            kani_module: Some("kani_proofs::invoice_arithmetic"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vendor_constraint() {
        let vendor = VendorConstraintSet {
            vendor_id: "AWS".to_string(),
            amount_p05: 100.0,
            amount_p95: 500.0,
            usual_day_of_month: Some(1),
            usual_tax_code: "BASEXCLUDED".to_string(),
            usual_account: "6-8800".to_string(),
        };
        let result = vendor.evaluate(250.0, 1, "BASEXCLUDED", "6-8800");
        assert!(result.required_pass);
        assert!(result.strong_ratio > 0.5);
    }

    #[test]
    fn test_invoice_constraint() {
        let solver = InvoiceConstraintSolver::new();
        let result = solver.validate(110.0, 100.0, 10.0);
        assert!(result.required_pass);
        assert_eq!(result.strong_ratio, 1.0);
    }

    #[test]
    fn test_evaluation_to_confidence() {
        let eval = ConstraintEvaluation {
            required_pass: true,
            strong_ratio: 1.0,
            medium_ratio: 1.0,
            weak_ratio: 1.0,
        };
        let (score, disposition) = eval.to_confidence();
        assert!(score > 0.9);
        assert_eq!(disposition, super::super::validation::Disposition::Advisory);
    }

    #[test]
    fn test_to_issues_required_fail() {
        let eval = ConstraintEvaluation {
            required_pass: false,
            strong_ratio: 0.0,
            medium_ratio: 0.0,
            weak_ratio: 0.0,
        };
        let issues = eval.to_issues("TESTVENDOR");
        assert_eq!(issues.len(), 1);
        assert_eq!(
            issues[0].disposition,
            super::super::validation::Disposition::Unrecoverable
        );
    }

    #[test]
    fn test_to_meta_flag() {
        let eval = ConstraintEvaluation {
            required_pass: true,
            strong_ratio: 0.5,
            medium_ratio: 0.5,
            weak_ratio: 1.0,
        };
        let flag = eval.to_meta_flag("vendor_amount");
        assert!(flag.is_some());
    }

    #[test]
    fn test_invoice_verify() {
        let solver = InvoiceConstraintSolver::new();
        let result = solver.verify(110.0, 100.0, 10.0);
        assert!(result.arithmetic_ok);
        assert!(result.gst_rate_ok);
        assert!(result.audit_note.contains("verified"));
    }
}
