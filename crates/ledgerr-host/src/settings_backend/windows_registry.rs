//! Windows Registry settings backend.
//!
//! Stores key-value pairs under `HKEY_CURRENT_USER\Software\b00t\settings`.
//! Each setting is a `REG_SZ` value named by the key.
//!
//! This module is only compiled on Windows targets (see `#[cfg(windows)]` on
//! the `mod windows_registry` declaration in the parent module).

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

use windows_registry::{CURRENT_USER, Key};

use super::{SettingsBackend, SettingsBackendError};

/// Registry path under HKCU where the real app's settings are stored.
///
/// Every production binary opens its store at `settings::default_settings_path()`,
/// so that exact path is special-cased to keep resolving here — preserving
/// already-persisted real user settings across releases.
const SETTINGS_PATH: &str = r"software\b00t\settings";

/// A settings backend backed by the Windows Registry.
///
/// Opens and closes the registry key on each operation. This avoids holding
/// a `Key` handle (which is `!Send + !Sync`) across thread boundaries and
/// keeps the struct trivially `Send`.
pub struct WindowsRegistryBackend {
    /// Registry key path under HKCU, derived from the `path` this backend
    /// was constructed with (see [`Self::subkey_for`]).
    subkey: String,
}

impl WindowsRegistryBackend {
    /// Create a new registry backend scoped to `path`. Validates the key can
    /// be opened/created.
    ///
    /// The registry has no notion of the caller's JSON-file `path`, so a
    /// fixed key alone would make every `SettingsStore` — including ones
    /// tests create over distinct temp directories, and a second real
    /// process instance — silently share one mutable global key. That
    /// previously caused racy, mutually-clobbering test failures, and would
    /// do the same to two concurrently running app instances. Deriving the
    /// key from `path` gives each non-default path its own isolated key.
    pub fn new(path: &Path) -> Result<Self, SettingsBackendError> {
        let backend = Self {
            subkey: Self::subkey_for(path),
        };
        let _key = backend.open_key()?;
        Ok(backend)
    }

    fn subkey_for(path: &Path) -> String {
        if path == crate::settings::default_settings_path() {
            return SETTINGS_PATH.to_string();
        }
        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        format!(r"{SETTINGS_PATH}-scoped\{:016x}", hasher.finish())
    }

    fn open_key(&self) -> Result<Key, SettingsBackendError> {
        CURRENT_USER
            .options()
            .read()
            .write()
            .create()
            .open(&self.subkey)
            .map_err(|e| {
                SettingsBackendError::Platform(format!("failed to open registry key: {e}"))
            })
    }
}

impl SettingsBackend for WindowsRegistryBackend {
    fn get(&self, key: &str) -> Result<Option<String>, SettingsBackendError> {
        let k = self.open_key()?;
        match k.get_string(key) {
            Ok(v) => Ok(Some(v)),
            Err(_) => Ok(None),
        }
    }

    fn set(&mut self, key: &str, value: &str) -> Result<(), SettingsBackendError> {
        let k = self.open_key()?;
        k.set_string(key, value)
            .map_err(|e| SettingsBackendError::Platform(format!("registry write failed: {e}")))?;
        Ok(())
    }

    fn delete(&mut self, key: &str) -> Result<(), SettingsBackendError> {
        let k = self.open_key()?;
        k.remove_value(key)
            .map_err(|e| SettingsBackendError::Platform(format!("registry delete failed: {e}")))?;
        Ok(())
    }

    fn get_all(&self) -> Result<HashMap<String, String>, SettingsBackendError> {
        let mut map = HashMap::new();

        // Query known settings keys. Currently only "app_settings" is used
        // by SettingsStore. This can be extended as more keys are added.
        if let Some(val) = self.get("app_settings")? {
            map.insert("app_settings".to_owned(), val);
        }

        Ok(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_path_keeps_the_stable_production_key() {
        assert_eq!(
            WindowsRegistryBackend::subkey_for(&crate::settings::default_settings_path()),
            SETTINGS_PATH
        );
    }

    #[test]
    fn distinct_non_default_paths_get_distinct_isolated_keys() {
        let a = WindowsRegistryBackend::subkey_for(Path::new(r"C:\temp\a\settings.json"));
        let b = WindowsRegistryBackend::subkey_for(Path::new(r"C:\temp\b\settings.json"));
        assert_ne!(a, b);
        assert_ne!(a, SETTINGS_PATH);
    }

    #[test]
    fn same_non_default_path_is_deterministic() {
        let path = Path::new(r"C:\temp\same\settings.json");
        assert_eq!(
            WindowsRegistryBackend::subkey_for(path),
            WindowsRegistryBackend::subkey_for(path)
        );
    }
}
