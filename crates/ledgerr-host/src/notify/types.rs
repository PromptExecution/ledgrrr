use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use ledgrrr_settings::{NotificationBackend, NotificationStatus, NotificationTestResult};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NotificationEvent {
    RunStarted,
    ApprovalRequired,
    ToolFailed { tool_name: String, message: String },
    TransactionSubmitted { reference: String },
    RunCompleted,
    Test { title: String, body: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationSettings {
    pub enabled: bool,
    pub backend: NotificationBackend,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_test_result: Option<NotificationTestResult>,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            backend: NotificationBackend::Auto,
            last_test_result: None,
        }
    }
}

#[derive(Debug, Error)]
pub enum NotifyError {
    #[error("notifications are disabled")]
    Disabled,
    #[error("notification command failed: {0}")]
    CommandFailed(String),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}

pub trait Notifier {
    fn is_enabled(&self) -> bool;
    fn status(&self) -> NotificationStatus;
    fn test(&self, title: &str, body: &str) -> Result<NotificationTestResult, NotifyError>;
    fn notify(&self, event: &NotificationEvent) -> Result<(), NotifyError>;
}
