//! Local state directory + service heartbeat/pid file.
//!
//! Phase 1 has no OS service manager integration (that is native-installer
//! territory per PRD-10 §3.2/§7). `ledgrrr-service` is a plain long-lived
//! process the controller can spawn/kill at the user level; this module is
//! the shared contract both sides use to find it.

use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

pub fn state_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("LEDGRRR_STATE_DIR") {
        return PathBuf::from(dir);
    }
    if cfg!(windows) {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            return PathBuf::from(local).join("ledgrrr");
        }
    }
    if let Ok(xdg) = std::env::var("XDG_STATE_HOME") {
        return PathBuf::from(xdg).join("ledgrrr");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".local/state/ledgrrr");
    }
    std::env::temp_dir().join("ledgrrr")
}

fn heartbeat_path() -> PathBuf {
    state_dir().join("service-heartbeat.json")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceHeartbeat {
    pub pid: u32,
    pub started_at_unix: u64,
    pub last_beat_unix: u64,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn write_heartbeat(pid: u32, started_at_unix: u64) -> std::io::Result<()> {
    let dir = state_dir();
    std::fs::create_dir_all(&dir)?;
    let hb = ServiceHeartbeat {
        pid,
        started_at_unix,
        last_beat_unix: now_unix(),
    };
    let body = serde_json::to_string_pretty(&hb).unwrap_or_default();
    let tmp = heartbeat_path().with_extension("json.tmp");
    let mut f = std::fs::File::create(&tmp)?;
    f.write_all(body.as_bytes())?;
    std::fs::rename(tmp, heartbeat_path())
}

pub fn read_heartbeat() -> Option<ServiceHeartbeat> {
    let body = std::fs::read_to_string(heartbeat_path()).ok()?;
    serde_json::from_str(&body).ok()
}

pub fn remove_heartbeat() {
    let _ = std::fs::remove_file(heartbeat_path());
}

pub fn now() -> u64 {
    now_unix()
}
