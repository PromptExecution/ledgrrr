//! Z3-capable legal rule verification for tax compliance.
//! Encodes hard legal predicates as satisfiability checks over transaction facts.

use serde::{Deserialize, Serialize};
use ledger_attest::attested;
use crate::attest::{Attested, AttestationSpec};
#[cfg(feature = "legal-z3")]
use z3::{ast::Bool, Config, Context, SatResult, Solver};

/// Jurisdiction for tax rule evaluation (US, AU, UK, etc.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[derive(Default)]
pub enum Jurisdiction {
    #[default]
    US,
    AU,
    UK,
}


impl Jurisdiction {
    pub fn code(&self) -> &'static str {
        match self {
            Self::US => "US",
            Self::AU => "AU",
            Self::UK => "UK",
        }
    }

    /// Return the minimum viable ruleset for this jurisdiction's US expat use case.
    pub fn legal_ruleset(&self) -> Vec<LegalRule> {
        match self {
            Jurisdiction::US => vec![
                us_schedule_c::rule_ordinary_necessary(),
                us_fbar::rule_threshold(),
                us_feie::rule_exclusion(),
            ],
            Jurisdiction::AU => vec![
                au_gst::rule_38_190(),
                au_gst::rule_40_5(),
                au_fbt::rule_car_benefit(),
            ],
            Jurisdiction::UK => Vec::new(),
        }
    }
}

/// Result of Z3 SAT check.
#[attested("z3_result_confidence_total")]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Z3Result {
    Satisfied,
    Violated { witness: String },
    Unknown,
}

impl Z3Result {
    /// Confidence score (0.0-1.0) — Satisfied=1.0, Violated=0.0, Unknown=0.5.
    pub fn to_confidence(&self) -> f32 {
        match self {
            Z3Result::Satisfied => 1.0,
            Z3Result::Violated { .. } => 0.0,
            Z3Result::Unknown => 0.5,
        }
    }

    /// Validation issues for the pipeline — wraps the From<Z3Result> conversion.
    pub fn to_issues(&self) -> Vec<crate::validation::Issue> {
        use crate::validation::{Disposition, Issue, IssueSource};
        match self {
            Z3Result::Satisfied => Vec::new(),
            Z3Result::Violated { witness } => vec![Issue {
                code: "legal_violation".to_string(),
                message: witness.clone(),
                field: None,
                disposition: Disposition::Unrecoverable,
                source: IssueSource::TypeCheck,
            }],
            Z3Result::Unknown => vec![Issue::recoverable(
                "legal_unknown",
                "legal solver returned Unknown -- facts may be incomplete",
                IssueSource::Constraint { strength: 0.0 },
            )],
        }
    }

    /// Disposition mapping — Satisfied→Advisory, Violated→Unrecoverable, Unknown→Advisory.
    pub fn to_disposition(&self) -> crate::validation::Disposition {
        use crate::validation::Disposition;
        match self {
            Z3Result::Satisfied => Disposition::Advisory,
            Z3Result::Violated { .. } => Disposition::Unrecoverable,
            Z3Result::Unknown => Disposition::Advisory,
        }
    }
}

impl From<Z3Result> for Vec<crate::validation::Issue> {
    fn from(result: Z3Result) -> Self {
        result.to_issues()
    }
}

impl Attested for Z3Result {
    fn attestation_spec() -> AttestationSpec {
        AttestationSpec {
            invariant: "z3_result_confidence_total",
            z3_predicate: Some("to_confidence: Satisfied->1.0 | Violated->0.0 | Unknown->0.5"),
            kasuari_description: None,
            kani_module: Some("kani_proofs::z3_result"),
        }
    }
}

/// A legal rule encoded for verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegalRule {
    pub id: String,
    pub description: String,
    pub jurisdiction: Jurisdiction,
    pub formula: String,
    pub category: String,
}

impl LegalRule {
    pub fn new(id: impl Into<String>, jurisdiction: Jurisdiction) -> Self {
        Self {
            id: id.into(),
            description: String::new(),
            jurisdiction,
            formula: String::new(),
            category: String::new(),
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn with_formula(mut self, formula: impl Into<String>) -> Self {
        self.formula = formula.into();
        self
    }

    pub fn with_category(mut self, cat: impl Into<String>) -> Self {
        self.category = cat.into();
        self
    }
}

/// Transaction facts for rule evaluation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransactionFacts {
    pub vendor_jurisdiction: Option<String>,
    pub supply_type: Option<String>,
    pub tax_code: Option<String>,
    pub amount: Option<String>,
    pub is_business_activity: Option<bool>,
    pub is_ordinary: Option<bool>,
    pub is_necessary: Option<bool>,
}

impl TransactionFacts {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_vendor(mut self, j: impl Into<String>) -> Self {
        self.vendor_jurisdiction = Some(j.into());
        self
    }

    pub fn with_supply_type(mut self, t: impl Into<String>) -> Self {
        self.supply_type = Some(t.into());
        self
    }

    pub fn with_tax_code(mut self, c: impl Into<String>) -> Self {
        self.tax_code = Some(c.into());
        self
    }

    pub fn with_amount(mut self, a: impl Into<String>) -> Self {
        self.amount = Some(a.into());
        self
    }
}

/// Legal verification for hard tax predicates.
pub struct LegalSolver;

impl Default for LegalSolver {
    fn default() -> Self {
        Self::new()
    }
}

impl LegalSolver {
    pub fn new() -> Self {
        Self
    }

    pub fn verify(&self, rule: &LegalRule, facts: &TransactionFacts) -> Z3Result {
        if rule.id.contains("au-gst-38-190") {
            return self.verify_au_gst_38_190(facts);
        }
        if rule.id.contains("schedule-c") {
            return self.verify_us_schedule_c(facts);
        }
        Z3Result::Unknown
    }

    /// Verify all rules against facts. Returns (aggregate_confidence, all_issues).
    /// Each rule that passes contributes 1/N confidence; violations are Unrecoverable.
    pub fn verify_all(
        &self,
        rules: &[LegalRule],
        facts: &TransactionFacts,
    ) -> (f32, Vec<crate::validation::Issue>) {
        if rules.is_empty() {
            return (1.0, Vec::new());
        }
        let mut all_issues = Vec::new();
        let mut passed = 0usize;
        for rule in rules {
            let result = self.verify(rule, facts);
            let issues: Vec<crate::validation::Issue> = result.into();
            if issues.is_empty() {
                passed += 1;
            }
            all_issues.extend(issues);
        }
        let confidence = passed as f32 / rules.len() as f32;
        (confidence, all_issues)
    }

    fn verify_au_gst_38_190(&self, facts: &TransactionFacts) -> Z3Result {
        let Some(vendor) = facts.vendor_jurisdiction.as_deref() else {
            return Z3Result::Unknown;
        };
        if facts.supply_type.as_deref() != Some("SaaS") {
            return Z3Result::Unknown;
        }
        if vendor == "US" || vendor == "UK" {
            return self.violation_result(
                facts.tax_code.as_deref() != Some("BASEXCLUDED"),
                "foreign SaaS should have BASEXCLUDED tax code",
            );
        }
        if vendor == "AU" {
            return self.violation_result(
                facts.tax_code.as_deref() != Some("INPUT"),
                "AU SaaS should have INPUT tax code",
            );
        }
        Z3Result::Unknown
    }

    fn verify_us_schedule_c(&self, facts: &TransactionFacts) -> Z3Result {
        if facts.is_business_activity != Some(true) {
            return Z3Result::Unknown;
        }
        self.violation_result(
            facts.is_ordinary != Some(true) || facts.is_necessary != Some(true),
            "Schedule C business expenses must be ordinary and necessary",
        )
    }

    #[cfg(feature = "legal-z3")]
    fn violation_result(&self, violation: bool, witness: &str) -> Z3Result {
        let cfg = Config::new();
        let ctx = Context::new(&cfg);
        let solver = Solver::new(&ctx);
        let violation_bool = Bool::from_bool(&ctx, violation);
        solver.assert(&violation_bool);
        match solver.check() {
            SatResult::Sat => Z3Result::Violated {
                witness: witness.to_string(),
            },
            SatResult::Unsat => Z3Result::Satisfied,
            SatResult::Unknown => Z3Result::Unknown,
        }
    }

    #[cfg(not(feature = "legal-z3"))]
    fn violation_result(&self, violation: bool, witness: &str) -> Z3Result {
        if violation {
            Z3Result::Violated {
                witness: witness.to_string(),
            }
        } else {
            Z3Result::Satisfied
        }
    }
}

/// AU GST Act s38-190 -- overseas SaaS is input-taxed supply.
pub mod au_gst {
    use super::*;

    pub fn rule_38_190() -> LegalRule {
        LegalRule::new("au-gst-38-190", Jurisdiction::AU)
            .with_description("Overseas SaaS is input-taxed supply under GST Act s38-190")
            .with_category("GST")
            .with_formula("vendor != AU AND type == SaaS -> tax_code == BASEXCLUDED")
    }

    pub fn rule_40_5() -> LegalRule {
        LegalRule::new("au-gst-40-5", Jurisdiction::AU)
            .with_description(
                "Financial supplies are input-taxed; no GST credits on related expenses",
            )
            .with_category("GST")
            .with_formula("supply_type == financial -> input_taxed AND no_gst_credit")
    }
}

/// AU Fringe Benefits Tax rules.
pub mod au_fbt {
    use super::*;

    pub fn rule_car_benefit() -> LegalRule {
        LegalRule::new("au-fbt-car-benefit", Jurisdiction::AU)
            .with_description("Employer-provided car is a fringe benefit; statutory formula or operating cost method applies")
            .with_category("FBT")
            .with_formula("employer_provided_car -> fbt_liable AND (statutory_formula OR operating_cost_method)")
    }
}

/// US FBAR reporting threshold rules.
pub mod us_fbar {
    use super::*;

    pub fn rule_threshold() -> LegalRule {
        LegalRule::new("us-fbar-threshold", Jurisdiction::US)
            .with_description("Foreign accounts with aggregate balance > $10,000 must be reported")
            .with_category("FBAR")
            .with_formula("aggregate_foreign_balance > 10000 -> fbar_required")
    }
}

/// US Foreign Earned Income Exclusion rules.
pub mod us_feie {
    use super::*;

    pub fn rule_exclusion() -> LegalRule {
        LegalRule::new("us-feie-exclusion", Jurisdiction::US)
            .with_description("Foreign earned income excludable up to annual limit if bona fide residence or physical presence test met")
            .with_category("FEIE")
            .with_formula("foreign_earned AND (bona_fide_residence OR physical_presence) -> excludable")
    }
}

/// US Schedule C deduction rules.
pub mod us_schedule_c {
    use super::*;

    pub fn rule_ordinary_necessary() -> LegalRule {
        LegalRule::new("us-schedule-c-ordinary-necessary", Jurisdiction::US)
            .with_description("Expenses must be ordinary and necessary for business")
            .with_category("deduction")
            .with_formula("business_activity AND ordinary AND necessary -> deductible")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_au_gst_us_saas() {
        let solver = LegalSolver::new();
        let rule = au_gst::rule_38_190();
        let facts = TransactionFacts::new()
            .with_vendor("US")
            .with_supply_type("SaaS")
            .with_tax_code("BASEXCLUDED");
        let result = solver.verify(&rule, &facts);
        assert_eq!(result, Z3Result::Satisfied);
    }

    #[test]
    fn test_au_gst_us_wrong_tax() {
        let solver = LegalSolver::new();
        let rule = au_gst::rule_38_190();
        let facts = TransactionFacts::new()
            .with_vendor("US")
            .with_supply_type("SaaS")
            .with_tax_code("INPUT");
        let result = solver.verify(&rule, &facts);
        assert!(matches!(result, Z3Result::Violated { .. }));
    }

    #[test]
    fn test_us_schedule_c() {
        let solver = LegalSolver::new();
        let rule = us_schedule_c::rule_ordinary_necessary();
        let mut facts = TransactionFacts::new();
        facts.is_business_activity = Some(true);
        facts.is_ordinary = Some(true);
        facts.is_necessary = Some(true);
        let result = solver.verify(&rule, &facts);
        assert_eq!(result, Z3Result::Satisfied);
    }

    #[test]
    fn test_from_z3result_satisfied() {
        let issues: Vec<crate::validation::Issue> = Z3Result::Satisfied.into();
        assert!(issues.is_empty());
    }

    #[test]
    fn test_from_z3result_violated() {
        let issues: Vec<crate::validation::Issue> = Z3Result::Violated {
            witness: "test violation".to_string(),
        }
        .into();
        assert_eq!(issues.len(), 1);
        assert_eq!(
            issues[0].disposition,
            crate::validation::Disposition::Unrecoverable
        );
    }

    #[test]
    fn test_verify_all_empty_rules() {
        let solver = LegalSolver::new();
        let (confidence, issues) = solver.verify_all(&[], &TransactionFacts::new());
        assert_eq!(confidence, 1.0);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_verify_all_mixed() {
        let solver = LegalSolver::new();
        let rules = vec![au_gst::rule_38_190()];
        let facts = TransactionFacts::new()
            .with_vendor("US")
            .with_supply_type("SaaS")
            .with_tax_code("BASEXCLUDED");
        let (confidence, issues) = solver.verify_all(&rules, &facts);
        assert_eq!(confidence, 1.0);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_jurisdiction_legal_ruleset() {
        let us_rules = Jurisdiction::US.legal_ruleset();
        assert!(!us_rules.is_empty());
        let au_rules = Jurisdiction::AU.legal_ruleset();
        assert!(!au_rules.is_empty());
    }
}
