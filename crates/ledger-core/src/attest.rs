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
    use crate::constraints::{ConstraintEvaluation, InvoiceVerification, VendorConstraintSet};
    use crate::legal::Z3Result;
    use crate::pipeline::DocumentFields;
    use crate::validation::{CommitGate, MetaCtx};
    use crate::workbook::AuditRow;

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
        assert_eq!(
            Z3Result::attestation_spec().invariant,
            "z3_result_confidence_total"
        );
        assert_eq!(
            MetaCtx::attestation_spec().invariant,
            "meta_ctx_confidence_bounded"
        );
        assert_eq!(
            DocumentFields::attestation_spec().invariant,
            "document_fields_decimal_safe"
        );
        assert_eq!(
            VendorConstraintSet::attestation_spec().invariant,
            "vendor_constraint_bounds_ordered"
        );
        assert_eq!(
            AuditRow::attestation_spec().invariant,
            "audit_row_entry_id_deterministic"
        );
    }

    #[test]
    fn attestation_spec_kani_modules_are_set() {
        assert!(ConstraintEvaluation::attestation_spec().kani_module.is_some());
        assert!(InvoiceVerification::attestation_spec().kani_module.is_some());
        assert!(CommitGate::attestation_spec().kani_module.is_some());
        assert!(Z3Result::attestation_spec().kani_module.is_some());
        assert!(MetaCtx::attestation_spec().kani_module.is_some());
        assert!(VendorConstraintSet::attestation_spec().kani_module.is_some());
    }
}
