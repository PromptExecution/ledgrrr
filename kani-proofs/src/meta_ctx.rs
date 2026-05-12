use ledger_core::validation::MetaCtx;

#[kani::proof]
fn meta_ctx_confidence_stays_bounded() {
    let c1: f32 = kani::any();
    let c2: f32 = kani::any();
    kani::assume(c1 >= 0.0 && c1 <= 1.0);
    kani::assume(c2 >= 0.0 && c2 <= 1.0);

    let ctx = MetaCtx::default();
    let ctx = ctx.advance("s1", c1, &[]);
    assert!(ctx.accumulated_confidence >= 0.0 && ctx.accumulated_confidence <= 1.0);
    let ctx = ctx.advance("s2", c2, &[]);
    assert!(ctx.accumulated_confidence >= 0.0 && ctx.accumulated_confidence <= 1.0);
}
