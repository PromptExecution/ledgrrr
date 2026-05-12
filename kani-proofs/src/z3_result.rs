use ledger_core::legal::Z3Result;

#[kani::proof]
fn z3_satisfied_confidence_is_one() {
    let r = Z3Result::Satisfied;
    assert_eq!(r.to_confidence(), 1.0_f32);
}

#[kani::proof]
fn z3_violated_confidence_is_zero() {
    let r = Z3Result::Violated { witness: String::new() };
    assert_eq!(r.to_confidence(), 0.0_f32);
}

#[kani::proof]
fn z3_unknown_confidence_is_half() {
    let r = Z3Result::Unknown;
    assert_eq!(r.to_confidence(), 0.5_f32);
}
