mod native;
mod types;

pub use native::NativeToastNotifier;
pub use types::{
    NotificationBackend, NotificationEvent, NotificationSettings, NotificationStatus,
    NotificationTestResult, Notifier, NotifyError,
};
