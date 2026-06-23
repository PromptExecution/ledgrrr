//! Typed Rust type-relationship graph and Cytoscape conversion.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::cytoscape::{
    CytoscapeEdge, CytoscapeEdgeData, CytoscapeGraph, CytoscapeNode, CytoscapeNodeData,
};

/// A Rust type node suitable for type relationship visualization.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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
    /// `ZLayer` variant from `HasVisualization::viz_spec()`, if the type implements that trait.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub z_layer: Option<String>,
    /// `SemanticType` variant from `HasVisualization::viz_spec()`, if the type implements that trait.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_type: Option<String>,
}

/// Supported relationship kinds between Rust types.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
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
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
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
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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

    /// Canonical seed graph loaded from the `VizDomain` manifest in `domain.rs`.
    ///
    /// Nodes are auto-derived via `#[derive(HolonEmit)]` — do not edit `gen.rs` by hand.
    /// Add new types as variants to `VizDomain` and add relationships to `gen::manifest_loader`.
    pub fn seed() -> Self {
        crate::gen::manifest_loader()
    }
}

impl From<b00t_reflect_types::HolonNode> for TypeNode {
    fn from(n: b00t_reflect_types::HolonNode) -> Self {
        TypeNode {
            id: n.id,
            label: n.label,
            kind: n.kind,
            parent_id: None,
            z_layer: n.z_layer,
            semantic_type: n.semantic_type,
        }
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
                    z_layer: node.z_layer,
                    semantic_type: node.semantic_type,
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
#[allow(dead_code)]
pub(crate) fn type_node(id: &str, label: &str, kind: &str) -> TypeNode {
    TypeNode {
        id: id.to_string(),
        label: label.to_string(),
        kind: kind.to_string(),
        parent_id: None,
        z_layer: None,
        semantic_type: None,
    }
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn typed_node(id: &str, label: &str, kind: &str, z_layer: &str, semantic_type: &str) -> TypeNode {
    TypeNode {
        id: id.to_string(),
        label: label.to_string(),
        kind: kind.to_string(),
        parent_id: None,
        z_layer: Some(z_layer.to_string()),
        semantic_type: Some(semantic_type.to_string()),
    }
}

pub(crate) fn rel(source: &str, target: &str, kind: TypeRelationshipKind) -> TypeRelationship {
    TypeRelationship::new(source, target, kind)
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
            z_layer: None,
            semantic_type: None,
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
    #[test]
    fn seed_typed_nodes_cover_all_has_visualization_impls() {
        let seed = TypeRelationshipGraph::seed();
        let typed_ids: std::collections::HashSet<&str> = seed
            .nodes
            .iter()
            .filter(|n| n.z_layer.is_some())
            .map(|n| n.id.as_str())
            .collect();

        let expected: &[&str] = &[
            "pipeline::PipelineState<Ingested>",
            "pipeline::PipelineState<Validated>",
            "pipeline::PipelineState<Classified>",
            "pipeline::PipelineState<Reconciled>",
            "pipeline::PipelineState<Committed>",
            "pipeline::PipelineState<NeedsReview>",
            "validation::CommitGate",
            "validation::StageResult<T>",
            "validation::Issue",
            "validation::MetaFlag",
            "pipeline::MetaCtx",
            "validation::Disposition",
            "constraints::VendorConstraintSet",
            "constraints::ConstraintEvaluation",
            "constraints::InvoiceConstraintSolver",
            "constraints::InvoiceVerification",
            "legal::Jurisdiction",
            "legal::LegalRule",
            "legal::TransactionFacts",
            "legal::LegalSolver",
            "legal::Z3Result",
            "pipeline::KasuariSolver",
            "attest::AttestationSpec",
            // Tax domain types (HasVisualization impls added in gh#517)
            "au_rd::AuRdActivity",
            "au_rd::AuRdOffset",
            "us_rdc::QreActivity",
            "us_rdc::UsRdcCredit",
            "crypto::CryptoTx",
            "crypto::CryptoWallet",
        ];

        let missing: Vec<&str> = expected
            .iter()
            .copied()
            .filter(|id| !typed_ids.contains(id))
            .collect();
        assert!(
            missing.is_empty(),
            "seed() is missing typed nodes for HasVisualization impls: {:?}",
            missing
        );
    }
}
