use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use thiserror::Error;

use super::schema::{AppSettings, SettingsSchemaVersion};
use crate::settings_backend::{create_backend, SettingsBackend, SettingsBackendError};

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

    /// Create a settings store backed by an explicitly injected backend,
    /// bypassing [`create_backend`]'s platform auto-selection.
    ///
    /// Intended for tests: on Windows, `create_backend`/[`SettingsStore::new`]
    /// prefers the registry backend for *any* path, which means tests using
    /// `SettingsStore::new` over a `tempfile::tempdir()` path would otherwise
    /// touch the real `HKCU` registry purely for test isolation. Pass an
    /// explicit `Box<dyn SettingsBackend>` (typically
    /// [`crate::settings_backend::JsonFileBackend`]) to keep a test entirely
    /// on the filesystem. Production code should keep using
    /// [`SettingsStore::new`] for the real, platform-appropriate backend.
    pub fn with_backend(path: PathBuf, backend: Box<dyn SettingsBackend>) -> Self {
        Self {
            path,
            backend: Mutex::new(backend),
        }
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
    use crate::settings_backend::JsonFileBackend;

    // Tests here are exercising `SettingsStore`'s own load/save/migrate logic,
    // not platform backend *selection* — so they use `with_backend` +
    // `JsonFileBackend` directly to stay off the real Windows registry (see
    // `with_backend`'s doc comment).
    fn store_over(path: PathBuf) -> SettingsStore {
        SettingsStore::with_backend(path.clone(), Box::new(JsonFileBackend::new(path)))
    }

    #[test]
    fn load_returns_defaults_when_backend_is_empty() {
        // A new store with a non-existent path → backend returns None → defaults.
        let dir = tempfile::tempdir().unwrap();
        let store = store_over(dir.path().join("no-such-file.json"));
        let settings = store.load().unwrap();
        assert!(settings.toast_enabled);
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let store = store_over(path);

        let original = AppSettings {
            toast_enabled: false,
            ..AppSettings::default()
        };
        store.save(&original).unwrap();

        let loaded = store.load().unwrap();
        assert_eq!(loaded, original);
    }
}
