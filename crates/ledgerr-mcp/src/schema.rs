use std::collections::BTreeMap;
use std::path::Path;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::ToolError;

/// A custom entity kind registered at runtime (no recompile needed).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomKind {
    pub name: String,
    pub description: String,
    pub created_at: String,
    /// Optional per-attribute type hints (e.g. {"amount": "decimal", "date": "string"}).
    pub attrs_schema: BTreeMap<String, String>,
}

/// Schema registry: built-in kinds (from ArtifactKind) + custom kinds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaKinds {
    pub built_in: Vec<String>,
    pub custom: Vec<CustomKind>,
}

fn built_in_kind_names() -> Vec<String> {
    vec![
        "document",
        "account",
        "institution",
        "transaction",
        "tax_category",
        "evidence_reference",
        "xero_contact",
        "xero_bank_account",
        "xero_invoice",
        "workflow_tag",
        "model_job",
        "model_proposal",
        "workbook_row",
        "audit_event",
        "validation_issue",
        "document_chunk",
        "classification_outcome",
    ]
    .into_iter()
    .map(|s| s.to_string())
    .collect()
}

impl Default for SchemaKinds {
    fn default() -> Self {
        Self {
            built_in: built_in_kind_names(),
            custom: Vec::new(),
        }
    }
}

/// Runtime schema store for entity kinds. Parallels OntologyStore's persistence pattern.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SchemaStore {
    pub kinds: SchemaKinds,
}

impl SchemaStore {
    /// Validate that the given path does not contain path traversal components
    /// (ParentDir, i.e. `..`). Mirrors the `allowed_base` pattern from ontology.rs.
    fn validate_path_allowed(path: &Path) -> Result<(), ToolError> {
        if path
            .components()
            .any(|c| c == std::path::Component::ParentDir)
        {
            return Err(ToolError::InvalidInput(format!(
                "path '{}' contains path traversal components (..)",
                path.display()
            )));
        }
        Ok(())
    }

    /// Load from a JSON file, or return a default with built-in kinds.
    /// On corruption, falls back to .bak file if available, logging a warning.
    /// If both main and backup are corrupt, returns default SchemaStore.
    pub fn load(path: &Path) -> Result<Self, ToolError> {
        Self::validate_path_allowed(path)?;
        if !path.exists() {
            return Ok(Self::default());
        }

        // Try loading the main file
        match Self::load_from_file(path) {
            Ok(store) => return Ok(store),
            Err(e) => {
                // Corrupt main file — try .bak
                let bak_path = path.with_extension("json.bak");
                if bak_path.exists() {
                    tracing::warn!(
                        "SchemaStore main file corrupt ({}), falling back to backup at {}",
                        e,
                        bak_path.display()
                    );
                    match Self::load_from_file(&bak_path) {
                        Ok(store) => {
                            // Try to rewrite main from backup to self-heal
                            let _ = store.persist(path);
                            return Ok(store);
                        }
                        Err(bak_err) => {
                            tracing::error!(
                                "SchemaStore backup also corrupt ({}), returning default",
                                bak_err
                            );
                        }
                    }
                } else {
                    tracing::warn!(
                        "SchemaStore main file corrupt ({}), no backup found, returning default",
                        e
                    );
                }
            }
        }

        // Both corrupt or no backup — return default
        Ok(Self::default())
    }

    /// Internal helper: load and parse from a specific file path.
    fn load_from_file(path: &Path) -> Result<Self, ToolError> {
        let raw = std::fs::read_to_string(path).map_err(|e| ToolError::Internal(e.to_string()))?;
        let mut store: Self =
            serde_json::from_str(&raw).map_err(|e| ToolError::Internal(e.to_string()))?;
        store.rebuild_built_in_list();
        Ok(store)
    }

    /// Persist to a JSON file using atomic write: write to .tmp then rename,
    /// then create a .bak backup of the previous file.
    pub fn persist(&self, path: &Path) -> Result<(), ToolError> {
        Self::validate_path_allowed(path)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ToolError::Internal(e.to_string()))?;
        }
        let payload =
            serde_json::to_string_pretty(self).map_err(|e| ToolError::Internal(e.to_string()))?;

        // Atomic write: write to .tmp first, then rename
        let tmp_path = path.with_extension("json.tmp");
        std::fs::write(&tmp_path, &payload)
            .map_err(|e| ToolError::Internal(format!("failed to write tmp: {e}")))?;

        // Backup existing main file to .bak before overwriting
        if path.exists() {
            let bak_path = path.with_extension("json.bak");
            let _ = std::fs::copy(path, &bak_path);
        }

        std::fs::rename(&tmp_path, path)
            .map_err(|e| ToolError::Internal(format!("failed to rename tmp: {e}")))?;

        Ok(())
    }

    /// Register a new custom entity kind. Returns error if name already exists (built-in or custom).
    pub fn register_kind(
        &mut self,
        name: &str,
        description: &str,
        attrs_schema: BTreeMap<String, String>,
    ) -> Result<(), ToolError> {
        let normal = name.trim().to_lowercase();
        if normal.is_empty() {
            return Err(ToolError::InvalidInput(
                "kind name must not be empty".to_string(),
            ));
        }
        // Check built-in variants (case-insensitive).
        if self
            .kinds
            .built_in
            .iter()
            .any(|b| b.eq_ignore_ascii_case(&normal))
        {
            return Err(ToolError::InvalidInput(format!(
                "kind '{normal}' is already a built-in ArtifactKind"
            )));
        }
        // Check existing custom kinds.
        if self
            .kinds
            .custom
            .iter()
            .any(|c| c.name.eq_ignore_ascii_case(&normal))
        {
            return Err(ToolError::InvalidInput(format!(
                "kind '{normal}' is already registered"
            )));
        }

        let now = Utc::now().to_rfc3339();
        self.kinds.custom.push(CustomKind {
            name: normal.clone(),
            description: description.to_string(),
            created_at: now,
            attrs_schema,
        });
        Ok(())
    }

    /// Remove a custom entity kind by name. Returns error if not found or is built-in.
    pub fn remove_kind(&mut self, name: &str) -> Result<(), ToolError> {
        let normal = name.trim().to_lowercase();
        if self
            .kinds
            .built_in
            .iter()
            .any(|b| b.eq_ignore_ascii_case(&normal))
        {
            return Err(ToolError::InvalidInput(format!(
                "cannot remove built-in kind '{normal}'"
            )));
        }
        let before = self.kinds.custom.len();
        self.kinds
            .custom
            .retain(|c| !c.name.eq_ignore_ascii_case(&normal));
        if self.kinds.custom.len() == before {
            return Err(ToolError::InvalidInput(format!(
                "custom kind '{normal}' not found"
            )));
        }
        Ok(())
    }

    /// List all kinds: built-in names + custom kind details.
    pub fn list_kinds(&self) -> &SchemaKinds {
        &self.kinds
    }

    /// Get a specific kind by name (searches built-in first, then custom).
    pub fn get_kind(&self, name: &str) -> Option<KindInfo<'_>> {
        let normal = name.trim().to_lowercase();
        if self.kinds.built_in.iter().any(|b| b == &normal) {
            return Some(KindInfo::BuiltIn(normal));
        }
        self.kinds
            .custom
            .iter()
            .find(|c| c.name == normal)
            .map(KindInfo::Custom)
    }

    /// Check if a kind name is known (built-in or custom).
    pub fn is_known_kind(&self, name: &str) -> bool {
        let normal = name.trim().to_lowercase();
        self.kinds.built_in.iter().any(|b| b == &normal)
            || self.kinds.custom.iter().any(|c| c.name == normal)
    }

    fn rebuild_built_in_list(&mut self) {
        self.kinds.built_in = built_in_kind_names();
    }
}

/// Info about a kind, returned from `get_kind`.
#[derive(Debug, Clone)]
pub enum KindInfo<'a> {
    BuiltIn(String),
    Custom(&'a CustomKind),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_default_has_all_built_in_kinds() {
        let store = SchemaStore::default();
        assert_eq!(store.kinds.built_in.len(), 17);
        assert!(store.kinds.custom.is_empty());
        assert!(store.is_known_kind("document"));
        assert!(store.is_known_kind("transaction"));
    }

    #[test]
    fn schema_register_custom_kind() {
        let mut store = SchemaStore::default();
        let mut schema = BTreeMap::new();
        schema.insert("currency".to_string(), "string".to_string());

        store
            .register_kind("custom_asset", "A custom financial asset", schema.clone())
            .unwrap();
        assert_eq!(store.kinds.custom.len(), 1);
        assert_eq!(store.kinds.custom[0].name, "custom_asset");
        assert!(store.is_known_kind("custom_asset"));
    }

    #[test]
    fn schema_register_duplicate_rejected() {
        let mut store = SchemaStore::default();
        store
            .register_kind("my_kind", "test", BTreeMap::new())
            .unwrap();
        let result = store.register_kind("my_kind", "duplicate", BTreeMap::new());
        assert!(result.is_err());
    }

    #[test]
    fn schema_register_built_in_rejected() {
        let mut store = SchemaStore::default();
        let result = store.register_kind("document", "trying to override", BTreeMap::new());
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("built-in ArtifactKind"));
    }

    #[test]
    fn schema_remove_custom_kind() {
        let mut store = SchemaStore::default();
        store
            .register_kind("temp_kind", "temporary", BTreeMap::new())
            .unwrap();
        store.remove_kind("temp_kind").unwrap();
        assert!(store.kinds.custom.is_empty());
    }

    #[test]
    fn schema_remove_built_in_rejected() {
        let mut store = SchemaStore::default();
        let result = store.remove_kind("document");
        assert!(result.is_err());
    }

    #[test]
    fn schema_roundtrip_persist() {
        use tempfile::tempdir;
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("schema.json");

        let mut store = SchemaStore::default();
        store
            .register_kind("my_type", "A custom type", BTreeMap::new())
            .unwrap();
        store.persist(&path).unwrap();

        let loaded = SchemaStore::load(&path).unwrap();
        assert!(loaded.is_known_kind("my_type"));
        assert_eq!(loaded.kinds.custom.len(), 1);
        assert_eq!(loaded.kinds.custom[0].description, "A custom type");
    }

    #[test]
    fn schema_get_kind() {
        let mut store = SchemaStore::default();
        store
            .register_kind("custom_thing", "My thing", BTreeMap::new())
            .unwrap();

        let built_in = store.get_kind("document");
        assert!(matches!(built_in, Some(KindInfo::BuiltIn(_))));

        let custom = store.get_kind("custom_thing");
        assert!(matches!(custom, Some(KindInfo::Custom(_))));

        let unknown = store.get_kind("nonexistent");
        assert!(unknown.is_none());
    }

    #[test]
    fn schema_load_rejects_path_traversal() {
        let result = SchemaStore::load(Path::new("../outside.json"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("path traversal"));

        let result = SchemaStore::load(Path::new("subdir/../../outside.json"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("path traversal"));
    }

    #[test]
    fn schema_persist_rejects_path_traversal() {
        let store = SchemaStore::default();
        let result = store.persist(Path::new("../outside.json"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("path traversal"));
    }

    #[test]
    fn schema_accepts_normal_paths() {
        use tempfile::tempdir;
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("valid_schema.json");

        let mut store = SchemaStore::default();
        store
            .register_kind("safe_kind", "Safe", BTreeMap::new())
            .unwrap();
        store.persist(&path).unwrap();

        let loaded = SchemaStore::load(&path).unwrap();
        assert!(loaded.is_known_kind("safe_kind"));
    }
}
