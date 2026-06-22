//! `VizDomain` — authoritative enum of all domain types visualised in the holonic graph.
//!
//! Each variant maps 1-to-1 to a node in the `TypeRelationshipGraph`.
//! The node list is auto-derived via `#[derive(HolonEmit)]` from `b00t-reflect`.
//!
//! To add a new type:
//!   1. Add a variant with a `#[holon(...)]` attribute.
//!   2. If the type implements `HasVisualization`, supply `z_layer` + `semantic_type`.
//!   3. Add the corresponding `rel(...)` entries in `gen.rs`.
//!
//! `generated_seed()` is DELETED — callers use `VizDomain::emit_nodes()` instead.

use b00t_reflect::HolonEmit;

/// Every domain type that appears in the holonic type-relationship graph.
///
/// Unit-only enum: no runtime data. Used purely as a codegen carrier.
#[derive(HolonEmit)]
pub enum VizDomain {
    // ── ISO abstract layer ────────────────────────────────────────────────
    #[holon(id = "iso::HasVisualization", label = "HasVisualization", kind = "abstract_trait")]
    HasVisualization,

    #[holon(id = "iso::VisualizationSpec", label = "VisualizationSpec", kind = "contract_type")]
    VisualizationSpec,

    #[holon(id = "iso::ZLayer", label = "ZLayer", kind = "metamodel_enum")]
    ZLayer,

    #[holon(id = "iso::SemanticType", label = "SemanticType", kind = "metamodel_enum")]
    SemanticType,

    #[holon(id = "iso::RhaiDsl", label = "RhaiDsl", kind = "dsl_contract")]
    RhaiDsl,

    // ── Z-layer scaffolding ───────────────────────────────────────────────
    #[holon(id = "zlayer::Document", label = "Document", kind = "z_document")]
    ZDocument,

    #[holon(id = "zlayer::Pipeline", label = "Pipeline", kind = "z_pipeline")]
    ZPipeline,

    #[holon(id = "zlayer::Constraint", label = "Constraint", kind = "z_constraint")]
    ZConstraint,

    #[holon(id = "zlayer::Legal", label = "Legal", kind = "z_legal")]
    ZLegal,

    #[holon(id = "zlayer::FormalProof", label = "FormalProof", kind = "z_proof")]
    ZFormalProof,

    #[holon(id = "zlayer::Attestation", label = "Attestation", kind = "z_attestation")]
    ZAttestation,

    // ── Pipeline states (HasVisualization impls) ──────────────────────────
    #[holon(id = "pipeline::PipelineState<Ingested>", label = "PipelineState<Ingested>",
            kind = "pipeline_state", z_layer = "Pipeline", semantic_type = "Pipeline")]
    PipelineStateIngested,

    #[holon(id = "pipeline::PipelineState<Validated>", label = "PipelineState<Validated>",
            kind = "pipeline_state", z_layer = "Pipeline", semantic_type = "Pipeline")]
    PipelineStateValidated,

    #[holon(id = "pipeline::PipelineState<Classified>", label = "PipelineState<Classified>",
            kind = "pipeline_state", z_layer = "Pipeline", semantic_type = "Pipeline")]
    PipelineStateClassified,

    #[holon(id = "pipeline::PipelineState<Reconciled>", label = "PipelineState<Reconciled>",
            kind = "pipeline_state", z_layer = "Pipeline", semantic_type = "Pipeline")]
    PipelineStateReconciled,

    #[holon(id = "pipeline::PipelineState<Committed>", label = "PipelineState<Committed>",
            kind = "pipeline_state", z_layer = "Pipeline", semantic_type = "Pipeline")]
    PipelineStateCommitted,

    #[holon(id = "pipeline::PipelineState<NeedsReview>", label = "PipelineState<NeedsReview>",
            kind = "review_state", z_layer = "Pipeline", semantic_type = "Pipeline")]
    PipelineStateNeedsReview,

    #[holon(id = "pipeline::MetaCtx", label = "MetaCtx",
            kind = "meta_type", z_layer = "Pipeline", semantic_type = "Pipeline")]
    MetaCtx,

    #[holon(id = "pipeline::KasuariSolver", label = "KasuariSolver",
            kind = "solver_type", z_layer = "FormalProof", semantic_type = "Proof")]
    KasuariSolver,

    // ── Validation (HasVisualization impls) ───────────────────────────────
    #[holon(id = "validation::CommitGate", label = "CommitGate",
            kind = "gate_type", z_layer = "Pipeline", semantic_type = "Gate")]
    CommitGate,

    #[holon(id = "validation::StageResult<T>", label = "StageResult<T>",
            kind = "validation_type", z_layer = "Pipeline", semantic_type = "Result")]
    StageResult,

    #[holon(id = "validation::Issue", label = "Issue",
            kind = "issue_type", z_layer = "Constraint", semantic_type = "Issue")]
    Issue,

    #[holon(id = "validation::MetaFlag", label = "MetaFlag",
            kind = "flag_type", z_layer = "Pipeline", semantic_type = "Flag")]
    MetaFlag,

    #[holon(id = "validation::Disposition", label = "Disposition",
            kind = "result_type", z_layer = "Pipeline", semantic_type = "Result")]
    Disposition,

    // ── Constraints (HasVisualization impls) ──────────────────────────────
    #[holon(id = "constraints::VendorConstraintSet", label = "VendorConstraintSet",
            kind = "constraint_type", z_layer = "Constraint", semantic_type = "Constraint")]
    VendorConstraintSet,

    #[holon(id = "constraints::ConstraintEvaluation", label = "ConstraintEvaluation",
            kind = "result_type", z_layer = "Constraint", semantic_type = "Result")]
    ConstraintEvaluation,

    #[holon(id = "constraints::InvoiceConstraintSolver", label = "InvoiceConstraintSolver",
            kind = "solver_type", z_layer = "Constraint", semantic_type = "Solver")]
    InvoiceConstraintSolver,

    #[holon(id = "constraints::InvoiceVerification", label = "InvoiceVerification",
            kind = "result_type", z_layer = "Constraint", semantic_type = "Result")]
    InvoiceVerification,

    // ── Legal (HasVisualization impls) ────────────────────────────────────
    #[holon(id = "legal::Jurisdiction", label = "Jurisdiction",
            kind = "legal_type", z_layer = "Legal", semantic_type = "Legal")]
    Jurisdiction,

    #[holon(id = "legal::LegalRule", label = "LegalRule",
            kind = "legal_type", z_layer = "Legal", semantic_type = "Legal")]
    LegalRule,

    #[holon(id = "legal::TransactionFacts", label = "TransactionFacts",
            kind = "fact_type", z_layer = "Legal", semantic_type = "Legal")]
    TransactionFacts,

    #[holon(id = "legal::LegalSolver", label = "LegalSolver",
            kind = "solver_type", z_layer = "Legal", semantic_type = "Solver")]
    LegalSolver,

    #[holon(id = "legal::Z3Result", label = "Z3Result",
            kind = "proof_result", z_layer = "Legal", semantic_type = "Result")]
    Z3Result,

    // ── Attestation ───────────────────────────────────────────────────────
    #[holon(id = "attest::AttestationSpec", label = "AttestationSpec",
            kind = "attestation_type", z_layer = "Attestation", semantic_type = "Attestation")]
    AttestationSpec,

    // ── Ontology ──────────────────────────────────────────────────────────
    #[holon(id = "ontology::ArtifactKind", label = "ArtifactKind", kind = "ontology_enum")]
    ArtifactKind,

    #[holon(id = "ontology::RelationKind", label = "RelationKind", kind = "ontology_enum")]
    RelationKind,

    #[holon(id = "ontology::OntologySnapshot", label = "OntologySnapshot", kind = "ontology_snapshot")]
    OntologySnapshot,

    // ── ARC-kit-AU evidence graph ─────────────────────────────────────────
    #[holon(id = "arc_kit_au::EvidenceGraph", label = "EvidenceGraph", kind = "evidence_graph")]
    EvidenceGraph,

    #[holon(id = "arc_kit_au::NodeType", label = "NodeType", kind = "ontology_enum")]
    NodeType,

    #[holon(id = "arc_kit_au::SourceDoc", label = "SourceDoc", kind = "evidence_node")]
    SourceDoc,

    #[holon(id = "arc_kit_au::ExtractedRow", label = "ExtractedRow", kind = "evidence_node")]
    ExtractedRow,

    #[holon(id = "arc_kit_au::Transaction", label = "Transaction", kind = "evidence_node")]
    Transaction,

    #[holon(id = "arc_kit_au::Classification", label = "Classification", kind = "evidence_node")]
    Classification,

    #[holon(id = "arc_kit_au::ModelProposal", label = "ModelProposal", kind = "evidence_node")]
    ModelProposal,

    #[holon(id = "arc_kit_au::OperatorApproval", label = "OperatorApproval", kind = "evidence_node")]
    OperatorApproval,

    #[holon(id = "arc_kit_au::WorkbookRow", label = "WorkbookRow", kind = "evidence_node")]
    WorkbookRow,

    // ── Workbook / classify / workflow ────────────────────────────────────
    #[holon(id = "workbook::TxProjectionRow", label = "TxProjectionRow", kind = "workbook_projection")]
    TxProjectionRow,

    #[holon(id = "classify::TaxCategory", label = "TaxCategory", kind = "taxonomy_type")]
    TaxCategory,

    #[holon(id = "workflow::WorkflowToml", label = "WorkflowToml", kind = "workflow_type")]
    WorkflowToml,

    // ── Tax domain — AU R&D (HasVisualization impls) ──────────────────────
    #[holon(id = "au_rd::AuRdActivity", label = "AuRdActivity",
            kind = "rd_activity", z_layer = "Constraint", semantic_type = "Constraint")]
    AuRdActivity,

    #[holon(id = "au_rd::AuRdOffset", label = "AuRdOffset",
            kind = "rd_offset", z_layer = "Constraint", semantic_type = "Result")]
    AuRdOffset,

    // ── Tax domain — US R&D Credit (HasVisualization impls) ───────────────
    #[holon(id = "us_rdc::QreActivity", label = "QreActivity",
            kind = "qre_activity", z_layer = "Constraint", semantic_type = "Constraint")]
    QreActivity,

    #[holon(id = "us_rdc::UsRdcCredit", label = "UsRdcCredit",
            kind = "rdc_credit", z_layer = "Constraint", semantic_type = "Result")]
    UsRdcCredit,

    // ── Tax domain — Crypto (HasVisualization impls) ──────────────────────
    #[holon(id = "crypto::CryptoTx", label = "CryptoTx",
            kind = "crypto_tx", z_layer = "Pipeline", semantic_type = "Pipeline")]
    CryptoTx,

    #[holon(id = "crypto::CryptoWallet", label = "CryptoWallet",
            kind = "crypto_wallet", z_layer = "Pipeline", semantic_type = "Pipeline")]
    CryptoWallet,
}
