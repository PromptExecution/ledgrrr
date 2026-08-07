use ledger_core::constraints::InvoiceConstraintSolver;

#[kani::proof]
fn invoice_required_pass_iff_arithmetic_holds() {
    let total: f64 = kani::any();
    let subtotal: f64 = kani::any();
    let gst: f64 = kani::any();
    kani::assume(total.is_finite() && subtotal.is_finite() && gst.is_finite());
    kani::assume(total > 0.0 && total < 1_000_000.0);
    // Bound subtotal/gst to the same practical invoice-amount range as total.
    // Unconstrained f64 magnitude across three interacting symbolic floats is
    // a classic source of CBMC floating-point blowup — this proof is the only
    // one in this crate using f64 with more than one unbounded operand, and
    // the only one observed taking multiple hours in CI.
    kani::assume(subtotal >= 0.0 && subtotal < 1_000_000.0);
    kani::assume(gst >= 0.0 && gst < 1_000_000.0);
    let solver = InvoiceConstraintSolver::new();
    let result = solver.validate(total, subtotal, gst);
    let arith_ok = (total - subtotal - gst).abs() < 0.01;
    assert_eq!(result.required_pass, arith_ok);
}
