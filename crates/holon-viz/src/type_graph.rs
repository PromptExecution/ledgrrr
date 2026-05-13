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
    /// `ZLayer` variant from `HasVisualization::viz_spec()`, if the type implements that trait.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub z_layer: Option<String>,
    /// `SemanticType` variant from `HasVisualization::viz_spec()`, if the type implements that trait.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_type: Option<String>,
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

    /// Canonical seed graph derived from `HasVisualization` impls in `ledger-core`.
    ///
    /// Nodes annotated with `z_layer`/`semantic_type` correspond to the 21 types that
    /// implement `HasVisualization` in `crates/ledger-core/src/iso_objects.rs`.
    pub fn seed() -> Self {
        let nodes = vec![
            type_node("iso::HasVisualization", "HasVisualization", "abstract_trait"),
            type_node("iso::VisualizationSpec", "VisualizationSpec", "contract_type"),
            type_node("iso::ZLayer", "ZLayer", "metamodel_enum"),
            type_node("iso::SemanticType", "SemanticType", "metamodel_enum"),
            type_node("iso::RhaiDsl", "RhaiDsl", "dsl_contract"),
            type_node("zlayer::Document", "Document", "z_document"),
            type_node("zlayer::Pipeline", "Pipeline", "z_pipeline"),
            type_node("zlayer::Constraint", "Constraint", "z_constraint"),
            type_node("zlayer::Legal", "Legal", "z_legal"),
            type_node("zlayer::FormalProof", "FormalProof", "z_proof"),
            type_node("zlayer::Attestation", "Attestation", "z_attestation"),
            typed_node("pipeline::PipelineState<Ingested>", "PipelineState<Ingested>", "pipeline_state", "Pipeline", "Pipeline"),
            typed_node("pipeline::PipelineState<Validated>", "PipelineState<Validated>", "pipeline_state", "Pipeline", "Pipeline"),
            typed_node("pipeline::PipelineState<Classified>", "PipelineState<Classified>", "pipeline_state", "Pipeline", "Pipeline"),
            typed_node("pipeline::PipelineState<Reconciled>", "PipelineState<Reconciled>", "pipeline_state", "Pipeline", "Pipeline"),
            typed_node("pipeline::PipelineState<Committed>", "PipelineState<Committed>", "pipeline_state", "Pipeline", "Pipeline"),
            typed_node("pipeline::PipelineState<NeedsReview>", "PipelineState<NeedsReview>", "review_state", "Pipeline", "Pipeline"),
            typed_node("validation::CommitGate", "CommitGate", "gate_type", "Pipeline", "Gate"),
            typed_node("validation::StageResult<T>", "StageResult<T>", "validation_type", "Pipeline", "Result"),
            typed_node("validation::Issue", "Issue", "issue_type", "Constraint", "Issue"),
            typed_node("validation::MetaFlag", "MetaFlag", "flag_type", "Pipeline", "Flag"),
            typed_node("constraints::VendorConstraintSet", "VendorConstraintSet", "constraint_type", "Constraint", "Constraint"),
            typed_node("constraints::ConstraintEvaluation", "ConstraintEvaluation", "result_type", "Constraint", "Result"),
            typed_node("constraints::InvoiceConstraintSolver", "InvoiceConstraintSolver", "solver_type", "Constraint", "Solver"),
            typed_node("constraints::InvoiceVerification", "InvoiceVerification", "result_type", "Constraint", "Result"),
            typed_node("legal::Jurisdiction", "Jurisdiction", "legal_type", "Legal", "Legal"),
            typed_node("legal::LegalRule", "LegalRule", "legal_type", "Legal", "Legal"),
            typed_node("legal::TransactionFacts", "TransactionFacts", "fact_type", "Legal", "Legal"),
            typed_node("legal::LegalSolver", "LegalSolver", "solver_type", "Legal", "Solver"),
            typed_node("legal::Z3Result", "Z3Result", "proof_result", "Legal", "Result"),
            typed_node("pipeline::KasuariSolver", "KasuariSolver", "solver_type", "FormalProof", "Proof"),
            typed_node("attest::AttestationSpec", "AttestationSpec", "attestation_type", "Attestation", "Attestation"),
            typed_node("pipeline::MetaCtx", "MetaCtx", "meta_type", "Pipeline", "Pipeline"),
            typed_node("validation::Disposition", "Disposition", "result_type", "Pipeline", "Result"),
            type_node("ontology::ArtifactKind", "ArtifactKind", "ontology_enum"),
            type_node("ontology::RelationKind", "RelationKind", "ontology_enum"),
            type_node("ontology::OntologySnapshot", "OntologySnapshot", "ontology_snapshot"),
            type_node("arc_kit_au::EvidenceGraph", "EvidenceGraph", "evidence_graph"),
            type_node("arc_kit_au::NodeType", "NodeType", "ontology_enum"),
            type_node("arc_kit_au::SourceDoc", "SourceDoc", "evidence_node"),
            type_node("arc_kit_au::ExtractedRow", "ExtractedRow", "evidence_node"),
            type_node("arc_kit_au::Transaction", "Transaction", "evidence_node"),
            type_node("arc_kit_au::Classification", "Classification", "evidence_node"),
            type_node("arc_kit_au::ModelProposal", "ModelProposal", "evidence_node"),
            type_node("arc_kit_au::OperatorApproval", "OperatorApproval", "evidence_node"),
            type_node("arc_kit_au::WorkbookRow", "WorkbookRow", "evidence_node"),
            type_node("workbook::TxProjectionRow", "TxProjectionRow", "workbook_projection"),
            type_node("classify::TaxCategory", "TaxCategory", "taxonomy_type"),
            type_node("workflow::WorkflowToml", "WorkflowToml", "workflow_type"),
        ];

        let relationships = vec![
            rel("iso::VisualizationSpec", "iso::HasVisualization", TypeRelationshipKind::Implements),
            rel("iso::VisualizationSpec", "iso::ZLayer", TypeRelationshipKind::Contains),
            rel("iso::VisualizationSpec", "iso::SemanticType", TypeRelationshipKind::Contains),
            rel("iso::VisualizationSpec", "iso::RhaiDsl", TypeRelationshipKind::Contains),
            rel("pipeline::PipelineState<Ingested>", "iso::HasVisualization", TypeRelationshipKind::Implements),
            rel("pipeline::PipelineState<Validated>", "iso::HasVisualization", TypeRelationshipKind::Implements),
            rel("pipeline::PipelineState<Classified>", "iso::HasVisualization", TypeRelationshipKind::Implements),
            rel("pipeline::PipelineState<Reconciled>", "iso::HasVisualization", TypeRelationshipKind::Implements),
            rel("pipeline::PipelineState<Committed>", "iso::HasVisualization", TypeRelationshipKind::Implements),
            rel("pipeline::PipelineState<NeedsReview>", "iso::HasVisualization", TypeRelationshipKind::Implements),
            rel("constraints::VendorConstraintSet", "iso::HasVisualization", TypeRelationshipKind::Implements),
            rel("constraints::ConstraintEvaluation", "iso::HasVisualization", TypeRelationshipKind::Implements),
            rel("constraints::InvoiceConstraintSolver", "iso::HasVisualization", TypeRelationshipKind::Implements),
            rel("constraints::InvoiceVerification", "iso::HasVisualization", TypeRelationshipKind::Implements),
            rel("legal::LegalRule", "iso::HasVisualization", TypeRelationshipKind::Implements),
            rel("legal::LegalSolver", "iso::HasVisualization", TypeRelationshipKind::Implements),
            rel("legal::Z3Result", "iso::HasVisualization", TypeRelationshipKind::Implements),
            rel("validation::CommitGate", "iso::HasVisualization", TypeRelationshipKind::Implements),
            rel("pipeline::MetaCtx", "iso::HasVisualization", TypeRelationshipKind::Implements),
            rel("validation::Disposition", "iso::HasVisualization", TypeRelationshipKind::Implements),
            rel("pipeline::PipelineState<Ingested>", "pipeline::PipelineState<Validated>", TypeRelationshipKind::AdvancesTo),
            rel("pipeline::PipelineState<Validated>", "pipeline::PipelineState<Classified>", TypeRelationshipKind::AdvancesTo),
            rel("pipeline::PipelineState<Classified>", "pipeline::PipelineState<Reconciled>", TypeRelationshipKind::AdvancesTo),
            rel("pipeline::PipelineState<Reconciled>", "validation::CommitGate", TypeRelationshipKind::AdvancesTo),
            rel("validation::CommitGate", "pipeline::PipelineState<Committed>", TypeRelationshipKind::AdvancesTo),
            rel("validation::CommitGate", "pipeline::PipelineState<NeedsReview>", TypeRelationshipKind::ValidatedBy),
            rel("constraints::VendorConstraintSet", "constraints::ConstraintEvaluation", TypeRelationshipKind::Produces),
            rel("constraints::InvoiceConstraintSolver", "constraints::InvoiceVerification", TypeRelationshipKind::Verifies),
            rel("constraints::ConstraintEvaluation", "validation::Issue", TypeRelationshipKind::Produces),
            rel("validation::Issue", "validation::StageResult<T>", TypeRelationshipKind::Contains),
            rel("validation::StageResult<T>", "validation::CommitGate", TypeRelationshipKind::ValidatedBy),
            rel("legal::Jurisdiction", "legal::LegalRule", TypeRelationshipKind::Contains),
            rel("legal::TransactionFacts", "legal::LegalSolver", TypeRelationshipKind::References),
            rel("legal::LegalRule", "legal::LegalSolver", TypeRelationshipKind::References),
            rel("legal::LegalSolver", "legal::Z3Result", TypeRelationshipKind::Verifies),
            rel("legal::Z3Result", "validation::Issue", TypeRelationshipKind::Produces),
            rel("pipeline::KasuariSolver", "constraints::ConstraintEvaluation", TypeRelationshipKind::Constrains),
            rel("legal::Z3Result", "attest::AttestationSpec", TypeRelationshipKind::Attests),
            rel("workflow::WorkflowToml", "pipeline::PipelineState<Ingested>", TypeRelationshipKind::References),
            rel("workflow::WorkflowToml", "pipeline::PipelineState<Committed>", TypeRelationshipKind::References),
            rel("ontology::OntologySnapshot", "ontology::ArtifactKind", TypeRelationshipKind::Contains),
            rel("ontology::OntologySnapshot", "ontology::RelationKind", TypeRelationshipKind::Contains),
            rel("ontology::RelationKind", "arc_kit_au::EvidenceGraph", TypeRelationshipKind::ProjectsTo),
            rel("arc_kit_au::EvidenceGraph", "arc_kit_au::NodeType", TypeRelationshipKind::Contains),
            rel("arc_kit_au::SourceDoc", "arc_kit_au::ExtractedRow", TypeRelationshipKind::Produces),
            rel("arc_kit_au::ExtractedRow", "arc_kit_au::Transaction", TypeRelationshipKind::Produces),
            rel("arc_kit_au::Transaction", "arc_kit_au::Classification", TypeRelationshipKind::ClassifiedAs),
            rel("arc_kit_au::Classification", "arc_kit_au::ModelProposal", TypeRelationshipKind::ValidatedBy),
            rel("arc_kit_au::ModelProposal", "arc_kit_au::OperatorApproval", TypeRelationshipKind::ValidatedBy),
            rel("arc_kit_au::Transaction", "arc_kit_au::WorkbookRow", TypeRelationshipKind::ProjectsTo),
            rel("arc_kit_au::WorkbookRow", "workbook::TxProjectionRow", TypeRelationshipKind::ProjectsTo),
            rel("workbook::TxProjectionRow", "classify::TaxCategory", TypeRelationshipKind::ClassifiedAs),
            rel("arc_kit_au::EvidenceGraph", "workbook::TxProjectionRow", TypeRelationshipKind::RecordsIn),
            rel("zlayer::Document", "arc_kit_au::SourceDoc", TypeRelationshipKind::Contains),
            rel("zlayer::Pipeline", "pipeline::PipelineState<Ingested>", TypeRelationshipKind::Contains),
            rel("zlayer::Constraint", "constraints::VendorConstraintSet", TypeRelationshipKind::Contains),
            rel("zlayer::Legal", "legal::LegalRule", TypeRelationshipKind::Contains),
            rel("zlayer::FormalProof", "legal::Z3Result", TypeRelationshipKind::Contains),
            rel("zlayer::Attestation", "attest::AttestationSpec", TypeRelationshipKind::Contains),
        ];

        Self::new(nodes, relationships)
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

fn type_node(id: &str, label: &str, kind: &str) -> TypeNode {
    TypeNode {
        id: id.to_string(),
        label: label.to_string(),
        kind: kind.to_string(),
        parent_id: None,
        z_layer: None,
        semantic_type: None,
    }
}

fn typed_node(id: &str, label: &str, kind: &str, z_layer: &str, semantic_type: &str) -> TypeNode {
    TypeNode {
        id: id.to_string(),
        label: label.to_string(),
        kind: kind.to_string(),
        parent_id: None,
        z_layer: Some(z_layer.to_string()),
        semantic_type: Some(semantic_type.to_string()),
    }
}

fn rel(source: &str, target: &str, kind: TypeRelationshipKind) -> TypeRelationship {
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
