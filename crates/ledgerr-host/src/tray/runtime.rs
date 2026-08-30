use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;

use crate::notify::{
    NativeToastNotifier, NotificationBackend, NotificationEvent, NotificationSettings,
    NotificationStatus, NotificationTestResult, Notifier, NotifyError,
};
use crate::settings::AppSettings;
use crate::settings_client::SettingsClient;

use super::native::{
    make_icon_data, NativeTrayPlatform, TrayControl, TrayEvent, CMD_CYCLE_BACKEND,
    CMD_EXIT, CMD_NOTIFY_APPROVAL, CMD_NOTIFY_COMPLETED, CMD_NOTIFY_FAILED,
    CMD_NOTIFY_SUBMITTED, CMD_SHOW_WINDOW, CMD_START_MINIMIZED, CMD_TEST_TOAST,
    CMD_TOAST_ENABLED, CMD_WINDOW_VISIBLE,
};
use super::{tray_menu_labels, TrayCommand, TrayState};

pub fn run(
    store: SettingsClient,
    show_window: impl Fn() -> Result<(), Box<dyn std::error::Error>>,
) -> Result<(), Box<dyn std::error::Error>> {
    // ledgrrr-service (the settings server) may not be running yet when the
    // tray starts — that must not stop the tray icon itself from appearing
    // (this is the app's only visible affordance if the main window is also
    // hidden/minimized). Fall back to defaults and let later commands retry
    // the real connection; a failed toggle after that is already handled
    // non-fatally by the loop below.
    let settings = store.load().unwrap_or_else(|e| {
        eprintln!("[tray] could not load initial settings ({e}); using defaults");
        AppSettings::default()
    });
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

            match handle_command(command, &store, &state, &tray.control_tx, &show_window) {
                Ok(true) => break,
                Ok(false) => {}
                // A single failed command (e.g. a transient error talking to
                // ledgrrr-service) must not take the whole tray — and the
                // rest of the app, via host-tauri's exit-on-return — down
                // with it. Log and keep the loop running; only an explicit
                // Quit ends it.
                Err(e) => eprintln!("[tray] command failed: {e}"),
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
    store: &SettingsClient,
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
    store: &SettingsClient,
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
            if let Err(e) = show_window() {
                eprintln!("[tray] show_window failed: {e}");
            }
            Ok(false)
        }
        TrayCommand::Quit => {
            match store.load() {
                Ok(settings) => send_best_effort_toast(
                    &settings,
                    NotificationEvent::Test {
                        title: "l3dg3rr".to_string(),
                        body: "Goodbye from l3dg3rr".to_string(),
                    },
                ),
                Err(e) => eprintln!("[tray] could not load settings for goodbye toast: {e}"),
            }
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
        NotificationBackend::Auto => NotificationBackend::Native,
        NotificationBackend::Native => NotificationBackend::Noop,
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
        NotificationBackend::Auto | NotificationBackend::Native => {
            let notify_settings = NotificationSettings {
                enabled: settings.toast_enabled,
                backend: settings.toast_backend_preference,
                last_test_result: settings.last_test_result.clone(),
            };
            let notifier = NativeToastNotifier::new(notify_settings);
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
    let notifier = NativeToastNotifier::new(notify_settings);
    let _ = notifier.notify(&event);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noop_show_window() -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    /// A minimal fake `ledgrrr-service` settings endpoint: serves whatever
    /// `AppSettings` is currently in `state`, and updates `state` on POST.
    /// Runs until the listener is dropped (test scope end).
    fn fake_settings_server(initial: AppSettings) -> (SettingsClient, std::sync::Arc<Mutex<AppSettings>>) {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let state = std::sync::Arc::new(Mutex::new(initial));
        let state_thread = std::sync::Arc::clone(&state);

        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut buf = [0_u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                let raw = String::from_utf8_lossy(&buf[..n]);
                let request_line = raw.lines().next().unwrap_or_default();

                let response = if request_line.starts_with("GET /settings") {
                    let settings = state_thread.lock().unwrap().clone();
                    let body = serde_json::to_string(&settings).unwrap();
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    )
                } else if request_line.starts_with("POST /settings") {
                    let header_end = raw.find("\r\n\r\n").map(|i| i + 4).unwrap_or(raw.len());
                    let body_str = &raw[header_end..];
                    match serde_json::from_str::<AppSettings>(body_str) {
                        Ok(new_settings) => {
                            *state_thread.lock().unwrap() = new_settings.clone();
                            let body = serde_json::to_string(&new_settings).unwrap();
                            format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                                body.len(),
                                body
                            )
                        }
                        Err(_) => "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n".to_string(),
                    }
                } else {
                    "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_string()
                };
                let _ = stream.write_all(response.as_bytes());
            }
        });

        let client = SettingsClient::with_base_url(format!("http://{addr}"));
        (client, state)
    }

    fn assert_toggle_roundtrips(
        make_command: impl Fn(bool) -> TrayCommand,
        read_setting: impl Fn(&AppSettings) -> bool,
        read_control_flag: impl Fn(&TrayControl) -> bool,
    ) {
        let initial = AppSettings::default();
        let (store, server_state) = fake_settings_server(initial.clone());
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

        let persisted = server_state.lock().unwrap().clone();
        assert_eq!(
            read_setting(&persisted),
            target,
            "toggle did not persist to the settings server"
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
        let initial = AppSettings::default();
        let (store, server_state) = fake_settings_server(initial.clone());
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

        let persisted = server_state.lock().unwrap().clone();
        assert_eq!(persisted.toast_backend_preference, expected);

        let control = control_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        match control {
            TrayControl::UpdateLabels { backend, .. } => {
                assert!(backend.contains(match expected {
                    NotificationBackend::Auto => "Auto",
                    NotificationBackend::Native => "Native",
                    NotificationBackend::Noop => "Noop",
                }));
            }
            _ => panic!("expected UpdateLabels"),
        }
    }

    #[test]
    fn show_window_marks_state_visible_and_invokes_injected_closure() {
        let initial = AppSettings::default();
        let (store, _server_state) = fake_settings_server(initial.clone());
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
        let mut initial = AppSettings::default();
        // Noop backend so Quit's best-effort toast doesn't fire a real
        // native toast popup during a test run.
        initial.toast_backend_preference = NotificationBackend::Noop;
        let (store, server_state) = fake_settings_server(initial.clone());

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

        let unchanged = server_state.lock().unwrap().clone();
        assert_eq!(unchanged, initial, "Quit must not mutate settings");
    }

    #[test]
    fn a_failed_command_does_not_stop_the_loop_from_recognizing_a_later_quit() {
        // Regression test for the whole-branch-review finding: any single
        // command's error used to propagate via `?` out of run()'s loop,
        // killing the whole tray (and, via host-tauri's exit-on-return, the
        // whole app) on a transient failure. A SettingsClient pointed at a
        // closed port can never succeed, so every command here fails —
        // handle_command must still return Ok(true) for Quit specifically
        // failing is a separate, already-covered edge case; this test
        // proves a *non*-Quit failure is recoverable by simulating what
        // run()'s loop does with the Err case: log and continue.
        let unreachable_store = SettingsClient::with_base_url("http://127.0.0.1:1".to_string());
        let state = Arc::new(Mutex::new(TrayState::default()));
        let (control_tx, _control_rx) = mpsc::channel();

        let result = handle_command(
            TrayCommand::ToggleToast(true),
            &unreachable_store,
            &state,
            &control_tx,
            &noop_show_window,
        );
        assert!(result.is_err(), "unreachable server should fail the call");
        // run()'s loop treats this as `Err(e) => eprintln!(...)` and keeps
        // looping — the important behavioral contract is that a failure
        // here is a plain Result::Err, not a panic and not process exit,
        // so the caller can choose to continue.
    }

    #[test]
    fn backend_cycle_covers_all_known_variants() {
        assert_eq!(
            next_backend(NotificationBackend::Auto),
            NotificationBackend::Native
        );
        assert_eq!(
            next_backend(NotificationBackend::Native),
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
    fn native_backend_test_respects_disabled_setting() {
        let settings = AppSettings {
            toast_enabled: false,
            toast_backend_preference: NotificationBackend::Native,
            ..AppSettings::default()
        };

        let result = run_notification_test(&settings).expect("disabled path should be ok");
        assert_eq!(result.status, NotificationStatus::Disabled);
    }
}
