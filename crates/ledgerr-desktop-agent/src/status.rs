//! `ledgrrr_status` — real local checks only, no mocked fields.
//!
//! Every field here is either measured (process/file/PATH lookup) or an
//! explicit `not_configured`/`missing` marker. PRD-10 §7 requires local
//! model use to be visible in status output; Phase 1 has no model runtime
//! yet, so that field always reports `configured: false`.

use std::process::Command;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sysinfo::{Pid, System};

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
    let Some(hb) = state::read_heartbeat() else {
        return ServiceStatus {
            running: false,
            pid: None,
            heartbeat_age_secs: None,
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
    }
}

/// Candidate tray binaries this repo already builds, checked next to the
/// running controller binary and then on PATH.
const TRAY_CANDIDATES: &[&str] = &["host-tray", "ledgerr-tauri"];

/// Finds a tray binary next to this controller's own executable, then on
/// PATH. Shared by [`status::collect`] (reporting) and
/// `service_control::open_tray` (launching).
pub fn detect_tray_binary() -> Option<String> {
    let mut search_dirs = Vec::new();
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
        if let Ok(out) = Command::new("which").arg(name).output() {
            if out.status.success() {
                let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
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
        office_addin: OfficeSurfaceStatus {
            state: "not_configured".to_string(),
        },
        sharepoint_webpart: OfficeSurfaceStatus {
            state: "not_configured".to_string(),
        },
    }
}
