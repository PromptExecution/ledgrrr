//! Attestation trait — types with formal verification backing.

/// Metadata describing a type's formal verification coverage.
pub struct AttestationSpec {
    pub invariant: &'static str,
    pub z3_predicate: Option<&'static str>,
    pub kasuari_description: Option<&'static str>,
    pub kani_module: Option<&'static str>,
}

/// Implemented by types that carry formal property attestations.
///
/// # Compile-fail: missing impl
/// ```compile_fail
/// use ledger_attest::attested;
/// use ledger_core::attest::Attested;
///
/// #[attested("missing")]
/// pub struct MissingImpl { pub x: u32 }
/// ```
pub trait Attested {
    fn attestation_spec() -> AttestationSpec;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constraints::{ConstraintEvaluation, InvoiceVerification};
    use crate::validation::CommitGate;

    #[test]
    fn attestation_spec_invariant_strings_are_correct() {
        assert_eq!(
            ConstraintEvaluation::attestation_spec().invariant,
            "constraint_evaluation_bounded"
        );
        assert_eq!(
            InvoiceVerification::attestation_spec().invariant,
            "invoice_arithmetic_valid"
        );
        assert_eq!(CommitGate::attestation_spec().invariant, "commit_gate_total");
    }

    #[test]
    fn attestation_spec_kani_modules_are_set() {
        assert!(ConstraintEvaluation::attestation_spec().kani_module.is_some());
        assert!(InvoiceVerification::attestation_spec().kani_module.is_some());
        assert!(CommitGate::attestation_spec().kani_module.is_some());
    }
}
