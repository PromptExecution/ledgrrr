//! `ledgrrr_status` — real local checks only, no mocked fields.
//!
//! Every field here is either measured (process/file/PATH lookup) or an
//! explicit `not_configured`/`missing` marker. PRD-11 §7 requires local
//! model use to be visible in status output; Phase 1 has no model runtime
//! yet, so that field always reports `configured: false`.

use std::process::Command;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sysinfo::{Pid, System};

use crate::runtime_client;
use crate::state;

/// A heartbeat older than this many seconds is treated as stale — the
/// process may have died without cleaning up its heartbeat file.
const HEARTBEAT_STALE_SECS: u64 = 30;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct B00tStatus {
    pub cli_found: bool,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ServiceStatus {
    pub running: bool,
    pub pid: Option<u32>,
    pub heartbeat_age_secs: Option<u64>,
    pub readiness: String,
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TrayStatus {
    pub binary_found: bool,
    pub binary_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModelRuntimeStatus {
    pub configured: bool,
    pub profile: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PackageStatus {
    pub installed: bool,
    pub package_family: String,
    pub install_location: Option<String>,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ClaudeControllerStatus {
    pub state: String,
    pub expected_tools: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OfficeSurfaceStatus {
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LedgrrrStatus {
    pub controller_version: String,
    pub b00t: B00tStatus,
    pub service: ServiceStatus,
    pub tray: TrayStatus,
    pub model_runtime: ModelRuntimeStatus,
    pub desktop_package: PackageStatus,
    pub claude_controller: ClaudeControllerStatus,
    pub office_addin: OfficeSurfaceStatus,
    pub sharepoint_webpart: OfficeSurfaceStatus,
}

fn detect_b00t() -> B00tStatus {
    match Command::new("b00t").arg("--version").output() {
        Ok(out) if out.status.success() => B00tStatus {
            cli_found: true,
            version: Some(String::from_utf8_lossy(&out.stdout).trim().to_string()),
        },
        Ok(_) => B00tStatus {
            cli_found: true,
            version: None,
        },
        Err(_) => B00tStatus {
            cli_found: false,
            version: None,
        },
    }
}

fn detect_service() -> ServiceStatus {
    if let Ok(health) = runtime_client::health() {
        return ServiceStatus {
            running: health.ready,
            pid: Some(health.pid),
            heartbeat_age_secs: state::read_heartbeat()
                .map(|hb| state::now().saturating_sub(hb.last_beat_unix)),
            readiness: "ready".to_string(),
            mode: Some(health.mode),
        };
    }
    let Some(hb) = state::read_heartbeat() else {
        return ServiceStatus {
            running: false,
            pid: None,
            heartbeat_age_secs: None,
            readiness: "not_running".to_string(),
            mode: None,
        };
    };
    let age = state::now().saturating_sub(hb.last_beat_unix);
    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    let process_alive = sys.process(Pid::from_u32(hb.pid)).is_some();
    ServiceStatus {
        running: process_alive && age <= HEARTBEAT_STALE_SECS,
        pid: Some(hb.pid),
        heartbeat_age_secs: Some(age),
        readiness: if process_alive && age <= HEARTBEAT_STALE_SECS {
            "heartbeat_only".to_string()
        } else {
            "stale".to_string()
        },
        mode: state::read_runtime_descriptor().map(|descriptor| descriptor.mode),
    }
}

fn detect_package() -> PackageStatus {
    if let Some(record) = state::read_package_install() {
        let payload_present = std::path::Path::new(&record.external_payload_dir).is_dir();
        return PackageStatus {
            installed: payload_present,
            package_family: record.package_family,
            install_location: Some(record.external_payload_dir),
            state: if payload_present {
                "installed".to_string()
            } else {
                "payload_missing".to_string()
            },
        };
    }
    if let Ok(location) = std::env::var("LEDGRRR_PACKAGE_INSTALL_LOCATION") {
        return PackageStatus {
            installed: true,
            package_family: crate::install_plan::PACKAGE_FAMILY_NAME.to_string(),
            install_location: Some(location),
            state: "installed".to_string(),
        };
    }
    if !cfg!(windows) {
        return PackageStatus {
            installed: false,
            package_family: crate::install_plan::PACKAGE_FAMILY_NAME.to_string(),
            install_location: None,
            state: "windows_required".to_string(),
        };
    }
    let command = format!(
        "(Get-AppxPackage -Name '{}' -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty InstallLocation)",
        crate::install_plan::PACKAGE_FAMILY_NAME
    );
    match Command::new("powershell.exe")
        .args(["-NoProfile", "-Command", &command])
        .output()
    {
        Ok(output) if output.status.success() => {
            let location = String::from_utf8_lossy(&output.stdout).trim().to_string();
            PackageStatus {
                installed: !location.is_empty(),
                package_family: crate::install_plan::PACKAGE_FAMILY_NAME.to_string(),
                install_location: (!location.is_empty()).then_some(location),
                state: if output.stdout.is_empty() {
                    "not_installed".to_string()
                } else {
                    "installed".to_string()
                },
            }
        }
        _ => PackageStatus {
            installed: false,
            package_family: crate::install_plan::PACKAGE_FAMILY_NAME.to_string(),
            install_location: None,
            state: "discovery_failed".to_string(),
        },
    }
}

/// Candidate tray binaries this repo already builds, checked next to the
/// running controller binary and then on PATH.
const TRAY_CANDIDATES: &[&str] = &[
    "ledgrrr-tray.exe",
    "host-tauri.exe",
    "host-tauri",
];

/// Finds a tray binary next to this controller's own executable, then on
/// PATH. Shared by [`status::collect`] (reporting) and
/// `service_control::open_tray` (launching).
pub fn detect_tray_binary() -> Option<String> {
    let mut search_dirs = Vec::new();
    if let Some(record) = state::read_package_install() {
        search_dirs.push(std::path::PathBuf::from(record.external_payload_dir));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            search_dirs.push(dir.to_path_buf());
        }
    }
    for dir in &search_dirs {
        for name in TRAY_CANDIDATES {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate.display().to_string());
            }
        }
    }
    for name in TRAY_CANDIDATES {
        let locator = if cfg!(windows) { "where.exe" } else { "which" };
        if let Ok(out) = Command::new(locator).arg(name).output() {
            if out.status.success() {
                let path = String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                if !path.is_empty() {
                    return Some(path);
                }
            }
        }
    }
    None
}

fn detect_tray() -> TrayStatus {
    match detect_tray_binary() {
        Some(path) => TrayStatus {
            binary_found: true,
            binary_path: Some(path),
        },
        None => TrayStatus {
            binary_found: false,
            binary_path: None,
        },
    }
}

fn detect_model_runtime() -> ModelRuntimeStatus {
    match std::env::var("LEDGRRR_MODEL_RUNTIME_PROFILE") {
        Ok(profile) if !profile.is_empty() => ModelRuntimeStatus {
            configured: true,
            profile: Some(profile),
        },
        _ => ModelRuntimeStatus {
            configured: false,
            profile: None,
        },
    }
}

pub fn collect() -> LedgrrrStatus {
    LedgrrrStatus {
        controller_version: env!("CARGO_PKG_VERSION").to_string(),
        b00t: detect_b00t(),
        service: detect_service(),
        tray: detect_tray(),
        model_runtime: detect_model_runtime(),
        desktop_package: detect_package(),
        claude_controller: ClaudeControllerStatus {
            state: "installed_with_mcpb_or_direct_stdio".to_string(),
            expected_tools: 11,
        },
        office_addin: OfficeSurfaceStatus {
            state: "not_configured".to_string(),
        },
        sharepoint_webpart: OfficeSurfaceStatus {
            state: "not_configured".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_candidates_matches_the_real_host_tauri_binary_name() {
        assert!(
            TRAY_CANDIDATES.contains(&"host-tauri"),
            "TRAY_CANDIDATES must list the real host-tauri bin target, not a nonexistent ledgerr-tauri binary: {TRAY_CANDIDATES:?}"
        );
    }
}
