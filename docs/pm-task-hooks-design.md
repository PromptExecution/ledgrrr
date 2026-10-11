# PM/Task Management Hook Architecture

**Status**: Implemented — 2026-10-11  
**Relates to**: Issue #259, `docs/statechart-unification-plan.md`

---

## Overview

Every observable event in ledgrrr-mcp fires through a single `ProjectEventBus`.
Hooks registered on the bus receive 100% of activity — not just task management
operations. This makes the bus the authoritative observability surface for agents,
monitoring, and the statechart runtime.

---

## Event Taxonomy

| Event kind | Trigger point | Source |
|------------|--------------|--------|
| `tool_called` | Every `tools/call` before dispatch | `ledgerr-mcp-server.rs` L284, `dispatch_tool()` |
| `task_created` | `WorkflowArgs::TaskCreate` arm | `mcp_adapter::handle_workflow_tool` |
| `task_done` | `WorkflowArgs::TaskDone` arm | `mcp_adapter::handle_workflow_tool` |
| `task_updated` | `WorkflowArgs::TaskUpdate` arm | `mcp_adapter::handle_workflow_tool` |
| `lifecycle_transition` | HSM state advance | `mcp_adapter::handle_workflow_tool` (transition arm) |

All events carry structured `attrs: Vec<(String, String)>` key-value pairs (see
`ProjectEvent` in `src/event_bus.rs`).

---

## Hook Registration

### In-process (Rust)

```rust
service.register_project_hook(Arc::new(|evt: &ProjectEvent| {
    match evt.kind.as_str() {
        "task_created" => { /* ... */ }
        "tool_called"  => { /* ... */ }
        _ => {}
    }
}));
```

`TurboLedgerService::register_project_hook` delegates to `ProjectEventBus::register`.
All hooks are `Arc<dyn Fn(&ProjectEvent) + Send + Sync + 'static>` — zero copy,
thread-safe.

### Via MCP tool

```json
{
  "action": "register_project_hook",
  "event_kind": "*"
}
```

`WorkflowArgs::RegisterProjectHook { event_kind }` in `contract.rs`.  Fires for
all events when `event_kind == "*"`, or filtered when a specific kind is named.
Response includes `hook_id` for deregistration.

---

## Dispatch Path (every tool call fires once)

```
Client → JSON-RPC tools/call
  └─ ledgerr-mcp-server.rs handle_request()
       ├─ fire_tool_event(tool_name, "dispatch", true)   ← fires BEFORE auth gate
       ├─ authorize_tool_call() (governance gate)
       └─ dispatch to handler
            └─ handle_workflow_tool()
                 ├─ TaskCreate  → project_event_bus.fire(task_created)
                 ├─ TaskDone    → project_event_bus.fire(task_done)
                 ├─ TaskUpdate  → project_event_bus.fire(task_updated)
                 └─ Transition  → project_event_bus.fire(lifecycle_transition)
```

The `dispatch_tool()` function (rmcp path, `mcp_adapter.rs`) mirrors the same
`fire_tool_event()` call at its entry point, ensuring the rmcp transport has
identical hook coverage.

---

## Statechart Integration

The `ProjectEventBus` is the bridge between lazily `StateChart` transitions and
the MCP tool surface:

- **HSM lifecycle** (`crates/ledgerr-mcp/src/hsm.rs`): `lifecycle_chart_def()`
  builds a `lazily::ChartDef` matching the six-stage pipeline topology. On-entry
  effects fire `lifecycle_transition` events on the bus.
- **kr0ki process execution** (`crates/kr0ki-core`): `ProcessChart::advance()`
  fires `lifecycle_transition` for each active leaf change (Phase 4, PR #265).
- **b00t-iface `SurfaceMachine`** (`crates/b00t-iface/src/core/machine.rs`):
  topology exportable as SCXML (`machine_scxml()`, `#[cfg(feature = "statechart")]`).
  Full migration to `lazily::ThreadSafeStateChart` is the b00t Phase 4 item.

---

## Test Coverage

| Test | Location | What it proves |
|------|----------|----------------|
| `hook_fires_for_all_events` | `event_bus.rs` | Hook receives all event kinds |
| `hook_fires_for_every_tool_family` | `event_bus.rs` | All 9 tool families trigger `tool_called` |
| `multiple_hooks_fire_in_order` | `event_bus.rs` | Registration order preserved |
| `task_created_event_has_correct_attrs` | `event_bus.rs` | `task_id`, `description` attrs present |
| `task_done_event_has_correct_attrs` | `event_bus.rs` | `task_id` attr present |
| `task_updated_event_has_correct_attrs` | `event_bus.rs` | `task_id`, `status`, `note` attrs present |
| `task_updated_event_no_note` | `event_bus.rs` | `note` attr absent when not provided |
| `tool_called_event_has_correct_attrs` | `event_bus.rs` | `tool`, `action`, `success` attrs present |

Run: `cargo test -p ledgerr-mcp --lib -- event_bus`  
Result: **11 passed; 0 failed**

---

## Invariants

1. Every `tools/call` dispatch fires exactly one `tool_called` event — before the
   governance gate, so authorization decisions are also observable.
2. Task operations (`TaskCreate`/`TaskDone`/`TaskUpdate`) fire exactly one typed
   event each — no double-fire with the `tool_called` wrapper.
3. Hooks never block dispatch — `ProjectEventBus::fire()` calls hooks synchronously
   but panics on lock poison, not on hook errors.
4. Hook registration is additive and ordered — hooks fire in registration order, all
   of them, for every event.
