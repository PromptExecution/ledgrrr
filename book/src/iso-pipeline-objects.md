# Isometric Pipeline Objects

Every domain type in the PRD-6 / PRD-7 pipeline implements the `HasVisualization` trait,
which pins it to a `ZLayer`, a `SemanticType`, and a Rhai DSL snippet.
This contract is enforced by the 20 lint tests in `crates/ledger-core/tests/iso_lint.rs`.

## ZLayer Stack

The 3-D space maps pipeline semantics to the Z axis:

| Z | Layer | Color | Base-Z | Types |
|---|-------|-------|--------|-------|
| 0 | Document | `#334155` | 0.0 | Raw ingestion, file surface |
| 1 | Pipeline | `#1d4ed8` | 136.0 | `PipelineState<S>`, `StageResult`, `CommitGate`, `MetaFlag` |
| 2 | Constraint | `#7c3aed` | 272.0 | `ConstraintEvaluation`, `VendorConstraintSet`, `InvoiceConstraintSolver`, `InvoiceVerification`, `Issue` |
| 3 | Legal | `#b91c1c` | 408.0 | `Z3Result`, `LegalRule`, `LegalSolver`, `Jurisdiction`, `TransactionFacts` |
| 4 | FormalProof | `#0f766e` | 544.0 | `KasuariSolver` |
| 5 | Attestation | `#b45309` | 680.0 | PRD-6-FUTURE: `InvariantEntry`, `AttestationSpec` |

The X axis encodes pipeline progress (0 → committed); the Y axis encodes confidence lift.

## Projection Formula

```rust
// iso_project — matches JS rhai-live-core.js isoProject()
screen_x = origin_x + (pt.x - pt.z) * scale * 0.866
screen_y = origin_y + (pt.x + pt.z) * scale * 0.5 - pt.y * scale
```

Contract test: `iso_project(Vec3{x:192, y:0, z:0}, 1.0, 0.0, 0.0)` → `IsoProjected{screen_x:≈166.27, screen_y:96.0}`.

## Rendered Mini-DSL Samples

These samples use the supported Rhai diagram mini-DSL so the mdBook preprocessor
can emit Mermaid blocks for the GitHub Pages build.

```rhai
fn raw_statement() -> extracted_rows
fn extracted_rows() -> deterministic_tx_ids
fn deterministic_tx_ids() -> validated_facts
fn validated_facts() -> classified_tx
fn classified_tx() -> workbook_projection
```

```rhai
if constraint_passed == true -> legal_verification
if constraint_passed == false -> operator_review
if recovered == true -> legal_verification
if recovered == false -> blocked_queue
```

```rhai
match issue.disposition => Unrecoverable -> blocked_queue
match issue.disposition => Recoverable -> repair_pipeline
match issue.disposition => Advisory -> workbook_projection
```

```rhai
fn z3_rule_check() -> proof_result
if proof_result == satisfied -> commit_gate
if proof_result == violated -> legal_review
if proof_result == unknown -> operator_review
```

```rhai
fn commit_gate() -> audit_log
fn audit_log() -> evidence_graph
fn evidence_graph() -> cpa_workbook
fn cpa_workbook() -> exported_artifact
```

```rhai
fn operator_review() -> approval_decision
fn approval_decision() -> replayable_audit_event
fn replayable_audit_event() -> evidence_graph
```

## Pipeline Layer (z=1)

### `PipelineState<Ingested>`

**Semantic:** `Pipeline` | **Z:** 1 | **Color:** `#1d4ed8`

Raw ingested transaction — structure validated, awaiting constraint pass.

```rhai
let tx = ingest(pdf_path);
check_constraints(tx, constraint_set);
```

---

### `PipelineState<Validated>`

**Semantic:** `Pipeline` | **Z:** 1 | **Color:** `#1d4ed8`

Post-constraint validated transaction — all numerical bounds passed, awaiting legal verification.

```rhai
let validated = tx.validate(constraint_set);
if validated.confidence >= MIN_CONF { route_to_legal(validated) }
else { flag("low_confidence") }
```

---

### `PipelineState<Classified>`

**Semantic:** `Pipeline` | **Z:** 1 | **Color:** `#1d4ed8`

Legal-verified transaction with tax category assigned — ready for workbook reconciliation.

```rhai
let classified = legal_verified_tx.classify(rules);
set_category(classified, tax_category);
emit_to_workbook(classified);
```

---

### `PipelineState<Reconciled>`

**Semantic:** `Pipeline` | **Z:** 1 | **Color:** `#1d4ed8`

Transaction matched against workbook entries — commit gate evaluation pending.

```rhai
let reconciled = match_workbook(classified_tx, workbook);
if reconciled.matched { open_commit_gate(reconciled) }
else { flag("unmatched_entry") }
```

---

### `PipelineState<Committed>`

**Semantic:** `Pipeline` | **Z:** 1 | **Color:** `#1d4ed8`

Committed to workbook — final immutable state, audit trail emitted.

```rhai
let committed = commit_gate.approve(reconciled_tx);
write_xlsx(committed);
emit_audit_trail(committed.id);
```

---

### `PipelineState<NeedsReview>`

**Semantic:** `Pipeline` | **Z:** 1 | **Color:** `#1d4ed8`

Legal verification failed — operator flag set, transaction held for manual review.

```rhai
let review = legal_fail(tx, z3_result);
flag_operator("legal_violation", review.rule_id);
route_to_review_queue(review);
```

---

### `CommitGate`

**Semantic:** `Gate` | **Z:** 1 | **Color:** `#1d4ed8`

Approval gate before workbook commit: `Approved` / `PendingOperator` / `Blocked` based on confidence and issues.

```rhai
let gate = evaluate_commit_gate(stage_result);
match gate {
    Approved         => commit_to_workbook(tx),
    PendingOperator  => route_to_operator(tx, gate.reason),
    Blocked          => abort_commit(gate.issues),
}
```

---

### `MetaFlag`

**Semantic:** `Flag` | **Z:** 1 | **Color:** `#1d4ed8`

Classification meta-annotation: `NewVendor`, `AnomalyDetected`, `RepairApplied`, `LowUpstreamConf`, or `ConstraintWeak`.

```rhai
if vendor_is_new(tx.vendor) {
    attach_flag(MetaFlag::NewVendor { vendor: tx.vendor });
}
if anomaly_score > THRESHOLD {
    attach_flag(MetaFlag::AnomalyDetected { code: "AMT_SPIKE", impact: 0.9 });
}
```

---

### `StageResult<T>`

**Semantic:** `Result` | **Z:** 1 | **Color:** `#1d4ed8`

Pipeline stage output wrapper: typed data payload, confidence score, issues, and meta context.

```rhai
let result = StageResult::ok(data, confidence)
    .with_issues(issues);
if result.confidence >= MIN_CONF { next_stage(result.data) }
else { flag_low_confidence(result) }
```

## Constraint Layer (z=2)

### `ConstraintEvaluation`

**Semantic:** `Result` | **Z:** 2 | **Color:** `#7c3aed`

Numerical constraint evaluation result with pass/fail per-field scores.

```rhai
let eval = constraint_set.evaluate(amount, day, code, acct);
if eval.required_pass { classify_ok() } else { flag("constraint_fail") }
```

---

### `VendorConstraintSet`

**Semantic:** `Constraint` | **Z:** 2 | **Color:** `#7c3aed`

Vendor-specific statistical bounds: amount percentiles, usual day-of-month, tax code, and account.

```rhai
let bounds = load_vendor_constraints(vendor_id);
let eval = bounds.evaluate(amount, day_of_month, tax_code, account);
emit_constraint_result(eval);
```

---

### `InvoiceConstraintSolver`

**Semantic:** `Solver` | **Z:** 2 | **Color:** `#7c3aed`

Invoice GST arithmetic solver — checks gross/net/GST consistency and rate conformance.

```rhai
let solver = InvoiceConstraintSolver::new(gst_rate, expected_net);
let verification = solver.verify(gross, gst_amount);
if verification.arithmetic_ok && verification.gst_rate_ok { pass() }
```

---

### `InvoiceVerification`

**Semantic:** `Result` | **Z:** 2 | **Color:** `#7c3aed`

Invoice verification result — `arithmetic_ok` and `gst_rate_ok` flags with audit note.

```rhai
let v = invoice_solver.verify(gross, gst);
if !v.arithmetic_ok { flag("arithmetic_mismatch", v.audit_note) }
if !v.gst_rate_ok   { flag("gst_rate_mismatch", v.audit_note) }
```

---

### `Issue`

**Semantic:** `Issue` | **Z:** 2 | **Color:** `#7c3aed`

Single typed validation issue with severity (`Unrecoverable` / `Recoverable` / `Advisory`), code, message, and field.

```rhai
let issue = Issue::unrecoverable("AMT_NEG", "amount is negative")
    .with_field("amount");
stage_result.add_issue(issue);
```

## Legal Layer (z=3)

### `Z3Result`

**Semantic:** `Result` | **Z:** 3 | **Color:** `#b91c1c`

Symbolic satisfiability outcome from Z3-style legal predicate check: `Satisfied` / `Violated` / `Unknown`.

```rhai
let result = legal_solver.verify(rule, facts);
match result {
    Satisfied  => ok(),
    Violated   => flag("legal_violation"),
    Unknown    => flag("legal_unknown"),
}
```

---

### `LegalRule`

**Semantic:** `Legal` | **Z:** 3 | **Color:** `#b91c1c`

Single jurisdiction-bound legal rule: threshold, exclusion, or benefit predicate.

```rhai
let rule = LegalRule::new(jurisdiction, "au-gst-38-190")
    .with_formula("supply_type == 'GST_FREE' && vendor_jurisdiction == 'AU'")
    .with_category("GST");
legal_solver.verify(rule, facts);
```

---

### `LegalSolver`

**Semantic:** `Solver` | **Z:** 3 | **Color:** `#b91c1c`

Runs all jurisdiction rules against `TransactionFacts`, returns aggregate confidence and issue list.

```rhai
let solver = LegalSolver::new();
let (confidence, issues) = solver.verify_all(jurisdiction.legal_ruleset(), facts);
if issues.is_empty() { advance_pipeline() } else { route_review(issues) }
```

---

### `Jurisdiction`

**Semantic:** `Legal` | **Z:** 3 | **Color:** `#b91c1c`

Jurisdiction enum controlling which legal ruleset applies: US (FBAR/FEIE), AU (GST/FBT), UK.

```rhai
let j = Jurisdiction::AU;
let rules = j.legal_ruleset();
// US -> FBAR/FEIE rules; AU -> GST/FBT rules
let code = j.code(); // "US" | "AU" | "UK"
```

---

### `TransactionFacts`

**Semantic:** `Legal` | **Z:** 3 | **Color:** `#b91c1c`

Raw fact bundle fed to `LegalSolver`: vendor jurisdiction, supply type, tax code, amount, and activity flags.

```rhai
let facts = TransactionFacts::new()
    .with_vendor("AU")
    .with_supply_type("TAXABLE")
    .with_tax_code("G1")
    .with_amount("1100.00");
legal_solver.verify_all(rules, facts);
```

## Formal Proof Layer (z=4)

### `KasuariSolver`

**Semantic:** `Proof` | **Z:** 4 | **Color:** `#0f766e`

Kasuari constraint layout solver — evaluates field values against `(min, max)` ranges, bridges constraint → formal verification layer.

```rhai
let solver = KasuariSolver;
let score = solver.evaluate("amount", value, [(min, max)]);
let strength = solver.strength("required"); // Required | Strong | Medium | Weak
// Bridges constraint satisfaction into formal layout verification
```

## Attestation Layer (z=5)

Populated in **PRD-6-FUTURE** once the `ledger-attest` proc-macro crate is implemented.
Planned types: `InvariantEntry`, `AttestationSpec`, `InvariantRegistry`.

## Running the Lint Suite

```sh
cargo test -p ledger-core --test iso_lint
```

All 20 lint tests assert per-object: non-empty `description`, non-empty `rhai_dsl`,
`z_layer.index() <= 5`, and non-empty `semantic_type.known_name()`.

## Animation Backends

| Phase | Backend | Output |
|-------|---------|--------|
| 0 | SVG SMIL (`IsoAnimationPath::to_smil_svg`) | Inline `<animateTransform>` |
| 1 | rerun.io | Interactive 3D timeline |
| 2 | manim Python stub (`IsoAnimationPath::to_manim_script`) | Rendered video |

See `crates/ledger-core/src/iso.rs` for `IsoTransform`, `IsoEasing`, and `IsoAnimationPath`.
