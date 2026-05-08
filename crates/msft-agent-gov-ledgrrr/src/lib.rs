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
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;

pub use agentmesh::{ClientError, GovernanceResult, TrustScore, TrustTier};

#[derive(Debug, Error)]
pub enum AgtError {
    #[error("client error: {0}")]
    Client(#[from] ClientError),
    #[error("policy file read error: {0}")]
    PolicyRead(#[from] std::io::Error),
    #[error("persistence error: {0}")]
    Persist(String),
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

/// Private snapshot type for ring assignment persistence.
#[derive(serde::Serialize, serde::Deserialize)]
struct RingSnapshot {
    /// agent_id → Ring variant
    assignments: HashMap<String, Ring>,
}

/// Unified AGT governance surface for ledgrrr.
///
/// Combines:
/// - `AgentMeshClient` — YAML policy engine + trust scoring + hash-chain audit log
/// - `RingEnforcer` — 4-tier execution privilege rings mapped to CommitGate tiers
pub struct LedgrrAgtGateway {
    client: AgentMeshClient,
    rings: Arc<RwLock<RingEnforcer>>,
    /// Shadow map for ring persistence: mirrors `RingEnforcer.assignments`.
    /// Present only when `with_persist_path` was used.
    ring_shadow: Arc<RwLock<HashMap<String, Ring>>>,
    /// Path to write ring snapshot JSON on every mutation.
    rings_persist_path: Option<std::path::PathBuf>,
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
        Self::build_gateway(agent_id, policy::LEDGERR_POLICY_YAML, trust)
    }

    /// Create a gateway that loads its policy from `policy_path` at runtime.
    ///
    /// Falls back to [`policy::LEDGERR_POLICY_YAML`] if the path does not exist.
    /// Returns an error if the path exists but cannot be read or contains invalid UTF-8.
    pub fn with_policy_path(
        agent_id: &str,
        policy_path: &std::path::Path,
    ) -> Result<Self, AgtError> {
        let yaml = if policy_path.exists() {
            std::fs::read_to_string(policy_path)?
        } else {
            policy::LEDGERR_POLICY_YAML.to_string()
        };
        Self::build_gateway(agent_id, &yaml, TrustConfig::default())
    }

    /// Shared construction core used by `new`, `with_trust_config`, and `with_policy_path`.
    fn build_gateway(
        agent_id: &str,
        policy_yaml: &str,
        trust: TrustConfig,
    ) -> Result<Self, AgtError> {
        let opts = ClientOptions {
            capabilities: policy::LEDGERR_CAPABILITIES
                .iter()
                .map(|s| s.to_string())
                .collect(),
            policy_yaml: Some(policy_yaml.to_string()),
            trust_config: Some(trust),
        };
        let client = AgentMeshClient::with_options(agent_id, opts)?;
        let mut enforcer = RingEnforcer::new();
        rings::configure_default_rings(&mut enforcer);
        enforcer.assign(agent_id, Ring::Standard);
        let mut shadow = HashMap::new();
        shadow.insert(agent_id.to_string(), Ring::Standard);
        Ok(Self {
            client,
            rings: Arc::new(RwLock::new(enforcer)),
            ring_shadow: Arc::new(RwLock::new(shadow)),
            rings_persist_path: None,
        })
    }

    /// Create a gateway that persists ring assignments to
    /// `sidecar_dir/{agent_id}.agt-rings.json` and trust scores to
    /// `sidecar_dir/{agent_id}.agt-trust.json`. Loads existing state on
    /// construction if the files exist.
    pub fn with_persist_path(
        agent_id: &str,
        sidecar_dir: &std::path::Path,
    ) -> Result<Self, AgtError> {
        let rings_path = sidecar_dir.join(format!("{}.agt-rings.json", agent_id));
        let trust_path = sidecar_dir.join(format!("{}.agt-trust.json", agent_id));

        let trust = TrustConfig {
            persist_path: Some(
                trust_path
                    .to_str()
                    .ok_or_else(|| AgtError::Persist("trust path contains invalid UTF-8".into()))?
                    .to_string(),
            ),
            ..TrustConfig::default()
        };

        let mut gw = Self::build_gateway(agent_id, policy::LEDGERR_POLICY_YAML, trust)?;
        gw.rings_persist_path = Some(rings_path.clone());

        // Load existing ring snapshot if present.
        if rings_path.exists() {
            let raw = std::fs::read_to_string(&rings_path)
                .map_err(|e| AgtError::Persist(format!("read rings snapshot: {e}")))?;
            let snapshot: RingSnapshot = serde_json::from_str(&raw)
                .map_err(|e| AgtError::Persist(format!("deserialize rings snapshot: {e}")))?;
            let mut enforcer = gw
                .rings
                .write()
                .expect("rings RwLock poisoned during construction");
            let mut shadow = gw
                .ring_shadow
                .write()
                .expect("ring_shadow RwLock poisoned during construction");
            for (id, ring) in &snapshot.assignments {
                enforcer.assign(id, *ring);
                shadow.insert(id.clone(), *ring);
            }
        }

        Ok(gw)
    }

    /// Serialize current ring shadow map to disk. No-op when no persist path is set.
    fn save_rings(&self) -> Result<(), AgtError> {
        let Some(ref path) = self.rings_persist_path else {
            return Ok(());
        };
        let shadow = self
            .ring_shadow
            .read()
            .expect("ring_shadow RwLock poisoned during save");
        let snapshot = RingSnapshot {
            assignments: shadow.clone(),
        };
        let json = serde_json::to_string(&snapshot)
            .map_err(|e| AgtError::Persist(format!("serialize rings snapshot: {e}")))?;
        std::fs::write(path, json)
            .map_err(|e| AgtError::Persist(format!("write rings snapshot: {e}")))?;
        Ok(())
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
    ///
    /// Returns `Err` if the ring snapshot cannot be persisted. Callers must
    /// surface this failure — a silent persist failure means the operator sees
    /// "promoted" in the UI but the promotion is lost on restart.
    pub fn promote_to_admin(&self, agent_id: &str) -> Result<(), AgtError> {
        self.rings.write().unwrap().assign(agent_id, Ring::Admin);
        self.ring_shadow
            .write()
            .unwrap()
            .insert(agent_id.to_string(), Ring::Admin);
        self.save_rings()
    }

    /// Register a new external agent at `Ring::Standard`.
    pub fn register_agent(&self, agent_id: &str) {
        self.rings
            .write()
            .unwrap()
            .assign(agent_id, Ring::Standard);
        self.ring_shadow
            .write()
            .unwrap()
            .insert(agent_id.to_string(), Ring::Standard);
        if let Err(e) = self.save_rings() {
            tracing::warn!(agent_id, error = %e, "ring persistence write failed after register_agent");
        }
    }

    /// Current trust score for any agent DID.
    ///
    /// # Deprecation
    ///
    /// Prefer [`trust_score_for_agent`](Self::trust_score_for_agent), which accepts a bare
    /// `agent_id` string and constructs the `did:agentmesh:` prefix internally.  Passing a raw
    /// `agent_id` here silently returns the default initial score instead of an error.
    #[deprecated(
        since = "1.8.1",
        note = "use trust_score_for_agent(agent_id) instead"
    )]
    pub fn trust_score(&self, did: &str) -> TrustScore {
        self.client.trust.get_trust_score(did)
    }

    /// Current trust score for a registered agent, identified by bare `agent_id`.
    ///
    /// Constructs `did:agentmesh:{agent_id}` internally so callers never need to
    /// format the DID prefix manually.  Returns the configured initial score (default
    /// 500) when the agent has no recorded trust events — it never panics.
    pub fn trust_score_for_agent(&self, agent_id: &str) -> TrustScore {
        let did = format!("did:agentmesh:{}", agent_id);
        self.client.trust.get_trust_score(&did)
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
        gw.promote_to_admin("hermes").expect("promote_to_admin must persist");
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

    /// Gap 12: trust_score_for_agent must agree with trust_score(full_did) after
    /// a trust event has been recorded for that agent's DID.
    #[test]
    fn trust_score_for_agent_matches_did() {
        // The gateway's own DID is `did:agentmesh:my-agent`.
        // check_tool_call on Standard ring calls execute_with_governance which
        // records a trust event under the gateway's identity DID.
        let gw = LedgrrAgtGateway::new("my-agent").unwrap();
        gw.check_tool_call("my-agent", "ledgerr_documents", "list_accounts");

        #[allow(deprecated)]
        let via_did = gw.trust_score("did:agentmesh:my-agent");
        let via_agent = gw.trust_score_for_agent("my-agent");

        assert_eq!(via_agent.score, via_did.score);
        assert_eq!(via_agent.tier, via_did.tier);
    }

    /// Gap 12: trust_score_for_agent must not panic for an unknown agent id and
    /// must return the configured initial score (500 by default — NOT zero).
    #[test]
    fn trust_score_bare_id_does_not_panic() {
        let gw = LedgrrAgtGateway::new("owner").unwrap();
        let result = gw.trust_score_for_agent("nobody");
        // TrustManager returns initial_score (default 500) for unknown DIDs.
        // The score is never zero unless initial_score is explicitly set to 0.
        assert_eq!(result.score, 500);
    }

    // --- Gap 8 tests ---

    #[test]
    fn policy_path_fallback_on_missing_file() {
        let gw = LedgrrAgtGateway::with_policy_path(
            "agent",
            std::path::Path::new("/nonexistent/gap8-policy.yaml"),
        )
        .expect("should succeed with fallback to default policy");
        let result = gw.check_tool_call("agent", "ledgerr_documents", "list_accounts");
        assert!(result.allowed, "default policy must allow list_accounts");
    }

    #[test]
    fn policy_path_loads_custom_yaml() {
        use std::io::Write as _;
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        // Write a minimal valid policy — same content as the built-in default so
        // construction succeeds; YAML semantics are tested in agentmesh unit tests.
        tmp.write_all(policy::LEDGERR_POLICY_YAML.as_bytes())
            .unwrap();
        let path = tmp.path().to_owned();
        LedgrrAgtGateway::with_policy_path("agent", &path)
            .expect("gateway must construct from a readable policy file");
    }

    #[test]
    fn policy_path_returns_error_on_unreadable() {
        // A directory path passed to read_to_string yields an io::Error on Linux.
        let dir = tempfile::tempdir().unwrap();
        let result = LedgrrAgtGateway::with_policy_path("agent", dir.path());
        assert!(
            matches!(result, Err(AgtError::PolicyRead(_))),
            "expected PolicyRead error"
        );
    }

    // --- Gap 11 tests ---

    /// Ring promotion persists to disk and is reloaded on reconstruction.
    #[test]
    fn rings_persist_and_reload() {
        let dir = tempfile::tempdir().unwrap();
        {
            let gw = LedgrrAgtGateway::with_persist_path("alpha", dir.path())
                .expect("gateway construction must succeed");
            gw.promote_to_admin("alpha").expect("promote_to_admin must persist");
            // gw drops here — in-memory state gone
        }
        let gw2 = LedgrrAgtGateway::with_persist_path("alpha", dir.path())
            .expect("reload must succeed");
        let ring = gw2
            .rings
            .read()
            .unwrap()
            .get_ring("alpha")
            .expect("alpha must be present after reload");
        assert_eq!(ring, Ring::Admin, "promoted ring must survive restart");
    }

    /// Fresh construction into an empty sidecar dir succeeds and starts Standard.
    #[test]
    fn rings_start_fresh_if_no_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let gw = LedgrrAgtGateway::with_persist_path("beta", dir.path())
            .expect("gateway must build without pre-existing sidecar files");
        let ring = gw
            .rings
            .read()
            .unwrap()
            .get_ring("beta")
            .expect("beta must be registered");
        assert_eq!(ring, Ring::Standard, "default ring must be Standard");
    }

    /// Promoted ring survives restart and check_tool_call returns allowed on reload.
    #[test]
    fn promoted_ring_survives_restart() {
        let dir = tempfile::tempdir().unwrap();
        {
            let gw = LedgrrAgtGateway::with_persist_path("gamma", dir.path())
                .expect("gateway construction must succeed");
            gw.promote_to_admin("gamma").expect("promote_to_admin must persist");
        }
        let gw2 = LedgrrAgtGateway::with_persist_path("gamma", dir.path())
            .expect("reload must succeed");
        // Admin ring bypasses policy gate — commit must be allowed.
        let r = gw2.check_tool_call("gamma", "ledgerr_reconciliation", "commit_entry");
        assert_eq!(r.ring, Ring::Admin, "ring must be Admin after reload");
        assert!(r.allowed, "Admin ring must allow commit_entry");
    }
}
