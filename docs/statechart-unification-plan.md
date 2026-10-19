# Statechart Unification Plan: b00t × kr0ki × ufo-types

**Status**: Draft — 2026-10-11  
**Closes**: Issue #259 (statechart integration, harmonization with b00t and ufo-types)

---

## 1. Current State

Three independent runtime layers that need to cooperate:

| Layer | Current impl | Gaps |
|-------|-------------|------|
| **b00t HsmMachine** | `crates/ledgerr-mcp/src/hsm.rs` — hand-rolled HSM with state/substate string pairs | No reactive cells; transitions fire strings, not typed events |
| **kr0ki process execution** | `crates/kr0ki-core` (Phase 4 — PR #265) — maps ufo-types SysML-v2 ActionUsage + Succession → `lazily::StateChart` | New; not yet connected to b00t event bus or MCP dispatch |
| **ufo-types SysML model** | `ElementKind::{ActionUsage, ActionDefinition}`, `Relation::Succession` (rev `5f8d018`) | No DecisionNode/ForkNode/JoinNode Rust types yet — these exist in SHACL TTL ontology only |

---

## 2. Target Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                    lazily::StateChart                        │
│   (Harel/SCXML conformant, reactive Source<BTreeSet<String>>,│
│    ThreadSafeStateChart, Effect hooks — lazily-rs v0.59)     │
└─────────────────────┬────────────────────────────────────────┘
                      │ drives
          ┌───────────┼─────────────┐
          ▼           ▼             ▼
   ┌──────────┐  ┌─────────┐  ┌──────────────┐
   │ b00t HSM │  │ kr0ki   │  │ ProjectEvent │
   │ (ledgerr │  │ Process │  │ Bus          │
   │  -mcp)   │  │ Chart   │  │ (PM hooks)   │
   └────┬─────┘  └────┬────┘  └──────┬───────┘
        │             │              │
        └─────────────┼──────────────┘
                      ▼
           ufo-types model layer
           (ElementKind, Relation)
```

### Roles

- **lazily::StateChart** — single reactive chart runtime for both process and HSM execution. Replaces the hand-rolled `HsmMachine` string dispatch in `hsm.rs`.
- **kr0ki-core::ProcessChart** — maps SysML-v2 process elements (ufo-types) into a `lazily::StateChart`. The translation layer: model → runtime.
- **b00t HsmMachine** — migration target: replace with a `lazily::ChartDef` built from the same Harel hierarchy, driven by `ThreadSafeStateChart::send`. The hook system (`HsmHookRegistry`) becomes `ProjectEventBus` callbacks wired to `StateChart` configuration changes.
- **ProjectEventBus** — the cross-cutting hook layer. Every state change, task event, and tool call fires here. Hooks registered via MCP tool (`ledgerr_workflow/register_project_hook`) or Rust callback.
- **ufo-types** — the model layer supplying `ElementKind` and `Relation` for both SysML-v2 process models (kr0ki) and future verifier models (#218).

---

## 3. Migration Phases

### Phase 1 (Done — PRs #263, #264)
- `lazily` dep added to workspace
- `HsmHookRegistry` wired to `lazily` (`StateBuilder`/`ChartBuilder`)
- `ProjectEventBus` on `TurboLedgerService`
- `tool_called`, `task_created`, `task_done`, `lifecycle_transition` events

### Phase 2 (Done — PR #265)
- `crates/kr0ki-core`: `ProcessChart::from_sysml(elements, relations)` → `ThreadSafeStateChart`
- Maps `ActionUsage` → atomic state, `ActionDefinition` → compound root, `Succession` → `"next"` transition

### Phase 3 (This PR — task/227-sse-streamable-http)
- `WorkflowArgs::TaskCreate / TaskList / TaskDone / TaskUpdate` — first-class task management on `ProjectEventBus`
- `task_updated` event added to bus
- Every task lifecycle fires a typed `ProjectEvent` hook

### Phase 4 — HsmMachine → lazily (Future)

Replace `HsmMachine`'s string-pair state with a `ChartDef`:

```rust
// crates/ledgerr-mcp/src/hsm.rs — MIGRATION TARGET
// Current:
pub struct HsmMachine { state: String, substate: String, … }

// Future:
pub struct HsmMachine {
    chart: ThreadSafeStateChart,
    ctx: ThreadSafeContext,
}
// States: "idle.waiting", "ingest.pending", "normalize.ready", …
// HsmHookRegistry → callbacks wired to ctx reactive source invalidation
```

Trigger: when `HsmHookRegistry` needs compound state (hierarchical guard), parallel regions, or history pseudo-states — `lazily` provides all three natively.

### Phase 5 — ufo-types extension (Future, depends on ufo-types upstream)

`DecisionNode`, `ForkNode`, `JoinNode` as `ElementKind` variants:
- `DecisionNode` → `StateBuilder::compound(id, initial).on_guarded(event, target, guard_name)`
- `ForkNode` → `StateBuilder::parallel(id)` with N children
- `JoinNode` → compound exit transition from all parallel children

Blocked on: ufo-types adding these variants (issue #218 blocked on ufo-types PR #13).

---

## 4. b00t Hook Contract

b00t task operations fire `ProjectEvent`s via MCP tool (`ledgerr_workflow`):

| b00t operation | MCP action | ProjectEvent kind |
|----------------|-----------|-------------------|
| task add | `task_create` | `task_created` |
| task list | `task_list` | `tool_called` |
| task done | `task_done` | `task_done` |
| task next / update | `task_update` | `task_updated` |
| hsm transition | `transition` | `lifecycle_transition` + `task_done` (→ summarize) |
| hsm resume | `resume` | `task_created` (checkpoint) |
| any tool call | any | `tool_called` |

Hooks register via `{"action":"register_project_hook","event_kind":"*"}` on `ledgerr_workflow`. This is the b00t hook surface — agents wire LFMF actions, cost checkpoints, and state machine gates here.

---

## 5. kr0ki ↔ b00t Integration Point

`kr0ki-core::ProcessChart` produces a `ThreadSafeStateChart`. To integrate with the b00t PM system:

```rust
// Proposed API (Phase 4):
impl ProcessChart {
    /// Attach a ProjectEventBus so every StateChart transition fires
    /// a `lifecycle_transition` event on the bus.
    pub fn with_event_bus(self, bus: Arc<ProjectEventBus>) -> Self { … }
}
```

Each `advance()` call fires `ProjectEvent::lifecycle_transition(from_leaves, to_leaves)` on the attached bus, making kr0ki process steps visible to all registered hooks.

---

## 6. Decision: statecharts.dev vs lazily

| Tool | Role |
|------|------|
| **statecharts.dev / XState** | Design tool — visual authoring, SCXML export, generates `ChartDef`-compatible JSON |
| **lazily::statechart** | Runtime — executes the chart in Rust, reactive, no JS dependency |
| **cmars/statechart (Go)** | Out of scope — wrong language |

Workflow: design in statecharts.dev → export SCXML → convert to `ChartBuilder` builder calls (or `ChartDef::from_json` via `statechart-json` feature) → execute in `lazily`.

---

## 7. Open Gaps

| Gap | Blocker | Issue |
|-----|---------|-------|
| `DecisionNode` / `ForkNode` / `JoinNode` in ufo-types | ufo-types PR #13 | #218 |
| HsmMachine full migration to lazily | Defer until Phase 4 trigger | — |
| `ProcessChart::with_event_bus` integration | Phase 4 | — |
| SCXML export from statecharts.dev → `ChartDef::from_json` pipeline | Future | — |
