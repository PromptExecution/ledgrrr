//! `ledgrrr_start_service` / `ledgrrr_stop_service` / `ledgrrr_open_tray`.
//!
//! The package may register the runtime with Windows Service Control Manager
//! after an elevated install.  The controller still supports the required
//! per-user fallback by launching the installed runtime and talking to its
//! authenticated loopback endpoint.

use std::process::{Command, Stdio};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sysinfo::{Pid, ProcessesToUpdate, Signal, System};

use crate::runtime_client;
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
    if let Some(record) = state::read_package_install() {
        candidates.push(std::path::PathBuf::from(record.external_payload_dir).join(
            if cfg!(windows) {
                "ledgrrr-service.exe"
            } else {
                "ledgrrr-service"
            },
        ));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(if cfg!(windows) {
                "ledgrrr-service.exe"
            } else {
                "ledgrrr-service"
            }));
        }
    }
    candidates.push(std::path::PathBuf::from(if cfg!(windows) {
        "ledgrrr-service.exe"
    } else {
        "ledgrrr-service"
    }));
    candidates
}

pub fn start_service() -> ServiceControlResult {
    if let Ok(health) = runtime_client::health() {
        state::audit(
            "controller",
            "start_service",
            "already_running",
            "runtime health check passed",
        );
        return ServiceControlResult {
            action: "start_service".to_string(),
            ok: true,
            pid: Some(health.pid),
            message: format!("runtime already running ({})", health.mode),
        };
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
            for _ in 0..25 {
                std::thread::sleep(std::time::Duration::from_millis(100));
                if let Ok(health) = runtime_client::health() {
                    state::audit(
                        "controller",
                        "start_service",
                        "ok",
                        format!("pid={}", health.pid),
                    );
                    return ServiceControlResult {
                        action: "start_service".to_string(),
                        ok: true,
                        pid: Some(health.pid),
                        message: format!("started authenticated {} runtime", health.mode),
                    };
                }
            }
            state::audit(
                "controller",
                "start_service",
                "failed",
                "runtime did not become ready",
            );
            return ServiceControlResult {
                action: "start_service".to_string(),
                ok: false,
                pid: Some(pid),
                message: "runtime process started but readiness endpoint did not become available"
                    .to_string(),
            };
        }
    }

    ServiceControlResult {
        action: "start_service".to_string(),
        ok: false,
        pid: None,
        message: "installed ledgrrr-service binary not found next to this controller or on PATH"
            .to_string(),
    }
}

pub fn stop_service() -> ServiceControlResult {
    if let Ok(health) = runtime_client::stop() {
        state::remove_heartbeat();
        state::remove_runtime_descriptor();
        state::audit(
            "controller",
            "stop_service",
            "ok",
            format!("pid={}", health.pid),
        );
        return ServiceControlResult {
            action: "stop_service".to_string(),
            ok: true,
            pid: Some(health.pid),
            message: "requested graceful runtime shutdown through authenticated IPC".to_string(),
        };
    }
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
    state::remove_runtime_descriptor();
    state::audit(
        "controller",
        "stop_service",
        "fallback",
        format!("pid={}", hb.pid),
    );

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
