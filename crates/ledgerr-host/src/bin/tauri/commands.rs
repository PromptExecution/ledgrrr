use std::sync::Arc;

use tauri::Emitter;

use ledgerr_host::agent_runtime::RigAgentRuntime;
use ledgerr_host::chat::{
    assistant_decision_log, build_rig_prompt_preview, pending_confirmation_card,
    render_rig_exchange_log, render_transcript, resume_after_confirmation, rhai_rule_prompt_seed,
    rhai_rule_prompt_seed_log, send_message_with_tools, tool_event_chat_turns, user_request_log,
    ChatError, ChatEvent, ChatRole, ChatTurn, ReviewLog, RigPromptPreview, ToolLoopOutcome,
    DEFAULT_MAX_TOOL_TURNS, DEFAULT_RHAI_RULE_MODEL, RHAI_RULE_SYSTEM_PROMPT,
};
use ledgerr_host::chat_tools::{self, GET_EVIDENCE_DASHBOARD};
use ledgerr_host::evidence::{EvidenceState, TodayQueue};
use ledgerr_host::internal_openai::{
    cloud_chat_settings, docs_playbook_status, foundry_local_chat_settings, foundry_local_status,
    internal_phi_backend_status, internal_phi_chat_settings,
    start_default_internal_openai_endpoint, InternalOpenAiError, InternalOpenAiHandle,
    FOUNDRY_LOCAL_MODEL, INTERNAL_OPENAI_CHAT_URL,
};
use ledgerr_host::settings::ChatSettings;
use ledgerr_host::settings_client::SettingsClient;

use super::state::{AppState, PendingToolLoopSession};
use holon_viz::{Holon, HolonKind, TypeRelationshipGraph};

fn desktop_json<T: serde::Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| error.to_string())
}

/// Return the controller status as JSON so the webview and Claude MCPB read
/// one shared desktop/runtime contract without duplicating probes in JS.
#[tauri::command]
#[specta::specta]
pub fn get_desktop_status() -> Result<String, String> {
    desktop_json(&ledgerr_desktop_agent::status::collect())
}

#[tauri::command]
#[specta::specta]
pub fn start_desktop_runtime() -> Result<String, String> {
    desktop_json(&ledgerr_desktop_agent::service_control::start_service())
}

#[tauri::command]
#[specta::specta]
pub fn stop_desktop_runtime() -> Result<String, String> {
    desktop_json(&ledgerr_desktop_agent::service_control::stop_service())
}

/// Opens only the per-user runtime log directory; no controller action can
/// browse or alter arbitrary host paths.
#[tauri::command]
#[specta::specta]
pub fn open_desktop_logs() -> Result<String, String> {
    let path = ledgerr_desktop_agent::state::RuntimeConfig::per_user().log_dir;
    std::fs::create_dir_all(&path).map_err(|error| error.to_string())?;
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer.exe")
            .arg(&path)
            .spawn()
            .map_err(|error| error.to_string())?;
    }
    Ok(path)
}

/// UI repair remains plan-only. The user must approve the matching controller
/// operation in Claude, preserving the MCPB/UAC boundary.
#[tauri::command]
#[specta::specta]
pub fn get_desktop_repair_plan() -> Result<String, String> {
    desktop_json(&ledgerr_desktop_agent::install_plan::invoke(
        "repair",
        ledgerr_desktop_agent::install_plan::PackageActionArgs::default(),
    ))
}

#[tauri::command]
#[specta::specta]
pub fn get_foundry_local_install_plan() -> Result<String, String> {
    desktop_json(&ledgerr_desktop_agent::foundry_install_plan::install_plan())
}

#[tauri::command]
#[specta::specta]
pub fn foundry_local_install_action(approved: bool) -> Result<String, String> {
    desktop_json(&ledgerr_desktop_agent::foundry_install_plan::invoke(
        ledgerr_desktop_agent::foundry_install_plan::FoundryInstallActionArgs { approved },
    ))
}

// ── Test harness config ───────────────────────────────────────────────────────

#[derive(serde::Serialize, Clone, specta::Type)]
pub struct TestHarnessConfig {
    pub kill_delay_ms: u32,
    pub screenshot_path: String,
    pub pkg_version: String,
    pub build_number: String,
}

#[tauri::command]
#[specta::specta]
pub fn get_cargo_pkg_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
#[specta::specta]
pub fn write_dom_dump(dump: String) -> String {
    let path = std::env::temp_dir().join("host-tauri-dom-dump.txt");
    match std::fs::write(&path, &dump) {
        Ok(()) => format!("wrote {} bytes to {}", dump.len(), path.display()),
        Err(e) => format!("write error: {e}"),
    }
}

#[tauri::command]
#[specta::specta]
pub fn get_test_harness_config() -> TestHarnessConfig {
    let _ = std::fs::write(
        std::env::temp_dir().join("host-tauri-ipc-alive.txt"),
        format!("get_test_harness_config called\n"),
    );
    TestHarnessConfig {
        kill_delay_ms: std::env::var("TAURI_TEST_KILL_DELAY")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
        screenshot_path: std::env::var("TAURI_TEST_SCREENSHOT_PATH")
            .ok()
            .unwrap_or_default(),
        pkg_version: env!("CARGO_PKG_VERSION").to_string(),
        build_number: std::env::var("TAURI_BUILD_NUMBER").ok().unwrap_or_default(),
    }
}

// ── Shared payload types ──────────────────────────────────────────────────────

#[derive(serde::Serialize, Clone, specta::Type)]
pub struct InitialState {
    pub version_text: String,
    pub status_text: String,
    pub endpoint_text: String,
    pub model_text: String,
    pub api_key_text: String,
    pub system_prompt_text: String,
    pub transcript_text: String,
    pub review_log_text: String,
    pub rig_log_text: String,
    pub draft_message_text: String,
    pub docs_status_text: String,
}

#[derive(serde::Serialize, Clone, specta::Type)]
pub struct ChatSettingsPayload {
    pub endpoint_text: String,
    pub model_text: String,
    pub api_key_text: String,
    pub system_prompt_text: String,
    pub status_text: String,
}

#[derive(serde::Serialize, Clone, specta::Type)]
pub struct RhaiPromptPayload {
    pub system_prompt: String,
    /// Non-empty when the caller should switch to this model (e.g. DEFAULT_RHAI_RULE_MODEL)
    pub suggested_model: String,
    pub draft_message: String,
    pub review_log_text: String,
    pub status: String,
}

#[derive(serde::Serialize, Clone, specta::Type)]
pub struct ChatUpdateEvent {
    pub transcript_text: String,
    pub review_log_text: Option<String>,
    pub rig_log_text: String,
    pub draft_message_text: String,
    pub status_text: String,
    pub busy: bool,
}

// ── Helper: ensure internal Phi endpoint is running ──────────────────────────

pub fn ensure_internal_endpoint(
    internal_endpoint: &Arc<std::sync::Mutex<Option<InternalOpenAiHandle>>>,
) -> Result<String, String> {
    let mut endpoint = internal_endpoint
        .lock()
        .map_err(|_| "internal endpoint state is poisoned".to_string())?;

    if endpoint.is_some() {
        return Ok("Internal endpoint already running.".to_string());
    }

    match start_default_internal_openai_endpoint() {
        Ok(handle) => {
            *endpoint = Some(handle);
            Ok("Started internal endpoint at http://127.0.0.1:15115.".to_string())
        }
        Err(InternalOpenAiError::Bind { source, .. })
            if source.kind() == std::io::ErrorKind::AddrInUse =>
        {
            Ok("Internal endpoint port is already in use; reusing localhost:15115.".to_string())
        }
        Err(error) => Err(error.to_string()),
    }
}

// ── Commands ──────────────────────────────────────────────────────────────────

#[tauri::command]
#[specta::specta]
pub fn get_initial_state(state: tauri::State<'_, AppState>) -> Result<InitialState, String> {
    let mut settings = state.store.load().map_err(|e| e.to_string())?;

    if settings.chat.model.trim().is_empty() || settings.chat.api_key.trim().is_empty() {
        settings.chat = internal_phi_chat_settings(settings.chat.system_prompt.clone());
    }

    let status_text = "Editing settings via ledgrrr-service".to_string();

    Ok(InitialState {
        version_text: format!("Version {}", env!("CARGO_PKG_VERSION")),
        status_text,
        endpoint_text: settings.chat.endpoint_url.clone(),
        model_text: settings.chat.model.clone(),
        api_key_text: settings.chat.api_key.clone(),
        system_prompt_text: settings.chat.system_prompt.clone(),
        transcript_text:
            "Tool tray chat is ready.\n\nSave the endpoint, model, and API key, then send a message."
                .to_string(),
        review_log_text: "No review log entries yet.".to_string(),
        rig_log_text: format!("No request sent yet.\n\n{}", internal_phi_backend_status()),
        draft_message_text: rhai_rule_prompt_seed().to_string(),
        docs_status_text: docs_playbook_status(),
    })
}

#[tauri::command]
#[specta::specta]
pub fn save_settings(
    endpoint: String,
    model: String,
    api_key: String,
    system_prompt: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let mut settings = state.store.load().map_err(|e| e.to_string())?;

    settings.chat = ChatSettings {
        endpoint_url: endpoint.trim().to_string(),
        model: model.trim().to_string(),
        api_key: api_key.trim().to_string(),
        system_prompt: system_prompt.trim().to_string(),
    };

    state.store.save(&settings).map_err(|e| e.to_string())?;

    Ok("Saved chat settings via ledgrrr-service".to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn send_message(
    window: tauri::Window,
    draft: String,
    endpoint: String,
    model: String,
    api_key: String,
    system_prompt: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    if draft.trim().is_empty() {
        return Err("Enter a message before sending.".to_string());
    }

    {
        let guard = state
            .pending_tool_loop
            .lock()
            .map_err(|_| "pending tool loop lock poisoned".to_string())?;
        if guard.is_some() {
            return Err(
                "A previous tool call is still awaiting your approval — approve or reject it \
                 before sending another message."
                    .to_string(),
            );
        }
    }

    let mut settings = state.store.load().map_err(|e| e.to_string())?;

    settings.chat = ChatSettings {
        endpoint_url: endpoint.trim().to_string(),
        model: model.trim().to_string(),
        api_key: api_key.trim().to_string(),
        system_prompt: system_prompt.trim().to_string(),
    };

    if settings.chat.endpoint_url.trim() == INTERNAL_OPENAI_CHAT_URL {
        ensure_internal_endpoint(&state.internal_endpoint)?;
    }

    state.store.save(&settings).map_err(|e| e.to_string())?;

    let user_turn = ChatTurn {
        role: ChatRole::User,
        content: draft.trim().to_string(),
    };
    let previous_rhai = user_turn.content.clone();

    let history_snapshot = {
        let mut history = state
            .history
            .lock()
            .map_err(|_| "lock poisoned".to_string())?;
        history.push(user_turn.clone());
        history.clone()
    };

    {
        let mut review_log = state
            .review_log
            .lock()
            .map_err(|_| "lock poisoned".to_string())?;
        review_log.push(user_request_log(&user_turn.content));
    }

    let request_preview = build_rig_prompt_preview(
        &settings.chat,
        &history_snapshot[..history_snapshot.len() - 1],
        &user_turn.content,
    );
    let backend_status = internal_phi_backend_status();
    let sending_status = format!(
        "Sending to {} with model {}",
        settings.chat.endpoint_url, settings.chat.model
    );

    // Emit a busy=true update immediately so the frontend can disable input
    let _ = window.emit(
        "chat-update",
        ChatUpdateEvent {
            transcript_text: render_transcript(&history_snapshot),
            review_log_text: Some(
                state
                    .review_log
                    .lock()
                    .map(|rl| rl.render())
                    .unwrap_or_default(),
            ),
            rig_log_text: render_rig_exchange_log(&request_preview, &backend_status, None, None),
            draft_message_text: draft.clone(),
            status_text: sending_status.clone(),
            busy: true,
        },
    );

    // Clone Arc handles for the blocking task
    let history_arc = Arc::clone(&state.history);
    let review_log_arc = Arc::clone(&state.review_log);
    let evidence_arc = Arc::clone(&state.evidence);
    let store_arc = Arc::clone(&state.store);
    let pending_tool_loop_arc = Arc::clone(&state.pending_tool_loop);
    let chat_settings = settings.chat.clone();
    // history_snapshot already excludes the current turn for the context window;
    // the current turn was appended above, so pass all but last.
    let context_len = history_snapshot.len().saturating_sub(1);
    let context = history_snapshot[..context_len].to_vec();
    let user_content = user_turn.content.clone();
    let request_preview_clone = request_preview.clone();
    let backend_status_clone = backend_status.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let runtime = RigAgentRuntime::new(chat_settings.clone());
        let tool_specs = chat_tools::tool_specs();
        let dispatch = build_dispatch_fn(evidence_arc, store_arc);
        let result = send_message_with_tools(
            &runtime,
            &chat_settings,
            &context,
            &user_content,
            &tool_specs,
            &dispatch,
            DEFAULT_MAX_TOOL_TURNS,
        );

        apply_tool_loop_outcome(
            &window,
            &history_arc,
            &review_log_arc,
            &pending_tool_loop_arc,
            &previous_rhai,
            &request_preview_clone,
            &backend_status_clone,
            result,
            draft,
        );
    });

    Ok(sending_status)
}

/// Records the operator's approve/reject decision for one pending mutation
/// tool call (see issue #208's follow-up). Once every pending id from the
/// same round has a resolution, resumes the suspended tool-calling loop —
/// dispatching approved calls via the same `chat_tools::dispatch_mcp_tool`
/// path as any other tool call, and synthesizing a declined-tool-result for
/// rejected ones — and emits the same `chat-update` event `send_message`
/// does. Until every pending id is resolved, only records the decision and
/// returns a short status string; nothing is dispatched early.
#[tauri::command]
#[specta::specta]
pub async fn confirm_pending_tool_call(
    window: tauri::Window,
    call_id: String,
    approved: bool,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let all_resolved = {
        let mut guard = state
            .pending_tool_loop
            .lock()
            .map_err(|_| "pending tool loop lock poisoned".to_string())?;
        let session = guard
            .as_mut()
            .ok_or_else(|| "no chat tool call is awaiting confirmation".to_string())?;
        if !session
            .suspended
            .pending_ids()
            .iter()
            .any(|id| *id == call_id)
        {
            return Err(format!(
                "'{call_id}' is not one of the tool calls currently awaiting confirmation"
            ));
        }
        session.resolutions.insert(call_id.clone(), approved);
        session
            .suspended
            .pending_ids()
            .iter()
            .all(|id| session.resolutions.contains_key(*id))
    };

    if !all_resolved {
        return Ok(format!(
            "Recorded {} for {call_id}; waiting on the remaining pending confirmation(s) before continuing.",
            if approved { "approval" } else { "rejection" }
        ));
    }

    let session = {
        let mut guard = state
            .pending_tool_loop
            .lock()
            .map_err(|_| "pending tool loop lock poisoned".to_string())?;
        guard
            .take()
            .ok_or_else(|| "no chat tool call is awaiting confirmation".to_string())?
    };

    let history_arc = Arc::clone(&state.history);
    let review_log_arc = Arc::clone(&state.review_log);
    let evidence_arc = Arc::clone(&state.evidence);
    let store_arc = Arc::clone(&state.store);
    let pending_tool_loop_arc = Arc::clone(&state.pending_tool_loop);
    let chat_settings = session.suspended.settings().clone();
    let request_preview = RigPromptPreview {
        endpoint_url: chat_settings.endpoint_url.clone(),
        model: chat_settings.model.clone(),
        messages_json: format!(
            "(resumed after operator confirmation of {} pending tool call(s))",
            session.resolutions.len()
        ),
    };
    let backend_status = internal_phi_backend_status();

    tauri::async_runtime::spawn_blocking(move || {
        let runtime = RigAgentRuntime::new(chat_settings);
        let dispatch = build_dispatch_fn(evidence_arc, store_arc);
        let result =
            resume_after_confirmation(&runtime, session.suspended, &session.resolutions, &dispatch);

        apply_tool_loop_outcome(
            &window,
            &history_arc,
            &review_log_arc,
            &pending_tool_loop_arc,
            "",
            &request_preview,
            &backend_status,
            result,
            String::new(),
        );
    });

    Ok("All pending confirmations resolved; resuming the tool-calling loop.".to_string())
}

/// Builds the tool dispatcher shared by `send_message` and
/// `confirm_pending_tool_call`: `get_evidence_dashboard` is Tauri-state-bound
/// (needs `AppState.evidence`/`AppState.store`) so it's special-cased here;
/// every other allowlisted tool goes through `chat_tools::dispatch_mcp_tool`.
fn build_dispatch_fn(
    evidence_arc: Arc<std::sync::Mutex<EvidenceState>>,
    store_arc: Arc<SettingsClient>,
) -> impl Fn(&str, &serde_json::Value) -> serde_json::Value {
    move |name: &str, arguments: &serde_json::Value| -> serde_json::Value {
        if name == GET_EVIDENCE_DASHBOARD {
            dispatch_get_evidence_dashboard(&evidence_arc, &store_arc)
        } else {
            chat_tools::dispatch_mcp_tool(name, arguments)
        }
    }
}

/// Shared `chat-update`-emitting outcome handler for both `send_message` and
/// `confirm_pending_tool_call`. `previous_rhai` and `draft_on_error` are only
/// meaningful for the `send_message` path (a resumed call has no new user
/// message and no draft to restore on failure) — pass `""`/`String::new()`
/// from `confirm_pending_tool_call`.
#[allow(clippy::too_many_arguments)]
fn apply_tool_loop_outcome(
    window: &tauri::Window,
    history_arc: &Arc<std::sync::Mutex<Vec<ChatTurn>>>,
    review_log_arc: &Arc<std::sync::Mutex<ReviewLog>>,
    pending_tool_loop_arc: &Arc<std::sync::Mutex<Option<PendingToolLoopSession>>>,
    previous_rhai: &str,
    request_preview: &RigPromptPreview,
    backend_status: &str,
    result: Result<ToolLoopOutcome, ChatError>,
    draft_on_error: String,
) {
    match result {
        Ok(ToolLoopOutcome::Completed { events, final_text }) => finish_tool_loop(
            window,
            history_arc,
            review_log_arc,
            previous_rhai,
            request_preview,
            backend_status,
            events,
            final_text,
            "Remote chat response received.".to_string(),
        ),
        Ok(ToolLoopOutcome::BudgetExhausted { events, final_text }) => {
            let status_text = format!(
                "Chat tool budget exhausted after {DEFAULT_MAX_TOOL_TURNS} round-trip(s) — see the transcript for the last tool result."
            );
            finish_tool_loop(
                window,
                history_arc,
                review_log_arc,
                previous_rhai,
                request_preview,
                backend_status,
                events,
                final_text,
                status_text,
            );
        }
        Ok(ToolLoopOutcome::AwaitingConfirmation { events, suspended }) => {
            let pending_payloads: Vec<PendingConfirmationPayload> = suspended
                .pending
                .iter()
                .map(|call| PendingConfirmationPayload {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    arguments_json: serde_json::to_string(&call.arguments)
                        .unwrap_or_else(|_| "{}".to_string()),
                    card_text: pending_confirmation_card(&call.name, &call.arguments),
                })
                .collect();

            let transcript = {
                match history_arc.lock() {
                    Ok(mut h) => {
                        h.extend(tool_event_chat_turns(&events));
                        render_transcript(&h)
                    }
                    Err(_) => "history poisoned".to_string(),
                }
            };
            let rig_log = render_rig_exchange_log(request_preview, backend_status, None, None);

            if let Ok(mut guard) = pending_tool_loop_arc.lock() {
                *guard = Some(PendingToolLoopSession {
                    suspended,
                    resolutions: std::collections::HashMap::new(),
                });
            }

            let status_text = format!(
                "{} action(s) need your approval before I can continue — see the transcript.",
                pending_payloads.len()
            );

            let _ = window.emit(
                "chat-update",
                ChatUpdateEvent {
                    transcript_text: transcript,
                    review_log_text: None,
                    rig_log_text: rig_log,
                    draft_message_text: String::new(),
                    status_text,
                    busy: true,
                },
            );
            let _ = window.emit(
                "tool-confirmation-required",
                ToolConfirmationRequiredEvent {
                    pending: pending_payloads,
                },
            );
        }
        Err(error) => {
            let transcript = {
                match history_arc.lock() {
                    Ok(h) => render_transcript(&h),
                    Err(_) => "history poisoned".to_string(),
                }
            };
            let rig_log = render_rig_exchange_log(
                request_preview,
                backend_status,
                None,
                Some(&error.to_string()),
            );
            let _ = window.emit(
                "chat-update",
                ChatUpdateEvent {
                    transcript_text: transcript,
                    review_log_text: None,
                    rig_log_text: rig_log,
                    draft_message_text: draft_on_error,
                    status_text: format!("Chat request failed: {error}"),
                    busy: false,
                },
            );
        }
    }
}

/// Finishes a tool loop that ended in either a final reply or a budget
/// exhaustion — the two cases share everything except the status text.
#[allow(clippy::too_many_arguments)]
fn finish_tool_loop(
    window: &tauri::Window,
    history_arc: &Arc<std::sync::Mutex<Vec<ChatTurn>>>,
    review_log_arc: &Arc<std::sync::Mutex<ReviewLog>>,
    previous_rhai: &str,
    request_preview: &RigPromptPreview,
    backend_status: &str,
    events: Vec<ChatEvent>,
    final_text: String,
    status_text: String,
) {
    let review_text = {
        match review_log_arc.lock() {
            Ok(mut rl) => {
                rl.push(assistant_decision_log(previous_rhai, &final_text));
                rl.render()
            }
            Err(_) => "review log poisoned".to_string(),
        }
    };

    let rig_log = render_rig_exchange_log(request_preview, backend_status, Some(&final_text), None);

    let transcript = {
        match history_arc.lock() {
            Ok(mut h) => {
                h.extend(tool_event_chat_turns(&events));
                h.push(ChatTurn {
                    role: ChatRole::Assistant,
                    content: final_text,
                });
                render_transcript(&h)
            }
            Err(_) => "history poisoned".to_string(),
        }
    };

    let _ = window.emit(
        "chat-update",
        ChatUpdateEvent {
            transcript_text: transcript,
            review_log_text: Some(review_text),
            rig_log_text: rig_log,
            draft_message_text: String::new(),
            status_text,
            busy: false,
        },
    );
}

/// Payload for one pending mutation tool call, sent to the frontend via the
/// `tool-confirmation-required` event so it can render a blocking popup
/// without needing to inspect logs — see issue #208's follow-up.
#[derive(serde::Serialize, Clone, specta::Type)]
pub struct PendingConfirmationPayload {
    pub id: String,
    pub name: String,
    pub arguments_json: String,
    pub card_text: String,
}

#[derive(serde::Serialize, Clone, specta::Type)]
pub struct ToolConfirmationRequiredEvent {
    pub pending: Vec<PendingConfirmationPayload>,
}

#[tauri::command]
#[specta::specta]
pub fn load_rhai_rule_prompt(
    current_model: String,
    current_system_prompt: String,
    state: tauri::State<'_, AppState>,
) -> Result<RhaiPromptPayload, String> {
    let entry = rhai_rule_prompt_seed_log(&current_model, &current_system_prompt);

    let review_log_text = {
        let mut review_log = state
            .review_log
            .lock()
            .map_err(|_| "lock poisoned".to_string())?;
        review_log.push(entry);
        review_log.render()
    };

    // Suggest the default Rhai rule model if the caller has no model set yet
    let suggested_model = if current_model.trim().is_empty() {
        DEFAULT_RHAI_RULE_MODEL.to_string()
    } else {
        String::new()
    };

    Ok(RhaiPromptPayload {
        system_prompt: RHAI_RULE_SYSTEM_PROMPT.to_string(),
        suggested_model,
        draft_message: rhai_rule_prompt_seed().to_string(),
        review_log_text,
        status:
            "Loaded a Rhai rule mutation prompt seed. Edit it, then send through the configured model."
                .to_string(),
    })
}

#[tauri::command]
#[specta::specta]
pub fn use_internal_phi(
    system_prompt: String,
    state: tauri::State<'_, AppState>,
) -> Result<ChatSettingsPayload, String> {
    let status = ensure_internal_endpoint(&state.internal_endpoint)?;
    let chat = internal_phi_chat_settings(system_prompt);
    let rig_status = internal_phi_backend_status();

    Ok(ChatSettingsPayload {
        endpoint_text: chat.endpoint_url,
        model_text: chat.model,
        api_key_text: chat.api_key,
        system_prompt_text: chat.system_prompt,
        status_text: format!("{status} Chat is set to internal Phi-4. {rig_status}"),
    })
}

#[tauri::command]
#[specta::specta]
pub fn use_foundry_local(system_prompt: String) -> Result<ChatSettingsPayload, String> {
    let chat = foundry_local_chat_settings(system_prompt)?;
    let rig_status = foundry_local_status();

    Ok(ChatSettingsPayload {
        endpoint_text: chat.endpoint_url,
        model_text: chat.model,
        api_key_text: chat.api_key,
        system_prompt_text: chat.system_prompt,
        status_text: format!(
            "Chat is set to Windows AI / Foundry Local with model {FOUNDRY_LOCAL_MODEL}. {rig_status}"
        ),
    })
}

#[tauri::command]
#[specta::specta]
pub fn use_cloud_model(system_prompt: String) -> Result<ChatSettingsPayload, String> {
    let chat = cloud_chat_settings(system_prompt);

    Ok(ChatSettingsPayload {
        endpoint_text: chat.endpoint_url,
        model_text: chat.model,
        api_key_text: chat.api_key,
        system_prompt_text: chat.system_prompt,
        status_text:
            "Chat is set to a cloud OpenAI-compatible endpoint. Enter model and API key before sending."
                .to_string(),
    })
}

#[tauri::command]
#[specta::specta]
pub fn open_docs_playbook(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    use tauri_plugin_opener::OpenerExt;

    let url = ledgerr_host::internal_openai::INTERNAL_DOCS_URL;

    let endpoint_status = match ensure_internal_endpoint(&state.internal_endpoint) {
        Ok(s) => s,
        Err(e) => format!("Warning: could not start internal server: {e}"),
    };

    // WSL2: xdg-open spawns but no browser appears — skip the attempt and
    // surface the URL directly so the user can paste it into a Windows browser.
    let is_wsl = std::env::var("WSL_DISTRO_NAME").is_ok()
        || std::env::var("WSLENV").is_ok()
        || std::fs::read_to_string("/proc/version")
            .map(|v| v.to_lowercase().contains("microsoft"))
            .unwrap_or(false);

    if is_wsl {
        return Ok(format!(
            "{endpoint_status} Running in WSL — open manually: {url}"
        ));
    }

    match app.opener().open_url(url, None::<&str>) {
        Ok(()) => Ok(format!("{endpoint_status} Opened {url} in the browser.")),
        Err(e) => Ok(format!(
            "{endpoint_status} Could not open browser ({e}) — open manually: {url}"
        )),
    }
}

// ── Evidence dashboard ────────────────────────────────────────────────────────

#[derive(serde::Serialize, Clone, specta::Type)]
pub struct EvidenceDashboardPayload {
    pub today_queue: ledgerr_host::evidence::TodayQueue,
}

#[tauri::command]
#[specta::specta]
pub fn get_evidence_dashboard(
    state: tauri::State<'_, AppState>,
) -> Result<EvidenceDashboardPayload, String> {
    let today_queue = evidence_dashboard_today_queue(&state.evidence, &state.store)?;
    Ok(EvidenceDashboardPayload { today_queue })
}

/// Shared logic behind both the `get_evidence_dashboard` Tauri command and
/// the chat tool loop's `get_evidence_dashboard` tool call — see
/// `dispatch_get_evidence_dashboard` below.
fn evidence_dashboard_today_queue(
    evidence: &Arc<std::sync::Mutex<EvidenceState>>,
    store: &Arc<SettingsClient>,
) -> Result<TodayQueue, String> {
    let settings = store.load().map_err(|e| e.to_string())?;
    let mut evidence = evidence
        .lock()
        .map_err(|_| "evidence lock poisoned".to_string())?;
    evidence.refresh_gaps();
    Ok(TodayQueue::from_state(&evidence, &settings))
}

/// `get_evidence_dashboard` is Tauri-state-bound (it lives on
/// `AppState.evidence`/`AppState.store`), so it is dispatched here rather
/// than through `chat_tools::dispatch_mcp_tool` — see that module's doc
/// comment. Never panics: errors are folded into the `{"ok": false, ...}`
/// envelope so the tool loop always has *something* to feed back to the
/// model as the tool result.
fn dispatch_get_evidence_dashboard(
    evidence: &Arc<std::sync::Mutex<EvidenceState>>,
    store: &Arc<SettingsClient>,
) -> serde_json::Value {
    match evidence_dashboard_today_queue(evidence, store) {
        Ok(today_queue) => match serde_json::to_value(&today_queue) {
            Ok(value) => serde_json::json!({ "today_queue": value }),
            Err(error) => serde_json::json!({ "ok": false, "error": error.to_string() }),
        },
        Err(error) => serde_json::json!({ "ok": false, "error": error }),
    }
}

#[derive(serde::Serialize, Clone, specta::Type)]
pub struct ProvenancePayload {
    pub badge: String,
    pub css_class: String,
}

#[tauri::command]
#[specta::specta]
pub fn get_tx_provenance(
    tx_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<ProvenancePayload, String> {
    let evidence = state
        .evidence
        .lock()
        .map_err(|_| "evidence lock poisoned".to_string())?;
    let badge = evidence.provenance_badge(&tx_id);
    Ok(ProvenancePayload {
        badge: badge.label().to_string(),
        css_class: badge.css_class().to_string(),
    })
}

/// Return the Cytoscape.js-compatible graph for the holonic pipeline.
///
/// The frontend Viz panel calls this once on activation. The graph crosses the
/// webview boundary as JSON so the model-first visualization types do not gain
/// a Rust-first `specta` dependency.
#[tauri::command]
#[specta::specta]
pub fn get_holon_viz_graph() -> Result<String, String> {
    use std::collections::HashMap;

    let holons = vec![
        Holon {
            id: "pipeline".into(),
            label: "Tax Ledger Pipeline".into(),
            kind: HolonKind::CapsuleGroup,
            parent_id: None,
            children: vec![
                "ingest".into(),
                "classify".into(),
                "reconcile".into(),
                "attest".into(),
            ],
            metadata: HashMap::new(),
        },
        Holon {
            id: "ingest".into(),
            label: "Ingest PDFs".into(),
            kind: HolonKind::SysmlBlock,
            parent_id: Some("pipeline".into()),
            children: vec!["docling".into(), "blake3-id".into()],
            metadata: HashMap::new(),
        },
        Holon {
            id: "classify".into(),
            label: "Classify Transactions".into(),
            kind: HolonKind::SysmlBlock,
            parent_id: Some("pipeline".into()),
            children: vec!["rhai-rules".into(), "flag-queue".into()],
            metadata: HashMap::new(),
        },
        Holon {
            id: "reconcile".into(),
            label: "Reconcile & Export".into(),
            kind: HolonKind::SysmlBlock,
            parent_id: Some("pipeline".into()),
            children: vec!["excel-workbook".into()],
            metadata: HashMap::new(),
        },
        Holon {
            id: "attest".into(),
            label: "Attest (CPA)".into(),
            kind: HolonKind::SysmlBlock,
            parent_id: Some("pipeline".into()),
            children: vec!["audit-log".into()],
            metadata: HashMap::new(),
        },
        Holon {
            id: "docling".into(),
            label: "Docling OCR".into(),
            kind: HolonKind::ProcessNode,
            parent_id: Some("ingest".into()),
            children: vec![],
            metadata: HashMap::new(),
        },
        Holon {
            id: "blake3-id".into(),
            label: "Blake3 Content ID".into(),
            kind: HolonKind::ProcessNode,
            parent_id: Some("ingest".into()),
            children: vec![],
            metadata: HashMap::new(),
        },
        Holon {
            id: "rhai-rules".into(),
            label: "Rhai Rule Engine".into(),
            kind: HolonKind::ProcessNode,
            parent_id: Some("classify".into()),
            children: vec![],
            metadata: HashMap::new(),
        },
        Holon {
            id: "flag-queue".into(),
            label: "Flag Queue".into(),
            kind: HolonKind::ProcessNode,
            parent_id: Some("classify".into()),
            children: vec![],
            metadata: HashMap::new(),
        },
        Holon {
            id: "excel-workbook".into(),
            label: "Excel Workbook".into(),
            kind: HolonKind::OwlClass,
            parent_id: Some("reconcile".into()),
            children: vec![],
            metadata: HashMap::new(),
        },
        Holon {
            id: "audit-log".into(),
            label: "Immutable Audit Log".into(),
            kind: HolonKind::AuditEvent,
            parent_id: Some("attest".into()),
            children: vec![],
            metadata: HashMap::new(),
        },
    ];

    desktop_json(&holon_viz::CytoscapeGraph::from_holons(&holons))
}

/// Return the Rust type relationship graph for the Viz panel.
///
/// Delegates to [`TypeRelationshipGraph::seed()`] in `holon-viz`.
#[tauri::command]
#[specta::specta]
pub fn get_type_graph() -> Result<String, String> {
    desktop_json(&TypeRelationshipGraph::seed().to_cytoscape())
}
