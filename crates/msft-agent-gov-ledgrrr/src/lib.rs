//! Microsoft Agent Governance Toolkit integration for ledgrrr.
//!
//! Wraps [`agentmesh::AgentMeshClient`] and [`agentmesh::RingEnforcer`] behind a single
//! [`LedgrrAgtGateway`] that maps ledgrrr's 10-tool MCP contract onto the AGT
//! policy / trust / audit pipeline.
//!
//! # Usage
//!
//! ```rust,no_run
//! use msft_agent_gov_ledgrrr::LedgrrAgtGateway;
//!
//! let gw = LedgrrAgtGateway::new("hermes").unwrap();
//! let r = gw.check_tool_call("hermes", "ledgerr_documents", "list_accounts");
//! assert!(r.allowed);
//! ```

pub mod policy;
pub mod rings;

use agentmesh::{AgentMeshClient, ClientOptions, PolicyDecision, Ring, RingEnforcer, TrustConfig};
use std::sync::{Arc, RwLock};
use thiserror::Error;

pub use agentmesh::{ClientError, GovernanceResult, TrustScore, TrustTier};

#[derive(Debug, Error)]
pub enum AgtError {
    #[error("client error: {0}")]
    Client(#[from] ClientError),
}

/// Result of a governed tool call check.
#[derive(Debug)]
pub struct ToolCallDecision {
    /// `true` if the call is permitted to proceed immediately.
    pub allowed: bool,
    /// AGT policy decision — Allow / Deny / RequiresApproval / RateLimited.
    pub policy: PolicyDecision,
    /// Current trust score for the calling agent.
    pub trust: TrustScore,
    /// Assigned execution ring.
    pub ring: Ring,
    /// Human-readable reason when blocked.
    pub reason: Option<String>,
}

/// Unified AGT governance surface for ledgrrr.
///
/// Combines:
/// - `AgentMeshClient` — YAML policy engine + trust scoring + hash-chain audit log
/// - `RingEnforcer` — 4-tier execution privilege rings mapped to CommitGate tiers
pub struct LedgrrAgtGateway {
    client: AgentMeshClient,
    rings: Arc<RwLock<RingEnforcer>>,
}

impl LedgrrAgtGateway {
    /// Create a gateway for `agent_id` with ledgrrr's default policy.
    ///
    /// Agent starts at `Ring::Standard` (trust 500-899): ingest + classify + read.
    /// Commit/reverse escalate to `RequiresApproval` until promoted to Admin.
    pub fn new(agent_id: &str) -> Result<Self, AgtError> {
        Self::with_trust_config(agent_id, TrustConfig::default())
    }

    /// Create a gateway with a custom initial trust config.
    pub fn with_trust_config(agent_id: &str, trust: TrustConfig) -> Result<Self, AgtError> {
        let opts = ClientOptions {
            capabilities: policy::LEDGERR_CAPABILITIES
                .iter()
                .map(|s| s.to_string())
                .collect(),
            policy_yaml: Some(policy::LEDGERR_POLICY_YAML.to_string()),
            trust_config: Some(trust),
        };
        let client = AgentMeshClient::with_options(agent_id, opts)?;
        let mut enforcer = RingEnforcer::new();
        rings::configure_default_rings(&mut enforcer);
        enforcer.assign(agent_id, Ring::Standard);
        Ok(Self {
            client,
            rings: Arc::new(RwLock::new(enforcer)),
        })
    }

    /// Check whether `agent_id` may call `tool` with `action`.
    ///
    /// Pipeline:
    /// 1. Ring check — `Sandboxed` agents denied immediately.
    /// 2. Policy engine — capability gate, approval rules, rate-limit rules.
    /// 3. Trust update — reward on Allow, penalty on Deny.
    /// 4. Ring sync — trust tier changes update the ring assignment.
    pub fn check_tool_call(&self, agent_id: &str, tool: &str, action: &str) -> ToolCallDecision {
        let ring = self
            .rings
            .read()
            .unwrap()
            .get_ring(agent_id)
            .unwrap_or(Ring::Sandboxed);

        if ring == Ring::Sandboxed {
            return ToolCallDecision {
                allowed: false,
                policy: PolicyDecision::Deny("agent not registered or sandboxed".into()),
                trust: self.client.trust.get_trust_score(&self.client.identity.did),
                ring,
                reason: Some("ring:Sandboxed — call register_agent first".into()),
            };
        }

        // Ring::Admin = operator already approved via Tauri toast; bypass policy gate.
        if ring == Ring::Admin {
            self.client.trust.record_success(&self.client.identity.did);
            return ToolCallDecision {
                allowed: true,
                policy: PolicyDecision::Allow,
                trust: self.client.trust.get_trust_score(&self.client.identity.did),
                ring,
                reason: None,
            };
        }

        // Dot-notation action: "ledgerr_documents.ingest_pdf"
        let dot_action = format!("{}.{}", tool, action);
        let result = self.client.execute_with_governance(&dot_action, None);

        let reason = match &result.decision {
            PolicyDecision::Deny(r) => Some(r.clone()),
            PolicyDecision::RequiresApproval(r) => Some(format!("approval_required: {r}")),
            PolicyDecision::RateLimited { retry_after_secs } => {
                Some(format!("rate_limited — retry after {retry_after_secs}s"))
            }
            PolicyDecision::Allow => None,
        };

        ToolCallDecision {
            allowed: result.allowed,
            policy: result.decision,
            trust: result.trust_score,
            ring,
            reason,
        }
    }

    /// Promote `agent_id` to `Ring::Admin` after a Tauri toast operator approval.
    pub fn promote_to_admin(&self, agent_id: &str) {
        self.rings.write().unwrap().assign(agent_id, Ring::Admin);
    }

    /// Register a new external agent at `Ring::Standard`.
    pub fn register_agent(&self, agent_id: &str) {
        self.rings.write().unwrap().assign(agent_id, Ring::Standard);
    }

    /// Current trust score for any agent DID.
    pub fn trust_score(&self, did: &str) -> TrustScore {
        self.client.trust.get_trust_score(did)
    }

    /// Verify the entire AGT audit hash-chain since gateway creation.
    pub fn verify_audit_chain(&self) -> bool {
        self.client.audit.verify()
    }

    /// Number of governance decisions recorded in the hash-chain audit log.
    pub fn audit_len(&self) -> usize {
        self.client.audit.entries().len()
    }

    /// DID of the governed agent (e.g. `did:agentmesh:hermes`).
    pub fn agent_did(&self) -> &str {
        &self.client.identity.did
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_allows_read_ops() {
        // Correct crate/import name: msft_agent_gov_ledgrrr (not agent_governance)
        // Mirrors the user's quick-start snippet with the right API.
        let gw = LedgrrAgtGateway::new("my-agent").unwrap();
        let result = gw.check_tool_call("my-agent", "ledgerr_documents", "list_accounts");
        assert!(result.allowed);
    }

    #[test]
    fn commit_requires_approval() {
        let gw = LedgrrAgtGateway::new("hermes").unwrap();
        let r = gw.check_tool_call("hermes", "ledgerr_reconciliation", "commit_entry");
        assert!(!r.allowed);
        assert!(matches!(r.policy, PolicyDecision::RequiresApproval(_)));
    }

    #[test]
    fn unregistered_agent_is_sandboxed() {
        let gw = LedgrrAgtGateway::new("owner").unwrap();
        let r = gw.check_tool_call("unknown-agent", "ledgerr_documents", "list_accounts");
        assert!(!r.allowed);
        assert!(matches!(r.policy, PolicyDecision::Deny(_)));
    }

    #[test]
    fn register_then_allow() {
        let gw = LedgrrAgtGateway::new("owner").unwrap();
        gw.register_agent("new-agent");
        let r = gw.check_tool_call("new-agent", "ledgerr_documents", "list_accounts");
        assert!(r.allowed);
    }

    #[test]
    fn promote_to_admin_sets_ring() {
        let gw = LedgrrAgtGateway::new("hermes").unwrap();
        gw.promote_to_admin("hermes");
        let r = gw.check_tool_call("hermes", "ledgerr_reconciliation", "commit_entry");
        assert_eq!(r.ring, Ring::Admin);
    }

    #[test]
    fn audit_chain_grows_and_verifies() {
        let gw = LedgrrAgtGateway::new("audit-agent").unwrap();
        gw.check_tool_call("audit-agent", "ledgerr_documents", "list_accounts");
        gw.check_tool_call("audit-agent", "ledgerr_evidence", "summary");
        gw.check_tool_call("audit-agent", "ledgerr_focus", "cost_report");
        assert_eq!(gw.audit_len(), 3);
        assert!(gw.verify_audit_chain());
    }

    #[test]
    fn did_format() {
        let gw = LedgrrAgtGateway::new("my-agent").unwrap();
        assert_eq!(gw.agent_did(), "did:agentmesh:my-agent");
    }
}
