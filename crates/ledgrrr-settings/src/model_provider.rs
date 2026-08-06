use serde::{Deserialize, Serialize};

/// Operator-facing model provider label.
///
/// This label is shown in the host UI instead of the technical backend name.
/// Each label maps to a readiness state and a setup path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ModelProviderLabel {
    /// Private local inference. Works immediately. May use a deterministic stub if no GGUF is configured.
    LocalDemo,
    /// Private local inference via Windows AI / Foundry Local. Requires setup first.
    WindowsAi,
    /// Explicit external API call. Requires operator-supplied endpoint and key.
    Cloud,
}

impl ModelProviderLabel {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::LocalDemo => "Local Demo",
            Self::WindowsAi => "Windows AI",
            Self::Cloud => "Cloud",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::LocalDemo => "Works immediately. Private. May use a deterministic fallback if no GGUF model is configured.",
            Self::WindowsAi => "Private. Requires Windows AI / Foundry Local setup first.",
            Self::Cloud => "Explicit external call. Requires endpoint and API key.",
        }
    }
}

/// Readiness state for a model provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ProviderReadiness {
    /// Provider can send requests now.
    Ready,
    /// Provider needs one setup step before use.
    SetupNeeded { next_command: String },
    /// Provider cannot be used in the current environment.
    Unavailable { reason: String },
    /// Provider endpoint exists but a smoke test or model load failed.
    Diagnostic { reason: String },
}

impl std::fmt::Display for ProviderReadiness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ready => write!(f, "Ready"),
            Self::SetupNeeded { next_command } => write!(f, "Setup Needed — run: {next_command}"),
            Self::Unavailable { reason } => write!(f, "Unavailable — {reason}"),
            Self::Diagnostic { reason } => write!(f, "Diagnostic — {reason}"),
        }
    }
}
