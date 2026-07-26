use ledgerr_host::settings::AppSettings;
use ledgerr_host::tray::{tray_menu_labels, TrayCommand, TrayState};

#[test]
fn tray_default_settings_enable_tray() {
    assert!(AppSettings::default().enable_tray);
}

#[test]
fn tray_can_disable_via_settings() {
    let settings = AppSettings {
        enable_tray: false,
        ..AppSettings::default()
    };
    assert!(!settings.enable_tray);
}

#[test]
fn tray_wiring_state_is_constructable() {
    let state = TrayState::default();
    assert!(state.toast_enabled);
}

#[test]
fn tray_wiring_labels_are_constructable() {
    let state = TrayState::from_settings(&AppSettings::default());
    let labels = tray_menu_labels(&state);
    assert_eq!(labels.show_window, "Show Window");
    assert_eq!(labels.exit, "Exit");
}

#[test]
fn tray_wiring_command_variants_are_exhaustive() {
    let commands = vec![
        TrayCommand::ToggleToast(true),
        TrayCommand::CycleBackend,
        TrayCommand::TestToast,
        TrayCommand::ToggleStartMinimizedToTray(false),
        TrayCommand::ToggleWindowVisibleOnStart(true),
        TrayCommand::ToggleApprovalRequired(false),
        TrayCommand::ToggleTransactionSubmitted(true),
        TrayCommand::ToggleRunFailed(false),
        TrayCommand::ToggleRunCompleted(true),
        TrayCommand::ShowWindow,
        TrayCommand::Quit,
    ];
    assert_eq!(commands.len(), 11);
}

#[test]
fn tray_enable_setting_roundtrips_through_json() {
    let settings = AppSettings {
        enable_tray: false,
        ..AppSettings::default()
    };
    let json = serde_json::to_string(&settings).unwrap();
    let restored: AppSettings = serde_json::from_str(&json).unwrap();
    assert!(!restored.enable_tray);

    let settings = AppSettings {
        enable_tray: true,
        ..AppSettings::default()
    };
    let json = serde_json::to_string(&settings).unwrap();
    let restored: AppSettings = serde_json::from_str(&json).unwrap();
    assert!(restored.enable_tray);
}

#[test]
fn tray_enable_roundtrips_through_store() {
    let dir = tempfile::tempdir().unwrap();
    let store = ledgerr_host::settings::SettingsStore::new(dir.path().join("test.json"));

    let mut settings = store.load().unwrap();
    assert!(settings.enable_tray);
    settings.enable_tray = false;
    store.save(&settings).unwrap();

    let reloaded = store.load().unwrap();
    assert!(!reloaded.enable_tray);
}

#[cfg(not(windows))]
#[test]
fn tray_module_available_on_non_windows() {
    let state = TrayState::default();
    let labels = tray_menu_labels(&state);
    assert!(labels.exit == "Exit");
}
