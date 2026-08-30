//! Plan-before-mutation winget install-assist for Windows Foundry Local.
//! Mirrors `install_plan.rs`'s exact safety contract for installing Windows
//! Foundry Local via winget (approve-then-invoke, never runs anything unapproved).

use std::process::Command;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::state;

/// Exact command already proven in this repo's own `Justfile`
/// (`windows-ai-install` recipe) — do not invent a different flag set.
const WINGET_INSTALL_COMMAND: &str = "winget install --id Microsoft.FoundryLocal --source winget --accept-package-agreements --accept-source-agreements";

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FoundryInstallPlan {
    pub action: String,
    pub executable_now: bool,
    pub blocked_reason: Option<String>,
    pub unattended_command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FoundryInstallActionArgs {
    #[serde(default)]
    pub approved: bool,
}

impl Default for FoundryInstallActionArgs {
    fn default() -> Self {
        Self { approved: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FoundryInstallResult {
    pub ok: bool,
    pub launched: bool,
    pub message: String,
    pub plan: FoundryInstallPlan,
}

pub fn install_plan() -> FoundryInstallPlan {
    let executable_now = cfg!(windows);
    let blocked_reason = if !cfg!(windows) {
        Some("Windows Foundry Local can only be installed via winget on Windows.".to_string())
    } else {
        None
    };
    FoundryInstallPlan {
        action: "install_foundry_local".to_string(),
        executable_now,
        blocked_reason,
        unattended_command: WINGET_INSTALL_COMMAND.to_string(),
    }
}

pub fn invoke(args: FoundryInstallActionArgs) -> FoundryInstallResult {
    let plan = install_plan();
    if !args.approved {
        return FoundryInstallResult {
            ok: false,
            launched: false,
            message: "approval required: call the install plan and retry with approved=true"
                .to_string(),
            plan,
        };
    }
    if !plan.executable_now {
        return FoundryInstallResult {
            ok: false,
            launched: false,
            message: plan
                .blocked_reason
                .clone()
                .unwrap_or_else(|| "Foundry Local install is unavailable".to_string()),
            plan,
        };
    }
    let launched = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            WINGET_INSTALL_COMMAND,
        ])
        .spawn()
        .is_ok();
    state::audit(
        "controller",
        "install_foundry_local",
        if launched { "launched" } else { "failed" },
        "winget install --id Microsoft.FoundryLocal",
    );
    FoundryInstallResult {
        ok: launched,
        launched,
        message: if launched {
            "launched Foundry Local install via winget".to_string()
        } else {
            "failed to launch the Foundry Local winget install".to_string()
        },
        plan,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invoke_without_approval_never_launches() {
        let result = invoke(FoundryInstallActionArgs { approved: false });
        assert!(!result.ok);
        assert!(!result.launched);
        assert!(result.message.contains("approval required"));
    }

    #[test]
    fn install_plan_reports_the_exact_proven_winget_command() {
        let plan = install_plan();
        assert_eq!(
            plan.unattended_command,
            "winget install --id Microsoft.FoundryLocal --source winget --accept-package-agreements --accept-source-agreements"
        );
    }

    #[test]
    fn default_action_args_are_not_approved() {
        let args = FoundryInstallActionArgs::default();
        assert!(!args.approved);
    }
}
