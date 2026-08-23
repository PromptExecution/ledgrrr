//! Export the VisualizationSpec JSON manifest for the docs UI.
//!
//! Collects `HasVisualization::viz_spec()` from all 31 domain types (every
//! `impl HasVisualization` in `ledger_core::iso_objects`, with the generic
//! `StageResult<T>` represented once via `StageResult<()>`) and writes a
//! `VizManifest` JSON file to the specified output path.

use std::path::Path;

use arc_kit_au::node::{Cost, Decision, Requirement};
use ledger_core::{
    au_rd::{AuRdActivity, AuRdOffset},
    constraints::{
        ConstraintEvaluation, InvoiceConstraintSolver, InvoiceVerification, VendorConstraintSet,
    },
    crypto::{CryptoTx, CryptoWallet},
    iso::{HasVisualization, VizManifest, VizManifestEntry},
    legal::{Jurisdiction, LegalRule, LegalSolver, TransactionFacts, Z3Result},
    pipeline::{
        Classified, Committed, Ingested, KasuariSolver, NeedsReview, PipelineState, Reconciled,
        Validated,
    },
    us_rdc::{QreActivity, UsRdcCredit},
    validation::{CommitGate, Disposition, Issue, MetaCtx, MetaFlag, StageResult},
};

pub fn export_viz_manifest(output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let objects: Vec<VizManifestEntry> = vec![
        VizManifestEntry::new(
            "PipelineState<Ingested>",
            PipelineState::<Ingested>::viz_spec(),
        ),
        VizManifestEntry::new(
            "PipelineState<Validated>",
            PipelineState::<Validated>::viz_spec(),
        ),
        VizManifestEntry::new(
            "PipelineState<Classified>",
            PipelineState::<Classified>::viz_spec(),
        ),
        VizManifestEntry::new(
            "PipelineState<Reconciled>",
            PipelineState::<Reconciled>::viz_spec(),
        ),
        VizManifestEntry::new(
            "PipelineState<Committed>",
            PipelineState::<Committed>::viz_spec(),
        ),
        VizManifestEntry::new(
            "PipelineState<NeedsReview>",
            PipelineState::<NeedsReview>::viz_spec(),
        ),
        VizManifestEntry::new("ConstraintEvaluation", ConstraintEvaluation::viz_spec()),
        VizManifestEntry::new("VendorConstraintSet", VendorConstraintSet::viz_spec()),
        VizManifestEntry::new(
            "InvoiceConstraintSolver",
            InvoiceConstraintSolver::viz_spec(),
        ),
        VizManifestEntry::new("InvoiceVerification", InvoiceVerification::viz_spec()),
        VizManifestEntry::new("Z3Result", Z3Result::viz_spec()),
        VizManifestEntry::new("LegalRule", LegalRule::viz_spec()),
        VizManifestEntry::new("LegalSolver", LegalSolver::viz_spec()),
        VizManifestEntry::new("Jurisdiction", Jurisdiction::viz_spec()),
        VizManifestEntry::new("TransactionFacts", TransactionFacts::viz_spec()),
        VizManifestEntry::new("CommitGate", CommitGate::viz_spec()),
        VizManifestEntry::new("Issue", Issue::viz_spec()),
        VizManifestEntry::new("MetaFlag", MetaFlag::viz_spec()),
        VizManifestEntry::new("StageResult<()>", StageResult::<()>::viz_spec()),
        VizManifestEntry::new("MetaCtx", MetaCtx::viz_spec()),
        VizManifestEntry::new("Disposition", Disposition::viz_spec()),
        VizManifestEntry::new("KasuariSolver", KasuariSolver::viz_spec()),
        VizManifestEntry::new("AuRdActivity", AuRdActivity::viz_spec()),
        VizManifestEntry::new("AuRdOffset", AuRdOffset::viz_spec()),
        VizManifestEntry::new("QreActivity", QreActivity::viz_spec()),
        VizManifestEntry::new("UsRdcCredit", UsRdcCredit::viz_spec()),
        VizManifestEntry::new("CryptoTx", CryptoTx::viz_spec()),
        VizManifestEntry::new("CryptoWallet", CryptoWallet::viz_spec()),
        VizManifestEntry::new("Requirement", Requirement::viz_spec()),
        VizManifestEntry::new("Decision", Decision::viz_spec()),
        VizManifestEntry::new("Cost", Cost::viz_spec()),
    ];

    let count = objects.len();
    let manifest = VizManifest {
        version: env!("CARGO_PKG_VERSION").to_string(),
        objects,
    };

    let json = serde_json::to_string_pretty(&manifest)?;

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(output, &json)?;

    println!(
        "wrote viz manifest: {} ({} objects)",
        output.display(),
        count
    );

    Ok(())
}
