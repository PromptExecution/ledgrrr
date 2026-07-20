//! Typed playbook/diagram model — PRD-10 §6.2.
//!
//! A playbook is the canonical, serializable description of a pipeline that
//! `ledgrrr_render_diagram` and `ledgrrr_simulate_pipeline` operate over.
//! `BTreeMap`/`Vec` (never `HashMap`) everywhere so JSON/Mermaid output is
//! byte-for-byte deterministic for a fixed input, per PRD-10 §9 "Diagram and
//! Office" done criteria.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Start,
    Task,
    Gate,
    End,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlaybookNode {
    pub id: String,
    pub label: String,
    pub kind: NodeKind,
    /// b00t capability datum this node invokes, e.g. "ledgrrr.mcp".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub b00t_capability: Option<String>,
    /// ledgrrr evidence graph node this playbook node is backed by.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlaybookEdge {
    pub from: String,
    pub to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ApprovalGate {
    pub id: String,
    pub node_id: String,
    pub description: String,
    pub required: bool,
    /// Only meaningful under the "deterministic" simulation profile: lets a
    /// playbook author pre-declare a gate as safe to auto-clear in CI/audit
    /// replay without a human approver. Defaults to false — silence never
    /// implies approval.
    #[serde(default)]
    pub auto_approve: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RollbackAction {
    pub id: String,
    pub target_node_id: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OfficeArtifactRef {
    pub id: String,
    pub format: String,
    pub path: String,
    pub version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlaybookModel {
    pub playbook_id: String,
    pub title: String,
    pub version: String,
    pub source: String,
    pub nodes: Vec<PlaybookNode>,
    pub edges: Vec<PlaybookEdge>,
    #[serde(default)]
    pub capability_refs: Vec<String>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    pub simulation_profile: String,
    #[serde(default)]
    pub approval_gates: Vec<ApprovalGate>,
    #[serde(default)]
    pub rollback_actions: Vec<RollbackAction>,
    #[serde(default)]
    pub office_artifacts: Vec<OfficeArtifactRef>,
}

#[derive(Debug, thiserror::Error)]
pub enum PlaybookError {
    #[error("playbook has no nodes")]
    Empty,
    #[error("edge references unknown node id: {0}")]
    UnknownNode(String),
    #[error("no node of kind Start found; traversal needs an explicit entry point")]
    NoStartNode,
}

impl PlaybookModel {
    /// Adjacency list, node id -> ordered list of outgoing edge targets.
    /// `BTreeMap` keeps iteration order deterministic regardless of input
    /// edge order.
    pub fn adjacency(&self) -> BTreeMap<&str, Vec<&PlaybookEdge>> {
        let mut adj: BTreeMap<&str, Vec<&PlaybookEdge>> = BTreeMap::new();
        for edge in &self.edges {
            adj.entry(edge.from.as_str()).or_default().push(edge);
        }
        adj
    }

    pub fn node(&self, id: &str) -> Option<&PlaybookNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn validate(&self) -> Result<(), PlaybookError> {
        if self.nodes.is_empty() {
            return Err(PlaybookError::Empty);
        }
        if !self.nodes.iter().any(|n| n.kind == NodeKind::Start) {
            return Err(PlaybookError::NoStartNode);
        }
        for edge in &self.edges {
            if self.node(&edge.from).is_none() {
                return Err(PlaybookError::UnknownNode(edge.from.clone()));
            }
            if self.node(&edge.to).is_none() {
                return Err(PlaybookError::UnknownNode(edge.to.clone()));
            }
        }
        Ok(())
    }

    /// Deterministic start node: the first node (by declaration order) whose
    /// kind is `Start`. Validated by `validate()` to exist.
    pub fn start_node(&self) -> Option<&PlaybookNode> {
        self.nodes.iter().find(|n| n.kind == NodeKind::Start)
    }

    pub fn gate_for_node(&self, node_id: &str) -> Option<&ApprovalGate> {
        self.approval_gates.iter().find(|g| g.node_id == node_id)
    }
}
