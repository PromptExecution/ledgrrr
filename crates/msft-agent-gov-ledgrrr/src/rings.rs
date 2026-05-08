//! Maps ledgrrr's CommitGate tiers to AGT execution rings and defines which
//! tools require operator approval before proceeding.

use agentmesh::{Ring, RingEnforcer};

/// Tools that always require operator approval (Ring::Admin escalation path).
/// Maps to `CommitGate::PendingOperator` in ledger-core.
pub const APPROVAL_REQUIRED_TOOLS: &[&str] = &[
    "ledgerr_reconciliation",
    "ledgerr_workflow",
];

/// Configure the default per-ring action permissions for ledgrrr.
///
/// Ring mapping:
///
/// | Ring        | Trust     | CommitGate tier    | Operations                              |
/// |-------------|-----------|--------------------|-----------------------------------------|
/// | Admin  (0)  | 900–1000  | Approved (commit)  | All ops including commit/reverse        |
/// | Standard(1) | 500–899   | Approved (write)   | Ingest, classify, read, ontology, focus |
/// | Restricted(2)| 300–499  | PendingOperator    | Read-only + evidence queries            |
/// | Sandboxed(3)| 0–299     | Blocked            | Nothing                                 |
pub fn configure_default_rings(enforcer: &mut RingEnforcer) {
    enforcer.set_ring_permissions(Ring::Admin, vec![
        // All ledgerr tools — Admin ring has implicit allow, but list for documentation.
        "ledgerr_documents.*".to_string(),
        "ledgerr_review.*".to_string(),
        "ledgerr_reconciliation.*".to_string(),
        "ledgerr_workflow.*".to_string(),
        "ledgerr_audit.*".to_string(),
        "ledgerr_tax.*".to_string(),
        "ledgerr_ontology.*".to_string(),
        "ledgerr_xero.*".to_string(),
        "ledgerr_evidence.*".to_string(),
        "ledgerr_focus.*".to_string(),
    ]);

    enforcer.set_ring_permissions(Ring::Standard, vec![
        "ledgerr_documents.list_accounts".to_string(),
        "ledgerr_documents.get_raw_context".to_string(),
        "ledgerr_documents.ingest_pdf".to_string(),
        "ledgerr_documents.ingest_rows".to_string(),
        "ledgerr_review.*".to_string(),
        "ledgerr_workflow.run_rhai_rule".to_string(),
        "ledgerr_workflow.classify_ingested".to_string(),
        "ledgerr_workflow.query_flags".to_string(),
        "ledgerr_workflow.classify_transaction".to_string(),
        "ledgerr_audit.query_audit_log".to_string(),
        "ledgerr_tax.get_schedule_summary".to_string(),
        "ledgerr_tax.export_cpa_workbook".to_string(),
        "ledgerr_ontology.*".to_string(),
        "ledgerr_evidence.*".to_string(),
        "ledgerr_focus.*".to_string(),
    ]);

    enforcer.set_ring_permissions(Ring::Restricted, vec![
        "ledgerr_documents.list_accounts".to_string(),
        "ledgerr_documents.get_raw_context".to_string(),
        "ledgerr_audit.query_audit_log".to_string(),
        "ledgerr_tax.get_schedule_summary".to_string(),
        "ledgerr_evidence.summary".to_string(),
        "ledgerr_evidence.list_nodes".to_string(),
        "ledgerr_evidence.node_detail".to_string(),
        "ledgerr_focus.*".to_string(),
    ]);

    // Ring::Sandboxed — RingEnforcer always denies with no permissions configured.
}

/// Map an AGT `TrustScore` to the appropriate `Ring`.
/// Called when an agent's trust score changes to update their ring assignment.
pub fn ring_for_trust(score: u32) -> Ring {
    match score {
        900..=1000 => Ring::Admin,
        500..=899  => Ring::Standard,
        300..=499  => Ring::Restricted,
        _          => Ring::Sandboxed,
    }
}
