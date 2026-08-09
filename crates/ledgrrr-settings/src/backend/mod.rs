//! Platform-agnostic settings storage abstraction.
//!
//! On Windows, settings are stored in the registry at
//! `HKEY_CURRENT_USER\Software\b00t\settings` via the `windows-registry` crate.
//! On other platforms, settings are stored in a JSON file (see `json_file`).
//!
//! The trait is intentionally simple: string-keyed, string-valued.
//! `SettingsStore` in the parent `settings` module handles serialization of
//! the structured `AppSettings` type into the single `"app_settings"` key.

use std::collections::HashMap;

mod json_file;
#[cfg(windows)]
mod windows_registry;

pub use json_file::JsonFileBackend;
#[cfg(windows)]
pub use windows_registry::WindowsRegistryBackend;

use thiserror::Error;

/// Errors that can occur during settings storage operations.
#[derive(Debug, Error)]
pub enum SettingsBackendError {
    /// An I/O error occurred during file operations.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// A JSON serialization/deserialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    /// A platform-specific error (e.g., registry access failure).
    #[error("{0}")]
    Platform(String),
}

/// Key-value settings storage abstraction.
///
/// Each implementation provides platform-appropriate persistence.
/// Methods are thread-safe through external synchronization.
pub trait SettingsBackend: Send {
    /// Read a value by key. Returns `None` if the key does not exist.
    fn get(&self, key: &str) -> Result<Option<String>, SettingsBackendError>;
    /// Write a value by key. Overwrites any existing value for the key.
    fn set(&mut self, key: &str, value: &str) -> Result<(), SettingsBackendError>;
    /// Remove a key-value pair. No-op if the key does not exist.
    fn delete(&mut self, key: &str) -> Result<(), SettingsBackendError>;
    /// Return all key-value pairs in the store.
    fn get_all(&self) -> Result<HashMap<String, String>, SettingsBackendError>;
}

/// Create the platform-appropriate settings backend with JSON file fallback.
///
/// On Windows, tries the registry first and falls back to the JSON file
/// on failure (e.g., permission denied). On other platforms, always uses
/// the JSON file backend.
pub fn create_backend(path: &std::path::Path) -> Box<dyn SettingsBackend> {
    #[cfg(windows)]
    {
        match WindowsRegistryBackend::new() {
            Ok(backend) => return Box::new(backend),
            Err(e) => {
                eprintln!(
                    "ledgerr-host: failed to open registry, \
                     falling back to JSON file: {e}"
                );
            }
        }
    }
    Box::new(JsonFileBackend::new(path.to_path_buf()))
}
