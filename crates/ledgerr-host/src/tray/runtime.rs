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

pub fn run(store: SettingsStore) -> Result<(), Box<dyn std::error::Error>> {
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
                        let enabled = !state
                            .lock()
                            .expect("tray state poisoned")
                            .toast_enabled;
                        TrayCommand::ToggleToast(enabled)
                    }
                    CMD_CYCLE_BACKEND => TrayCommand::CycleBackend,
                    CMD_TEST_TOAST => TrayCommand::TestToast,
                    CMD_START_MINIMIZED => {
                        let enabled = !state
                            .lock()
                            .expect("tray state poisoned")
                            .start_minimized_to_tray;
                        TrayCommand::ToggleStartMinimizedToTray(enabled)
                    }
                    CMD_WINDOW_VISIBLE => {
                        let enabled = !state
                            .lock()
                            .expect("tray state poisoned")
                            .window_visible_on_start;
                        TrayCommand::ToggleWindowVisibleOnStart(enabled)
                    }
                    CMD_NOTIFY_APPROVAL => {
                        let enabled = !state
                            .lock()
                            .expect("tray state poisoned")
                            .show_notifications_for
                            .approval_required;
                        TrayCommand::ToggleApprovalRequired(enabled)
                    }
                    CMD_NOTIFY_SUBMITTED => {
                        let enabled = !state
                            .lock()
                            .expect("tray state poisoned")
                            .show_notifications_for
                            .transaction_submitted;
                        TrayCommand::ToggleTransactionSubmitted(enabled)
                    }
                    CMD_NOTIFY_FAILED => {
                        let enabled = !state
                            .lock()
                            .expect("tray state poisoned")
                            .show_notifications_for
                            .run_failed;
                        TrayCommand::ToggleRunFailed(enabled)
                    }
                    CMD_NOTIFY_COMPLETED => {
                        let enabled = !state
                            .lock()
                            .expect("tray state poisoned")
                            .show_notifications_for
                            .run_completed;
                        TrayCommand::ToggleRunCompleted(enabled)
                    }
                    CMD_SHOW_WINDOW => TrayCommand::ShowWindow,
                    CMD_EXIT => TrayCommand::Quit,
                    _ => continue,
                },
            };

            let should_quit = handle_command(command, &store, &state, &tray.control_tx)?;
            if should_quit {
                break;
            }
        }
    }

    tray.shutdown();
    Ok(())
}

fn handle_command(
    command: TrayCommand,
    store: &SettingsStore,
    state: &Arc<Mutex<TrayState>>,
    control_tx: &mpsc::Sender<TrayControl>,
) -> Result<bool, Box<dyn std::error::Error>> {
    match command {
        TrayCommand::ToggleToast(enabled) => {
            let mut settings = store.load()?;
            settings.toast_enabled = enabled;
            store.save(&settings)?;

            sync_state(state, &settings, control_tx);
            Ok(false)
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
        TrayCommand::ToggleStartMinimizedToTray(enabled) => {
            let mut settings = store.load()?;
            settings.start_minimized_to_tray = enabled;
            store.save(&settings)?;

            sync_state(state, &settings, control_tx);
            Ok(false)
        }
        TrayCommand::ToggleWindowVisibleOnStart(enabled) => {
            let mut settings = store.load()?;
            settings.window_visible_on_start = enabled;
            store.save(&settings)?;

            sync_state(state, &settings, control_tx);
            Ok(false)
        }
        TrayCommand::ToggleApprovalRequired(enabled) => {
            let mut settings = store.load()?;
            settings.show_notifications_for.approval_required = enabled;
            store.save(&settings)?;

            sync_state(state, &settings, control_tx);
            Ok(false)
        }
        TrayCommand::ToggleTransactionSubmitted(enabled) => {
            let mut settings = store.load()?;
            settings.show_notifications_for.transaction_submitted = enabled;
            store.save(&settings)?;

            sync_state(state, &settings, control_tx);
            Ok(false)
        }
        TrayCommand::ToggleRunFailed(enabled) => {
            let mut settings = store.load()?;
            settings.show_notifications_for.run_failed = enabled;
            store.save(&settings)?;

            sync_state(state, &settings, control_tx);
            Ok(false)
        }
        TrayCommand::ToggleRunCompleted(enabled) => {
            let mut settings = store.load()?;
            settings.show_notifications_for.run_completed = enabled;
            store.save(&settings)?;

            sync_state(state, &settings, control_tx);
            Ok(false)
        }
        TrayCommand::ShowWindow => {
            if let Ok(mut state) = state.lock() {
                state.window_visible = true;
            }
            show_window_process()?;
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

fn show_window_process() -> Result<(), Box<dyn std::error::Error>> {
    let current_exe = std::env::current_exe()?;
    let host_window = current_exe.with_file_name("host-window.exe");
    std::process::Command::new(host_window).spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
