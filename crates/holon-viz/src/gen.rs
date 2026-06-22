// Auto-derived via `#[derive(HolonEmit)]` on `VizDomain` in `domain.rs`.
// Do NOT hand-edit the node list — add variants to `VizDomain` instead.
// Relationships remain explicit below: they encode directed semantics that
// cannot be auto-derived from enum structure alone.
use crate::domain::VizDomain;
use crate::type_graph::{TypeRelationshipGraph, TypeRelationshipKind};
use crate::type_graph::rel;

/// Build the canonical `TypeRelationshipGraph` from the `VizDomain` manifest.
///
/// Nodes are auto-derived from `#[derive(HolonEmit)]` on `VizDomain`.
/// Relationships are declared explicitly below.
pub fn manifest_loader() -> TypeRelationshipGraph {
    let nodes = VizDomain::emit_nodes();

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
        // Tax domain relationships
        rel("au_rd::AuRdActivity", "iso::HasVisualization", TypeRelationshipKind::Implements),
        rel("au_rd::AuRdOffset", "iso::HasVisualization", TypeRelationshipKind::Implements),
        rel("us_rdc::QreActivity", "iso::HasVisualization", TypeRelationshipKind::Implements),
        rel("us_rdc::UsRdcCredit", "iso::HasVisualization", TypeRelationshipKind::Implements),
        rel("crypto::CryptoTx", "iso::HasVisualization", TypeRelationshipKind::Implements),
        rel("crypto::CryptoWallet", "iso::HasVisualization", TypeRelationshipKind::Implements),
        rel("zlayer::Constraint", "au_rd::AuRdActivity", TypeRelationshipKind::Contains),
        rel("zlayer::Constraint", "us_rdc::QreActivity", TypeRelationshipKind::Contains),
        rel("au_rd::AuRdActivity", "au_rd::AuRdOffset", TypeRelationshipKind::Produces),
        rel("us_rdc::QreActivity", "us_rdc::UsRdcCredit", TypeRelationshipKind::Produces),
        rel("zlayer::Pipeline", "crypto::CryptoWallet", TypeRelationshipKind::Contains),
        rel("crypto::CryptoWallet", "crypto::CryptoTx", TypeRelationshipKind::Contains),
    ];

    TypeRelationshipGraph::new(nodes, relationships)
}
