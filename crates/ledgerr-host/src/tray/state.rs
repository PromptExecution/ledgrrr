use crate::notify::{NotificationBackend, NotificationStatus, NotificationTestResult};
use crate::settings::{AppSettings, ShowNotificationsFor};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayState {
    pub toast_enabled: bool,
    pub notification_backend: NotificationBackend,
    pub notification_status: NotificationStatus,
    pub last_test_result: Option<NotificationTestResult>,
    pub start_minimized_to_tray: bool,
    pub window_visible_on_start: bool,
    pub show_notifications_for: ShowNotificationsFor,
    pub window_visible: bool,
}

impl Default for TrayState {
    fn default() -> Self {
        Self {
            toast_enabled: true,
            notification_backend: NotificationBackend::Native,
            notification_status: NotificationStatus::Unknown,
            last_test_result: None,
            start_minimized_to_tray: false,
            window_visible_on_start: true,
            show_notifications_for: ShowNotificationsFor::default(),
            window_visible: true,
        }
    }
}

impl TrayState {
    pub fn from_settings(settings: &AppSettings) -> Self {
        let notification_status = if settings.toast_enabled {
            settings
                .last_test_result
                .as_ref()
                .map(|result| result.status)
                .unwrap_or(NotificationStatus::Unknown)
        } else {
            NotificationStatus::Disabled
        };

        Self {
            toast_enabled: settings.toast_enabled,
            notification_backend: settings.toast_backend_preference,
            notification_status,
            last_test_result: settings.last_test_result.clone(),
            start_minimized_to_tray: settings.start_minimized_to_tray,
            window_visible_on_start: settings.window_visible_on_start,
            show_notifications_for: settings.show_notifications_for.clone(),
            window_visible: settings.window_visible_on_start,
        }
    }

    pub fn apply_settings(&mut self, settings: &AppSettings) {
        let window_visible = self.window_visible;
        *self = Self::from_settings(settings);
        self.window_visible = window_visible;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayCommand {
    ToggleToast(bool),
    CycleBackend,
    TestToast,
    ToggleStartMinimizedToTray(bool),
    ToggleWindowVisibleOnStart(bool),
    ToggleApprovalRequired(bool),
    ToggleTransactionSubmitted(bool),
    ToggleRunFailed(bool),
    ToggleRunCompleted(bool),
    ShowWindow,
    Quit,
}
