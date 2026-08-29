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
                        state.transitions.iter().any(|t| t.event.as_deref()
                            == Some(trigger)
                            && t.targets == vec![to.to_string()]),
                        "missing transition {from} --{trigger}--> {to}"
                    );
                }
            }
        }
    }
}
