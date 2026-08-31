use serde::{Deserialize, Serialize};

use crate::model_provider::ModelProviderLabel;
use crate::notification::{NotificationBackend, NotificationTestResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingsSchemaVersion {
    V1,
    #[default]
    V2,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShowNotificationsFor {
    pub approval_required: bool,
    pub transaction_submitted: bool,
    pub run_failed: bool,
    pub run_completed: bool,
}

impl Default for ShowNotificationsFor {
    fn default() -> Self {
        Self {
            approval_required: true,
            transaction_submitted: true,
            run_failed: true,
            run_completed: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatSettings {
    #[serde(default = "default_chat_endpoint")]
    pub endpoint_url: String,
    #[serde(default = "default_chat_api_key")]
    pub api_key: String,
    #[serde(default = "default_chat_model")]
    pub model: String,
    #[serde(default = "default_chat_system_prompt")]
    pub system_prompt: String,
}

impl Default for ChatSettings {
    fn default() -> Self {
        Self {
            endpoint_url: default_chat_endpoint(),
            api_key: default_chat_api_key(),
            model: default_chat_model(),
            system_prompt: default_chat_system_prompt(),
        }
    }
}

fn default_chat_endpoint() -> String {
    "http://127.0.0.1:15115/v1/chat/completions".to_string()
}

fn default_chat_api_key() -> String {
    "local-tool-tray".to_string()
}

fn default_chat_model() -> String {
    "phi-4-mini-reasoning".to_string()
}

fn default_chat_system_prompt() -> String {
    "You are a concise assistant inside the l3dg3rr operator tray.".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default)]
    pub schema_version: SettingsSchemaVersion,
    #[serde(default = "default_model_provider")]
    pub model_provider: ModelProviderLabel,
    pub toast_enabled: bool,
    pub toast_backend_preference: NotificationBackend,
    pub start_minimized_to_tray: bool,
    pub window_visible_on_start: bool,
    #[serde(default)]
    pub show_notifications_for: ShowNotificationsFor,
    #[serde(default)]
    pub chat: ChatSettings,
    #[serde(default = "default_enable_tray")]
    pub enable_tray: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_test_result: Option<NotificationTestResult>,
    /// b00t-server API key (`b00t-sk-...`) for `ledgerr-llm`'s document/receipt
    /// extraction client. Minted lazily by `ledgerr-llm` via `b00t server key
    /// create` on first use and persisted here so it survives process restarts
    /// (l3dg3rr#212). `None` until a key has been minted or the operator sets
    /// one manually.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub b00t_server_api_key: Option<String>,
}

fn default_enable_tray() -> bool {
    true
}

fn default_model_provider() -> ModelProviderLabel {
    ModelProviderLabel::LocalDemo
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            schema_version: SettingsSchemaVersion::V2,
            model_provider: default_model_provider(),
            toast_enabled: true,
            toast_backend_preference: NotificationBackend::Native,
            start_minimized_to_tray: false,
            window_visible_on_start: true,
            show_notifications_for: ShowNotificationsFor::default(),
            chat: ChatSettings::default(),
            enable_tray: true,
            last_test_result: None,
            b00t_server_api_key: None,
        }
    }
}
