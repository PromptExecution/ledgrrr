# Kani Proof Inventory

| Harness | File | Target | Status |
|---|---|---|---|
| `invoice_required_pass_iff_arithmetic_holds` | `src/invoice_arithmetic.rs` | `InvoiceConstraintSolver::validate` | passing |
| `vendor_required_pass_iff_nonzero_amount` | `src/vendor_constraints.rs` | `VendorConstraintSet::evaluate` | passing |
| `commit_gate_is_total` | `src/commit_gate.rs` | `evaluate_commit_gate` | passing |
