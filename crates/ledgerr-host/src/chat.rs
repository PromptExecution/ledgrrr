use thiserror::Error;

use crate::agent_runtime::{
    AgentRuntime, AgentRuntimeError, ModelRequest, ModelRole, ModelToolSpec, ModelTurn,
    RigAgentRuntime,
};
use crate::settings::ChatSettings;

pub const RHAI_RULE_SYSTEM_PROMPT: &str = "You are the l3dg3rr Rhai rule editor. Return only supported documentation DSL lines unless explicitly asked for explanation. Supported lines are `fn source() -> target`, `if expression -> target`, and `match expr => Arm -> target`. Preserve financial audit safety: do not bypass confidence, review, or commit approval gates.";
pub const DEFAULT_RHAI_RULE_MODEL: &str = "phi-4-mini-reasoning";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatTurn {
    pub role: ChatRole,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatRole {
    System,
    User,
    Assistant,
    /// A tool call or tool result from the tool-calling loop, rendered
    /// distinctly from model text in the transcript.
    Tool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionDiff {
    pub field: String,
    pub before: String,
    pub after: String,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewLogEntry {
    pub action: String,
    pub summary: String,
    pub diffs: Vec<DecisionDiff>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReviewLog {
    entries: Vec<ReviewLogEntry>,
}

impl ReviewLog {
    pub fn push(&mut self, entry: ReviewLogEntry) {
        self.entries.push(entry);
    }

    pub fn entries(&self) -> &[ReviewLogEntry] {
        &self.entries
    }

    pub fn render(&self) -> String {
        render_review_log(&self.entries)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RigPromptPreview {
    pub endpoint_url: String,
    pub model: String,
    pub messages_json: String,
}

impl RigPromptPreview {
    pub fn render(&self) -> String {
        format!(
            "Rig OpenAI-compatible request\nPOST {}\nmodel: {}\n\n{}",
            self.endpoint_url, self.model, self.messages_json
        )
    }
}

pub fn render_rig_exchange_log(
    preview: &RigPromptPreview,
    backend_status: &str,
    response: Option<&str>,
    error: Option<&str>,
) -> String {
    let mut lines = vec![
        preview.render(),
        String::new(),
        "Rig/OpenAI backend status".to_string(),
        backend_status.trim().to_string(),
        String::new(),
        "Rig/OpenAI response".to_string(),
    ];

    match (response, error) {
        (Some(response), _) => lines.push(response.trim().to_string()),
        (_, Some(error)) => lines.push(format!("ERROR: {}", error.trim())),
        (None, None) => lines.push("Awaiting response...".to_string()),
    }

    lines.join("\n")
}

#[derive(Debug, Error)]
pub enum ChatError {
    #[error("chat endpoint is empty")]
    MissingEndpoint,
    #[error("chat model is empty")]
    MissingModel,
    #[error("api key is empty")]
    MissingApiKey,
    #[error("message is empty")]
    EmptyMessage,
    #[error("failed to create async runtime: {0}")]
    Runtime(std::io::Error),
    #[error("chat request failed: {0}")]
    Rig(rig::completion::CompletionError),
    #[error("chat client setup failed: {0}")]
    RigHttp(rig::http_client::Error),
    #[error("response did not contain an assistant message")]
    MissingAssistantMessage,
    #[error("failed to parse structured model response: {0}")]
    Parse(serde_json::Error),
    #[error("typed model output failed validation: {0}")]
    InvalidTypedOutput(String),
    #[error("local llm error: {0}")]
    LocalLlm(String),
}

pub fn send_chat_message(
    settings: &ChatSettings,
    history: &[ChatTurn],
    pending_message: &str,
) -> Result<String, ChatError> {
    let request = ModelRequest::text(pending_message)
        .with_system_prompt(settings.system_prompt.clone())
        .with_history(history.iter().map(model_turn));
    let runtime = RigAgentRuntime::new(settings.clone());
    let response = runtime.complete(request).map_err(ChatError::from)?;
    Ok(response.assistant_text)
}

/// Default cap on model<->tool round-trips within a single
/// `send_message_with_tools` call before giving up and returning a
/// budget-exhausted outcome. Generous enough for a multi-step
/// lookup-then-classify flow while still bounding worst-case latency/cost if
/// the model loops on a tool.
pub const DEFAULT_MAX_TOOL_TURNS: usize = 6;

/// One event in a tool-calling exchange, for transcript rendering — lets the
/// UI show tool calls/results distinctly from model text instead of flattening
/// everything into one assistant turn.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatEvent {
    ToolCall {
        name: String,
        arguments: serde_json::Value,
    },
    ToolResult {
        name: String,
        result: serde_json::Value,
    },
    Assistant(String),
}

/// Result of running the tool-calling loop to completion (or budget
/// exhaustion).
#[derive(Debug, Clone, PartialEq)]
pub struct ToolLoopOutcome {
    /// Every tool call/result pair and the final assistant reply, in order.
    pub events: Vec<ChatEvent>,
    /// The model's final plain-text reply, or an explanatory budget-exhausted
    /// message — never silently empty.
    pub final_text: String,
    /// True if the loop hit `max_turns` while the model was still requesting
    /// tool calls, rather than stopping naturally on a plain-text reply.
    pub budget_exhausted: bool,
}

/// Runs the chat<->tool loop: send with the allowlisted tool schema, dispatch
/// any `tool_calls` the model requests via `dispatch`, feed results back, and
/// repeat until the model replies with plain text or `max_turns` round-trips
/// are exhausted.
///
/// `runtime` is generic over [`AgentRuntime`] (rather than this function
/// constructing a `RigAgentRuntime` itself) and `dispatch` is injected
/// (rather than this module calling `chat_tools::dispatch_mcp_tool`
/// directly) so the whole loop — including the turn-budget cap — is
/// unit-testable with a fake backend, without a live model endpoint or a
/// `TurboLedgerService`/Tauri `AppState`. See `bin/tauri/commands.rs` for the
/// real wiring. Allowlist enforcement lives in `dispatch`, not here: this
/// loop calls whatever `dispatch` returns for any tool name the model asks
/// for and feeds that back as the tool result, same as a rejection.
pub fn send_message_with_tools<R: AgentRuntime>(
    runtime: &R,
    settings: &ChatSettings,
    history: &[ChatTurn],
    pending_message: &str,
    tools: &[ModelToolSpec],
    dispatch: &dyn Fn(&str, &serde_json::Value) -> serde_json::Value,
    max_turns: usize,
) -> Result<ToolLoopOutcome, ChatError> {
    let mut events = Vec::new();

    let mut turns_so_far: Vec<ModelTurn> = history.iter().map(model_turn).collect();
    turns_so_far.push(ModelTurn {
        role: ModelRole::User,
        content: pending_message.to_string(),
        ..Default::default()
    });

    let mut request = ModelRequest::text(pending_message)
        .with_system_prompt(settings.system_prompt.clone())
        .with_history(history.iter().map(model_turn))
        .with_tools(tools.to_vec());

    for _ in 0..max_turns {
        let response = AgentRuntime::complete(runtime, request).map_err(ChatError::from)?;

        if response.tool_calls.is_empty() {
            events.push(ChatEvent::Assistant(response.assistant_text.clone()));
            return Ok(ToolLoopOutcome {
                events,
                final_text: response.assistant_text,
                budget_exhausted: false,
            });
        }

        turns_so_far.push(ModelTurn::assistant_tool_calls(response.tool_calls.clone()));

        for call in &response.tool_calls {
            events.push(ChatEvent::ToolCall {
                name: call.name.clone(),
                arguments: call.arguments.clone(),
            });
            let result = dispatch(&call.name, &call.arguments);
            events.push(ChatEvent::ToolResult {
                name: call.name.clone(),
                result: result.clone(),
            });
            let content = serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());
            turns_so_far.push(ModelTurn::tool_result(call.id.clone(), content));
        }

        let Some(trailing) = turns_so_far.last().cloned() else {
            // Unreachable: the loop above just pushed at least one tool-result
            // turn (response.tool_calls was checked non-empty). Fail safe
            // rather than index/unwrap if that invariant is ever broken.
            break;
        };
        let history_for_request = turns_so_far[..turns_so_far.len() - 1].to_vec();
        request = ModelRequest::continue_with(history_for_request, trailing)
            .with_system_prompt(settings.system_prompt.clone())
            .with_tools(tools.to_vec());
    }

    let exhausted_text = format!(
        "Tool budget exhausted after {max_turns} round-trip(s) without a final reply. \
         Ask a narrower question, or increase the turn budget, and try again."
    );
    events.push(ChatEvent::Assistant(exhausted_text.clone()));
    Ok(ToolLoopOutcome {
        events,
        final_text: exhausted_text,
        budget_exhausted: true,
    })
}

pub fn build_rig_prompt_preview(
    settings: &ChatSettings,
    history: &[ChatTurn],
    pending_message: &str,
) -> RigPromptPreview {
    let mut messages = Vec::new();
    let system_prompt = settings.system_prompt.trim();
    if !system_prompt.is_empty() {
        messages.push(serde_json::json!({
            "role": "system",
            "content": system_prompt,
        }));
    }
    for turn in history {
        let content = turn.content.trim();
        if content.is_empty() {
            continue;
        }
        messages.push(serde_json::json!({
            "role": chat_role_name(turn.role),
            "content": content,
        }));
    }
    messages.push(serde_json::json!({
        "role": "user",
        "content": pending_message.trim(),
    }));

    let payload = serde_json::json!({
        "model": settings.model.trim(),
        "messages": messages,
        "stream": false,
    });

    RigPromptPreview {
        endpoint_url: settings.endpoint_url.trim().to_string(),
        model: settings.model.trim().to_string(),
        messages_json: serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()),
    }
}

pub fn render_transcript(history: &[ChatTurn]) -> String {
    if history.is_empty() {
        return "No messages yet.".to_string();
    }

    history
        .iter()
        .map(|turn| {
            let speaker = match turn.role {
                ChatRole::System => "System",
                ChatRole::User => "You",
                ChatRole::Assistant => "Assistant",
                ChatRole::Tool => "Tool",
            };
            format!("{speaker}\n{}\n", turn.content.trim())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn chat_role_name(role: ChatRole) -> &'static str {
    match role {
        ChatRole::System => "system",
        ChatRole::User => "user",
        ChatRole::Assistant => "assistant",
        ChatRole::Tool => "tool",
    }
}

pub fn rhai_rule_prompt_seed() -> &'static str {
    "Mutate this Rhai workflow to add a review path for medium-confidence classifications and explain the change in one short paragraph:\n\nfn ingest_pdf() -> detect_shape\nfn detect_shape() -> classify_rows\nif confidence > 0.85 -> commit_workbook\nif confidence <= 0.85 -> review_flag\nfn review_flag() -> commit_workbook"
}

pub fn rhai_rule_prompt_seed_log(
    previous_model: &str,
    previous_system_prompt: &str,
) -> ReviewLogEntry {
    let mut diffs = Vec::new();
    if previous_model.trim().is_empty() {
        diffs.push(DecisionDiff {
            field: "chat.model".to_string(),
            before: "<empty>".to_string(),
            after: DEFAULT_RHAI_RULE_MODEL.to_string(),
            rationale: "Use the default local Phi-family example target for rule mutation prompts."
                .to_string(),
        });
    }
    if previous_system_prompt.trim() != RHAI_RULE_SYSTEM_PROMPT {
        diffs.push(DecisionDiff {
            field: "chat.system_prompt".to_string(),
            before: summarize_value(previous_system_prompt),
            after: summarize_value(RHAI_RULE_SYSTEM_PROMPT),
            rationale: "Constrain model output to supported Rhai DSL and audit-safe review gates."
                .to_string(),
        });
    }

    ReviewLogEntry {
        action: "seed_rhai_rule_prompt".to_string(),
        summary: "Prepared the chat surface for Rhai rule mutation review.".to_string(),
        diffs,
    }
}

pub fn user_request_log(message: &str) -> ReviewLogEntry {
    ReviewLogEntry {
        action: "submit_chat_request".to_string(),
        summary: summarize_value(message),
        diffs: vec![DecisionDiff {
            field: "pending_request".to_string(),
            before: "<none>".to_string(),
            after: summarize_value(message),
            rationale: "Capture the operator request that produced the next model response."
                .to_string(),
        }],
    }
}

pub fn assistant_decision_log(previous_rhai: &str, assistant_text: &str) -> ReviewLogEntry {
    let proposed = extract_rhai_decision_lines(assistant_text);
    let before = extract_rhai_decision_lines(previous_rhai);
    let diffs = diff_decision_lines(&before, &proposed);
    let summary = if proposed.is_empty() {
        "Assistant response did not contain supported Rhai DSL decision lines.".to_string()
    } else {
        format!(
            "Assistant proposed {} supported Rhai decision line(s).",
            proposed.len()
        )
    };

    ReviewLogEntry {
        action: "assistant_decision_diff".to_string(),
        summary,
        diffs,
    }
}

pub fn render_review_log(entries: &[ReviewLogEntry]) -> String {
    if entries.is_empty() {
        return "No review log entries yet.".to_string();
    }

    entries
        .iter()
        .enumerate()
        .map(|(idx, entry)| {
            let mut out = format!(
                "#{idx}: {}\n{}\n",
                entry.action,
                entry.summary,
                idx = idx + 1
            );
            if entry.diffs.is_empty() {
                out.push_str("Diffset: no field changes detected.\n");
            } else {
                out.push_str("Diffset:\n");
                for diff in &entry.diffs {
                    out.push_str(&format!(
                        "- {}: {} -> {}\n  because {}\n",
                        diff.field, diff.before, diff.after, diff.rationale
                    ));
                }
            }
            out
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn extract_rhai_decision_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| {
            line.starts_with("fn ") || line.starts_with("if ") || line.starts_with("match ")
        })
        .map(str::to_string)
        .collect()
}

fn diff_decision_lines(before: &[String], after: &[String]) -> Vec<DecisionDiff> {
    let mut diffs = Vec::new();
    for line in after {
        if !before.contains(line) {
            diffs.push(DecisionDiff {
                field: "rhai.decision.added".to_string(),
                before: "<absent>".to_string(),
                after: line.clone(),
                rationale: "Model output introduced a new supported Rhai decision line."
                    .to_string(),
            });
        }
    }
    for line in before {
        if !after.contains(line) {
            diffs.push(DecisionDiff {
                field: "rhai.decision.removed".to_string(),
                before: line.clone(),
                after: "<absent>".to_string(),
                rationale: "Model output omitted a previously visible Rhai decision line."
                    .to_string(),
            });
        }
    }
    diffs
}

fn summarize_value(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return "<empty>".to_string();
    }
    const MAX_CHARS: usize = 120;
    let mut summary: String = trimmed.chars().take(MAX_CHARS).collect();
    if trimmed.chars().count() > MAX_CHARS {
        summary.push_str("...");
    }
    summary
}

fn model_turn(turn: &ChatTurn) -> ModelTurn {
    ModelTurn {
        role: match turn.role {
            ChatRole::System => ModelRole::System,
            ChatRole::User => ModelRole::User,
            ChatRole::Assistant => ModelRole::Assistant,
            // Persisted tool-activity turns are folded back in as plain
            // context on later user messages, not replayed as literal
            // OpenAI tool-role messages: we deliberately don't try to
            // reconstruct the original `tool_call_id` pairing across
            // separate `send_message` calls, since a `tool`-role message
            // that doesn't immediately follow its matching assistant
            // `tool_calls` message is a protocol error most OpenAI-compatible
            // endpoints reject outright. Within a single tool-calling loop
            // (`send_message_with_tools`), the real id-correct threading
            // happens via `ModelTurn::assistant_tool_calls`/`tool_result` on
            // the loop's own ephemeral turn list — it never goes through
            // this function.
            ChatRole::Tool => ModelRole::User,
        },
        content: turn.content.clone(),
        ..Default::default()
    }
}

/// Renders the tool-call/tool-result events from a [`ToolLoopOutcome`] as
/// `ChatRole::Tool` turns, so callers can append them to the persisted chat
/// history and `render_transcript` shows them distinctly from model text.
/// The trailing `ChatEvent::Assistant` (the final reply) is intentionally
/// skipped — callers append that themselves as an ordinary
/// `ChatRole::Assistant` turn, same as the no-tools path.
pub fn tool_event_chat_turns(events: &[ChatEvent]) -> Vec<ChatTurn> {
    events
        .iter()
        .filter_map(|event| match event {
            ChatEvent::ToolCall { name, arguments } => Some(ChatTurn {
                role: ChatRole::Tool,
                content: format!("Called {name}({arguments})"),
            }),
            ChatEvent::ToolResult { name, result } => Some(ChatTurn {
                role: ChatRole::Tool,
                content: format!(
                    "{name} ->\n{}",
                    serde_json::to_string_pretty(result).unwrap_or_else(|_| result.to_string())
                ),
            }),
            ChatEvent::Assistant(_) => None,
        })
        .collect()
}

impl From<AgentRuntimeError> for ChatError {
    fn from(value: AgentRuntimeError) -> Self {
        match value {
            AgentRuntimeError::MissingEndpoint => Self::MissingEndpoint,
            AgentRuntimeError::MissingModel => Self::MissingModel,
            AgentRuntimeError::MissingApiKey => Self::MissingApiKey,
            AgentRuntimeError::EmptyMessage => Self::EmptyMessage,
            AgentRuntimeError::Runtime(error) => Self::Runtime(error),
            AgentRuntimeError::Rig(error) => Self::Rig(error),
            AgentRuntimeError::RigHttp(error) => Self::RigHttp(error),
            AgentRuntimeError::MissingAssistantMessage => Self::MissingAssistantMessage,
            AgentRuntimeError::Parse(error) => Self::Parse(error),
            AgentRuntimeError::InvalidTypedOutput(msg) => Self::InvalidTypedOutput(msg),
            AgentRuntimeError::LocalLlm(msg) => Self::LocalLlm(msg),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_runtime::{ModelResponse, ModelToolCall};

    fn test_settings() -> ChatSettings {
        ChatSettings {
            endpoint_url: "https://example.test/v1/chat/completions".to_string(),
            api_key: "test-key".to_string(),
            model: "gpt-test".to_string(),
            system_prompt: "You are terse.".to_string(),
        }
    }

    #[test]
    fn empty_fields_are_rejected_before_network() {
        let missing_endpoint = ChatSettings {
            endpoint_url: String::new(),
            ..test_settings()
        };
        assert!(matches!(
            send_chat_message(&missing_endpoint, &[], "hello"),
            Err(ChatError::MissingEndpoint)
        ));

        let missing_model = ChatSettings {
            model: String::new(),
            ..test_settings()
        };
        assert!(matches!(
            send_chat_message(&missing_model, &[], "hello"),
            Err(ChatError::MissingModel)
        ));

        let missing_key = ChatSettings {
            api_key: String::new(),
            ..test_settings()
        };
        assert!(matches!(
            send_chat_message(&missing_key, &[], "hello"),
            Err(ChatError::MissingApiKey)
        ));

        assert!(matches!(
            send_chat_message(&test_settings(), &[], "   "),
            Err(ChatError::EmptyMessage)
        ));
    }

    #[test]
    fn tool_event_chat_turns_skips_the_final_assistant_event_and_labels_the_rest_tool() {
        let events = vec![
            ChatEvent::ToolCall {
                name: "query_audit_log".to_string(),
                arguments: serde_json::json!({}),
            },
            ChatEvent::ToolResult {
                name: "query_audit_log".to_string(),
                result: serde_json::json!({"isError": false}),
            },
            ChatEvent::Assistant("here is what I found".to_string()),
        ];

        let turns = tool_event_chat_turns(&events);

        assert_eq!(turns.len(), 2);
        assert!(turns.iter().all(|turn| turn.role == ChatRole::Tool));
        assert!(turns[0].content.contains("Called query_audit_log"));
        assert!(turns[1].content.contains("query_audit_log"));
        assert!(turns[1].content.contains("isError"));
    }

    #[test]
    fn transcript_renders_roles_for_slint_display() {
        let transcript = render_transcript(&[
            ChatTurn {
                role: ChatRole::User,
                content: "mutate rule".to_string(),
            },
            ChatTurn {
                role: ChatRole::Assistant,
                content: "fn classify() -> review".to_string(),
            },
        ]);

        assert!(transcript.contains("You\nmutate rule"));
        assert!(transcript.contains("Assistant\nfn classify() -> review"));
    }

    #[test]
    fn rig_prompt_preview_shows_internal_openai_request_shape() {
        let settings = ChatSettings {
            endpoint_url: "http://127.0.0.1:15115/v1/chat/completions".to_string(),
            api_key: "local-tool-tray".to_string(),
            model: "phi-4-mini-reasoning".to_string(),
            system_prompt: "Use Rhai DSL.".to_string(),
        };
        let preview = build_rig_prompt_preview(
            &settings,
            &[ChatTurn {
                role: ChatRole::Assistant,
                content: "Earlier answer".to_string(),
            }],
            "fn classify_rows() -> score_confidence",
        );

        let rendered = preview.render();
        assert!(rendered.contains("POST http://127.0.0.1:15115/v1/chat/completions"));
        assert!(rendered.contains("phi-4-mini-reasoning"));
        assert!(rendered.contains("\"role\": \"system\""));
        assert!(rendered.contains("\"role\": \"assistant\""));
        assert!(rendered.contains("fn classify_rows() -> score_confidence"));
    }

    #[test]
    fn rig_exchange_log_shows_request_backend_and_response() {
        let preview = RigPromptPreview {
            endpoint_url: "http://127.0.0.1:15115/v1/chat/completions".to_string(),
            model: DEFAULT_RHAI_RULE_MODEL.to_string(),
            messages_json: "{}".to_string(),
        };

        let log = render_rig_exchange_log(
            &preview,
            "mistralrs: not compiled\ncandle: not compiled\nmodel: phi-4-mini-reasoning",
            Some("assistant text"),
            None,
        );

        assert!(log.contains("POST http://127.0.0.1:15115/v1/chat/completions"));
        assert!(log.contains("mistralrs: not compiled"));
        assert!(log.contains("candle: not compiled"));
        assert!(log.contains("assistant text"));
    }

    #[test]
    fn seed_prompt_log_records_model_and_system_prompt_diffset() {
        let entry = rhai_rule_prompt_seed_log("", "old prompt");

        assert_eq!(entry.action, "seed_rhai_rule_prompt");
        assert!(entry
            .diffs
            .iter()
            .any(|diff| { diff.field == "chat.model" && diff.after == DEFAULT_RHAI_RULE_MODEL }));
        assert!(entry
            .diffs
            .iter()
            .any(|diff| diff.field == "chat.system_prompt"));
    }

    #[test]
    fn assistant_decision_log_diffs_supported_rhai_lines() {
        let entry = assistant_decision_log(
            "fn classify_rows() -> score_confidence\nif confidence <= 0.85 -> review_flag",
            "Explanation\n```rhai\nfn classify_rows() -> score_confidence\nif confidence > 0.85 -> commit_workbook\nif confidence > 0.60 -> review_flag\n```",
        );

        assert_eq!(entry.action, "assistant_decision_diff");
        assert!(entry
            .diffs
            .iter()
            .any(|diff| diff.field == "rhai.decision.added"
                && diff.after == "if confidence > 0.60 -> review_flag"));
        assert!(entry
            .diffs
            .iter()
            .any(|diff| diff.field == "rhai.decision.removed"
                && diff.before == "if confidence <= 0.85 -> review_flag"));
    }

    #[test]
    fn review_log_render_is_a_readable_diffset() {
        let mut log = ReviewLog::default();
        log.push(user_request_log("Add a review lane"));

        let rendered = log.render();
        assert!(rendered.contains("#1: submit_chat_request"));
        assert!(rendered.contains("Diffset:"));
        assert!(rendered.contains("pending_request"));
    }

    /// A fake backend that always asks to call the same tool, forever — used
    /// to prove the turn-budget cap actually stops the loop instead of
    /// looping until the model produces text (which it never will here).
    struct AlwaysToolCallRuntime;

    impl AgentRuntime for AlwaysToolCallRuntime {
        fn complete(&self, _request: ModelRequest) -> Result<ModelResponse, AgentRuntimeError> {
            Ok(ModelResponse {
                assistant_text: String::new(),
                tool_calls: vec![ModelToolCall {
                    id: "call_1".to_string(),
                    name: "query_audit_log".to_string(),
                    arguments: serde_json::json!({}),
                }],
            })
        }
    }

    #[test]
    fn send_message_with_tools_stops_at_the_turn_budget_and_says_so() {
        let runtime = AlwaysToolCallRuntime;
        let dispatch = |_name: &str, _args: &serde_json::Value| serde_json::json!({"ok": true});

        let outcome = send_message_with_tools(
            &runtime,
            &test_settings(),
            &[],
            "how many flags are open?",
            &[],
            &dispatch,
            3,
        )
        .expect("the loop returns Ok even when the turn budget is exhausted");

        assert!(outcome.budget_exhausted);
        assert!(
            outcome.final_text.contains("Tool budget exhausted"),
            "budget exhaustion must be a visible transcript entry, not a silent truncation: {}",
            outcome.final_text
        );
        let tool_call_events = outcome
            .events
            .iter()
            .filter(|event| matches!(event, ChatEvent::ToolCall { .. }))
            .count();
        assert_eq!(
            tool_call_events, 3,
            "loop must stop after exactly `max_turns` tool round-trips, not fewer or more"
        );
    }

    #[test]
    fn send_message_with_tools_terminates_immediately_when_the_budget_is_zero() {
        let runtime = AlwaysToolCallRuntime;
        let dispatch = |_name: &str, _args: &serde_json::Value| serde_json::json!({"ok": true});

        let outcome =
            send_message_with_tools(&runtime, &test_settings(), &[], "hello", &[], &dispatch, 0)
                .expect("zero turns is a valid, non-panicking budget");

        assert!(outcome.budget_exhausted);
        assert!(outcome
            .events
            .iter()
            .all(|event| !matches!(event, ChatEvent::ToolCall { .. })));
    }

    /// A fake backend that asks for a (disallowed) tool once, then checks
    /// that the dispatcher's rejection came back as *this* turn's trailing
    /// tool-result content with the matching call id, before replying in text.
    struct ToolCallThenTextRuntime {
        call_count: std::sync::atomic::AtomicUsize,
    }

    impl AgentRuntime for ToolCallThenTextRuntime {
        fn complete(&self, request: ModelRequest) -> Result<ModelResponse, AgentRuntimeError> {
            let call = self
                .call_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if call == 0 {
                Ok(ModelResponse {
                    assistant_text: String::new(),
                    tool_calls: vec![ModelToolCall {
                        id: "call_1".to_string(),
                        name: "reconcile_postings".to_string(),
                        arguments: serde_json::json!({}),
                    }],
                })
            } else {
                let trailing = request
                    .trailing_turn
                    .expect("a continuation call must set trailing_turn");
                assert_eq!(trailing.tool_call_id.as_deref(), Some("call_1"));
                assert!(
                    trailing.content.contains("reconcile_postings")
                        && trailing.content.contains("not in the chat allowlist"),
                    "dispatcher's rejection text must reach the model verbatim: {}",
                    trailing.content
                );
                Ok(ModelResponse::text(
                    "understood, that tool is not available",
                ))
            }
        }
    }

    #[test]
    fn send_message_with_tools_feeds_a_rejected_tool_call_back_as_its_result() {
        let runtime = ToolCallThenTextRuntime {
            call_count: std::sync::atomic::AtomicUsize::new(0),
        };
        let dispatch = |name: &str, _args: &serde_json::Value| {
            serde_json::json!({
                "ok": false,
                "error": format!("tool '{name}' is not in the chat allowlist")
            })
        };

        let outcome = send_message_with_tools(
            &runtime,
            &test_settings(),
            &[],
            "please call a disallowed tool",
            &[],
            &dispatch,
            DEFAULT_MAX_TOOL_TURNS,
        )
        .expect("loop completes once the model stops requesting tools");

        assert!(!outcome.budget_exhausted);
        assert_eq!(outcome.final_text, "understood, that tool is not available");
        assert!(outcome.events.iter().any(
            |event| matches!(event, ChatEvent::ToolResult { name, .. } if name == "reconcile_postings")
        ));
    }
}
