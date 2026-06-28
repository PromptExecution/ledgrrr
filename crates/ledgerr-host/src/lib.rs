pub mod agent_runtime;
pub mod chat;
pub mod evidence;
pub mod internal_openai;
pub mod notification;
#[cfg(feature = "local-llm")]
pub mod local_llm;
#[cfg(feature = "mistralrs-llm")]
pub mod local_llm_mistral;
pub mod notify;
pub mod settings;
pub mod settings_backend;
pub mod tray;
pub use internal_openai::{
    cloud_chat_settings, local_demo_chat_settings, windows_ai_chat_settings, provider_status,
    resolve_chat_settings, ModelProviderLabel, ProviderInfo, ProviderReadiness,
};
pub use evidence::{EvidenceState, TodayQueue};
