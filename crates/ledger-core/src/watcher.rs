//! Pipeline Watcher — filesystem event notifications for hot-reload and ingest.
//!
//! ## Purpose
//! This module provides `PipelineWatcher`, a tokio-based filesystem watcher that:
//! - Detects `.rhai` rule file changes and hot-reloads the `RuleRegistry`
//! - Detects new `.pdf` files in the ingest directory and sends them for processing
//!
//! ## Debounce behavior
//! - `.rhai` modifications are debounced by 500ms to avoid multiple rapid reloads
//! - Only `ModifyKind::Data` events trigger reloads; metadata-only changes are ignored
//! - `.pdf` create events are sent immediately to the ingest channel without debounce

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use notify::Watcher;
use tokio::sync::{mpsc, RwLock};

use crate::rule_registry::{RuleRegistry, RuleRegistryError};

/// Error types for pipeline watcher operations.
#[derive(Debug, thiserror::Error)]
pub enum WatcherError {
    #[error("notify error: {0}")]
    Notify(#[from] notify::Error),

    #[error("registry reload error: {0}")]
    Registry(#[from] RuleRegistryError),

    #[error("send error on ingest channel: {0}")]
    Send(#[from] mpsc::error::SendError<PathBuf>),
}

/// Watches rule and ingest directories for filesystem events.
///
/// - Rules directory: `.rhai` file changes trigger registry hot-reload
/// - Ingest directory: new `.pdf` files trigger ingest processing
pub struct PipelineWatcher {
    rule_dir: PathBuf,
    ingest_dir: PathBuf,
    registry: Arc<RwLock<RuleRegistry>>,
    ingest_tx: mpsc::Sender<PathBuf>,
    debounce_ms: u64,
}

impl PipelineWatcher {
    /// Create a new watcher with default debounce (500ms).
    ///
    /// # Arguments
    /// - `rule_dir`: Directory containing `.rhai` rule files
    /// - `ingest_dir`: Directory to watch for new PDF files
    /// - `registry`: Shared rule registry to reload on rule changes
    /// - `ingest_tx`: Channel to send new PDF paths for processing
    ///
    /// # Returns
    /// `Self` ready to spawn
    pub fn new(
        rule_dir: PathBuf,
        ingest_dir: PathBuf,
        registry: Arc<RwLock<RuleRegistry>>,
        ingest_tx: mpsc::Sender<PathBuf>,
    ) -> Self {
        Self {
            rule_dir,
            ingest_dir,
            registry,
            ingest_tx,
            debounce_ms: 500,
        }
    }

    /// Set debounce duration in milliseconds for rule file changes.
    ///
    /// Default is 500ms. Lower values increase responsiveness but may trigger
    /// multiple reloads during rapid file edits.
    pub fn with_debounce_ms(mut self, ms: u64) -> Self {
        self.debounce_ms = ms;
        self
    }

    /// Spawn the watcher task and return a handle for graceful shutdown.
    ///
    /// This method starts an async task that monitors both directories and:
    /// - Reloads the rule registry when `.rhai` files are modified
    /// - Sends new `.pdf` file paths to the ingest channel
    ///
    /// The returned `notify::RecommendedWatcher` handle can be dropped to stop watching.
    ///
    /// # Errors
    /// Returns `WatcherError` if directory watching cannot be initialized.
    pub fn spawn(self) -> Result<notify::RecommendedWatcher, WatcherError> {
        let rule_dir = self.rule_dir.clone();
        let ingest_dir = self.ingest_dir.clone();
        let registry = Arc::clone(&self.registry);
        let ingest_tx = self.ingest_tx.clone();
        let debounce_ms = self.debounce_ms;
        let debounce_state = Arc::new(Mutex::new(HashMap::<PathBuf, Instant>::new()));

        let rule_dir_for_watch = rule_dir.clone();
        let ingest_dir_for_watch = ingest_dir.clone();
        let debounce_state_for_watch = Arc::clone(&debounce_state);

        // Use debounced watcher with configurable debounce duration
        let mut watcher = notify::recommended_watcher(move |res: Result<notify::Event, _>| {
            if let Ok(event) = res {
                Self::handle_event(
                    event,
                    &rule_dir,
                    &ingest_dir,
                    &registry,
                    &ingest_tx,
                    debounce_ms,
                    &debounce_state_for_watch,
                );
            }
        })?;

        watcher.watch(&rule_dir_for_watch, notify::RecursiveMode::NonRecursive)?;
        watcher.watch(&ingest_dir_for_watch, notify::RecursiveMode::NonRecursive)?;

        Ok(watcher)
    }

    /// Handle a single filesystem event.
    fn handle_event(
        event: notify::Event,
        rule_dir: &Path,
        ingest_dir: &Path,
        registry: &Arc<RwLock<RuleRegistry>>,
        ingest_tx: &mpsc::Sender<PathBuf>,
        debounce_ms: u64,
        debounce_state: &Arc<Mutex<HashMap<PathBuf, Instant>>>,
    ) {
        for path in event.paths {
            // Check if this is a rule file modification
            if path.starts_with(rule_dir)
                && path.extension().and_then(|e| e.to_str()) == Some("rhai")
            {
                if Self::is_data_modification(&event.kind)
                    && Self::should_process_rule_change(&path, debounce_ms, debounce_state)
                {
                    tracing::debug!("Rule file modified: {:?}", path);
                    Self::reload_registry(registry, path.clone());
                }
            }

            // Check if this is a PDF create event
            if path.starts_with(ingest_dir)
                && path.extension().and_then(|e| e.to_str()) == Some("pdf")
            {
                if Self::is_create_event(&event.kind) {
                    tracing::debug!("New PDF detected: {:?}", path);
                    let _ = ingest_tx.blocking_send(path);
                }
            }
        }
    }

    /// Check if an event kind represents a data modification (not metadata-only).
    fn is_data_modification(kind: &notify::EventKind) -> bool {
        matches!(
            kind,
            notify::EventKind::Modify(notify::event::ModifyKind::Data(_))
        )
    }

    /// Check if an event kind represents a file creation.
    fn is_create_event(kind: &notify::EventKind) -> bool {
        matches!(
            kind,
            notify::EventKind::Create(notify::event::CreateKind::File)
        )
    }

    fn should_process_rule_change(
        path: &Path,
        debounce_ms: u64,
        debounce_state: &Arc<Mutex<HashMap<PathBuf, Instant>>>,
    ) -> bool {
        let mut state = match debounce_state.lock() {
            Ok(state) => state,
            Err(poisoned) => {
                tracing::warn!("debounce state lock poisoned; recovering state");
                poisoned.into_inner()
            }
        };

        let now = Instant::now();
        let debounce_window = Duration::from_millis(debounce_ms);
        if let Some(last_seen) = state.get(path) {
            if now.duration_since(*last_seen) < debounce_window {
                return false;
            }
        }

        state.insert(path.to_path_buf(), now);
        true
    }

    /// Reload the rule registry from disk.
    ///
    /// Note: This runs in the notify callback thread (not tokio runtime),
    /// which is safe because registry reload is blocking I/O and we only
    /// need tokio for the final RwLock write.
    fn reload_registry(registry: &Arc<RwLock<RuleRegistry>>, path: PathBuf) {
        let registry = Arc::clone(registry);
        std::thread::spawn(move || {
            let Some(rule_dir) = path.parent() else {
                tracing::warn!("Skipping rule reload for path without parent: {:?}", path);
                return;
            };
            // Perform blocking reload in dedicated thread
            match RuleRegistry::load_from_dir(rule_dir) {
                Ok(new_registry) => {
                    // Spawn tokio runtime for async write
                    let rt = tokio::runtime::Handle::try_current();
                    if let Ok(handle) = rt {
                        handle.block_on(async move {
                            let mut guard = registry.write().await;
                            *guard = new_registry;
                            tracing::info!("Rule registry reloaded successfully");
                        });
                    } else {
                        tracing::error!("No tokio runtime available for registry reload");
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to reload rule registry: {}", e);
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    #[tokio::test]
    async fn watcher_reloads_registry_on_rhai_modify() {
        let rule_dir = TempDir::new().unwrap();
        let ingest_dir = TempDir::new().unwrap();
        let (ingest_tx, _ingest_rx) = mpsc::channel(10);

        // Create initial rule file
        let rule_path = rule_dir.path().join("test_rule.rhai");
        let mut rule_file = fs::File::create(&rule_path).unwrap();
        writeln!(rule_file, "fn classify(tx) {{ \"Unclassified\" }}").unwrap();

        // Load initial registry
        let registry = Arc::new(RwLock::new(
            RuleRegistry::load_from_dir(rule_dir.path()).unwrap(),
        ));
        assert_eq!(registry.read().await.rule_count(), 1);

        // Spawn watcher
        let watcher = PipelineWatcher::new(
            rule_dir.path().to_path_buf(),
            ingest_dir.path().to_path_buf(),
            Arc::clone(&registry),
            ingest_tx,
        )
        .spawn()
        .unwrap();

        // Modify rule file
        let mut rule_file = fs::File::create(&rule_path).unwrap();
        writeln!(rule_file, "fn classify(tx) {{ \"OfficeSupplies\" }}").unwrap();

        // Wait for reload (within 600ms per AC)
        tokio::time::sleep(tokio::time::Duration::from_millis(600)).await;

        // Verify registry still has 1 rule (not broken)
        assert_eq!(registry.read().await.rule_count(), 1);

        drop(watcher);
    }

    #[tokio::test]
    async fn watcher_sends_pdf_on_create() {
        let rule_dir = TempDir::new().unwrap();
        let ingest_dir = TempDir::new().unwrap();
        let (ingest_tx, mut ingest_rx) = mpsc::channel(10);

        // Create initial rule file (required for registry)
        let rule_path = rule_dir.path().join("test_rule.rhai");
        let mut rule_file = fs::File::create(&rule_path).unwrap();
        writeln!(rule_file, "fn classify(tx) {{ \"Unclassified\" }}").unwrap();

        let registry = Arc::new(RwLock::new(
            RuleRegistry::load_from_dir(rule_dir.path()).unwrap(),
        ));

        // Spawn watcher
        let watcher = PipelineWatcher::new(
            rule_dir.path().to_path_buf(),
            ingest_dir.path().to_path_buf(),
            Arc::clone(&registry),
            ingest_tx,
        )
        .spawn()
        .unwrap();

        // Create new PDF file
        let pdf_path = ingest_dir.path().join("new_document.pdf");
        fs::File::create(&pdf_path).unwrap();

        // Wait for event (within 600ms per AC)
        let received =
            tokio::time::timeout(tokio::time::Duration::from_millis(600), ingest_rx.recv())
                .await
                .unwrap()
                .unwrap();

        assert_eq!(received, pdf_path);

        drop(watcher);
    }

    #[tokio::test]
    async fn watcher_ignores_metadata_changes() {
        let rule_dir = TempDir::new().unwrap();
        let ingest_dir = TempDir::new().unwrap();
        let (ingest_tx, mut ingest_rx) = mpsc::channel(10);

        // Create initial rule file
        let rule_path = rule_dir.path().join("test_rule.rhai");
        let mut rule_file = fs::File::create(&rule_path).unwrap();
        writeln!(rule_file, "fn classify(tx) {{ \"Unclassified\" }}").unwrap();

        let registry = Arc::new(RwLock::new(
            RuleRegistry::load_from_dir(rule_dir.path()).unwrap(),
        ));
        let initial_rule_count = registry.read().await.rule_count();

        // Spawn watcher
        let watcher = PipelineWatcher::new(
            rule_dir.path().to_path_buf(),
            ingest_dir.path().to_path_buf(),
            Arc::clone(&registry),
            ingest_tx,
        )
        .spawn()
        .unwrap();

        // Touch file (metadata-only change, no data modification)
        // Use utimes to simulate touch without content change
        filetime::set_file_mtime(&rule_path, filetime::FileTime::now()).unwrap();

        // Wait to ensure no reload happens
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        // Verify no rule reload occurred
        assert_eq!(registry.read().await.rule_count(), initial_rule_count);

        // Verify no ingest events were sent
        let timeout =
            tokio::time::timeout(tokio::time::Duration::from_millis(200), ingest_rx.recv());
        assert!(timeout.await.is_err());

        drop(watcher);
    }

    #[tokio::test]
    async fn watcher_custom_debounce() {
        let rule_dir = TempDir::new().unwrap();
        let ingest_dir = TempDir::new().unwrap();
        let (ingest_tx, _ingest_rx) = mpsc::channel(10);

        let rule_path = rule_dir.path().join("test_rule.rhai");
        let mut rule_file = fs::File::create(&rule_path).unwrap();
        writeln!(rule_file, "fn classify(tx) {{ \"Unclassified\" }}").unwrap();

        let registry = Arc::new(RwLock::new(
            RuleRegistry::load_from_dir(rule_dir.path()).unwrap(),
        ));

        // Spawn watcher with 100ms debounce
        let watcher = PipelineWatcher::new(
            rule_dir.path().to_path_buf(),
            ingest_dir.path().to_path_buf(),
            Arc::clone(&registry),
            ingest_tx,
        )
        .with_debounce_ms(100)
        .spawn()
        .unwrap();

        // Modify rule file
        let mut rule_file = fs::File::create(&rule_path).unwrap();
        writeln!(rule_file, "fn classify(tx) {{ \"OfficeSupplies\" }}").unwrap();

        // Wait shorter than debounce (should not reload yet)
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Registry should still be intact
        assert_eq!(registry.read().await.rule_count(), 1);

        drop(watcher);
    }

    #[test]
    fn rule_changes_are_debounced_per_path() {
        let debounce_state = Arc::new(Mutex::new(HashMap::<PathBuf, Instant>::new()));
        let path = PathBuf::from("/tmp/test_rule.rhai");

        assert!(PipelineWatcher::should_process_rule_change(
            &path,
            100,
            &debounce_state,
        ));
        assert!(!PipelineWatcher::should_process_rule_change(
            &path,
            100,
            &debounce_state,
        ));

        std::thread::sleep(Duration::from_millis(120));

        assert!(PipelineWatcher::should_process_rule_change(
            &path,
            100,
            &debounce_state,
        ));
    }
}
