//! `ledgrrr_install_plan` / privileged-action plans — PRD-10 §3.1, §7.
//!
//! Phase 1 has no native Windows installer (that is a separate future
//! release artifact per PRD-10 §3.2/§8). Every privileged action in this
//! module therefore returns a `Plan` describing what *would* happen and why
//! it cannot execute yet — never a silent no-op, never a fabricated success.
//! This matches the non-goal in PRD-10 §10: "MCPB is not the privileged
//! installer."

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::state;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PrivilegeLevel {
    /// No elevation: user-writable state dir, spawn/kill a user process.
    User,
    /// Requires the native Windows installer + UAC elevation. Not available
    /// in Phase 1.
    Elevated,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PlannedPath {
    pub purpose: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct InstallPlan {
    pub action: String,
    pub privilege_required: PrivilegeLevel,
    pub executable_now: bool,
    pub blocked_reason: Option<String>,
    pub paths: Vec<PlannedPath>,
}

pub fn install_plan() -> InstallPlan {
    let dir = state::state_dir();
    InstallPlan {
        action: "install_desktop".to_string(),
        privilege_required: PrivilegeLevel::Elevated,
        executable_now: false,
        blocked_reason: Some(
            "native Windows installer (ledgrrr-service.exe/ledgrrr-tray.exe packaging) is not built yet — PRD-10 §3.2/§8".to_string(),
        ),
        paths: vec![
            PlannedPath {
                purpose: "local state / heartbeat".to_string(),
                path: dir.display().to_string(),
            },
            PlannedPath {
                purpose: "service binary (Phase 1 user-level, no OS service manager)".to_string(),
                path: dir.join("bin").join("ledgrrr-service").display().to_string(),
            },
        ],
    }
}

/// Shared "not yet available" plan for the actions this Phase 1 controller
/// cannot execute: repair, uninstall. `install_desktop` uses `install_plan`
/// above since it has richer path detail.
pub fn native_installer_required_plan(action: &str) -> InstallPlan {
    InstallPlan {
        action: action.to_string(),
        privilege_required: PrivilegeLevel::Elevated,
        executable_now: false,
        blocked_reason: Some(format!(
            "{action} requires the native Windows installer, which is not built yet — PRD-10 §3.2/§8"
        )),
        paths: Vec::new(),
    }
}
