# Statechart Tooling Survey & Harmonization Plan

*lazily-rs v0.59+ as the unified runtime across b00t-iface, kr0ki, ufo-types, and ledgerr-mcp*

---

## 1. Decision Summary

**Chosen runtime: [`lazily::statechart`](https://github.com/lazily-hub/lazily-rs)**

| Property | Value |
|----------|-------|
| Crate | `lazily = { version = "0.59", features = ["statechart", "thread-safe"] }` |
| Model | Harel Statecharts (SCXML-compatible), hierarchical states, parallel regions |
| Concurrency | `ThreadSafeStateChart` + `ThreadSafeContext` — cheaply cloneable, lock-free send |
| Effects | `entry`/`exit` action strings → matched by caller; no magic callbacks |
| Guards | `HashMap<String, bool>` passed at send-time — caller evaluates predicates |
| MSRV | Rust 1.88 |
| License | Apache-2.0 |

Evaluated and rejected alternatives:

| Library | Language | Why not |
|---------|----------|---------|
| statecharts.dev / XState | TypeScript/JS | Wrong language. Use as visual design tool only, not Rust runtime. |
| cmars/statechart | Go | Wrong language. |
| `statig` | Rust | Simpler flat HSM macro; no reactive cells, no parallel regions, no SCXML-level hierarchy. Covers a subset of what lazily covers. |
| `scxml` crate | Rust | Document-model / interchange layer only — no executor. Stays for export. |

---

## 2. What Each Component Has Today

### 2a. ufo-types (`src/statechart.rs` — rev 1846e6f)

- Exports the OODA proposal lifecycle as a **W3C SCXML document** via the `scxml` crate.
- `ooda_phases_to_statechart()` builds a `Statechart` struct (states + transitions) from `OodaPhase`/`OodaEvent` matches in the authoritative `OodaStateMachine::dispatch`.
- This is **interchange / export only** — ufo-types ships no live executor.
- Pattern: the authoritative transition table lives in Rust match arms; statechart.rs mirrors it for tooling (diagrams, validators, SCXML round-trip).

### 2b. b00t-iface (`src/core/machine.rs` — PR #267, branch `task/b00t-lazily-surface-chart`)

- `SurfaceChart` (feature-gated `#[cfg(feature = "lazily")]`) wraps a `ThreadSafeStateChart`.
- `surface_chart_def()` builds a `ChartDef` from the `valid_transition` table in the same file — the canonical authoritative shape is `valid_transition(from, to) -> Option<&str>`.
- States: Idle → Ready → Running → Healthy | Quarantined → Terminated.
- `SurfaceChart::send(event, guards)` drives the live executor; `last_actions()` surfaces entry effects for caller dispatch.
- Existing `MachineTransition` / `valid_transition` static table remains as the MECE truth — `SurfaceChart` is an additive live layer on top.

### 2c. kr0ki-core (`src/chart.rs` — PR #265, branch `task/259-p4-kr0ki-statechart`)

- `ProcessChart` builds a `ThreadSafeStateChart` from SysML-v2 element/relation slices.
- Mapping: `ActionDefinition` → compound root state; `ActionUsage` → atomic leaf; `Succession` → `on("next", target)` transition.
- `from_sysml(elements, relations) -> Result<ProcessChart, Kr0kiError>` is the public API.
- Phases 1–4 done; Phase 5 (DecisionNode/ForkNode/JoinNode) blocked on ufo-types PR #13 (issue #268).

### 2d. ledgerr-mcp (`src/hsm.rs` — PR #264, branch `task/227-sse-streamable-http`)

- `HsmChart` drives the tax-pipeline lifecycle: Ingest → Normalize → Validate → Reconcile → Commit → Summarize.
- Each stage has `.pending` / `.ready` substates (hierarchical).
- `TurboLedgerService` holds `Mutex<Option<HsmChart>>`; PM hooks fire as lazily entry-action strings when the chart advances.
- `advance()`, `fast_forward_to()`, `resume_from_marker()` — chart-driven PM events, not manual state writes.

---

## 3. Harmonization Plan

### Principle: One Runtime, Three Roles

```
lazily::statechart::ThreadSafeStateChart
    ├── b00t-iface:   SurfaceChart      — surface lifecycle (operational)
    ├── kr0ki-core:   ProcessChart      — SysML-v2 process topology (structural)
    └── ledgerr-mcp:  HsmChart          — tax-pipeline stage machine (domain)

scxml crate (interchange / export only)
    └── ufo-types:    ooda_phases_to_statechart() — OODA lifecycle diagram export
```

Authoritative transition logic **always lives as Rust match arms / tables** in each crate. lazily drives the live executor against those shapes. The `scxml` crate exports them for diagram tools (statecharts.dev visual editor accepts SCXML).

### Design invariants

1. **No prompt construction in the statechart layer.** lazily entry/exit actions are plain `String` tokens — the caller matches them and decides what to do.
2. **Guards are caller-evaluated predicates.** `send(ctx, event, &guards)` receives a `HashMap<String, bool>`; the chart never imports business logic.
3. **One `ChartDef` per domain.** Build it once at startup via a builder function; clone `ThreadSafeContext` per instance.
4. **SCXML export is additive.** Any chart can add a `to_scxml()` that walks its `ChartDef` and emits an `scxml::model::Statechart`. This is not required for the runtime to function.

### Phase 5 unblock path (issue #268)

When ufo-types PR #13 merges (adds `ElementKind::DecisionNode`, `ForkNode`, `JoinNode` and corresponding `Relation::ConditionalSuccession`):

1. Update workspace `ufo-types` rev pin to include PR #13 commit.
2. In `kr0ki-core/src/chart.rs`: extend `from_sysml` with `DecisionNode` → compound guard-branching state, `ForkNode`/`JoinNode` → parallel region boundary, `ConditionalSuccession` → guarded `on("branch.<label>", target)` transition.
3. Add SHACL shape in `ledgrrr_validate` for control-flow element topology constraints.
4. Tests: branch topology roundtrip (fork enters both branches in parallel; join waits).

### PM Hooks integration (see `docs/pm-hooks-design.md`)

`HsmChart`'s entry actions are the PM-hook surface. When `advance()` returns `true`, `TurboLedgerService` reads `last_actions()` and dispatches each string as a hook event:

- `"ingest.ready"` → emit `PmHookEvent::StageReady { stage: "ingest" }`
- `"normalize.pending"` → emit `PmHookEvent::StageBlocked { stage: "normalize" }`
- `"commit.ready"` → emit `PmHookEvent::PipelineComplete`

---

## 4. Visual Tooling

- **Design time**: statecharts.dev accepts SCXML. Export any chart's `ChartDef` to SCXML with a `to_scxml()` helper; paste into statecharts.dev for visual validation.
- **Runtime introspection**: `last_actions()` + a thin SSE emitter (already in `ledgerr-mcp`'s streamable-HTTP layer from PR #264) surfaces live state transitions to any connected client.
- **Diagram generation**: holon-viz's existing Cytoscape.js renderer can consume statechart topology as a node/edge graph — no separate diagram tool needed for dependency/issue graphs.

---

## 5. Open Work

| Item | Tracks | Effort |
|------|--------|--------|
| Phase 5 control-flow nodes | Issue #268, blocked on ufo-types PR #13 | ~1-2h once unblocked |
| `to_scxml()` helpers for `SurfaceChart` / `ProcessChart` | Optional, enhances tooling | ~1h each |
| ufo-types OODA statechart → lazily runtime (not just export) | No issue yet — additive | ~2h |
| Unified `ChartEvent` trait across all three chart types | No issue yet — reduces boilerplate | ~1h |
