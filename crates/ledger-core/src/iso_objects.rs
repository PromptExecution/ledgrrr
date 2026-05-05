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
                r#"let eval = constraint_set.evaluate(amount, day, code, acct);
if eval.required_pass { classify_ok() } else { flag("constraint_fail") }"#,
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
let eval = bounds.evaluate(amount, day_of_month, tax_code, account);
emit_constraint_result(eval);"#),
            description: "Vendor-specific statistical bounds: amount percentiles, usual day-of-month, tax code, and account",
        }
    }
}

impl HasVisualization for InvoiceConstraintSolver {
    fn viz_spec() -> VisualizationSpec {
        VisualizationSpec {
            semantic_type: SemanticType::Solver,
            z_layer: ZLayer::Constraint,
            rhai_dsl: RhaiDsl::new(r#"let solver = InvoiceConstraintSolver::new(gst_rate, expected_net);
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
match result {
    Satisfied  => ok(),
    Violated   => flag("legal_violation"),
    Unknown    => flag("legal_unknown"),
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
                r#"let rule = LegalRule::new(jurisdiction, "au-gst-38-190")
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
            rhai_dsl: RhaiDsl::new(r#"let solver = LegalSolver::new();
let (confidence, issues) = solver.verify_all(jurisdiction.legal_ruleset(), facts);
if issues.is_empty() { advance_pipeline() } else { route_review(issues) }"#),
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
            rhai_dsl: RhaiDsl::new(r#"let facts = TransactionFacts::new()
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
match gate {
    Approved         => commit_to_workbook(tx),
    PendingOperator  => route_to_operator(tx, gate.reason),
    Blocked          => abort_commit(gate.issues),
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
    attach_flag(MetaFlag::NewVendor { vendor: tx.vendor });
}
if anomaly_score > THRESHOLD {
    attach_flag(MetaFlag::AnomalyDetected { code: "AMT_SPIKE", impact: 0.9 });
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
            rhai_dsl: RhaiDsl::new(r#"let meta = MetaCtx::default();
meta.accumulated_confidence = 0.85;
meta.flags.push(MetaFlag::NewVendor { vendor: "ACME Corp" });
meta.stage_trace.push(StageScore { stage: "ingest", confidence: 0.9 });
meta.stage_trace.push(StageScore { stage: "validate", confidence: 0.8 });"#),
            description: "Accumulated pipeline state flowing forward: confidence score, classification flags, and per-stage trace history",
        }
    }
}

impl HasVisualization for Disposition {
    fn viz_spec() -> VisualizationSpec {
        VisualizationSpec {
            semantic_type: SemanticType::Result,
            z_layer: ZLayer::Pipeline,
            rhai_dsl: RhaiDsl::new(r#"match issue.disposition {
    Disposition::Unrecoverable => halt_pipeline("critical failure"),
    Disposition::Recoverable  => continue_with_degraded_confidence(),
    Disposition::Advisory     => log_and_continue(),
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
let score = solver.evaluate("amount", value, [(min, max)]);
let strength = solver.strength("required"); // Required | Strong | Medium | Weak
// Bridges constraint satisfaction into formal layout verification"#),
            description: "Kasuari constraint layout solver — evaluates field values against (min, max) ranges, bridges constraint → formal verification layer",
        }
    }
}
