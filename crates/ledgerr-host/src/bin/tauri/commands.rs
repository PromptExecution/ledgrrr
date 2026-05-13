use std::sync::Arc;

use tauri::Emitter;

use ledgerr_host::chat::{
    assistant_decision_log, build_rig_prompt_preview, render_rig_exchange_log, render_transcript,
    rhai_rule_prompt_seed, rhai_rule_prompt_seed_log, send_chat_message, user_request_log,
    ChatRole, ChatTurn, DEFAULT_RHAI_RULE_MODEL, RHAI_RULE_SYSTEM_PROMPT,
};
use ledgerr_host::internal_openai::{
    cloud_chat_settings, docs_playbook_status, foundry_local_chat_settings, foundry_local_status,
    internal_phi_backend_status, internal_phi_chat_settings,
    start_default_internal_openai_endpoint, InternalOpenAiError, InternalOpenAiHandle,
    FOUNDRY_LOCAL_MODEL, INTERNAL_OPENAI_CHAT_URL,
};
use ledgerr_host::settings::ChatSettings;

use super::state::AppState;

// ── Test harness config ───────────────────────────────────────────────────────

#[derive(serde::Serialize, Clone)]
pub struct TestHarnessConfig {
    pub kill_delay_ms: u64,
    pub screenshot_path: String,
    pub pkg_version: String,
    pub build_number: String,
}

#[tauri::command]
pub fn get_cargo_pkg_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
pub fn write_dom_dump(dump: String) -> String {
    let path = std::env::temp_dir().join("host-tauri-dom-dump.txt");
    match std::fs::write(&path, &dump) {
        Ok(()) => format!("wrote {} bytes to {}", dump.len(), path.display()),
        Err(e) => format!("write error: {e}"),
    }
}

#[tauri::command]
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
        build_number: std::env::var("TAURI_BUILD_NUMBER")
            .ok()
            .unwrap_or_default(),
    }
}

// ── Shared payload types ──────────────────────────────────────────────────────

#[derive(serde::Serialize, Clone)]
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

#[derive(serde::Serialize, Clone)]
pub struct ChatSettingsPayload {
    pub endpoint_text: String,
    pub model_text: String,
    pub api_key_text: String,
    pub system_prompt_text: String,
    pub status_text: String,
}

#[derive(serde::Serialize, Clone)]
pub struct RhaiPromptPayload {
    pub system_prompt: String,
    /// Non-empty when the caller should switch to this model (e.g. DEFAULT_RHAI_RULE_MODEL)
    pub suggested_model: String,
    pub draft_message: String,
    pub review_log_text: String,
    pub status: String,
}

#[derive(serde::Serialize, Clone)]
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
pub fn get_initial_state(state: tauri::State<'_, AppState>) -> Result<InitialState, String> {
    let mut settings = state.store.load().map_err(|e| e.to_string())?;

    if settings.chat.model.trim().is_empty() || settings.chat.api_key.trim().is_empty() {
        settings.chat = internal_phi_chat_settings(settings.chat.system_prompt.clone());
    }

    let status_text = format!("Editing {}", state.store.path().display());

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

    Ok(format!(
        "Saved chat settings to {}",
        state.store.path().display()
    ))
}

#[tauri::command]
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
    let chat_settings = settings.chat.clone();
    // history_snapshot already excludes the current turn for the context window;
    // the current turn was appended above, so pass all but last.
    let context_len = history_snapshot.len().saturating_sub(1);
    let context = history_snapshot[..context_len].to_vec();
    let user_content = user_turn.content.clone();
    let request_preview_clone = request_preview.clone();
    let backend_status_clone = backend_status.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let result = send_chat_message(&chat_settings, &context, &user_content);

        match result {
            Ok(response) => {
                let review_text = {
                    match review_log_arc.lock() {
                        Ok(mut rl) => {
                            rl.push(assistant_decision_log(&previous_rhai, &response));
                            rl.render()
                        }
                        Err(_) => "review log poisoned".to_string(),
                    }
                };

                let rig_log = render_rig_exchange_log(
                    &request_preview_clone,
                    &backend_status_clone,
                    Some(&response),
                    None,
                );

                let transcript = {
                    match history_arc.lock() {
                        Ok(mut h) => {
                            h.push(ChatTurn {
                                role: ChatRole::Assistant,
                                content: response,
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
                        status_text: "Remote chat response received.".to_string(),
                        busy: false,
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
                    &request_preview_clone,
                    &backend_status_clone,
                    None,
                    Some(&error.to_string()),
                );
                let _ = window.emit(
                    "chat-update",
                    ChatUpdateEvent {
                        transcript_text: transcript,
                        review_log_text: None,
                        rig_log_text: rig_log,
                        draft_message_text: draft,
                        status_text: format!("Chat request failed: {error}"),
                        busy: false,
                    },
                );
            }
        }
    });

    Ok(sending_status)
}

#[tauri::command]
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

#[derive(serde::Serialize, Clone)]
pub struct EvidenceDashboardPayload {
    pub today_queue: ledgerr_host::evidence::TodayQueue,
}

#[tauri::command]
pub fn get_evidence_dashboard(
    state: tauri::State<'_, AppState>,
) -> Result<EvidenceDashboardPayload, String> {
    let settings = state.store.load().map_err(|e| e.to_string())?;
    let mut evidence = state
        .evidence
        .lock()
        .map_err(|_| "evidence lock poisoned".to_string())?;
    evidence.refresh_gaps();
    let today_queue = ledgerr_host::evidence::TodayQueue::from_state(&evidence, &settings);
    Ok(EvidenceDashboardPayload { today_queue })
}

#[derive(serde::Serialize, Clone)]
pub struct ProvenancePayload {
    pub badge: String,
    pub css_class: String,
}

#[tauri::command]
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

/// Return the Cytoscape.js-compatible JSON for the holonic pipeline graph.
///
/// The frontend Viz panel calls this once on activation and passes the result
/// directly to `cytoscape({ elements: ... })`.
#[tauri::command]
pub fn get_holon_viz_graph() -> Result<String, String> {
    use holon_viz::{CytoscapeGraph, Holon, HolonKind};
    use std::collections::HashMap;

    let holons = vec![
        Holon { id: "pipeline".into(), label: "Tax Ledger Pipeline".into(), kind: HolonKind::CapsuleGroup,
            parent_id: None, children: vec!["ingest".into(),"classify".into(),"reconcile".into(),"attest".into()], metadata: HashMap::new() },
        Holon { id: "ingest".into(), label: "Ingest PDFs".into(), kind: HolonKind::SysmlBlock,
            parent_id: Some("pipeline".into()), children: vec!["docling".into(),"blake3-id".into()], metadata: HashMap::new() },
        Holon { id: "classify".into(), label: "Classify Transactions".into(), kind: HolonKind::SysmlBlock,
            parent_id: Some("pipeline".into()), children: vec!["rhai-rules".into(),"flag-queue".into()], metadata: HashMap::new() },
        Holon { id: "reconcile".into(), label: "Reconcile & Export".into(), kind: HolonKind::SysmlBlock,
            parent_id: Some("pipeline".into()), children: vec!["excel-workbook".into()], metadata: HashMap::new() },
        Holon { id: "attest".into(), label: "Attest (CPA)".into(), kind: HolonKind::SysmlBlock,
            parent_id: Some("pipeline".into()), children: vec!["audit-log".into()], metadata: HashMap::new() },
        Holon { id: "docling".into(), label: "Docling OCR".into(), kind: HolonKind::ProcessNode,
            parent_id: Some("ingest".into()), children: vec![], metadata: HashMap::new() },
        Holon { id: "blake3-id".into(), label: "Blake3 Content ID".into(), kind: HolonKind::ProcessNode,
            parent_id: Some("ingest".into()), children: vec![], metadata: HashMap::new() },
        Holon { id: "rhai-rules".into(), label: "Rhai Rule Engine".into(), kind: HolonKind::ProcessNode,
            parent_id: Some("classify".into()), children: vec![], metadata: HashMap::new() },
        Holon { id: "flag-queue".into(), label: "Flag Queue".into(), kind: HolonKind::ProcessNode,
            parent_id: Some("classify".into()), children: vec![], metadata: HashMap::new() },
        Holon { id: "excel-workbook".into(), label: "Excel Workbook".into(), kind: HolonKind::OwlClass,
            parent_id: Some("reconcile".into()), children: vec![], metadata: HashMap::new() },
        Holon { id: "audit-log".into(), label: "Immutable Audit Log".into(), kind: HolonKind::AuditEvent,
            parent_id: Some("attest".into()), children: vec![], metadata: HashMap::new() },
    ];

    CytoscapeGraph::from_holons(&holons)
        .to_json()
        .map_err(|e| e.to_string())
}
