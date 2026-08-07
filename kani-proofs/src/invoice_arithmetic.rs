// This module was a #[kani::proof] that symbolically verified InvoiceConstraintSolver::
// validate()'s `required_pass` field against an independently-derived formula for
// all finite f64 total/subtotal/gst. The property is a syntactic tautology (both
// sides compute the identical `(total - subtotal - gst).abs() < 0.01` expression),
// but CBMC's default bit-blasting SAT solver (CaDiCaL) could not discharge the
// accompanying NaN safety-checks for three interacting f64 operands within bounded
// CI time — every real CI run took 5-6+ hours before the runner lost communication
// with GitHub Actions. Native SMT solvers (Z3, Bitwuzla) solve the same harness in
// ~0.1s, but the CBMC/kani-driver integration reports those NaN checks as ERROR
// rather than SUCCESS, which is not a genuine pass — so they aren't a safe substitute.
//
// Downgraded to concrete/edge-case coverage below. The remaining kani-proofs
// harnesses (commit_gate, vendor_constraints, z3_result, meta_ctx — all f32, all
// solve in seconds) retain full symbolic proof.
#[cfg(test)]
mod tests {
    use ledger_core::constraints::InvoiceConstraintSolver;

    fn check(total: f64, subtotal: f64, gst: f64) {
        let solver = InvoiceConstraintSolver::new();
        let result = solver.validate(total, subtotal, gst);
        let arith_ok = (total - subtotal - gst).abs() < 0.01;
        assert_eq!(result.required_pass, arith_ok);
    }

    #[test]
    fn invoice_required_pass_holds_on_exact_match() {
        check(1100.0, 1000.0, 100.0);
    }

    #[test]
    fn invoice_required_pass_fails_on_mismatch() {
        check(1100.0, 1000.0, 50.0);
    }

    #[test]
    fn invoice_required_pass_holds_on_all_zero() {
        check(0.0, 0.0, 0.0);
    }

    #[test]
    fn invoice_required_pass_holds_just_inside_tolerance() {
        check(1100.005, 1000.0, 100.0);
    }

    #[test]
    fn invoice_required_pass_fails_just_outside_tolerance() {
        check(1100.02, 1000.0, 100.0);
    }

    #[test]
    fn invoice_required_pass_holds_with_negative_subtotal() {
        check(100.0, -50.0, 150.0);
    }

    #[test]
    fn invoice_required_pass_holds_near_million_boundary() {
        check(999_999.99, 909_090.9, 90_909.09);
    }
}
