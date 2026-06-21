//! Surface — typed lifecycle for any b00t-managed process.
//!
//! A surface is the abstract syntax for a process that b00t governs:
//! requirements → init → operate → maintain → terminate.
//! Every method returns a `Result` wrapping a `LifecyclePromise` so the
//! executive can chain, observe, and verify each transition.

use super::governance::{AgentRole, GovernancePolicy};
use std::path::Path;
use std::time::Duration;

/// A requirement that must be satisfied before a surface can operate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Requirement {
    PathExists(String),
    BinaryOnPath(String),
    PortAvailable(u16),
}

impl std::fmt::Display for Requirement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PathExists(p) => write!(f, "path exists: {p}"),
            Self::BinaryOnPath(b) => write!(f, "binary on PATH: {b}"),
            Self::PortAvailable(p) => write!(f, "port available: {p}"),
        }
    }
}

/// Action returned by the maintenance cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaintenanceAction {
    NoOp,
    Restart,
    Terminate,
    Quarantine { reason: String },
}

/// A record produced when a surface terminates.
#[derive(Debug, Clone)]
pub struct AuditRecord {
    pub surface_name: String,
    pub uptime: Duration,
    pub exit_reason: String,
    pub crash_count: u32,
    pub bytes_logged: u64,
}

/// SurfaceCapability — what a surface promises to the executive.
///
/// This is the solver-verifiable contract. Every surface declares its
/// capabilities at compile time; the executive checks that governance
/// constraints are satisfied before dispatching any promise.
#[derive(Debug, Clone)]
pub struct SurfaceCapability {
    pub name: &'static str,
    pub requirements: Vec<Requirement>,
    pub governance: GovernancePolicy,
}

/// The core trait: any process surface in the b00t ecosystem.
///
/// Implementors define the lifecycle as typed methods.
/// The `SurfaceCapability` is the static contract; the lifecycle methods
/// produce the runtime `LifecyclePromise` events.
pub trait ProcessSurface {
    type Config: serde::de::DeserializeOwned;
    type Error: std::error::Error;
    type Handle;

    /// Static capability declaration — the solver-verifiable contract.
    fn capability(&self) -> SurfaceCapability;

    /// Declare what this surface needs before it can run (shortcut into capability).
    fn requirements(&self) -> Vec<Requirement> {
        self.capability().requirements
    }

    /// Returns the governance policy for this surface (shortcut into capability).
    fn governance(&self) -> GovernancePolicy {
        self.capability().governance
    }

    /// Validate config, resolve dependencies, acquire resources.
    fn init(&mut self, config: Self::Config) -> Result<(), Self::Error>;

    /// Start the process, return a handle for lifecycle control.
    fn operate(&self) -> Result<Self::Handle, Self::Error>;

    /// Graceful shutdown, resource release, audit record.
    fn terminate(handle: Self::Handle) -> Result<AuditRecord, Self::Error>;

    /// Health check: return a maintenance action.
    fn maintain(&self) -> MaintenanceAction;
}

/// A surface that watches a directory of b00t datum files.
pub struct DatumWatcher {
    pub datum_dir: std::path::PathBuf,
    pub watched_files: Vec<String>,
    pub crash_count: u32,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct DatumWatcherConfig {
    pub datum_dir: String,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: u64,
}

fn default_poll_interval() -> u64 {
    30
}

#[derive(Debug, thiserror::Error)]
pub enum DatumWatcherError {
    #[error("datum directory not found: {0}")]
    DirNotFound(String),
    #[error("notify error: {0}")]
    Notify(#[from] notify::Error),
    #[error("datum validation failed: {0}")]
    Validation(String),
}

impl Default for DatumWatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl DatumWatcher {
    pub fn new() -> Self {
        let base = if cfg!(test) {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../_b00t_")
        } else {
            Path::new("_b00t_").to_path_buf()
        };
        Self {
            datum_dir: base.join("datums"),
            watched_files: Vec::new(),
            crash_count: 0,
        }
    }

    fn collect_datum_files(&self) -> Result<Vec<String>, DatumWatcherError> {
        let dir = &self.datum_dir;
        if !dir.exists() {
            return Err(DatumWatcherError::DirNotFound(dir.display().to_string()));
        }
        let entries = std::fs::read_dir(dir)
            .map_err(|e| DatumWatcherError::DirNotFound(format!("{}: {e}", dir.display())))?;
        let mut files = Vec::new();
        for entry in entries {
            let entry = entry
                .map_err(|e| DatumWatcherError::DirNotFound(format!("{}: {e}", dir.display())))?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".datum") {
                files.push(name.trim_end_matches(".datum").to_owned());
            }
        }
        files.sort();
        Ok(files)
    }
}

impl ProcessSurface for DatumWatcher {
    type Config = DatumWatcherConfig;
    type Error = DatumWatcherError;
    type Handle = ();

    fn capability(&self) -> SurfaceCapability {
        SurfaceCapability {
            name: "datum-watcher",
            requirements: vec![Requirement::PathExists(
                self.datum_dir.display().to_string(),
            )],
            governance: GovernancePolicy {
                allowed_starters: vec![AgentRole::Executive, AgentRole::Operator],
                max_ttl: Duration::from_secs(86400),
                auto_restart: true,
                crash_budget: 5,
            },
        }
    }

    fn init(&mut self, config: Self::Config) -> Result<(), Self::Error> {
        self.datum_dir = Path::new(&config.datum_dir).to_path_buf();
        if !self.datum_dir.exists() {
            return Err(DatumWatcherError::DirNotFound(config.datum_dir));
        }
        self.watched_files = self.collect_datum_files()?;
        tracing::info!(
            "DatumWatcher initialized: {} files in {}",
            self.watched_files.len(),
            self.datum_dir.display()
        );
        Ok(())
    }

    fn operate(&self) -> Result<Self::Handle, Self::Error> {
        for name in &self.watched_files {
            match datum::load_datum(&self.datum_dir, name) {
                Ok(d) => tracing::debug!("datum OK: {} ({})", d.name, d.h1),
                Err(e) => {
                    return Err(DatumWatcherError::Validation(format!("{name}: {e}")));
                }
            }
        }
        tracing::info!(
            "DatumWatcher operating: {} datums validated",
            self.watched_files.len()
        );
        Ok(())
    }

    fn terminate((): Self::Handle) -> Result<AuditRecord, Self::Error> {
        Ok(AuditRecord {
            surface_name: "datum-watcher".into(),
            uptime: Duration::from_secs(0),
            exit_reason: "manual".into(),
            crash_count: 0,
            bytes_logged: 0,
        })
    }

    fn maintain(&self) -> MaintenanceAction {
        if self.crash_count >= self.governance().crash_budget {
            return MaintenanceAction::Quarantine {
                reason: format!(
                    "crash budget exhausted: {} >= {}",
                    self.crash_count,
                    self.governance().crash_budget
                ),
            };
        }
        if !self.datum_dir.exists() {
            return MaintenanceAction::Quarantine {
                reason: format!("datum dir vanished: {}", self.datum_dir.display()),
            };
        }
        MaintenanceAction::NoOp
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn datum_watcher_init_and_operate() {
        let tmp = tempfile::tempdir().expect("failed to create temp directory");
        let mut watcher = DatumWatcher::new();
        let config = DatumWatcherConfig {
            datum_dir: tmp.path().display().to_string(),
            poll_interval_secs: 30,
        };
        watcher.init(config).expect("init");
        watcher.operate().expect("operate");
    }

    #[test]
    fn datum_watcher_missing_dir() {
        let mut watcher = DatumWatcher::new();
        let config = DatumWatcherConfig {
            datum_dir: "/nonexistent/b00t/datums".into(),
            poll_interval_secs: 30,
        };
        let err = watcher.init(config).unwrap_err();
        assert!(matches!(err, DatumWatcherError::DirNotFound(_)));
    }

    #[test]
    fn datum_watcher_maintain() {
        let tmp = tempfile::tempdir().expect("failed to create temp directory");
        let mut watcher = DatumWatcher::new();
        let config = DatumWatcherConfig {
            datum_dir: tmp.path().display().to_string(),
            poll_interval_secs: 30,
        };
        watcher.init(config).expect("init");
        assert_eq!(watcher.maintain(), MaintenanceAction::NoOp);
    }

    #[cfg(feature = "real_datums")]
    #[test]
    fn collect_datum_files() {
        let watcher = DatumWatcher::new();
        let files = match watcher.collect_datum_files() {
            Ok(f) => f,
            Err(DatumWatcherError::DirNotFound(_)) => return, // _b00t_/datums absent in this env
            Err(e) => panic!("unexpected error: {e}"),
        };
        assert!(files.contains(&"opencode".to_string()));
        assert!(files.contains(&"opencode-codebase-memory-integration".to_string()));
    }

    #[test]
    fn capability_returns_contract() {
        let watcher = DatumWatcher::new();
        let cap = watcher.capability();
        assert_eq!(cap.name, "datum-watcher");
        assert!(!cap.requirements.is_empty());
        assert!(cap.governance.validate().is_ok());
    }
}
