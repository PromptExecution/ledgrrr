/// General project-management event bus for ledgrrr (issue #259 / PM hooks).
///
/// `HsmHookRegistry` covers HSM lifecycle state transitions. This module
/// adds a broader event bus that fires on any named `ProjectEvent`:
/// - Tool calls (every `tools/call` dispatch)
/// - Task lifecycle (create/done/blocked — forwarded from b00t task commands)
/// - Lifecycle transitions (mirrored from HsmHookRegistry for cross-cutting hooks)
///
/// The bus is held on `TurboLedgerService` as `Arc<ProjectEventBus>` and is
/// the single place to register hooks that span multiple event types.
use std::sync::{Arc, Mutex};

/// Payload for a project-level event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectEvent {
    /// Short machine-readable label, e.g. `"tool_called"`, `"task_created"`,
    /// `"lifecycle_transition"`.
    pub kind: String,
    /// Arbitrary key=value context (tool name, task ID, lifecycle state, …).
    pub attrs: Vec<(String, String)>,
}

impl ProjectEvent {
    /// Convenience: build a `tool_called` event.
    pub fn tool_called(tool: &str, action: &str, success: bool) -> Self {
        Self {
            kind: "tool_called".to_string(),
            attrs: vec![
                ("tool".to_string(), tool.to_string()),
                ("action".to_string(), action.to_string()),
                ("success".to_string(), success.to_string()),
            ],
        }
    }

    /// Convenience: build a `task_created` event.
    pub fn task_created(task_id: &str, description: &str) -> Self {
        Self {
            kind: "task_created".to_string(),
            attrs: vec![
                ("task_id".to_string(), task_id.to_string()),
                ("description".to_string(), description.to_string()),
            ],
        }
    }

    /// Convenience: build a `task_done` event.
    pub fn task_done(task_id: &str) -> Self {
        Self {
            kind: "task_done".to_string(),
            attrs: vec![("task_id".to_string(), task_id.to_string())],
        }
    }

    /// Convenience: build a `task_updated` event.
    pub fn task_updated(task_id: &str, status: &str, note: Option<&str>) -> Self {
        let mut attrs = vec![
            ("task_id".to_string(), task_id.to_string()),
            ("status".to_string(), status.to_string()),
        ];
        if let Some(n) = note {
            attrs.push(("note".to_string(), n.to_string()));
        }
        Self { kind: "task_updated".to_string(), attrs }
    }

    /// Convenience: build a `lifecycle_transition` event.
    pub fn lifecycle_transition(from: &str, to: &str) -> Self {
        Self {
            kind: "lifecycle_transition".to_string(),
            attrs: vec![
                ("from".to_string(), from.to_string()),
                ("to".to_string(), to.to_string()),
            ],
        }
    }

    /// Look up an attribute value by key.
    pub fn attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

/// A registered project-event hook callback.
pub type ProjectHookFn = Arc<dyn Fn(&ProjectEvent) + Send + Sync + 'static>;

/// General-purpose event bus: register hooks that fire on any `ProjectEvent`.
///
/// Hooks fire synchronously in registration order. Each hook sees every event
/// regardless of kind; use `event.kind` to filter inside the hook.
///
/// Use `ProjectEventBus::fire_if` to fire only when `event.kind` matches a
/// specific value — the standard pattern for targeted hooks.
#[derive(Default)]
pub struct ProjectEventBus {
    hooks: Mutex<Vec<ProjectHookFn>>,
}

impl ProjectEventBus {
    /// Register a hook that fires on every event.
    pub fn register(&self, hook: ProjectHookFn) {
        self.hooks
            .lock()
            .expect("ProjectEventBus lock poisoned")
            .push(hook);
    }

    /// Fire all registered hooks synchronously.
    pub fn fire(&self, event: &ProjectEvent) {
        for hook in self
            .hooks
            .lock()
            .expect("ProjectEventBus lock poisoned")
            .iter()
        {
            hook(event);
        }
    }

    /// Fire all hooks, but only if `event.kind == kind`.
    pub fn fire_if(&self, kind: &str, event: &ProjectEvent) {
        if event.kind == kind {
            self.fire(event);
        }
    }

    pub fn len(&self) -> usize {
        self.hooks
            .lock()
            .expect("ProjectEventBus lock poisoned")
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// MCP-facing request to register a project-event hook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterProjectHookRequest {
    /// The event kind to subscribe to: `"tool_called"`, `"task_created"`,
    /// `"task_done"`, `"lifecycle_transition"`, or `"*"` for all events.
    pub event_kind: String,
    /// Optional tag stored in the response for caller correlation.
    pub tag: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterProjectHookResponse {
    pub registered: bool,
    pub event_kind: String,
    pub tag: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn bus_starts_empty() {
        let bus = ProjectEventBus::default();
        assert!(bus.is_empty());
    }

    #[test]
    fn hook_fires_for_all_events() {
        let bus = ProjectEventBus::default();
        let log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let log_clone = Arc::clone(&log);
        bus.register(Arc::new(move |evt: &ProjectEvent| {
            log_clone.lock().unwrap().push(evt.kind.clone());
        }));

        bus.fire(&ProjectEvent::tool_called("ledgerr_workflow", "status", true));
        bus.fire(&ProjectEvent::task_done("task-42"));

        let log = log.lock().unwrap();
        assert_eq!(log.as_slice(), &["tool_called", "task_done"]);
    }

    #[test]
    fn fire_if_filters_by_kind() {
        let bus = ProjectEventBus::default();
        let count: Arc<Mutex<u32>> = Arc::new(Mutex::new(0));
        let count_clone = Arc::clone(&count);
        bus.register(Arc::new(move |evt: &ProjectEvent| {
            if evt.kind == "task_done" {
                *count_clone.lock().unwrap() += 1;
            }
        }));

        bus.fire_if("tool_called", &ProjectEvent::tool_called("x", "y", true));
        assert_eq!(*count.lock().unwrap(), 0, "tool_called must not trigger task_done hook");

        bus.fire_if("task_done", &ProjectEvent::task_done("t1"));
        assert_eq!(*count.lock().unwrap(), 1);
    }

    #[test]
    fn project_event_attr_lookup() {
        let evt = ProjectEvent::lifecycle_transition("ingest.pending", "normalize.ready");
        assert_eq!(evt.attr("from"), Some("ingest.pending"));
        assert_eq!(evt.attr("to"), Some("normalize.ready"));
        assert_eq!(evt.attr("missing"), None);
    }

    #[test]
    fn multiple_hooks_fire_in_order() {
        let bus = ProjectEventBus::default();
        let order: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        for i in 0u8..3 {
            let o = Arc::clone(&order);
            bus.register(Arc::new(move |_| { o.lock().unwrap().push(i); }));
        }
        bus.fire(&ProjectEvent::task_created("t1", "test task"));
        assert_eq!(*order.lock().unwrap(), vec![0, 1, 2]);
    }

    #[test]
    fn task_created_event_has_correct_attrs() {
        let evt = ProjectEvent::task_created("task-007", "classify ingested PDFs");
        assert_eq!(evt.kind, "task_created");
        assert_eq!(evt.attr("task_id"), Some("task-007"));
        assert_eq!(evt.attr("description"), Some("classify ingested PDFs"));
    }

    #[test]
    fn task_done_event_has_correct_attrs() {
        let evt = ProjectEvent::task_done("task-007");
        assert_eq!(evt.kind, "task_done");
        assert_eq!(evt.attr("task_id"), Some("task-007"));
    }

    #[test]
    fn task_updated_event_has_correct_attrs() {
        let evt = ProjectEvent::task_updated("task-007", "in_progress", Some("unblocked by PR merge"));
        assert_eq!(evt.kind, "task_updated");
        assert_eq!(evt.attr("task_id"), Some("task-007"));
        assert_eq!(evt.attr("status"), Some("in_progress"));
        assert_eq!(evt.attr("note"), Some("unblocked by PR merge"));
    }

    #[test]
    fn task_updated_event_no_note() {
        let evt = ProjectEvent::task_updated("task-42", "blocked", None);
        assert_eq!(evt.attr("note"), None);
    }

    #[test]
    fn tool_called_event_has_correct_attrs() {
        let evt = ProjectEvent::tool_called("ledgerr_workflow", "transition", true);
        assert_eq!(evt.kind, "tool_called");
        assert_eq!(evt.attr("tool"), Some("ledgerr_workflow"));
        assert_eq!(evt.attr("action"), Some("transition"));
        assert_eq!(evt.attr("success"), Some("true"));
    }
}
