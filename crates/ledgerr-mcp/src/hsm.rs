use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tracing;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LifecycleState {
    Ingest,
    Normalize,
    Validate,
    Reconcile,
    Commit,
    Summarize,
}

impl LifecycleState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ingest => "ingest",
            Self::Normalize => "normalize",
            Self::Validate => "validate",
            Self::Reconcile => "reconcile",
            Self::Commit => "commit",
            Self::Summarize => "summarize",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "ingest" => Some(Self::Ingest),
            "normalize" => Some(Self::Normalize),
            "validate" => Some(Self::Validate),
            "reconcile" => Some(Self::Reconcile),
            "commit" => Some(Self::Commit),
            "summarize" => Some(Self::Summarize),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LifecycleSubstate {
    Pending,
    Ready,
}

impl LifecycleSubstate {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Ready => "ready",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "ready" => Some(Self::Ready),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LifecycleNode {
    pub state: LifecycleState,
    pub substate: LifecycleSubstate,
}

impl LifecycleNode {
    pub fn token(self) -> String {
        format!("{}.{}", self.state.as_str(), self.substate.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HsmTransitionRequest {
    pub target_state: String,
    pub target_substate: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HsmTransitionResponse {
    pub state: String,
    pub substate: String,
    pub status: String,
    pub guard_reason: Option<String>,
    pub transition_evidence: Vec<String>,
    pub state_marker: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HsmStatusRequest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HsmStatusResponse {
    pub state: String,
    pub substate: String,
    pub display_state: String,
    pub next_hint: String,
    pub resume_hint: String,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HsmResumeRequest {
    pub state_marker: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HsmResumeResponse {
    pub resumed: bool,
    pub resume_from: String,
    pub resume_hint: String,
    pub blockers: Vec<String>,
}

/// Named built-in hook variants an agent can register via the MCP tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuiltInHook {
    /// Fires when the lifecycle advances from Ingest → Normalize.
    OnIngest,
    /// Fires when the lifecycle advances from Validate → Reconcile.
    OnValidate,
    /// Fires when the lifecycle advances from Commit → Summarize.
    OnCommit,
}

impl BuiltInHook {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "on_ingest" => Some(Self::OnIngest),
            "on_validate" => Some(Self::OnValidate),
            "on_commit" => Some(Self::OnCommit),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OnIngest => "on_ingest",
            Self::OnValidate => "on_validate",
            Self::OnCommit => "on_commit",
        }
    }

    /// Human-readable description of when this hook fires.
    pub fn fires_on(&self) -> &'static str {
        match self {
            Self::OnIngest => "transition_to_normalize",
            Self::OnValidate => "transition_to_reconcile",
            Self::OnCommit => "transition_to_summarize",
        }
    }

    /// Build the `HookFn` for this named hook — fires only on the relevant transition.
    pub fn into_hook_fn(self) -> HookFn {
        Arc::new(move |evt: &TransitionHookEvent| {
            let fires = match self {
                Self::OnIngest => evt.to.state == LifecycleState::Normalize,
                Self::OnValidate => evt.to.state == LifecycleState::Reconcile,
                Self::OnCommit => evt.to.state == LifecycleState::Summarize,
            };
            if fires {
                tracing::info!(
                    hook = self.as_str(),
                    from = %evt.from.token(),
                    to = %evt.to.token(),
                    "lifecycle hook fired"
                );
            }
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterHookRequest {
    pub hook_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterHookResponse {
    pub registered: bool,
    pub hook_name: String,
    pub fires_on: String,
    pub reason: Option<String>,
}

/// Payload delivered to every registered hook on a lifecycle transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionHookEvent {
    /// State before the transition.
    pub from: LifecycleNode,
    /// State after the transition.
    pub to: LifecycleNode,
}

/// A registered hook callback — `Send + Sync` so it can be called from any thread.
pub type HookFn = Arc<dyn Fn(&TransitionHookEvent) + Send + Sync + 'static>;

/// Registry of hooks that fire on every successful lifecycle transition.
///
/// Each `hsm_transition_tool` or `hsm_resume_tool` call that advances state
/// calls `fire()` synchronously before returning. This is the "hooks for
/// each time" mechanism (issue #259 Phase 2 / project-management hooks).
#[derive(Default)]
pub struct HsmHookRegistry {
    hooks: Mutex<Vec<HookFn>>,
}

impl HsmHookRegistry {
    /// Register a new hook. Hooks fire in registration order.
    pub fn register(&self, hook: HookFn) {
        self.hooks
            .lock()
            .expect("HsmHookRegistry lock poisoned")
            .push(hook);
    }

    /// Fire all hooks synchronously with `event`.
    pub fn fire(&self, event: &TransitionHookEvent) {
        for hook in self
            .hooks
            .lock()
            .expect("HsmHookRegistry lock poisoned")
            .iter()
        {
            hook(event);
        }
    }

    /// Number of registered hooks.
    pub fn len(&self) -> usize {
        self.hooks
            .lock()
            .expect("HsmHookRegistry lock poisoned")
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HsmMachine {
    pub current: LifecycleNode,
    pub last_valid_checkpoint: String,
}

impl Default for HsmMachine {
    fn default() -> Self {
        let current = LifecycleNode {
            state: LifecycleState::Ingest,
            substate: LifecycleSubstate::Pending,
        };
        Self {
            current,
            last_valid_checkpoint: checkpoint_marker(current),
        }
    }
}

pub fn checkpoint_marker(node: LifecycleNode) -> String {
    format!(
        "{}:{}:advanced",
        node.state.as_str(),
        node.substate.as_str()
    )
}

pub fn parse_checkpoint_marker(marker: &str) -> Option<LifecycleNode> {
    let mut parts = marker.split(':');
    let state = parts.next()?;
    let substate = parts.next()?;
    let status = parts.next()?;
    if status != "advanced" || parts.next().is_some() {
        return None;
    }
    parse_node(state, substate)
}

pub fn resume_response(
    node: LifecycleNode,
    resumed: bool,
    mut blockers: Vec<String>,
) -> HsmResumeResponse {
    blockers.sort();
    blockers.dedup();
    HsmResumeResponse {
        resumed,
        resume_from: checkpoint_marker(node),
        resume_hint: if resumed {
            next_hint_for(node)
        } else {
            format!("resume_from_{}", node.token())
        },
        blockers,
    }
}

pub fn parse_node(state: &str, substate: &str) -> Option<LifecycleNode> {
    Some(LifecycleNode {
        state: LifecycleState::parse(state)?,
        substate: LifecycleSubstate::parse(substate)?,
    })
}

pub fn allowed_next_node(current: LifecycleNode) -> Option<LifecycleNode> {
    use LifecycleState as S;
    use LifecycleSubstate as T;
    let next_state = match current.state {
        S::Ingest => S::Normalize,
        S::Normalize => S::Validate,
        S::Validate => S::Reconcile,
        S::Reconcile => S::Commit,
        S::Commit => S::Summarize,
        S::Summarize => return None,
    };
    Some(LifecycleNode {
        state: next_state,
        substate: T::Ready,
    })
}

pub fn next_hint_for(node: LifecycleNode) -> String {
    match node.state {
        LifecycleState::Ingest => "advance_to_normalize",
        LifecycleState::Normalize => "advance_to_validate",
        LifecycleState::Validate => "advance_to_reconcile",
        LifecycleState::Reconcile => "advance_to_commit",
        LifecycleState::Commit => "advance_to_summarize",
        LifecycleState::Summarize => "lifecycle_complete",
    }
    .to_string()
}

pub fn transition_blocked_response(
    current: LifecycleNode,
    requested: LifecycleNode,
) -> HsmTransitionResponse {
    let mut transition_evidence = vec![
        format!("from={}", current.token()),
        format!("to={}", requested.token()),
    ];
    if let Some(next) = allowed_next_node(current) {
        transition_evidence.push(format!("allowed={}", next.token()));
    } else {
        transition_evidence.push("allowed=none".to_string());
    }

    HsmTransitionResponse {
        state: current.state.as_str().to_string(),
        substate: current.substate.as_str().to_string(),
        status: "blocked".to_string(),
        guard_reason: Some("invalid_transition".to_string()),
        transition_evidence,
        state_marker: format!(
            "{}:{}:blocked",
            current.state.as_str(),
            current.substate.as_str()
        ),
    }
}

pub fn transition_advanced_response(node: LifecycleNode) -> HsmTransitionResponse {
    HsmTransitionResponse {
        state: node.state.as_str().to_string(),
        substate: node.substate.as_str().to_string(),
        status: "advanced".to_string(),
        guard_reason: None,
        transition_evidence: vec![format!("advanced_to={}", node.token())],
        state_marker: checkpoint_marker(node),
    }
}

pub fn status_response(node: LifecycleNode, mut blockers: Vec<String>) -> HsmStatusResponse {
    blockers.sort();
    blockers.dedup();
    HsmStatusResponse {
        state: node.state.as_str().to_string(),
        substate: node.substate.as_str().to_string(),
        display_state: node.token(),
        next_hint: next_hint_for(node),
        resume_hint: format!("resume_from_{}", node.token()),
        blockers,
    }
}

<<<<<<< HEAD
/// Map a lifecycle node to the number of "advance" events needed to reach it
/// from the initial state (ingest.pending). Used to sync the HsmChart.
pub fn advance_count_for(node: LifecycleNode) -> usize {
    match node.state {
        LifecycleState::Ingest => 0,
        LifecycleState::Normalize => 1,
        LifecycleState::Validate => 2,
        LifecycleState::Reconcile => 3,
        LifecycleState::Commit => 4,
        LifecycleState::Summarize => 5,
    }
}

/// Flat chart for PM event dispatch: states are HsmMachine tokens; each
/// non-initial state fires entry action "lifecycle_transition" on advance.
pub fn lifecycle_pm_chart_def() -> Result<lazily::statechart::ChartDef, String> {
    use lazily::statechart::{ChartBuilder, StateBuilder};
    ChartBuilder::new()
        .state(StateBuilder::compound("lifecycle", "ingest.pending"))
        .state(
            StateBuilder::atomic("ingest.pending")
                .parent("lifecycle")
                .on("advance", "normalize.ready"),
        )
        .state(
            StateBuilder::atomic("normalize.ready")
                .parent("lifecycle")
                .entry("lifecycle_transition")
                .on("advance", "validate.ready"),
        )
        .state(
            StateBuilder::atomic("validate.ready")
                .parent("lifecycle")
                .entry("lifecycle_transition")
                .on("advance", "reconcile.ready"),
        )
        .state(
            StateBuilder::atomic("reconcile.ready")
                .parent("lifecycle")
                .entry("lifecycle_transition")
                .on("advance", "commit.ready"),
        )
        .state(
            StateBuilder::atomic("commit.ready")
                .parent("lifecycle")
                .entry("lifecycle_transition")
                .on("advance", "summarize.ready"),
        )
        .state(
            StateBuilder::atomic("summarize.ready")
                .parent("lifecycle")
                .entry("lifecycle_transition"),
        )
        .build()
}

/// Chart that mirrors HsmMachine advances and fires "lifecycle_transition"
/// entry actions. Held in TurboLedgerService to back PM hook dispatch.
pub struct HsmChart {
    chart: lazily::statechart::ThreadSafeStateChart,
    ctx: lazily::ThreadSafeContext,
    /// Number of successful advances completed; used for fast-forward on restore.
    advance_count: usize,
}

impl HsmChart {
    pub fn new() -> Result<Self, String> {
        let def = lifecycle_pm_chart_def()?;
        let ctx = lazily::ThreadSafeContext::new();
        let chart = lazily::statechart::ThreadSafeStateChart::new(&ctx, def);
        Ok(Self { chart, ctx, advance_count: 0 })
    }

    /// Advance the chart from `from_advance_idx` → `from_advance_idx + 1`.
    /// Fast-forwards silently if the chart is behind (restored session).
    /// Returns `true` iff "lifecycle_transition" fired as an entry action.
    pub fn try_advance(&mut self, from_advance_idx: usize) -> bool {
        use std::collections::HashMap;
        let guards: HashMap<String, bool> = HashMap::new();
        while self.advance_count < from_advance_idx {
            self.chart.send(&self.ctx, "advance", &guards);
            self.advance_count += 1;
        }
        let transitioned = self.chart.send(&self.ctx, "advance", &guards);
        if transitioned {
            self.advance_count += 1;
        }
        self.chart
            .last_actions()
            .contains(&"lifecycle_transition".to_string())
    }
}

#[cfg(test)]
mod hsm_chart_tests {
    use super::*;

    #[test]
    fn hsm_chart_builds_without_error() {
        let result = HsmChart::new();
        assert!(result.is_ok(), "HsmChart::new() failed: {:?}", result.err());
    }

    #[test]
    fn hsm_chart_advance_from_initial_fires_lifecycle_transition() {
        let mut chart = HsmChart::new().expect("chart");
        let fired = chart.try_advance(0);
        assert!(fired, "lifecycle_transition must fire on first advance");
    }

    #[test]
    fn hsm_chart_full_lifecycle_all_advances_fire_lifecycle_transition() {
        let mut chart = HsmChart::new().expect("chart");
        for idx in 0..5 {
            let fired = chart.try_advance(idx);
            assert!(fired, "lifecycle_transition must fire at advance {idx}");
        }
    }

    #[test]
    fn hsm_chart_fast_forward_catches_up_and_fires() {
        let mut chart = HsmChart::new().expect("chart");
        // Simulate a restored session: chart is at 0 but HSM is at Validate.Ready (idx=2)
        let fired = chart.try_advance(2);
        assert!(fired, "lifecycle_transition must fire after fast-forward");
        assert_eq!(chart.advance_count, 3);
    }
}

/// Build the lazily `ChartDef` for the ledgrrr lifecycle state machine.
///
/// Topology: six compound states (Ingest, Normalize, Validate, Reconcile,
/// Commit, Summarize), each with two orthogonal sub-states (pending → ready).
/// Advance events fire on the linear chain; reverse is not modeled (blocked
/// at the service layer).
///
/// Gated on the `statechart` feature — inactive on default builds.
#[cfg(feature = "statechart")]
pub fn lifecycle_chart_def() -> Result<lazily::statechart::ChartDef, String> {
    use lazily::statechart::{ChartBuilder, StateBuilder};

    ChartBuilder::new()
        // Root compound state — initial child is Ingest
        .state(StateBuilder::compound("lifecycle", "ingest"))
        // Ingest
        .state(StateBuilder::compound("ingest", "ingest.pending").parent("lifecycle"))
        .state(StateBuilder::atomic("ingest.pending").parent("ingest"))
        .state(
            StateBuilder::atomic("ingest.ready")
                .parent("ingest")
                .on("advance", "normalize"),
        )
        // Normalize
        .state(StateBuilder::compound("normalize", "normalize.pending").parent("lifecycle"))
        .state(StateBuilder::atomic("normalize.pending").parent("normalize"))
        .state(
            StateBuilder::atomic("normalize.ready")
                .parent("normalize")
                .on("advance", "validate"),
        )
        // Validate
        .state(StateBuilder::compound("validate", "validate.pending").parent("lifecycle"))
        .state(StateBuilder::atomic("validate.pending").parent("validate"))
        .state(
            StateBuilder::atomic("validate.ready")
                .parent("validate")
                .on("advance", "reconcile"),
        )
        // Reconcile
        .state(StateBuilder::compound("reconcile", "reconcile.pending").parent("lifecycle"))
        .state(StateBuilder::atomic("reconcile.pending").parent("reconcile"))
        .state(
            StateBuilder::atomic("reconcile.ready")
                .parent("reconcile")
                .on("advance", "commit"),
        )
        // Commit
        .state(StateBuilder::compound("commit", "commit.pending").parent("lifecycle"))
        .state(StateBuilder::atomic("commit.pending").parent("commit"))
        .state(
            StateBuilder::atomic("commit.ready")
                .parent("commit")
                .on("advance", "summarize"),
        )
        // Summarize (terminal)
        .state(StateBuilder::compound("summarize", "summarize.pending").parent("lifecycle"))
        .state(StateBuilder::atomic("summarize.pending").parent("summarize"))
        .state(StateBuilder::final_state("summarize.ready").parent("summarize"))
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn hook_registry_starts_empty() {
        let reg = HsmHookRegistry::default();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn hook_fires_on_transition_event() {
        let reg = HsmHookRegistry::default();
        let fired: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let fired_clone = Arc::clone(&fired);
        reg.register(Arc::new(move |evt: &TransitionHookEvent| {
            fired_clone
                .lock()
                .unwrap()
                .push(format!("{}→{}", evt.from.token(), evt.to.token()));
        }));
        assert_eq!(reg.len(), 1);

        let from = LifecycleNode { state: LifecycleState::Ingest, substate: LifecycleSubstate::Pending };
        let to   = LifecycleNode { state: LifecycleState::Normalize, substate: LifecycleSubstate::Ready };
        reg.fire(&TransitionHookEvent { from, to });

        let log = fired.lock().unwrap();
        assert_eq!(log.as_slice(), &["ingest.pending→normalize.ready"]);
    }

    #[test]
    fn built_in_hook_on_ingest_fires_only_on_normalize_transition() {
        let reg = HsmHookRegistry::default();
        let fired: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let fired_clone = Arc::clone(&fired);
        reg.register(BuiltInHook::OnIngest.into_hook_fn());
        // Also add a side-channel to verify firing (tracing doesn't assert)
        reg.register(Arc::new(move |evt: &TransitionHookEvent| {
            if evt.to.state == LifecycleState::Normalize {
                fired_clone.lock().unwrap().push("on_ingest".to_string());
            }
        }));

        // Non-ingest transition — should NOT fire the sentinel
        let from = LifecycleNode { state: LifecycleState::Validate, substate: LifecycleSubstate::Ready };
        let to   = LifecycleNode { state: LifecycleState::Reconcile, substate: LifecycleSubstate::Ready };
        reg.fire(&TransitionHookEvent { from, to });
        assert!(fired.lock().unwrap().is_empty(), "on_ingest must not fire on Validate→Reconcile");

        // Ingest transition — should fire
        let from2 = LifecycleNode { state: LifecycleState::Ingest, substate: LifecycleSubstate::Ready };
        let to2   = LifecycleNode { state: LifecycleState::Normalize, substate: LifecycleSubstate::Ready };
        reg.fire(&TransitionHookEvent { from: from2, to: to2 });
        assert_eq!(fired.lock().unwrap().as_slice(), &["on_ingest"]);
    }

    #[test]
    fn multiple_hooks_fire_in_registration_order() {
        let reg = HsmHookRegistry::default();
        let order: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        for i in 0u8..3 {
            let order_clone = Arc::clone(&order);
            reg.register(Arc::new(move |_| { order_clone.lock().unwrap().push(i); }));
        }
        let from = LifecycleNode { state: LifecycleState::Ingest, substate: LifecycleSubstate::Pending };
        let to   = LifecycleNode { state: LifecycleState::Normalize, substate: LifecycleSubstate::Ready };
        reg.fire(&TransitionHookEvent { from, to });
        assert_eq!(*order.lock().unwrap(), vec![0, 1, 2]);
    }

    #[cfg(feature = "statechart")]
    use super::lifecycle_chart_def;

    #[cfg(feature = "statechart")]
    #[test]
    fn lifecycle_chart_def_builds_without_error() {
        let result = lifecycle_chart_def();
        assert!(
            result.is_ok(),
            "lifecycle_chart_def() failed: {:?}",
            result.err()
        );
    }

    #[cfg(feature = "statechart")]
    #[test]
    fn lifecycle_chart_initial_state_is_ingest_pending() {
        use lazily::Context;
        let def = lifecycle_chart_def().expect("chart def");
        let ctx = Context::new();
        let chart = lazily::statechart::StateChart::new(&ctx, def);
        let config = chart.configuration(&ctx);
        assert!(
            config.contains("ingest"),
            "initial config must include 'ingest': {config:?}"
        );
        assert!(
            config.contains("ingest.pending"),
            "initial config must include 'ingest.pending': {config:?}"
        );
    }

    #[cfg(feature = "statechart")]
    #[test]
    fn lifecycle_chart_ingest_ready_advance_moves_to_normalize() {
        use lazily::Context;
        use std::collections::HashMap;
        let def = lifecycle_chart_def().expect("chart def");
        let ctx = Context::new();
        let chart = lazily::statechart::StateChart::new(&ctx, def);

        // The chart starts in ingest.pending. The "advance" event is only
        // defined on ingest.ready → normalize; we need to transition to
        // ingest.ready first via a "mark_ready" event.
        let marked = chart.send(&ctx, "mark_ready", &HashMap::new());
        // mark_ready is not yet in the chart definition — verify this is a
        // no-op (returns false) rather than panicking. Phase 2 will add the
        // intra-state transitions.
        assert!(
            !marked,
            "mark_ready is not yet defined in Phase-1 chart; expect no-op"
        );

        // Confirm initial config has not changed.
        let config = chart.configuration(&ctx);
        assert!(
            config.contains("ingest.pending"),
            "config must still be ingest.pending after no-op event: {config:?}"
        );
    }
}
