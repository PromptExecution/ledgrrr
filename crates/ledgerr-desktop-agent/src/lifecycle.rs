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
}
