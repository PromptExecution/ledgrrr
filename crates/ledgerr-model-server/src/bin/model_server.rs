//! `model-server` binary entry point.
//!
//! Reads `ModelServerConfig` from environment variables and calls
//! `ModelServerMcp::start()`.
//!
//! ## Environment Variables
//!
//! | Variable              | Default         | Description                          |
//! |-----------------------|-----------------|--------------------------------------|
//! | `MODEL_SERVER_HOST`   | `127.0.0.1`     | Bind host                            |
//! | `MODEL_SERVER_PORT`   | `8765`          | TCP port                             |
//! | `MODEL_SERVER_PATH`   | *(required)*    | Absolute path to model weights       |
//! | `MODEL_SERVER_CTX`    | `4096`          | Context window in tokens             |

use std::path::PathBuf;

use anyhow::{Context, Result};
use ledgerr_model_server::{ModelServerConfig, ModelServerMcp};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = config_from_env().context("failed to build ModelServerConfig from environment")?;
    let server = ModelServerMcp::new(config);
    server
        .start()
        .await
        .context("ModelServerMcp::start failed")?;
    Ok(())
}

/// Build `ModelServerConfig` from environment variables.
fn config_from_env() -> Result<ModelServerConfig> {
    let host = std::env::var("MODEL_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());

    let port: u16 = std::env::var("MODEL_SERVER_PORT")
        .unwrap_or_else(|_| "8765".to_string())
        .parse()
        .context("MODEL_SERVER_PORT must be a valid u16")?;

    let model_path = std::env::var("MODEL_SERVER_PATH")
        .context("MODEL_SERVER_PATH is required")?;

    let context_window: usize = std::env::var("MODEL_SERVER_CTX")
        .unwrap_or_else(|_| "4096".to_string())
        .parse()
        .context("MODEL_SERVER_CTX must be a valid usize")?;

    Ok(ModelServerConfig {
        host,
        port,
        model_path: PathBuf::from(model_path),
        context_window,
    })
}
