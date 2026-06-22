//! Evidence edge types and traversal.
//!
//! Edges represent provenance relationships between evidence nodes.

use serde::{Deserialize, Serialize};

use crate::node::NodeId;

/// Type of provenance relationship.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeType {
    /// Row was extracted from this source document
    ExtractedFrom,
    /// Row produces this transaction
    Produces,
    /// Transaction has this classification
    ClassifiedAs,
    /// Classification was proposed by model
    ProposedBy,
    /// Classification was approved by operator
    ApprovedBy,
    /// Transaction appears in workbook row
    ExportedTo,
    /// Transaction has a validation issue
    ValidatedAs,
    /// Transaction/tool was executed by an agent
    ExecutedBy,
    /// Entity satisfies the named constraint (UFO Relator arc).
    SatisfiesConstraint,
    /// Activity/expenditure is registered under the cited legislation section.
    RegisteredUnder,
}

impl EdgeType {
    pub fn label(&self) -> &'static str {
        match self {
            Self::ExtractedFrom => "extracted_from",
            Self::Produces => "produces",
            Self::ClassifiedAs => "classified_as",
            Self::ProposedBy => "proposed_by",
            Self::ApprovedBy => "approved_by",
            Self::ExportedTo => "exported_to",
            Self::ValidatedAs => "validated_as",
            Self::ExecutedBy => "executed_by",
            Self::SatisfiesConstraint => "satisfies_constraint",
            Self::RegisteredUnder => "registered_under",
        }
    }
}

impl std::fmt::Display for EdgeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// Directed edge in the evidence graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceEdge {
    pub from: NodeId,
    pub to: NodeId,
    pub edge_type: EdgeType,
}

impl EvidenceEdge {
    pub fn new(from: NodeId, to: NodeId, edge_type: EdgeType) -> Self {
        Self {
            from,
            to,
            edge_type,
        }
    }
}

/// Edge traversal utilities.
pub trait EdgeTraversal {
    /// Find all edges of a specific type from a node.
    fn edges_of_type<'a>(&'a self, from: &'a NodeId, edge_type: EdgeType) -> Vec<&'a EvidenceEdge>;

    /// Find all incoming edges to a node.
    fn incoming_edges<'a>(&'a self, to: &'a NodeId) -> Vec<&'a EvidenceEdge>;

    /// Find all outgoing edges from a node.
    fn outgoing_edges<'a>(&'a self, from: &'a NodeId) -> Vec<&'a EvidenceEdge>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_type_labels_are_stable() {
        assert_eq!(EdgeType::ExtractedFrom.label(), "extracted_from");
        assert_eq!(EdgeType::Produces.label(), "produces");
        assert_eq!(EdgeType::ClassifiedAs.label(), "classified_as");
        assert_eq!(EdgeType::ProposedBy.label(), "proposed_by");
        assert_eq!(EdgeType::ApprovedBy.label(), "approved_by");
        assert_eq!(EdgeType::ExportedTo.label(), "exported_to");
        assert_eq!(EdgeType::ValidatedAs.label(), "validated_as");
        assert_eq!(EdgeType::ExecutedBy.label(), "executed_by");
    }

    #[test]
    fn evidence_edge_serializes_correctly() {
        let edge = EvidenceEdge::new(
            NodeId::new(crate::node::NodeType::SourceDoc, "abc123"),
            NodeId::new(crate::node::NodeType::ExtractedRow, "def456"),
            EdgeType::ExtractedFrom,
        );
        let json = serde_json::to_string(&edge).unwrap();
        assert!(json.contains("extracted_from"));
        assert!(json.contains("doc:abc123"));
        assert!(json.contains("row:def456"));
    }
}
