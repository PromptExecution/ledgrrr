//! Typed Rust type-relationship graph and Cytoscape conversion.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::cytoscape::{
    CytoscapeEdge, CytoscapeEdgeData, CytoscapeGraph, CytoscapeNode, CytoscapeNodeData,
};

/// A Rust type node suitable for type relationship visualization.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, specta::Type)]
pub struct TypeNode {
    /// Stable type identifier, usually a fully-qualified Rust path.
    pub id: String,
    /// Display label for the type.
    pub label: String,
    /// Type category such as `struct`, `enum`, `trait`, or `type_alias`.
    pub kind: String,
    /// Optional Cytoscape compound parent ID, typically a module or namespace node.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
}

/// Supported relationship kinds between Rust types.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "snake_case")]
pub enum TypeRelationshipKind {
    Implements,
    Contains,
    DerivesFrom,
    References,
    AdvancesTo,
    Constrains,
    Verifies,
    Produces,
    ProjectsTo,
    RecordsIn,
    ClassifiedAs,
    ValidatedBy,
    Attests,
    BelongsTo,
}

impl TypeRelationshipKind {
    /// Stable label emitted into Cytoscape edge data.
    pub const fn as_label(self) -> &'static str {
        match self {
            Self::Implements => "implements",
            Self::Contains => "contains",
            Self::DerivesFrom => "derives_from",
            Self::References => "references",
            Self::AdvancesTo => "advances_to",
            Self::Constrains => "constrains",
            Self::Verifies => "verifies",
            Self::Produces => "produces",
            Self::ProjectsTo => "projects_to",
            Self::RecordsIn => "records_in",
            Self::ClassifiedAs => "classified_as",
            Self::ValidatedBy => "validated_by",
            Self::Attests => "attests",
            Self::BelongsTo => "belongs_to",
        }
    }
}

/// A directed relationship between two type nodes.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, specta::Type,
)]
pub struct TypeRelationship {
    pub source: String,
    pub target: String,
    pub kind: TypeRelationshipKind,
}

impl TypeRelationship {
    pub fn new(
        source: impl Into<String>,
        target: impl Into<String>,
        kind: TypeRelationshipKind,
    ) -> Self {
        Self {
            source: source.into(),
            target: target.into(),
            kind,
        }
    }

    fn cytoscape_id(&self) -> String {
        format!("{}__{}__{}", self.source, self.kind.as_label(), self.target)
    }
}

/// Serializable Rust type relationship graph.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct TypeRelationshipGraph {
    pub nodes: Vec<TypeNode>,
    pub relationships: Vec<TypeRelationship>,
}

impl TypeRelationshipGraph {
    pub fn new(nodes: Vec<TypeNode>, relationships: Vec<TypeRelationship>) -> Self {
        Self {
            nodes,
            relationships,
        }
    }

    /// Convert to Cytoscape elements with deterministic ordering and deduping.
    pub fn to_cytoscape(&self) -> CytoscapeGraph {
        CytoscapeGraph::from(self)
    }
}

impl From<&TypeRelationshipGraph> for CytoscapeGraph {
    fn from(graph: &TypeRelationshipGraph) -> Self {
        let canonical_nodes = graph
            .nodes
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .fold(BTreeMap::new(), |mut nodes, node| {
                nodes.entry(node.id.clone()).or_insert(node);
                nodes
            });

        let nodes: Vec<CytoscapeNode> = canonical_nodes
            .into_values()
            .map(|node| CytoscapeNode {
                data: CytoscapeNodeData {
                    id: node.id,
                    label: node.label,
                    kind: node.kind,
                    parent: node.parent_id,
                },
            })
            .collect();

        let edges: Vec<CytoscapeEdge> = graph
            .relationships
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|relationship| CytoscapeEdge {
                data: CytoscapeEdgeData {
                    id: relationship.cytoscape_id(),
                    source: relationship.source,
                    target: relationship.target,
                    label: relationship.kind.as_label().to_string(),
                },
            })
            .collect();

        CytoscapeGraph { nodes, edges }
    }
}

impl From<TypeRelationshipGraph> for CytoscapeGraph {
    fn from(graph: TypeRelationshipGraph) -> Self {
        CytoscapeGraph::from(&graph)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, label: &str, kind: &str) -> TypeNode {
        TypeNode {
            id: id.to_string(),
            label: label.to_string(),
            kind: kind.to_string(),
            parent_id: None,
        }
    }

    #[test]
    fn cytoscape_conversion_sorts_nodes_and_edges_deterministically() {
        let graph = TypeRelationshipGraph::new(
            vec![
                node("crate::B", "B", "struct"),
                node("crate::A", "A", "trait"),
            ],
            vec![
                TypeRelationship::new("crate::B", "crate::C", TypeRelationshipKind::References),
                TypeRelationship::new("crate::B", "crate::A", TypeRelationshipKind::Implements),
            ],
        );

        let cytoscape = graph.to_cytoscape();

        let node_ids: Vec<&str> = cytoscape
            .nodes
            .iter()
            .map(|node| node.data.id.as_str())
            .collect();
        let edge_ids: Vec<&str> = cytoscape
            .edges
            .iter()
            .map(|edge| edge.data.id.as_str())
            .collect();

        assert_eq!(node_ids, vec!["crate::A", "crate::B"]);
        assert_eq!(
            edge_ids,
            vec![
                "crate::B__implements__crate::A",
                "crate::B__references__crate::C",
            ]
        );
    }

    #[test]
    fn cytoscape_conversion_dedups_exact_duplicate_nodes_and_relationships() {
        let graph = TypeRelationshipGraph::new(
            vec![
                node("crate::A", "A", "struct"),
                node("crate::A", "A", "struct"),
            ],
            vec![
                TypeRelationship::new("crate::A", "crate::B", TypeRelationshipKind::Contains),
                TypeRelationship::new("crate::A", "crate::B", TypeRelationshipKind::Contains),
            ],
        );

        let cytoscape = graph.to_cytoscape();

        assert_eq!(cytoscape.nodes.len(), 1);
        assert_eq!(cytoscape.edges.len(), 1);
        assert_eq!(cytoscape.edges[0].data.id, "crate::A__contains__crate::B");
    }

    #[test]
    fn cytoscape_conversion_dedups_nodes_by_id() {
        let graph = TypeRelationshipGraph::new(
            vec![
                node("crate::A", "Zed", "struct"),
                node("crate::A", "A", "struct"),
            ],
            Vec::new(),
        );

        let cytoscape = graph.to_cytoscape();

        assert_eq!(cytoscape.nodes.len(), 1);
        assert_eq!(cytoscape.nodes[0].data.id, "crate::A");
        assert_eq!(cytoscape.nodes[0].data.label, "A");
    }

    #[test]
    fn relationship_kind_serializes_as_snake_case_label() {
        let json = serde_json::to_string(&TypeRelationshipKind::DerivesFrom).unwrap();

        assert_eq!(json, "\"derives_from\"");
        assert_eq!(TypeRelationshipKind::DerivesFrom.as_label(), "derives_from");
    }
}
