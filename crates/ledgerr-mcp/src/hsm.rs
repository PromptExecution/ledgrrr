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
