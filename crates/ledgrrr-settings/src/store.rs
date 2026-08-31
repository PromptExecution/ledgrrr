use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use thiserror::Error;

use crate::backend::{create_backend, SettingsBackend, SettingsBackendError};
use crate::schema::{AppSettings, SettingsSchemaVersion};

/// Errors that can occur during settings loading and saving.
#[derive(Debug, Error)]
pub enum SettingsError {
    /// An I/O error during file operations.
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    /// A JSON serialization/deserialization error.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    /// A backend storage error (file system or registry).
    #[error("backend error: {0}")]
    Backend(#[from] SettingsBackendError),
}

/// Thread-safe settings store backed by a platform-appropriate [`SettingsBackend`].
///
/// On Windows, settings persist in the registry at `HKCU\Software\b00t\settings`
/// (with JSON file fallback). On other platforms, settings are stored as a JSON file.
///
/// The `path` field is retained for backward compatibility (display purposes and
/// tests that check the JSON file location).
pub struct SettingsStore {
    path: PathBuf,
    backend: Mutex<Box<dyn SettingsBackend>>,
}

impl fmt::Debug for SettingsStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SettingsStore")
            .field("path", &self.path)
            .field("backend", &"Box<dyn SettingsBackend>")
            .finish()
    }
}

impl SettingsStore {
    /// Create a new settings store backed by the platform-appropriate backend.
    ///
    /// `path` refers to the JSON file location used on non-Windows platforms
    /// (or as fallback on Windows).
    pub fn new(path: PathBuf) -> Self {
        let backend = create_backend(&path);
        Self {
            path,
            backend: Mutex::new(backend),
        }
    }

    /// Create a settings store backed by an explicitly supplied backend,
    /// bypassing the platform auto-selection in [`create_backend`].
    ///
    /// On Windows, `create_backend`'s registry backend ignores its `path`
    /// argument entirely — it always targets the one fixed production
    /// registry key. That's correct for `new()`'s real callers (which all
    /// pass the same production path), but it means any test that wants
    /// genuine isolation (a fresh, empty store per test) must not go
    /// through registry auto-selection at all. Use this with an explicit
    /// `JsonFileBackend` over a tempdir path instead — or use
    /// [`SettingsStore::new_json_file`], a convenience wrapper around this
    /// for exactly that JSON-file case.
    pub fn with_backend(path: PathBuf, backend: Box<dyn SettingsBackend>) -> Self {
        Self {
            path,
            backend: Mutex::new(backend),
        }
    }

    /// Create a settings store that always uses the JSON-file backend,
    /// bypassing platform backend selection entirely.
    ///
    /// On Windows, [`SettingsStore::new`] always prefers the registry backend
    /// (`create_backend` ignores its `path` argument whenever the registry
    /// key can be opened) — so two stores built from different `path`s are
    /// *not* actually isolated from each other there; both end up reading and
    /// writing the same real `HKCU\Software\b00t\settings` key. Any test that
    /// needs a hermetic, path-isolated store (e.g. a `tempfile::tempdir()`
    /// fixture) must use this constructor instead of `new`. A thin wrapper
    /// around [`SettingsStore::with_backend`] for the common JSON-file case.
    pub fn new_json_file(path: PathBuf) -> Self {
        Self::with_backend(
            path.clone(),
            Box::new(crate::backend::JsonFileBackend::new(path)),
        )
    }

    /// Return the JSON file path (for display / backward compatibility).
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Load settings from the backend, returning defaults if no data exists.
    ///
    /// V1→V2 migration is performed in memory: if the stored schema version is V1,
    /// the returned settings will have schema version bumped to V2. The caller can
    /// optionally persist the migration with [`migrate_v1_to_v2`](Self::migrate_v1_to_v2).
    pub fn load(&self) -> Result<AppSettings, SettingsError> {
        let backend = self.backend.lock().expect("settings backend lock poisoned");
        match backend.get("app_settings")? {
            Some(json_str) => {
                match serde_json::from_str::<AppSettings>(&json_str) {
                    Ok(settings) => {
                        // V1→V2 migration: bump schema in memory.
                        // Persist separately via migrate_v1_to_v2().
                        if settings.schema_version == SettingsSchemaVersion::V1 {
                            let mut migrated = settings;
                            migrated.schema_version = SettingsSchemaVersion::V2;
                            return Ok(migrated);
                        }
                        Ok(settings)
                    }
                    // Malformed JSON → fall back to clean defaults.
                    Err(_) => Ok(AppSettings::default()),
                }
            }
            None => Ok(AppSettings::default()),
        }
    }

    /// Migrate V1 settings to V2 on disk. Returns `true` if a migration occurred.
    ///
    /// Separates the read path from the write path to avoid fragile side-effects
    /// during a normal [`load`](Self::load) call.
    pub fn migrate_v1_to_v2(&self) -> Result<bool, SettingsError> {
        let mut backend = self.backend.lock().expect("settings backend lock poisoned");
        match backend.get("app_settings")? {
            Some(json_str) => {
                let settings: AppSettings = serde_json::from_str(&json_str)?;
                if settings.schema_version == SettingsSchemaVersion::V1 {
                    let mut migrated = settings;
                    migrated.schema_version = SettingsSchemaVersion::V2;
                    let updated = serde_json::to_string_pretty(&migrated)?;
                    backend.set("app_settings", &updated)?;
                    return Ok(true);
                }
                Ok(false)
            }
            None => Ok(false),
        }
    }

    /// Persist settings to the backend storage.
    pub fn save(&self, settings: &AppSettings) -> Result<(), SettingsError> {
        let json_str = serde_json::to_string_pretty(settings)?;
        let mut backend = self.backend.lock().expect("settings backend lock poisoned");
        backend.set("app_settings", &json_str)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_returns_defaults_when_backend_is_empty() {
        // A new store with a non-existent path → backend returns None → defaults.
        // Uses new_json_file (not new): on Windows, `new` prefers the real
        // registry backend regardless of this tempdir path, which makes the
        // "empty backend" premise false whenever this dev/CI account already
        // has an app_settings registry value from other real usage.
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new_json_file(dir.path().join("no-such-file.json"));
        let settings = store.load().unwrap();
        assert!(settings.toast_enabled);
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let store = SettingsStore::new_json_file(path.clone());

        let original = AppSettings {
            toast_enabled: false,
            ..AppSettings::default()
        };
        store.save(&original).unwrap();

        let loaded = store.load().unwrap();
        assert_eq!(loaded, original);
    }

    #[test]
    fn new_json_file_stores_are_isolated_by_path() {
        // Two new_json_file stores at different paths must never see each
        // other's data — this is the isolation guarantee `new` cannot
        // provide on Windows (see new_json_file's doc comment).
        let dir = tempfile::tempdir().unwrap();
        let store_a = SettingsStore::new_json_file(dir.path().join("a.json"));
        let store_b = SettingsStore::new_json_file(dir.path().join("b.json"));

        let mut settings_a = store_a.load().unwrap();
        settings_a.toast_enabled = false;
        store_a.save(&settings_a).unwrap();

        let settings_b = store_b.load().unwrap();
        assert!(
            settings_b.toast_enabled,
            "store_b must still see defaults, unaffected by store_a's write"
        );
    }
}
