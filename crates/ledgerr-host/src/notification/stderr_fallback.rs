//! Non-Windows fallback notification backend.
//!
//! Prints notification content to stderr in a structured format.
//! Used on platforms (Linux, macOS) where native toast notification APIs
//! are not available through the `windows` crate.

use super::{NotificationError, Notifier};

/// Fallback notifier that writes notification content to stderr.
///
/// This is the last-resort backend on non-Windows targets. It ensures
/// notification content is at least observable in logs even when no
/// native toast system is available.
#[derive(Debug, Clone, Default)]
pub struct StderrNotifier;

impl StderrNotifier {
    /// Create a new stderr-based fallback notifier.
    pub fn new() -> Self {
        Self
    }
}

impl Notifier for StderrNotifier {
    fn send_toast(&self, title: &str, body: &str) -> Result<(), NotificationError> {
        eprintln!("[toast] {title}: {body}");
        Ok(())
    }

    fn send_toast_with_actions(
        &self,
        title: &str,
        body: &str,
        actions: &[(&str, &str)],
    ) -> Result<(), NotificationError> {
        eprintln!("[toast] {title}: {body}");
        for (action, arg) in actions {
            eprintln!("  [{action}] {arg}");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stderr_notifier_send_toast_returns_ok() {
        let notifier = StderrNotifier::new();
        assert!(notifier.send_toast("test", "body").is_ok());
    }

    #[test]
    fn stderr_notifier_send_toast_with_actions_returns_ok() {
        let notifier = StderrNotifier::new();
        assert!(notifier
            .send_toast_with_actions("test", "body", &[("A", "1"), ("B", "2")])
            .is_ok());
    }

    #[test]
    fn stderr_notifier_send_toast_with_empty_actions_returns_ok() {
        let notifier = StderrNotifier::new();
        assert!(notifier
            .send_toast_with_actions("test", "body", &[])
            .is_ok());
    }
}
