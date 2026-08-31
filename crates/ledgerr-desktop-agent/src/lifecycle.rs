//! Windows Foundry Local's install/service lifecycle, and its export as a
//! W3C SCXML statechart (`foundry_lifecycle_to_statechart`, added in a
//! later step of this module) — following the exact pattern
//! `ufo-types::statechart::ooda_phases_to_statechart` already establishes
//! for this ecosystem's SysML-v2/statechart spine
//! (`elasticdotventures/_b00t_#1177`).
//!
//! Deliberate divergence from an event-dispatched state machine (e.g.
//! `OodaStateMachine::dispatch`): there is no live "engine" driving
//! transitions here. [`current_state`] is a pure derivation from a
//! [`crate::status::FoundryLocalStatus`] snapshot, computed fresh every
//! time status is collected — that function IS this phase's real source of
//! truth. The SCXML export models the *shape* of how that derived state
//! can move over time (useful for visualization/governance), not a
//! literal transition log; this mirrors how `ooda_phases_to_statechart`'s
//! own doc comment flags its `Cancel`-as-`Final`-state divergence from the
//! live OODA dispatcher.

use crate::status::FoundryLocalStatus;
use scxml::model::{State, Statechart, Transition};

/// Windows Foundry Local's observed lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoundryLocalLifecycle {
    /// The `foundry` CLI is not on PATH.
    NotInstalled,
    /// The CLI is present but its service is not reporting as running.
    InstalledStopped,
    /// The CLI is present and its service reports as running.
    InstalledRunning,
}

/// Derives the current lifecycle state from a real status snapshot. This
/// is the authoritative logic — the SCXML export (see
/// `foundry_lifecycle_to_statechart`, added later in this file) documents
/// this same shape for visualization, it does not replace this function.
pub fn current_state(status: &FoundryLocalStatus) -> FoundryLocalLifecycle {
    if !status.cli_found {
        FoundryLocalLifecycle::NotInstalled
    } else if status.service_running {
        FoundryLocalLifecycle::InstalledRunning
    } else {
        FoundryLocalLifecycle::InstalledStopped
    }
}

pub(crate) fn state_id(state: FoundryLocalLifecycle) -> &'static str {
    match state {
        FoundryLocalLifecycle::NotInstalled => "not_installed",
        FoundryLocalLifecycle::InstalledStopped => "installed_stopped",
        FoundryLocalLifecycle::InstalledRunning => "installed_running",
    }
}

/// Exports [`FoundryLocalLifecycle`]'s state shape as a W3C SCXML
/// statechart, following `ufo-types::statechart::ooda_phases_to_statechart`'s
/// exact pattern. See this module's doc comment for why these transitions
/// describe the *shape* of the lifecycle rather than a literally-dispatched
/// event stream.
pub fn foundry_lifecycle_to_statechart() -> Statechart {
    let mut not_installed = State::atomic(state_id(FoundryLocalLifecycle::NotInstalled));
    not_installed.transitions.push(Transition::new(
        "InstallSucceeded",
        state_id(FoundryLocalLifecycle::InstalledStopped),
    ));

    let mut installed_stopped = State::atomic(state_id(FoundryLocalLifecycle::InstalledStopped));
    installed_stopped.transitions.push(Transition::new(
        "ServiceStarted",
        state_id(FoundryLocalLifecycle::InstalledRunning),
    ));

    let mut installed_running = State::atomic(state_id(FoundryLocalLifecycle::InstalledRunning));
    installed_running.transitions.push(Transition::new(
        "ServiceStopped",
        state_id(FoundryLocalLifecycle::InstalledStopped),
    ));

    Statechart::new(
        state_id(FoundryLocalLifecycle::NotInstalled),
        vec![not_installed, installed_stopped, installed_running],
    )
    .with_name("foundry_local_lifecycle")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_not_found_means_not_installed_regardless_of_service_flag() {
        let status = FoundryLocalStatus {
            cli_found: false,
            service_running: true, // must be ignored — cli_found gates everything
        };
        assert_eq!(current_state(&status), FoundryLocalLifecycle::NotInstalled);
    }

    #[test]
    fn cli_found_and_service_not_running_is_installed_stopped() {
        let status = FoundryLocalStatus {
            cli_found: true,
            service_running: false,
        };
        assert_eq!(current_state(&status), FoundryLocalLifecycle::InstalledStopped);
    }

    #[test]
    fn cli_found_and_service_running_is_installed_running() {
        let status = FoundryLocalStatus {
            cli_found: true,
            service_running: true,
        };
        assert_eq!(current_state(&status), FoundryLocalLifecycle::InstalledRunning);
    }

    #[test]
    fn state_ids_are_distinct_and_stable() {
        assert_eq!(state_id(FoundryLocalLifecycle::NotInstalled), "not_installed");
        assert_eq!(state_id(FoundryLocalLifecycle::InstalledStopped), "installed_stopped");
        assert_eq!(state_id(FoundryLocalLifecycle::InstalledRunning), "installed_running");
    }

    use scxml::export::xml::to_xml;
    use scxml::model::StateKind;
    use scxml::parse_xml;
    use scxml::validate;

    #[test]
    fn statechart_has_exactly_the_three_lifecycle_states() {
        let chart = foundry_lifecycle_to_statechart();
        assert_eq!(chart.states.len(), 3);
        for state in [
            FoundryLocalLifecycle::NotInstalled,
            FoundryLocalLifecycle::InstalledStopped,
            FoundryLocalLifecycle::InstalledRunning,
        ] {
            let found = chart.find_state(state_id(state));
            assert!(found.is_some(), "missing state {}", state_id(state));
            assert_eq!(found.unwrap().kind, StateKind::Atomic);
        }
    }

    #[test]
    fn not_installed_transitions_to_installed_stopped_on_install_succeeded() {
        let chart = foundry_lifecycle_to_statechart();
        let not_installed = chart
            .find_state(state_id(FoundryLocalLifecycle::NotInstalled))
            .unwrap();
        let transition = not_installed
            .transitions
            .iter()
            .find(|t| t.event.as_deref() == Some("InstallSucceeded"))
            .expect("NotInstalled must accept InstallSucceeded");
        assert_eq!(
            transition.targets,
            vec![state_id(FoundryLocalLifecycle::InstalledStopped)]
        );
    }

    #[test]
    fn installed_stopped_and_running_transition_to_each_other() {
        let chart = foundry_lifecycle_to_statechart();
        let stopped = chart
            .find_state(state_id(FoundryLocalLifecycle::InstalledStopped))
            .unwrap();
        let start = stopped
            .transitions
            .iter()
            .find(|t| t.event.as_deref() == Some("ServiceStarted"))
            .expect("InstalledStopped must accept ServiceStarted");
        assert_eq!(start.targets, vec![state_id(FoundryLocalLifecycle::InstalledRunning)]);

        let running = chart
            .find_state(state_id(FoundryLocalLifecycle::InstalledRunning))
            .unwrap();
        let stop = running
            .transitions
            .iter()
            .find(|t| t.event.as_deref() == Some("ServiceStopped"))
            .expect("InstalledRunning must accept ServiceStopped");
        assert_eq!(stop.targets, vec![state_id(FoundryLocalLifecycle::InstalledStopped)]);
    }

    #[test]
    fn statechart_passes_scxml_structural_validation() {
        let chart = foundry_lifecycle_to_statechart();
        validate(&chart).expect("exported statechart should be structurally valid SCXML");
    }

    #[test]
    fn statechart_round_trips_through_xml_export_and_parse() {
        let chart = foundry_lifecycle_to_statechart();
        let xml = to_xml(&chart);
        let parsed = parse_xml(&xml).expect("exported XML must parse back as valid SCXML");
        assert_eq!(parsed, chart);
    }
}
