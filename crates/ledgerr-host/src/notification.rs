//! Cross-platform toast notification abstraction.
//!
//! On Windows, uses `windows-rs` WinRT [`ToastNotificationManager`] to send
//! native Windows toast notifications via the action center.
//! On non-Windows targets, falls back to printing notification content to
//! stderr (no platform-native toast support outside Windows).

use thiserror::Error;

/// Trait for sending toast notifications.
///
/// Implementors provide platform-specific rendering of toast content.
/// The trait is kept intentionally simple — title/body for basic toasts,
/// and an extended method for toasts with interactive action buttons.
pub trait Notifier {
    /// Send a simple toast notification with a title and body text.
    fn send_toast(&self, title: &str, body: &str) -> Result<(), NotificationError>;

    /// Send a toast with interactive action buttons.
    ///
    /// Each action is a `(label, arguments)` tuple where `arguments` is
    /// passed back to the app when the button is activated.
    fn send_toast_with_actions(
        &self,
        title: &str,
        body: &str,
        actions: &[(&str, &str)],
    ) -> Result<(), NotificationError>;
}

/// Errors that can occur during notification delivery.
#[derive(Debug, Error)]
pub enum NotificationError {
    /// The notification backend reported a failure.
    #[error("notification failed: {0}")]
    Failed(String),

    /// An I/O error occurred (stderr fallback path).
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}

// Platform-specific backends are conditionally compiled.
// Only one backend is ever active in a given build.

/// Windows toast backend via `windows::UI::Notifications`.
#[cfg(windows)]
pub mod windows_toast;

/// Non-Windows fallback — prints notification content to stderr.
#[cfg(not(windows))]
pub mod stderr_fallback;
