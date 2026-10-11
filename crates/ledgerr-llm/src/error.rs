use thiserror::Error;

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("API error {status}: {message}")]
    ApiError { status: u16, message: String },

    #[error("LLM returned no content")]
    EmptyResponse,

    #[error("Failed to parse extraction output: {0}")]
    ParseError(String),

    #[error("Image too large: {size} bytes (max {max})")]
    ImageTooLarge { size: usize, max: usize },

    #[error("Unsupported MIME type: {0}")]
    UnsupportedMime(String),

    /// b00t-server API key resolution/minting failed (l3dg3rr#212). Never
    /// silently swallowed into an empty api_key — callers must surface this.
    #[error("b00t-server key provisioning failed: {0}")]
    KeyProvisioning(String),
}

pub type LlmResult<T> = Result<T, LlmError>;
