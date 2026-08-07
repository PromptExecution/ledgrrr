// This module was a #[kani::proof] asserting that evaluate_commit_gate()'s return
// value matches one of CommitGate's three variants — a tautology of Rust's type
// system (any CommitGate value trivially matches one of its own variants) rather
// than a real business-logic property, and the harness's use of
// PipelineState::new_for_kani (which always constructs an empty `issues: Vec::new()`)
// meant the Blocked variant was never even reachable from it.
//
// It was also the actual root cause of every "Kani model checking" CI failure on
// #164/#165 (and, by inheritance, #162): evaluate_commit_gate's PendingOperator
// branch builds `reason` via `format!("... {:.2} ... {:.2}", ...)`, and CBMC's
// automatic loop unwinding cannot bound the data-dependent loops inside Rust's
// float-to-decimal formatting internals (core::num::flt2dec::round_up), so the
// harness unwound indefinitely. Removing the precision specifier doesn't help —
// even plain Display formatting on a symbolic value pulls in the equally
// unboundable core::unicode::unicode_data::skip_search tables (confirmed locally
// by running `cargo kani` directly rather than guessing from CI's opaque 5-6h
// timeouts). This is a known, general Kani limitation: `format!`/Display in code
// reachable from a proof harness routinely defeats automatic unwinding.
//
// Downgraded to concrete tests covering the three real branches (Approved,
// PendingOperator, Blocked) below — meaningfully more coverage than the original
// vacuous proof had, since it never exercised Blocked at all.
#[cfg(test)]
mod tests {
    use ledger_core::pipeline::{evaluate_commit_gate, PipelineState, Reconciled};
    use ledger_core::validation::{CommitGate, Issue};

    fn state(confidence: f32) -> PipelineState<Reconciled> {
        PipelineState::new("doc-1", "source/statement.rkyv").with_confidence(confidence)
    }

    #[test]
    fn approved_when_confidence_at_or_above_threshold() {
        let gate = evaluate_commit_gate(&state(0.85), 0.85);
        assert!(matches!(gate, CommitGate::Approved { confidence } if confidence == 0.85));
    }

    #[test]
    fn pending_operator_when_confidence_below_threshold() {
        let gate = evaluate_commit_gate(&state(0.5), 0.85);
        match gate {
            CommitGate::PendingOperator { confidence, reason } => {
                assert_eq!(confidence, 0.5);
                assert!(reason.contains("0.5") && reason.contains("0.85"));
            }
            other => panic!("expected PendingOperator, got {other:?}"),
        }
    }

    #[test]
    fn blocked_when_unrecoverable_issue_present() {
        let mut s = state(1.0);
        s.issues.push(Issue::unrecoverable("AMT_NEG", "amount is negative"));
        let gate = evaluate_commit_gate(&s, 0.85);
        match gate {
            CommitGate::Blocked { issues } => assert_eq!(issues.len(), 1),
            other => panic!("expected Blocked, got {other:?}"),
        }
    }

    #[test]
    fn blocked_takes_priority_over_low_confidence() {
        let mut s = state(0.0);
        s.issues.push(Issue::unrecoverable("AMT_NEG", "amount is negative"));
        let gate = evaluate_commit_gate(&s, 0.85);
        assert!(matches!(gate, CommitGate::Blocked { .. }));
    }
}
