//! `ledgrrr_export_office_artifact` — local versioned bundle only.
//!
//! PRD-11 §9 requires "Diagram refresh never silently mutates a published
//! artifact without creating a new version/evidence node." Phase 1 has no
//! OneNote/SharePoint tenant to publish to, so this writes a local,
//! version-numbered bundle (mermaid + svg + playbook json + provenance) that
//! a future Office/SPFx bridge can pick up — publishing itself is out of
//! scope until a real tenant is available.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::playbook::PlaybookModel;
use crate::render::{self, RenderError};
use crate::state;

#[derive(Debug, thiserror::Error)]
pub enum OfficeArtifactError {
    #[error(transparent)]
    Render(#[from] RenderError),
    #[error("io error writing artifact bundle: {0}")]
    Io(#[from] std::io::Error),
    #[error("io error writing artifact bundle: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Provenance {
    pub playbook_id: String,
    pub playbook_version: String,
    pub artifact_version: u32,
    pub source: String,
    pub render_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OfficeArtifactBundle {
    pub artifact_version: u32,
    pub bundle_dir: String,
    pub mermaid_path: String,
    pub svg_path: String,
    pub playbook_json_path: String,
    pub provenance_path: String,
}

fn artifacts_root(playbook_id: &str) -> PathBuf {
    state::state_dir().join("artifacts").join(playbook_id)
}

/// Next version number: one past the highest existing `vN` directory. Starts
/// at 1 so the first export is always "v1", never "v0".
fn next_version(playbook_id: &str) -> u32 {
    let root = artifacts_root(playbook_id);
    let Ok(entries) = std::fs::read_dir(&root) else {
        return 1;
    };
    entries
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter_map(|name| name.strip_prefix('v').and_then(|n| n.parse::<u32>().ok()))
        .max()
        .map(|n| n + 1)
        .unwrap_or(1)
}

pub fn export(model: &PlaybookModel) -> Result<OfficeArtifactBundle, OfficeArtifactError> {
    let mermaid = render::render_mermaid(model)?;
    let svg = render::render_svg(model)?;
    let json = render::render_json(model)?;
    let render_hash = blake3::hash(json.as_bytes()).to_hex().to_string();

    let version = next_version(&model.playbook_id);
    let bundle_dir = artifacts_root(&model.playbook_id).join(format!("v{version}"));
    std::fs::create_dir_all(&bundle_dir)?;

    let mermaid_path = bundle_dir.join("diagram.mmd");
    let svg_path = bundle_dir.join("diagram.svg");
    let json_path = bundle_dir.join("playbook.json");
    let provenance_path = bundle_dir.join("provenance.json");

    std::fs::write(&mermaid_path, &mermaid)?;
    std::fs::write(&svg_path, &svg)?;
    std::fs::write(&json_path, &json)?;

    let provenance = Provenance {
        playbook_id: model.playbook_id.clone(),
        playbook_version: model.version.clone(),
        artifact_version: version,
        source: model.source.clone(),
        render_hash,
    };
    std::fs::write(&provenance_path, serde_json::to_string_pretty(&provenance)?)?;

    Ok(OfficeArtifactBundle {
        artifact_version: version,
        bundle_dir: bundle_dir.display().to_string(),
        mermaid_path: mermaid_path.display().to_string(),
        svg_path: svg_path.display().to_string(),
        playbook_json_path: json_path.display().to_string(),
        provenance_path: provenance_path.display().to_string(),
    })
}
