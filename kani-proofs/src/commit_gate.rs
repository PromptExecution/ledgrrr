use ledger_core::pipeline::{evaluate_commit_gate, PipelineState, Reconciled};
use ledger_core::validation::CommitGate;

#[kani::proof]
fn commit_gate_is_total() {
    let confidence: f32 = kani::any();
    kani::assume(confidence >= 0.0 && confidence <= 1.0);
    let state = PipelineState::<Reconciled>::new_for_kani(confidence);
    let gate = evaluate_commit_gate(&state, 0.85);
    assert!(matches!(
        gate,
        CommitGate::Approved { .. } | CommitGate::PendingOperator { .. } | CommitGate::Blocked { .. }
    ));
}
