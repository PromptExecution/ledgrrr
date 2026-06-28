use ledgerr_host::settings::{AppSettings, SettingsStore};

#[test]
fn creates_parent_directory_on_first_save() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("settings.json");
    let store = SettingsStore::new(path.clone());
    store.save(&AppSettings::default()).unwrap();
    assert!(path.exists());
}

#[test]
fn atomic_save_replaces_old_file_without_partial_contents() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let store = SettingsStore::new(path.clone());
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
