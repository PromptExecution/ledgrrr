//! Append-only process controller with Blake3-authorized step transitions.
//!
//! Each `ProcessStep` transition is authorized by hashing
//! `(step_id, authorized_by, timestamp_ms)` with Blake3.
//! The digest is returned in `TransitionReceipt` for audit trail.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{
    log::{ActionKind, ImmutableActionLog},
    HolonError,
};

/// A single step definition in a process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessStep {
    /// Unique step identifier.
    pub step_id: String,
    /// Human-readable description.
    pub description: String,
}

/// Receipt returned when a step transition is authorized.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitionReceipt {
    /// The step ID that was authorized.
    pub step_id: String,
    /// The authorizing agent.
    pub authorized_by: String,
    /// Timestamp at which the authorization was recorded.
    pub timestamp_ms: u64,
    /// Blake3 digest of `(step_id || authorized_by || timestamp_ms le-bytes)`.
    pub authorization_hash: [u8; 32],
}

/// Wraps a sequence of `ProcessStep`s with append-only, Blake3-gated transitions.
///
/// Steps are registered once; each `authorize_step` call records the transition
/// in the internal `ImmutableActionLog` and returns a `TransitionReceipt`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessController {
    steps: Vec<ProcessStep>,
    /// Map from step_id to position in `steps` for O(1) lookup.
    step_index: HashMap<String, usize>,
    log: ImmutableActionLog,
}

impl ProcessController {
    /// Create a new controller with no registered steps.
    pub fn new() -> Self {
        Self {
            steps: Vec::new(),
            step_index: HashMap::new(),
            log: ImmutableActionLog::new(),
        }
    }

    /// Register a process step. Returns `Err` if the step ID is empty or
    /// already registered.
    pub fn register_step(&mut self, step: ProcessStep) -> Result<(), HolonError> {
        if step.step_id.is_empty() {
            return Err(HolonError::EmptyStepId);
        }
        if self.step_index.contains_key(&step.step_id) {
            return Err(HolonError::DuplicateStep(step.step_id));
        }
        let idx = self.steps.len();
        self.step_index.insert(step.step_id.clone(), idx);
        self.steps.push(step);
        Ok(())
    }

    /// Authorize and record a transition for the named step.
    ///
    /// # Errors
    /// - `HolonError::NotFound` — step ID not registered
    /// - `HolonError::EmptyStepId` — step_id is empty
    /// - `HolonError::EmptyAuthorizer` — authorized_by is empty
    pub fn authorize_step(
        &mut self,
        step_id: impl Into<String>,
        authorized_by: impl Into<String>,
        timestamp_ms: u64,
    ) -> Result<TransitionReceipt, HolonError> {
        let step_id: String = step_id.into();
        let authorized_by: String = authorized_by.into();

        if step_id.is_empty() {
            return Err(HolonError::EmptyStepId);
        }
        if authorized_by.is_empty() {
            return Err(HolonError::EmptyAuthorizer);
        }
        if !self.step_index.contains_key(&step_id) {
            return Err(HolonError::NotFound(step_id));
        }

        let authorization_hash = compute_transition_hash(&step_id, &authorized_by, timestamp_ms);

        // Record in the append-only log; payload_hash encodes the auth digest.
        self.log.append(
            ActionKind::StepAuthorized,
            &authorized_by,
            timestamp_ms,
            authorization_hash,
        );

        Ok(TransitionReceipt {
            step_id,
            authorized_by,
            timestamp_ms,
            authorization_hash,
        })
    }

    /// All registered steps in registration order.
    pub fn steps(&self) -> &[ProcessStep] {
        &self.steps
    }

    /// Read-only view of the authorization log.
    pub fn log(&self) -> &ImmutableActionLog {
        &self.log
    }
}

impl Default for ProcessController {
    fn default() -> Self {
        Self::new()
    }
}

/// Blake3 hash of `step_id || 0x00 || authorized_by || 0x00 || timestamp_ms le-bytes`.
fn compute_transition_hash(step_id: &str, authorized_by: &str, timestamp_ms: u64) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(step_id.as_bytes());
    hasher.update(&[0x00]);
    hasher.update(authorized_by.as_bytes());
    hasher.update(&[0x00]);
    hasher.update(&timestamp_ms.to_le_bytes());
    *hasher.finalize().as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_step(id: &str) -> ProcessStep {
        ProcessStep {
            step_id: id.to_string(),
            description: format!("Step {}", id),
        }
    }

    #[test]
    fn register_and_authorize_step() {
        let mut ctrl = ProcessController::new();
        ctrl.register_step(make_step("review")).expect("register");
        let receipt = ctrl
            .authorize_step("review", "alice", 42_000)
            .expect("authorize");
        assert_eq!(receipt.step_id, "review");
        assert_eq!(receipt.authorized_by, "alice");
        assert_eq!(ctrl.log().len(), 1);
    }

    #[test]
    fn duplicate_step_is_rejected() {
        let mut ctrl = ProcessController::new();
        ctrl.register_step(make_step("sign-off")).expect("first");
        let err = ctrl.register_step(make_step("sign-off")).unwrap_err();
        assert!(matches!(err, HolonError::DuplicateStep(_)));
    }

    #[test]
    fn unknown_step_authorization_is_rejected() {
        let mut ctrl = ProcessController::new();
        let err = ctrl
            .authorize_step("nonexistent", "alice", 0)
            .unwrap_err();
        assert!(matches!(err, HolonError::NotFound(_)));
    }

    #[test]
    fn empty_authorizer_is_rejected() {
        let mut ctrl = ProcessController::new();
        ctrl.register_step(make_step("approve")).expect("register");
        let err = ctrl.authorize_step("approve", "", 0).unwrap_err();
        assert!(matches!(err, HolonError::EmptyAuthorizer));
    }

    #[test]
    fn transition_hash_is_deterministic() {
        let h1 = compute_transition_hash("step-1", "bob", 9_999);
        let h2 = compute_transition_hash("step-1", "bob", 9_999);
        assert_eq!(h1, h2);
    }
}
