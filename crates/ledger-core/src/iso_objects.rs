//! `HasVisualization` implementations for the 21 domain types that participate
//! in the isometric pipeline view.

use crate::iso::{HasVisualization, RhaiDsl, SemanticType, VisualizationSpec, ZLayer};

use crate::constraints::{
    ConstraintEvaluation, InvoiceConstraintSolver, InvoiceVerification, VendorConstraintSet,
};
use crate::legal::{Jurisdiction, LegalRule, LegalSolver, TransactionFacts, Z3Result};
use crate::pipeline::{
    Classified, Committed, Ingested, KasuariSolver, NeedsReview, PipelineState, Reconciled,
    Validated,
};
use crate::validation::{CommitGate, Disposition, Issue, MetaCtx, MetaFlag, StageResult};

// ============================================================================
// PIPELINE STATES (z=1, Pipeline layer)
// ============================================================================

impl HasVisualization for PipelineState<Ingested> {
    fn viz_spec() -> VisualizationSpec {
        VisualizationSpec {
            semantic_type: SemanticType::Pipeline,
            z_layer: ZLayer::Pipeline,
            rhai_dsl: RhaiDsl::new(
                r#"let tx = ingest(pdf_path);
check_constraints(tx, constraint_set);"#,
            ),
            description: "Raw ingested transaction — structure validated, awaiting constraint pass",
        }
    }
}

impl HasVisualization for PipelineState<Validated> {
    fn viz_spec() -> VisualizationSpec {
        VisualizationSpec {
            semantic_type: SemanticType::Pipeline,
            z_layer: ZLayer::Pipeline,
            rhai_dsl: RhaiDsl::new(r#"let validated = tx.validate(constraint_set);
if validated.confidence >= MIN_CONF { route_to_legal(validated) }
else { flag("low_confidence") }"#),
            description: "Post-constraint validated transaction — all numerical bounds passed, awaiting legal verification",
        }
    }
}

impl HasVisualization for PipelineState<Classified> {
    fn viz_spec() -> VisualizationSpec {
        VisualizationSpec {
            semantic_type: SemanticType::Pipeline,
            z_layer: ZLayer::Pipeline,
            rhai_dsl: RhaiDsl::new(r#"let classified = legal_verified_tx.classify(rules);
set_category(classified, tax_category);
emit_to_workbook(classified);"#),
            description: "Legal-verified transaction with tax category assigned — ready for workbook reconciliation",
        }
    }
}

impl HasVisualization for PipelineState<Reconciled> {
    fn viz_spec() -> VisualizationSpec {
        VisualizationSpec {
            semantic_type: SemanticType::Pipeline,
            z_layer: ZLayer::Pipeline,
            rhai_dsl: RhaiDsl::new(
                r#"let reconciled = match_workbook(classified_tx, workbook);
if reconciled.matched { open_commit_gate(reconciled) }
else { flag("unmatched_entry") }"#,
            ),
            description:
                "Transaction matched against workbook entries — commit gate evaluation pending",
        }
    }
}

impl HasVisualization for PipelineState<Committed> {
    fn viz_spec() -> VisualizationSpec {
        VisualizationSpec {
            semantic_type: SemanticType::Pipeline,
            z_layer: ZLayer::Pipeline,
            rhai_dsl: RhaiDsl::new(
                r#"let committed = commit_gate.approve(reconciled_tx);
write_xlsx(committed);
emit_audit_trail(committed.id);"#,
            ),
            description: "Committed to workbook — final immutable state, audit trail emitted",
        }
    }
}

impl HasVisualization for PipelineState<NeedsReview> {
    fn viz_spec() -> VisualizationSpec {
        VisualizationSpec {
            semantic_type: SemanticType::Pipeline,
            z_layer: ZLayer::Pipeline,
            rhai_dsl: RhaiDsl::new(
                r#"let review = legal_fail(tx, z3_result);
flag_operator("legal_violation", review.rule_id);
route_to_review_queue(review);"#,
            ),
            description:
                "Legal verification failed — operator flag set, transaction held for manual review",
        }
    }
}

// ============================================================================
// CONSTRAINTS (z=2, Constraint layer)
// ============================================================================

impl HasVisualization for ConstraintEvaluation {
    fn viz_spec() -> VisualizationSpec {
        VisualizationSpec {
            semantic_type: SemanticType::Result,
            z_layer: ZLayer::Constraint,
            rhai_dsl: RhaiDsl::new(
                r#"let res = constraint_set.evaluate(amount, day, code, acct);
if res.required_pass { classify_ok() } else { flag("constraint_fail") }"#,
            ),
            description: "Numerical constraint evaluation result with pass/fail per-field scores",
        }
    }
}

impl HasVisualization for VendorConstraintSet {
    fn viz_spec() -> VisualizationSpec {
        VisualizationSpec {
            semantic_type: SemanticType::Constraint,
            z_layer: ZLayer::Constraint,
            rhai_dsl: RhaiDsl::new(r#"let bounds = load_vendor_constraints(vendor_id);
let res = bounds.evaluate(amount, day_of_month, tax_code, account);
emit_constraint_result(res);"#),
            description: "Vendor-specific statistical bounds: amount percentiles, usual day-of-month, tax code, and account",
        }
    }
}

impl HasVisualization for InvoiceConstraintSolver {
    fn viz_spec() -> VisualizationSpec {
        VisualizationSpec {
            semantic_type: SemanticType::Solver,
            z_layer: ZLayer::Constraint,
            rhai_dsl: RhaiDsl::new(r#"let solver = InvoiceConstraintSolver(gst_rate, expected_net);
let verification = solver.verify(gross, gst_amount);
if verification.arithmetic_ok && verification.gst_rate_ok { pass() }"#),
            description: "Invoice GST arithmetic solver — checks gross/net/GST consistency and rate conformance",
        }
    }
}

impl HasVisualization for InvoiceVerification {
    fn viz_spec() -> VisualizationSpec {
        VisualizationSpec {
            semantic_type: SemanticType::Result,
            z_layer: ZLayer::Constraint,
            rhai_dsl: RhaiDsl::new(
                r#"let v = invoice_solver.verify(gross, gst);
if !v.arithmetic_ok { flag("arithmetic_mismatch", v.audit_note) }
if !v.gst_rate_ok   { flag("gst_rate_mismatch", v.audit_note) }"#,
            ),
            description:
                "Invoice verification result — arithmetic_ok and gst_rate_ok flags with audit note",
        }
    }
}

// ============================================================================
// LEGAL (z=3, Legal layer)
// ============================================================================

impl HasVisualization for Z3Result {
    fn viz_spec() -> VisualizationSpec {
        VisualizationSpec {
            semantic_type: SemanticType::Result,
            z_layer: ZLayer::Legal,
            rhai_dsl: RhaiDsl::new(r#"let result = legal_solver.verify(rule, facts);
switch result {
    "Satisfied" => ok(),
    "Violated"  => flag("legal_violation"),
    "Unknown"   => flag("legal_unknown"),
}"#),
            description: "Symbolic satisfiability outcome from Z3-style legal predicate check: Satisfied/Violated/Unknown",
        }
    }
}

impl HasVisualization for LegalRule {
    fn viz_spec() -> VisualizationSpec {
        VisualizationSpec {
            semantic_type: SemanticType::Legal,
            z_layer: ZLayer::Legal,
            rhai_dsl: RhaiDsl::new(
                r#"let rule = LegalRule(jurisdiction, "au-gst-38-190")
    .with_formula("supply_type == 'GST_FREE' && vendor_jurisdiction == 'AU'")
    .with_category("GST");
legal_solver.verify(rule, facts);"#,
            ),
            description:
                "Single jurisdiction-bound legal rule: threshold, exclusion, or benefit predicate",
        }
    }
}

impl HasVisualization for LegalSolver {
    fn viz_spec() -> VisualizationSpec {
        VisualizationSpec {
            semantic_type: SemanticType::Solver,
            z_layer: ZLayer::Legal,
            rhai_dsl: RhaiDsl::new(r#"let solver = LegalSolver();
let result = solver.verify_all(jurisdiction.legal_ruleset(), facts);
if result.issues.is_empty() { advance_pipeline() } else { route_review(result.issues) }"#),
            description: "Runs all jurisdiction rules against TransactionFacts, returns aggregate confidence and issue list",
        }
    }
}

impl HasVisualization for Jurisdiction {
    fn viz_spec() -> VisualizationSpec {
        VisualizationSpec {
            semantic_type: SemanticType::Legal,
            z_layer: ZLayer::Legal,
            rhai_dsl: RhaiDsl::new(r#"let j = Jurisdiction::AU;
let rules = j.legal_ruleset();
// US -> FBAR/FEIE rules; AU -> GST/FBT rules
let code = j.code(); // "US" | "AU" | "UK""#),
            description: "Jurisdiction enum controlling which legal ruleset applies: US (FBAR/FEIE), AU (GST/FBT), UK",
        }
    }
}

impl HasVisualization for TransactionFacts {
    fn viz_spec() -> VisualizationSpec {
        VisualizationSpec {
            semantic_type: SemanticType::Legal,
            z_layer: ZLayer::Legal,
            rhai_dsl: RhaiDsl::new(r#"let facts = TransactionFacts()
    .with_vendor("AU")
    .with_supply_type("TAXABLE")
    .with_tax_code("G1")
    .with_amount("1100.00");
legal_solver.verify_all(rules, facts);"#),
            description: "Raw fact bundle fed to LegalSolver: vendor jurisdiction, supply type, tax code, amount, and activity flags",
        }
    }
}

// ============================================================================
// VALIDATION (z=1 gate / z=2 constraint layer)
// ============================================================================

impl HasVisualization for CommitGate {
    fn viz_spec() -> VisualizationSpec {
        VisualizationSpec {
            semantic_type: SemanticType::Gate,
            z_layer: ZLayer::Pipeline,
            rhai_dsl: RhaiDsl::new(r#"let gate = evaluate_commit_gate(stage_result);
switch gate {
    "Approved"        => commit_to_workbook(tx),
    "PendingOperator" => route_to_operator(tx, gate.reason),
    "Blocked"         => abort_commit(gate.issues),
}"#),
            description: "Approval gate before workbook commit: Approved/PendingOperator/Blocked based on confidence and issues",
        }
    }
}

impl HasVisualization for Issue {
    fn viz_spec() -> VisualizationSpec {
        VisualizationSpec {
            semantic_type: SemanticType::Issue,
            z_layer: ZLayer::Constraint,
            rhai_dsl: RhaiDsl::new(r#"let issue = Issue::unrecoverable("AMT_NEG", "amount is negative")
    .with_field("amount");
stage_result.add_issue(issue);"#),
            description: "Single typed validation issue with severity (Unrecoverable/Recoverable/Advisory), code, message, and field",
        }
    }
}

impl HasVisualization for MetaFlag {
    fn viz_spec() -> VisualizationSpec {
        VisualizationSpec {
            semantic_type: SemanticType::Flag,
            z_layer: ZLayer::Pipeline,
            rhai_dsl: RhaiDsl::new(r#"if vendor_is_new(tx.vendor) {
    let f = #{ type: "NewVendor", vendor: tx.vendor };
    attach_flag(f);
}
if anomaly_score > THRESHOLD {
    let f = #{ type: "AnomalyDetected", code: "AMT_SPIKE", impact: 0.9 };
    attach_flag(f);
}"#),
            description: "Classification meta-annotation: NewVendor, AnomalyDetected, RepairApplied, LowUpstreamConf, or ConstraintWeak",
        }
    }
}

// ============================================================================
// META CONTEXT AND DISPOSITION (z=1, Pipeline layer)
// ============================================================================

impl HasVisualization for MetaCtx {
    fn viz_spec() -> VisualizationSpec {
        VisualizationSpec {
            semantic_type: SemanticType::Pipeline,
            z_layer: ZLayer::Pipeline,
            rhai_dsl: RhaiDsl::new(r#"let meta = MetaCtx();
meta.accumulated_confidence = 0.85;
meta.flags.push(#{ type: "NewVendor", vendor: "ACME Corp" });
meta.stage_trace.push(#{ stage: "ingest", confidence: 0.9 });
meta.stage_trace.push(#{ stage: "validate", confidence: 0.8 });"#),
            description: "Accumulated pipeline state flowing forward: confidence score, classification flags, and per-stage trace history",
        }
    }
}

impl HasVisualization for Disposition {
    fn viz_spec() -> VisualizationSpec {
        VisualizationSpec {
            semantic_type: SemanticType::Result,
            z_layer: ZLayer::Pipeline,
            rhai_dsl: RhaiDsl::new(r#"switch issue.disposition {
    "Unrecoverable" => halt_pipeline("critical failure"),
    "Recoverable"   => continue_with_degraded_confidence(),
    "Advisory"      => log_and_continue(),
}"#),
            description: "Issue handling strategy: Unrecoverable halts pipeline, Recoverable continues with degraded confidence, Advisory is informational only",
        }
    }
}

impl<T: 'static> HasVisualization for StageResult<T> {
    fn viz_spec() -> VisualizationSpec {
        VisualizationSpec {
            semantic_type: SemanticType::Result,
            z_layer: ZLayer::Pipeline,
            rhai_dsl: RhaiDsl::new(r#"let result = StageResult::ok(data, confidence)
    .with_issues(issues);
if result.confidence >= MIN_CONF { next_stage(result.data) }
else { flag_low_confidence(result) }"#),
            description: "Pipeline stage output wrapper: typed data payload, confidence score, issues, and meta context",
        }
    }
}

// ============================================================================
// FORMAL PROOF (z=4, FormalProof layer)
// ============================================================================

impl HasVisualization for KasuariSolver {
    fn viz_spec() -> VisualizationSpec {
        VisualizationSpec {
            semantic_type: SemanticType::Proof,
            z_layer: ZLayer::FormalProof,
            rhai_dsl: RhaiDsl::new(r#"let solver = KasuariSolver;
let score = solver.evaluate("amount", value, [[min, max]]);
let strength = solver.strength("required"); // Required | Strong | Medium | Weak
// Bridges constraint satisfaction into formal layout verification"#),
            description: "Kasuari constraint layout solver — evaluates field values against (min, max) ranges, bridges constraint → formal verification layer",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_viz_spec_rhai_dsl_has_valid_syntax() {
        let engine = rhai::Engine::new();
        macro_rules! check {
            ($t:ty) => {{
                let spec = <$t as HasVisualization>::viz_spec();
                engine.compile(spec.rhai_dsl.source()).unwrap_or_else(|e| {
                    panic!(
                        "Rhai DSL syntax error in {}: {}",
                        stringify!($t),
                        e
                    )
                });
            }};
        }
        check!(PipelineState<Ingested>);
        check!(PipelineState<Validated>);
        check!(PipelineState<Classified>);
        check!(PipelineState<Reconciled>);
        check!(PipelineState<Committed>);
        check!(PipelineState<NeedsReview>);
        check!(ConstraintEvaluation);
        check!(VendorConstraintSet);
        check!(InvoiceConstraintSolver);
        check!(InvoiceVerification);
        check!(Z3Result);
        check!(LegalRule);
        check!(LegalSolver);
        check!(Jurisdiction);
        check!(TransactionFacts);
        check!(CommitGate);
        check!(Issue);
        check!(MetaFlag);
        check!(MetaCtx);
        check!(Disposition);
        check!(StageResult<()>);
        check!(KasuariSolver);
    }
}

/// Returns a map of type_id → Rhai DSL source for all registered visualization types.
///
/// Used by `ledgerr-mcp`'s `handle_manifest_tool` to expose the full canonical viz manifest.
pub fn canonical_viz_dsl_map() -> std::collections::BTreeMap<String, String> {
    use crate::iso::HasVisualization;
    let mut map = std::collections::BTreeMap::new();
    macro_rules! push {
        ($key:expr, $t:ty) => {
            let spec = <$t as HasVisualization>::viz_spec();
            map.insert($key.to_string(), spec.rhai_dsl.to_string());
        };
    }
    push!("PipelineState<Ingested>", PipelineState<Ingested>);
    push!("PipelineState<Classified>", PipelineState<Classified>);
    push!("PipelineState<Validated>", PipelineState<Validated>);
    push!("PipelineState<Reconciled>", PipelineState<Reconciled>);
    push!("PipelineState<Committed>", PipelineState<Committed>);
    push!("PipelineState<NeedsReview>", PipelineState<NeedsReview>);
    push!("ConstraintEvaluation", ConstraintEvaluation);
    push!("VendorConstraintSet", VendorConstraintSet);
    push!("InvoiceConstraintSolver", InvoiceConstraintSolver);
    push!("InvoiceVerification", InvoiceVerification);
    push!("Z3Result", Z3Result);
    push!("LegalRule", LegalRule);
    push!("LegalSolver", LegalSolver);
    push!("Jurisdiction", Jurisdiction);
    push!("TransactionFacts", TransactionFacts);
    push!("CommitGate", CommitGate);
    push!("Issue", Issue);
    push!("MetaFlag", MetaFlag);
    push!("MetaCtx", MetaCtx);
    push!("Disposition", Disposition);
    push!("StageResult", StageResult<()>);
    push!("KasuariSolver", KasuariSolver);
    map
}
