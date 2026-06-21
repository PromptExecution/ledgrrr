//! # ledgerr-model-server
//!
//! Local MCP model server configuration and lifecycle stub.
//!
//! The full inference path (model loading, tokenization, generation) is
//! deferred to a later phase. This crate establishes the config contract,
//! error surface, and startup skeleton so downstream integration points can
//! be wired before the inference engine is selected.
//!
//! ## Design
//!
//! - `ModelServerConfig` is the single source of truth for server parameters;
//!   it is `Serialize`/`Deserialize` for env-based hydration and testing.
//! - `ModelServerMcp` owns a config and exposes `start(&self) -> Result<()>`.
//! - `ModelServerError` is the typed error enum for this crate.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::info;

/// Typed errors for the model server lifecycle.
#[derive(Debug, Error)]
pub enum ModelServerError {
    /// The specified model path does not exist.
    #[error("model path not found: {0}")]
    ModelPathNotFound(PathBuf),

    /// Port number is outside the valid range 1–65535.
    #[error("invalid port: {0}")]
    InvalidPort(u16),

    /// Context window must be at least 512 tokens.
    #[error("context_window too small: {0} (minimum 512)")]
    ContextWindowTooSmall(usize),

    /// An IO error occurred during startup.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Configuration for the local MCP model server.
///
/// Intended to be hydrated from environment variables or a TOML/JSON config
/// file — all fields are `Serialize`/`Deserialize`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelServerConfig {
    /// Bind host (e.g. `"127.0.0.1"` for loopback-only).
    pub host: String,
    /// TCP port to listen on.
    pub port: u16,
    /// Absolute path to the model weights file or directory.
    pub model_path: PathBuf,
    /// Maximum context window in tokens.
    pub context_window: usize,
}

impl ModelServerConfig {
    /// Validate the configuration fields.
    ///
    /// Does not touch the filesystem — path existence is checked in
    /// `ModelServerMcp::start`.
    pub fn validate(&self) -> Result<(), ModelServerError> {
        if self.port == 0 {
            return Err(ModelServerError::InvalidPort(self.port));
        }
        if self.context_window < 512 {
            return Err(ModelServerError::ContextWindowTooSmall(self.context_window));
        }
        Ok(())
    }
}

/// MCP model server stub.
///
/// `start` logs the resolved configuration and returns `Ok(())`. Full
/// inference wiring will replace this in a subsequent phase.
pub struct ModelServerMcp {
    config: ModelServerConfig,
}

impl ModelServerMcp {
    /// Create a new server from a validated config.
    pub fn new(config: ModelServerConfig) -> Self {
        Self { config }
    }

    /// Start the model server.
    ///
    /// Currently: validates config, checks that model_path exists, logs the
    /// resolved parameters, and returns `Ok(())`.
    ///
    /// Future: will bind a Tokio TCP listener and serve MCP requests.
    pub async fn start(&self) -> Result<(), ModelServerError> {
        self.config.validate()?;

        if !self.config.model_path.exists() {
            return Err(ModelServerError::ModelPathNotFound(
                self.config.model_path.clone(),
            ));
        }

        info!(
            host = %self.config.host,
            port = self.config.port,
            model_path = %self.config.model_path.display(),
            context_window = self.config.context_window,
            "model server stub started — inference engine not yet wired"
        );

        Ok(())
    }

    /// Expose the config for inspection (e.g., from health-check endpoints).
    pub fn config(&self) -> &ModelServerConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn valid_config() -> ModelServerConfig {
        ModelServerConfig {
            host: "127.0.0.1".to_string(),
            port: 8080,
            model_path: PathBuf::from("/tmp"),
            context_window: 2048,
        }
    }

    #[test]
    fn valid_config_passes_validation() {
        valid_config().validate().expect("should be valid");
    }

    #[test]
    fn port_zero_is_rejected() {
        let mut cfg = valid_config();
        cfg.port = 0;
        assert!(matches!(
            cfg.validate(),
            Err(ModelServerError::InvalidPort(0))
        ));
    }

    #[test]
    fn small_context_window_is_rejected() {
        let mut cfg = valid_config();
        cfg.context_window = 128;
        assert!(matches!(
            cfg.validate(),
            Err(ModelServerError::ContextWindowTooSmall(128))
        ));
    }

    #[tokio::test]
    async fn start_fails_for_missing_model_path() {
        let cfg = ModelServerConfig {
            host: "127.0.0.1".to_string(),
            port: 9090,
            model_path: PathBuf::from("/nonexistent/path/to/model"),
            context_window: 1024,
        };
        let server = ModelServerMcp::new(cfg);
        let err = server.start().await.unwrap_err();
        assert!(matches!(err, ModelServerError::ModelPathNotFound(_)));
    }
}
