//! `ledgrrr_start_service` / `ledgrrr_stop_service` / `ledgrrr_open_tray`.
//!
//! Phase 1 has no OS service manager (no systemd unit, no Windows Service
//! Control Manager registration — that is native-installer territory, PRD-11
//! §3.2). What Phase 1 *can* do honestly, without elevation, is spawn/kill a
//! user-level `ledgrrr-service` child process and track it via the
//! heartbeat file in [`crate::state`], and launch a tray binary if one is
//! already built in this repo.

use std::process::{Command, Stdio};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sysinfo::{Pid, ProcessesToUpdate, Signal, System};

use crate::state;
use crate::status::detect_tray_binary;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ServiceControlResult {
    pub action: String,
    pub ok: bool,
    pub pid: Option<u32>,
    pub message: String,
}

fn service_binary_candidates() -> Vec<std::path::PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("ledgrrr-service"));
        }
    }
    candidates.push(std::path::PathBuf::from("ledgrrr-service"));
    candidates
}

pub fn start_service() -> ServiceControlResult {
    if let Some(hb) = state::read_heartbeat() {
        let mut sys = System::new();
        sys.refresh_processes(ProcessesToUpdate::All, true);
        if sys.process(Pid::from_u32(hb.pid)).is_some() {
            return ServiceControlResult {
                action: "start_service".to_string(),
                ok: true,
                pid: Some(hb.pid),
                message: "service already running".to_string(),
            };
        }
    }

    for candidate in service_binary_candidates() {
        let spawned = if candidate.is_absolute() {
            Command::new(&candidate)
        } else {
            Command::new(candidate.as_os_str())
        }
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();

        if let Ok(child) = spawned {
            let pid = child.id();
            // The child writes its own heartbeat once running; record a
            // provisional one immediately so `ledgrrr_status` has something
            // to report even before the first self-write.
            let _ = state::write_heartbeat(pid, state::now());
            return ServiceControlResult {
                action: "start_service".to_string(),
                ok: true,
                pid: Some(pid),
                message: format!("spawned {}", candidate.display()),
            };
        }
    }

    ServiceControlResult {
        action: "start_service".to_string(),
        ok: false,
        pid: None,
        message: "ledgrrr-service binary not found next to this controller or on PATH".to_string(),
    }
}

pub fn stop_service() -> ServiceControlResult {
    let Some(hb) = state::read_heartbeat() else {
        return ServiceControlResult {
            action: "stop_service".to_string(),
            ok: true,
            pid: None,
            message: "no service heartbeat on record — nothing to stop".to_string(),
        };
    };

    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    let killed = match sys.process(Pid::from_u32(hb.pid)) {
        Some(process) => process.kill_with(Signal::Term).unwrap_or(false),
        None => false,
    };
    state::remove_heartbeat();

    ServiceControlResult {
        action: "stop_service".to_string(),
        ok: true,
        pid: Some(hb.pid),
        message: if killed {
            "sent terminate signal to service process".to_string()
        } else {
            "service process was already gone — cleared stale heartbeat".to_string()
        },
    }
}

pub fn open_tray() -> ServiceControlResult {
    match detect_tray_binary() {
        Some(path) => match Command::new(&path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => ServiceControlResult {
                action: "open_tray".to_string(),
                ok: true,
                pid: Some(child.id()),
                message: format!("launched {path}"),
            },
            Err(e) => ServiceControlResult {
                action: "open_tray".to_string(),
                ok: false,
                pid: None,
                message: format!("found {path} but failed to launch: {e}"),
            },
        },
        None => ServiceControlResult {
            action: "open_tray".to_string(),
            ok: false,
            pid: None,
            message:
                "no tray binary (host-tray, ledgerr-tauri) found next to controller or on PATH"
                    .to_string(),
        },
    }
}
