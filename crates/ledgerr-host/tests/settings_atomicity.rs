use ledgerr_host::settings::{AppSettings, SettingsStore};

/// `SettingsStore::new`'s registry backend (on Windows) ignores its `path`
/// argument and always targets the one fixed production key — correct for
/// real callers, but it would make every test here share one mutable global
/// registry key (and could corrupt a real running host-tauri.exe's settings
/// on a dev machine). Use an explicit `JsonFileBackend` over the given
/// tempdir path instead, for genuine per-test isolation.
fn test_store(path: std::path::PathBuf) -> SettingsStore {
    SettingsStore::with_backend(
        path.clone(),
        Box::new(ledgrrr_settings::backend::JsonFileBackend::new(path)),
    )
}

#[test]
fn creates_parent_directory_on_first_save() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("settings.json");
    let store = test_store(path.clone());
    store.save(&AppSettings::default()).unwrap();
    assert!(path.exists());
}

#[test]
fn atomic_save_replaces_old_file_without_partial_contents() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let store = test_store(path.clone());
    store.save(&AppSettings::default()).unwrap();

    let updated = AppSettings {
        toast_enabled: false,
        start_minimized_to_tray: true,
        ..AppSettings::default()
    };
    store.save(&updated).unwrap();

    let raw = std::fs::read_to_string(path).unwrap();

    // With the new backend format, settings are serialized as a JSON string
    // under the `"app_settings"` key. Parse the outer JSON to extract the
    // inner AppSettings content and verify the values were persisted.
    let outer: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let inner_str = outer["app_settings"]
        .as_str()
        .expect("app_settings should be a JSON string");
    let inner: serde_json::Value = serde_json::from_str(inner_str).unwrap();

    assert_eq!(inner["toast_enabled"], false);
    assert_eq!(inner["start_minimized_to_tray"], true);
}
