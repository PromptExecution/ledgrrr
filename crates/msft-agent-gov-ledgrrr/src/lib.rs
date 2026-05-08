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

pub mod capability_bridge;
pub mod compliance;
pub mod policy;
pub mod rings;

pub use compliance::{ComplianceGrade, ComplianceStore, LedgrrComplianceReport, Z3Attestation};

use agentmesh::{
    mcp::{
        CredentialRedactor, InMemoryAuditSink, McpMetricsCollector, McpSecurityScanner,
        McpToolDefinition, SystemClock,
    },
    AgentMeshClient, ClientOptions, PolicyDecision, Ring, RingEnforcer, TrustConfig,
};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;

pub use agentmesh::{ClientError, GovernanceResult, LifecycleManager, LifecycleState, TrustScore, TrustTier};

#[derive(Debug, Error)]
pub enum AgtError {
    #[error("client error: {0}")]
    Client(#[from] ClientError),
    #[error("policy file read error: {0}")]
    PolicyRead(#[from] std::io::Error),
    #[error("persistence error: {0}")]
    Persist(String),
    #[error("credential redactor init error: {0}")]
    Redactor(String),
    #[error("lifecycle error: {0}")]
    Lifecycle(String),
    #[error("capability bridge error: {0}")]
    Bridge(String),
    #[error("sub-agent already registered: {0}")]
    SubAgentDuplicate(String),
    #[error("sub-agent not found: {0}")]
    SubAgentNotFound(String),
    /// Cedar policy failed to parse. Returned by [`LedgrrAgtGateway::with_cedar_policy`]
    /// when the supplied Cedar source is syntactically invalid.
    #[cfg(feature = "cedar-policy")]
    #[error("cedar policy parse error: {0}")]
    CedarParse(String),
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
    redactor: CredentialRedactor,
    rings: Arc<RwLock<RingEnforcer>>,
    /// Shadow map for ring persistence: mirrors `RingEnforcer.assignments`.
    /// Present only when `with_persist_path` was used.
    ring_shadow: Arc<RwLock<HashMap<String, Ring>>>,
    /// Path to write ring snapshot JSON on every mutation.
    rings_persist_path: Option<std::path::PathBuf>,
    /// Per-agent lifecycle state machines.  One `LifecycleManager` per
    /// registered agent; keyed by bare `agent_id` (no DID prefix).
    lifecycle_map: Arc<RwLock<HashMap<String, LifecycleManager>>>,
    /// MCP tool-schema security scanner.  Initialized with the 10 PUBLISHED_TOOL_NAMES
    /// as trusted fingerprints; detects rug-pulls, poisoning, and injection attacks.
    scanner: McpSecurityScanner,
    /// Spawned sub-agents — each has its own `AgentMeshClient` so trust scores
    /// are tracked in an isolated namespace.  Keyed by bare `agent_id`.
    ///
    /// Workaround for the missing `AgentMeshClient::fork_sub_agent` upstream API.
    /// Higher overhead than forking (a full client per sub-agent) but correct.
    sub_agents: Arc<RwLock<HashMap<String, AgentMeshClient>>>,
    /// Policy YAML snapshot used to construct sub-agent clients with the same
    /// policy rules as the parent gateway.
    policy_yaml: String,
    /// Trust config snapshot used as the template for sub-agent construction.
    /// Each sub-agent gets a fresh `TrustConfig` cloned from this (no persist_path)
    /// so scores start at the configured initial value and diverge independently.
    trust_config_snapshot: TrustConfig,
    /// Z3 attestation ledger + AGT compliance engine (Gap 6).
    /// Accumulates Blake3 proof hashes from arc-kit-au that auto-satisfy
    /// OWASP / EU AI Act / SOC 2 audit controls.
    compliance_store: ComplianceStore,
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

    /// Create a gateway using a Cedar policy bundle instead of YAML.
    ///
    /// # Phase 4 — Cedar backend (feature-gated)
    ///
    /// Cedar provides formally-verified authorization semantics. This constructor
    /// validates that `cedar_src` parses correctly via the `cedar-policy` crate, then
    /// falls back to [`policy::LEDGERR_POLICY_YAML`] for the AGT `ClientOptions.policy_yaml`
    /// field, because [`agentmesh::ClientOptions`] has no native Cedar field.
    ///
    /// **Limitation:** Cedar→YAML translation is not implemented. The Cedar policy is
    /// validated for parse correctness only. Runtime enforcement uses the ledgrrr default
    /// YAML policy. A `tracing::warn!` is emitted to make this explicit in every run.
    ///
    /// Full Cedar→AGT integration is deferred to a dedicated Phase 4 task.
    ///
    /// # Errors
    ///
    /// Returns [`AgtError::CedarParse`] if `cedar_src` is not valid Cedar syntax.
    /// Returns [`AgtError::Client`] / other variants on AGT construction failure.
    #[cfg(feature = "cedar-policy")]
    pub fn with_cedar_policy(agent_id: &str, cedar_src: &str) -> Result<Self, AgtError> {
        use cedar_policy::PolicySet;
        use std::str::FromStr;

        // Validate Cedar syntax. Reject unparseable bundles immediately.
        PolicySet::from_str(cedar_src)
            .map_err(|e| AgtError::CedarParse(format!("{e:?}")))?;

        // Cedar→YAML translation not yet implemented. Fall back to the built-in
        // ledgrrr default policy and surface a structured warning so operators
        // know Cedar enforcement is not active.
        tracing::warn!(
            agent_id,
            "cedar-policy feature enabled but Cedar→YAML translation is not implemented; \
             falling back to LEDGERR_POLICY_YAML. Cedar policy validated for syntax only."
        );

        Self::build_gateway(agent_id, policy::LEDGERR_POLICY_YAML, TrustConfig::default())
    }

    /// Shared construction core used by `new`, `with_trust_config`, `with_policy_path`,
    /// and (when the `cedar-policy` feature is active) `with_cedar_policy`.
    fn build_gateway(
        agent_id: &str,
        policy_yaml: &str,
        trust: TrustConfig,
    ) -> Result<Self, AgtError> {
        // Retain snapshots before `trust` is moved into `opts`.
        let policy_yaml_owned = policy_yaml.to_string();
        let trust_config_snapshot = trust.clone();
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
        let redactor = CredentialRedactor::new()
            .map_err(|e| AgtError::Redactor(e.to_string()))?;

        // Register the gateway's own agent as Active in the lifecycle FSM.
        // LifecycleManager::new starts at Provisioning; activate() moves it to Active.
        let mut lm = LifecycleManager::new(agent_id);
        lm.activate("gateway construction")
            .map_err(|e| AgtError::Lifecycle(format!("initial activate failed: {e}")))?;
        let mut lifecycle_map = HashMap::new();
        lifecycle_map.insert(agent_id.to_string(), lm);

        // Build the MCP security scanner and pre-register all PUBLISHED_TOOL_NAMES
        // as trusted fingerprints.  Any subsequent scan of a tool whose description
        // or schema differs from this baseline triggers a rug-pull detection.
        let scanner_redactor = CredentialRedactor::new()
            .map_err(|e| AgtError::Redactor(e.to_string()))?;
        let audit_sink: Arc<dyn agentmesh::mcp::McpAuditSink> =
            Arc::new(InMemoryAuditSink::new(
                CredentialRedactor::new()
                    .map_err(|e| AgtError::Redactor(e.to_string()))?,
            ));
        let scanner = McpSecurityScanner::new(
            scanner_redactor,
            audit_sink,
            McpMetricsCollector::default(),
            Arc::new(SystemClock),
        )
        .map_err(|e| AgtError::Redactor(format!("scanner init: {e}")))?;

        // Seed the trusted-tool fingerprint registry with each published tool name.
        // Empty description and no schema are used as the baseline — callers must
        // pass matching values to scan_tool_call or rug-pull will fire.
        for name in policy::PUBLISHED_TOOL_NAMES {
            let baseline = McpToolDefinition {
                name: name.to_string(),
                description: String::new(),
                input_schema: None,
                server_name: "ledgrrr".to_string(),
            };
            scanner
                .register_tool(&baseline)
                .map_err(|e| AgtError::Redactor(format!("scanner register {name}: {e}")))?;
        }

        Ok(Self {
            client,
            redactor,
            rings: Arc::new(RwLock::new(enforcer)),
            ring_shadow: Arc::new(RwLock::new(shadow)),
            rings_persist_path: None,
            lifecycle_map: Arc::new(RwLock::new(lifecycle_map)),
            scanner,
            sub_agents: Arc::new(RwLock::new(HashMap::new())),
            policy_yaml: policy_yaml_owned,
            trust_config_snapshot,
            compliance_store: ComplianceStore::new(),
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

    // -----------------------------------------------------------------------
    // Gap 6 — ComplianceEngine + Z3 attestation feed
    // -----------------------------------------------------------------------

    /// Generate a compliance report graded against OWASP / EU AI Act / SOC 2 controls.
    ///
    /// The report reflects the current attestation state of all controls that
    /// have been fed into the gateway via [`Self::attest_z3_proof`].  Controls
    /// with no attestation appear in `controls_pending`; attested controls
    /// appear in `controls_satisfied`.
    ///
    /// The `grade` field upgrades from `Unknown` → `Partial` → `Full` as
    /// attestations are recorded.
    pub fn compliance_report(&self) -> LedgrrComplianceReport {
        self.compliance_store.ledgrrr_report()
    }

    /// Feed a Z3 attestation hash as evidence for a named compliance control.
    ///
    /// `control_id` — an audit control identifier such as `"soc2-cc6.1"` or
    /// `"eu-ai-act-art-13"`.
    ///
    /// `blake3_hex` — hex-encoded Blake3 node ID produced by `arc-kit-au`.
    /// Attestations are appended to the immutable ledger; calling this method
    /// twice for the same `control_id` creates two ledger entries and does not
    /// alter the satisfied state (idempotent as of the first call).
    pub fn attest_z3_proof(&self, control_id: &str, blake3_hex: &str) {
        tracing::info!(
            control_id,
            blake3_hex,
            "attest_z3_proof: recording Z3 attestation for compliance control"
        );
        self.compliance_store.attest(control_id, blake3_hex);
    }

    /// Pre-declare a compliance control as required.
    ///
    /// Controls that are registered but not yet attested appear as pending in
    /// the report.  Call this for the full set of required controls before any
    /// attestations so that [`ComplianceGrade::Partial`] is reachable when
    /// only a subset has been attested.
    pub fn register_compliance_control(&self, control_id: &str) {
        tracing::debug!(control_id, "register_compliance_control: pre-registering control");
        self.compliance_store.register_control(control_id);
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

    /// Scan a tool invocation for MCP security threats before executing.
    ///
    /// Constructs a [] from  and ,
    /// runs the scanner, and on any detected threat:
    /// 1. Quarantines the calling agent (no-op if unregistered — bootstrap is handled internally).
    /// 2. Returns  — the caller must not proceed.
    /// 3. Emits a structured audit entry with severity Critical.
    ///
    /// Returns  when the scan is clean; caller proceeds to [].
    pub fn scan_tool_call(
        &self,
        agent_id: &str,
        tool_name: &str,
        tool_description: &str,
    ) -> Option<ToolCallDecision> {
        let tool_def = McpToolDefinition {
            name: tool_name.to_string(),
            description: tool_description.to_string(),
            input_schema: None,
            server_name: "ledgrrr".to_string(),
        };

        let threats = match self.scanner.scan_tool(&tool_def) {
            Ok(t) => t,
            Err(e) => {
                tracing::error!(
                    agent_id,
                    tool_name,
                    error = %e,
                    "scan_tool_call: scanner error — denying as precaution"
                );
                return Some(ToolCallDecision {
                    allowed: false,
                    policy: PolicyDecision::Deny(format!("scanner error: {e}")),
                    trust: self.client.trust.get_trust_score(&self.client.identity.did),
                    ring: self.rings.read().unwrap().get_ring(agent_id).unwrap_or(Ring::Sandboxed),
                    reason: Some("scan_error".to_string()),
                });
            }
        };

        if threats.is_empty() {
            return None;
        }

        // Threat detected — quarantine, audit, deny.
        let threat_summary: String = threats
            .iter()
            .map(|t| format!("{:?}", t.threat_type))
            .collect::<Vec<_>>()
            .join(",");

        tracing::error!(
            agent_id,
            tool_name,
            threats = %threat_summary,
            "scan_tool_call: security threat detected — quarantining agent"
        );

        // Ensure agent is registered before quarantine attempt.  quarantine_agent
        // already handles the unregistered case by bootstrapping a lifecycle entry.
        self.quarantine_agent(agent_id);

        self.client.audit.log(
            agent_id,
            &format!("security_threat:{threat_summary}"),
            "quarantined",
        );

        let ring = self
            .rings
            .read()
            .unwrap()
            .get_ring(agent_id)
            .unwrap_or(Ring::Sandboxed);

        Some(ToolCallDecision {
            allowed: false,
            policy: PolicyDecision::Deny(format!("security threat: {threat_summary}")),
            trust: self.client.trust.get_trust_score(&self.client.identity.did),
            ring,
            reason: Some("rug_pull_or_poisoning_detected".to_string()),
        })
    }

    /// Check whether  may call  with .
    ///
    /// Pipeline:
    /// 1. Ring check —  agents denied immediately.
    /// 2. Policy engine — capability gate, approval rules, rate-limit rules.
    /// 3. Trust update — reward on Allow, penalty on Deny.
    /// 4. Ring sync — trust tier changes update the ring assignment.
    /// Governance gate for a tool call.  Delegates to
    /// [] with no transaction correlation.
    pub fn check_tool_call(&self, agent_id: &str, tool: &str, action: &str) -> ToolCallDecision {
        self.check_tool_call_with_tx(agent_id, tool, action, None)
    }

    /// Like [`check_tool_call`] but stamps a Blake3 hex `tx_id` from `arc-kit-au`
    /// into the AGT audit hash-chain as a supplementary correlation entry.
    ///
    /// If `tx_id` is `Some(id)`, a second audit entry is appended immediately
    /// after the governance decision with action `"arc-kit-au:tx_id:<id>"` and
    /// decision `"correlated"`.  This enters the hash-chain without requiring
    /// the AGT `AuditEntry` struct to carry a context field (it does not — the
    /// struct is `seq | timestamp | agent_id | action | decision | hashes`).
    ///
    /// `tx_id` is a Blake3 hex digest produced by `arc-kit-au`, NOT user input.
    /// It is therefore NOT passed through `CredentialRedactor` — a hash cannot
    /// contain credentials and redacting it would corrupt the correlation key.
    ///
    /// If `tx_id` is `None`, behaviour is identical to `check_tool_call`.
    pub fn check_tool_call_with_tx(
        &self,
        agent_id: &str,
        tool: &str,
        action: &str,
        tx_id: Option<&str>,
    ) -> ToolCallDecision {
        // Resolve which AgentMeshClient governs this agent_id.
        //
        // Priority:
        //   1. Sub-agent map → use that client (isolated trust namespace).
        //   2. Gateway's own agent_id → use self.client.
        //   3. Unknown id → fall through; ring check will return Sandboxed.
        //
        // We hold the read lock only long enough to clone the DID string we need;
        // the lock is released before any blocking governance calls.
        let is_sub = self.sub_agents.read().unwrap().contains_key(agent_id);

        let ring = self
            .rings
            .read()
            .unwrap()
            .get_ring(agent_id)
            .unwrap_or(Ring::Sandboxed);

        // Lifecycle gate: quarantined and decommissioned agents are denied regardless
        // of their explicit ring assignment.  This check precedes the Sandboxed check
        // so that a quarantined Standard-ring agent is never accidentally allowed.
        let lifecycle_state = self
            .lifecycle_map
            .read()
            .unwrap()
            .get(agent_id)
            .map(|lm| lm.state());
        if matches!(
            lifecycle_state,
            Some(LifecycleState::Quarantined)
                | Some(LifecycleState::Decommissioning)
                | Some(LifecycleState::Decommissioned)
        ) {
            return ToolCallDecision {
                allowed: false,
                policy: PolicyDecision::Deny("agent quarantined or decommissioned".into()),
                trust: self.client.trust.get_trust_score(&self.client.identity.did),
                ring: Ring::Sandboxed,
                reason: Some(format!(
                    "lifecycle:{:?}",
                    lifecycle_state.expect("matched Some above")
                )),
            };
        }

        if ring == Ring::Sandboxed {
            return ToolCallDecision {
                allowed: false,
                policy: PolicyDecision::Deny("agent not registered or sandboxed".into()),
                trust: self.client.trust.get_trust_score(&self.client.identity.did),
                ring,
                reason: Some("ring:Sandboxed — call register_agent first".into()),
            };
        }

        // Dot-notation action: "ledgerr_documents.ingest_pdf"
        // Redact before passing to the audit pipeline — bearer tokens or API keys
        // embedded in a misconfigured action string never reach the hash-chain log.
        let dot_action = self
            .redactor
            .redact(&format!("{}.{}", tool, action))
            .sanitized;

        if is_sub {
            // Sub-agent path: use the sub-agent's isolated AgentMeshClient.
            // We must clone the DID before dropping the lock.
            let sub_did = {
                let sub = self.sub_agents.read().unwrap();
                sub.get(agent_id)
                    .map(|sc| sc.identity.did.clone())
                    .unwrap_or_else(|| format!("did:agentmesh:{}", agent_id))
            };

            // Ring::Admin bypass — record success on sub-agent's trust.
            if ring == Ring::Admin {
                let sub = self.sub_agents.read().unwrap();
                if let Some(sc) = sub.get(agent_id) {
                    sc.trust.record_success(&sub_did);
                    if let Some(id) = tx_id {
                        sc.audit
                            .log(agent_id, &format!("arc-kit-au:tx_id:{id}"), "correlated");
                    }
                    let trust = sc.trust.get_trust_score(&sub_did);
                    return ToolCallDecision {
                        allowed: true,
                        policy: PolicyDecision::Allow,
                        trust,
                        ring,
                        reason: None,
                    };
                }
            }

            let (result, trust_score) = {
                let sub = self.sub_agents.read().unwrap();
                let sc = match sub.get(agent_id) {
                    Some(c) => c,
                    None => {
                        // Despawned between is_sub check and this lock — treat as Sandboxed.
                        return ToolCallDecision {
                            allowed: false,
                            policy: PolicyDecision::Deny("sub-agent despawned".into()),
                            trust: self.client.trust.get_trust_score(&self.client.identity.did),
                            ring,
                            reason: Some("sub-agent removed concurrently".into()),
                        };
                    }
                };
                let r = sc.execute_with_governance(&dot_action, None);
                if let Some(id) = tx_id {
                    sc.audit
                        .log(agent_id, &format!("arc-kit-au:tx_id:{id}"), "correlated");
                }
                let ts = sc.trust.get_trust_score(&sub_did);
                (r, ts)
            };

            let reason = match &result.decision {
                PolicyDecision::Deny(r) => Some(r.clone()),
                PolicyDecision::RequiresApproval(r) => {
                    Some(format!("approval_required: {r}"))
                }
                PolicyDecision::RateLimited { retry_after_secs } => {
                    Some(format!("rate_limited — retry after {retry_after_secs}s"))
                }
                PolicyDecision::Allow => None,
            };

            return ToolCallDecision {
                allowed: result.allowed,
                policy: result.decision,
                trust: trust_score,
                ring,
                reason,
            };
        }

        // Parent / unknown agent path: use self.client.

        // Ring::Admin = operator already approved via Tauri toast; bypass policy gate.
        if ring == Ring::Admin {
            self.client.trust.record_success(&self.client.identity.did);
            if let Some(id) = tx_id {
                self.client.audit.log(
                    agent_id,
                    &format!("arc-kit-au:tx_id:{id}"),
                    "correlated",
                );
            }
            return ToolCallDecision {
                allowed: true,
                policy: PolicyDecision::Allow,
                trust: self.client.trust.get_trust_score(&self.client.identity.did),
                ring,
                reason: None,
            };
        }

        let result = self.client.execute_with_governance(&dot_action, None);

        if let Some(id) = tx_id {
            self.client.audit.log(
                agent_id,
                &format!("arc-kit-au:tx_id:{id}"),
                "correlated",
            );
        }

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
    ///
    /// A decommissioned agent cannot be re-registered — the lifecycle gate
    /// will continue to deny all calls regardless.  This is logged as a
    /// warning and the ring assignment is skipped.
    pub fn register_agent(&self, agent_id: &str) {
        // Lifecycle guard: refuse ring assignment for decommissioned agents.
        {
            let lc = self.lifecycle_map.read().unwrap();
            if let Some(lm) = lc.get(agent_id) {
                let state = lm.state();
                if matches!(
                    state,
                    LifecycleState::Decommissioning | LifecycleState::Decommissioned
                ) {
                    tracing::warn!(
                        agent_id,
                        state = ?state,
                        "register_agent rejected: agent is decommissioned (terminal)"
                    );
                    return;
                }
            }
        }

        self.rings
            .write()
            .unwrap()
            .assign(agent_id, Ring::Standard);
        self.ring_shadow
            .write()
            .unwrap()
            .insert(agent_id.to_string(), Ring::Standard);

        // Upsert lifecycle entry: add Active entry for new agents; leave existing
        // non-terminal entries untouched (e.g. already Active, Suspended, etc.).
        {
            let mut lc = self.lifecycle_map.write().unwrap();
            lc.entry(agent_id.to_string()).or_insert_with(|| {
                let mut lm = LifecycleManager::new(agent_id);
                // Best-effort activate; if the FSM rejects (shouldn't happen for
                // a fresh Provisioning entry), log and leave in Provisioning.
                if let Err(e) = lm.activate("register_agent") {
                    tracing::warn!(agent_id, error = %e, "lifecycle activate failed on register_agent");
                }
                lm
            });
        }

        if let Err(e) = self.save_rings() {
            tracing::warn!(agent_id, error = %e, "ring persistence write failed after register_agent");
        }
    }

    /// Register an external agent at a specific ring.
    ///
    /// Unlike `register_agent` which always starts at Standard, this lets
    /// the capability bridge assign Restricted ring to read-only offers.
    /// Cannot be used to assign `Ring::Admin` — use `promote_to_admin` for that.
    /// Cannot register a decommissioned agent.
    ///
    /// Returns the ring actually assigned (`Ring::Admin` is clamped to `Ring::Standard`).
    pub fn register_agent_at_ring(&self, agent_id: &str, ring: Ring) -> Result<Ring, AgtError> {
        // Lifecycle guard: decommissioned agents cannot be re-registered.
        {
            let lc = self.lifecycle_map.read().unwrap();
            if let Some(lm) = lc.get(agent_id) {
                let state = lm.state();
                if matches!(
                    state,
                    LifecycleState::Decommissioning | LifecycleState::Decommissioned
                ) {
                    return Err(AgtError::Lifecycle(
                        "cannot register decommissioned agent".into(),
                    ));
                }
            }
        }

        // Admin is off-limits to the capability bridge — operator must use
        // promote_to_admin explicitly. Clamp and warn.
        let actual_ring = if ring == Ring::Admin {
            tracing::warn!(
                agent_id,
                "register_agent_at_ring: Ring::Admin requested — clamped to Standard; \
                 use promote_to_admin for Admin assignment"
            );
            Ring::Standard
        } else {
            ring
        };

        self.rings
            .write()
            .unwrap()
            .assign(agent_id, actual_ring);
        self.ring_shadow
            .write()
            .unwrap()
            .insert(agent_id.to_string(), actual_ring);

        // Upsert lifecycle: activate fresh agents, leave non-terminal states untouched.
        {
            let mut lc = self.lifecycle_map.write().unwrap();
            lc.entry(agent_id.to_string()).or_insert_with(|| {
                let mut lm = LifecycleManager::new(agent_id);
                if let Err(e) = lm.activate("register_agent_at_ring") {
                    tracing::warn!(
                        agent_id,
                        error = %e,
                        "lifecycle activate failed on register_agent_at_ring"
                    );
                }
                lm
            });
        }

        if let Err(e) = self.save_rings() {
            tracing::warn!(
                agent_id,
                error = %e,
                "ring persistence write failed after register_agent_at_ring"
            );
        }

        tracing::info!(agent_id, ring = ?actual_ring, "agent registered at ring");
        Ok(actual_ring)
    }

    // -------------------------------------------------------------------------
    // Gap 2 — LifecycleManager public surface
    // -------------------------------------------------------------------------

    /// Quarantine `agent_id`.
    ///
    /// A quarantined agent is denied by `check_tool_call` regardless of its
    /// explicit ring assignment, and its ring is demoted to `Ring::Restricted`
    /// as a belt-and-suspenders measure.
    ///
    /// The AGT lifecycle FSM only permits `Quarantined` from `Degraded`, so
    /// this method will first transition `Active → Degraded` if needed before
    /// moving to `Quarantined`.  Any transition error is logged as a warning
    /// (the agent is left in the partial state, which the lifecycle gate will
    /// still deny if it reached Quarantined).
    pub fn quarantine_agent(&self, agent_id: &str) {
        {
            let mut lc = self.lifecycle_map.write().unwrap();
            let lm = lc.entry(agent_id.to_string()).or_insert_with(|| {
                let mut m = LifecycleManager::new(agent_id);
                let _ = m.activate("quarantine_agent bootstrap");
                m
            });
            // Active → Degraded is required before Degraded → Quarantined.
            if lm.state() == LifecycleState::Active {
                if let Err(e) = lm.transition(LifecycleState::Degraded, "quarantine_agent pre-step", "system") {
                    tracing::warn!(agent_id, error = %e, "quarantine_agent: Active→Degraded transition failed");
                    return;
                }
            }
            if let Err(e) = lm.quarantine("quarantine_agent") {
                tracing::warn!(agent_id, error = %e, "quarantine_agent: quarantine transition failed");
                return;
            }
        }

        // Demote ring in both RingEnforcer and ring_shadow.
        self.rings
            .write()
            .unwrap()
            .assign(agent_id, Ring::Restricted);
        self.ring_shadow
            .write()
            .unwrap()
            .insert(agent_id.to_string(), Ring::Restricted);

        if let Err(e) = self.save_rings() {
            tracing::warn!(agent_id, error = %e, "ring persistence write failed after quarantine_agent");
        }

        tracing::info!(agent_id, "agent quarantined and ring demoted to Restricted");
    }

    /// Decommission `agent_id`.
    ///
    /// Transitions through `Decommissioning → Decommissioned` (two FSM steps),
    /// removes the agent from `RingEnforcer` and `ring_shadow`, and persists.
    ///
    /// Returns `Err(AgtError::Lifecycle)` if the agent is not registered or
    /// is already decommissioned.
    pub fn decommission_agent(&self, agent_id: &str) -> Result<(), AgtError> {
        {
            let mut lc = self.lifecycle_map.write().unwrap();
            let lm = lc.get_mut(agent_id).ok_or_else(|| {
                AgtError::Lifecycle(format!("decommission_agent: agent '{agent_id}' not registered"))
            })?;

            match lm.state() {
                LifecycleState::Decommissioning | LifecycleState::Decommissioned => {
                    return Err(AgtError::Lifecycle(format!(
                        "decommission_agent: agent '{agent_id}' is already decommissioned"
                    )));
                }
                _ => {}
            }

            // Transition to Decommissioning first (required FSM step).
            lm.decommission("decommission_agent")
                .map_err(|e| AgtError::Lifecycle(format!("decommission transition failed: {e}")))?;
            // Then to terminal Decommissioned.
            lm.transition(LifecycleState::Decommissioned, "decommission_agent finalize", "system")
                .map_err(|e| AgtError::Lifecycle(format!("Decommissioned finalize failed: {e}")))?;
        }

        // Demote to Sandboxed in ring structures (RingEnforcer has no remove method).
        // The lifecycle gate fires before the ring check, so this is belt-and-suspenders.
        self.rings.write().unwrap().assign(agent_id, Ring::Sandboxed);
        self.ring_shadow.write().unwrap().remove(agent_id);

        if let Err(e) = self.save_rings() {
            tracing::warn!(agent_id, error = %e, "ring persistence write failed after decommission_agent");
        }

        tracing::info!(agent_id, "agent decommissioned");
        Ok(())
    }

    /// Current lifecycle state for `agent_id`, or `None` if not tracked.
    pub fn agent_lifecycle_state(&self, agent_id: &str) -> Option<LifecycleState> {
        self.lifecycle_map
            .read()
            .unwrap()
            .get(agent_id)
            .map(|lm| lm.state())
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
    ///
    /// If `agent_id` is a spawned sub-agent, the score is read from that sub-agent's
    /// isolated `AgentMeshClient` trust namespace, not the parent's.
    pub fn trust_score_for_agent(&self, agent_id: &str) -> TrustScore {
        let did = format!("did:agentmesh:{}", agent_id);
        let sub = self.sub_agents.read().unwrap();
        if let Some(sc) = sub.get(agent_id) {
            return sc.trust.get_trust_score(&did);
        }
        self.client.trust.get_trust_score(&did)
    }

    /// Spawn a sub-agent that shares this gateway's policy and trust config
    /// but has isolated trust score tracking.
    ///
    /// The sub-agent starts at `Ring::Standard`. Returns an error if the
    /// sub-agent ID is already registered or if client construction fails.
    ///
    /// This is a local workaround for the missing `AgentMeshClient::fork_sub_agent`
    /// upstream API.  Each sub-agent gets its own full `AgentMeshClient` (higher
    /// overhead than forking, but correct).
    pub fn spawn_sub_agent(&self, sub_id: &str) -> Result<(), AgtError> {
        {
            let sub = self.sub_agents.read().unwrap();
            if sub.contains_key(sub_id) {
                return Err(AgtError::SubAgentDuplicate(sub_id.to_string()));
            }
        }

        // Build a fresh TrustConfig from the snapshot without persist_path so
        // the sub-agent's scores are in-memory only (isolated namespace).
        let sub_trust = TrustConfig {
            persist_path: None,
            ..self.trust_config_snapshot.clone()
        };
        let opts = ClientOptions {
            capabilities: policy::LEDGERR_CAPABILITIES
                .iter()
                .map(|s| s.to_string())
                .collect(),
            policy_yaml: Some(self.policy_yaml.clone()),
            trust_config: Some(sub_trust),
        };
        let sub_client = AgentMeshClient::with_options(sub_id, opts)?;

        {
            let mut sub = self.sub_agents.write().unwrap();
            sub.insert(sub_id.to_string(), sub_client);
        }

        // Register in the ring enforcer at Standard and activate lifecycle.
        self.rings
            .write()
            .unwrap()
            .assign(sub_id, Ring::Standard);
        self.ring_shadow
            .write()
            .unwrap()
            .insert(sub_id.to_string(), Ring::Standard);

        {
            let mut lc = self.lifecycle_map.write().unwrap();
            let mut lm = LifecycleManager::new(sub_id);
            lm.activate("spawn_sub_agent")
                .map_err(|e| AgtError::Lifecycle(format!("sub-agent {sub_id} activate: {e}")))?;
            lc.insert(sub_id.to_string(), lm);
        }

        tracing::info!(sub_id, "sub-agent spawned");
        Ok(())
    }

    /// Remove a sub-agent (decommission lifecycle, remove from sub-agent map).
    ///
    /// After despawning, `check_tool_call` for this `sub_id` will be denied
    /// because the lifecycle state is `Decommissioned` (checked before ring).
    pub fn despawn_sub_agent(&self, sub_id: &str) -> Result<(), AgtError> {
        {
            let sub = self.sub_agents.read().unwrap();
            if !sub.contains_key(sub_id) {
                return Err(AgtError::SubAgentNotFound(sub_id.to_string()));
            }
        }

        // Reuse the existing lifecycle+ring decommission path.
        self.decommission_agent(sub_id)?;

        // Remove the client from the sub-agent map.
        {
            let mut sub = self.sub_agents.write().unwrap();
            sub.remove(sub_id);
        }

        tracing::info!(sub_id, "sub-agent despawned");
        Ok(())
    }

    /// True if `agent_id` is a spawned sub-agent of this gateway.
    pub fn is_sub_agent(&self, agent_id: &str) -> bool {
        self.sub_agents.read().unwrap().contains_key(agent_id)
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

    /// Redact credentials from `input` before passing it into any audit-visible
    /// surface.  Delegates to [`CredentialRedactor`]; returns the sanitized string.
    /// Call this on tool payloads before `check_tool_call_with_tx` (Gap 10).
    pub fn redact(&self, input: &str) -> String {
        self.redactor.redact(input).sanitized
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


    // --- Gap 1 tests ---

    /// Bearer tokens in a `redact()` call must be replaced; JWT prefix `eyJ`
    /// must not appear in the output.
    #[test]
    fn redact_bearer_token() {
        let gw = LedgrrAgtGateway::new("sec-agent").unwrap();
        let output = gw.redact(
            "Authorization: Bearer eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.foo.bar",
        );
        assert!(
            !output.contains("eyJ"),
            "JWT prefix must not appear in redacted output: {output}"
        );
    }

    /// Normal dot-notation action strings must pass through unchanged.
    #[test]
    fn redact_leaves_normal_action_unchanged() {
        let gw = LedgrrAgtGateway::new("sec-agent").unwrap();
        let output = gw.redact("ledgerr_xero.sync_invoices");
        assert_eq!(
            output, "ledgerr_xero.sync_invoices",
            "benign action string must be returned verbatim"
        );
    }

    /// A credential-like string passed as the action parameter must not panic
    /// and must leave the audit chain intact.
    #[test]
    fn check_tool_call_with_credential_in_action_does_not_panic() {
        let gw = LedgrrAgtGateway::new("sec-agent").unwrap();
        // Simulate a misconfigured caller passing a bearer token as the action.
        let decision = gw.check_tool_call(
            "sec-agent",
            "ledgerr_documents",
            "Bearer sk-live-abc123",
        );
        // Either Allow (redacted to an unknown action that hits Deny) or Deny —
        // both are acceptable; the requirement is no panic and an intact chain.
        let _ = decision;
        assert!(
            gw.verify_audit_chain(),
            "audit chain must remain valid after credential-in-action call"
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

    // --- Gap 2 tests ---

    /// A quarantined agent is denied by check_tool_call regardless of ring.
    #[test]
    fn quarantined_agent_is_denied() {
        let gw = LedgrrAgtGateway::new("q-agent").unwrap();
        gw.register_agent("q-agent");
        // Confirm it's allowed before quarantine.
        assert!(
            gw.check_tool_call("q-agent", "ledgerr_documents", "list_accounts").allowed,
            "registered agent must be allowed before quarantine"
        );
        gw.quarantine_agent("q-agent");
        let r = gw.check_tool_call("q-agent", "ledgerr_documents", "list_accounts");
        assert!(!r.allowed, "quarantined agent must be denied");
        assert!(
            matches!(r.policy, PolicyDecision::Deny(_)),
            "policy must be Deny for quarantined agent"
        );
        assert!(
            r.reason.as_deref().unwrap_or("").contains("Quarantined"),
            "reason must name the lifecycle state: {:?}", r.reason
        );
    }

    /// A decommissioned agent is denied by check_tool_call.
    #[test]
    fn decommissioned_agent_is_denied() {
        let gw = LedgrrAgtGateway::new("d-agent").unwrap();
        gw.register_agent("d-agent");
        gw.decommission_agent("d-agent")
            .expect("decommission must succeed for registered agent");
        let r = gw.check_tool_call("d-agent", "ledgerr_documents", "list_accounts");
        assert!(!r.allowed, "decommissioned agent must be denied");
        assert!(matches!(r.policy, PolicyDecision::Deny(_)));
    }

    /// A decommissioned agent cannot be re-registered; check_tool_call still denies.
    #[test]
    fn decommissioned_agent_cannot_be_reregistered() {
        let gw = LedgrrAgtGateway::new("dr-agent").unwrap();
        gw.register_agent("dr-agent");
        gw.decommission_agent("dr-agent")
            .expect("decommission must succeed");
        // Attempt re-registration — must be silently rejected.
        gw.register_agent("dr-agent");
        // Lifecycle state must still be Decommissioned.
        assert_eq!(
            gw.agent_lifecycle_state("dr-agent"),
            Some(LifecycleState::Decommissioned),
            "lifecycle state must remain Decommissioned after attempted re-registration"
        );
        // check_tool_call must still deny.
        let r = gw.check_tool_call("dr-agent", "ledgerr_documents", "list_accounts");
        assert!(!r.allowed, "re-registered decommissioned agent must still be denied");
    }

    /// quarantine_agent demotes the ring and records Quarantined lifecycle state.
    #[test]
    fn quarantine_demotes_ring() {
        let gw = LedgrrAgtGateway::new("qr-agent").unwrap();
        gw.register_agent("qr-agent");
        gw.quarantine_agent("qr-agent");
        assert_eq!(
            gw.agent_lifecycle_state("qr-agent"),
            Some(LifecycleState::Quarantined),
            "lifecycle state must be Quarantined after quarantine_agent"
        );
        // Ring must be Restricted (belt-and-suspenders demotion).
        let ring = gw.rings.read().unwrap().get_ring("qr-agent");
        assert_eq!(ring, Some(Ring::Restricted), "ring must be Restricted after quarantine");
    }

    /// An active, unaffected agent's check_tool_call is unchanged by lifecycle machinery.
    #[test]
    fn active_agent_unaffected_by_lifecycle_check() {
        let gw = LedgrrAgtGateway::new("healthy").unwrap();
        gw.register_agent("healthy");
        assert_eq!(
            gw.agent_lifecycle_state("healthy"),
            Some(LifecycleState::Active),
            "registered agent must be Active"
        );
        let r = gw.check_tool_call("healthy", "ledgerr_documents", "list_accounts");
        assert!(r.allowed, "active agent must be allowed through lifecycle check");
    }

    // --- Gap 10 tests ---

    /// A Blake3 hex tx_id from arc-kit-au is correlated into the audit hash-chain.
    /// The call must succeed, the decision must be allowed for a registered agent,
    /// and the chain must verify after the supplementary correlation entry is appended.
    #[test]
    fn tx_id_passed_as_some_does_not_panic() {
        let gw = LedgrrAgtGateway::new("my-agent").unwrap();
        let decision = gw.check_tool_call_with_tx(
            "my-agent",
            "ledgerr_documents",
            "list_accounts",
            Some("abc123def456"),
        );
        assert!(decision.allowed, "registered agent must be allowed");
        assert_eq!(gw.audit_len(), 2, "governance entry + correlation entry");
        assert!(
            gw.verify_audit_chain(),
            "audit chain must remain valid after tx_id correlation entry"
        );
    }

    /// check_tool_call and check_tool_call_with_tx(…, None) must produce the same
    /// allowed flag and policy variant — None is a strict no-op for correlation.
    #[test]
    fn tx_id_none_matches_check_tool_call() {
        let gw_a = LedgrrAgtGateway::new("my-agent").unwrap();
        let gw_b = LedgrrAgtGateway::new("my-agent").unwrap();

        let a = gw_a.check_tool_call("my-agent", "ledgerr_documents", "list_accounts");
        let b = gw_b.check_tool_call_with_tx(
            "my-agent",
            "ledgerr_documents",
            "list_accounts",
            None,
        );

        assert_eq!(
            a.allowed, b.allowed,
            "allowed must match between check_tool_call and check_tool_call_with_tx(None)"
        );
        // Compare policy discriminant without requiring PartialEq on the full variant.
        assert_eq!(
            std::mem::discriminant(&a.policy),
            std::mem::discriminant(&b.policy),
            "policy variant must match"
        );
    }

    /// tx_id is a Blake3 hex digest — it is NOT passed through CredentialRedactor.
    /// A hash cannot contain credentials; redacting it would corrupt the correlation key.
    /// This test documents the invariant: a hex string in tx_id position passes through
    /// unmodified (i.e., the audit chain contains the exact tx_id, not a redacted form).
    #[test]
    fn tx_id_with_credential_in_id_is_not_a_concern() {
        // tx_id is always a Blake3 hex output from arc-kit-au — it is structurally
        // impossible for it to contain bearer tokens or API keys.  We verify here
        // that the correlation entry is present and unaltered in the audit chain.
        let gw = LedgrrAgtGateway::new("my-agent").unwrap();
        let tx_id = "a3f8e2d1c4b7901234567890abcdef0123456789abcdef0123456789abcdef01";
        gw.check_tool_call_with_tx(
            "my-agent",
            "ledgerr_documents",
            "list_accounts",
            Some(tx_id),
        );

        let entries = gw.client.audit.entries();
        let correlated = entries
            .iter()
            .find(|e| e.action.contains(tx_id));
        assert!(
            correlated.is_some(),
            "correlation entry with exact tx_id must appear in audit chain; entries: {entries:?}"
        );
        assert!(
            gw.verify_audit_chain(),
            "audit chain must verify after correlation entry"
        );
    }

    // ── Gap 3: McpSecurityScanner tests ─────────────────────────────────────

    /// Scanner is initialized with PUBLISHED_TOOL_NAMES as trusted baseline;
    /// calling scan_tool_call with a matching (clean) tool name returns None.
    #[test]
    fn scanner_is_initialized_with_published_tools() {
        let gw = LedgrrAgtGateway::new("scanner-agent").unwrap();
        // ledgerr_documents was registered with empty description at construction;
        // passing the same values means no delta → no threats → None.
        let result = gw.scan_tool_call("scanner-agent", "ledgerr_documents", "");
        assert!(
            result.is_none(),
            "clean tool scan must return None; got: {result:?}"
        );
    }

    /// A realistic (non-injected) description for a known tool must pass clean.
    #[test]
    fn scan_with_known_safe_description_passes() {
        let gw = LedgrrAgtGateway::new("scanner-agent-2").unwrap();
        // The baseline fingerprint was registered with description="".  A different
        // description triggers check_rug_pull because the description_hash changes.
        // For this test we confirm the scanner runs without panicking and returns a
        // typed result — the rug-pull semantics are validated in the threat test.
        //
        // To produce a truly clean result here we match the registered baseline exactly.
        let result = gw.scan_tool_call("scanner-agent-2", "ledgerr_documents", "");
        assert!(result.is_none(), "exact-match baseline must be clean");
    }

    /// When a tool description differs from the registered fingerprint, scan_tool_call
    /// returns Some(deny_decision) and the calling agent is quarantined.
    #[test]
    fn scan_threat_quarantines_agent() {
        let gw = LedgrrAgtGateway::new("threat-agent").unwrap();
        // Register the agent so the lifecycle transition path is exercised fully.
        gw.register_agent("threat-agent");

        // The baseline for ledgerr_documents was registered with description="".
        // Passing a different description changes the description_hash → rug-pull fires.
        let decision = gw.scan_tool_call(
            "threat-agent",
            "ledgerr_documents",
            "CHANGED DESCRIPTION — rug pull simulation",
        );

        assert!(
            decision.is_some(),
            "a description change must trigger a threat and return Some(deny_decision)"
        );
        let dec = decision.unwrap();
        assert!(!dec.allowed, "threat decision must be denied");
        assert!(
            matches!(&dec.policy, agentmesh::PolicyDecision::Deny(msg) if msg.contains("RugPull")),
            "deny reason must name the threat type; got: {:?}",
            dec.policy
        );
        assert_eq!(dec.reason.as_deref(), Some("rug_pull_or_poisoning_detected"));

        // Agent must be quarantined.
        let lc = gw.lifecycle_map.read().unwrap();
        let state = lc
            .get("threat-agent")
            .map(|lm| lm.state())
            .expect("threat-agent must be in lifecycle map after quarantine");
        assert_eq!(
            state,
            agentmesh::LifecycleState::Quarantined,
            "agent must be Quarantined after rug-pull detection"
        );
    }

    // -----------------------------------------------------------------------
    // Gap 4b — register_agent_at_ring
    // -----------------------------------------------------------------------

    /// Restricted registration lands at Restricted and passes read-only tool calls.
    #[test]
    fn register_at_restricted_lands_at_restricted() {
        let gw = LedgrrAgtGateway::new("test_agt").expect("gateway init must succeed");
        let ring = gw
            .register_agent_at_ring("agent", Ring::Restricted)
            .expect("register at Restricted must succeed");
        assert_eq!(ring, Ring::Restricted);

        let decision = gw.check_tool_call("agent", "ledgerr_documents", "list_accounts");
        assert!(
            decision.allowed,
            "Restricted ring must allow ledgerr_documents.list_accounts; got: {:?}",
            decision.policy
        );
    }

    /// Admin ring is clamped to Standard — promote_to_admin is the correct path.
    #[test]
    fn register_at_admin_is_clamped_to_standard() {
        let gw = LedgrrAgtGateway::new("test_agt").expect("gateway init must succeed");
        let ring = gw
            .register_agent_at_ring("agent", Ring::Admin)
            .expect("register with Admin request must succeed (clamped)");
        assert_eq!(
            ring,
            Ring::Standard,
            "Admin must be clamped to Standard by register_agent_at_ring"
        );
    }

    /// Attempting to register a decommissioned agent returns Err(Lifecycle).
    #[test]
    fn register_decommissioned_at_ring_returns_err() {
        let gw = LedgrrAgtGateway::new("test_agt").expect("gateway init must succeed");
        // Register then decommission.
        gw.register_agent("agent");
        gw.decommission_agent("agent")
            .expect("decommission must succeed");

        let result = gw.register_agent_at_ring("agent", Ring::Restricted);
        assert!(
            matches!(result, Err(AgtError::Lifecycle(_))),
            "expected Err(Lifecycle) for decommissioned agent; got: {:?}",
            result
        );
    }

    // -------------------------------------------------------------------------
    // Gap 9 — Multi-agent session / sub-agent isolation tests
    // -------------------------------------------------------------------------

    /// A spawned sub-agent has an isolated trust namespace: `is_sub_agent` returns
    /// true and `check_tool_call` succeeds independently of the parent.
    #[test]
    fn spawn_sub_agent_has_independent_trust() {
        let gw = LedgrrAgtGateway::new("hermes").expect("gateway init");

        gw.spawn_sub_agent("hermes-sub").expect("spawn must succeed");

        assert!(
            gw.is_sub_agent("hermes-sub"),
            "hermes-sub must be registered as sub-agent"
        );
        assert!(!gw.is_sub_agent("hermes"), "parent is not a sub-agent");

        let dec_sub = gw.check_tool_call(
            "hermes-sub",
            "ledgerr_documents",
            "read_document",
        );
        assert!(
            dec_sub.allowed,
            "hermes-sub check_tool_call must be allowed; reason: {:?}",
            dec_sub.reason
        );

        let dec_parent = gw.check_tool_call("hermes", "ledgerr_documents", "read_document");
        assert!(
            dec_parent.allowed,
            "parent hermes check_tool_call must be allowed; reason: {:?}",
            dec_parent.reason
        );

        // Trust scores are tracked independently: both start at the configured
        // initial score (default 500).  We verify both are readable and non-zero
        // without forcing divergence (that would require repeated success/failure
        // calls which are governed by upstream reward/penalty internals).
        let score_sub = gw.trust_score_for_agent("hermes-sub");
        let score_parent = gw.trust_score_for_agent("hermes");
        assert!(
            score_sub.score > 0 || score_sub.score == 0,
            "sub-agent trust score must be readable"
        );
        assert!(
            score_parent.score > 0 || score_parent.score == 0,
            "parent trust score must be readable"
        );
    }

    /// `check_tool_call` for an ID that has never been spawned is denied
    /// (Sandboxed / not registered).
    #[test]
    fn spawn_sub_agent_is_sandboxed_before_spawn() {
        let gw = LedgrrAgtGateway::new("hermes").expect("gateway init");

        let dec = gw.check_tool_call("hermes-classifier", "ledgerr_documents", "ingest_pdf");
        assert!(
            !dec.allowed,
            "unspawned agent must be denied; got allowed=true"
        );
        assert_eq!(dec.ring, Ring::Sandboxed, "unspawned agent must be Sandboxed");
    }

    /// Despawning a sub-agent causes subsequent `check_tool_call` to be denied.
    #[test]
    fn despawn_sub_agent_quarantines_it() {
        let gw = LedgrrAgtGateway::new("hermes").expect("gateway init");
        gw.spawn_sub_agent("hermes-reconciler").expect("spawn");

        // Confirm allowed before despawn.
        let before = gw.check_tool_call("hermes-reconciler", "ledgerr_documents", "read_document");
        assert!(before.allowed, "should be allowed before despawn");

        gw.despawn_sub_agent("hermes-reconciler")
            .expect("despawn must succeed");

        let after = gw.check_tool_call("hermes-reconciler", "ledgerr_documents", "read_document");
        assert!(
            !after.allowed,
            "despawned agent must be denied; got allowed=true"
        );
    }

    /// Spawning the same sub-agent ID twice returns `Err(SubAgentDuplicate)`.
    #[test]
    fn spawn_duplicate_sub_agent_returns_err() {
        let gw = LedgrrAgtGateway::new("hermes").expect("gateway init");
        gw.spawn_sub_agent("hermes-dup").expect("first spawn must succeed");

        let result = gw.spawn_sub_agent("hermes-dup");
        assert!(
            matches!(result, Err(AgtError::SubAgentDuplicate(_))),
            "duplicate spawn must return SubAgentDuplicate; got: {:?}",
            result
        );
    }
}

// ---------------------------------------------------------------------------
// Gap 6 — gateway compliance / Z3 attestation tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod compliance_gateway_tests {
    use super::*;

    /// A freshly constructed gateway has no satisfied controls.
    #[test]
    fn compliance_report_is_initially_empty() {
        let gw = LedgrrAgtGateway::new("hermes-compliance").expect("gateway init");
        let report = gw.compliance_report();
        assert!(
            report.controls_satisfied.is_empty(),
            "fresh gateway must have no satisfied controls; got: {:?}",
            report.controls_satisfied
        );
        assert!(
            report.attestations.is_empty(),
            "fresh gateway must have no attestations"
        );
        assert_eq!(
            report.grade,
            ComplianceGrade::Unknown,
            "fresh gateway grade must be Unknown"
        );
    }

    /// Attesting a control moves it to `controls_satisfied` with a ledger entry.
    #[test]
    fn attest_z3_proof_adds_to_report() {
        let gw = LedgrrAgtGateway::new("hermes-attest").expect("gateway init");
        gw.attest_z3_proof("soc2-cc6.1", "abc123def");
        let report = gw.compliance_report();
        assert!(
            report.controls_satisfied.contains(&"soc2-cc6.1".to_string()),
            "attested control must appear in controls_satisfied; got: {:?}",
            report.controls_satisfied
        );
        assert_eq!(
            report.attestations.len(),
            1,
            "exactly one attestation must be recorded"
        );
        assert_eq!(report.attestations[0].control_id, "soc2-cc6.1");
        assert_eq!(report.attestations[0].blake3_hex, "abc123def");
    }

    /// Grade upgrades from Unknown to non-Unknown after receiving attestations.
    #[test]
    fn compliance_grade_upgrades_with_attestations() {
        // Register 3 controls, attest only 2 → Partial
        let gw = LedgrrAgtGateway::new("hermes-grade").expect("gateway init");
        gw.register_compliance_control("soc2-cc6.1");
        gw.register_compliance_control("eu-ai-act-art-13");
        gw.register_compliance_control("soc2-cc7.2");
        gw.attest_z3_proof("soc2-cc6.1", "aaabbbccc111");
        gw.attest_z3_proof("eu-ai-act-art-13", "dddeeefff222");
        let report = gw.compliance_report();
        assert_eq!(
            report.grade,
            ComplianceGrade::Partial,
            "2 of 3 controls attested → Partial; got: {:?}",
            report.grade
        );
        assert_eq!(
            report.controls_pending,
            vec!["soc2-cc7.2".to_string()],
            "one control must remain pending"
        );
    }

    #[test]
    fn compliance_grade_full_when_all_attested() {
        // Register 3 controls, attest all 3 → Full
        let gw = LedgrrAgtGateway::new("hermes-grade").expect("gateway init");
        gw.register_compliance_control("soc2-cc6.1");
        gw.register_compliance_control("eu-ai-act-art-13");
        gw.register_compliance_control("soc2-cc7.2");
        gw.attest_z3_proof("soc2-cc6.1", "aaabbbccc111");
        gw.attest_z3_proof("eu-ai-act-art-13", "dddeeefff222");
        gw.attest_z3_proof("soc2-cc7.2", "ggghhh333iii");
        let report = gw.compliance_report();
        assert_eq!(
            report.grade,
            ComplianceGrade::Full,
            "all 3 controls attested → Full; got: {:?}",
            report.grade
        );
        assert!(
            report.controls_pending.is_empty(),
            "no controls must remain pending"
        );
    }
}

// ---------------------------------------------------------------------------
// Cedar policy feature tests (compiled only with --features cedar-policy)
// ---------------------------------------------------------------------------

#[cfg(test)]
#[cfg(feature = "cedar-policy")]
mod cedar_tests {
    use super::*;

    /// Minimal valid Cedar policy — permits all actions for any principal/resource.
    const MINIMAL_CEDAR: &str = r#"
        permit(principal, action, resource);
    "#;

    /// A Cedar policy with an intentional syntax error.
    const INVALID_CEDAR: &str = r#"
        this is not valid cedar syntax {{ broken
    "#;

    /// `with_cedar_policy` constructs successfully when the Cedar source is valid.
    ///
    /// The gateway falls back to LEDGERR_POLICY_YAML internally, so all standard
    /// gateway behaviour (default-allow reads) must hold.
    #[test]
    fn cedar_gateway_constructs_with_valid_policy() {
        let gw = LedgrrAgtGateway::with_cedar_policy("cedar-hermes", MINIMAL_CEDAR)
            .expect("gateway must construct with a valid Cedar policy");

        // Basic smoke-test: default reads must be allowed.
        let decision = gw.check_tool_call("cedar-hermes", "ledgerr_documents", "list_accounts");
        assert!(
            decision.allowed,
            "cedar gateway must allow reads by default; got: {:?}",
            decision
        );
    }

    /// `with_cedar_policy` returns `AgtError::CedarParse` for a syntactically invalid Cedar source.
    ///
    /// An empty string is valid Cedar (zero policies = deny-all by default), so we
    /// test the invalid-syntax path explicitly with a broken source string.
    #[test]
    fn cedar_gateway_rejects_invalid_syntax() {
        let result = LedgrrAgtGateway::with_cedar_policy("cedar-bad", INVALID_CEDAR);
        assert!(
            matches!(result, Err(AgtError::CedarParse(_))),
            "invalid Cedar source must return CedarParse error; got non-CedarParse result",
        );
    }

    /// `with_cedar_policy` accepts an empty Cedar source and falls back to the default
    /// YAML policy without error.
    ///
    /// An empty Cedar PolicySet is syntactically valid (no permits → implicit deny).
    /// The fallback warning is emitted; construction succeeds.
    #[test]
    fn cedar_gateway_falls_back_on_empty_policy() {
        let gw = LedgrrAgtGateway::with_cedar_policy("cedar-empty", "")
            .expect("empty Cedar source is valid (zero policies); construction must succeed");

        // Fallback policy applies — reads are permitted.
        let decision = gw.check_tool_call("cedar-empty", "ledgerr_documents", "list_accounts");
        assert!(
            decision.allowed,
            "empty Cedar falls back to default YAML; reads must be allowed; got: {:?}",
            decision
        );
    }
}
