# PM Hooks Design

*Project-management / task capability operating on statechart hooks*

---

## 1. Problem Statement

The tax-pipeline has a well-defined lifecycle (Ingest → Normalize → Validate → Reconcile → Commit → Summarize). Tasks and project-management state — "what stage is work at, what's blocked, what needs human review" — must reflect that lifecycle in real time, not be maintained separately by a human or agent writing ad-hoc status updates.

The PM capability must be **hook-driven from the statechart**, not polled or manually updated.

---

## 2. Architecture

```
TurboLedgerService
  └── hsm_chart: Mutex<Option<HsmChart>>        ← lazily::statechart executor
       │
       │  on advance() / fast_forward_to() / resume_from_marker()
       ↓
  last_actions() → Vec<String>                  ← lazily entry-action tokens
       │
       ↓
  PmHookDispatcher::dispatch(actions)
       ├── PmHookEvent::StageReady { stage }    → update task status to "ready"
       ├── PmHookEvent::StageBlocked { stage }  → flag task as "needs review"
       ├── PmHookEvent::PipelineComplete        → close active PM cycle
       └── PmHookEvent::PipelineReset           → reopen PM cycle
```

The `PmHookDispatcher` is a thin mapping layer — it translates lazily action strings to typed `PmHookEvent`s and calls registered handlers. Handlers are registered at startup; they are synchronous and infallible (they log failures, never panic).

---

## 3. Hook Events

### 3a. Action-string to event mapping

| lazily entry-action string | PmHookEvent | Meaning |
|---------------------------|-------------|---------|
| `"ingest.pending"` | `StageBlocked { stage: "ingest" }` | Pipeline started, waiting on PDF input |
| `"ingest.ready"` | `StageReady { stage: "ingest" }` | Ingest complete, next stage unblocked |
| `"normalize.pending"` | `StageBlocked { stage: "normalize" }` | Normalization not yet started |
| `"normalize.ready"` | `StageReady { stage: "normalize" }` | Normalization done |
| `"validate.pending"` | `StageBlocked { stage: "validate" }` | Validation pending |
| `"validate.ready"` | `StageReady { stage: "validate" }` | Validation clean |
| `"reconcile.pending"` | `StageBlocked { stage: "reconcile" }` | Reconciliation needed (human review) |
| `"reconcile.ready"` | `StageReady { stage: "reconcile" }` | Reconciliation approved |
| `"commit.pending"` | `StageBlocked { stage: "commit" }` | Awaiting commit authorization |
| `"commit.ready"` | `StageReady { stage: "commit" }` | Committed to Excel |
| `"summarize.pending"` | `StageBlocked { stage: "summarize" }` | Summary not yet generated |
| `"summarize.ready"` | `PipelineComplete` | Full pipeline cycle complete |
| `"reset"` | `PipelineReset` | Pipeline rewound to Ingest |

### 3b. PmHookEvent enum (Rust)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PmHookEvent {
    StageReady { stage: String },
    StageBlocked { stage: String },
    PipelineComplete,
    PipelineReset,
}
```

---

## 4. Handler Contract

Handlers implement:

```rust
pub trait PmHookHandler: Send + Sync {
    fn on_event(&self, event: &PmHookEvent);
}
```

Three built-in handlers:

| Handler | Effect |
|---------|--------|
| `LoggingPmHookHandler` | Emits a structured `tracing::info!` span per event |
| `SsePmHookHandler` | Pushes the event as a JSON SSE frame to all connected clients (uses ledgerr-mcp's SSE channel) |
| `TaskStatusPmHookHandler` | Updates an in-memory `TaskStatusStore` (stage → Ready/Blocked/Complete) queryable via the `task_status` MCP tool |

Handlers are composable — `TurboLedgerService` holds a `Vec<Box<dyn PmHookHandler>>`.

---

## 5. Integration Points

### 5a. TurboLedgerService construction

```rust
let service = TurboLedgerService::builder()
    .pm_hook(Box::new(LoggingPmHookHandler))
    .pm_hook(Box::new(SsePmHookHandler::new(sse_tx.clone())))
    .pm_hook(Box::new(TaskStatusPmHookHandler::new()))
    .build()?;
```

### 5b. Statechart advance loop

Every call to `advance()`, `fast_forward_to()`, or `resume_from_marker()` follows this pattern:

```rust
let taken = self.hsm_chart.lock().unwrap()
    .as_mut()
    .map(|c| c.advance(target_state, target_substate))
    .unwrap_or(false);

if taken {
    let actions = self.hsm_chart.lock().unwrap()
        .as_ref()
        .map(|c| c.last_actions())
        .unwrap_or_default();
    self.pm_hook_dispatcher.dispatch(&actions);
}
```

### 5c. MCP tool: `task_status`

Returns the current per-stage status from `TaskStatusStore`:

```json
{
  "ingest":     "complete",
  "normalize":  "complete",
  "validate":   "blocked",
  "reconcile":  "pending",
  "commit":     "pending",
  "summarize":  "pending"
}
```

This tool is a read-only projection of hook-driven state — it is never written directly.

---

## 6. SSE PM Events

The streamable-HTTP layer (PR #264) emits PM hook events as SSE frames:

```
event: pm_hook
data: {"type":"StageReady","stage":"normalize","ts":"2026-10-11T12:00:00Z"}

event: pm_hook
data: {"type":"PipelineComplete","ts":"2026-10-11T14:22:00Z"}
```

Clients (Tauri desktop agent, MCP consumers) subscribe to `GET /events` and react to `pm_hook` frames to update UI status, trigger notifications, or advance dependent workflows.

---

## 7. Design Invariants

1. **PM state is derived, never authoritative.** The statechart is authoritative; `TaskStatusStore` and SSE frames are projections.
2. **Hooks are synchronous and infallible.** Handler errors are logged, never propagated to the pipeline.
3. **No handler writes back to the statechart.** Handlers observe; they never call `advance()` themselves.
4. **Action strings are the only coupling.** `HsmChart` and `PmHookDispatcher` share only the action string vocabulary — no shared types.

---

## 8. Open Work

| Item | Effort |
|------|--------|
| `PmHookEvent` enum + `PmHookHandler` trait (add to `ledgerr-mcp`) | ~1h |
| `TaskStatusPmHookHandler` + `task_status` MCP tool | ~1h |
| Wire handlers into `TurboLedgerService::new()` (already has `hsm_chart`) | ~30m |
| SSE frame shape finalization (type discriminant key) | ~15m |
| Tests: advance pipeline → hook fires → TaskStatusStore updated | ~1h |
