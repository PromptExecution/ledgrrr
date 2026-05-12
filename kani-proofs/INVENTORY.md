# Kani Proof Inventory

| Harness | File | Target | Status |
|---|---|---|---|
| `invoice_required_pass_iff_arithmetic_holds` | `src/invoice_arithmetic.rs` | `InvoiceConstraintSolver::validate` | passing |
| `vendor_required_pass_iff_nonzero_amount` | `src/vendor_constraints.rs` | `VendorConstraintSet::evaluate` | passing |
| `commit_gate_is_total` | `src/commit_gate.rs` | `evaluate_commit_gate` | passing |
| `z3_satisfied_confidence_is_one` | `src/z3_result.rs` | `Z3Result::to_confidence` | passing |
| `z3_violated_confidence_is_zero` | `src/z3_result.rs` | `Z3Result::to_confidence` | passing |
| `z3_unknown_confidence_is_half` | `src/z3_result.rs` | `Z3Result::to_confidence` | passing |
| `meta_ctx_confidence_stays_bounded` | `src/meta_ctx.rs` | `MetaCtx::advance` | passing |
