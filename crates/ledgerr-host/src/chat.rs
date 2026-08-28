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
    /// A mutation tool call blocked awaiting explicit operator confirmation
    /// — see `ChatEvent::PendingConfirmation` / `pending_confirmation_card`.
    PendingConfirmation,
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
/// UI show tool calls/results/pending-confirmations distinctly from model
/// text instead of flattening everything into one assistant turn.
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
    /// A mutation tool call is blocked awaiting explicit operator
    /// confirmation (approve/reject) before it is dispatched — see
    /// [`PendingToolCall`] and [`resume_after_confirmation`].
    PendingConfirmation {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    Assistant(String),
}

/// A mutation tool call the model requested, held back from dispatch until
/// the operator explicitly approves or rejects it via a Tauri command (see
/// `bin/tauri/commands.rs::confirm_pending_tool_call`). Never dispatched off
/// raw model output — see `chat_tools::is_mutation_tool`.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Everything needed to resume a suspended tool-calling loop once every
/// entry in `pending` has been resolved (approved or rejected) by the
/// operator. Opaque to callers other than `resume_after_confirmation` — the
/// Tauri layer holds this in `AppState` between the `send_message` call that
/// produced it and the `confirm_pending_tool_call` call(s) that resolve it.
#[derive(Debug, Clone)]
pub struct SuspendedToolLoop {
    settings: ChatSettings,
    tools: Vec<ModelToolSpec>,
    turns_so_far: Vec<ModelTurn>,
    /// Model round-trips already consumed by this exchange. Time spent
    /// waiting for operator confirmation does NOT advance this — the budget
    /// only counts actual calls to the model, so a slow human response can
    /// never starve the loop of its turn budget (issue #208 follow-up,
    /// requirement 5).
    turns_used: usize,
    max_turns: usize,
    pub pending: Vec<PendingToolCall>,
}

impl SuspendedToolLoop {
    /// The tool-call ids awaiting a resolution, in the order the model
    /// requested them.
    pub fn pending_ids(&self) -> Vec<&str> {
        self.pending.iter().map(|call| call.id.as_str()).collect()
    }

    /// The chat settings this loop was running under — needed by callers to
    /// rebuild an `AgentRuntime` to pass to `resume_after_confirmation`.
    pub fn settings(&self) -> &ChatSettings {
        &self.settings
    }
}

/// Result of running (or resuming) the tool-calling loop.
#[derive(Debug, Clone)]
pub enum ToolLoopOutcome {
    /// The model produced a final plain-text reply.
    Completed {
        events: Vec<ChatEvent>,
        final_text: String,
    },
    /// The turn budget was exhausted before a final reply.
    BudgetExhausted {
        events: Vec<ChatEvent>,
        final_text: String,
    },
    /// One or more mutation tool calls need operator confirmation before the
    /// loop can continue. Call `resume_after_confirmation` once every id in
    /// `suspended.pending` has a resolution.
    AwaitingConfirmation {
        events: Vec<ChatEvent>,
        suspended: SuspendedToolLoop,
    },
}

/// Runs the chat<->tool loop: send with the allowlisted tool schema, dispatch
/// any read-only `tool_calls` the model requests via `dispatch`, hold
/// mutation tool calls for operator confirmation (see [`ChatEvent::PendingConfirmation`]),
/// feed results back, and repeat until the model replies with plain text, the
/// loop suspends awaiting confirmation, or `max_turns` round-trips are
/// exhausted.
///
/// `runtime` is generic over [`AgentRuntime`] (rather than this function
/// constructing a `RigAgentRuntime` itself) and `dispatch` is injected
/// (rather than this module calling `chat_tools::dispatch_mcp_tool`
/// directly) so the whole loop — including the turn-budget cap and the
/// confirmation gate — is unit-testable with a fake backend, without a live
/// model endpoint or a `TurboLedgerService`/Tauri `AppState`. See
/// `bin/tauri/commands.rs` for the real wiring. Allowlist enforcement lives
/// in `dispatch`, not here: this loop calls whatever `dispatch` returns for
/// any read-only tool name the model asks for and feeds that back as the
/// tool result, same as a rejection. `dispatch` is never called for a
/// mutation tool name from this function — see `drive_tool_loop`.
pub fn send_message_with_tools<R: AgentRuntime>(
    runtime: &R,
    settings: &ChatSettings,
    history: &[ChatTurn],
    pending_message: &str,
    tools: &[ModelToolSpec],
    dispatch: &dyn Fn(&str, &serde_json::Value) -> serde_json::Value,
    max_turns: usize,
) -> Result<ToolLoopOutcome, ChatError> {
    let mut turns_so_far: Vec<ModelTurn> = history.iter().map(model_turn).collect();
    turns_so_far.push(ModelTurn {
        role: ModelRole::User,
        content: pending_message.to_string(),
        ..Default::default()
    });

    drive_tool_loop(
        runtime,
        settings,
        tools,
        turns_so_far,
        0,
        max_turns,
        dispatch,
        Vec::new(),
    )
}

/// Resumes a loop suspended by [`ToolLoopOutcome::AwaitingConfirmation`].
///
/// `resolutions` maps each pending call's id to the operator's decision
/// (`true` = approved). A pending id with no entry in `resolutions` is
/// treated as rejected — fail safe: a mutation call is only ever dispatched
/// on an explicit `true`, never on a missing/ambiguous answer. Approved
/// calls are dispatched via `dispatch` (the same injected dispatcher used
/// for read-only tools); rejected calls get a synthesized "operator
/// declined" tool result fed back to the model with the correct
/// `tool_call_id`, so the model can acknowledge it and continue rather than
/// the conversation just dying.
pub fn resume_after_confirmation<R: AgentRuntime>(
    runtime: &R,
    suspended: SuspendedToolLoop,
    resolutions: &std::collections::HashMap<String, bool>,
    dispatch: &dyn Fn(&str, &serde_json::Value) -> serde_json::Value,
) -> Result<ToolLoopOutcome, ChatError> {
    let SuspendedToolLoop {
        settings,
        tools,
        mut turns_so_far,
        turns_used,
        max_turns,
        pending,
    } = suspended;
    let mut events = Vec::new();

    for call in &pending {
        let approved = resolutions.get(&call.id).copied().unwrap_or(false);
        let result = if approved {
            events.push(ChatEvent::ToolCall {
                name: call.name.clone(),
                arguments: call.arguments.clone(),
            });
            let result = dispatch(&call.name, &call.arguments);
            events.push(ChatEvent::ToolResult {
                name: call.name.clone(),
                result: result.clone(),
            });
            result
        } else {
            let declined = serde_json::json!({
                "ok": false,
                "operator_declined": true,
                "error": format!(
                    "The operator reviewed this {} call and declined to approve it. \
                     Do not retry it verbatim; ask the operator what they'd like instead \
                     or continue without it.",
                    call.name
                )
            });
            events.push(ChatEvent::ToolResult {
                name: call.name.clone(),
                result: declined.clone(),
            });
            declined
        };
        let content = serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());
        turns_so_far.push(ModelTurn::tool_result(call.id.clone(), content));
    }

    drive_tool_loop(
        runtime,
        &settings,
        &tools,
        turns_so_far,
        turns_used,
        max_turns,
        dispatch,
        events,
    )
}

/// Shared core of `send_message_with_tools`/`resume_after_confirmation`:
/// repeatedly asks the model to continue from `turns_so_far` (whose last
/// entry is always the trailing/prompt turn — a fresh user message on the
/// very first call, a tool result on every call after) until it replies in
/// plain text, requests a mutation tool call (suspend), or the turn budget
/// (`turns_used` vs `max_turns`) is exhausted.
#[allow(clippy::too_many_arguments)]
fn drive_tool_loop<R: AgentRuntime>(
    runtime: &R,
    settings: &ChatSettings,
    tools: &[ModelToolSpec],
    mut turns_so_far: Vec<ModelTurn>,
    mut turns_used: usize,
    max_turns: usize,
    dispatch: &dyn Fn(&str, &serde_json::Value) -> serde_json::Value,
    mut events: Vec<ChatEvent>,
) -> Result<ToolLoopOutcome, ChatError> {
    loop {
        if turns_used >= max_turns {
            let exhausted_text = format!(
                "Tool budget exhausted after {max_turns} round-trip(s) without a final reply. \
                 Ask a narrower question, or increase the turn budget, and try again."
            );
            events.push(ChatEvent::Assistant(exhausted_text.clone()));
            return Ok(ToolLoopOutcome::BudgetExhausted {
                events,
                final_text: exhausted_text,
            });
        }

        let Some(trailing) = turns_so_far.last().cloned() else {
            // Unreachable: turns_so_far always has at least the seeded user
            // turn (send_message_with_tools) or a freshly pushed tool-result
            // turn (resume_after_confirmation). Fail safe rather than
            // index/unwrap if that invariant is ever broken.
            return Ok(ToolLoopOutcome::BudgetExhausted {
                events,
                final_text: String::new(),
            });
        };
        let history_for_request = turns_so_far[..turns_so_far.len() - 1].to_vec();
        let request = ModelRequest::continue_with(history_for_request, trailing)
            .with_system_prompt(settings.system_prompt.clone())
            .with_tools(tools.to_vec());

        let response = AgentRuntime::complete(runtime, request).map_err(ChatError::from)?;
        turns_used += 1;

        if response.tool_calls.is_empty() {
            events.push(ChatEvent::Assistant(response.assistant_text.clone()));
            return Ok(ToolLoopOutcome::Completed {
                events,
                final_text: response.assistant_text,
            });
        }

        turns_so_far.push(ModelTurn::assistant_tool_calls(response.tool_calls.clone()));

        let mut pending = Vec::new();
        for call in &response.tool_calls {
            if crate::chat_tools::is_mutation_tool(&call.name) {
                pending.push(PendingToolCall {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    arguments: call.arguments.clone(),
                });
                events.push(ChatEvent::PendingConfirmation {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    arguments: call.arguments.clone(),
                });
                continue;
            }

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

        if !pending.is_empty() {
            return Ok(ToolLoopOutcome::AwaitingConfirmation {
                events,
                suspended: SuspendedToolLoop {
                    settings: settings.clone(),
                    tools: tools.to_vec(),
                    turns_so_far,
                    turns_used,
                    max_turns,
                    pending,
                },
            });
        }
    }
}

/// Renders the human-readable confirmation card for a pending mutation tool
/// call — the transcript text and the detail shown in the Tauri-side popup
/// both come from this, so they can never drift apart. Tool-specific so the
/// operator sees the fields that actually matter for that action (tx_ids,
/// proposed category/confidence, resolution action, ...) without needing to
/// inspect logs — see issue #208's follow-up requirement 2.
pub fn pending_confirmation_card(name: &str, arguments: &serde_json::Value) -> String {
    fn as_str<'a>(arguments: &'a serde_json::Value, key: &str) -> &'a str {
        arguments
            .get(key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or("<missing>")
    }

    fn tx_ids(arguments: &serde_json::Value) -> Vec<String> {
        arguments
            .get("tx_ids")
            .and_then(serde_json::Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    match name {
        "classify_transaction" => {
            let mut lines = vec![
                "CONFIRMATION REQUIRED — classify_transaction".to_string(),
                format!("  transaction: {}", as_str(arguments, "tx_id")),
                format!("  proposed category: {}", as_str(arguments, "category")),
                format!("  confidence: {}", as_str(arguments, "confidence")),
            ];
            if let Some(note) = arguments.get("note").and_then(serde_json::Value::as_str) {
                lines.push(format!("  note: {note}"));
            }
            lines.push("Approve or reject before this classification is written.".to_string());
            lines.join("\n")
        }
        "batch_classify" => {
            let ids = tx_ids(arguments);
            let dry_run = arguments
                .get("dry_run")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let mut lines = vec![
                "CONFIRMATION REQUIRED — batch_classify".to_string(),
                format!("  {} transaction(s): {}", ids.len(), ids.join(", ")),
                format!(
                    "  proposed category (uniform across the batch): {}",
                    as_str(arguments, "category")
                ),
                format!("  confidence: {}", as_str(arguments, "confidence")),
                format!("  dry_run: {dry_run}"),
            ];
            if let Some(note) = arguments.get("note").and_then(serde_json::Value::as_str) {
                lines.push(format!("  note: {note}"));
            }
            lines.push(
                "Approve or reject before any of these classifications are written.".to_string(),
            );
            lines.join("\n")
        }
        "bulk_resolve_flags" => {
            let ids = tx_ids(arguments);
            let mut lines = vec![
                "CONFIRMATION REQUIRED — bulk_resolve_flags".to_string(),
                format!("  {} flagged transaction(s): {}", ids.len(), ids.join(", ")),
                format!("  resolution action: {}", as_str(arguments, "resolution")),
            ];
            if let Some(reason) = arguments.get("reason").and_then(serde_json::Value::as_str) {
                lines.push(format!("  reason: {reason}"));
            }
            lines.push("Approve or reject before these flags are resolved.".to_string());
            lines.join("\n")
        }
        other => format!(
            "CONFIRMATION REQUIRED — {other}\n  arguments: {arguments}\n\
             Approve or reject before this action runs."
        ),
    }
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
                ChatRole::PendingConfirmation => "CONFIRM",
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
        ChatRole::PendingConfirmation => "pending_confirmation",
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
            // Same reasoning as ChatRole::Tool above: a persisted
            // confirmation card is historical context by the time it's
            // replayed on a later message, not a live protocol turn.
            ChatRole::PendingConfirmation => ModelRole::User,
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
            ChatEvent::PendingConfirmation {
                name, arguments, ..
            } => Some(ChatTurn {
                role: ChatRole::PendingConfirmation,
                content: pending_confirmation_card(name, arguments),
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

        let ToolLoopOutcome::BudgetExhausted { events, final_text } = outcome else {
            panic!("expected BudgetExhausted, got {outcome:?}");
        };
        assert!(
            final_text.contains("Tool budget exhausted"),
            "budget exhaustion must be a visible transcript entry, not a silent truncation: {final_text}"
        );
        let tool_call_events = events
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

        let ToolLoopOutcome::BudgetExhausted { events, .. } = outcome else {
            panic!("expected BudgetExhausted, got {outcome:?}");
        };
        assert!(events
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

        let ToolLoopOutcome::Completed { events, final_text } = outcome else {
            panic!("expected Completed, got {outcome:?}");
        };
        assert_eq!(final_text, "understood, that tool is not available");
        assert!(events.iter().any(
            |event| matches!(event, ChatEvent::ToolResult { name, .. } if name == "reconcile_postings")
        ));
    }

    // ── Mutation-tool confirmation gate (issue #208 follow-up) ─────────────

    /// A fake backend that requests a `batch_classify` mutation call on its
    /// first invocation, then (if ever called again) asserts the resumed
    /// request's trailing tool-result content and replies in plain text.
    /// Counts total `complete()` calls so tests can assert the budget only
    /// advances on real model round-trips, never while waiting on a human.
    struct MutationCallThenTextRuntime {
        call_count: std::sync::atomic::AtomicUsize,
    }

    impl MutationCallThenTextRuntime {
        fn new() -> Self {
            Self {
                call_count: std::sync::atomic::AtomicUsize::new(0),
            }
        }

        fn calls_made(&self) -> usize {
            self.call_count.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    impl AgentRuntime for MutationCallThenTextRuntime {
        fn complete(&self, request: ModelRequest) -> Result<ModelResponse, AgentRuntimeError> {
            let call = self
                .call_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if call == 0 {
                Ok(ModelResponse {
                    assistant_text: String::new(),
                    tool_calls: vec![ModelToolCall {
                        id: "call_mut_1".to_string(),
                        name: "batch_classify".to_string(),
                        arguments: serde_json::json!({
                            "tx_ids": ["tx_1", "tx_2"],
                            "category": "Meals",
                            "confidence": "0.9",
                            "actor": "agent"
                        }),
                    }],
                })
            } else {
                let trailing = request
                    .trailing_turn
                    .expect("a continuation call must set trailing_turn");
                assert_eq!(trailing.tool_call_id.as_deref(), Some("call_mut_1"));
                Ok(ModelResponse::text(format!(
                    "acknowledged: {}",
                    trailing.content
                )))
            }
        }
    }

    fn panics_dispatch() -> impl Fn(&str, &serde_json::Value) -> serde_json::Value {
        |name: &str, _args: &serde_json::Value| {
            panic!("dispatch must never be called for a gated mutation tool call without operator approval: {name}")
        }
    }

    #[test]
    fn send_message_with_tools_suspends_on_a_mutation_tool_call_without_dispatching_it() {
        let runtime = MutationCallThenTextRuntime::new();
        let dispatch = panics_dispatch();

        let outcome = send_message_with_tools(
            &runtime,
            &test_settings(),
            &[],
            "classify these two transactions as Meals",
            &[],
            &dispatch,
            DEFAULT_MAX_TOOL_TURNS,
        )
        .expect("suspending is an Ok outcome, not an error");

        let ToolLoopOutcome::AwaitingConfirmation { events, suspended } = outcome else {
            panic!("expected AwaitingConfirmation, got {outcome:?}");
        };
        assert_eq!(suspended.pending.len(), 1);
        assert_eq!(suspended.pending[0].id, "call_mut_1");
        assert_eq!(suspended.pending[0].name, "batch_classify");
        assert_eq!(suspended.pending_ids(), vec!["call_mut_1"]);
        assert!(events.iter().any(|event| matches!(
            event,
            ChatEvent::PendingConfirmation { name, .. } if name == "batch_classify"
        )));
        // The gate must never have let dispatch run — panics_dispatch()
        // would have aborted the test above if it had.
        assert_eq!(runtime.calls_made(), 1);
    }

    #[test]
    fn resume_after_confirmation_dispatches_on_approval_with_matching_call_id() {
        let runtime = MutationCallThenTextRuntime::new();
        let outcome = send_message_with_tools(
            &runtime,
            &test_settings(),
            &[],
            "classify these two transactions as Meals",
            &[],
            &panics_dispatch(),
            DEFAULT_MAX_TOOL_TURNS,
        )
        .expect("suspends cleanly");
        let ToolLoopOutcome::AwaitingConfirmation { suspended, .. } = outcome else {
            panic!("expected AwaitingConfirmation");
        };

        let dispatch_calls = std::sync::Mutex::new(Vec::new());
        let dispatch = |name: &str, arguments: &serde_json::Value| -> serde_json::Value {
            dispatch_calls
                .lock()
                .unwrap()
                .push((name.to_string(), arguments.clone()));
            serde_json::json!({"isError": false, "content": "classified"})
        };

        let mut resolutions = std::collections::HashMap::new();
        resolutions.insert("call_mut_1".to_string(), true);

        let resumed = resume_after_confirmation(&runtime, suspended, &resolutions, &dispatch)
            .expect("resume completes once approved");

        let ToolLoopOutcome::Completed { events, final_text } = resumed else {
            panic!("expected Completed after approval");
        };
        assert!(final_text.contains("isError"), "final_text: {final_text}");
        assert_eq!(dispatch_calls.lock().unwrap().len(), 1);
        assert_eq!(dispatch_calls.lock().unwrap()[0].0, "batch_classify");
        assert!(events.iter().any(
            |event| matches!(event, ChatEvent::ToolCall { name, .. } if name == "batch_classify")
        ));
        assert!(events.iter().any(
            |event| matches!(event, ChatEvent::ToolResult { name, .. } if name == "batch_classify")
        ));
        // Exactly two model round-trips total: the one that produced the
        // pending call, and the one after resume — matches
        // DEFAULT_MAX_TOOL_TURNS-budget expectations for a single confirm.
        assert_eq!(runtime.calls_made(), 2);
    }

    #[test]
    fn resume_after_confirmation_synthesizes_a_decline_threaded_with_the_correct_call_id() {
        let runtime = MutationCallThenTextRuntime::new();
        let outcome = send_message_with_tools(
            &runtime,
            &test_settings(),
            &[],
            "classify these two transactions as Meals",
            &[],
            &panics_dispatch(),
            DEFAULT_MAX_TOOL_TURNS,
        )
        .expect("suspends cleanly");
        let ToolLoopOutcome::AwaitingConfirmation { suspended, .. } = outcome else {
            panic!("expected AwaitingConfirmation");
        };

        let mut resolutions = std::collections::HashMap::new();
        resolutions.insert("call_mut_1".to_string(), false);

        // dispatch must never be invoked on a rejection.
        let resumed =
            resume_after_confirmation(&runtime, suspended, &resolutions, &panics_dispatch())
                .expect("resume completes once rejected");

        let ToolLoopOutcome::Completed { events, final_text } = resumed else {
            panic!("expected Completed after rejection, got a different outcome");
        };
        // MutationCallThenTextRuntime's second call asserts
        // trailing.tool_call_id == "call_mut_1" itself; reaching here at all
        // proves that held.
        assert!(
            final_text.contains("operator_declined"),
            "the model must see that the operator declined: {final_text}"
        );
        let declined_result = events.iter().find_map(|event| match event {
            ChatEvent::ToolResult { name, result } if name == "batch_classify" => Some(result),
            _ => None,
        });
        let declined_result = declined_result.expect("a ToolResult event for the declined call");
        assert_eq!(
            declined_result["operator_declined"],
            serde_json::json!(true)
        );
        assert_eq!(declined_result["ok"], serde_json::json!(false));
    }

    #[test]
    fn resume_after_confirmation_treats_a_missing_resolution_as_rejected_fail_safe() {
        let runtime = MutationCallThenTextRuntime::new();
        let outcome = send_message_with_tools(
            &runtime,
            &test_settings(),
            &[],
            "classify these two transactions as Meals",
            &[],
            &panics_dispatch(),
            DEFAULT_MAX_TOOL_TURNS,
        )
        .expect("suspends cleanly");
        let ToolLoopOutcome::AwaitingConfirmation { suspended, .. } = outcome else {
            panic!("expected AwaitingConfirmation");
        };

        // Empty resolutions map: call_mut_1 has no entry at all.
        let resolutions = std::collections::HashMap::new();
        let resumed =
            resume_after_confirmation(&runtime, suspended, &resolutions, &panics_dispatch())
                .expect("resume completes even with no resolution recorded");

        let ToolLoopOutcome::Completed { final_text, .. } = resumed else {
            panic!("expected Completed");
        };
        assert!(final_text.contains("operator_declined"));
    }

    #[test]
    fn waiting_for_confirmation_does_not_consume_turn_budget() {
        // max_turns(1): the single model call that discovers the pending
        // mutation call is allowed, but the loop must NOT call the model
        // again just because a human took a while to respond — the budget
        // check happens before the next model call, and approval dispatch
        // itself is not a model call.
        let runtime = MutationCallThenTextRuntime::new();
        let outcome = send_message_with_tools(
            &runtime,
            &test_settings(),
            &[],
            "classify these two transactions as Meals",
            &[],
            &panics_dispatch(),
            1,
        )
        .expect("suspends cleanly even at a tight budget");
        let ToolLoopOutcome::AwaitingConfirmation { suspended, .. } = outcome else {
            panic!("expected AwaitingConfirmation");
        };
        assert_eq!(
            runtime.calls_made(),
            1,
            "one model call to discover the pending mutation"
        );

        let dispatched = std::sync::Mutex::new(false);
        let dispatch = |_name: &str, _args: &serde_json::Value| -> serde_json::Value {
            *dispatched.lock().unwrap() = true;
            serde_json::json!({"isError": false})
        };
        let mut resolutions = std::collections::HashMap::new();
        resolutions.insert("call_mut_1".to_string(), true);

        let resumed = resume_after_confirmation(&runtime, suspended, &resolutions, &dispatch)
            .expect("resume returns Ok even when the budget is already exhausted");

        // The approved call WAS dispatched (waiting for confirmation didn't
        // block that)...
        assert!(*dispatched.lock().unwrap());
        // ...but the already-exhausted budget (1 turn already used, cap is
        // 1) means no second model call happens after resuming.
        assert_eq!(
            runtime.calls_made(),
            1,
            "resume must not spend a model call once the budget is already used up"
        );
        assert!(matches!(resumed, ToolLoopOutcome::BudgetExhausted { .. }));
    }
}
