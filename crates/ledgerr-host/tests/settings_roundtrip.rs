use ledgerr_host::notify::{NotificationBackend, NotificationStatus, NotificationTestResult};
use ledgerr_host::settings::{AppSettings, ChatSettings, SettingsStore};

#[test]
fn load_defaults_when_file_missing() {
    let dir = tempfile::tempdir().unwrap();
    let store = SettingsStore::new(dir.path().join("settings.json"));
    let settings = store.load().unwrap();
    assert!(settings.toast_enabled);
    assert_eq!(
        settings.toast_backend_preference,
        NotificationBackend::PowerShell
    );
    assert_eq!(
        settings.chat.endpoint_url,
        "http://127.0.0.1:15115/v1/chat/completions"
    );
    assert_eq!(settings.chat.model, "phi-4-mini-reasoning");
    assert_eq!(settings.chat.api_key, "local-tool-tray");
}

#[test]
fn save_then_reload_roundtrips_settings() {
    let dir = tempfile::tempdir().unwrap();
    let store = SettingsStore::new(dir.path().join("settings.json"));
    let settings = AppSettings {
        toast_enabled: false,
        window_visible_on_start: false,
        chat: ChatSettings {
            endpoint_url: "https://example.test/v1/chat/completions".into(),
            api_key: "secret".into(),
            model: "gpt-test".into(),
            system_prompt: "Be concise.".into(),
        },
        last_test_result: Some(NotificationTestResult {
            status: NotificationStatus::Ready,
            timestamp: None,
            message: Some("ok".into()),
        }),
        ..AppSettings::default()
    };

    store.save(&settings).unwrap();
    let reloaded = store.load().unwrap();
    assert_eq!(reloaded, settings);
}

#[test]
fn malformed_json_falls_back_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(&path, "{bad json").unwrap();
    let store = SettingsStore::new(path);
    let settings = store.load().unwrap();
    assert_eq!(settings, AppSettings::default());
}

#[test]
fn toggle_toast_enabled_persists_across_fresh_store_instance() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let store = SettingsStore::new(path.clone());
    let mut settings = store.load().unwrap();
    settings.toast_enabled = false;
    store.save(&settings).unwrap();

    let fresh_store = SettingsStore::new(path);
    let reloaded = fresh_store.load().unwrap();
    assert!(!reloaded.toast_enabled);
}

#[test]
fn legacy_v1_settings_without_chat_block_uses_default_chat() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");

    // Simulate V1 settings without a chat block, stored in the new
    // key-value format (`{"app_settings": "<json>"}`).
    let inner = serde_json::json!({
        "schema_version": "v1",
        "toast_enabled": true,
        "toast_backend_preference": "powershell",
        "start_minimized_to_tray": false,
        "window_visible_on_start": true,
        "show_notifications_for": {
            "approval_required": true,
            "transaction_submitted": true,
            "run_failed": true,
            "run_completed": false
        }
    });
    let outer = serde_json::json!({
        "app_settings": inner.to_string()
    });

    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&outer).unwrap(),
    )
    .unwrap();

    let store = SettingsStore::new(path);
    let settings = store.load().unwrap();
    assert_eq!(settings.chat, ChatSettings::default());
}
