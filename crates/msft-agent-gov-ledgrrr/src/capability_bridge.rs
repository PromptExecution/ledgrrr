//! Bridges `b00t-iface::CapabilityOffer` into AGT ring assignment.
//!
//! ## Ring mapping
//!
//! | Offer content                               | Derived ring      | Notes                                      |
//! |---------------------------------------------|-------------------|--------------------------------------------|
//! | `name` contains `ledgerr_reconciliation.commit` | `Ring::Admin`  | Returned as `Ring::Standard`; caller MUST invoke `gw.promote_to_admin()` after operator approval. |
//! | `name` starts with `ledgerr_` (non-commit)  | `Ring::Standard`  | Write-capable ledgerr operation.           |
//! | Everything else                             | `Ring::Restricted`| Read-only or unknown capability.           |
//!
//! ## Restricted ring caveat
//!
//! `LedgrrAgtGateway::register_agent` always initialises the agent at
//! `Ring::Standard`. There is currently no public gateway method that assigns
//! `Ring::Restricted` without also triggering the quarantine lifecycle
//! transition (semantically incorrect for a clean read-only offer).
//!
//! Until `LedgrrAgtGateway::register_agent_at_ring` is added upstream, this
//! module registers read-only offers at `Ring::Standard` and returns
//! `Ring::Restricted` as the *intended* ring so callers can record it.
//! See the `accept_capability_offer` doc for details.

use b00t_iface::handshake::CapabilityOffer;

use agentmesh::Ring;

use crate::{AgtError, LedgrrAgtGateway};

/// Derive the appropriate `Ring` for the given `CapabilityOffer`.
///
/// Inspects `offer.name` according to the ledgerr cap naming convention:
/// - `ledgerr_reconciliation.commit` → would require `Ring::Admin` (operator must confirm)
/// - `ledgerr_*` (any other ledgerr cap) → `Ring::Standard`
/// - anything else → `Ring::Restricted`
///
/// This function never returns `Ring::Sandboxed`; that state is reserved for
/// unregistered agents.
pub fn ring_for_offer(offer: &CapabilityOffer) -> Ring {
    let name = offer.name.as_str();
    if name.contains("ledgerr_reconciliation.commit") {
        // Admin requires operator confirmation — the derived ring is Admin
        // but `accept_capability_offer` must NOT auto-assign it.
        Ring::Admin
    } else if name.starts_with("ledgerr_") {
        Ring::Standard
    } else {
        Ring::Restricted
    }
}

/// Accept a `CapabilityOffer`: register the offering agent at the derived ring,
/// activate its lifecycle, and return the assigned ring.
///
/// The agent identifier used for registration is `offer.name`.
///
/// ## Admin / commit caps
///
/// When `ring_for_offer` returns `Ring::Admin`, this function:
/// 1. Registers the agent at `Ring::Standard` (safe default).
/// 2. Logs that operator promotion is required.
/// 3. Returns `Ok(Ring::Standard)`.
///
/// The caller is responsible for invoking `gw.promote_to_admin(offer_agent_id)`
/// after receiving out-of-band operator approval (e.g. a Tauri toast confirmation).
///
/// ## Restricted caps
///
/// When the derived ring is `Ring::Restricted`, this function registers the
/// agent at `Ring::Standard` (the only non-quarantine public path available on
/// the current gateway API) and returns `Ok(Ring::Restricted)` to signal the
/// intended ring to the caller. A future `register_agent_at_ring` extension
/// will resolve this asymmetry.
///
/// ## Errors
///
/// Returns `Err(AgtError::Lifecycle)` if `offer.name` is empty (no usable
/// agent identifier). All other registration failures are logged and swallowed
/// internally by `LedgrrAgtGateway::register_agent`.
pub fn accept_capability_offer(
    gw: &LedgrrAgtGateway,
    offer: &CapabilityOffer,
) -> Result<Ring, AgtError> {
    let agent_id = offer.name.as_str();
    if agent_id.is_empty() {
        return Err(AgtError::Bridge(
            "CapabilityOffer.name is empty — cannot derive agent_id for registration".to_owned(),
        ));
    }

    let derived = ring_for_offer(offer);

    match derived {
        Ring::Admin => {
            tracing::info!(
                agent_id,
                "CapabilityBridge: commit cap detected — registering at Standard, \
                 operator promotion required before Admin access is granted"
            );
            gw.register_agent(agent_id);
            Ok(Ring::Standard)
        }
        Ring::Standard => {
            tracing::info!(agent_id, "CapabilityBridge: write cap — registering at Standard");
            gw.register_agent(agent_id);
            Ok(Ring::Standard)
        }
        Ring::Restricted => {
            tracing::info!(
                agent_id,
                "CapabilityBridge: read-only cap — registering at Standard \
                 (intended ring: Restricted; full Restricted assignment pending \
                 register_agent_at_ring upstream extension)"
            );
            gw.register_agent(agent_id);
            Ok(Ring::Restricted)
        }
        Ring::Sandboxed => {
            // ring_for_offer never returns Sandboxed; this arm is exhaustiveness-only.
            tracing::warn!(agent_id, "CapabilityBridge: unexpected Sandboxed ring derived — not registering");
            Ok(Ring::Sandboxed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use b00t_iface::handshake::{CapabilityKind, CapabilityOffer};
    use std::collections::HashMap;

    fn make_offer(name: &str) -> CapabilityOffer {
        CapabilityOffer {
            kind: CapabilityKind::Tool,
            name: name.to_owned(),
            endpoint: None,
            api_key: None,
            params: HashMap::new(),
        }
    }

    fn make_gateway() -> LedgrrAgtGateway {
        LedgrrAgtGateway::new("hermes").expect("gateway init must succeed in tests")
    }

    // -----------------------------------------------------------------------
    // ring_for_offer mapping tests
    // -----------------------------------------------------------------------

    /// Commit cap → ring_for_offer returns Admin (operator must confirm before
    /// Auto-assigning). accept_capability_offer returns Standard, not Admin.
    #[test]
    fn offer_with_commit_cap_maps_to_standard_pending_operator() {
        let offer = make_offer("ledgerr_reconciliation.commit");
        let ring = ring_for_offer(&offer);
        // ring_for_offer signals Admin *intent* ...
        assert_eq!(ring, Ring::Admin, "commit cap should derive Ring::Admin intent");

        // ... but accept_capability_offer must NOT auto-assign Admin.
        let gw = make_gateway();
        let assigned = accept_capability_offer(&gw, &offer)
            .expect("accept must not error on a valid offer");
        assert_eq!(
            assigned,
            Ring::Standard,
            "commit cap: accept must return Standard — operator confirmation required"
        );
        // Confirm the agent is no longer Sandboxed (it was registered).
        let decision = gw.check_tool_call(&offer.name, "ledgerr_documents", "list_accounts");
        assert!(
            decision.allowed,
            "registered agent should pass read-only check at Standard"
        );
    }

    /// Non-commit ledgerr write cap → Standard.
    #[test]
    fn offer_with_write_caps_maps_to_standard() {
        let offer = make_offer("ledgerr_transactions.classify");
        assert_eq!(ring_for_offer(&offer), Ring::Standard);
    }

    /// Read-only (non-ledgerr) cap → Restricted.
    #[test]
    fn offer_with_read_only_caps_maps_to_restricted() {
        let offer = make_offer("openai.completions.read");
        assert_eq!(ring_for_offer(&offer), Ring::Restricted);
    }

    // -----------------------------------------------------------------------
    // accept_capability_offer integration tests
    // -----------------------------------------------------------------------

    /// After accept_capability_offer, the agent passes check_tool_call (no longer Sandboxed).
    #[test]
    fn accept_offer_registers_agent() {
        let gw = make_gateway();
        let offer = make_offer("ledgerr_documents.ingest");
        let ring = accept_capability_offer(&gw, &offer).expect("accept must succeed");
        assert_eq!(ring, Ring::Standard);

        // Agent is now registered — a read-only tool call should be allowed.
        let decision = gw.check_tool_call(&offer.name, "ledgerr_documents", "list_accounts");
        assert!(
            decision.allowed,
            "registered agent must not be Sandboxed after accept_capability_offer"
        );
    }

    /// Empty name → Err rather than silent panic or garbage registration.
    #[test]
    fn accept_offer_empty_name_returns_error() {
        let gw = make_gateway();
        let offer = make_offer("");
        let result = accept_capability_offer(&gw, &offer);
        assert!(result.is_err(), "empty agent_id must return Err");
    }
}
