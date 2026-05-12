use ledger_core::constraints::VendorConstraintSet;

#[kani::proof]
fn vendor_required_pass_iff_nonzero_amount() {
    let amount: f64 = kani::any();
    kani::assume(amount.is_finite());
    let vendor = VendorConstraintSet {
        vendor_id: String::new(),
        amount_p05: 0.0,
        amount_p95: 1_000_000.0,
        usual_day_of_month: None,
        usual_tax_code: String::new(),
        usual_account: String::new(),
    };
    let result = vendor.evaluate(amount, 1, "", "");
    assert_eq!(result.required_pass, amount != 0.0);
    assert!(result.strong_ratio >= 0.0 && result.strong_ratio <= 1.0);
}
