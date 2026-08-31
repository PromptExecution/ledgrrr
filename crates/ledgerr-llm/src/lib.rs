pub mod error;
pub mod extract;
pub mod provisioning;

pub use error::{LlmError, LlmResult};
pub use extract::{
    DocumentExtraction, ExtractedAmount, ReceiptExtraction, ReceiptLineItem,
    TransactionClassification,
};
pub use provisioning::B00T_SERVER_KEY_ENV;

use std::path::Path;

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use rust_decimal::Decimal;
use serde_json::{json, Value};
use tracing::debug;

use crate::extract::{CLASSIFY_SYSTEM_PROMPT, DOCUMENT_SYSTEM_PROMPT, RECEIPT_SYSTEM_PROMPT};

/// Maximum image bytes to encode; GPT-4o handles ~20 MB but we cap at 10 MB.
const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;

/// Default base URL when `base_url` is unset: `_b00t_`'s local model-serving
/// proxy (`b00t server start` / `b00t-mcp --http --llm`), NOT real OpenAI
/// (l3dg3rr#212). This matches `b00t server start`'s own `--port` default
/// (`b00t-cli/src/commands/server.rs`) — note this differs from the bare
/// `b00t-mcp --http --llm` binary's own clap default of 3000; `b00t server
/// start` is the documented entry point and always passes an explicit
/// `--port 5273` to the child b00t-mcp process, so 5273 is what a real
/// deployment actually listens on.
///
/// Bare host:port, no `/v1` suffix — `LlmClient::new` appends
/// `/v1/chat/completions` itself, matching the previous
/// `https://api.openai.com` convention.
pub const DEFAULT_B00T_SERVER_BASE_URL: &str = "http://127.0.0.1:5273";

/// Shared across this crate's `tests` module AND `provisioning::tests` —
/// both mutate the same process-wide env vars (`OPENAI_API_KEY`,
/// `LEDGERR_B00T_SERVER_KEY`, `LEDGERR_LLM_BASE_URL`). Two separate
/// per-module mutexes would not actually exclude each other since `cargo
/// test` runs a crate's tests in parallel threads by default; a single
/// crate-level mutex is required for real isolation.
#[cfg(test)]
pub(crate) static ENV_TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Configuration for the LLM client.
#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub api_key: String,
    /// Model for vision/chat completions (default: phi-4-mini-reasoning for local-first).
    pub model: String,
    /// Optional base URL override — enables local OpenAI-compatible endpoints
    /// (Ollama, LM Studio, future Gemma4 / Qwen3 adapters, b00t-server).
    /// Defaults to [`DEFAULT_B00T_SERVER_BASE_URL`] when unset.
    pub base_url: Option<String>,
    pub temperature: f32,
}

impl LlmConfig {
    /// Env-only construction. `api_key` comes solely from `OPENAI_API_KEY`
    /// (unset ⇒ empty string) — this never touches the b00t-server key store
    /// or shells out to mint a key. Use [`LlmConfig::provision`] for the full
    /// resolution chain (OPENAI_API_KEY → LEDGERR_B00T_SERVER_KEY → stored
    /// b00t-server key → lazily minted one).
    pub fn from_env() -> Self {
        Self {
            api_key: std::env::var("OPENAI_API_KEY").unwrap_or_default(),
            model: std::env::var("LEDGERR_LLM_MODEL")
                .unwrap_or_else(|_| "phi-4-mini-reasoning".into()),
            base_url: std::env::var("LEDGERR_LLM_BASE_URL").ok(),
            temperature: 0.0,
        }
    }

    pub fn with_key(api_key: impl Into<String>) -> Self {
        let mut c = Self::from_env();
        c.api_key = api_key.into();
        c
    }

    /// The base URL that [`LlmClient::new`] will actually use: the explicit
    /// `base_url` (itself sourced from `LEDGERR_LLM_BASE_URL` via
    /// [`LlmConfig::from_env`]) when set, else [`DEFAULT_B00T_SERVER_BASE_URL`].
    /// Deliberately independent of `api_key` — the base URL a client talks to
    /// and whether it currently holds a valid key are orthogonal.
    pub fn resolved_base_url(&self) -> &str {
        self.base_url.as_deref().unwrap_or(DEFAULT_B00T_SERVER_BASE_URL)
    }
}

/// Blocking HTTP client for OpenAI-compatible vision + chat completion APIs.
pub struct LlmClient {
    config: LlmConfig,
    http: reqwest::blocking::Client,
    chat_url: String,
}

impl LlmClient {
    pub fn new(config: LlmConfig) -> LlmResult<Self> {
        let chat_url = format!("{}/v1/chat/completions", config.resolved_base_url());
        let http = reqwest::blocking::Client::builder()
            .user_agent(concat!("ledgerr/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self {
            config,
            http,
            chat_url,
        })
    }

    // ── Vision ────────────────────────────────────────────────────────────────

    pub fn extract_receipt(&self, image_path: &Path) -> LlmResult<ReceiptExtraction> {
        let (mime, b64) = load_image(image_path)?;
        let content = vision_user_content(&b64, &mime, "Extract receipt data.");
        let raw = self.chat_json(RECEIPT_SYSTEM_PROMPT, content)?;
        parse_extraction(&raw)
    }

    pub fn extract_receipt_bytes(
        &self,
        bytes: &[u8],
        mime_type: &str,
    ) -> LlmResult<ReceiptExtraction> {
        validate_image_size(bytes)?;
        let b64 = B64.encode(bytes);
        let content = vision_user_content(&b64, mime_type, "Extract receipt data.");
        let raw = self.chat_json(RECEIPT_SYSTEM_PROMPT, content)?;
        parse_extraction(&raw)
    }

    pub fn extract_document(&self, image_path: &Path) -> LlmResult<DocumentExtraction> {
        let (mime, b64) = load_image(image_path)?;
        let content = vision_user_content(&b64, &mime, "Extract document metadata.");
        let raw = self.chat_json(DOCUMENT_SYSTEM_PROMPT, content)?;
        parse_extraction(&raw)
    }

    pub fn extract_document_bytes(
        &self,
        bytes: &[u8],
        mime_type: &str,
    ) -> LlmResult<DocumentExtraction> {
        validate_image_size(bytes)?;
        let b64 = B64.encode(bytes);
        let content = vision_user_content(&b64, mime_type, "Extract document metadata.");
        let raw = self.chat_json(DOCUMENT_SYSTEM_PROMPT, content)?;
        parse_extraction(&raw)
    }

    // ── Text classification ───────────────────────────────────────────────────

    pub fn classify_transaction(
        &self,
        description: &str,
        amount: Decimal,
    ) -> LlmResult<TransactionClassification> {
        let user_msg = format!("Transaction: {description}\nAmount: {amount}");
        let content = json!([{"type": "text", "text": user_msg}]);
        let raw = self.chat_json(CLASSIFY_SYSTEM_PROMPT, content)?;
        parse_extraction(&raw)
    }

    // ── Internal ──────────────────────────────────────────────────────────────

    fn chat_json(&self, system: &str, user_content: Value) -> LlmResult<String> {
        let body = json!({
            "model": self.config.model,
            "temperature": self.config.temperature,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user_content}
            ]
        });

        debug!(model = %self.config.model, "LLM completion request");

        let resp = self
            .http
            .post(&self.chat_url)
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()?;

        let status = resp.status().as_u16();
        if status >= 400 {
            let message = resp.text().unwrap_or_default();
            return Err(LlmError::ApiError { status, message });
        }

        let resp_json: Value = resp.json()?;
        let text = resp_json["choices"][0]["message"]["content"]
            .as_str()
            .ok_or(LlmError::EmptyResponse)?
            .to_string();

        Ok(text)
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn load_image(path: &Path) -> LlmResult<(String, String)> {
    let bytes = std::fs::read(path)?;
    validate_image_size(&bytes)?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mime = ext_to_mime(&ext)?;
    Ok((mime.to_string(), B64.encode(&bytes)))
}

fn validate_image_size(bytes: &[u8]) -> LlmResult<()> {
    if bytes.len() > MAX_IMAGE_BYTES {
        Err(LlmError::ImageTooLarge {
            size: bytes.len(),
            max: MAX_IMAGE_BYTES,
        })
    } else {
        Ok(())
    }
}

fn ext_to_mime(ext: &str) -> LlmResult<&'static str> {
    match ext {
        "jpg" | "jpeg" => Ok("image/jpeg"),
        "png" => Ok("image/png"),
        "gif" => Ok("image/gif"),
        "webp" => Ok("image/webp"),
        "tif" | "tiff" => Ok("image/tiff"),
        other => Err(LlmError::UnsupportedMime(other.to_string())),
    }
}

fn vision_user_content(b64: &str, mime: &str, instruction: &str) -> Value {
    json!([
        {
            "type": "image_url",
            "image_url": {
                "url": format!("data:{mime};base64,{b64}"),
                "detail": "high"
            }
        },
        {"type": "text", "text": instruction}
    ])
}

pub fn parse_extraction<T: serde::de::DeserializeOwned>(raw: &str) -> LlmResult<T> {
    let cleaned = raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    serde_json::from_str(cleaned).map_err(|e| LlmError::ParseError(format!("{e}: {cleaned}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_receipt_roundtrip() {
        let json = r##"{
            "vendor_name": "Coffee House",
            "date": "2026-04-18",
            "total_amount": 12.50,
            "currency": "USD",
            "subtotal": 11.36,
            "tax_amount": 1.14,
            "line_items": [{"description": "Latte", "quantity": 1.0, "unit_price": 5.00, "amount": 5.00}],
            "suggested_category": "Meals",
            "suggested_tags": ["#receipt", "#meals"],
            "confidence": 0.95,
            "raw_text": null
        }"##;
        let r: ReceiptExtraction = serde_json::from_str(json).unwrap();
        assert_eq!(r.vendor_name.as_deref(), Some("Coffee House"));
    }

    #[test]
    fn parse_handles_markdown_fence() {
        let raw = "```json\n{\"vendor_name\":null,\"date\":null,\"total_amount\":null,\"currency\":null,\"subtotal\":null,\"tax_amount\":null,\"line_items\":[],\"suggested_category\":null,\"suggested_tags\":[],\"confidence\":0.1,\"raw_text\":null}\n```";
        let r: ReceiptExtraction = parse_extraction(raw).unwrap();
        assert!(r.vendor_name.is_none());
    }

    // ── base_url resolution (l3dg3rr#212) ───────────────────────────────────
    //
    // These tests mutate process env vars, so they share `crate::ENV_TEST_MUTEX`
    // with `provisioning::tests` (matches the ledgerr-cloud::config convention
    // for env-var tests) to avoid interleaving under cargo test's default
    // parallel execution — a mutex local to just this module would NOT
    // exclude provisioning::tests's env var mutations, since they run as
    // separate threads within the same test binary.
    use crate::ENV_TEST_MUTEX;

    fn clear_base_url_env() {
        std::env::remove_var("LEDGERR_LLM_BASE_URL");
    }

    #[test]
    fn default_base_url_is_b00t_server_when_no_env_override_and_no_stored_key() {
        let _lock = ENV_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        clear_base_url_env();
        // "no stored key exists yet" — base_url resolution never touches the
        // settings store at all, so there is nothing to set up here; that
        // independence is exactly what this test (paired with the next one)
        // demonstrates.
        let config = LlmConfig::from_env();
        assert_eq!(config.resolved_base_url(), DEFAULT_B00T_SERVER_BASE_URL);
    }

    #[test]
    fn default_base_url_is_unaffected_by_key_presence() {
        let _lock = ENV_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        clear_base_url_env();
        // Simulate "a key is present" via the explicit escape-hatch env var
        // (provisioning.rs covers the full precedence chain) — base_url
        // resolution must be identical either way.
        std::env::set_var(B00T_SERVER_KEY_ENV, "b00t-sk-test-key");
        let config = LlmConfig::from_env();
        let resolved = config.resolved_base_url().to_string();
        std::env::remove_var(B00T_SERVER_KEY_ENV);
        assert_eq!(resolved, DEFAULT_B00T_SERVER_BASE_URL);
    }

    #[test]
    fn ledgerr_llm_base_url_env_still_overrides_the_default() {
        let _lock = ENV_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("LEDGERR_LLM_BASE_URL", "http://example.test:9999");
        let config = LlmConfig::from_env();
        let resolved = config.resolved_base_url().to_string();
        clear_base_url_env();
        assert_eq!(resolved, "http://example.test:9999");
        assert_ne!(resolved, DEFAULT_B00T_SERVER_BASE_URL);
    }

    #[test]
    fn chat_url_is_built_from_resolved_base_url() {
        let _lock = ENV_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        clear_base_url_env();
        let config = LlmConfig::from_env();
        let client = LlmClient::new(config).expect("client construction never fails on base_url alone");
        assert_eq!(
            client.chat_url,
            format!("{DEFAULT_B00T_SERVER_BASE_URL}/v1/chat/completions")
        );
    }
}
