pub mod error;
pub mod extract;

pub use error::{LlmError, LlmResult};
pub use extract::{
    DocumentExtraction, ExtractedAmount, ReceiptExtraction, ReceiptLineItem,
    TransactionClassification,
};

use std::path::Path;

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use rust_decimal::Decimal;
use serde_json::{json, Value};
use tracing::debug;

use crate::extract::{CLASSIFY_SYSTEM_PROMPT, DOCUMENT_SYSTEM_PROMPT, RECEIPT_SYSTEM_PROMPT};

/// Maximum image bytes to encode; GPT-4o handles ~20 MB but we cap at 10 MB.
const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;

/// Configuration for the LLM client.
#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub api_key: String,
    /// Model for vision/chat completions (default: phi-4-mini-reasoning for local-first).
    pub model: String,
    /// Optional base URL override — enables local OpenAI-compatible endpoints
    /// (Ollama, LM Studio, future Gemma4 / Qwen3 adapters).
    pub base_url: Option<String>,
    pub temperature: f32,
}

impl LlmConfig {
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
}

/// Blocking HTTP client for OpenAI-compatible vision + chat completion APIs.
pub struct LlmClient {
    config: LlmConfig,
    http: reqwest::blocking::Client,
    chat_url: String,
}

impl LlmClient {
    pub fn new(config: LlmConfig) -> LlmResult<Self> {
        let base = config
            .base_url
            .as_deref()
            .unwrap_or("https://api.openai.com");
        let chat_url = format!("{base}/v1/chat/completions");
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
}
