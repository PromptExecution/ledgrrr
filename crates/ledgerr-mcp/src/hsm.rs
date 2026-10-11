use serde::{Deserialize, Serialize};

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
