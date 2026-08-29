//! Exercises `JsonFileBackend`'s own atomic-write contract directly.
//!
//! `SettingsStore::new(path)` is *not* used here: on Windows it prefers the
//! registry backend over the JSON file backend for any `path` whose registry
//! access succeeds (see `settings_backend::create_backend`), so these tests
//! would silently never touch a file at all on Windows if written against
//! `SettingsStore`. Atomicity and parent-directory creation are properties
//! of `JsonFileBackend`'s `write_map`, so test that backend directly —
//! deterministic on every platform.

use ledgerr_host::settings_backend::{JsonFileBackend, SettingsBackend};

#[test]
fn creates_parent_directory_on_first_save() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("settings.json");
    let mut backend = JsonFileBackend::new(path.clone());
    backend.set("app_settings", "{}").unwrap();
    assert!(path.exists());
}

#[test]
fn atomic_save_replaces_old_file_without_partial_contents() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let mut backend = JsonFileBackend::new(path.clone());
    backend.set("app_settings", "first").unwrap();
    backend.set("app_settings", "second").unwrap();

    // The temp-file + rename swap must leave exactly the final value on
    // disk — no truncated write, no leftover `.json.tmp`, no merge of both
    // writes.
    let raw = std::fs::read_to_string(&path).unwrap();
    let outer: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(outer["app_settings"], "second");
    assert!(!path.with_extension("json.tmp").exists());
}
