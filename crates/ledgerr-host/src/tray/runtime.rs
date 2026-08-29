use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;

use crate::notify::{
    NotificationBackend, NotificationEvent, NotificationSettings, NotificationStatus,
    NotificationTestResult, Notifier, NotifyError, PowerShellBurntToastNotifier,
};
use crate::settings::{AppSettings, SettingsStore};

use super::native::{
    make_icon_data, NativeTrayPlatform, TrayControl, TrayEvent, CMD_CYCLE_BACKEND,
    CMD_EXIT, CMD_NOTIFY_APPROVAL, CMD_NOTIFY_COMPLETED, CMD_NOTIFY_FAILED,
    CMD_NOTIFY_SUBMITTED, CMD_SHOW_WINDOW, CMD_START_MINIMIZED, CMD_TEST_TOAST,
    CMD_TOAST_ENABLED, CMD_WINDOW_VISIBLE,
};
use super::{tray_menu_labels, TrayCommand, TrayState};

pub fn run(
    store: SettingsStore,
    show_window: impl Fn() -> Result<(), Box<dyn std::error::Error>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let settings = store.load()?;
    let state = Arc::new(Mutex::new(TrayState::from_settings(&settings)));
    let labels = tray_menu_labels(&state.lock().expect("tray state poisoned"));

    let (rgba, width, height) = make_icon_data();

    let mut tray = NativeTrayPlatform::spawn(
        &format!("l3dg3rr {}", env!("CARGO_PKG_VERSION")),
        rgba,
        width,
        height,
        &labels,
    )?;

    send_best_effort_toast(
        &settings,
        NotificationEvent::Test {
            title: "l3dg3rr".to_string(),
            body: format!("Hello from l3dg3rr {}", env!("CARGO_PKG_VERSION")),
        },
    );

    loop {
        if let Ok(event) = tray.event_rx.recv_timeout(Duration::from_millis(250)) {
            let command = match event {
                TrayEvent::MenuCommand(id) => match id {
                    CMD_TOAST_ENABLED => {
                        negate(&state, |s| s.toast_enabled, TrayCommand::ToggleToast)
                    }
                    CMD_CYCLE_BACKEND => TrayCommand::CycleBackend,
                    CMD_TEST_TOAST => TrayCommand::TestToast,
                    CMD_START_MINIMIZED => negate(
                        &state,
                        |s| s.start_minimized_to_tray,
                        TrayCommand::ToggleStartMinimizedToTray,
                    ),
                    CMD_WINDOW_VISIBLE => negate(
                        &state,
                        |s| s.window_visible_on_start,
                        TrayCommand::ToggleWindowVisibleOnStart,
                    ),
                    CMD_NOTIFY_APPROVAL => negate(
                        &state,
                        |s| s.show_notifications_for.approval_required,
                        TrayCommand::ToggleApprovalRequired,
                    ),
                    CMD_NOTIFY_SUBMITTED => negate(
                        &state,
                        |s| s.show_notifications_for.transaction_submitted,
                        TrayCommand::ToggleTransactionSubmitted,
                    ),
                    CMD_NOTIFY_FAILED => negate(
                        &state,
                        |s| s.show_notifications_for.run_failed,
                        TrayCommand::ToggleRunFailed,
                    ),
                    CMD_NOTIFY_COMPLETED => negate(
                        &state,
                        |s| s.show_notifications_for.run_completed,
                        TrayCommand::ToggleRunCompleted,
                    ),
                    CMD_SHOW_WINDOW => TrayCommand::ShowWindow,
                    CMD_EXIT => TrayCommand::Quit,
                    _ => continue,
                },
            };

            let should_quit =
                handle_command(command, &store, &state, &tray.control_tx, &show_window)?;
            if should_quit {
                break;
            }
        }
    }

    tray.shutdown();
    Ok(())
}

/// Read one `TrayState` field and wrap its negation as the `TrayCommand`
/// that would set it to that new value — the shared shape behind every
/// checkbox-style tray menu item ("read current, flip it, emit the command
/// that persists the flip").
fn negate(
    state: &Arc<Mutex<TrayState>>,
    read: impl FnOnce(&TrayState) -> bool,
    wrap: impl FnOnce(bool) -> TrayCommand,
) -> TrayCommand {
    let current = read(&state.lock().expect("tray state poisoned"));
    wrap(!current)
}

/// Persist a single-field settings mutation and push the resulting labels to
/// the tray — the shared shape behind every toggle command in
/// [`handle_command`]: load, mutate one field, save, sync.
fn apply_toggle(
    store: &SettingsStore,
    state: &Arc<Mutex<TrayState>>,
    control_tx: &mpsc::Sender<TrayControl>,
    set: impl FnOnce(&mut AppSettings),
) -> Result<bool, Box<dyn std::error::Error>> {
    let mut settings = store.load()?;
    set(&mut settings);
    store.save(&settings)?;

    sync_state(state, &settings, control_tx);
    Ok(false)
}

fn handle_command(
    command: TrayCommand,
    store: &SettingsStore,
    state: &Arc<Mutex<TrayState>>,
    control_tx: &mpsc::Sender<TrayControl>,
    show_window: &dyn Fn() -> Result<(), Box<dyn std::error::Error>>,
) -> Result<bool, Box<dyn std::error::Error>> {
    match command {
        TrayCommand::ToggleToast(enabled) => {
            apply_toggle(store, state, control_tx, |s| s.toast_enabled = enabled)
        }
        TrayCommand::CycleBackend => {
            let mut settings = store.load()?;
            settings.toast_backend_preference = next_backend(settings.toast_backend_preference);
            store.save(&settings)?;

            sync_state(state, &settings, control_tx);
            Ok(false)
        }
        TrayCommand::TestToast => {
            let mut settings = store.load()?;
            let test_result = match run_notification_test(&settings) {
                Ok(result) => result,
                Err(error) => NotificationTestResult {
                    status: NotificationStatus::Failed,
                    timestamp: Some(Utc::now()),
                    message: Some(error.to_string()),
                },
            };
            settings.last_test_result = Some(test_result);
            store.save(&settings)?;

            sync_state(state, &settings, control_tx);
            Ok(false)
        }
        TrayCommand::ToggleStartMinimizedToTray(enabled) => apply_toggle(
            store,
            state,
            control_tx,
            |s| s.start_minimized_to_tray = enabled,
        ),
        TrayCommand::ToggleWindowVisibleOnStart(enabled) => apply_toggle(
            store,
            state,
            control_tx,
            |s| s.window_visible_on_start = enabled,
        ),
        TrayCommand::ToggleApprovalRequired(enabled) => apply_toggle(store, state, control_tx, |s| {
            s.show_notifications_for.approval_required = enabled
        }),
        TrayCommand::ToggleTransactionSubmitted(enabled) => {
            apply_toggle(store, state, control_tx, |s| {
                s.show_notifications_for.transaction_submitted = enabled
            })
        }
        TrayCommand::ToggleRunFailed(enabled) => apply_toggle(store, state, control_tx, |s| {
            s.show_notifications_for.run_failed = enabled
        }),
        TrayCommand::ToggleRunCompleted(enabled) => apply_toggle(store, state, control_tx, |s| {
            s.show_notifications_for.run_completed = enabled
        }),
        TrayCommand::ShowWindow => {
            if let Ok(mut state) = state.lock() {
                state.window_visible = true;
            }
            show_window()?;
            Ok(false)
        }
        TrayCommand::Quit => {
            let settings = store.load()?;
            send_best_effort_toast(
                &settings,
                NotificationEvent::Test {
                    title: "l3dg3rr".to_string(),
                    body: "Goodbye from l3dg3rr".to_string(),
                },
            );
            Ok(true)
        }
    }
}

fn sync_state(
    state: &Arc<Mutex<TrayState>>,
    settings: &AppSettings,
    control_tx: &mpsc::Sender<TrayControl>,
) {
    let mut state_guard = state.lock().expect("tray state poisoned");
    state_guard.apply_settings(settings);
    let labels = tray_menu_labels(&state_guard);
    let _ = control_tx.send(TrayControl::UpdateLabels {
        version: labels.version,
        backend: labels.backend,
        last_test: labels.last_test,
        status: labels.status,
        toast_enabled: state_guard.toast_enabled,
        start_minimized: state_guard.start_minimized_to_tray,
        window_visible: state_guard.window_visible_on_start,
        notify_approval: state_guard.show_notifications_for.approval_required,
        notify_submitted: state_guard.show_notifications_for.transaction_submitted,
        notify_failed: state_guard.show_notifications_for.run_failed,
        notify_completed: state_guard.show_notifications_for.run_completed,
    });
}

fn next_backend(current: NotificationBackend) -> NotificationBackend {
    match current {
        NotificationBackend::Auto => NotificationBackend::PowerShell,
        NotificationBackend::PowerShell => NotificationBackend::Noop,
        NotificationBackend::Noop => NotificationBackend::Auto,
    }
}

fn run_notification_test(settings: &AppSettings) -> Result<NotificationTestResult, NotifyError> {
    match settings.toast_backend_preference {
        NotificationBackend::Noop => Ok(NotificationTestResult {
            status: NotificationStatus::Disabled,
            timestamp: Some(Utc::now()),
            message: Some("noop backend selected".to_string()),
        }),
        NotificationBackend::Auto | NotificationBackend::PowerShell => {
            let notify_settings = NotificationSettings {
                enabled: settings.toast_enabled,
                backend: settings.toast_backend_preference,
                last_test_result: settings.last_test_result.clone(),
            };
            let notifier = PowerShellBurntToastNotifier::new(notify_settings);
            notifier.test("l3dg3rr", "tray test toast")
        }
    }
}

fn send_best_effort_toast(settings: &AppSettings, event: NotificationEvent) {
    if matches!(settings.toast_backend_preference, NotificationBackend::Noop) {
        return;
    }

    let notify_settings = NotificationSettings {
        enabled: settings.toast_enabled,
        backend: settings.toast_backend_preference,
        last_test_result: settings.last_test_result.clone(),
    };
    let notifier = PowerShellBurntToastNotifier::new(notify_settings);
    let _ = notifier.notify(&event);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noop_show_window() -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    /// Drives `handle_command` for a single toggle variant and asserts three
    /// things every toggle handler must get right: (1) it doesn't request
    /// quit, (2) the new value is durably persisted (visible from a *fresh*
    /// `SettingsStore` over the same path, not just the in-memory one), and
    /// (3) the tray gets a matching `TrayControl::UpdateLabels` so the menu
    /// checkmark actually updates. Parameterized so each of the seven
    /// near-identical `handle_command` toggle arms gets equal coverage
    /// without seven near-identical test bodies.
    fn assert_toggle_roundtrips(
        make_command: impl Fn(bool) -> TrayCommand,
        read_setting: impl Fn(&AppSettings) -> bool,
        read_control_flag: impl Fn(&TrayControl) -> bool,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let store = SettingsStore::new(path.clone());
        let initial = store.load().unwrap();
        let state = Arc::new(Mutex::new(TrayState::from_settings(&initial)));
        let (control_tx, control_rx) = mpsc::channel();

        let target = !read_setting(&initial);
        let should_quit = handle_command(
            make_command(target),
            &store,
            &state,
            &control_tx,
            &noop_show_window,
        )
        .unwrap();
        assert!(!should_quit);

        let fresh_store = SettingsStore::new(path);
        let persisted = fresh_store.load().unwrap();
        assert_eq!(
            read_setting(&persisted),
            target,
            "toggle did not persist to a fresh store instance"
        );

        let control = control_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("handle_command should send a TrayControl::UpdateLabels");
        assert_eq!(
            read_control_flag(&control),
            target,
            "TrayControl::UpdateLabels did not reflect the new value"
        );
    }

    #[test]
    fn toggle_toast_persists_and_updates_control() {
        assert_toggle_roundtrips(
            TrayCommand::ToggleToast,
            |s| s.toast_enabled,
            |c| matches!(c, TrayControl::UpdateLabels { toast_enabled, .. } if *toast_enabled),
        );
    }

    #[test]
    fn toggle_start_minimized_persists_and_updates_control() {
        assert_toggle_roundtrips(
            TrayCommand::ToggleStartMinimizedToTray,
            |s| s.start_minimized_to_tray,
            |c| matches!(c, TrayControl::UpdateLabels { start_minimized, .. } if *start_minimized),
        );
    }

    #[test]
    fn toggle_window_visible_on_start_persists_and_updates_control() {
        assert_toggle_roundtrips(
            TrayCommand::ToggleWindowVisibleOnStart,
            |s| s.window_visible_on_start,
            |c| matches!(c, TrayControl::UpdateLabels { window_visible, .. } if *window_visible),
        );
    }

    #[test]
    fn toggle_notify_approval_persists_and_updates_control() {
        assert_toggle_roundtrips(
            TrayCommand::ToggleApprovalRequired,
            |s| s.show_notifications_for.approval_required,
            |c| matches!(c, TrayControl::UpdateLabels { notify_approval, .. } if *notify_approval),
        );
    }

    #[test]
    fn toggle_notify_submitted_persists_and_updates_control() {
        assert_toggle_roundtrips(
            TrayCommand::ToggleTransactionSubmitted,
            |s| s.show_notifications_for.transaction_submitted,
            |c| matches!(c, TrayControl::UpdateLabels { notify_submitted, .. } if *notify_submitted),
        );
    }

    #[test]
    fn toggle_notify_failed_persists_and_updates_control() {
        assert_toggle_roundtrips(
            TrayCommand::ToggleRunFailed,
            |s| s.show_notifications_for.run_failed,
            |c| matches!(c, TrayControl::UpdateLabels { notify_failed, .. } if *notify_failed),
        );
    }

    #[test]
    fn toggle_notify_completed_persists_and_updates_control() {
        assert_toggle_roundtrips(
            TrayCommand::ToggleRunCompleted,
            |s| s.show_notifications_for.run_completed,
            |c| matches!(c, TrayControl::UpdateLabels { notify_completed, .. } if *notify_completed),
        );
    }

    #[test]
    fn cycle_backend_persists_and_updates_control() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let store = SettingsStore::new(path.clone());
        let initial = store.load().unwrap();
        let state = Arc::new(Mutex::new(TrayState::from_settings(&initial)));
        let (control_tx, control_rx) = mpsc::channel();

        let expected = next_backend(initial.toast_backend_preference);
        let should_quit = handle_command(
            TrayCommand::CycleBackend,
            &store,
            &state,
            &control_tx,
            &noop_show_window,
        )
        .unwrap();
        assert!(!should_quit);

        let fresh_store = SettingsStore::new(path);
        let persisted = fresh_store.load().unwrap();
        assert_eq!(persisted.toast_backend_preference, expected);

        let control = control_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        match control {
            TrayControl::UpdateLabels { backend, .. } => {
                assert!(backend.contains(match expected {
                    NotificationBackend::Auto => "Auto",
                    NotificationBackend::PowerShell => "PowerShell",
                    NotificationBackend::Noop => "Noop",
                }));
            }
            _ => panic!("expected UpdateLabels"),
        }
    }

    #[test]
    fn show_window_invokes_the_injected_closure_and_marks_state_visible() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(dir.path().join("settings.json"));
        let initial = store.load().unwrap();
        let mut state = TrayState::from_settings(&initial);
        state.window_visible = false;
        let state = Arc::new(Mutex::new(state));
        let (control_tx, _control_rx) = mpsc::channel();

        let call_count = Arc::new(Mutex::new(0));
        let call_count_clone = Arc::clone(&call_count);
        let show_window = move || -> Result<(), Box<dyn std::error::Error>> {
            *call_count_clone.lock().unwrap() += 1;
            Ok(())
        };

        let _ = handle_command(
            TrayCommand::ShowWindow,
            &store,
            &state,
            &control_tx,
            &show_window,
        );

        assert_eq!(*call_count.lock().unwrap(), 1);
        assert!(state.lock().unwrap().window_visible);
    }

    #[test]
    fn quit_requests_shutdown_without_persisting_changes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let store = SettingsStore::new(path.clone());
        // Noop backend so Quit's best-effort toast doesn't shell out to
        // PowerShell during a test run.
        let mut initial = store.load().unwrap();
        initial.toast_backend_preference = NotificationBackend::Noop;
        store.save(&initial).unwrap();

        let state = Arc::new(Mutex::new(TrayState::from_settings(&initial)));
        let (control_tx, _control_rx) = mpsc::channel();

        let should_quit = handle_command(
            TrayCommand::Quit,
            &store,
            &state,
            &control_tx,
            &noop_show_window,
        )
        .unwrap();
        assert!(should_quit);

        let fresh = SettingsStore::new(path).load().unwrap();
        assert_eq!(fresh, initial, "Quit must not mutate settings");
    }

    #[test]
    fn backend_cycle_covers_all_known_variants() {
        assert_eq!(
            next_backend(NotificationBackend::Auto),
            NotificationBackend::PowerShell
        );
        assert_eq!(
            next_backend(NotificationBackend::PowerShell),
            NotificationBackend::Noop
        );
        assert_eq!(
            next_backend(NotificationBackend::Noop),
            NotificationBackend::Auto
        );
    }

    #[test]
    fn noop_backend_test_returns_disabled_result() {
        let settings = AppSettings {
            toast_backend_preference: NotificationBackend::Noop,
            ..AppSettings::default()
        };

        let result = run_notification_test(&settings).expect("noop backend should not fail");
        assert_eq!(result.status, NotificationStatus::Disabled);
        assert_eq!(result.message.as_deref(), Some("noop backend selected"));
    }

    #[test]
    fn powershell_backend_test_respects_disabled_setting() {
        let settings = AppSettings {
            toast_enabled: false,
            toast_backend_preference: NotificationBackend::PowerShell,
            ..AppSettings::default()
        };

        let result = run_notification_test(&settings).expect("disabled path should be ok");
        assert_eq!(result.status, NotificationStatus::Disabled);
    }
}
