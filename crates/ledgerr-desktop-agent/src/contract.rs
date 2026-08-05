//! `ledgrrr_*` tool contract — names, JSON schemas, and dispatch.
//!
//! Mirrors the hand-rolled stdio JSON-RPC pattern already used by
//! `ledgerr-mcp-server` (`crates/ledgerr-mcp/src/bin/ledgerr-mcp-server.rs`):
//! Rust structs are the single source of truth for `tools/list` schemas,
//! `tools/call` dispatches on the tool name literal.

use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::install_plan;
use crate::office_artifact;
use crate::playbook::PlaybookModel;
use crate::render;
use crate::service_control;
use crate::simulate;
use crate::status;

pub const STATUS_TOOL: &str = "ledgrrr_status";
pub const INSTALL_PLAN_TOOL: &str = "ledgrrr_install_plan";
pub const INSTALL_DESKTOP_TOOL: &str = "ledgrrr_install_desktop";
pub const START_SERVICE_TOOL: &str = "ledgrrr_start_service";
pub const STOP_SERVICE_TOOL: &str = "ledgrrr_stop_service";
pub const OPEN_TRAY_TOOL: &str = "ledgrrr_open_tray";
pub const RENDER_DIAGRAM_TOOL: &str = "ledgrrr_render_diagram";
pub const SIMULATE_PIPELINE_TOOL: &str = "ledgrrr_simulate_pipeline";
pub const EXPORT_OFFICE_ARTIFACT_TOOL: &str = "ledgrrr_export_office_artifact";
pub const REPAIR_TOOL: &str = "ledgrrr_repair";
pub const UNINSTALL_TOOL: &str = "ledgrrr_uninstall";

pub const TOOL_REGISTRY: &[&str] = &[
    STATUS_TOOL,
    INSTALL_PLAN_TOOL,
    INSTALL_DESKTOP_TOOL,
    START_SERVICE_TOOL,
    STOP_SERVICE_TOOL,
    OPEN_TRAY_TOOL,
    RENDER_DIAGRAM_TOOL,
    SIMULATE_PIPELINE_TOOL,
    EXPORT_OFFICE_ARTIFACT_TOOL,
    REPAIR_TOOL,
    UNINSTALL_TOOL,
];

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    #[error("invalid arguments for {tool}: {source}")]
    InvalidArguments {
        tool: &'static str,
        #[source]
        source: serde_json::Error,
    },
    #[error(transparent)]
    Playbook(#[from] crate::playbook::PlaybookError),
    #[error(transparent)]
    Render(#[from] render::RenderError),
    #[error(transparent)]
    OfficeArtifact(#[from] office_artifact::OfficeArtifactError),
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EmptyArgs {}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RenderDiagramArgs {
    pub playbook: PlaybookModel,
    /// One of: mermaid, json, svg. PNG is not yet supported (PRD-10 §6.1
    /// marks it a Future format, deferred until a rasterizer dep lands).
    pub format: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SimulatePipelineArgs {
    pub playbook: PlaybookModel,
    /// "deterministic" (default) or "local-cpu". Phase 1 only implements
    /// the deterministic, non-LLM profile.
    #[serde(default = "default_profile")]
    pub profile: String,
}

fn default_profile() -> String {
    simulate::DETERMINISTIC_PROFILE.to_string()
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExportOfficeArtifactArgs {
    pub playbook: PlaybookModel,
}

fn schema_json<T: JsonSchema>() -> Value {
    serde_json::to_value(schema_for!(T)).unwrap_or(Value::Null)
}

pub fn tool_descriptors() -> Vec<Value> {
    vec![
        json!({ "name": STATUS_TOOL, "inputSchema": schema_json::<EmptyArgs>() }),
        json!({ "name": INSTALL_PLAN_TOOL, "inputSchema": schema_json::<EmptyArgs>() }),
        json!({ "name": INSTALL_DESKTOP_TOOL, "inputSchema": schema_json::<EmptyArgs>() }),
        json!({ "name": START_SERVICE_TOOL, "inputSchema": schema_json::<EmptyArgs>() }),
        json!({ "name": STOP_SERVICE_TOOL, "inputSchema": schema_json::<EmptyArgs>() }),
        json!({ "name": OPEN_TRAY_TOOL, "inputSchema": schema_json::<EmptyArgs>() }),
        json!({ "name": RENDER_DIAGRAM_TOOL, "inputSchema": schema_json::<RenderDiagramArgs>() }),
        json!({ "name": SIMULATE_PIPELINE_TOOL, "inputSchema": schema_json::<SimulatePipelineArgs>() }),
        json!({ "name": EXPORT_OFFICE_ARTIFACT_TOOL, "inputSchema": schema_json::<ExportOfficeArtifactArgs>() }),
        json!({ "name": REPAIR_TOOL, "inputSchema": schema_json::<EmptyArgs>() }),
        json!({ "name": UNINSTALL_TOOL, "inputSchema": schema_json::<EmptyArgs>() }),
    ]
}

fn parse_args<T: for<'de> Deserialize<'de>>(
    tool: &'static str,
    arguments: &Value,
) -> Result<T, ToolError> {
    serde_json::from_value(arguments.clone())
        .map_err(|source| ToolError::InvalidArguments { tool, source })
}

fn to_json<T: Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

pub fn dispatch(tool_name: &str, arguments: &Value) -> Result<Value, ToolError> {
    match tool_name {
        STATUS_TOOL => Ok(to_json(&status::collect())),
        INSTALL_PLAN_TOOL => Ok(to_json(&install_plan::install_plan())),
        INSTALL_DESKTOP_TOOL => Ok(to_json(&install_plan::install_plan())),
        START_SERVICE_TOOL => Ok(to_json(&service_control::start_service())),
        STOP_SERVICE_TOOL => Ok(to_json(&service_control::stop_service())),
        OPEN_TRAY_TOOL => Ok(to_json(&service_control::open_tray())),
        RENDER_DIAGRAM_TOOL => {
            let args: RenderDiagramArgs = parse_args(RENDER_DIAGRAM_TOOL, arguments)?;
            let rendered = render::render(&args.playbook, &args.format)?;
            Ok(json!({ "format": args.format, "content": rendered }))
        }
        SIMULATE_PIPELINE_TOOL => {
            let args: SimulatePipelineArgs = parse_args(SIMULATE_PIPELINE_TOOL, arguments)?;
            let trace = simulate::simulate(&args.playbook, &args.profile)?;
            Ok(to_json(&trace))
        }
        EXPORT_OFFICE_ARTIFACT_TOOL => {
            let args: ExportOfficeArtifactArgs =
                parse_args(EXPORT_OFFICE_ARTIFACT_TOOL, arguments)?;
            let bundle = office_artifact::export(&args.playbook)?;
            Ok(to_json(&bundle))
        }
        REPAIR_TOOL => Ok(to_json(&install_plan::native_installer_required_plan(
            "repair",
        ))),
        UNINSTALL_TOOL => Ok(to_json(&install_plan::native_installer_required_plan(
            "uninstall",
        ))),
        other => Err(ToolError::UnknownTool(other.to_string())),
    }
}
