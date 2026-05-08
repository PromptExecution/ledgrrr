//! Z3 attestation layer for the ledgrrr compliance engine.
//!
//! Wraps [`agentmesh::ComplianceEngine`] and adds a Blake3-backed attestation
//! ledger that maps named audit controls to proof hashes.  A control moves from
//! `controls_pending` → `controls_satisfied` the moment a valid `blake3_hex`
//! attestation is recorded for it.
//!
//! [`LedgrrComplianceReport`] is the *ledgrrr-domain* report type; it is
//! distinct from [`agentmesh::ComplianceReport`], which is violation-centric.
//! Both are available: callers that need the AGT violation report can call
//! [`ComplianceStore::agt_report`]; callers that need the attestation-aware
//! report call [`ComplianceStore::ledgrrr_report`].

use agentmesh::{ComplianceEngine, ComplianceFramework};
use serde::Serialize;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

fn unix_secs_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// A Blake3 proof hash that attests a named compliance control has been
/// cryptographically satisfied (e.g. by an arc-kit-au Z3 node ID).
#[derive(Debug, Clone, Serialize)]
pub struct Z3Attestation {
    /// Audit control identifier, e.g. `"soc2-cc6.1"` or `"eu-ai-act-art-13"`.
    pub control_id: String,
    /// Hex-encoded Blake3 digest produced by arc-kit-au.
    pub blake3_hex: String,
    /// Unix epoch seconds at the time `attest_z3_proof` was called.
    pub timestamp_utc: u64,
}

/// Compliance grade derived from the ratio of satisfied controls.
#[derive(Debug, Default, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ComplianceGrade {
    /// No controls have been attested yet.
    #[default]
    Unknown,
    /// At least one control satisfied, but not all registered controls.
    Partial,
    /// Every registered control has at least one attestation.
    Full,
}

/// Ledgrrr-domain compliance report.
///
/// Populated by successive [`ComplianceStore::attest_z3_proof`] calls.
/// The `grade` field is computed at report generation time.
#[derive(Debug, Default, Serialize)]
pub struct LedgrrComplianceReport {
    pub controls_satisfied: Vec<String>,
    pub controls_pending: Vec<String>,
    pub attestations: Vec<Z3Attestation>,
    pub grade: ComplianceGrade,
}

// ---------------------------------------------------------------------------
// ComplianceStore
// ---------------------------------------------------------------------------

/// Inner (non-Arc) state shared between `ComplianceStore` and
/// `LedgrrAgtGateway::attest_z3_proof`.
struct StoreInner {
    /// Registered control IDs that must be attested to reach `Full` grade.
    /// Populated lazily: any control_id passed to `attest_z3_proof` is added.
    registered: HashSet<String>,
    /// Control IDs that have received at least one attestation.
    satisfied: HashSet<String>,
    /// Append-only attestation ledger.
    attestations: Vec<Z3Attestation>,
}

/// Manages the attestation ledger and wraps the AGT compliance engine.
///
/// Cheaply cloneable via inner `Arc<Mutex<…>>`.
#[derive(Clone)]
pub struct ComplianceStore {
    engine: Arc<ComplianceEngine>,
    inner: Arc<Mutex<StoreInner>>,
}

impl ComplianceStore {
    /// Create a store backed by an AGT `ComplianceEngine` that tracks
    /// SOC 2 controls by default.
    pub fn new() -> Self {
        Self {
            engine: Arc::new(ComplianceEngine::default()),
            inner: Arc::new(Mutex::new(StoreInner {
                registered: HashSet::new(),
                satisfied: HashSet::new(),
                attestations: Vec::new(),
            })),
        }
    }

    /// Pre-declare a compliance control as required without attesting it.
    ///
    /// Controls registered here appear in `controls_pending` until a
    /// corresponding [`Self::attest`] call satisfies them.  Calling this
    /// enables [`ComplianceGrade::Partial`]: the set of required controls is
    /// known up-front, so some-but-not-all satisfied produces a non-empty
    /// pending set.
    pub fn register_control(&self, control_id: &str) {
        let mut guard = self
            .inner
            .lock()
            .expect("ComplianceStore mutex poisoned in register_control");
        let inserted = guard.registered.insert(control_id.to_string());
        if inserted {
            tracing::debug!(control_id, "compliance control pre-registered");
        }
    }

    /// Record a Blake3 attestation hash for `control_id`.
    ///
    /// The control is moved from `controls_pending` to `controls_satisfied` in
    /// the next [`Self::ledgrrr_report`] call.  The attestation is appended to
    /// the ledger regardless of whether the control was previously satisfied.
    pub fn attest(&self, control_id: &str, blake3_hex: &str) {
        let attestation = Z3Attestation {
            control_id: control_id.to_string(),
            blake3_hex: blake3_hex.to_string(),
            timestamp_utc: unix_secs_now(),
        };
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        guard.registered.insert(control_id.to_string());
        guard.satisfied.insert(control_id.to_string());
        guard.attestations.push(attestation);

        tracing::info!(
            control_id,
            blake3_hex,
            "z3_attestation recorded — control marked satisfied"
        );
    }

    /// Generate the ledgrrr-domain compliance report.
    ///
    /// `controls_pending` contains every registered control that has NOT yet
    /// received an attestation.  Grade is computed as:
    /// - `Unknown`  — no attestations recorded
    /// - `Partial`  — some (not all) controls satisfied
    /// - `Full`     — all registered controls satisfied
    pub fn ledgrrr_report(&self) -> LedgrrComplianceReport {
        let guard = self
            .inner
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        let mut satisfied: Vec<String> = guard.satisfied.iter().cloned().collect();
        satisfied.sort();

        let pending: Vec<String> = guard
            .registered
            .iter()
            .filter(|id| !guard.satisfied.contains(*id))
            .cloned()
            .collect();

        let grade = if guard.satisfied.is_empty() {
            ComplianceGrade::Unknown
        } else if pending.is_empty() {
            ComplianceGrade::Full
        } else {
            ComplianceGrade::Partial
        };

        LedgrrComplianceReport {
            controls_satisfied: satisfied,
            controls_pending: pending,
            attestations: guard.attestations.clone(),
            grade,
        }
    }

    /// Generate the raw AGT SOC 2 violation report.
    ///
    /// Useful for audit pipelines that consume the AGT violation model.
    pub fn agt_report(&self) -> agentmesh::ComplianceReport {
        self.engine.generate_report(ComplianceFramework::Soc2)
    }
}

impl Default for ComplianceStore {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compliance_report_is_initially_empty() {
        let store = ComplianceStore::new();
        let report = store.ledgrrr_report();
        assert!(
            report.controls_satisfied.is_empty(),
            "fresh store must have no satisfied controls"
        );
        assert!(
            report.attestations.is_empty(),
            "fresh store must have no attestations"
        );
        assert_eq!(
            report.grade,
            ComplianceGrade::Unknown,
            "fresh store grade must be Unknown"
        );
    }

    #[test]
    fn attest_z3_proof_adds_to_report() {
        let store = ComplianceStore::new();
        store.attest("soc2-cc6.1", "abc123def");
        let report = store.ledgrrr_report();
        assert!(
            report.controls_satisfied.contains(&"soc2-cc6.1".to_string()),
            "attested control must appear in controls_satisfied"
        );
        assert_eq!(
            report.attestations.len(),
            1,
            "exactly one attestation must be recorded"
        );
        assert_eq!(report.attestations[0].control_id, "soc2-cc6.1");
        assert_eq!(report.attestations[0].blake3_hex, "abc123def");
    }

    #[test]
    fn compliance_grade_upgrades_with_attestations() {
        // Register 3 controls up-front, attest only 2 → Partial
        let store = ComplianceStore::new();
        store.register_control("soc2-cc6.1");
        store.register_control("eu-ai-act-art-13");
        store.register_control("soc2-cc7.2");
        store.attest("soc2-cc6.1", "aaabbbccc111");
        store.attest("eu-ai-act-art-13", "dddeeefff222");
        let report = store.ledgrrr_report();
        assert_eq!(
            report.grade,
            ComplianceGrade::Partial,
            "2 of 3 controls attested → Partial; got: {:?}",
            report.grade
        );
        assert_eq!(
            report.controls_pending,
            vec!["soc2-cc7.2".to_string()],
            "one control must remain pending"
        );
    }

    #[test]
    fn compliance_grade_full_when_all_attested() {
        // Register 3 controls, attest all 3 → Full
        let store = ComplianceStore::new();
        store.register_control("soc2-cc6.1");
        store.register_control("eu-ai-act-art-13");
        store.register_control("soc2-cc7.2");
        store.attest("soc2-cc6.1", "aaabbbccc111");
        store.attest("eu-ai-act-art-13", "dddeeefff222");
        store.attest("soc2-cc7.2", "ggghhh333iii");
        let report = store.ledgrrr_report();
        assert_eq!(
            report.grade,
            ComplianceGrade::Full,
            "all 3 controls attested → Full; got: {:?}",
            report.grade
        );
        assert!(
            report.controls_pending.is_empty(),
            "no controls must remain pending"
        );
    }
}
