# Progressive, Role/Scope-Gated Tool Discovery — Design

Status: design complete; a first small increment (ring-gated `tools/list`
filtering, process-wide, opt-in) is implemented alongside this doc. Runtime
scope-increase and per-caller identity remain design-only — see §6.

Tracks [`ledgrrr#222`](https://github.com/PromptExecution/ledgrrr/issues/222)
(re-scoped backlog item, not blocking). Builds on the additive HTTP transport
in [`ledgrrr#223`](https://github.com/PromptExecution/ledgrrr/pull/223) and
the AGT governance gate in `crates/msft-agent-gov-ledgrrr` (its own
[`SPEC.md`](../crates/msft-agent-gov-ledgrrr/SPEC.md)).

Explicitly **not** in scope: a full `rmcp` SDK migration (re-scoped away from
this issue already), and Cedar policy (`cedar-policy` feature — SPEC.md's own
Phase 4, untouched here).

## 1. Current state (verified 2026-08-30)

### 1.1 `tools/list` today has no notion of a caller

`crates/ledgerr-mcp/src/bin/ledgerr-mcp-server.rs`'s `handle_request` is a
pure, transport-agnostic `fn(Value) -> Option<Value>` — confirmed unchanged
by #223, which reuses it verbatim for both the stdio loop (`serve()`) and the
new opt-in HTTP transport. Its `"tools/list"` arm is:

```rust
"tools/list" => {
    let tools = mcp_adapter::tool_descriptors();
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": { "tools": tools } }))
}
```

`mcp_adapter::tool_descriptors()` returns the same static, flat list to
every caller: the 13 `BUILTIN_TOOL_NAMES` (`ledgerr_documents`,
`ledgerr_review`, `ledgerr_reconciliation`, `ledgerr_workflow`,
`ledgerr_audit`, `ledgerr_tax`, `ledgerr_ontology`, `ledgerr_xero`,
`ledgerr_focus`, `ledgerr_evidence`, `ledgerr_schema`, `ledgerr_manifest`,
`ledgerr_budget` — confirmed by `docs/mcp-capability-contract.md` and the
`doc_01_mcp_boundary_tool_catalog_exposes_reduced_top_level_surface` test in
`crates/ledgerr-mcp/tests/mcp_adapter_contract.rs`) plus any external
provider tools registered under the `b00t` feature.

Critically: **neither `request` (the JSON-RPC envelope `handle_request`
receives) nor either transport carries any calling-agent identity.** There is
no header, no field, no session token — nothing to gate *by*. `tools/call`
dispatch (the large `match tool_name` in the same file) is likewise
unconditional: any caller can invoke any tool name it knows, listed or not
(the `l3dg3rr_*` / `proxy_*` names are deliberately-hidden compatibility
aliases per `docs/mcp-capability-contract.md`'s "Compatibility" section — not
security boundaries).

This is the real reason #222 can't be closed with a small patch: the
visibility/allow gating the issue asks for needs *something* to gate on, and
today's server has nothing.

### 1.2 `msft-agent-gov-ledgrrr` is the right integration point — confirmed, with one important caveat

`crates/msft-agent-gov-ledgrrr` wraps `agentmesh::AgentMeshClient` +
`agentmesh::RingEnforcer` behind `LedgrrAgtGateway`, with:

- Four execution rings — `Admin` (0) / `Standard` (1) / `Restricted` (2) /
  `Sandboxed` (3) — assigned per `agent_id` via `RingEnforcer`.
- A YAML policy engine (`LEDGERR_POLICY_YAML` in `src/policy.rs`) evaluated
  by `AgentMeshClient::execute_with_governance`.
- A SHA-256 hash-chain audit log, trust scoring, `LifecycleManager`
  (quarantine/decommission), credential redaction, and a security scanner —
  all wired and unit-tested (`crates/msft-agent-gov-ledgrrr/src/lib.rs`, 30+
  tests).
- `capability_bridge.rs` — already maps a `b00t_iface::CapabilityOffer` to a
  ring on agent registration (`ring_for_offer` / `accept_capability_offer`),
  which is precisely the "agent registration can grant additional tools"
  requirement from the issue, already built (for capability *offers*, not
  yet wired to MCP tool visibility).
- `http_gateway.rs` — builds an AGT `McpGateway` (HTTP-boundary session
  tokens, rate limiting, response scanning) intended, per its own doc
  comment and SPEC.md Gap 7 / Phase 4, for wrapping an HTTP-facing MCP
  endpoint. Not wired to #223's `tiny_http` transport (Phase 4, not done —
  SPEC.md is explicit that this is future work).
- `crates/ledgerr-mcp/Cargo.toml` already depends on `msft-agent-gov-ledgrrr`
  unconditionally — but **nothing in `crates/ledgerr-mcp/src/` calls into it
  today** (`grep -rn msft_agent_gov_ledgrrr crates/ledgerr-mcp/src/` was
  empty before this change). The dependency is declared but unused. So: yes,
  this is the natural integration point *architecturally* — but it is
  currently completely disconnected from the MCP server, not partially wired
  as the issue's phrasing ("could gate which tools a given ring sees, rather
  than being a purely allow/deny check at call time as it is today")
  slightly implies. It isn't even an allow/deny check at call time yet,
  because nothing calls `check_tool_call` from `ledgerr-mcp-server.rs`.

**The important caveat**, found while reading `rings.rs` /
`agentmesh-b00t/src/rings.rs` / `agentmesh-b00t/src/lib.rs` closely:
`configure_default_rings` populates `RingEnforcer`'s per-ring permission
table (`set_ring_permissions`), but `LedgrrAgtGateway::check_tool_call` only
ever calls `RingEnforcer::get_ring` — for the `Admin`-bypass and
`Sandboxed`-deny shortcuts. The actual Allow / Deny / RequiresApproval
decision for `Standard` and `Restricted` rings comes from
`AgentMeshClient::execute_with_governance`, which evaluates the single
shared `LEDGERR_POLICY_YAML` — a policy with **no ring parameter at all**.
`RingEnforcer::check_access(agent_id, action)` (the method that *would*
consult the per-ring permission table) is never called anywhere in
`msft-agent-gov-ledgrrr`. So today, **`Standard` and `Restricted` rings are
authorized identically** at `check_tool_call` time; only `Admin`
(bypass-everything) and `Sandboxed` (deny-everything) actually differ. The
rich per-ring permission lists in `rings.rs` are, as of this design pass,
call-time-inert — they were dead configuration until this change (§5) made
them power `tools/list` visibility instead. `rings.rs`'s own comment
half-admits this: `"Admin ring has implicit allow, but list for
documentation"` — the same is quietly true of Standard/Restricted too. This
doc's §5 increment gives that configuration its first real consumer;
ring-differentiated *call-time* enforcement remains a gap, tracked as a
sub-issue in §6.

### 1.3 `msft-agent-gov-ledgrrr`'s own SPEC.md already half-plans this

Its Phase 4 (`Gaps 5, 6, 7`) already lists "`McpGateway` HTTP wrapper for
`ledgerr-mcp` — Session token auth for external agents" as future work, and
its status header says "Phase 1 complete" even though the code shows Phase 2
(quarantine/decommission/persistence/`trust_score_for_agent`) and part of
Phase 3 (`capability_bridge.rs`) are actually implemented and tested — the
header is stale. Worth a docs fix in that crate independent of this issue.

## 2. Core group — proposal

**Core group: `ledgerr_schema` and `ledgerr_manifest`.** Always visible in
`tools/list`, for any caller, regardless of ring — including an unset or
unrecognized ring (i.e. "no identity available yet").

Rationale:
- Both are read-only-in-spirit introspection surfaces: `ledgerr_manifest`
  has exactly one action (`get_manifest` — the canonical viz-manifest DSL
  mapping); `ledgerr_schema` lists/inspects registered entity kinds
  (`list_kinds`, `get_kind`, plus `register_kind`/`remove_kind` which are
  writes, but scoped to schema metadata, not financial data).
- Neither appears in `PUBLISHED_TOOL_NAMES`
  (`crates/msft-agent-gov-ledgrrr/src/policy.rs`) or any ring's permission
  list in `rings.rs` — they were added to `ledgerr-mcp`'s `BUILTIN_TOOL_NAMES`
  after the AGT policy contract was written and never retrofitted into it.
  Rather than guessing which ring they "should" belong to, treating them as
  a ring-independent core group matches their actual current status (outside
  the AGT contract) instead of inventing a new one.
- They give an unauthenticated or newly-registered agent enough information
  to discover what it *could* ask for (schema shape, manifest DSL) without
  exposing any financial data or mutating operations — a reasonable
  "baseline capability every agent gets" per the issue's requirement.

**Explicitly not core:** `ledgerr_budget` (GPU-training cloud budget
reconciliation) — also outside the AGT contract today, but it touches
organizational cost data, not pure self-description, so it defaults to
ring-gated rather than core. It currently isn't listed in any ring's
permission set either; §6 sub-issue includes deciding where it belongs
(Standard, most likely) alongside `ledgerr_schema`/`ledgerr_manifest`'s
eventual promotion into the AGT policy contract proper.

**Important asymmetry to keep in mind:** the ring model treats `Sandboxed`
as "deny everything" at call time. Making `ledgerr_schema` /
`ledgerr_manifest` core means they're *visible* even to a Sandboxed or
unidentified caller — but §5's increment only changes `tools/list`, not
`tools/call` dispatch, which is unconditional today for every caller
regardless of ring. So "core tools are always visible" does not yet imply
"core tools are the *only* thing callable for Sandboxed callers" — that
would require call-time enforcement wired too (§6). This doc treats
visibility and callability as two separate, sequenced pieces of work, which
is consistent with MCP's own model (a listed tool isn't a promise the call
will succeed).

## 3. Ring → tool-family mapping

Rather than inventing a second mapping, this design derives `tools/list`
visibility **from the same action-pattern lists** `rings.rs`'s
`configure_default_rings` already declares for `RingEnforcer` — refactored
so both consumers read from one set of functions
(`admin_action_patterns()` / `standard_action_patterns()` /
`restricted_action_patterns()`), eliminating any risk of the visibility
mapping and the (currently call-time-inert, per §1.2) permission mapping
drifting apart. See `crates/msft-agent-gov-ledgrrr/src/rings.rs`.

Derived family sets (family = the part of each `"{family}.{action}"` pattern
before the first `.`):

| Ring | Visible families (beyond core) |
|------|---------------------------------|
| `Admin` | all 10 `PUBLISHED_TOOL_NAMES` families (documents, review, reconciliation, workflow, audit, tax, ontology, xero, evidence, focus) |
| `Standard` | documents, review, workflow, audit, tax, ontology, evidence, focus (no reconciliation, no xero) |
| `Restricted` | documents, audit, tax, evidence, focus (read-only subset of Standard) |
| `Sandboxed` | none (core group only) |

`ledgerr_budget` is visible under none of these today (see §2) — an open
item for the sub-issue that promotes schema/manifest/budget into the AGT
policy contract.

## 4. Requesting a scope increase at runtime — MCP-level mechanism

Not implemented in this pass — this section is the spec for follow-on work.

### 4.1 Survey: how other MCP servers handle dynamic tool visibility

The MCP spec (2024-11-05 and later) defines a `tools` server capability with
an optional `listChanged: bool` flag, and a corresponding
`notifications/tools/list_changed` notification the server can send when its
tool set changes; clients that declare support for it are expected to
re-issue `tools/list` on receipt rather than caching the result from
`initialize` forever. This is the spec-native hook for "an agent's available
tools changed without a new session" — it doesn't define an authorization
model on top (that's left to the server), but it is exactly the
notification primitive a scope-increase flow needs. Ledgerr's current
`"initialize"` response advertises `"capabilities": { "tools": {} }` — no
`listChanged` — so this needs adding.

The common pattern across MCP servers doing capability tiers (as opposed to
a flat list) is: authenticate/identify the caller once (session
token, API key, or OAuth at the transport layer), hold a per-session scope
server-side, filter `tools/list` by that scope, and emit
`list_changed` when the scope is mutated in-session (e.g. an OAuth
re-consent flow, or an explicit "request more access" tool call). Very few
servers implement scope escalation as its own tool call in the tool
catalog itself (that has an obvious bootstrapping snag: the escalation tool
must itself be in the core group, or the agent can never reach it) — most
push escalation to an out-of-band flow (re-auth) and treat the MCP session
as reflecting whatever the identity layer currently grants.

### 4.2 Proposed mechanism for ledgerr-mcp

1. **Core group includes a scope/identity tool**, e.g. extend
   `ledgerr_schema` or add a new minimal `ledgerr_scope` family (action
   `whoami` returning `{agent_id, ring, granted_families, core_families}`,
   and action `request_increase` taking `{ring: "standard"}` or
   `{tool: "ledgerr_reconciliation"}`).
2. **`request_increase` maps to `LedgrrAgtGateway`, not a parallel gate.**
   Internally it should call something like
   `gw.check_tool_call(agent_id, "ledgerr_scope", "request_increase")`
   against a **new** policy rule (or reuse the existing
   `commit-approval-gate` pattern) so the *decision* — auto-grant,
   `RequiresApproval`, or `Deny` — comes from the same YAML policy /
   trust-score pipeline as every other governed action, not a bespoke
   scope-increase engine. On approval, call
   `gw.register_agent_at_ring(agent_id, new_ring)` (already implemented,
   `capability_bridge.rs`-adjacent) or `gw.promote_to_admin` for the Admin
   case (operator-approval-only, per existing invariant).
3. **Server advertises `listChanged: true`** in `initialize`, and after a
   successful ring change, sends `notifications/tools/list_changed` on the
   same connection (works naturally for stdio, which is a persistent
   bidirectional pipe; needs the HTTP transport to move from #223's
   synchronous request/response `tiny_http` model to a streaming transport —
   SSE or MCP's Streamable HTTP — to support server-initiated pushes over
   HTTP; a pure `POST /` responder can't push a notification between
   requests). Until that lands, an HTTP caller can still poll `tools/list`
   after calling `request_increase` and see the widened set immediately
   (the RPC response to `request_increase` should also just say so) — the
   notification is a convenience, not a hard requirement, for HTTP.
4. **`tools/call` must also start enforcing per-request**, once there's an
   identity to check — right now (§1.1/§1.2) it enforces nothing. This is
   the biggest single piece of remaining work and the actual precondition
   for "requesting a scope increase" meaning anything: there's no scope to
   increase *from* until calls are being scoped at all.

### 4.3 The real precondition: caller identity has to exist first

None of §4.2 works without a way to know who's calling, per request, on
whichever transport. Candidates, not mutually exclusive:

- **stdio**: one process per agent already (see
  `crates/ledgerr-mcp/tests/mcp_stdio_e2e.rs` spawning a fresh server per
  test with `LEDGERR_MCP_MANIFEST` as an env var) — an
  `LEDGERR_MCP_AGENT_ID` env var set at spawn time is a natural fit,
  process-wide, matching this doc's §5 increment's `LEDGERR_MCP_RING`
  pattern exactly. Good enough for "one agent per stdio connection", which
  is the actual stdio usage pattern (desktop clients, `.mcpb` bundles).
- **HTTP** (#223 / ACA): a header, e.g. `X-Agent-Id` plus a bearer credential
  the mesh has already validated (see §4.4) — per-request, matching HTTP's
  stateless-request model, unlike stdio's process lifetime.
- Either way, `LedgrrAgtGateway` needs to move from "one gateway per
  process, one identity" (its current construction shape —
  `LedgrrAgtGateway::new(agent_id)`) to "one gateway, many identities looked
  up per request" for the HTTP case specifically — `register_agent` /
  `check_tool_call` already take an `agent_id` parameter per call, so the
  gateway itself is multi-agent-capable; it's `ledgerr-mcp-server.rs`'s
  single global `OnceLock` construction pattern (mirroring
  `global_raw_service()`) that needs to become "one shared gateway,
  identity read per request" instead.

### 4.4 Service-mesh integration (Dapr / ACA)

Per the issue's framing and `infrastructure#139`/`#141`: once `ledgerr-mcp`
runs behind a Dapr sidecar on ACA, the mesh is the natural place to
*authenticate* the caller (mTLS between sidecars, Dapr's app-id headers, or
a mesh-issued short-lived credential) — `ledgerr-mcp` itself should not need
to verify signatures or manage its own PKI. The mesh's job is to hand
`ledgerr-mcp` a verified `agent_id` (e.g. via a trusted header injected by
the sidecar, analogous to `dapr-app-id`); `ledgerr-mcp`'s job — via
`LedgrrAgtGateway` — is entirely the authorization/ring/trust/audit
question of *what that verified identity may see and do*, which is exactly
what `msft-agent-gov-ledgrrr` already does today for a single in-process
identity. This keeps the "mesh does authn, gateway does authz" boundary
clean and doesn't require `ledgerr-mcp` to grow its own credential
verification stack. `http_gateway.rs`'s `McpGateway` (session tokens, rate
limiting, response scanning) is the piece SPEC.md already earmarked
(Phase 4 / Gap 7) for the HTTP-boundary half of this — it isn't wired to
#223's transport yet, and doing so is a natural companion sub-issue to
per-request identity plumbing.

## 5. What's implemented in this pass

A small, additive, opt-in increment: **`RingEnforcer`-derived `tools/list`
filtering, gated by a process-wide `LEDGERR_MCP_RING` env var.**

- `crates/msft-agent-gov-ledgrrr/src/rings.rs`: refactored
  `configure_default_rings`'s three literal permission lists into
  `admin_action_patterns()` / `standard_action_patterns()` /
  `restricted_action_patterns()` (behavior-preserving — same strings, same
  `RingEnforcer` configuration as before). Added:
  - `CORE_TOOL_FAMILIES: &[&str]` — `["ledgerr_schema", "ledgerr_manifest"]`.
  - `ring_visible_tool_families(ring: Ring) -> BTreeSet<String>` — derives
    the family set from the same action patterns (§3's table), so there is
    exactly one place these lists live.
  - `ring_from_env_str(&str) -> Option<Ring>` — case-insensitive parser for
    the env var, `None` on anything unrecognized (fails open to "no
    filtering", not closed).
  - Unit tests for all of the above (family derivation per ring, subset
    relationships, core/published disjointness, env parsing).
- `crates/msft-agent-gov-ledgrrr/src/lib.rs`: re-exports `Ring` from the
  crate root (`pub use agentmesh::{..., Ring, ...}`) so `ledgerr-mcp` only
  needs its existing `msft-agent-gov-ledgrrr` dependency, no new Cargo.toml
  edit.
- `crates/ledgerr-mcp/src/mcp_adapter.rs`: `filter_tools_for_ring(tools,
  ring: Option<Ring>) -> Vec<Value>` — `None` or `Ring::Admin` returns
  `tools` unchanged; otherwise keeps only tools in `CORE_TOOL_FAMILIES` or
  `ring_visible_tool_families(ring)`. `tool_descriptors()` itself is
  untouched, so the existing
  `doc_01_mcp_boundary_tool_catalog_exposes_reduced_top_level_surface` test
  (asserting all 13 tools, unfiltered) keeps passing unmodified — filtering
  is applied only in the server binary, not in the shared adapter function
  other callers may rely on.
- `crates/ledgerr-mcp/src/bin/ledgerr-mcp-server.rs`: `tools/list` now
  calls `mcp_adapter::filter_tools_for_ring(tools, configured_ring())`,
  where `configured_ring()` reads `LEDGERR_MCP_RING` once per call via
  `rings::ring_from_env_str`. Unset (the default, and the only behavior
  before this change) → fully unfiltered, byte-for-byte identical output to
  before. `tools/call` dispatch is completely untouched.

### 5.1 What this deliberately is not

This is **not** per-calling-agent dynamic scoping — it's one ring for the
whole server process, because (§4.3) neither transport carries per-request
identity yet. It's a deliberate stepping stone: it proves the
`RingEnforcer`-config → `tools/list`-filtering wiring end-to-end, with a
config knob (`LEDGERR_MCP_RING`) that's a straightforward generalization
path to "look up ring per request" once identity exists — the filtering
function (`filter_tools_for_ring`) already takes `Option<Ring>` per call, so
swapping `configured_ring()` (env-var, process-wide) for a per-request
lookup is a localized change when that lands, not a rewrite.

Also not touched: `tools/call` enforcement (still unconditional for every
caller, exactly as before — this only affects what's *listed*), the
scope-increase mechanism (§4, design only), external provider tool
visibility under ring filtering (hidden whenever a ring is configured, since
they're in neither `CORE_TOOL_FAMILIES` nor any ring's family set — flagged
as an open question below), and anything Cedar/Phase-4-related.

## 6. Sub-issue breakdown

Filed as sub-issues linked from #222:

1. [`#224`](https://github.com/PromptExecution/ledgrrr/issues/224) —
   **Per-request caller identity on both transports** — `LEDGERR_MCP_AGENT_ID`
   env var for stdio (process-scoped, matches existing
   `LEDGERR_MCP_MANIFEST` convention) and an `X-Agent-Id` (+ credential)
   header for the #223 HTTP transport; `ledgerr-mcp-server.rs`'s global
   `OnceLock` gateway pattern needs to move to "one shared
   `LedgrrAgtGateway`, identity read per request" for HTTP specifically.
   Precondition for everything below.
2. [`#225`](https://github.com/PromptExecution/ledgrrr/issues/225) —
   **Wire `LedgrrAgtGateway::check_tool_call` into `tools/call` dispatch**,
   once #224 lands — today `tools/call` enforces nothing at all regardless of
   ring; this is the actual authorization gate the issue originally asked
   for, and per §1.2 it also needs `RingEnforcer::check_access` (or an
   equivalent ring-aware policy evaluation) to actually differentiate
   Standard from Restricted at call time, since `execute_with_governance`
   alone doesn't today.
3. [`#226`](https://github.com/PromptExecution/ledgrrr/issues/226) —
   **Scope-increase MCP mechanism** (§4.2) — new `ledgerr_scope` tool
   family (or extension of `ledgerr_schema`) with `whoami` /
   `request_increase` actions, backed by a new AGT policy rule (not a
   parallel engine), plus `listChanged: true` + `notifications/tools/list_changed`
   after a successful increase. Depends on #224 and #225.
4. [`#227`](https://github.com/PromptExecution/ledgrrr/issues/227) —
   **HTTP transport → streaming** (SSE or MCP Streamable HTTP), replacing or
   augmenting #223's synchronous `tiny_http` POST/response model, needed for
   server-initiated `list_changed` pushes over HTTP. Can ship independently
   of #226 — HTTP callers can poll `tools/list` after `request_increase`
   without it; only the push notification needs it.
5. [`#228`](https://github.com/PromptExecution/ledgrrr/issues/228) —
   **Wire `http_gateway.rs`'s `McpGateway` to the #223 HTTP transport**
   (SPEC.md Phase 4 / Gap 7 — session tokens, rate limiting, response
   scanning at the HTTP boundary) — natural companion to #224's HTTP identity
   header, since `McpGateway` is where a session-token-based identity would
   most naturally be validated before it ever reaches `LedgrrAgtGateway`.
6. [`#229`](https://github.com/PromptExecution/ledgrrr/issues/229) —
   **Promote `ledgerr_schema` / `ledgerr_manifest` / `ledgerr_budget` into
   the AGT policy contract** (`PUBLISHED_TOOL_NAMES`, `LEDGERR_POLICY_YAML`,
   ring permission lists) — decide `ledgerr_budget`'s ring (proposed:
   Standard) and whether `ledgerr_schema`'s mutating actions
   (`register_kind`/`remove_kind`) should stay core-visible while being
   gated at call time to a higher ring than `list_kinds`/`get_kind` (i.e.
   action-level, not just family-level, gating — a bigger step than
   anything in this doc, since today's `tools/list` and ring model both
   operate at family granularity).
7. [`#230`](https://github.com/PromptExecution/ledgrrr/issues/230) —
   **External provider tool visibility under ring filtering** — decide
   whether `b00t`-registered external provider tools (from
   `external_tool_descriptors()`) get their own ring mapping, inherit a
   default ring, or stay hidden whenever `LEDGERR_MCP_RING` is set (current
   behavior after this pass, chosen conservatively — no design work done on
   this yet).
8. [`#231`](https://github.com/PromptExecution/ledgrrr/issues/231) —
   **Stale `SPEC.md` status header fix** in `msft-agent-gov-ledgrrr` — says
   "Phase 1 complete" though Phase 2 and part of Phase 3 are implemented and
   tested. Small, independent, doesn't block anything above.

## 7. Non-goals (unchanged from #222 / operator direction)

- No `rmcp` SDK migration.
- No Cedar policy / `cedar-policy` feature enablement.
- No change to the ACA/Terraform side (`infrastructure` repo) — this PR is
  `ledgrrr`-only.
