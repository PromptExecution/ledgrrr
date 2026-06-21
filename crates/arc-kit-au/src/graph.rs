//! Evidence graph — petgraph-backed provenance tracking.
//!
//! The graph stores evidence nodes and their provenance relationships.
//! It supports traversal queries for review and gap detection.

use petgraph::graph::DiGraph;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::edge::{EdgeType, EvidenceEdge};
use crate::missing::ProvenanceScanner;
use crate::node::{EvidenceNode, NodeId, NodeType};

/// Summary of the evidence graph's work queue state.
///
/// All counts derive from the same graph, eliminating the risk of
/// manual counter drift (TRIZ: the graph IS the work queue).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkQueueSummary {
    pub total_transactions: usize,
    pub ready_to_review: usize,
    pub blocked: usize,
    pub exported: usize,
    /// Transactions with validation issues requiring attention.
    /// Derived from ValidationIssue nodes in the graph.
    pub with_validation_issues: usize,
}

#[derive(Debug, Error)]
pub enum GraphError {
    #[error("node not found: {0}")]
    NodeNotFound(NodeId),
    #[error("duplicate node: {0}")]
    DuplicateNode(NodeId),
    #[error("invalid edge: from {from} to {to} via {edge_type}")]
    InvalidEdge {
        from: NodeId,
        to: NodeId,
        edge_type: EdgeType,
    },
}

/// Core evidence graph.
///
/// Built incrementally from ingest/classify/approval/export events.
/// Deterministic: same events produce same graph structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceGraph {
    nodes: Vec<EvidenceNode>,
    edges: Vec<EvidenceEdge>,
    #[serde(skip)]
    node_index: std::collections::HashMap<NodeId, petgraph::prelude::NodeIndex>,
    #[serde(skip)]
    graph: DiGraph<EvidenceNode, EdgeType>,
}

impl EvidenceGraph {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            node_index: std::collections::HashMap::new(),
            graph: DiGraph::new(),
        }
    }

    /// Add a node to the graph. Returns the node ID.
    pub fn add_node(&mut self, node: EvidenceNode) -> Result<NodeId, GraphError> {
        let node_id = node.node_id();
        if self.node_index.contains_key(&node_id) {
            return Err(GraphError::DuplicateNode(node_id));
        }
        let idx = self.graph.add_node(node.clone());
        self.node_index.insert(node_id.clone(), idx);
        self.nodes.push(node);
        Ok(node_id)
    }

    /// Add an edge between two nodes.
    pub fn add_edge(
        &mut self,
        from: NodeId,
        to: NodeId,
        edge_type: EdgeType,
    ) -> Result<(), GraphError> {
        let from_idx = self
            .node_index
            .get(&from)
            .ok_or(GraphError::NodeNotFound(from.clone()))?;
        let to_idx = self
            .node_index
            .get(&to)
            .ok_or(GraphError::NodeNotFound(to.clone()))?;

        self.graph.add_edge(*from_idx, *to_idx, edge_type);
        self.edges.push(EvidenceEdge::new(from, to, edge_type));
        Ok(())
    }

    /// Get node by ID.
    pub fn get_node(&self, id: &NodeId) -> Option<&EvidenceNode> {
        self.nodes.iter().find(|n| n.node_id() == *id)
    }

    /// Get all nodes of a specific type.
    pub fn nodes_of_type(&self, node_type: NodeType) -> Vec<&EvidenceNode> {
        self.nodes
            .iter()
            .filter(|n| n.node_type() == node_type)
            .collect()
    }

    /// Get all edges of a specific type.
    pub fn edges_of_type(&self, edge_type: EdgeType) -> Vec<&EvidenceEdge> {
        self.edges
            .iter()
            .filter(|e| e.edge_type == edge_type)
            .collect()
    }

    /// Find all outgoing edges from a node.
    pub fn outgoing_edges(&self, from: &NodeId) -> Vec<&EvidenceEdge> {
        self.edges.iter().filter(|e| e.from == *from).collect()
    }

    /// Find all incoming edges to a node.
    pub fn incoming_edges(&self, to: &NodeId) -> Vec<&EvidenceEdge> {
        self.edges.iter().filter(|e| e.to == *to).collect()
    }

    /// Get all nodes in the graph.
    pub fn all_nodes(&self) -> &[EvidenceNode] {
        &self.nodes
    }

    /// Get all edges in the graph.
    pub fn all_edges(&self) -> &[EvidenceEdge] {
        &self.edges
    }

    /// Get node count.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Get edge count.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Check if graph is empty.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Clear all nodes and edges.
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.edges.clear();
        self.node_index.clear();
        self.graph.clear();
    }

    /// Ensure a node exists in the graph. If it already exists, this is a no-op.
    /// Returns true if the node was newly inserted, false if it already existed.
    /// Idempotent — safe to call multiple times with the same node.
    pub fn ensure_node(&mut self, node: EvidenceNode) -> bool {
        let node_id = node.node_id();
        if self.node_index.contains_key(&node_id) {
            return false;
        }
        let idx = self.graph.add_node(node.clone());
        self.node_index.insert(node_id, idx);
        self.nodes.push(node);
        true
    }

    /// Ensure an edge exists in the graph. If either endpoint is missing, logs and returns false.
    /// Idempotent — duplicates are prevented by the underlying petgraph DiGraph semantics.
    pub fn ensure_edge(&mut self, from: NodeId, to: NodeId, edge_type: EdgeType) -> bool {
        let from_idx = match self.node_index.get(&from) {
            Some(idx) => *idx,
            None => {
                tracing::warn!("evidence: ensure_edge skipped — missing source node {from}");
                return false;
            }
        };
        let to_idx = match self.node_index.get(&to) {
            Some(idx) => *idx,
            None => {
                tracing::warn!("evidence: ensure_edge skipped — missing target node {to}");
                return false;
            }
        };
        // petgraph allows parallel edges, so we check first
        if self
            .graph
            .edges_connecting(from_idx, to_idx)
            .any(|e| *e.weight() == edge_type)
        {
            return false;
        }
        self.graph.add_edge(from_idx, to_idx, edge_type);
        self.edges.push(EvidenceEdge::new(from, to, edge_type));
        true
    }

    /// Work queue summary — a projection of the graph's incomplete chains.
    ///
    /// All three counts derive from the same evidence graph so they stay consistent.
    /// This replaces TodayQueue's manual counting of separate node types.
    pub fn work_queue_summary(&self) -> WorkQueueSummary {
        let gaps = self.find_missing_provenance();
        WorkQueueSummary {
            total_transactions: self.nodes_of_type(NodeType::Transaction).len(),
            ready_to_review: gaps.iter().filter(|g| !g.is_critical()).count(),
            blocked: gaps.iter().filter(|g| g.is_critical()).count(),
            exported: self.nodes_of_type(NodeType::WorkbookRow).len(),
            with_validation_issues: self.nodes_of_type(NodeType::ValidationIssue).len(),
        }
    }

    /// Serialize graph to JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(&serde_json::json!({
            "version": 1,
            "nodes": self.nodes,
            "edges": self.edges,
        }))
    }

    /// Deserialize graph from JSON.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let data: serde_json::Value = serde_json::from_str(json)?;
        let nodes: Vec<EvidenceNode> = serde_json::from_value(data["nodes"].clone())?;
        let edges: Vec<EvidenceEdge> = serde_json::from_value(data["edges"].clone())?;

        let mut graph = Self::new();
        for node in nodes {
            let _ = graph.add_node(node);
        }
        for edge in edges {
            let _ = graph.add_edge(edge.from.clone(), edge.to.clone(), edge.edge_type);
        }
        Ok(graph)
    }
}

impl Default for EvidenceGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{SourceDoc, Transaction};
    use chrono::TimeZone;
    use chrono::Utc;

    fn test_doc() -> SourceDoc {
        SourceDoc {
            filename: "WF--BH-CHK--2024-01--statement.pdf".to_string(),
            vendor: "WF".to_string(),
            account_id: "BH-CHK".to_string(),
            statement_date: "2024-01-31".to_string(),
            document_type: "statement".to_string(),
            content_hash: "abc123".to_string(),
            ingested_at: Utc.with_ymd_and_hms(2024, 2, 1, 10, 0, 0).unwrap(),
            raw_context_path: None,
        }
    }

    fn test_tx() -> Transaction {
        Transaction {
            tx_id: "tx_123".to_string(),
            account_id: "BH-CHK".to_string(),
            date: "2024-01-15".to_string(),
            amount: "-12.34".to_string(),
            description: "Cafe lunch".to_string(),
            source_rows: vec![],
        }
    }

    #[test]
    fn graph_starts_empty() {
        let graph = EvidenceGraph::new();
        assert!(graph.is_empty());
        assert_eq!(graph.node_count(), 0);
        assert_eq!(graph.edge_count(), 0);
    }

    #[test]
    fn add_node_increments_count() {
        let mut graph = EvidenceGraph::new();
        let doc = test_doc();
        let id = graph.add_node(EvidenceNode::SourceDoc(doc)).unwrap();
        assert_eq!(graph.node_count(), 1);
        assert!(graph.get_node(&id).is_some());
    }

    #[test]
    fn duplicate_node_returns_error() {
        let mut graph = EvidenceGraph::new();
        let doc = test_doc();
        let node = EvidenceNode::SourceDoc(doc.clone());
        let _ = graph.add_node(node.clone()).unwrap();
        let result = graph.add_node(node);
        assert!(matches!(result, Err(GraphError::DuplicateNode(_))));
    }

    #[test]
    fn add_edge_connects_nodes() {
        let mut graph = EvidenceGraph::new();
        let doc = test_doc();
        let tx = test_tx();
        let doc_id = graph.add_node(EvidenceNode::SourceDoc(doc)).unwrap();
        let tx_id = graph.add_node(EvidenceNode::Transaction(tx)).unwrap();

        graph
            .add_edge(doc_id.clone(), tx_id.clone(), EdgeType::Produces)
            .unwrap();
        assert_eq!(graph.edge_count(), 1);

        let outgoing = graph.outgoing_edges(&doc_id);
        assert_eq!(outgoing.len(), 1);
        assert_eq!(outgoing[0].edge_type, EdgeType::Produces);
    }

    #[test]
    fn edge_to_missing_node_returns_error() {
        let mut graph = EvidenceGraph::new();
        let fake_id = NodeId::new(NodeType::SourceDoc, "nonexistent");
        let result = graph.add_edge(
            fake_id.clone(),
            NodeId::new(NodeType::Transaction, "tx_123"),
            EdgeType::Produces,
        );
        assert!(matches!(result, Err(GraphError::NodeNotFound(_))));
    }

    #[test]
    fn nodes_of_type_filters_correctly() {
        let mut graph = EvidenceGraph::new();
        let doc = test_doc();
        let tx = test_tx();
        graph.add_node(EvidenceNode::SourceDoc(doc)).unwrap();
        graph.add_node(EvidenceNode::Transaction(tx)).unwrap();

        let docs = graph.nodes_of_type(NodeType::SourceDoc);
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].node_type(), NodeType::SourceDoc);

        let txs = graph.nodes_of_type(NodeType::Transaction);
        assert_eq!(txs.len(), 1);
    }

    #[test]
    fn json_roundtrip_preserves_graph() {
        let mut graph = EvidenceGraph::new();
        let doc = test_doc();
        let tx = test_tx();
        let doc_id = graph.add_node(EvidenceNode::SourceDoc(doc)).unwrap();
        let tx_id = graph.add_node(EvidenceNode::Transaction(tx)).unwrap();
        graph.add_edge(doc_id, tx_id, EdgeType::Produces).unwrap();

        let json = graph.to_json().unwrap();
        let restored = EvidenceGraph::from_json(&json).unwrap();

        assert_eq!(restored.node_count(), 2);
        assert_eq!(restored.edge_count(), 1);
    }

    #[test]
    fn clear_resets_graph() {
        let mut graph = EvidenceGraph::new();
        let doc = test_doc();
        graph.add_node(EvidenceNode::SourceDoc(doc)).unwrap();
        assert_eq!(graph.node_count(), 1);

        graph.clear();
        assert!(graph.is_empty());
        assert_eq!(graph.node_count(), 0);
    }
}
