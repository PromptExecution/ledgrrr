//! Docling process surface — b00t attestation for the PDF-ingest sidecar.
//!
//! `PdfIngestOp` (in `ledger-core`) shells out to a `reqif-opa-mcp` Python
//! checkout via `uv run python -m reqif_ingest_cli extract ...` to turn a PDF
//! into a `DoclingDocumentGraph` (see `ledger-core/src/docling_bridge.rs`).
//! There is no standalone `docling` binary on `PATH` in this architecture —
//! the two hard preconditions for that subprocess to succeed are `uv` being
//! on `PATH` and the `reqif-opa-mcp` checkout existing on disk. This surface
//! is the node-level attestation of those two things, so callers can check
//! readiness before claiming `docling_ready: true` instead of hardcoding it.

use crate::core::{
    AuditRecord, GovernancePolicy, MaintenanceAction, ProcessSurface, Requirement,
    SurfaceCapability,
};
use serde::Deserialize;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
pub struct DoclingProcessSurfaceConfig {
    /// Path to a `reqif-opa-mcp` checkout (e.g. `~/promptexecution/reqif-opa-mcp`).
    #[serde(default = "default_reqif_opa_mcp_dir")]
    pub reqif_opa_mcp_dir: PathBuf,
}

fn default_reqif_opa_mcp_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join("promptexecution/reqif-opa-mcp")
}

impl Default for DoclingProcessSurfaceConfig {
    fn default() -> Self {
        Self {
            reqif_opa_mcp_dir: default_reqif_opa_mcp_dir(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DoclingError {
    #[error("uv not on PATH")]
    NotOnPath,
    #[error("reqif-opa-mcp checkout not found: {0}")]
    CheckoutMissing(String),
}

fn uv_on_path() -> bool {
    std::process::Command::new("uv")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Attests that the PDF-ingest sidecar (`uv` + a `reqif-opa-mcp` checkout) is
/// operational on this node.
#[derive(Debug, Default)]
pub struct DoclingProcessSurface {
    config: DoclingProcessSurfaceConfig,
}

impl DoclingProcessSurface {
    pub fn new() -> Self {
        Self::default()
    }

    /// True iff `uv` is on `PATH` and the configured sidecar checkout exists.
    /// This is what a caller should check before advertising `docling_ready`.
    pub fn is_ready(&self) -> bool {
        uv_on_path() && self.config.reqif_opa_mcp_dir.exists()
    }
}

impl ProcessSurface for DoclingProcessSurface {
    type Config = DoclingProcessSurfaceConfig;
    type Error = DoclingError;
    type Handle = ();

    fn capability(&self) -> SurfaceCapability {
        SurfaceCapability {
            name: "docling",
            requirements: vec![
                Requirement::BinaryOnPath("uv".into()),
                Requirement::PathExists(self.config.reqif_opa_mcp_dir.display().to_string()),
            ],
            governance: GovernancePolicy::default(),
        }
    }

    fn init(&mut self, config: Self::Config) -> Result<(), Self::Error> {
        if !uv_on_path() {
            return Err(DoclingError::NotOnPath);
        }
        if !config.reqif_opa_mcp_dir.exists() {
            return Err(DoclingError::CheckoutMissing(
                config.reqif_opa_mcp_dir.display().to_string(),
            ));
        }
        tracing::info!(
            "DoclingProcessSurface initialized: sidecar at {}",
            config.reqif_opa_mcp_dir.display()
        );
        self.config = config;
        Ok(())
    }

    fn operate(&self) -> Result<Self::Handle, Self::Error> {
        if !self.is_ready() {
            return Err(DoclingError::NotOnPath);
        }
        Ok(())
    }

    fn terminate((): Self::Handle) -> Result<AuditRecord, Self::Error> {
        Ok(AuditRecord {
            surface_name: "docling".into(),
            uptime: Duration::from_secs(0),
            exit_reason: "manual".into(),
            crash_count: 0,
            bytes_logged: 0,
        })
    }

    fn maintain(&self) -> MaintenanceAction {
        if self.is_ready() {
            MaintenanceAction::NoOp
        } else {
            MaintenanceAction::Quarantine {
                reason: "docling sidecar precondition no longer satisfied".into(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_ready_when_checkout_missing() {
        let surface = DoclingProcessSurface {
            config: DoclingProcessSurfaceConfig {
                reqif_opa_mcp_dir: PathBuf::from("/nonexistent/reqif-opa-mcp"),
            },
        };
        assert!(!surface.is_ready());
    }

    #[test]
    fn init_fails_when_checkout_missing() {
        let mut surface = DoclingProcessSurface::new();
        let result = surface.init(DoclingProcessSurfaceConfig {
            reqif_opa_mcp_dir: PathBuf::from("/nonexistent/reqif-opa-mcp"),
        });
        assert!(matches!(result, Err(DoclingError::CheckoutMissing(_))));
    }

    #[test]
    fn capability_declares_uv_and_checkout_requirements() {
        let surface = DoclingProcessSurface::new();
        let cap = surface.capability();
        assert_eq!(cap.name, "docling");
        assert!(cap
            .requirements
            .contains(&Requirement::BinaryOnPath("uv".into())));
    }
}
