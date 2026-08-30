//! Maps ledgrrr's CommitGate tiers to AGT execution rings and defines which
//! tools require operator approval before proceeding.
//!
//! Also derives `tools/list` **visibility** (which top-level `ledgerr_*` tool
//! families a ring should see) from the same action-pattern lists used for
//! call-time `RingEnforcer` permissions, so the two never drift apart. See
//! `ring_visible_tool_families` and `docs/progressive-tool-scoping-design.md`
//! for the full design (issue #222).

use std::collections::BTreeSet;

use agentmesh::{Ring, RingEnforcer};

/// Tools that always require operator approval (Ring::Admin escalation path).
/// Maps to `CommitGate::PendingOperator` in ledger-core.
pub const APPROVAL_REQUIRED_TOOLS: &[&str] = &[
    "ledgerr_reconciliation",
    "ledgerr_workflow",
];

/// Top-level tool families that sit outside the AGT ring model entirely
/// (not part of `PUBLISHED_TOOL_NAMES` / any ring's action-pattern list
/// below) and are always visible in `tools/list` regardless of the
/// caller's ring — including an unset/unknown ring.
///
/// Both are read-only, self-describing introspection surfaces (schema
/// listing, canonical DSL manifest) with no destructive actions, which is
/// why they're a safe "core group" per issue #222. This does **not** grant
/// them a call-time bypass — `tools/call` dispatch is unchanged by ring
/// filtering; only `tools/list` visibility is affected.
pub const CORE_TOOL_FAMILIES: &[&str] = &["ledgerr_schema", "ledgerr_manifest"];

fn admin_action_patterns() -> Vec<String> {
    vec![
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
    ]
}

fn standard_action_patterns() -> Vec<String> {
    vec![
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
    ]
}

fn restricted_action_patterns() -> Vec<String> {
    vec![
        "ledgerr_documents.list_accounts".to_string(),
        "ledgerr_documents.get_raw_context".to_string(),
        "ledgerr_audit.query_audit_log".to_string(),
        "ledgerr_tax.get_schedule_summary".to_string(),
        "ledgerr_evidence.summary".to_string(),
        "ledgerr_evidence.list_nodes".to_string(),
        "ledgerr_evidence.node_detail".to_string(),
        "ledgerr_focus.*".to_string(),
    ]
}

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
///
/// # Known limitation (tracked in issue #222)
///
/// This configures `RingEnforcer`'s per-ring permission table, but
/// `LedgrrAgtGateway::check_tool_call` currently only consults
/// `RingEnforcer::get_ring` (for the Admin-bypass / Sandboxed-deny shortcuts)
/// — the actual Allow/Deny/RequiresApproval decision for Standard and
/// Restricted rings comes from `AgentMeshClient::execute_with_governance`,
/// which evaluates the single shared `LEDGERR_POLICY_YAML` policy with no
/// ring parameter at all. In other words: **the permission lists below are
/// not yet consulted for call-time authorization** — Standard and Restricted
/// rings are currently authorized identically at `tools/call` time. They
/// power `ring_visible_tool_families` (`tools/list` filtering) as of this
/// change, which is a real, load-bearing use — but ring-differentiated
/// *call-time* enforcement is separate follow-up work.
pub fn configure_default_rings(enforcer: &mut RingEnforcer) {
    enforcer.set_ring_permissions(Ring::Admin, admin_action_patterns());
    enforcer.set_ring_permissions(Ring::Standard, standard_action_patterns());
    enforcer.set_ring_permissions(Ring::Restricted, restricted_action_patterns());

    // Ring::Sandboxed — RingEnforcer always denies with no permissions configured.
}

/// Derive the set of top-level tool-family names (`tools/list` granularity)
/// visible to `ring`, from the same action-pattern lists `configure_default_rings`
/// feeds into `RingEnforcer`. Every pattern is `"{family}.{action_or_*}"`;
/// this takes just the family part, deduplicated. Single source of truth —
/// there is no separately-maintained list to drift out of sync.
///
/// Does **not** include `CORE_TOOL_FAMILIES` — callers that want "core plus
/// ring-gated" visibility (the intended `tools/list` behavior) must union
/// this with `CORE_TOOL_FAMILIES` themselves.
///
/// `Ring::Sandboxed` has no configured patterns and returns an empty set.
pub fn ring_visible_tool_families(ring: Ring) -> BTreeSet<String> {
    let patterns = match ring {
        Ring::Admin => admin_action_patterns(),
        Ring::Standard => standard_action_patterns(),
        Ring::Restricted => restricted_action_patterns(),
        Ring::Sandboxed => Vec::new(),
    };
    patterns
        .iter()
        .filter_map(|p| p.split('.').next().map(String::from))
        .collect()
}

/// Parse a `LEDGERR_MCP_RING` environment value into a `Ring`.
///
/// Case-insensitive; leading/trailing whitespace is trimmed. Returns `None`
/// for anything unrecognized (including empty string) — callers should treat
/// `None` as "no ring filtering configured", not as an error, so an unset or
/// misspelled env var degrades to today's unfiltered behavior rather than
/// failing closed or open unexpectedly.
pub fn ring_from_env_str(s: &str) -> Option<Ring> {
    match s.trim().to_ascii_lowercase().as_str() {
        "admin" => Some(Ring::Admin),
        "standard" => Some(Ring::Standard),
        "restricted" => Some(Ring::Restricted),
        "sandboxed" => Some(Ring::Sandboxed),
        _ => None,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_visible_families_match_action_patterns() {
        let families = ring_visible_tool_families(Ring::Standard);
        let expected: BTreeSet<String> = [
            "ledgerr_documents",
            "ledgerr_review",
            "ledgerr_workflow",
            "ledgerr_audit",
            "ledgerr_tax",
            "ledgerr_ontology",
            "ledgerr_evidence",
            "ledgerr_focus",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        assert_eq!(families, expected);
    }

    #[test]
    fn restricted_visible_families_are_a_subset_of_standard() {
        let restricted = ring_visible_tool_families(Ring::Restricted);
        let standard = ring_visible_tool_families(Ring::Standard);
        assert!(
            restricted.is_subset(&standard),
            "Restricted must not see families Standard doesn't: {restricted:?} vs {standard:?}"
        );
        assert_eq!(
            restricted,
            ["ledgerr_documents", "ledgerr_audit", "ledgerr_tax", "ledgerr_evidence", "ledgerr_focus"]
                .into_iter()
                .map(String::from)
                .collect::<BTreeSet<_>>()
        );
    }

    #[test]
    fn admin_visible_families_cover_all_published_tools() {
        let families = ring_visible_tool_families(Ring::Admin);
        assert_eq!(families.len(), 10, "expected all 10 PUBLISHED_TOOL_NAMES families: {families:?}");
        assert!(families.contains("ledgerr_reconciliation"));
        assert!(families.contains("ledgerr_xero"));
    }

    #[test]
    fn sandboxed_sees_no_ring_gated_families() {
        assert!(ring_visible_tool_families(Ring::Sandboxed).is_empty());
    }

    #[test]
    fn core_tool_families_are_disjoint_from_published_tool_names() {
        // Core-group tools (schema, manifest) live outside the AGT policy
        // contract entirely — that's *why* they're safe to always expose.
        let admin = ring_visible_tool_families(Ring::Admin);
        for core in CORE_TOOL_FAMILIES {
            assert!(
                !admin.contains(*core),
                "{core} should not appear in the ring-gated action patterns"
            );
        }
    }

    #[test]
    fn ring_from_env_str_parses_known_values_case_insensitively() {
        assert_eq!(ring_from_env_str("admin"), Some(Ring::Admin));
        assert_eq!(ring_from_env_str("Standard"), Some(Ring::Standard));
        assert_eq!(ring_from_env_str("RESTRICTED"), Some(Ring::Restricted));
        assert_eq!(ring_from_env_str("  sandboxed  "), Some(Ring::Sandboxed));
    }

    #[test]
    fn ring_from_env_str_rejects_unknown_values() {
        assert_eq!(ring_from_env_str(""), None);
        assert_eq!(ring_from_env_str("superuser"), None);
        assert_eq!(ring_from_env_str("ring0"), None);
    }
}
