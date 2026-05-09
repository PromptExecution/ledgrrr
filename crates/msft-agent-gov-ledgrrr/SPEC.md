# msft-agent-gov-ledgrrr — Spec-Driven Development

> **Status**: Phase 1 complete — core governance gate wired.  
> **Scope**: AGT integration contract, gap analysis, upstream proposals, and phased improvement roadmap.

---

## 1. Interface Contract

### 1.1 Public API — `LedgrrAgtGateway`

```rust
impl LedgrrAgtGateway {
    /// Create a gateway for `agent_id` with ledgrrr's default policy.
    /// Agent starts at Ring::Standard.
    pub fn new(agent_id: &str) -> Result<Self, AgtError>;

    /// Create with a custom initial TrustConfig (decay rate, initial score).
    pub fn with_trust_config(agent_id: &str, trust: TrustConfig) -> Result<Self, AgtError>;

    /// Check whether `agent_id` may call `tool` with `action`.
    /// Pipeline: ring check → policy engine → trust update.
    pub fn check_tool_call(&self, agent_id: &str, tool: &str, action: &str) -> ToolCallDecision;

    /// Promote `agent_id` to Ring::Admin after Tauri toast operator approval.
    pub fn promote_to_admin(&self, agent_id: &str);

    /// Register an external agent at Ring::Standard.
    pub fn register_agent(&self, agent_id: &str);

    /// Current trust score for any agent DID.
    pub fn trust_score(&self, did: &str) -> TrustScore;

    /// Verify the entire AGT audit hash-chain since gateway creation.
    pub fn verify_audit_chain(&self) -> bool;

    /// Number of governance decisions recorded in the audit log.
    pub fn audit_len(&self) -> usize;

    /// DID of the governed agent (e.g. `did:agentmesh:hermes`).
    pub fn agent_did(&self) -> &str;
}
```

### 1.2 `ToolCallDecision` — decision envelope

| Field    | Type             | Semantics |
|----------|------------------|-----------|
| `allowed`  | `bool`           | `true` = proceed immediately |
| `policy`   | `PolicyDecision` | `Allow \| Deny(String) \| RequiresApproval(String) \| RateLimited { retry_after_secs }` |
| `trust`    | `TrustScore`     | Current numeric trust score (0–1000) for the calling agent |
| `ring`     | `Ring`           | `Admin(0) \| Standard(1) \| Restricted(2) \| Sandboxed(3)` |
| `reason`   | `Option<String>` | Human-readable block/approval message; `None` when allowed |

### 1.3 Behavioral invariants

- **Sandboxed → always deny.** An agent not registered via `register_agent` or `promote_to_admin` receives `Deny("agent not registered or sandboxed")` for every call.
- **Admin → always allow.** `Ring::Admin` short-circuits the policy engine; the operator's Tauri toast approval is the trust anchor.
- **Rings are explicit-only.** Trust score changes do NOT automatically alter ring assignments. Only `register_agent`, `promote_to_admin`, or a future `demote_agent` call mutates ring state.
- **Dot-notation action format.** The policy engine receives `"{tool}_{subns}.{action}"` e.g. `ledgerr_documents.ingest_pdf`. YAML patterns must use underscores in the namespace prefix, not dots.
- **Hash-chain audit is always-on.** Every call to `check_tool_call` for non-Sandboxed agents appends an `AuditEntry` to the SHA-256 hash-chain.
- **Policy is loaded once at construction.** `LEDGERR_POLICY_YAML` is a static constant embedded at compile time.

### 1.4 Policy rule set (v1.0)

| Name | Type | Priority | Effect |
|------|------|----------|--------|
| `block-shell` | capability | 100 | Deny `shell:*`, `system:*`, `os:*` |
| `commit-approval-gate` | approval | 90 | RequiresApproval for `commit*`, `reverse*`, `approve*` on reconciliation + workflow |
| `ingest-rate-limit` | rate_limit | 80 | 120 calls/60s for `ingest_pdf`, `ingest_rows` |
| `xero-rate-limit` | rate_limit | 75 | 30 calls/60s for `ledgerr_xero.*` |
| `allow-all-ledgerr-ops` | capability | 50 | Allow `ledgerr_*.*` |

### 1.5 Ring permission matrix

| Ring | Trust range | CommitGate tier | Permitted actions |
|------|-------------|-----------------|-------------------|
| Admin | 900–1000 | Approved (commit) | All ledgerr tools — commit/reverse included |
| Standard | 500–899 | Approved (write) | Ingest, classify, read, ontology, focus, evidence, tax read, workflow classify |
| Restricted | 300–499 | PendingOperator | Read-only: list_accounts, get_raw_context, audit log, schedule summary, evidence |
| Sandboxed | 0–299 | Blocked | Nothing |

---

## 2. Gap Analysis

The following AGT capabilities exist in `agentmesh` v3.5.0 but are **not yet wired** in `LedgrrAgtGateway`.

### Gap 1 — No `CredentialRedactor` in audit pipeline

**What AGT offers**: `CredentialRedactor` strips API keys, bearer tokens, and connection strings from payloads before they enter the audit log. Pattern list: AWS/GCP/Azure key formats, `Bearer …`, `postgresql://…`.

**Ledgrrr risk**: Xero OAuth tokens and ATO/IRS credentials in `ledgerr_xero.*` calls appear in the hash-chain audit in plaintext. The audit log is write-protected but not confidential.

**Spec**: Every `AuditEntry` payload MUST be passed through `CredentialRedactor` before storage. This must happen inside `check_tool_call`, not at the gateway boundary, so the hash-chain integrity is over redacted data from inception.

---

### Gap 2 — No `LifecycleManager`

**What AGT offers**: State machine `Created → Active → Suspended → Quarantined → Decommissioned`. `quarantine(id)` removes ring permissions and freezes trust accumulation. `decommission(id)` is terminal.

**Ledgrrr risk**: Compromised or misbehaving sub-agents (e.g. an MCP client that starts producing anomalous reconciliation calls) have no quarantine path. Only option today is manual ring demotion.

**Spec**: `LedgrrAgtGateway` must expose `quarantine_agent(id)` and `decommission_agent(id)`. A quarantined agent must be treated as `Ring::Sandboxed` by `check_tool_call` regardless of explicit ring assignment.

---

### Gap 3 — No `McpSecurityScanner`

**What AGT offers**: Tool-schema fingerprinting + rug-pull detection across 6 threat types: `ToolPoisoning`, `RugPull`, `CrossServerAttack`, `DescriptionInjection`, `SchemaAbuse`, `HiddenInstruction`.

**Ledgrrr risk**: The `ledgerr-mcp` server exposes 10 tool schemas over RMCP. A compromised client could submit mutated schemas or inject hidden instructions into tool descriptions. Currently no fingerprint baseline is maintained.

**Spec**: On gateway construction, `McpSecurityScanner` must be initialized with the 10 PUBLISHED_TOOL_NAMES as the trusted schema baseline. `check_tool_call` must run the scanner against the current tool schema before allowing execution. Any rug-pull detection must:
1. Immediately quarantine the calling agent
2. Deny the call
3. Emit an `AuditEntry` with severity `Critical`

---

### Gap 4 — No `TrustHandshake` / `CapabilityRegistry` (IATP bridge)

**What AGT offers**: IATP capability negotiation via `TrustHandshake` — verifies counterparty DID, exchanges `CapabilityToken`s, establishes a session trust score.

**What ledgrrr has**: `b00t-iface` `CapabilityOffer` — inter-node capability trading with `offer_id`, capability set, and TTL.

**Gap**: The two systems are structurally parallel but not bridged. An incoming `CapabilityOffer` from another `b00t` node doesn't produce an AGT `TrustScore` or register the offering agent in `RingEnforcer`.

**Spec**: Write a `CapabilityBridge` that converts a `b00t-iface::CapabilityOffer` into a `TrustHandshake` request and registers the result as a new agent at the appropriate ring. See §4.3.

---

### Gap 5 — No Cedar / OPA policy backend

**What AGT offers**: `CedarEvaluator` (Amazon Cedar policies) and `OPAEvaluator` (Rego policies) as drop-in policy backends alongside the existing YAML engine.

**Ledgrrr status**: Only the YAML backend is used. The YAML policy is sufficient for the current 5-rule set, but tax rule logic in `rhai` and Z3 proof certificates (Phase 3) need a richer policy language.

**Spec**: Do not migrate to Cedar/OPA yet. Add a feature flag `cedar-policy` that swaps `LEDGERR_POLICY_YAML` for a Cedar bundle at construction time. This is a Phase 3 concern. Upstream action: none (AGT already supports it).

---

### Gap 6 — No `ComplianceEngine` wiring

**What AGT offers**: `ComplianceEngine` grades an agent session against OWASP Top 10, EU AI Act, HIPAA, SOC 2 control sets. Returns a graded report.

**Ledgrrr opportunity**: Z3 proof certificates from `arc-kit-au` attestations map naturally to SOC 2 and EU AI Act audit evidence requirements.

**Spec**: Expose `compliance_report() -> ComplianceReport` on `LedgrrAgtGateway`. Phase 3: feed Z3 attestation hashes as evidence into the compliance engine to auto-satisfy audit controls.

---

### Gap 7 — `McpGateway` not wired for HTTP boundary

**What AGT offers**: `McpGateway` — an HTTP proxy layer that authenticates, rate-limits, and scans MCP calls over the network. Requires session tokens, external agent identity verification.

**Ledgrrr status**: `LedgrrAgtGateway` is in-process only. This is correct for the current single-operator deployment model.

**Spec**: Phase 3 (multi-agent / Hermes mesh). When `ledgerr-mcp` exposes an HTTP-facing RMCP endpoint, wrap it with `McpGateway` using `LedgrrAgtGateway` as the backing governance decision source. No changes needed in Phase 1/2.

---

### Gap 8 — Policy loaded from static constant

**Ledgrrr status**: `LEDGERR_POLICY_YAML` is `const` — embedded at compile time, immutable at runtime.

**Risk**: Operator cannot tune rate limits or add emergency deny rules without a rebuild. Xero API limits change (currently 30/min; could change in an ATO/Xero policy update).

**Spec**: Add `LedgrrAgtGateway::with_policy_path(agent_id, path)` constructor that reads a YAML file at runtime. Fall back to `LEDGERR_POLICY_YAML` if the path doesn't exist. This enables hot-patching policy without a full rebuild.

---

### Gap 9 — Single identity; no multi-agent sessions

**Ledgrrr status**: `LedgrrAgtGateway::new("hermes")` creates one `AgentMeshClient` with the identity `did:agentmesh:hermes`. All policy evaluation runs under this identity.

**Risk**: When Hermes spawns sub-agents (Phase 2), each sub-agent needs its own trust profile. Currently `register_agent` adds them to `RingEnforcer` but they all evaluate against the gateway's single `AgentMeshClient` identity.

**Spec**: `AgentMeshClient` needs to be called once per sub-agent for trust tracking, not shared. This requires either:
- (a) A `HashMap<agent_id, AgentMeshClient>` in the gateway — high overhead, one full policy load per agent
- (b) An upstream `AgentMeshClient::for_sub_agent(id)` that shares the policy engine but creates a separate trust namespace

**Upstream proposal**: Request `AgentMeshClient::fork_sub_agent(id) -> AgentMeshClient` that inherits the parent's policy engine and audit logger but isolates trust score. See §3.1.

---

### Gap 10 — No `arc-kit-au` tx_id correlation in audit entries

**Ledgrrr status**: `arc-kit-au` assigns Blake3 content-hash IDs to every transaction node. AGT `AuditEntry` has a free-form `context` field but no structured correlation ID.

**Risk**: When an AGT audit log says "allow ledgerr_reconciliation.commit_entry at 14:32:01", there is no way to trace which transaction IDs were committed without cross-referencing timestamps — fragile for forensic review.

**Spec**: Add `check_tool_call_with_tx_id(agent_id, tool, action, tx_id: Option<Blake3Hash>)` that appends `tx_id` to the AGT `AuditEntry.context` field. The CPA can then trace every governance decision to a specific ledger transaction.

---

### Gap 11 — No persistence for rings or trust scores

**Ledgrrr status**: Ring assignments and trust scores live only in memory. Gateway restart resets all agents to `Ring::Standard` with trust 500.

**Risk**: In a long-running session (Hermes processing 6 years of retroactive PDFs), a gateway restart mid-run could silently re-promote quarantined agents.

**Spec**: Pass a `persist_path` to `TrustConfig` (AGT already supports this field). Serialize ring assignments to a JSON sidecar at the workbook path: `{workbook}.agt-rings.json`. Load on construction.

---

### Gap 12 — `trust_score()` DID mismatch

**Ledgrrr status**: `trust_score(did)` takes a full DID string (`did:agentmesh:hermes`). But `register_agent(id)` and `check_tool_call(agent_id, ...)` take bare `agent_id` strings. The bridge from `agent_id → DID` is silent: `format!("did:agentmesh:{}", id)`.

**Risk**: If a caller passes a raw `agent_id` to `trust_score()` instead of the formatted DID, they get a `TrustScore` of 0 (not-found) with no error. Silent failure.

**Spec**: Remove `trust_score(did)` from the public API. Replace with `trust_score_for_agent(agent_id: &str) -> TrustScore` that constructs the DID internally. The raw DID variant is `#[doc(hidden)]`.

---

## 3. Upstream Proposals

### 3.1 `AgentMeshClient::fork_sub_agent(id)` (Gap 9)

**Problem**: Sharing one `AgentMeshClient` across all sub-agents means all trust tracking is conflated under a single DID. The policy engine and audit logger are stateless enough to share, but `TrustManager` must be per-agent.

**Proposed API addition to `agentmesh`**:
```rust
impl AgentMeshClient {
    /// Create a sub-agent client that shares the parent's policy engine and audit
    /// logger but has an isolated TrustManager namespace.
    pub fn fork_sub_agent(&self, sub_id: &str) -> AgentMeshClient;
}
```

**Implementation sketch**: `fork_sub_agent` clones `Arc<PolicyEngine>` and `Arc<AuditLogger>` (already `Arc`-wrapped in v3.5.0), but creates a fresh `TrustManager` initialized with the parent's `TrustConfig`. The sub-agent DID is `did:agentmesh:{sub_id}`.

**Impact**: No breaking change. New method only.

---

### 3.2 `AuditEntry` structured correlation ID (Gap 10)

**Problem**: `AuditEntry.context` is `Option<String>`. Structured forensic cross-reference requires a typed field.

**Proposed API change to `agentmesh`**:
```rust
pub struct AuditEntry {
    pub timestamp: u64,
    pub agent_did: String,
    pub action: String,
    pub decision: String,
    pub trust_score: u32,
    pub context: Option<String>,
    // NEW:
    pub correlation_id: Option<String>,  // opaque, caller-assigned; e.g. Blake3 hex tx_id
    pub prev_hash: String,
    pub entry_hash: String,
}
```

**Impact**: Additive field; default `None`. Hash is computed over all fields including `correlation_id`.

---

### 3.3 No other upstream changes needed

The remaining gaps (1–3, 5–8, 11–12) are addressable on the ledgrrr side without modifying `agentmesh`.

---

## 4. Interface Improvements — Phased Plan

### Phase 1 (done) — Core governance gate

- [x] `LedgrrAgtGateway` wrapping `AgentMeshClient` + `RingEnforcer`
- [x] 5-rule YAML policy (block-shell, commit-gate, ingest rate, xero rate, allow-all)
- [x] 4-tier ring model mapped to `CommitGate`
- [x] 7 unit tests covering allow/deny/approval/sandbox/admin/audit/DID

### Phase 2 — Hardening (Gaps 1, 2, 8, 11, 12)

**Target**: Before Hermes agent mesh goes live.

| Task | Gap | API change |
|------|-----|------------|
| Wire `CredentialRedactor` into audit pipeline | 1 | internal only |
| Add `quarantine_agent` / `decommission_agent` | 2 | new public methods |
| `with_policy_path` constructor | 8 | new constructor |
| Ring + trust persistence via `TrustConfig.persist_path` + JSON sidecar | 11 | new constructor param |
| Replace `trust_score(did)` with `trust_score_for_agent(id)` | 12 | API rename (non-breaking — add new, deprecate old) |

**New public API after Phase 2**:
```rust
// Constructors
pub fn with_policy_path(agent_id: &str, policy_path: &Path) -> Result<Self, AgtError>;
pub fn with_persist_path(agent_id: &str, sidecar_dir: &Path) -> Result<Self, AgtError>;

// Lifecycle
pub fn quarantine_agent(&self, agent_id: &str);
pub fn decommission_agent(&self, agent_id: &str) -> Result<(), AgtError>;
pub fn agent_lifecycle_state(&self, agent_id: &str) -> LifecycleState;

// Trust (ID-safe)
pub fn trust_score_for_agent(&self, agent_id: &str) -> TrustScore;

// Audit with correlation
pub fn check_tool_call_with_tx(
    &self,
    agent_id: &str,
    tool: &str,
    action: &str,
    tx_id: Option<&str>,  // Blake3 hex
) -> ToolCallDecision;
```

### Phase 3 — Security scanner + multi-agent (Gaps 3, 9, 10)

**Target**: When Hermes sub-agents are spawned from `b00t-iface`.

| Task | Gap | Dependency |
|------|-----|------------|
| Wire `McpSecurityScanner` with PUBLISHED_TOOL_NAMES baseline | 3 | Phase 2 done |
| Upstream `fork_sub_agent` (or local workaround) | 9 | Upstream PR #3.1 |
| `check_tool_call_with_tx` correlation IDs | 10 | Phase 2 `check_tool_call_with_tx` |

**`CapabilityBridge`** — new module `crates/msft-agent-gov-ledgrrr/src/capability_bridge.rs`:
```rust
/// Convert a b00t-iface CapabilityOffer into an AGT trust event + ring assignment.
pub fn accept_capability_offer(
    gw: &LedgrrAgtGateway,
    offer: &b00t_iface::CapabilityOffer,
) -> Result<Ring, AgtError>;
```

Maps `CapabilityOffer.name` → ring (actual field — `capabilities` does not exist on the struct):
- `ledgerr_reconciliation.commit` substring in `offer.name` → intent Admin, assigned Standard pending operator confirmation
- Any `ledgerr_*` write cap name → Standard
- Read-only cap names only → Restricted (note: `register_agent_at_ring` not yet implemented; Restricted currently lands at Standard — tracked as pre-Gap-5 debt)

### Phase 4 — Cedar / Compliance / HTTP boundary (Gaps 5, 6, 7)

**Target**: Hermes HTTP mesh + CPA compliance reporting.

| Task | Gap | Notes |
|------|-----|-------|
| `cedar-policy` feature flag | 5 | Cedar bundle path at construction |
| `compliance_report()` on gateway | 6 | Feed Z3 attestation hashes as evidence |
| `McpGateway` HTTP wrapper for `ledgerr-mcp` | 7 | Session token auth for external agents |

---

## 5. Test Specification

Every public method in §4's phased API additions must have a companion test before merging.

### Required test cases (Phase 2 additions)

| Test | Assertion |
|------|-----------|
| `credential_in_xero_call_is_redacted` | Audit entry context must not contain `Bearer ` prefix strings |
| `quarantined_agent_is_denied` | After `quarantine_agent`, `check_tool_call` returns `Deny` even if ring was Standard |
| `decommissioned_agent_is_denied_and_terminal` | After `decommission_agent`, agent cannot be re-registered |
| `policy_path_falls_back_to_default` | `with_policy_path(nonexistent)` uses `LEDGERR_POLICY_YAML` |
| `trust_score_for_unregistered_agent_returns_initial` | `trust_score_for_agent("nobody")` returns `initial_score` (default 500) without panic — security enforcement is the ring gate, not the trust score |
| `tx_id_appears_in_audit_entry` | `check_tool_call_with_tx(..., Some("abc123"))` produces `audit_entry.correlation_id == Some("abc123")` |
| `rings_persist_across_construction` | Construct gateway, promote agent, drop, reconstruct from sidecar, assert ring == Admin |

---

## 6. Non-Goals

- **Not** replacing `rhai` rule engine with Cedar/OPA for tax classification. Rhai handles runtime-editable classification; AGT policy handles agent governance. These are orthogonal.
- **Not** sending audit logs to cloud. The hash-chain audit log stays local at the workbook sidecar path. `export_json()` is available for CPA hand-off.
- **Not** implementing DID resolution via a DID registry. `did:agentmesh:` DIDs are self-sovereign local identities only.
- **Not** replacing `CommitGate` with AGT ring model. `CommitGate` is the workbook-level mutation guard; AGT rings are the agent-call governance layer. They are complementary.
