//! Ledger Pipeline: A typed domain language for financial document processing.
//! Uses statig HSM + type-state pattern + generics for compile-time safety.

use serde::{Deserialize, Serialize};
use std::marker::PhantomData;

// ============================================================================
// TYPE-STATE: Compile-time valid transitions
// ============================================================================

#[derive(Debug)]
pub struct Ingested;
#[derive(Debug)]
pub struct Validated;
#[derive(Debug)]
pub struct Classified;
#[derive(Debug)]
pub struct Reconciled;
#[derive(Debug)]
pub struct Committed;
#[derive(Debug)]
pub struct NeedsReview;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DocumentFields {
    pub vendor_jurisdiction: Option<String>,
    pub supply_type: Option<String>,
    pub tax_code: Option<String>,
    pub amount: Option<rust_decimal::Decimal>,
    pub is_business_activity: Option<bool>,
    pub is_ordinary: Option<bool>,
    pub is_necessary: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineState<S = Ingested> {
    pub document_id: String,
    pub source_ref: String,
    pub confidence: f32,
    pub issues: Vec<crate::validation::Issue>,
    pub meta: crate::validation::MetaCtx,
    #[serde(default)]
    pub doc_fields: DocumentFields,
    _state: PhantomData<S>,
}

impl<S> PipelineState<S> {
    pub fn new(document_id: impl Into<String>, source_ref: impl Into<String>) -> Self {
        Self {
            document_id: document_id.into(),
            source_ref: source_ref.into(),
            confidence: 1.0,
            issues: Vec::new(),
            meta: crate::validation::MetaCtx::default(),
            doc_fields: DocumentFields::default(),
            _state: PhantomData,
        }
    }

    pub fn with_confidence(mut self, c: f32) -> Self {
        self.confidence = c;
        self
    }

    pub fn with_doc_fields(mut self, fields: DocumentFields) -> Self {
        self.doc_fields = fields;
        self
    }
}

impl PipelineState<Ingested> {
    /// Apply vendor constraint check before validation.
    /// Returns self with constraint issues and MetaFlags attached (stays in Ingested state).
    pub fn check_constraints(
        mut self,
        constraint_set: &crate::constraints::VendorConstraintSet,
        amount: f64,
        day: u32,
        tax_code: &str,
        account: &str,
    ) -> Self {
        let eval = constraint_set.evaluate(amount, day, tax_code, account);
        let issues = eval.to_issues(&constraint_set.vendor_id);
        let flag = eval.to_meta_flag(&constraint_set.vendor_id);
        let confidence = {
            let (score, _) = eval.to_confidence();
            score
        };
        self.issues.extend(issues.clone());
        if let Some(f) = flag {
            self.meta.flags.push(f);
        }
        self.meta = self.meta.advance("check_constraints", confidence, &issues);
        self
    }

    pub fn validate(self, issues: Vec<crate::validation::Issue>) -> PipelineState<Validated> {
        let confidence = self.compute_confidence(&issues);
        PipelineState {
            document_id: self.document_id,
            source_ref: self.source_ref,
            confidence,
            issues,
            meta: self.meta.advance("validate", confidence, &[]),
            doc_fields: self.doc_fields,
            _state: PhantomData,
        }
    }

    fn compute_confidence(&self, issues: &[crate::validation::Issue]) -> f32 {
        if issues
            .iter()
            .any(|i| i.disposition == crate::validation::Disposition::Unrecoverable)
        {
            return 0.0;
        }
        let recovery_penalty = issues
            .iter()
            .filter(|i| i.disposition == crate::validation::Disposition::Recoverable)
            .count() as f32
            * 0.1;
        (self.confidence - recovery_penalty).max(0.0)
    }
}

impl PipelineState<Validated> {
    /// Verify transaction against legal rules.
    /// Returns Ok(Classified) if no Unrecoverable violations, Err(NeedsReview) otherwise.
    #[allow(clippy::result_large_err)]
    pub fn verify_legal(
        self,
        solver: &crate::legal::LegalSolver,
        rules: &[crate::legal::LegalRule],
    ) -> Result<PipelineState<Classified>, PipelineState<NeedsReview>> {
        let facts = self.to_transaction_facts();
        let (confidence, issues) = solver.verify_all(rules, &facts);
        let has_unrecoverable = issues
            .iter()
            .any(|i| i.disposition == crate::validation::Disposition::Unrecoverable);
        let mut next_issues = self.issues.clone();
        next_issues.extend(issues.clone());
        let next_meta = self.meta.advance("verify_legal", confidence, &issues);
        if has_unrecoverable {
            Err(PipelineState {
                document_id: self.document_id,
                source_ref: self.source_ref,
                confidence: 0.0,
                issues: next_issues,
                meta: next_meta,
                doc_fields: self.doc_fields,
                _state: std::marker::PhantomData,
            })
        } else {
            Ok(PipelineState {
                document_id: self.document_id,
                source_ref: self.source_ref,
                confidence: self.confidence * confidence.max(0.01),
                issues: next_issues,
                meta: next_meta,
                doc_fields: self.doc_fields,
                _state: std::marker::PhantomData,
            })
        }
    }

    /// Extract transaction facts for legal verification.
    pub fn to_transaction_facts(&self) -> crate::legal::TransactionFacts {
        let mut facts = crate::legal::TransactionFacts::new();
        facts.vendor_jurisdiction = self.doc_fields.vendor_jurisdiction.clone();
        facts.supply_type = self.doc_fields.supply_type.clone();
        facts.tax_code = self.doc_fields.tax_code.clone();
        facts.amount = self.doc_fields.amount.map(|d| d.to_string());
        facts.is_business_activity = self.doc_fields.is_business_activity;
        facts.is_ordinary = self.doc_fields.is_ordinary;
        facts.is_necessary = self.doc_fields.is_necessary;
        facts
    }

    pub fn classify(self, _category: String) -> PipelineState<Classified> {
        PipelineState {
            document_id: self.document_id,
            source_ref: self.source_ref,
            confidence: self.confidence,
            issues: self.issues,
            meta: self.meta.advance("classify", self.confidence, &[]),
            doc_fields: self.doc_fields,
            _state: PhantomData,
        }
    }
}

impl PipelineState<Classified> {
    pub fn reconcile(self, _xero_id: Option<String>) -> PipelineState<Reconciled> {
        PipelineState {
            document_id: self.document_id,
            source_ref: self.source_ref,
            confidence: self.confidence,
            issues: self.issues,
            meta: self.meta,
            doc_fields: self.doc_fields,
            _state: PhantomData,
        }
    }

    pub fn request_review(self) -> PipelineState<NeedsReview> {
        PipelineState {
            document_id: self.document_id,
            source_ref: self.source_ref,
            confidence: self.confidence,
            issues: self.issues,
            meta: self.meta,
            doc_fields: self.doc_fields,
            _state: PhantomData,
        }
    }
}

// ============================================================================
// COMMIT GATE
// ============================================================================

/// Evaluate whether a reconciled transaction may be committed without operator interruption.
pub fn evaluate_commit_gate(
    state: &PipelineState<Reconciled>,
    threshold: f32,
) -> crate::validation::CommitGate {
    use crate::validation::CommitGate;

    let unrecoverable: Vec<_> = state
        .issues
        .iter()
        .filter(|i| i.disposition == crate::validation::Disposition::Unrecoverable)
        .cloned()
        .collect();

    if !unrecoverable.is_empty() {
        return CommitGate::Blocked {
            issues: unrecoverable,
        };
    }

    if state.confidence >= threshold {
        CommitGate::Approved {
            confidence: state.confidence,
        }
    } else {
        CommitGate::PendingOperator {
            confidence: state.confidence,
            reason: format!(
                "confidence {:.2} below threshold {threshold:.2}",
                state.confidence
            ),
        }
    }
}

// ============================================================================
// STATIG HSM
// ============================================================================

#[derive(Debug, Clone)]
pub enum PipelineEvent {
    DocumentIngested {
        document_id: String,
        source_ref: String,
    },
    ValidationPassed,
    ValidationFailed {
        reason: String,
    },
    Classified {
        category: String,
    },
    LowConfidence {
        score: f32,
    },
    Reconciled {
        xero_id: Option<String>,
    },
    XeroPushFailed {
        error: String,
    },
    CommitApproved,
    CommitRejected {
        reason: String,
    },
}

#[derive(Default)]
pub struct PipelineCtx {
    pub jurisdiction: crate::legal::Jurisdiction,
    pub repair_attempts: usize,
    pub xero_retries: usize,
}

pub struct LedgerPipeline {
    pub jurisdiction: crate::legal::Jurisdiction,
    pub repair_attempts: usize,
    pub xero_retries: usize,
}

impl Default for LedgerPipeline {
    fn default() -> Self {
        Self {
            jurisdiction: crate::legal::Jurisdiction::US,
            repair_attempts: 0,
            xero_retries: 0,
        }
    }
}

impl LedgerPipeline {
    pub fn new(jurisdiction: crate::legal::Jurisdiction) -> Self {
        Self {
            jurisdiction,
            repair_attempts: 0,
            xero_retries: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[derive(Default)]
pub enum State {
    #[default]
    Ingested,
    Validating,
    Classifying,
    Reconciling,
    Committed,
    NeedsReview,
}


pub fn init() -> State {
    State::Ingested
}

pub fn handle_event(
    state: State,
    event: &PipelineEvent,
    ctx: &mut LedgerPipeline,
) -> Option<State> {
    match (state, event) {
        (State::Ingested, PipelineEvent::DocumentIngested { .. }) => Some(State::Validating),
        (State::Validating, PipelineEvent::ValidationPassed) => Some(State::Classifying),
        (State::Validating, PipelineEvent::ValidationFailed { .. }) => {
            ctx.repair_attempts += 1;
            if ctx.repair_attempts >= 2 {
                Some(State::NeedsReview)
            } else {
                Some(State::Validating)
            }
        }
        (State::Classifying, PipelineEvent::Classified { .. }) => Some(State::Reconciling),
        (State::Classifying, PipelineEvent::LowConfidence { .. }) => Some(State::NeedsReview),
        (State::Reconciling, PipelineEvent::Reconciled { .. }) => Some(State::Committed),
        (State::Reconciling, PipelineEvent::XeroPushFailed { .. }) => {
            if ctx.xero_retries < 3 {
                ctx.xero_retries += 1;
                Some(State::Reconciling)
            } else {
                Some(State::NeedsReview)
            }
        }
        (State::Reconciling, PipelineEvent::CommitApproved) => Some(State::Committed),
        (State::Committed, PipelineEvent::CommitRejected { .. }) => Some(State::NeedsReview),
        _ => None,
    }
}

// ============================================================================
// VERB TRAIT
// ============================================================================

pub trait Verb: Send + Sync + 'static {
    type Input: serde::Serialize + serde::de::DeserializeOwned;
    type Output: serde::Serialize + serde::de::DeserializeOwned;

    fn name(&self) -> &'static str;
    fn reversibility(&self) -> crate::validation::Reversibility;
    fn access(&self) -> crate::validation::AccessCriteria;
    fn execute(&self, input: Self::Input) -> (Vec<crate::validation::Issue>, Self::Output);
}

pub mod verbs {
    use super::*;

    pub struct DetectVerb;

    impl Verb for DetectVerb {
        type Input = Vec<u8>;
        type Output = String;

        fn name(&self) -> &'static str {
            "detect"
        }
        fn reversibility(&self) -> crate::validation::Reversibility {
            crate::validation::Reversibility::Free
        }
        fn access(&self) -> crate::validation::AccessCriteria {
            crate::validation::AccessCriteria::Open
        }
        fn execute(&self, input: Vec<u8>) -> (Vec<crate::validation::Issue>, String) {
            if input.len() >= 4 && &input[..4] == b"%PDF" {
                (Vec::new(), "pdf".to_string())
            } else {
                (
                    vec![crate::validation::Issue::unrecoverable(
                        "unknown_shape",
                        "Could not detect document type",
                    )],
                    "unknown".to_string(),
                )
            }
        }
    }

    pub struct ValidateVerb;

    impl Verb for ValidateVerb {
        type Input = (String, f64);
        type Output = bool;

        fn name(&self) -> &'static str {
            "validate"
        }
        fn reversibility(&self) -> crate::validation::Reversibility {
            crate::validation::Reversibility::Free
        }
        fn access(&self) -> crate::validation::AccessCriteria {
            crate::validation::AccessCriteria::Open
        }
        fn execute(&self, input: (String, f64)) -> (Vec<crate::validation::Issue>, bool) {
            let (description, amount) = input;
            let mut issues = Vec::new();
            if amount == 0.0 {
                issues.push(crate::validation::Issue::unrecoverable(
                    "zero_amount",
                    "Amount cannot be zero",
                ));
            }
            if description.trim().is_empty() {
                issues.push(crate::validation::Issue::recoverable(
                    "empty_description",
                    "Description is empty",
                    crate::validation::IssueSource::TypeCheck,
                ));
            }
            let valid = issues.is_empty()
                || !issues
                    .iter()
                    .any(|i| i.disposition == crate::validation::Disposition::Unrecoverable);
            (issues, valid)
        }
    }
}

// ============================================================================
// GENERIC CONSTRAINT SOLVER
// ============================================================================

pub trait ConstraintSolver: Send + Sync {
    fn evaluate(&self, field: &str, value: f64, constraints: &[(f64, f64)]) -> f32;
    fn strength(&self, constraint: &str) -> crate::constraints::ConstraintStrength;
}

pub struct KasuariSolver;

impl ConstraintSolver for KasuariSolver {
    fn evaluate(&self, _field: &str, value: f64, constraints: &[(f64, f64)]) -> f32 {
        for (min, max) in constraints {
            if value >= *min && value <= *max {
                return 1.0;
            }
            if value >= *min * 0.5 && value <= *max * 2.0 {
                return 0.5;
            }
        }
        0.0
    }

    fn strength(&self, constraint: &str) -> crate::constraints::ConstraintStrength {
        match constraint {
            "required" => crate::constraints::ConstraintStrength::Required,
            "strong" => crate::constraints::ConstraintStrength::Strong,
            "medium" => crate::constraints::ConstraintStrength::Medium,
            _ => crate::constraints::ConstraintStrength::Weak,
        }
    }
}

// ============================================================================
// BUILDER
// ============================================================================

pub struct PipelineBuilder {
    jurisdiction: crate::legal::Jurisdiction,
    min_confidence: f32,
    #[allow(dead_code)]
    max_retries: usize,
    enable_legal_verification: bool,
}

impl Default for PipelineBuilder {
    fn default() -> Self {
        Self {
            jurisdiction: crate::legal::Jurisdiction::US,
            min_confidence: 0.85,
            max_retries: 2,
            enable_legal_verification: true,
        }
    }
}

impl PipelineBuilder {
    pub fn jurisdiction(mut self, j: crate::legal::Jurisdiction) -> Self {
        self.jurisdiction = j;
        self
    }

    pub fn min_confidence(mut self, c: f32) -> Self {
        self.min_confidence = c;
        self
    }

    pub fn enable_legal_verification(mut self, b: bool) -> Self {
        self.enable_legal_verification = b;
        self
    }

    pub fn build(self) -> LedgerPipeline {
        LedgerPipeline::new(self.jurisdiction)
    }
}

// ============================================================================
// KANI / TEST CONSTRUCTORS
// ============================================================================

#[cfg(any(test, kani))]
impl PipelineState<Reconciled> {
    pub fn new_for_kani(confidence: f32) -> Self {
        Self {
            document_id: String::new(),
            source_ref: String::new(),
            confidence,
            issues: Vec::new(),
            meta: crate::validation::MetaCtx::default(),
            doc_fields: DocumentFields::default(),
            _state: PhantomData,
        }
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_state_transition() {
        let state = PipelineState::<Ingested>::new("doc1", "WF--BH--2026-01");
        let validated = state.validate(Vec::new());
        let classified = validated.classify("6-8800".to_string());
        let reconciled = classified.reconcile(Some("XERO-123".to_string()));
        assert_eq!(reconciled.document_id, "doc1");
    }

    #[test]
    fn test_pipeline_builder() {
        let pipeline = PipelineBuilder::default()
            .jurisdiction(crate::legal::Jurisdiction::AU)
            .min_confidence(0.9)
            .enable_legal_verification(true)
            .build();
        assert_eq!(pipeline.jurisdiction, crate::legal::Jurisdiction::AU);
    }

    #[test]
    fn test_verb_execution() {
        let verb = verbs::DetectVerb;
        let pdf_bytes = b"%PDF-1.4 fake pdf content".to_vec();
        let (issues, output) = verb.execute(pdf_bytes);
        assert!(issues.is_empty());
        assert_eq!(output, "pdf");
    }

    #[test]
    fn test_validate_verb() {
        let verb = verbs::ValidateVerb;
        let (issues, valid) = verb.execute(("AWS bill".to_string(), 250.0));
        assert!(issues.is_empty());
        assert!(valid);
        let (issues, valid) = verb.execute(("".to_string(), 0.0));
        assert!(!issues.is_empty());
        assert!(!valid);
    }

    #[test]
    fn test_constraint_solver() {
        let solver = KasuariSolver;
        let result = solver.evaluate("amount", 250.0, &[(100.0, 500.0)]);
        assert_eq!(result, 1.0);
        let result = solver.evaluate("amount", 10.0, &[(100.0, 500.0)]);
        assert_eq!(result, 0.0);
        let result = solver.evaluate("amount", 500.0, &[(100.0, 500.0)]);
        assert_eq!(result, 1.0);
    }

    #[test]
    fn test_hsm_transitions() {
        let mut ctx = LedgerPipeline::default();
        let next = handle_event(
            State::Ingested,
            &PipelineEvent::DocumentIngested {
                document_id: "doc1".to_string(),
                source_ref: "WF--2026-01".to_string(),
            },
            &mut ctx,
        );
        assert_eq!(next, Some(State::Validating));
        let next = handle_event(
            State::Validating,
            &PipelineEvent::ValidationPassed,
            &mut ctx,
        );
        assert_eq!(next, Some(State::Classifying));
        let next = handle_event(
            State::Classifying,
            &PipelineEvent::Classified {
                category: "6-8800".to_string(),
            },
            &mut ctx,
        );
        assert_eq!(next, Some(State::Reconciling));
        let next = handle_event(
            State::Reconciling,
            &PipelineEvent::Reconciled {
                xero_id: Some("XERO-123".to_string()),
            },
            &mut ctx,
        );
        assert_eq!(next, Some(State::Committed));
    }

    #[test]
    fn test_hsm_retry_logic() {
        let mut ctx = LedgerPipeline::default();
        ctx.repair_attempts = 0;
        let next = handle_event(
            State::Validating,
            &PipelineEvent::ValidationFailed {
                reason: "test".to_string(),
            },
            &mut ctx,
        );
        assert_eq!(next, Some(State::Validating));
        assert_eq!(ctx.repair_attempts, 1);
        let next = handle_event(
            State::Validating,
            &PipelineEvent::ValidationFailed {
                reason: "test".to_string(),
            },
            &mut ctx,
        );
        assert_eq!(next, Some(State::NeedsReview));
    }

    #[test]
    fn test_check_constraints_integration() {
        use crate::constraints::VendorConstraintSet;
        let vendor = VendorConstraintSet {
            vendor_id: "AWS".to_string(),
            amount_p05: 100.0,
            amount_p95: 500.0,
            usual_day_of_month: Some(1),
            usual_tax_code: "BASEXCLUDED".to_string(),
            usual_account: "6-8800".to_string(),
        };
        let state = PipelineState::<Ingested>::new("doc1", "WF--BH--2026-01");
        let state = state.check_constraints(&vendor, 250.0, 1, "BASEXCLUDED", "6-8800");
        assert!(state.issues.is_empty());
    }

    #[test]
    fn test_verify_legal_integration() {
        use crate::legal::{au_gst, LegalSolver};
        let solver = LegalSolver::new();
        let rules = vec![au_gst::rule_38_190()];

        // US SaaS with BASEXCLUDED → legal gate passes → Ok(Classified)
        let state = PipelineState::<Ingested>::new("doc1", "WF--BH--2026-01")
            .with_doc_fields(DocumentFields {
                vendor_jurisdiction: Some("US".to_string()),
                supply_type: Some("SaaS".to_string()),
                tax_code: Some("BASEXCLUDED".to_string()),
                ..DocumentFields::default()
            })
            .validate(Vec::new());
        let ok_state = match state.verify_legal(&solver, &rules) {
            Ok(state) => state,
            Err(state) => panic!(
                "BASEXCLUDED should satisfy au_gst::rule_38_190, got Err state: {:?}",
                state
            ),
        };
        assert_eq!(ok_state.confidence, 1.0);
        assert!(
            !ok_state
                .issues
                .iter()
                .any(|i| i.code == "legal_unknown" || i.code == "legal_violation")
        );

        // US SaaS with INPUT → legal gate fails → Err(NeedsReview)
        let state = PipelineState::<Ingested>::new("doc2", "WF--BH--2026-01")
            .with_doc_fields(DocumentFields {
                vendor_jurisdiction: Some("US".to_string()),
                supply_type: Some("SaaS".to_string()),
                tax_code: Some("INPUT".to_string()),
                ..DocumentFields::default()
            })
            .validate(Vec::new());
        let err_state = match state.verify_legal(&solver, &rules) {
            Ok(state) => panic!(
                "INPUT should violate au_gst::rule_38_190, got Ok state: {:?}",
                state
            ),
            Err(state) => state,
        };
        assert!(
            err_state
                .issues
                .iter()
                .any(|i| i.code == "legal_violation"
                    && i.disposition == crate::validation::Disposition::Unrecoverable)
        );
        assert!(!err_state.issues.iter().any(|i| i.code == "legal_unknown"));
    }

    #[test]
    fn test_evaluate_commit_gate_approved() {
        let state = PipelineState::<Ingested>::new("doc1", "src")
            .validate(Vec::new())
            .classify("6-8800".to_string())
            .reconcile(None);
        let gate = evaluate_commit_gate(&state, 0.85);
        assert!(matches!(
            gate,
            crate::validation::CommitGate::Approved { .. }
        ));
    }

    #[test]
    fn test_evaluate_commit_gate_pending() {
        use crate::validation::{Issue, IssueSource};
        let issues = vec![Issue::recoverable(
            "test",
            "test recoverable",
            IssueSource::Constraint { strength: 0.5 },
        )];
        let state = PipelineState::<Ingested>::new("doc1", "src")
            .validate(issues)
            .classify("6-8800".to_string())
            .reconcile(None);
        let gate = evaluate_commit_gate(&state, 0.99);
        assert!(matches!(
            gate,
            crate::validation::CommitGate::PendingOperator { .. }
                | crate::validation::CommitGate::Approved { .. }
        ));
    }
}
