//! Harness — the executive substrate that runs surface lifecycles as promise chains.
//!
//! A `SurfaceHarness` wraps any `ProcessSurface` and manages its state machine,
//! governance validation, and promise event log.

use crate::core::{
    AuditRecord, GovernancePolicy, LifecyclePromise, MachineState, MaintenanceAction,
    ProcessSurface, PromiseChain, PromiseOp, SurfaceCapability, SurfaceMachine,
};
use std::time::{Duration, Instant};

/// Outcome of a full lifecycle run. Errors are always `String` for
/// solver-readable audit trails.
#[derive(Debug, Clone)]
pub struct LifecycleOutcome<H: Clone> {
    pub surface: String,
    pub chain: PromiseChain<H>,
    pub governance_violations: Vec<String>,
    pub audit: Option<AuditRecord>,
    pub policy: GovernancePolicy,
    pub maintenance_action: MaintenanceAction,
}

/// The executive harness — wraps a surface with governance + machine.
pub struct SurfaceHarness<S: ProcessSurface> {
    pub surface: S,
    pub capability: SurfaceCapability,
    pub machine: SurfaceMachine,
    pub governance_violations: Vec<String>,
}

impl<S: ProcessSurface> SurfaceHarness<S> {
    pub fn new(surface: S) -> Self {
        let capability = surface.capability();
        let name = capability.name.to_owned();
        Self {
            surface,
            capability,
            machine: SurfaceMachine::new(&name),
            governance_violations: Vec::new(),
        }
    }

    /// Run the full lifecycle: init → operate → [maintain] → terminate.
    pub fn run_lifecycle(&mut self, config: S::Config) -> LifecycleOutcome<S::Handle>
    where
        S::Handle: Clone,
    {
        let policy = self.capability.governance.clone();
        if let Err(v) = self.capability.governance.validate() {
            self.governance_violations.push(v);
        }

        let now = Instant::now();

        // Init
        let init_promise = match self.surface.init(config) {
            Ok(()) => {
                let _ = self.machine.transition(MachineState::Ready, now.elapsed());
                LifecyclePromise::fulfilled(PromiseOp::Init, now.elapsed(), ())
            }
            Err(e) => LifecyclePromise::rejected(PromiseOp::Init, now.elapsed(), e.to_string()),
        };

        if init_promise.is_rejected() {
            return self.fail_fast(&now, init_promise);
        }

        // Operate
        let operate_promise = match self.surface.operate() {
            Ok(h) => {
                let _ = self
                    .machine
                    .transition(MachineState::Running, now.elapsed());
                LifecyclePromise::fulfilled(PromiseOp::Operate, now.elapsed(), h)
            }
            Err(e) => LifecyclePromise::rejected(PromiseOp::Operate, now.elapsed(), e.to_string()),
        };

        // Maintain (single pass)
        let maintain_action = self.surface.maintain();
        let maintain_promise = match self
            .machine
            .apply_maintenance(&maintain_action, now.elapsed())
        {
            Ok(()) => LifecyclePromise::fulfilled(
                PromiseOp::Maintain,
                now.elapsed(),
                maintain_action.clone(),
            ),
            Err(e) => LifecyclePromise::rejected(PromiseOp::Maintain, now.elapsed(), e),
        };

        // If operate failed, short-circuit with no terminate
        let handle = match &operate_promise.value {
            crate::core::PromiseValue::Fulfilled(h) => h.clone(),
            _ => {
                return LifecycleOutcome {
                    surface: self.capability.name.to_owned(),
                    chain: PromiseChain {
                        init: init_promise,
                        operate: operate_promise,
                        maintains: vec![maintain_promise],
                        terminate: LifecyclePromise::rejected(
                            PromiseOp::Terminate,
                            Duration::ZERO,
                            "operate never succeeded".into(),
                        ),
                    },
                    governance_violations: self.governance_violations.clone(),
                    policy,
                    maintenance_action: MaintenanceAction::NoOp,
                    audit: None,
                };
            }
        };

        // Terminate
        let terminate_promise = match S::terminate(handle) {
            Ok(record) => {
                let _ = self
                    .machine
                    .transition(MachineState::Terminated, now.elapsed());
                LifecyclePromise::fulfilled(PromiseOp::Terminate, now.elapsed(), record)
            }
            Err(e) => {
                LifecyclePromise::rejected(PromiseOp::Terminate, now.elapsed(), e.to_string())
            }
        };

        let audit = match &terminate_promise.value {
            crate::core::PromiseValue::Fulfilled(_) => Some(AuditRecord {
                surface_name: self.capability.name.to_owned(),
                uptime: now.elapsed(),
                exit_reason: "terminated".into(),
                crash_count: self.machine.crash_count,
                bytes_logged: 0,
            }),
            _ => None,
        };

        LifecycleOutcome {
            surface: self.capability.name.to_owned(),
            chain: PromiseChain {
                init: init_promise,
                operate: operate_promise,
                maintains: vec![maintain_promise],
                terminate: terminate_promise,
            },
            governance_violations: self.governance_violations.clone(),
            policy,
            maintenance_action: maintain_action,
            audit,
        }
    }

    /// Short-circuit when init fails.
    fn fail_fast(
        &self,
        _start: &Instant,
        init: LifecyclePromise<(), String>,
    ) -> LifecycleOutcome<S::Handle>
    where
        S::Handle: Clone,
    {
        LifecycleOutcome {
            surface: self.capability.name.to_owned(),
            chain: PromiseChain {
                init,
                operate: LifecyclePromise::rejected(
                    PromiseOp::Operate,
                    Duration::ZERO,
                    "init failed".into(),
                ),
                maintains: vec![],
                terminate: LifecyclePromise::rejected(
                    PromiseOp::Terminate,
                    Duration::ZERO,
                    "init failed".into(),
                ),
            },
            governance_violations: self.governance_violations.clone(),
            policy: self.capability.governance.clone(),
            maintenance_action: MaintenanceAction::Terminate,
            audit: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::DatumWatcherConfig;

    #[test]
    fn harness_runs_datum_watcher_lifecycle() {
        let tmp = tempfile::tempdir().expect("failed to create temp directory");
        let watcher = crate::core::DatumWatcher::new();
        let mut harness = SurfaceHarness::new(watcher);
        let config = DatumWatcherConfig {
            datum_dir: tmp.path().display().to_string(),
            poll_interval_secs: 30,
        };
        let outcome = harness.run_lifecycle(config);
        assert!(outcome.chain.init.is_fulfilled(), "init should pass");
        assert!(outcome.chain.operate.is_fulfilled(), "operate should pass");
        assert!(
            outcome.governance_violations.is_empty(),
            "no governance violations"
        );
    }

    #[test]
    fn harness_reports_init_failure() {
        let watcher = crate::core::DatumWatcher::new();
        let mut harness = SurfaceHarness::new(watcher);
        let config = DatumWatcherConfig {
            datum_dir: "/does/not/exist".into(),
            poll_interval_secs: 30,
        };
        let outcome = harness.run_lifecycle(config);
        assert!(outcome.chain.init.is_rejected());
    }

    #[test]
    fn harness_tracks_machine_state() {
        let tmp = tempfile::tempdir().expect("failed to create temp directory");
        let watcher = crate::core::DatumWatcher::new();
        let mut harness = SurfaceHarness::new(watcher);
        let config = DatumWatcherConfig {
            datum_dir: tmp.path().display().to_string(),
            poll_interval_secs: 30,
        };
        let _ = harness.run_lifecycle(config);
        assert_eq!(harness.machine.state, MachineState::Terminated);
    }

    #[test]
    fn governance_violation_captured() {
        let tmp = tempfile::tempdir().expect("failed to create temp directory");
        let watcher = crate::core::DatumWatcher::new();
        let mut harness = SurfaceHarness::new(watcher);
        harness.capability.governance.crash_budget = crate::core::MAX_CRASH_BUDGET + 1;
        let config = DatumWatcherConfig {
            datum_dir: tmp.path().display().to_string(),
            poll_interval_secs: 30,
        };
        let outcome = harness.run_lifecycle(config);
        assert!(!outcome.governance_violations.is_empty());
    }
}
