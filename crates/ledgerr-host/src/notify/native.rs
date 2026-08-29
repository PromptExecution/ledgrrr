//! Native toast delivery for the `notify` module's [`Notifier`] trait.
//!
//! Replaces the old `powershell.exe` + BurntToast module dependency with
//! the already-native `crate::notification` backends: `windows_toast`
//! (real `windows::UI::Notifications` toasts) on Windows, `stderr_fallback`
//! on other platforms. No external process, no PowerShell module install
//! required on the target machine.

use chrono::Utc;

use super::types::{
    NotificationEvent, NotificationSettings, NotificationStatus, NotificationTestResult, Notifier,
    NotifyError,
};
use crate::notification::Notifier as PlatformNotifier;

#[cfg(windows)]
type PlatformToastNotifier = crate::notification::windows_toast::ToastNotifier;
#[cfg(not(windows))]
type PlatformToastNotifier = crate::notification::stderr_fallback::StderrNotifier;

#[derive(Debug, Clone)]
pub struct NativeToastNotifier {
    settings: NotificationSettings,
    platform: PlatformToastNotifier,
}

impl NativeToastNotifier {
    pub fn new(settings: NotificationSettings) -> Self {
        Self {
            settings,
            platform: PlatformToastNotifier::default(),
        }
    }

    fn disabled_result() -> NotificationTestResult {
        NotificationTestResult {
            status: NotificationStatus::Disabled,
            timestamp: Some(Utc::now()),
            message: Some("notifications disabled".to_string()),
        }
    }

    fn event_title_body(event: &NotificationEvent) -> (&str, String) {
        match event {
            NotificationEvent::RunStarted => ("l3dg3rr", "Run started".to_string()),
            NotificationEvent::ApprovalRequired => ("l3dg3rr", "Approval required".to_string()),
            NotificationEvent::ToolFailed { tool_name, message } => {
                ("l3dg3rr", format!("Tool failed: {tool_name}: {message}"))
            }
            NotificationEvent::TransactionSubmitted { reference } => {
                ("l3dg3rr", format!("Transaction submitted: {reference}"))
            }
            NotificationEvent::RunCompleted => ("l3dg3rr", "Run completed".to_string()),
            NotificationEvent::Test { title, body } => (title.as_str(), body.clone()),
        }
    }
}

impl From<crate::notification::NotificationError> for NotifyError {
    fn from(err: crate::notification::NotificationError) -> Self {
        match err {
            crate::notification::NotificationError::Failed(msg) => NotifyError::CommandFailed(msg),
            crate::notification::NotificationError::Io(io_err) => NotifyError::Io(io_err),
        }
    }
}

impl Notifier for NativeToastNotifier {
    fn is_enabled(&self) -> bool {
        self.settings.enabled
    }

    fn status(&self) -> NotificationStatus {
        if self.settings.enabled {
            NotificationStatus::Unknown
        } else {
            NotificationStatus::Disabled
        }
    }

    fn test(&self, title: &str, body: &str) -> Result<NotificationTestResult, NotifyError> {
        if !self.settings.enabled {
            return Ok(Self::disabled_result());
        }

        self.platform.send_toast(title, body)?;

        Ok(NotificationTestResult {
            status: NotificationStatus::Ready,
            timestamp: Some(Utc::now()),
            message: Some("toast sent".to_string()),
        })
    }

    fn notify(&self, event: &NotificationEvent) -> Result<(), NotifyError> {
        if !self.settings.enabled {
            return Err(NotifyError::Disabled);
        }

        let (title, body) = Self::event_title_body(event);
        self.platform.send_toast(title, &body)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notify::NotificationBackend;

    #[test]
    fn disabled_path_returns_disabled_status() {
        let notifier = NativeToastNotifier::new(NotificationSettings {
            enabled: false,
            backend: NotificationBackend::Native,
            last_test_result: None,
        });
        let result = notifier.test("x", "y").unwrap();
        assert_eq!(result.status, NotificationStatus::Disabled);
    }

    #[test]
    fn disabled_notify_returns_disabled_error() {
        let notifier = NativeToastNotifier::new(NotificationSettings {
            enabled: false,
            backend: NotificationBackend::Native,
            last_test_result: None,
        });
        let err = notifier.notify(&NotificationEvent::RunStarted).unwrap_err();
        assert!(matches!(err, NotifyError::Disabled));
    }

    #[test]
    fn event_title_body_uses_explicit_payload_for_test_event() {
        let event = NotificationEvent::Test {
            title: "a".into(),
            body: "b".into(),
        };
        let (title, body) = NativeToastNotifier::event_title_body(&event);
        assert_eq!(title, "a");
        assert_eq!(body, "b");
    }

    #[test]
    fn event_title_body_formats_tool_failed() {
        let event = NotificationEvent::ToolFailed {
            tool_name: "build".into(),
            message: "oops".into(),
        };
        let (title, body) = NativeToastNotifier::event_title_body(&event);
        assert_eq!(title, "l3dg3rr");
        assert_eq!(body, "Tool failed: build: oops");
    }
}
