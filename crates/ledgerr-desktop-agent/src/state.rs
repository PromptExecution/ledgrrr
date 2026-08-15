//! Per-user runtime configuration, audit records, and service discovery.
//!
//! The installed Windows package and the unprivileged controller intentionally
//! share only this small, versioned contract.  It gives the controller a
//! loopback endpoint plus a bearer token without exposing a network listener
//! beyond the interactive user session.  A Windows Service may be installed
//! by the package when elevation is available; the exact same contract also
//! supports the documented per-user fallback.

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

pub fn runtime_descriptor_path() -> PathBuf {
    state_dir().join("runtime.json")
}

pub fn audit_log_path() -> PathBuf {
    state_dir().join("audit.jsonl")
}

/// The installer keeps only the public dogfood package materials here: the
/// test-signed MSIX and public certificate. This lets `repair` re-register the
/// sparse identity package without asking Claude to rediscover a release URL.
pub fn package_cache_dir() -> PathBuf {
    state_dir().join("package-cache")
}

pub fn cached_package_path() -> PathBuf {
    package_cache_dir().join("ledgrrr-test-signed.msix")
}

pub fn cached_certificate_path() -> PathBuf {
    package_cache_dir().join("ledgrrr-test.cer")
}

pub fn package_install_path() -> PathBuf {
    state_dir().join("package-install.json")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub schema_version: u8,
    pub mode: String,
    pub data_dir: String,
    pub log_dir: String,
}

impl RuntimeConfig {
    pub fn per_user() -> Self {
        let dir = state_dir();
        Self {
            schema_version: 1,
            mode: "per_user".to_string(),
            data_dir: dir.join("data").display().to_string(),
            log_dir: dir.join("logs").display().to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeDescriptor {
    pub schema_version: u8,
    pub pid: u32,
    pub endpoint: String,
    pub bearer_token: String,
    pub started_at_unix: u64,
    pub mode: String,
}

/// Native-package output consumed by controller processes that run outside
/// the external payload (for example, Claude Desktop's MCPB directory).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInstallRecord {
    pub schema_version: u8,
    pub package_family: String,
    pub external_payload_dir: String,
    pub scope: String,
    pub installed_at_unix: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeAuditRecord {
    pub at_unix: u64,
    pub actor: String,
    pub action: String,
    pub outcome: String,
    pub detail: String,
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

pub fn write_runtime_descriptor(descriptor: &RuntimeDescriptor) -> std::io::Result<()> {
    let dir = state_dir();
    std::fs::create_dir_all(&dir)?;
    let body = serde_json::to_vec_pretty(descriptor).map_err(std::io::Error::other)?;
    let tmp = runtime_descriptor_path().with_extension("json.tmp");
    let mut file = std::fs::File::create(&tmp)?;
    file.write_all(&body)?;
    std::fs::rename(tmp, runtime_descriptor_path())
}

pub fn read_runtime_descriptor() -> Option<RuntimeDescriptor> {
    let body = std::fs::read(runtime_descriptor_path()).ok()?;
    serde_json::from_slice(&body).ok()
}

pub fn remove_runtime_descriptor() {
    let _ = std::fs::remove_file(runtime_descriptor_path());
}

pub fn write_package_install(record: &PackageInstallRecord) -> std::io::Result<()> {
    let dir = state_dir();
    std::fs::create_dir_all(&dir)?;
    let body = serde_json::to_vec_pretty(record).map_err(std::io::Error::other)?;
    let tmp = package_install_path().with_extension("json.tmp");
    let mut file = std::fs::File::create(&tmp)?;
    file.write_all(&body)?;
    std::fs::rename(tmp, package_install_path())
}

pub fn read_package_install() -> Option<PackageInstallRecord> {
    let body = std::fs::read(package_install_path()).ok()?;
    serde_json::from_slice(&body).ok()
}

pub fn remove_package_install() {
    let _ = std::fs::remove_file(package_install_path());
}

pub fn append_audit(record: &RuntimeAuditRecord) -> std::io::Result<()> {
    let dir = state_dir();
    std::fs::create_dir_all(&dir)?;
    let line = serde_json::to_string(record).map_err(std::io::Error::other)?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(audit_log_path())?;
    writeln!(file, "{line}")
}

pub fn audit(actor: &str, action: &str, outcome: &str, detail: impl Into<String>) {
    let _ = append_audit(&RuntimeAuditRecord {
        at_unix: now_unix(),
        actor: actor.to_string(),
        action: action.to_string(),
        outcome: outcome.to_string(),
        detail: detail.into(),
    });
}

pub fn now() -> u64 {
    now_unix()
}
