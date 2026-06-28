use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

use super::{SettingsBackend, SettingsBackendError};

/// A settings backend that stores key-value pairs as a JSON object in a file.
///
/// The file path is platform-dependent and determined by [`crate::settings::default_settings_path`].
/// Each key is a top-level field in the JSON object. The primary key used by
/// `SettingsStore` is `"app_settings"`, which holds the serialized `AppSettings`.
pub struct JsonFileBackend {
    path: PathBuf,
}

impl JsonFileBackend {
    /// Create a new JSON file backend backed by the given path.
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Read the entire key-value map from disk. Returns an empty map on `NotFound`.
    fn read_map(&self) -> Result<HashMap<String, String>, SettingsBackendError> {
        match std::fs::read_to_string(&self.path) {
            Ok(raw) => Ok(serde_json::from_str(&raw).unwrap_or_default()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(HashMap::new()),
            Err(e) => Err(SettingsBackendError::Io(e)),
        }
    }

    /// Atomically write the key-value map to disk using a temp-file + rename strategy.
    fn write_map(&self, map: &HashMap<String, String>) -> Result<(), SettingsBackendError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let temp_path = self.path.with_extension("json.tmp");
        let json = serde_json::to_vec_pretty(map)?;
        let mut temp = std::fs::File::create(&temp_path)?;
        temp.write_all(&json)?;
        temp.flush()?;
        drop(temp);

        // On Windows, `std::fs::rename` does not overwrite an existing destination.
        #[cfg(windows)]
        if self.path.exists() {
            std::fs::remove_file(&self.path)?;
        }

        std::fs::rename(temp_path, &self.path)?;
        Ok(())
    }
}

impl SettingsBackend for JsonFileBackend {
    fn get(&self, key: &str) -> Result<Option<String>, SettingsBackendError> {
        let map = self.read_map()?;
        Ok(map.get(key).cloned())
    }

    fn set(&mut self, key: &str, value: &str) -> Result<(), SettingsBackendError> {
        let mut map = self.read_map()?;
        map.insert(key.to_owned(), value.to_owned());
        self.write_map(&map)
    }

    fn delete(&mut self, key: &str) -> Result<(), SettingsBackendError> {
        let mut map = self.read_map()?;
        map.remove(key);
        self.write_map(&map)
    }

    fn get_all(&self) -> Result<HashMap<String, String>, SettingsBackendError> {
        self.read_map()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn missing_file_returns_empty_get() {
        let dir = tempdir().unwrap();
        let backend = JsonFileBackend::new(dir.path().join("nonexistent.json"));
        assert_eq!(backend.get("any").unwrap(), None);
    }

    #[test]
    fn set_then_get_roundtrips() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.json");
        let mut backend = JsonFileBackend::new(path.clone());
        backend.set("greeting", "hello").unwrap();
        assert_eq!(backend.get("greeting").unwrap(), Some("hello".into()));
    }

    #[test]
    fn delete_removes_key() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.json");
        let mut backend = JsonFileBackend::new(path.clone());
        backend.set("k1", "v1").unwrap();
        backend.set("k2", "v2").unwrap();
        backend.delete("k1").unwrap();
        assert_eq!(backend.get("k1").unwrap(), None);
        assert_eq!(backend.get("k2").unwrap(), Some("v2".into()));
    }

    #[test]
    fn get_all_returns_all_keys() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.json");
        let mut backend = JsonFileBackend::new(path.clone());
        backend.set("a", "1").unwrap();
        backend.set("b", "2").unwrap();
        let all = backend.get_all().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all.get("a"), Some(&"1".into()));
        assert_eq!(all.get("b"), Some(&"2".into()));
    }
}
