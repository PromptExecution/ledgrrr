//! Package discovery and plan-before-mutation actions for the Windows desktop.
//!
//! The authoritative artifact is a test-signed MSIX external-location package.
//! The Claude MCPB remains unprivileged: it may launch the package helper only
//! after an explicit `approved: true` request, and the helper owns UAC.

use std::path::PathBuf;
use std::process::Command;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::state;

pub const PACKAGE_FAMILY_NAME: &str = "ventures.elastic.ledgrrr";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PrivilegeLevel {
    User,
    Elevated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InstallScope {
    PerUser,
    Machine,
}

impl Default for InstallScope {
    fn default() -> Self {
        Self::PerUser
    }
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
    pub package_family: String,
    pub scope: InstallScope,
    pub paths: Vec<PlannedPath>,
    pub unattended_command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PackageActionArgs {
    /// Required for a mutating action.  Call `ledgrrr_install_plan` first and
    /// surface its paths and UAC requirement to the operator.
    #[serde(default)]
    pub approved: bool,
    #[serde(default)]
    pub scope: InstallScope,
    /// Explicit MSIX path for install; repair/uninstall discover the package.
    #[serde(default)]
    pub package_path: Option<String>,
}

impl Default for PackageActionArgs {
    fn default() -> Self {
        Self {
            approved: false,
            scope: InstallScope::PerUser,
            package_path: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PackageActionResult {
    pub action: String,
    pub ok: bool,
    pub launched: bool,
    pub privilege_required: PrivilegeLevel,
    pub message: String,
    pub plan: InstallPlan,
}

pub fn package_script_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("LEDGRRR_PACKAGE_SCRIPT") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    let exe = std::env::current_exe().ok()?;
    let path = exe.parent()?.join("ledgrrr-package.ps1");
    path.is_file().then_some(path)
}

fn external_location(scope: InstallScope) -> PathBuf {
    match scope {
        InstallScope::PerUser => std::env::var("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| state::state_dir())
            .join("Programs")
            .join("ledgrrr"),
        InstallScope::Machine => std::env::var("ProgramFiles")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(r"C:\\Program Files"))
            .join("ledgrrr"),
    }
}

pub fn install_plan_for(
    action: &str,
    scope: InstallScope,
    package_path: Option<&str>,
) -> InstallPlan {
    let config = state::RuntimeConfig::per_user();
    let package_ready = package_script_path().is_some();
    let resolved_package_path = package_path.map(ToOwned::to_owned).or_else(|| {
        state::cached_package_path()
            .is_file()
            .then(|| state::cached_package_path().display().to_string())
    });
    // Uninstall removes an existing registration and its payload; unlike
    // install/repair it never needs a copy of the MSIX file.
    let needs_package_path = action != "uninstall";
    let package_path_ready = !needs_package_path || resolved_package_path.is_some();
    let executable_now = cfg!(windows) && package_ready && package_path_ready;
    let privilege_required = if scope == InstallScope::Machine {
        PrivilegeLevel::Elevated
    } else {
        PrivilegeLevel::User
    };
    let blocked_reason = if !cfg!(windows) {
        Some("Windows MSIX package actions can only run on Windows.".to_string())
    } else if !package_ready {
        Some("ledgrrr-package.ps1 is not installed next to the controller.".to_string())
    } else if needs_package_path && !package_path_ready {
        Some(format!(
            "{action} requires an explicit test-signed MSIX package_path; no cached package is available."
        ))
    } else {
        None
    };
    let package_arg = resolved_package_path
        .as_deref()
        .unwrap_or("<ledgrrr-test-signed.msix>");
    InstallPlan {
        action: action.to_string(),
        privilege_required,
        executable_now,
        blocked_reason,
        package_family: PACKAGE_FAMILY_NAME.to_string(),
        scope,
        paths: vec![
            PlannedPath {
                purpose: "MSIX external install location (package-owned binaries)".to_string(),
                path: external_location(scope).display().to_string(),
            },
            PlannedPath {
                purpose: "per-user runtime configuration and audit".to_string(),
                path: state::state_dir().display().to_string(),
            },
            PlannedPath {
                purpose: "runtime data".to_string(),
                path: config.data_dir,
            },
            PlannedPath {
                purpose: "runtime logs".to_string(),
                path: config.log_dir,
            },
        ],
        unattended_command: format!(
            "powershell.exe -NoProfile -ExecutionPolicy Bypass -File ledgrrr-package.ps1 -Action {} -Scope {:?}{} -Quiet",
            match action {
                "install_desktop" => "Install",
                "repair" => "Repair",
                "uninstall" => "Uninstall",
                _ => "Plan",
            },
            scope,
            if needs_package_path {
                format!(" -PackagePath \"{package_arg}\"")
            } else {
                String::new()
            }
        ),
    }
}

pub fn install_plan() -> InstallPlan {
    install_plan_for("install_desktop", InstallScope::PerUser, None)
}

pub fn action_plan(action: &str, args: &PackageActionArgs) -> InstallPlan {
    install_plan_for(action, args.scope, args.package_path.as_deref())
}

fn helper_action(action: &str) -> &'static str {
    match action {
        "install_desktop" => "Install",
        "repair" => "Repair",
        "uninstall" => "Uninstall",
        _ => "Plan",
    }
}

pub fn invoke(action: &str, args: PackageActionArgs) -> PackageActionResult {
    let mut args = args;
    if args.package_path.is_none() && state::cached_package_path().is_file() {
        args.package_path = Some(state::cached_package_path().display().to_string());
    }
    let plan = action_plan(action, &args);
    if !args.approved {
        return PackageActionResult {
            action: action.to_string(),
            ok: false,
            launched: false,
            privilege_required: plan.privilege_required,
            message: "approval required: call ledgrrr_install_plan and retry with approved=true"
                .to_string(),
            plan,
        };
    }
    if !plan.executable_now {
        return PackageActionResult {
            action: action.to_string(),
            ok: false,
            launched: false,
            privilege_required: plan.privilege_required,
            message: plan
                .blocked_reason
                .clone()
                .unwrap_or_else(|| "package action is unavailable".to_string()),
            plan,
        };
    }
    let Some(script) = package_script_path() else {
        return PackageActionResult {
            action: action.to_string(),
            ok: false,
            launched: false,
            privilege_required: plan.privilege_required,
            message: "package helper disappeared before launch".to_string(),
            plan,
        };
    };
    let scope = match args.scope {
        InstallScope::PerUser => "PerUser",
        InstallScope::Machine => "Machine",
    };
    let mut command = Command::new("powershell.exe");
    command.args([
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        script.to_string_lossy().as_ref(),
        "-Action",
        helper_action(action),
        "-Scope",
        scope,
    ]);
    if let Some(package_path) = args.package_path {
        command.args(["-PackagePath", &package_path]);
    }
    let launched = command.spawn().is_ok();
    state::audit(
        "controller",
        action,
        if launched { "launched" } else { "failed" },
        format!("scope={scope}; package_helper={}", script.display()),
    );
    PackageActionResult {
        action: action.to_string(),
        ok: launched,
        launched,
        privilege_required: plan.privilege_required,
        message: if launched {
            "launched installed package workflow; UAC, repair, and uninstall remain visible in Windows".to_string()
        } else {
            "failed to launch the installed package workflow".to_string()
        },
        plan,
    }
}
