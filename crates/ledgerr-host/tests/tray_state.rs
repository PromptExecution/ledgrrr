use ledgerr_host::notify::{NotificationBackend, NotificationStatus, NotificationTestResult};
use ledgerr_host::settings::{AppSettings, ShowNotificationsFor};
use ledgerr_host::tray::{tray_menu_labels, TrayCommand, TrayState};

#[test]
fn tray_default_state_is_sane() {
    let state = TrayState::default();
    assert!(state.toast_enabled);
    assert_eq!(state.notification_backend, NotificationBackend::Native);
    assert_eq!(state.notification_status, NotificationStatus::Unknown);
}

#[test]
fn tray_menu_renders_disabled_status() {
    let state = TrayState {
        toast_enabled: false,
        notification_backend: NotificationBackend::Noop,
        notification_status: NotificationStatus::Disabled,
        last_test_result: Some(NotificationTestResult {
            status: NotificationStatus::Disabled,
            timestamp: None,
            message: Some("noop backend selected".into()),
        }),
        start_minimized_to_tray: true,
        window_visible_on_start: false,
        show_notifications_for: ShowNotificationsFor::default(),
        window_visible: false,
    };
    let labels = tray_menu_labels(&state);
    assert_eq!(labels.backend, "Backend: Noop");
    assert_eq!(
        labels.last_test,
        "Last Test: Disabled (noop backend selected)"
    );
    assert_eq!(labels.status, "Status: Disabled");
}

#[test]
fn tray_command_toggle_preserves_explicit_value() {
    let command = TrayCommand::ToggleToast(false);
    match command {
        TrayCommand::ToggleToast(value) => assert!(!value),
        _ => panic!("unexpected tray command"),
    }
}

#[test]
fn tray_state_can_be_derived_from_settings() {
    let settings = AppSettings {
        toast_enabled: true,
        toast_backend_preference: NotificationBackend::Auto,
        start_minimized_to_tray: true,
        window_visible_on_start: false,
        show_notifications_for: ShowNotificationsFor {
            approval_required: true,
            transaction_submitted: false,
            run_failed: true,
            run_completed: true,
        },
        last_test_result: Some(NotificationTestResult {
            status: NotificationStatus::Degraded,
            timestamp: None,
            message: Some("toast took fallback path".into()),
        }),
        ..AppSettings::default()
    };

    let state = TrayState::from_settings(&settings);
    assert!(state.toast_enabled);
    assert_eq!(state.notification_backend, NotificationBackend::Auto);
    assert_eq!(state.notification_status, NotificationStatus::Degraded);
    assert!(state.start_minimized_to_tray);
    assert!(!state.window_visible_on_start);
    assert!(!state.show_notifications_for.transaction_submitted);
    assert!(state.show_notifications_for.run_completed);
}
