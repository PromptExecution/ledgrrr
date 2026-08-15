//! Deterministic, non-LLM pipeline simulation — PRD-11 §6.3.
//!
//! This is the CI/audit-replay simulation mode: no model call, no wall-clock
//! dependency, same `PlaybookModel` + profile always produces the same
//! `SimulationTrace` (asserted by golden tests). A gate blocks traversal
//! unless the playbook author marked it `auto_approve` — silence never
//! implies approval, matching the PRD-11 §7 governance requirement that
//! privileged/gated steps must not be silently bypassed.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::playbook::{NodeKind, PlaybookError, PlaybookModel};

pub const DETERMINISTIC_PROFILE: &str = "deterministic";
pub const LOCAL_CPU_PROFILE: &str = "local-cpu";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Executed,
    GateBlocked,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SimulationStep {
    pub sequence: u32,
    pub node_id: String,
    pub status: StepStatus,
    pub evidence_id: String,
    /// Deterministic outcome emitted by the state. A declared node outcome is
    /// preserved verbatim; otherwise the execution status supplies the
    /// canonical fallback.
    pub outcome: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GateDecisionKind {
    Approved,
    Pending,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GateDecision {
    pub gate_id: String,
    pub node_id: String,
    pub decision: GateDecisionKind,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ResourceEstimate {
    pub estimated_steps: u32,
    /// Fixed per-step cost model for the deterministic profile (no wall
    /// clock, no model latency) — 1 logical second per executed step.
    pub estimated_seconds: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SimulationTrace {
    pub run_id: String,
    pub playbook_id: String,
    pub playbook_version: String,
    pub profile: String,
    pub steps: Vec<SimulationStep>,
    pub gate_decisions: Vec<GateDecision>,
    pub resource_estimate: ResourceEstimate,
    pub evidence_ids: Vec<String>,
}

/// Deterministic evidence id: blake3 over playbook identity + node id, never
/// wall-clock or randomness derived.
fn evidence_id(playbook_id: &str, version: &str, node_id: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(playbook_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(version.as_bytes());
    hasher.update(b"\0");
    hasher.update(node_id.as_bytes());
    hasher.finalize().to_hex().to_string()
}

/// Deterministic run id: blake3 over playbook identity + profile only (not
/// over the trace itself), so the same input always yields the same run id
/// and the id can be computed before simulation runs.
fn run_id(playbook_id: &str, version: &str, profile: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"run\0");
    hasher.update(playbook_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(version.as_bytes());
    hasher.update(b"\0");
    hasher.update(profile.as_bytes());
    hasher.finalize().to_hex().to_string()
}

pub fn simulate(model: &PlaybookModel, profile: &str) -> Result<SimulationTrace, PlaybookError> {
    model.validate()?;
    let adjacency = model.adjacency();
    // `validate()` guarantees a Start node exists.
    let start = model
        .start_node()
        .expect("validated playbook has a Start node");

    let mut steps = Vec::new();
    let mut gate_decisions = Vec::new();
    let mut evidence_ids = Vec::new();
    let mut visited = std::collections::BTreeSet::new();
    let mut frontier = vec![start.id.as_str()];
    let mut sequence = 0u32;

    while let Some(node_id) = frontier.pop() {
        if !visited.insert(node_id) {
            continue;
        }
        let Some(node) = model.node(node_id) else {
            continue;
        };

        let status = if let Some(gate) = model.gate_for_node(node_id) {
            let auto_clear =
                !gate.required || (profile == DETERMINISTIC_PROFILE && gate.auto_approve);
            if auto_clear {
                gate_decisions.push(GateDecision {
                    gate_id: gate.id.clone(),
                    node_id: node_id.to_string(),
                    decision: GateDecisionKind::Approved,
                    reason: if gate.required {
                        "auto_approve set for deterministic profile".to_string()
                    } else {
                        "gate not required".to_string()
                    },
                });
                StepStatus::Executed
            } else {
                gate_decisions.push(GateDecision {
                    gate_id: gate.id.clone(),
                    node_id: node_id.to_string(),
                    decision: GateDecisionKind::Pending,
                    reason: "required approval gate has no auto_approve — halting traversal"
                        .to_string(),
                });
                StepStatus::GateBlocked
            }
        } else {
            StepStatus::Executed
        };

        sequence += 1;
        let eid = evidence_id(&model.playbook_id, &model.version, node_id);
        evidence_ids.push(eid.clone());
        steps.push(SimulationStep {
            sequence,
            node_id: node_id.to_string(),
            status,
            evidence_id: eid,
            outcome: node.outcome.clone().unwrap_or_else(|| match status {
                StepStatus::Executed => "executed".to_string(),
                StepStatus::GateBlocked => "approval_required".to_string(),
                StepStatus::Skipped => "skipped".to_string(),
            }),
            execution_role: node.execution_role.clone(),
            capability: node.b00t_capability.clone(),
        });

        if status == StepStatus::GateBlocked || node.kind == NodeKind::End {
            continue;
        }

        if let Some(edges) = adjacency.get(node_id) {
            // Push in reverse so pop() visits edges in declared order —
            // keeps traversal deterministic for a fixed input.
            for edge in edges.iter().rev() {
                frontier.push(edge.to.as_str());
            }
        }
    }

    let estimated_steps = steps.len() as u32;
    Ok(SimulationTrace {
        run_id: run_id(&model.playbook_id, &model.version, profile),
        playbook_id: model.playbook_id.clone(),
        playbook_version: model.version.clone(),
        profile: profile.to_string(),
        steps,
        gate_decisions,
        resource_estimate: ResourceEstimate {
            estimated_steps,
            estimated_seconds: estimated_steps,
        },
        evidence_ids,
    })
}
