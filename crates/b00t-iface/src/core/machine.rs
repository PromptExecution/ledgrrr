//! Machine — abstract state machine over surface states.
//!
//! Every surface transitions through a finite set of states. The machine
//! enforces that transitions are valid and that governance constraints
//! are satisfied at each step.
//!
//! # State diagram
//!
//! ```text
//!                 +----------+
//!                 |  Idle    |
//!                 +----+-----+
//!                      |
//!                    init()
//!                      |
//!                 +----v-----+
//!          +------+ Ready    +------+
//!          |      +----+-----+      |
//!          |           |            |
//!       maintain()  operate()    terminate()
//!          |           |            |
//!          |      +----v-----+      |
//!          +------+ Running  |      |
//!          |      +----+-----+      |
//!          |           |            |
//!       maintain()  maintain()      |
//!          |     +----+-----+       |
//!          +-----> Healthy  |       |
//!          |     +----------+       |
//!          |                        |
//!          +----> Quarantined       |
//!          |     +----------+       |
//!          |                        |
//!          +-------------------v----+
//!                         Terminated
//! ```

#[cfg(feature = "lazily")]
use super::governance::GovernancePolicy;
use super::surface::MaintenanceAction;
use std::fmt;
use std::time::Duration;

/// The finite set of states a surface machine can be in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachineState {
    Idle,
    Ready,
    Running,
    Healthy,
    Quarantined,
    Terminated,
}

impl fmt::Display for MachineState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Idle => write!(f, "idle"),
            Self::Ready => write!(f, "ready"),
            Self::Running => write!(f, "running"),
            Self::Healthy => write!(f, "healthy"),
            Self::Quarantined => write!(f, "quarantined"),
            Self::Terminated => write!(f, "terminated"),
        }
    }
}

/// A transition between two machine states.
#[derive(Debug, Clone)]
pub struct MachineTransition {
    pub from: MachineState,
    pub to: MachineState,
    pub trigger: &'static str,
    pub elapsed: Duration,
}

/// Valid transitions between machine states.
/// Returns `None` if the transition is invalid (solver-verifiable).
pub fn valid_transition(from: MachineState, to: MachineState) -> Option<&'static str> {
    use MachineState::*;
    match (from, to) {
        (Idle, Ready) => Some("init"),
        (Ready, Running) => Some("operate"),
        (Running, Healthy) => Some("maintain:noop"),
        (Running, Ready) => Some("maintain:restart"),
        (Running, Quarantined) => Some("maintain:quarantine"),
        (Ready, Terminated) => Some("terminate"),
        (Running, Terminated) => Some("terminate"),
        (Healthy, Running) => Some("operate"),
        (Healthy, Ready) => Some("maintain:restart"),
        (Healthy, Terminated) => Some("terminate"),
        (Quarantined, Terminated) => Some("terminate"),
        _ => None,
    }
}

/// All states in `MachineState`, in the order they appear in the state
/// diagram above.
#[cfg(feature = "statechart")]
const ALL_STATES: [MachineState; 6] = [
    MachineState::Idle,
    MachineState::Ready,
    MachineState::Running,
    MachineState::Healthy,
    MachineState::Quarantined,
    MachineState::Terminated,
];

/// Compile the surface lifecycle as a W3C SCXML statechart (_b00t_#1177 P8),
/// derived directly from `valid_transition` so it can never drift from the
/// governance-enforced transition table above. A state exports as `Final`
/// only when it has zero outgoing transitions — that's just `Terminated`
/// here: `Quarantined` still has a real `terminate` transition out, even
/// though `is_terminal()` groups the two together as "no further useful
/// work," so it stays `Atomic`.
#[cfg(feature = "statechart")]
pub fn machine_scxml() -> scxml::model::Statechart {
    use scxml::model::{State, Transition};

    let states: Vec<State> = ALL_STATES
        .iter()
        .map(|&from| {
            let transitions: Vec<Transition> = ALL_STATES
                .iter()
                .filter_map(|&to| valid_transition(from, to).map(|trigger| (to, trigger)))
                .map(|(to, trigger)| Transition::new(trigger, to.to_string()))
                .collect();

            if transitions.is_empty() {
                State::final_state(from.to_string())
            } else {
                let mut state = State::atomic(from.to_string());
                state.transitions = transitions;
                state
            }
        })
        .collect();

    scxml::model::Statechart::new(MachineState::Idle.to_string(), states)
        .with_name("SurfaceMachine")
}

/// The state machine that governs a surface lifecycle.
#[derive(Debug, Clone)]
pub struct SurfaceMachine {
    pub surface_name: String,
    pub state: MachineState,
    pub crash_count: u32,
    pub total_uptime: Duration,
    pub transitions: Vec<MachineTransition>,
}

impl SurfaceMachine {
    pub fn new(surface_name: &str) -> Self {
        Self {
            surface_name: surface_name.to_owned(),
            state: MachineState::Idle,
            crash_count: 0,
            total_uptime: Duration::ZERO,
            transitions: Vec::new(),
        }
    }

    /// Attempt a transition. Returns the trigger label if valid.
    pub fn transition(
        &mut self,
        to: MachineState,
        elapsed: Duration,
    ) -> Result<&'static str, String> {
        let trigger = valid_transition(self.state, to)
            .ok_or_else(|| format!("invalid transition: {} → {}", self.state, to))?;

        self.transitions.push(MachineTransition {
            from: self.state,
            to,
            trigger,
            elapsed,
        });
        self.total_uptime += elapsed;
        self.state = to;
        Ok(trigger)
    }

    /// Process a maintenance action and transition accordingly.
    pub fn apply_maintenance(
        &mut self,
        action: &MaintenanceAction,
        elapsed: Duration,
    ) -> Result<(), String> {
        match action {
            MaintenanceAction::NoOp => {
                self.transition(MachineState::Healthy, elapsed)?;
            }
            MaintenanceAction::Restart => {
                self.crash_count += 1;
                self.transition(MachineState::Ready, elapsed)?;
            }
            MaintenanceAction::Terminate => {
                self.transition(MachineState::Terminated, elapsed)?;
            }
            MaintenanceAction::Quarantine { reason: _ } => {
                self.crash_count += 1;
                self.transition(MachineState::Quarantined, elapsed)?;
            }
        }
        Ok(())
    }

    /// Whether the machine is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        self.state == MachineState::Terminated || self.state == MachineState::Quarantined
    }
}

/// Live lazily-backed execution layer for the surface lifecycle chart.
///
/// `SurfaceChart` wraps a `ThreadSafeStateChart` whose topology is derived
/// directly from `valid_transition`. `GovernancePolicy` fields become named
/// guards resolved at each `send` call — the topology never needs to change
/// when policy changes.
///
/// # Guard names
///
/// | Guard name             | Allows transition when                                  |
/// |------------------------|---------------------------------------------------------|
/// | `auto_restart_allowed` | `policy.auto_restart && crash_count < policy.crash_budget` |
/// | `ttl_ok`               | `elapsed < policy.max_ttl`                              |
#[cfg(feature = "lazily")]
pub struct SurfaceChart {
    chart: lazily::statechart::ThreadSafeStateChart,
    ctx: lazily::ThreadSafeContext,
    pub policy: GovernancePolicy,
}

/// Build a `ChartDef` whose topology exactly mirrors `valid_transition`.
/// Guarded edges use named guards resolved at `send` time.
#[cfg(feature = "lazily")]
pub fn surface_chart_def() -> Result<lazily::statechart::ChartDef, String> {
    use lazily::statechart::{ChartBuilder, StateBuilder};

    ChartBuilder::new()
        // Single root compound state; initial child is idle
        .state(StateBuilder::compound("surface", "idle"))
        .state(StateBuilder::atomic("idle").parent("surface").on("init", "ready"))
        .state(
            StateBuilder::atomic("ready")
                .parent("surface")
                .on("operate", "running")
                .on("terminate", "terminated"),
        )
        .state(
            StateBuilder::atomic("running")
                .parent("surface")
                .on("maintain:noop", "healthy")
                .on_guarded("maintain:restart", "ready", "auto_restart_allowed")
                .on("maintain:quarantine", "quarantined")
                .on("terminate", "terminated"),
        )
        .state(
            StateBuilder::atomic("healthy")
                .parent("surface")
                .on_guarded("operate", "running", "ttl_ok")
                .on_guarded("maintain:restart", "ready", "auto_restart_allowed")
                .on("terminate", "terminated"),
        )
        .state(
            StateBuilder::atomic("quarantined")
                .parent("surface")
                .on("terminate", "terminated"),
        )
        .state(StateBuilder::final_state("terminated").parent("surface"))
        .build()
}

#[cfg(feature = "lazily")]
impl SurfaceChart {
    pub fn new(policy: GovernancePolicy) -> Result<Self, String> {
        let def = surface_chart_def()?;
        let ctx = lazily::ThreadSafeContext::new();
        let chart = lazily::statechart::ThreadSafeStateChart::new(&ctx, def);
        Ok(Self { chart, ctx, policy })
    }

    /// Current active leaf states.
    pub fn active_leaves(&self) -> Vec<String> {
        self.chart.active_leaves(&self.ctx)
    }

    /// Whether the chart is currently in the named state.
    pub fn matches(&self, id: &str) -> bool {
        self.chart.matches(&self.ctx, id)
    }

    /// Send an event, resolving guards from current `crash_count` and elapsed time.
    /// Returns `true` if the transition fired.
    pub fn send(&self, event: &str, crash_count: u32, elapsed: Duration) -> bool {
        use std::collections::HashMap;
        let guards = HashMap::from([
            (
                "auto_restart_allowed".to_string(),
                self.policy.auto_restart && crash_count < self.policy.crash_budget,
            ),
            ("ttl_ok".to_string(), elapsed < self.policy.max_ttl),
        ]);
        self.chart.send(&self.ctx, event, &guards)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_lifecycle() {
        let mut m = SurfaceMachine::new("test");
        m.transition(MachineState::Ready, Duration::from_millis(10))
            .unwrap();
        m.transition(MachineState::Running, Duration::from_millis(50))
            .unwrap();
        m.apply_maintenance(&MaintenanceAction::NoOp, Duration::from_millis(5))
            .unwrap();
        m.transition(MachineState::Terminated, Duration::from_millis(20))
            .unwrap();
        assert!(m.is_terminal());
        assert_eq!(m.transitions.len(), 4);
    }

    #[test]
    fn invalid_transition_fails() {
        let mut m = SurfaceMachine::new("test");
        let err = m
            .transition(MachineState::Running, Duration::ZERO)
            .unwrap_err();
        assert!(err.contains("invalid transition"));
    }

    #[test]
    fn all_valid_transitions() {
        use MachineState::*;
        // Every valid pair should return Some
        assert!(valid_transition(Idle, Ready).is_some());
        assert!(valid_transition(Ready, Running).is_some());
        assert!(valid_transition(Running, Healthy).is_some());
        assert!(valid_transition(Running, Ready).is_some());
        assert!(valid_transition(Running, Quarantined).is_some());
        assert!(valid_transition(Ready, Terminated).is_some());
        assert!(valid_transition(Running, Terminated).is_some());
        assert!(valid_transition(Healthy, Running).is_some());
        assert!(valid_transition(Healthy, Ready).is_some());
        assert!(valid_transition(Healthy, Terminated).is_some());
        assert!(valid_transition(Quarantined, Terminated).is_some());
        // Invalid pairs should return None
        assert!(valid_transition(Idle, Running).is_none());
        assert!(valid_transition(Terminated, Running).is_none());
    }

    #[test]
    fn crash_count_tracks_restarts() {
        let mut m = SurfaceMachine::new("test");
        m.transition(MachineState::Ready, Duration::ZERO).unwrap();
        m.transition(MachineState::Running, Duration::ZERO).unwrap();
        m.apply_maintenance(&MaintenanceAction::Restart, Duration::ZERO)
            .unwrap();
        assert_eq!(m.crash_count, 1);
        assert_eq!(m.state, MachineState::Ready);
    }

    #[test]
    fn machine_display() {
        assert_eq!(MachineState::Running.to_string(), "running");
        assert_eq!(MachineState::Quarantined.to_string(), "quarantined");
    }

    #[cfg(feature = "lazily")]
    #[test]
    fn surface_chart_def_builds_without_error() {
        surface_chart_def().expect("surface_chart_def should build");
    }

    #[cfg(feature = "lazily")]
    #[test]
    fn surface_chart_topology_matches_valid_transition() {
        let policy = GovernancePolicy::default();
        let sc = SurfaceChart::new(policy).expect("SurfaceChart::new");

        // Starts in idle
        assert!(sc.matches("idle"));

        // Drive through the happy path: idle→ready→running→healthy
        assert!(sc.send("init", 0, Duration::ZERO));
        assert!(sc.matches("ready"));

        assert!(sc.send("operate", 0, Duration::ZERO));
        assert!(sc.matches("running"));

        let short = Duration::from_secs(1);
        // maintain:noop → healthy
        assert!(sc.send("maintain:noop", 0, short));
        assert!(sc.matches("healthy"));

        // healthy → running (ttl_ok)
        assert!(sc.send("operate", 0, short));
        assert!(sc.matches("running"));

        // terminate from running
        assert!(sc.send("terminate", 0, Duration::ZERO));
        assert!(sc.matches("terminated"));
    }

    #[cfg(feature = "lazily")]
    #[test]
    fn surface_chart_restart_blocked_when_crash_budget_exhausted() {
        let policy = GovernancePolicy {
            auto_restart: true,
            crash_budget: 2,
            ..GovernancePolicy::default()
        };
        let sc = SurfaceChart::new(policy).expect("SurfaceChart::new");

        // Advance to running
        sc.send("init", 0, Duration::ZERO);
        sc.send("operate", 0, Duration::ZERO);
        assert!(sc.matches("running"));

        // crash_count=2 == crash_budget=2 → restart blocked (fail-closed)
        let fired = sc.send("maintain:restart", 2, Duration::ZERO);
        assert!(!fired, "restart should be blocked when crash_count >= crash_budget");
        assert!(sc.matches("running"), "state must not change on blocked restart");
    }

    #[cfg(feature = "lazily")]
    #[test]
    fn surface_chart_restart_allowed_when_budget_remains() {
        let policy = GovernancePolicy {
            auto_restart: true,
            crash_budget: 3,
            ..GovernancePolicy::default()
        };
        let sc = SurfaceChart::new(policy).expect("SurfaceChart::new");

        sc.send("init", 0, Duration::ZERO);
        sc.send("operate", 0, Duration::ZERO);
        assert!(sc.matches("running"));

        // crash_count=2 < crash_budget=3 → restart allowed
        let fired = sc.send("maintain:restart", 2, Duration::ZERO);
        assert!(fired, "restart should be allowed when crash_count < crash_budget");
        assert!(sc.matches("ready"));
    }

    #[cfg(feature = "statechart")]
    #[test]
    fn machine_scxml_round_trips_and_covers_every_real_transition() {
        use scxml::model::StateKind;

        let chart = machine_scxml();
        scxml::validate(&chart)
            .expect("SurfaceMachine's exported statechart should be valid SCXML");

        let xml = scxml::export::xml::to_xml(&chart);
        let round_tripped = scxml::parse_xml(&xml).expect("round-tripped XML should re-parse");
        assert_eq!(chart, round_tripped);

        assert_eq!(round_tripped.initial.as_str(), "idle");

        // Only Terminated has zero outgoing transitions; Quarantined still
        // has a real `terminate` transition out, so it stays Atomic despite
        // is_terminal() grouping the two together.
        let terminated = round_tripped.find_state("terminated").unwrap();
        assert_eq!(terminated.kind, StateKind::Final);
        let quarantined = round_tripped.find_state("quarantined").unwrap();
        assert_eq!(quarantined.kind, StateKind::Atomic);

        for from in ALL_STATES {
            for to in ALL_STATES {
                if let Some(trigger) = valid_transition(from, to) {
                    let state = round_tripped.find_state(&from.to_string()).unwrap();
                    assert!(
                        state
                            .transitions
                            .iter()
                            .any(|t| t.event.as_deref() == Some(trigger)
                                && t.targets == vec![to.to_string()]),
                        "missing transition {from} --{trigger}--> {to}"
                    );
                }
            }
        }
    }
}
