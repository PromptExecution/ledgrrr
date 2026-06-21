use ledger_core::constraints::InvoiceConstraintSolver;

#[kani::proof]
fn invoice_required_pass_iff_arithmetic_holds() {
    let total: f64 = kani::any();
    let subtotal: f64 = kani::any();
    let gst: f64 = kani::any();
    kani::assume(total.is_finite() && subtotal.is_finite() && gst.is_finite());
    kani::assume(total > 0.0 && total < 1_000_000.0);
    let solver = InvoiceConstraintSolver::new();
    let result = solver.validate(total, subtotal, gst);
    let arith_ok = (total - subtotal - gst).abs() < 0.01;
    assert_eq!(result.required_pass, arith_ok);
}
