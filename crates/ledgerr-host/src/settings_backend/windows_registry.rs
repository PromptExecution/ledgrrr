//! Windows Registry settings backend.
//!
//! Stores key-value pairs under `HKEY_CURRENT_USER\Software\b00t\settings`.
//! Each setting is a `REG_SZ` value named by the key.
//!
//! This module is only compiled on Windows targets (see `#[cfg(windows)]` on
//! the `mod windows_registry` declaration in the parent module).

use std::collections::HashMap;

use windows_registry::*;

use super::{SettingsBackend, SettingsBackendError};

/// Registry path under HKCU where settings are stored.
const SETTINGS_PATH: &str = r"software\b00t\settings";

/// A settings backend backed by the Windows Registry.
///
/// Opens and closes the registry key on each operation. This avoids holding
/// a `Key` handle (which is `!Send + !Sync`) across thread boundaries and
/// keeps the struct trivially `Send`.
pub struct WindowsRegistryBackend;

impl WindowsRegistryBackend {
    /// Create a new registry backend. Validates the key can be opened/created.
    pub fn new() -> Result<Self, SettingsBackendError> {
        let _key = Self::open_key()?;
        Ok(Self)
    }

    fn open_key() -> Result<Key, SettingsBackendError> {
        CURRENT_USER
            .options()
            .read()
            .write()
            .create()
            .open(SETTINGS_PATH)
            .map_err(|e| {
                SettingsBackendError::Platform(format!("failed to open registry key: {e}"))
            })
    }
}

impl SettingsBackend for WindowsRegistryBackend {
    fn get(&self, key: &str) -> Result<Option<String>, SettingsBackendError> {
        let k = Self::open_key()?;
        match k.get_string(key) {
            Ok(v) => Ok(Some(v)),
            Err(_) => Ok(None),
        }
    }

    fn set(&mut self, key: &str, value: &str) -> Result<(), SettingsBackendError> {
        let k = Self::open_key()?;
        k.set_string(key, value)
            .map_err(|e| SettingsBackendError::Platform(format!("registry write failed: {e}")))?;
        Ok(())
    }

    fn delete(&mut self, key: &str) -> Result<(), SettingsBackendError> {
        let k = Self::open_key()?;
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
