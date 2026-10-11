//! # Agent runtime — the AI interface contract
//!
//! **Who this module is for.**  This file defines the boundary between the rest of
//! the application and *any* AI model, whether that model runs locally on the
//! workstation or calls a remote API.  It is designed to be auditable by governance
//! committees who need to understand what information the AI is given, what it can
//! return, and how the system is wired together.
//!
//! ---
//!
//! ## What is an "agent runtime"?
//!
//! An *agent* in this system is a piece of software that uses an LLM to make
//! decisions — classifying a transaction, detecting anomalies, generating a
//! reconciliation rationale.  The word "runtime" refers to the execution environment
//! that actually runs the model: it might be a local GGUF file on disk, or an
//! OpenAI-compatible HTTP endpoint, or a future cloud provider.
//!
//! The `AgentRuntime` trait in this module is a **contract**: any concrete model
//! backend must implement it.  This means:
//!
//! * The pipeline code that calls the AI never knows *which* backend is in use.
//! * Switching from a local model to a cloud API (or vice versa) requires changing
//!   one configuration line — not rewriting the classification logic.
//! * Tests can substitute a deterministic fake runtime that always returns fixed
//!   responses, making financial-logic tests reproducible without a running model.
//!
//! ---
//!
//! ## The conversation model
//!
//! Modern LLMs are driven through a structured conversation made up of *turns*, each
//! tagged with a *role*:
//!
//! * **System** — background instructions set by the application (e.g. "you are a tax
//!   classification assistant; respond only in JSON").  The user never writes this.
//! * **User** — the current question or task (e.g. a transaction line to classify).
//! * **Assistant** — the model's previous replies, included when the application needs
//!   the model to reason across multiple steps.
//!
//! A `ModelRequest` bundles these three components into one struct.  The backend turns
//! them into whatever format the underlying model accepts (a chat-completion JSON body,
//! a GGUF prompt template, etc.).
//!
//! ---
//!
//! ## Audit trail
//!
//! Every model call can be observed via the `ModelAuditSink` trait.  The sink receives
//! a `ModelCallEvent` that records the provider, model name, endpoint URL, prompt
//! character count, history depth, token limit, and whether the call succeeded or
//! failed — without recording the actual prompt content (which may contain financial
//! data).  This gives auditors a complete call log while keeping the data layer clean.
//!
//! ---
//!
//! ## Backends
//!
//! | Backend | Where defined | When to use |
//! |---|---|---|
//! | `RigAgentRuntime` | This file | OpenAI-compatible HTTP endpoint (local or cloud) |
//! | `LocalCandelRuntime` | `local_llm.rs` (feature `local-llm`) | Smoke tests, correctness validation, pure-Rust CI |
//! | `LocalMistralRuntime` | `local_llm_mistral.rs` (feature `mistralrs-llm`) | Fast local inference, development, interactive use |

use std::sync::{Arc, Mutex};

use rig::{
    client::CompletionClient,
    completion::{AssistantContent, CompletionModel, Message, ToolDefinition},
    message::{ToolCall as RigToolCall, ToolFunction as RigToolFunction},
    providers::openai,
    OneOrMany,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;

use crate::settings::ChatSettings;

/// One turn in a multi-turn conversation, tagged with who spoke.
///
/// Turns are included in `ModelRequest::history` to give the model context from
/// earlier in a multi-step reasoning session.  For single-shot classification tasks
/// the history is typically empty.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModelTurn {
    pub role: ModelRole,
    pub content: String,
    /// Populated only on `Assistant` turns that requested tool calls.
    pub tool_calls: Vec<ModelToolCall>,
    /// Populated only on `Tool` turns — the id of the call this responds to.
    pub tool_call_id: Option<String>,
}

impl ModelTurn {
    /// An assistant turn that requested one or more tool calls (no text).
    pub fn assistant_tool_calls(tool_calls: Vec<ModelToolCall>) -> Self {
        Self {
            role: ModelRole::Assistant,
            tool_calls,
            ..Default::default()
        }
    }

    /// A tool-role turn reporting `content` back as the result of `call_id`.
    pub fn tool_result(call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: ModelRole::Tool,
            content: content.into(),
            tool_call_id: Some(call_id.into()),
            ..Default::default()
        }
    }
}

/// A tool the model may call, described in OpenAI function-calling shape.
///
/// Kept as a plain local type — rather than re-exporting `rig`'s `ToolDefinition`
/// directly — so callers outside this module never need a `rig` dependency to
/// build one (see the module-level "who this module is for" note).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// One tool invocation the model requested, echoed back from the provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelToolCall {
    /// Provider-assigned call id. Must be echoed back verbatim as the
    /// matching tool result's `tool_call_id` (`ModelTurn::tool_result`) so
    /// the provider can pair the two up on the next turn.
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Which participant in the conversation authored a turn.
///
/// See the module-level documentation for what each role means and how the model
/// uses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModelRole {
    /// Application-controlled background instructions.  Never shown to the end user.
    #[default]
    System,
    /// The task or question being posed in this call.
    User,
    /// A previous reply from the model, included for multi-step reasoning continuity.
    Assistant,
    /// A tool result being reported back to the model. The turn's `tool_call_id`
    /// names which `ModelToolCall` this responds to.
    Tool,
}

/// All inputs the application passes to the model for a single call.
///
/// Build with `ModelRequest::text(message)` and the builder methods below.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRequest {
    /// Optional instructions that frame the model's behaviour for this call.
    /// Sent as a `System`-role message before any history or user content.
    pub system_prompt: Option<String>,
    /// Earlier turns from this reasoning session, oldest first.  Empty for one-shot
    /// classification tasks.
    pub history: Vec<ModelTurn>,
    /// The current user-facing question or task.  Ignored when `trailing_turn`
    /// is set — see `ModelRequest::continue_with`.
    pub user_message: String,
    pub max_tokens: Option<usize>,
    /// Tools the model may call this turn. Empty means no tool-calling.
    pub tools: Vec<ModelToolSpec>,
    /// When set, this turn — not `user_message` — is the trailing/prompt
    /// message sent to the provider, with `history` as everything before it.
    /// Used by the tool-calling loop to resend a tool result as the "current"
    /// turn without fabricating a fresh user utterance.
    pub trailing_turn: Option<ModelTurn>,
}

impl ModelRequest {
    pub fn text(user_message: impl Into<String>) -> Self {
        Self {
            system_prompt: None,
            history: Vec::new(),
            user_message: user_message.into(),
            max_tokens: None,
            tools: Vec::new(),
            trailing_turn: None,
        }
    }

    /// Build a request whose trailing/prompt message is `trailing` (e.g. a
    /// `ModelTurn::tool_result`) rather than a fresh user utterance.
    /// `history` is everything that came before it in the conversation.
    pub fn continue_with(history: Vec<ModelTurn>, trailing: ModelTurn) -> Self {
        Self {
            system_prompt: None,
            history,
            user_message: String::new(),
            max_tokens: None,
            tools: Vec::new(),
            trailing_turn: Some(trailing),
        }
    }

    pub fn with_system_prompt(mut self, system_prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(system_prompt.into());
        self
    }

    pub fn with_history(mut self, history: impl IntoIterator<Item = ModelTurn>) -> Self {
        self.history = history.into_iter().collect();
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    pub fn with_tools(mut self, tools: Vec<ModelToolSpec>) -> Self {
        self.tools = tools;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelResponse {
    pub assistant_text: String,
    /// Tool calls the model requested this turn. Empty for a plain-text reply.
    pub tool_calls: Vec<ModelToolCall>,
}

impl ModelResponse {
    pub fn text(assistant_text: impl Into<String>) -> Self {
        Self {
            assistant_text: assistant_text.into(),
            tool_calls: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCallEvent {
    pub provider: String,
    pub model: String,
    pub endpoint_url: String,
    pub prompt_chars: usize,
    pub history_turns: usize,
    pub max_tokens: Option<usize>,
    pub outcome: ModelCallOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelCallOutcome {
    Succeeded { response_chars: usize },
    Failed { error_kind: String },
}

pub trait ModelAuditSink: std::fmt::Debug + Send + Sync {
    fn record_model_call(&self, event: ModelCallEvent);
}

#[derive(Debug, Default)]
pub struct NoopModelAuditSink;

impl ModelAuditSink for NoopModelAuditSink {
    fn record_model_call(&self, _event: ModelCallEvent) {}
}

#[derive(Debug, Default)]
pub struct InMemoryModelAuditSink {
    events: Mutex<Vec<ModelCallEvent>>,
}

impl InMemoryModelAuditSink {
    pub fn events(&self) -> Vec<ModelCallEvent> {
        self.events
            .lock()
            .map(|events| events.clone())
            .unwrap_or_default()
    }
}

impl ModelAuditSink for InMemoryModelAuditSink {
    fn record_model_call(&self, event: ModelCallEvent) {
        if let Ok(mut events) = self.events.lock() {
            events.push(event);
        }
    }
}

/// The common interface every model backend must satisfy.
///
/// `Send + Sync` is required because the pipeline schedules agent calls across threads.
/// Implementors store only configuration (paths, strings) — not live model state —
/// so satisfying these bounds is straightforward.
///
/// Call `complete()` for free-text replies; call `extract::<T>()` to parse the reply
/// as a typed JSON value (useful for structured classification output).
pub trait AgentRuntime: Send + Sync {
    /// Send `request` to the model and return the assistant's reply.
    fn complete(&self, request: ModelRequest) -> Result<ModelResponse, AgentRuntimeError>;

    fn extract<T: DeserializeOwned>(&self, request: ModelRequest) -> Result<T, AgentRuntimeError> {
        let response = self.complete(request)?;
        parse_json_response(&response.assistant_text)
    }
}

#[derive(Debug, Error)]
pub enum AgentRuntimeError {
    #[error("chat endpoint is empty")]
    MissingEndpoint,
    #[error("chat model is empty")]
    MissingModel,
    #[error("api key is empty")]
    MissingApiKey,
    #[error("message is empty")]
    EmptyMessage,
    #[error("failed to create async runtime: {0}")]
    Runtime(#[from] std::io::Error),
    #[error("chat request failed: {0}")]
    Rig(#[from] rig::completion::CompletionError),
    #[error("chat client setup failed: {0}")]
    RigHttp(#[from] rig::http_client::Error),
    #[error("response did not contain an assistant message")]
    MissingAssistantMessage,
    #[error("failed to parse structured model response: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("typed model output failed validation: {0}")]
    InvalidTypedOutput(String),
    #[error("local llm error: {0}")]
    LocalLlm(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClassifyTransactionJob {
    pub tx_id: String,
    pub account_id: String,
    pub date: String,
    pub amount: String,
    pub description: String,
}

impl ClassifyTransactionJob {
    pub fn to_model_request(&self) -> Result<ModelRequest, AgentRuntimeError> {
        let payload = serde_json::to_string(&serde_json::json!({
            "job": "classify_transaction",
            "transaction": self,
            "return_schema": {
                "category": "non-empty string",
                "confidence": "number in [0,1]",
                "reason": "string or null",
                "suggested_tags": ["string"]
            }
        }))?;

        Ok(ModelRequest::text(payload)
            .with_system_prompt(PHI4_TYPED_JOB_SYSTEM_PROMPT)
            .with_max_tokens(256))
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct TransactionClassificationOutput {
    pub category: String,
    pub confidence: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default)]
    pub suggested_tags: Vec<String>,
}

impl TransactionClassificationOutput {
    pub fn validate(&self) -> Result<(), AgentRuntimeError> {
        if self.category.trim().is_empty() {
            return Err(AgentRuntimeError::InvalidTypedOutput(
                "category must be non-empty".to_string(),
            ));
        }
        if !self.confidence.is_finite() || !(0.0..=1.0).contains(&self.confidence) {
            return Err(AgentRuntimeError::InvalidTypedOutput(
                "confidence must be a finite number in [0,1]".to_string(),
            ));
        }
        Ok(())
    }
}

pub const PHI4_TYPED_JOB_SYSTEM_PROMPT: &str = "\
You are l3dg3rr's local Phi-4 typed-job worker. Return only valid JSON matching the requested schema. Do not include markdown.";

pub fn run_classify_transaction_job<R: AgentRuntime>(
    runtime: &R,
    job: &ClassifyTransactionJob,
) -> Result<TransactionClassificationOutput, AgentRuntimeError> {
    let output: TransactionClassificationOutput = runtime.extract(job.to_model_request()?)?;
    output.validate()?;
    Ok(output)
}

#[derive(Debug, Clone)]
pub struct RigAgentRuntime {
    settings: ChatSettings,
    audit_sink: Arc<dyn ModelAuditSink>,
}

impl RigAgentRuntime {
    pub fn new(settings: ChatSettings) -> Self {
        Self {
            settings,
            audit_sink: Arc::new(NoopModelAuditSink),
        }
    }

    pub fn with_audit_sink(mut self, audit_sink: Arc<dyn ModelAuditSink>) -> Self {
        self.audit_sink = audit_sink;
        self
    }

    async fn complete_async(
        &self,
        request: ModelRequest,
    ) -> Result<ModelResponse, AgentRuntimeError> {
        validate_settings(&self.settings)?;
        validate_request(&request)?;
        let event_base = ModelCallEventBase::new(&self.settings, &request);

        let client = openai::CompletionsClient::builder()
            .api_key(self.settings.api_key.trim())
            .base_url(normalize_base_url(self.settings.endpoint_url.trim()))
            .build()?;
        let model = client.completion_model(self.settings.model.trim());
        let request = build_completion_request(model.clone(), request);
        let response = match model.completion(request).await {
            Ok(response) => response,
            Err(error) => {
                self.audit_sink
                    .record_model_call(event_base.failed("completion_error"));
                return Err(error.into());
            }
        };
        let (assistant_text, tool_calls) = extract_assistant_reply(response.choice);
        if assistant_text.is_none() && tool_calls.is_empty() {
            self.audit_sink
                .record_model_call(event_base.failed("missing_assistant_message"));
            return Err(AgentRuntimeError::MissingAssistantMessage);
        }
        let assistant_text = assistant_text.unwrap_or_default();
        self.audit_sink
            .record_model_call(event_base.succeeded(assistant_text.chars().count()));

        Ok(ModelResponse {
            assistant_text,
            tool_calls,
        })
    }
}

impl AgentRuntime for RigAgentRuntime {
    fn complete(&self, request: ModelRequest) -> Result<ModelResponse, AgentRuntimeError> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime.block_on(self.complete_async(request))
    }
}

impl ledger_core::verify::ModelClient for RigAgentRuntime {
    fn complete(&self, prompt: &str, max_tokens: usize) -> anyhow::Result<String> {
        let response =
            AgentRuntime::complete(self, ModelRequest::text(prompt).with_max_tokens(max_tokens))?;
        Ok(response.assistant_text)
    }

    fn extract<T: DeserializeOwned>(&self, prompt: &str) -> anyhow::Result<T> {
        Ok(AgentRuntime::extract(self, ModelRequest::text(prompt))?)
    }
}

struct ModelCallEventBase {
    provider: String,
    model: String,
    endpoint_url: String,
    prompt_chars: usize,
    history_turns: usize,
    max_tokens: Option<usize>,
}

impl ModelCallEventBase {
    fn new(settings: &ChatSettings, request: &ModelRequest) -> Self {
        Self {
            provider: "openai-compatible".to_string(),
            model: settings.model.trim().to_string(),
            endpoint_url: normalize_base_url(settings.endpoint_url.trim()),
            prompt_chars: request.user_message.chars().count()
                + request
                    .system_prompt
                    .as_deref()
                    .map(|prompt| prompt.chars().count())
                    .unwrap_or_default()
                + request
                    .history
                    .iter()
                    .map(|turn| turn.content.chars().count())
                    .sum::<usize>()
                + request
                    .trailing_turn
                    .as_ref()
                    .map(|turn| turn.content.chars().count())
                    .unwrap_or_default(),
            history_turns: request.history.len(),
            max_tokens: request.max_tokens,
        }
    }

    fn succeeded(&self, response_chars: usize) -> ModelCallEvent {
        self.event(ModelCallOutcome::Succeeded { response_chars })
    }

    fn failed(&self, error_kind: &str) -> ModelCallEvent {
        self.event(ModelCallOutcome::Failed {
            error_kind: error_kind.to_string(),
        })
    }

    fn event(&self, outcome: ModelCallOutcome) -> ModelCallEvent {
        ModelCallEvent {
            provider: self.provider.clone(),
            model: self.model.clone(),
            endpoint_url: self.endpoint_url.clone(),
            prompt_chars: self.prompt_chars,
            history_turns: self.history_turns,
            max_tokens: self.max_tokens,
            outcome,
        }
    }
}

fn validate_settings(settings: &ChatSettings) -> Result<(), AgentRuntimeError> {
    if settings.endpoint_url.trim().is_empty() {
        return Err(AgentRuntimeError::MissingEndpoint);
    }
    if settings.model.trim().is_empty() {
        return Err(AgentRuntimeError::MissingModel);
    }
    if settings.api_key.trim().is_empty() {
        return Err(AgentRuntimeError::MissingApiKey);
    }
    Ok(())
}

fn validate_request(request: &ModelRequest) -> Result<(), AgentRuntimeError> {
    if request.trailing_turn.is_none() && request.user_message.trim().is_empty() {
        return Err(AgentRuntimeError::EmptyMessage);
    }
    Ok(())
}

pub(crate) fn build_completion_request<M: CompletionModel>(
    model: M,
    request: ModelRequest,
) -> rig::completion::CompletionRequest {
    let prompt_message = match &request.trailing_turn {
        Some(turn) => history_message(turn),
        None => Message::user(request.user_message.trim()),
    };
    let mut completion = model.completion_request(prompt_message);

    if let Some(system_prompt) = request.system_prompt.as_deref().map(str::trim) {
        if !system_prompt.is_empty() {
            completion = completion.preamble(system_prompt.to_string());
        }
    }

    if !request.history.is_empty() {
        completion = completion.messages(request.history.iter().map(history_message));
    }

    if let Some(max_tokens) = request.max_tokens {
        completion = completion.max_tokens(max_tokens as u64);
    }

    if !request.tools.is_empty() {
        completion = completion.tools(request.tools.iter().map(tool_definition).collect());
    }

    completion.build()
}

fn tool_definition(spec: &ModelToolSpec) -> ToolDefinition {
    ToolDefinition {
        name: spec.name.clone(),
        description: spec.description.clone(),
        parameters: spec.parameters.clone(),
    }
}

fn history_message(turn: &ModelTurn) -> Message {
    match turn.role {
        ModelRole::System => Message::system(turn.content.trim()),
        ModelRole::User => Message::user(turn.content.trim()),
        ModelRole::Assistant => {
            if turn.tool_calls.is_empty() {
                Message::assistant(turn.content.trim())
            } else {
                assistant_tool_call_message(turn)
            }
        }
        ModelRole::Tool => Message::tool_result(
            turn.tool_call_id.clone().unwrap_or_default(),
            turn.content.trim(),
        ),
    }
}

/// Builds an assistant message carrying tool calls (plus any accompanying
/// text). Only called when `turn.tool_calls` is non-empty.
fn assistant_tool_call_message(turn: &ModelTurn) -> Message {
    let mut contents: Vec<AssistantContent> = turn
        .tool_calls
        .iter()
        .map(|call| {
            AssistantContent::ToolCall(RigToolCall::new(
                call.id.clone(),
                RigToolFunction::new(call.name.clone(), call.arguments.clone()),
            ))
        })
        .collect();
    let trimmed = turn.content.trim();
    if !trimmed.is_empty() {
        contents.push(AssistantContent::text(trimmed));
    }
    match OneOrMany::many(contents) {
        Ok(content) => Message::Assistant { id: None, content },
        // Unreachable in practice: `contents` always holds at least the
        // caller's non-empty `tool_calls`. Fall back instead of panicking.
        Err(_) => Message::assistant(trimmed),
    }
}

pub(crate) fn normalize_base_url(endpoint_url: &str) -> String {
    endpoint_url
        .trim_end_matches('/')
        .trim_end_matches("/chat/completions")
        .trim_end_matches("/responses")
        .to_string()
}

/// Splits a provider's assistant content into plain text (joined, trimmed,
/// `None` if there was none) and any tool calls it requested.
pub(crate) fn extract_assistant_reply(
    contents: impl IntoIterator<Item = AssistantContent>,
) -> (Option<String>, Vec<ModelToolCall>) {
    let mut text_parts = Vec::new();
    let mut tool_calls = Vec::new();
    for content in contents {
        match content {
            AssistantContent::Text(text) => {
                let trimmed = text.text.trim();
                if !trimmed.is_empty() {
                    text_parts.push(trimmed.to_string());
                }
            }
            AssistantContent::ToolCall(tool_call) => {
                tool_calls.push(ModelToolCall {
                    id: tool_call.id,
                    name: tool_call.function.name,
                    arguments: tool_call.function.arguments,
                });
            }
            _ => {}
        }
    }
    let text = (!text_parts.is_empty()).then(|| text_parts.join("\n"));
    (text, tool_calls)
}

pub fn parse_json_response<T: DeserializeOwned>(raw: &str) -> Result<T, AgentRuntimeError> {
    let cleaned = raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    serde_json::from_str(cleaned).map_err(AgentRuntimeError::Parse)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rig::{completion::AssistantContent, providers::openai};
    use serde::Deserialize;

    #[derive(Debug)]
    struct FixedJsonRuntime {
        response: &'static str,
    }

    impl AgentRuntime for FixedJsonRuntime {
        fn complete(&self, request: ModelRequest) -> Result<ModelResponse, AgentRuntimeError> {
            assert_eq!(
                request.system_prompt.as_deref(),
                Some(PHI4_TYPED_JOB_SYSTEM_PROMPT)
            );
            assert_eq!(request.max_tokens, Some(256));
            Ok(ModelResponse::text(self.response))
        }
    }

    fn test_settings() -> ChatSettings {
        ChatSettings {
            endpoint_url: "https://example.test/v1/chat/completions".to_string(),
            api_key: "test-key".to_string(),
            model: "gpt-test".to_string(),
            system_prompt: "You are terse.".to_string(),
        }
    }

    #[test]
    fn build_request_includes_system_history_pending_user_and_max_tokens() {
        let model = openai::Client::new("test-key")
            .expect("test client")
            .completions_api()
            .completion_model("gpt-test");
        let request = ModelRequest::text("What next?")
            .with_system_prompt("You are terse.")
            .with_history([ModelTurn {
                role: ModelRole::Assistant,
                content: "Earlier answer".to_string(),
                ..Default::default()
            }])
            .with_max_tokens(128);

        let request = build_completion_request(model, request);
        let messages: Vec<_> = request.chat_history.into_iter().collect();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0], Message::system("You are terse."));
        assert_eq!(messages[1], Message::assistant("Earlier answer"));
        assert_eq!(messages[2], Message::user("What next?"));
        assert_eq!(request.max_tokens, Some(128));
    }

    #[test]
    fn build_request_attaches_tools_when_present() {
        let model = openai::Client::new("test-key")
            .expect("test client")
            .completions_api()
            .completion_model("gpt-test");
        let request = ModelRequest::text("What next?").with_tools(vec![ModelToolSpec {
            name: "get_evidence_dashboard".to_string(),
            description: "Read the evidence dashboard".to_string(),
            parameters: serde_json::json!({"type": "object", "properties": {}}),
        }]);

        let request = build_completion_request(model, request);
        assert_eq!(request.tools.len(), 1);
        assert_eq!(request.tools[0].name, "get_evidence_dashboard");
    }

    #[test]
    fn build_request_with_trailing_tool_result_places_it_last_with_matching_call_id() {
        let model = openai::Client::new("test-key")
            .expect("test client")
            .completions_api()
            .completion_model("gpt-test");

        let assistant_turn = ModelTurn::assistant_tool_calls(vec![ModelToolCall {
            id: "call_abc".to_string(),
            name: "get_evidence_dashboard".to_string(),
            arguments: serde_json::json!({}),
        }]);
        let tool_result_turn = ModelTurn::tool_result("call_abc", "{\"ok\":true}");

        let request = ModelRequest::continue_with(vec![assistant_turn], tool_result_turn);
        let request = build_completion_request(model, request);
        let messages: Vec<_> = request.chat_history.into_iter().collect();

        assert_eq!(messages.len(), 2);
        match &messages[0] {
            Message::Assistant { content, .. } => {
                assert!(matches!(content.first(), AssistantContent::ToolCall(_)));
            }
            other => panic!("expected assistant tool-call message, got {other:?}"),
        }
        match &messages[1] {
            Message::User { content } => match content.first() {
                rig::message::UserContent::ToolResult(result) => {
                    assert_eq!(result.id, "call_abc");
                }
                other => panic!("expected tool result content, got {other:?}"),
            },
            other => panic!("expected tool result message, got {other:?}"),
        }
    }

    #[test]
    fn extract_assistant_reply_prefers_text_content() {
        let (text, tool_calls) = extract_assistant_reply([
            AssistantContent::text("  hello world  "),
            AssistantContent::reasoning("ignored"),
        ]);
        assert_eq!(text.as_deref(), Some("hello world"));
        assert!(tool_calls.is_empty());
    }

    #[test]
    fn extract_assistant_reply_surfaces_tool_calls_instead_of_dropping_them() {
        // Regression guard: the earlier `extract_assistant_message` only
        // matched `AssistantContent::Text` and silently discarded
        // `ToolCall` content. That's the bug this loop depends on being fixed.
        let call = AssistantContent::ToolCall(RigToolCall::new(
            "call_1".to_string(),
            RigToolFunction::new("get_evidence_dashboard".to_string(), serde_json::json!({})),
        ));
        let (text, tool_calls) = extract_assistant_reply([call]);
        assert_eq!(text, None);
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].id, "call_1");
        assert_eq!(tool_calls[0].name, "get_evidence_dashboard");
    }

    #[test]
    fn normalize_base_url_accepts_full_or_root_openai_urls() {
        assert_eq!(
            normalize_base_url("https://api.openai.com/v1/chat/completions"),
            "https://api.openai.com/v1"
        );
        assert_eq!(
            normalize_base_url("https://api.openai.com/v1/"),
            "https://api.openai.com/v1"
        );
    }

    #[test]
    fn empty_fields_are_rejected_before_network() {
        let runtime = RigAgentRuntime::new(ChatSettings {
            endpoint_url: String::new(),
            ..test_settings()
        });
        assert!(matches!(
            AgentRuntime::complete(&runtime, ModelRequest::text("hello")),
            Err(AgentRuntimeError::MissingEndpoint)
        ));

        let runtime = RigAgentRuntime::new(ChatSettings {
            model: String::new(),
            ..test_settings()
        });
        assert!(matches!(
            AgentRuntime::complete(&runtime, ModelRequest::text("hello")),
            Err(AgentRuntimeError::MissingModel)
        ));

        let runtime = RigAgentRuntime::new(ChatSettings {
            api_key: String::new(),
            ..test_settings()
        });
        assert!(matches!(
            AgentRuntime::complete(&runtime, ModelRequest::text("hello")),
            Err(AgentRuntimeError::MissingApiKey)
        ));

        let runtime = RigAgentRuntime::new(test_settings());
        assert!(matches!(
            AgentRuntime::complete(&runtime, ModelRequest::text("   ")),
            Err(AgentRuntimeError::EmptyMessage)
        ));
    }

    #[test]
    fn parse_json_response_accepts_markdown_fence() {
        #[derive(Debug, Deserialize, PartialEq, Eq)]
        struct Output {
            answer: String,
        }

        let output: Output = parse_json_response("```json\n{\"answer\":\"yes\"}\n```").unwrap();
        assert_eq!(
            output,
            Output {
                answer: "yes".to_string()
            }
        );
    }

    #[test]
    fn classify_transaction_job_builds_typed_request() {
        let job = ClassifyTransactionJob {
            tx_id: "tx_123".to_string(),
            account_id: "WF-BH-CHK".to_string(),
            date: "2024-01-31".to_string(),
            amount: "-12.34".to_string(),
            description: "Cafe lunch".to_string(),
        };

        let request = job.to_model_request().expect("model request");

        assert_eq!(
            request.system_prompt.as_deref(),
            Some(PHI4_TYPED_JOB_SYSTEM_PROMPT)
        );
        assert_eq!(request.max_tokens, Some(256));
        assert!(request
            .user_message
            .contains("\"job\":\"classify_transaction\""));
        assert!(request.user_message.contains("\"tx_id\":\"tx_123\""));
        assert!(request
            .user_message
            .contains("\"confidence\":\"number in [0,1]\""));
    }

    #[test]
    fn typed_classification_output_validation_rejects_invalid_values() {
        let empty_category = TransactionClassificationOutput {
            category: " ".to_string(),
            confidence: 0.5,
            reason: None,
            suggested_tags: Vec::new(),
        };
        assert!(matches!(
            empty_category.validate(),
            Err(AgentRuntimeError::InvalidTypedOutput(_))
        ));

        let bad_confidence = TransactionClassificationOutput {
            category: "Meals".to_string(),
            confidence: 1.5,
            reason: None,
            suggested_tags: Vec::new(),
        };
        assert!(matches!(
            bad_confidence.validate(),
            Err(AgentRuntimeError::InvalidTypedOutput(_))
        ));
    }

    #[test]
    fn run_classify_transaction_job_extracts_and_validates_json() {
        let runtime = FixedJsonRuntime {
            response: r##"{"category":"Meals","confidence":0.81,"reason":"lunch vendor","suggested_tags":["#meal"]}"##,
        };
        let job = ClassifyTransactionJob {
            tx_id: "tx_123".to_string(),
            account_id: "WF-BH-CHK".to_string(),
            date: "2024-01-31".to_string(),
            amount: "-12.34".to_string(),
            description: "Cafe lunch".to_string(),
        };

        let output = run_classify_transaction_job(&runtime, &job).expect("typed output");

        assert_eq!(output.category, "Meals");
        assert_eq!(output.confidence, 0.81);
        assert_eq!(output.suggested_tags, ["#meal"]);
    }

    #[test]
    fn model_call_event_base_excludes_prompt_content() {
        let request = ModelRequest::text("secret prompt text")
            .with_system_prompt("system preamble")
            .with_history([ModelTurn {
                role: ModelRole::Assistant,
                content: "earlier response".to_string(),
                ..Default::default()
            }])
            .with_max_tokens(64);
        let base = ModelCallEventBase::new(&test_settings(), &request);
        let event = base.failed("completion_error");

        assert_eq!(event.model, "gpt-test");
        assert_eq!(event.endpoint_url, "https://example.test/v1");
        assert_eq!(
            event.prompt_chars,
            "secret prompt text".len() + "system preamble".len() + "earlier response".len()
        );
        assert_eq!(event.history_turns, 1);
        assert_eq!(event.max_tokens, Some(64));
        assert_eq!(
            event.outcome,
            ModelCallOutcome::Failed {
                error_kind: "completion_error".to_string()
            }
        );
    }
}
