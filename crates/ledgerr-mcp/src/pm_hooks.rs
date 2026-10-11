//! PM hook layer: typed events derived from statechart action strings,
//! composable handlers, and a shared TaskStatusStore.
//!
//! Wiring: `TurboLedgerService` registers a `TaskStatusPmHookHandler` on the
//! `ProjectEventBus` at construction time. Every `lifecycle_transition` event
//! fired by `hsm_transition_tool` flows through here without any direct
//! coupling between the HSM state machine and the PM layer.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::event_bus::{ProjectEvent, ProjectEventBus};

/// Typed PM hook event derived from a `lifecycle_transition` ProjectEvent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PmHookEvent {
    StageReady { stage: String },
    StageBlocked { stage: String },
    PipelineComplete,
    PipelineReset,
}

impl PmHookEvent {
    /// Derive a `PmHookEvent` from a lifecycle transition `to` token.
    /// Returns `None` for unrecognised tokens (forward-compatible).
    pub fn from_transition(from: &str, to: &str) -> Option<Self> {
        let _ = from; // reserved for future guard conditions
        let mut parts = to.splitn(2, '.');
        let stage = parts.next()?;
        let sub = parts.next()?;
        match (stage, sub) {
            ("summarize", "ready") => Some(PmHookEvent::PipelineComplete),
            (s, "ready") => Some(PmHookEvent::StageReady {
                stage: s.to_string(),
            }),
            (s, "pending") => Some(PmHookEvent::StageBlocked {
                stage: s.to_string(),
            }),
            ("ingest", "initial") => Some(PmHookEvent::PipelineReset),
            _ => None,
        }
    }
}

/// Per-stage status for the `task_status` MCP tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageStatus {
    Pending,
    Blocked,
    Ready,
    Complete,
}

impl StageStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Blocked => "blocked",
            Self::Ready => "ready",
            Self::Complete => "complete",
        }
    }
}

/// Shared, thread-safe store of per-stage statuses.
#[derive(Debug, Default, Clone)]
pub struct TaskStatusStore {
    inner: Arc<Mutex<BTreeMap<String, StageStatus>>>,
}

impl TaskStatusStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply a PM hook event, updating the relevant stage.
    pub fn apply(&self, event: &PmHookEvent) {
        let mut map = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        match event {
            PmHookEvent::StageReady { stage } => {
                map.insert(stage.clone(), StageStatus::Ready);
            }
            PmHookEvent::StageBlocked { stage } => {
                map.insert(stage.clone(), StageStatus::Blocked);
            }
            PmHookEvent::PipelineComplete => {
                map.insert("summarize".to_string(), StageStatus::Complete);
            }
            PmHookEvent::PipelineReset => {
                map.clear();
                map.insert("ingest".to_string(), StageStatus::Blocked);
            }
        }
    }

    /// Snapshot: stage → status string. Sorted for deterministic output.
    pub fn snapshot(&self) -> BTreeMap<String, String> {
        let map = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        map.iter()
            .map(|(k, v)| (k.clone(), v.as_str().to_string()))
            .collect()
    }
}

/// Handler trait — synchronous, infallible.
/// Handlers observe events; they never write back to the statechart.
pub trait PmHookHandler: Send + Sync {
    fn on_event(&self, event: &PmHookEvent);
}

/// Updates `TaskStatusStore` from every PM hook event.
pub struct TaskStatusPmHookHandler {
    pub store: TaskStatusStore,
}

impl TaskStatusPmHookHandler {
    pub fn new(store: TaskStatusStore) -> Self {
        Self { store }
    }
}

impl PmHookHandler for TaskStatusPmHookHandler {
    fn on_event(&self, event: &PmHookEvent) {
        self.store.apply(event);
    }
}

/// Logs every PM hook event via `tracing::info!`.
pub struct LoggingPmHookHandler;

impl PmHookHandler for LoggingPmHookHandler {
    fn on_event(&self, event: &PmHookEvent) {
        tracing::info!(pm_hook = ?event, "PM hook fired");
    }
}

/// Adapter: registers a `ProjectEventBus` listener that converts
/// `lifecycle_transition` events into `PmHookEvent`s and dispatches them to
/// all registered `PmHookHandler`s.
pub struct PmHookDispatcher {
    handlers: Vec<Box<dyn PmHookHandler>>,
}

impl PmHookDispatcher {
    pub fn new(handlers: Vec<Box<dyn PmHookHandler>>) -> Self {
        Self { handlers }
    }

    /// Register this dispatcher as a listener on `bus`.
    /// All `lifecycle_transition` events on the bus will be forwarded here.
    pub fn register_on_bus(self, bus: &ProjectEventBus) {
        let dispatcher = Arc::new(self);
        bus.register(Arc::new(move |event: &ProjectEvent| {
            if event.kind == "lifecycle_transition" {
                let from = event.attr("from").unwrap_or("");
                let to = event.attr("to").unwrap_or("");
                if let Some(pm_event) = PmHookEvent::from_transition(from, to) {
                    for handler in &dispatcher.handlers {
                        handler.on_event(&pm_event);
                    }
                }
            }
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_transition_stage_ready() {
        let e = PmHookEvent::from_transition("ingest.pending", "normalize.ready");
        assert_eq!(e, Some(PmHookEvent::StageReady { stage: "normalize".to_string() }));
    }

    #[test]
    fn from_transition_stage_blocked() {
        let e = PmHookEvent::from_transition("normalize.ready", "validate.pending");
        assert_eq!(e, Some(PmHookEvent::StageBlocked { stage: "validate".to_string() }));
    }

    #[test]
    fn from_transition_pipeline_complete() {
        let e = PmHookEvent::from_transition("commit.ready", "summarize.ready");
        assert_eq!(e, Some(PmHookEvent::PipelineComplete));
    }

    #[test]
    fn from_transition_unknown_substate_returns_none() {
        let e = PmHookEvent::from_transition("a.b", "validate.running");
        assert_eq!(e, None);
    }

    #[test]
    fn task_status_store_apply_stage_ready() {
        let store = TaskStatusStore::new();
        store.apply(&PmHookEvent::StageReady { stage: "ingest".to_string() });
        let snap = store.snapshot();
        assert_eq!(snap["ingest"], "ready");
    }

    #[test]
    fn task_status_store_apply_pipeline_complete() {
        let store = TaskStatusStore::new();
        store.apply(&PmHookEvent::PipelineComplete);
        let snap = store.snapshot();
        assert_eq!(snap["summarize"], "complete");
    }

    #[test]
    fn task_status_store_pipeline_reset_clears() {
        let store = TaskStatusStore::new();
        store.apply(&PmHookEvent::StageReady { stage: "ingest".to_string() });
        store.apply(&PmHookEvent::PipelineReset);
        let snap = store.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap["ingest"], "blocked");
    }

    #[test]
    fn dispatcher_fires_handler_on_lifecycle_transition_event() {
        let store = TaskStatusStore::new();
        let store_clone = store.clone();
        let handler = TaskStatusPmHookHandler::new(store_clone);
        let dispatcher = PmHookDispatcher::new(vec![Box::new(handler)]);

        let bus = ProjectEventBus::default();
        dispatcher.register_on_bus(&bus);

        bus.fire(&ProjectEvent::lifecycle_transition(
            "ingest.pending",
            "normalize.ready",
        ));

        let snap = store.snapshot();
        assert_eq!(snap["normalize"], "ready");
    }

    #[test]
    fn dispatcher_ignores_non_lifecycle_events() {
        let store = TaskStatusStore::new();
        let store_clone = store.clone();
        let handler = TaskStatusPmHookHandler::new(store_clone);
        let dispatcher = PmHookDispatcher::new(vec![Box::new(handler)]);

        let bus = ProjectEventBus::default();
        dispatcher.register_on_bus(&bus);

        bus.fire(&ProjectEvent::tool_called("ledgerr_workflow", "status", true));

        let snap = store.snapshot();
        assert!(snap.is_empty(), "non-lifecycle event must not update task status");
    }
}
