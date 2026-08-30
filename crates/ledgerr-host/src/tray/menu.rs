use crate::notify::{NotificationBackend, NotificationStatus, NotificationTestResult};

use super::state::TrayState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayMenuLabels {
    pub version: String,
    pub backend: String,
    pub cycle_backend: &'static str,
    pub last_test: String,
    pub toast_enabled: &'static str,
    pub start_minimized_to_tray: &'static str,
    pub window_visible_on_start: &'static str,
    pub notify_approval_required: &'static str,
    pub notify_transaction_submitted: &'static str,
    pub notify_run_failed: &'static str,
    pub notify_run_completed: &'static str,
    pub test_toast: &'static str,
    pub status: String,
    pub show_window: &'static str,
    pub exit: &'static str,
}

pub fn tray_menu_labels(state: &TrayState) -> TrayMenuLabels {
    TrayMenuLabels {
        version: format!("Version: {}", env!("CARGO_PKG_VERSION")),
        backend: format!("Backend: {}", backend_label(state.notification_backend)),
        cycle_backend: "Cycle Backend",
        last_test: format!(
            "Last Test: {}",
            last_test_label(state.last_test_result.as_ref())
        ),
        toast_enabled: "Toast Enabled",
        start_minimized_to_tray: "Start Minimized To Tray",
        window_visible_on_start: "Show Window On Start",
        notify_approval_required: "Notify: Approval Required",
        notify_transaction_submitted: "Notify: Transaction Submitted",
        notify_run_failed: "Notify: Run Failed",
        notify_run_completed: "Notify: Run Completed",
        test_toast: "Test Toast",
        status: format!("Status: {}", status_label(state.notification_status)),
        show_window: "Show Window",
        exit: "Exit",
    }
}

fn backend_label(backend: NotificationBackend) -> &'static str {
    match backend {
        NotificationBackend::Auto => "Auto",
        NotificationBackend::Native => "Native",
        NotificationBackend::Noop => "Noop",
    }
}

fn status_label(status: NotificationStatus) -> &'static str {
    match status {
        NotificationStatus::Disabled => "Disabled",
        NotificationStatus::Unknown => "Unknown",
        NotificationStatus::Ready => "Ready",
        NotificationStatus::Degraded => "Degraded",
        NotificationStatus::Failed => "Failed",
    }
}

fn last_test_label(last_test_result: Option<&NotificationTestResult>) -> String {
    match last_test_result {
        Some(result) => {
            let status = status_label(result.status);
            match result.message.as_deref() {
                Some(message) if !message.trim().is_empty() => {
                    format!("{status} ({})", summarize_message(message))
                }
                _ => status.to_string(),
            }
        }
        None => "Never".to_string(),
    }
}

fn summarize_message(message: &str) -> String {
    const MAX_CHARS: usize = 40;

    let trimmed = message.trim();
    if trimmed.chars().count() <= MAX_CHARS {
        return trimmed.to_string();
    }

    let shortened: String = trimmed.chars().take(MAX_CHARS - 1).collect();
    format!("{shortened}…")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::ShowNotificationsFor;

    #[test]
    fn status_label_is_deterministic() {
        let state = TrayState {
            toast_enabled: true,
            notification_backend: NotificationBackend::Native,
            notification_status: NotificationStatus::Ready,
            last_test_result: None,
            start_minimized_to_tray: false,
            window_visible_on_start: true,
            show_notifications_for: ShowNotificationsFor::default(),
            window_visible: false,
        };
        let labels = tray_menu_labels(&state);
        assert_eq!(
            labels.version,
            format!("Version: {}", env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(labels.backend, "Backend: Native");
        assert_eq!(labels.last_test, "Last Test: Never");
        assert_eq!(labels.status, "Status: Ready");
        assert_eq!(labels.toast_enabled, "Toast Enabled");
        assert_eq!(labels.exit, "Exit");
    }

    #[test]
    fn last_test_label_truncates_long_messages() {
        let state = TrayState {
            last_test_result: Some(NotificationTestResult {
                status: NotificationStatus::Failed,
                timestamp: None,
                message: Some(
                    "native toast notification failed because the AUMID was not registered"
                        .to_string(),
                ),
            }),
            ..TrayState::default()
        };
        let labels = tray_menu_labels(&state);
        assert_eq!(
            labels.last_test,
            "Last Test: Failed (native toast notification failed becaus…)"
        );
    }
}
