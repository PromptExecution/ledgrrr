//! Spike: a minimal Rust client for `PromptExecution/reqif-opa-mcp`'s MCP
//! Streamable-HTTP server, plus a converter from its `RequirementRecord`
//! shape into `arc_kit_au::Requirement`.
//!
//! Decision 6 (`docs/systems-modeling-registry-rescope.md` §5): reqif-opa-mcp
//! is wrapped over MCP, not ported to Rust — `arc-kit-au` stays the canonical
//! decision+cost ledger, this crate only supplies parsed requirements into it.
//!
//! Protocol notes (reverse-engineered against a live server, FastMCP 3.0.0b1,
//! protocol version 2024-11-05, 2026-08-22):
//! - Every request is `POST {base_url}/mcp` with
//!   `Accept: application/json, text/event-stream`.
//! - The response is a single SSE frame (`event: message\ndata: {json}\n\n`),
//!   not a long-lived stream — read the whole body and extract the `data:`
//!   line.
//! - `initialize` returns an `mcp-session-id` response header that must be
//!   echoed on every subsequent request.
//! - A `notifications/initialized` notification (no `id`, no response body)
//!   must be sent once after `initialize` before any `tools/*` call.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use arc_kit_au::node::Requirement;

#[derive(Debug, thiserror::Error)]
pub enum McpClientError {
    #[error("HTTP transport error: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("server response had no SSE `data:` line: {0}")]
    NoDataLine(String),
    #[error("malformed JSON-RPC response: {0}")]
    MalformedJson(#[from] serde_json::Error),
    #[error("server did not return an mcp-session-id header on initialize")]
    NoSessionId,
    #[error("JSON-RPC error response: {0}")]
    RpcError(String),
    #[error("tool call reported isError=true: {0}")]
    ToolError(String),
}

/// Blocking client for one MCP Streamable-HTTP server instance.
pub struct McpHttpClient {
    http: reqwest::blocking::Client,
    base_url: String,
    session_id: Option<String>,
    next_id: u64,
}

impl McpHttpClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            http: reqwest::blocking::Client::new(),
            base_url: base_url.into(),
            session_id: None,
            next_id: 1,
        }
    }

    fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Perform the MCP `initialize` handshake and send the mandatory
    /// `notifications/initialized` follow-up.
    pub fn initialize(&mut self, client_name: &str, client_version: &str) -> Result<Value, McpClientError> {
        let id = self.alloc_id();
        let body = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": client_name, "version": client_version }
            }
        });

        let resp = self
            .http
            .post(format!("{}/mcp", self.base_url))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .json(&body)
            .send()?;

        let session_id = resp
            .headers()
            .get("mcp-session-id")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
            .ok_or(McpClientError::NoSessionId)?;
        self.session_id = Some(session_id);

        let text = resp.text()?;
        let result = extract_rpc_result(&text)?;

        // Mandatory notification, no response body expected.
        let notif = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        self.http
            .post(format!("{}/mcp", self.base_url))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("mcp-session-id", self.session_id.as_ref().unwrap())
            .json(&notif)
            .send()?;

        Ok(result)
    }

    /// Call one MCP tool by name, returning its parsed `structuredContent`
    /// (falling back to parsing the first text content block as JSON).
    pub fn call_tool(&mut self, name: &str, arguments: Value) -> Result<Value, McpClientError> {
        let id = self.alloc_id();
        let body = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": { "name": name, "arguments": arguments }
        });

        let mut req = self
            .http
            .post(format!("{}/mcp", self.base_url))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .json(&body);
        if let Some(sid) = &self.session_id {
            req = req.header("mcp-session-id", sid);
        }

        let text = req.send()?.text()?;
        let result = extract_rpc_result(&text)?;

        if result.get("isError").and_then(Value::as_bool) == Some(true) {
            return Err(McpClientError::ToolError(result.to_string()));
        }

        if let Some(structured) = result.get("structuredContent") {
            return Ok(structured.clone());
        }
        // Fall back to the first text content block (export_req_set returns
        // its payload as a JSON string inside `content`, not `structuredContent`).
        if let Some(text_block) = result
            .get("content")
            .and_then(Value::as_array)
            .and_then(|blocks| blocks.first())
            .and_then(|b| b.get("text"))
            .and_then(Value::as_str)
        {
            return Ok(serde_json::from_str(text_block)?);
        }
        Ok(result)
    }
}

/// Extract the JSON-RPC `result` object from a single SSE frame response body.
fn extract_rpc_result(sse_body: &str) -> Result<Value, McpClientError> {
    let data_line = sse_body
        .lines()
        .find_map(|line| line.strip_prefix("data: "))
        .ok_or_else(|| McpClientError::NoDataLine(sse_body.to_string()))?;

    let envelope: Value = serde_json::from_str(data_line)?;
    if let Some(err) = envelope.get("error") {
        return Err(McpClientError::RpcError(err.to_string()));
    }
    envelope
        .get("result")
        .cloned()
        .ok_or_else(|| McpClientError::NoDataLine(sse_body.to_string()))
}

/// Mirrors `schemas/requirement-record.schema.json` in reqif-opa-mcp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequirementRecord {
    pub uid: String,
    pub key: String,
    pub subtypes: Vec<String>,
    pub status: String,
    pub policy_baseline: PolicyBaselineRef,
    #[serde(default)]
    pub rubrics: Vec<Rubric>,
    pub text: String,
    #[serde(default)]
    pub attrs: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyBaselineRef {
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rubric {
    pub engine: String,
    pub bundle: String,
    pub package: String,
    pub rule: String,
}

/// Convert one reqif-opa-mcp `RequirementRecord` into an `arc-kit-au`
/// `Requirement` node (the `ArtifactKind::Requirement` / `NodeType::Requirement`
/// widening from ledgrrr#184).
///
/// Field mapping (no direct 1:1 — reqif-opa-mcp's schema predates and is
/// independent of arc-kit-au's node model):
/// - `requirement_id` <- `uid` (globally unique across baselines; `key` is
///   only unique within one standard, e.g. "PO-3-1")
/// - `title` <- `key` (the human-readable requirement key)
/// - `rationale` <- `text` (the full requirement statement doubles as its
///   rationale — reqif-opa-mcp has no separate rationale field)
/// - `source` <- `attrs.source_standard` + `attrs.source_url` if present,
///   else falls back to the policy baseline id
/// - `status` <- `status` (already one of active/obsolete/draft, a superset-
///   compatible vocabulary with arc-kit-au's free-form `String`)
/// - `related_decisions` <- always empty; reqif-opa-mcp carries no decision
///   links, those are created later in arc-kit-au itself
/// - `imported_at` <- caller-supplied (usually `Utc::now()` at conversion time)
pub fn requirement_record_to_node(rec: &RequirementRecord, imported_at: DateTime<Utc>) -> Requirement {
    let source = match (rec.attrs.get("source_standard"), rec.attrs.get("source_url")) {
        (Some(std), Some(url)) => Some(format!(
            "{} ({})",
            std.as_str().unwrap_or_default(),
            url.as_str().unwrap_or_default()
        )),
        (Some(std), None) => Some(std.as_str().unwrap_or_default().to_string()),
        _ => Some(rec.policy_baseline.id.clone()),
    };

    Requirement {
        requirement_id: rec.uid.clone(),
        title: rec.key.clone(),
        rationale: Some(rec.text.clone()),
        source,
        status: rec.status.clone(),
        related_decisions: Vec::new(),
        imported_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_record() -> RequirementRecord {
        let mut attrs = HashMap::new();
        attrs.insert("severity".to_string(), json!("high"));
        attrs.insert("source_standard".to_string(), json!("NIST SSDF 1.1 (SP 800-218)"));
        attrs.insert("source_url".to_string(), json!("https://doi.org/10.6028/NIST.SP.800-218"));

        RequirementRecord {
            uid: "REQ-NIST-SSDF-002".to_string(),
            key: "PW-7-2".to_string(),
            subtypes: vec!["SECURE_SDLC".to_string()],
            status: "active".to_string(),
            policy_baseline: PolicyBaselineRef {
                id: "nist-ssdf".to_string(),
                version: "2026.01".to_string(),
                hash: "e86ec0846e64074e".to_string(),
            },
            rubrics: vec![Rubric {
                engine: "opa".to_string(),
                bundle: "org/compliance".to_string(),
                package: "compliance.secure.sdlc".to_string(),
                rule: "decision".to_string(),
            }],
            text: "Perform the code review and/or code analysis...".to_string(),
            attrs,
        }
    }

    #[test]
    fn converts_requirement_record_field_shape() {
        let rec = sample_record();
        let now = Utc::now();
        let node = requirement_record_to_node(&rec, now);

        assert_eq!(node.requirement_id, "REQ-NIST-SSDF-002");
        assert_eq!(node.title, "PW-7-2");
        assert_eq!(node.rationale.as_deref(), Some(rec.text.as_str()));
        assert_eq!(
            node.source.as_deref(),
            Some("NIST SSDF 1.1 (SP 800-218) (https://doi.org/10.6028/NIST.SP.800-218)")
        );
        assert_eq!(node.status, "active");
        assert!(node.related_decisions.is_empty());
        assert_eq!(node.imported_at, now);
    }

    #[test]
    fn falls_back_to_policy_baseline_id_when_no_source_attrs() {
        let mut rec = sample_record();
        rec.attrs.clear();
        let node = requirement_record_to_node(&rec, Utc::now());
        assert_eq!(node.source.as_deref(), Some("nist-ssdf"));
    }

    #[test]
    fn node_id_is_deterministic_for_converted_requirement() {
        let rec = sample_record();
        let now = Utc::now();
        let a = requirement_record_to_node(&rec, now);
        let b = requirement_record_to_node(&rec, now);
        assert_eq!(a.node_id(), b.node_id());
    }

    #[test]
    fn extract_rpc_result_parses_single_sse_frame() {
        let sse = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"ok\":true}}\n\n";
        let result = extract_rpc_result(sse).unwrap();
        assert_eq!(result["ok"], json!(true));
    }

    #[test]
    fn extract_rpc_result_surfaces_json_rpc_errors() {
        let sse = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"error\":{\"message\":\"boom\"}}\n\n";
        let err = extract_rpc_result(sse).unwrap_err();
        assert!(matches!(err, McpClientError::RpcError(_)));
    }
}
