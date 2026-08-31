use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationBackend {
    Auto,
    /// Native Windows toast via `windows::UI::Notifications` (or the
    /// stderr fallback on non-Windows). `#[serde(alias)]` keeps reading
    /// settings persisted before this was renamed from `PowerShell` —
    /// this backend no longer shells out to `powershell.exe` at all.
    #[serde(alias = "powershell")]
    Native,
    Noop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationStatus {
    Disabled,
    Unknown,
    Ready,
    Degraded,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationTestResult {
    pub status: NotificationStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}
