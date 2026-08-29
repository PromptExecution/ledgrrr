// DEPRECATED, REFERENCE ONLY — not part of the build (no [[bin]] entry,
// and this directory is outside Cargo's src/bin/*.rs auto-discovery).
// Superseded by host-tauri.exe's integrated webview UI. Kept for
// reference only; may bit-rot as the rest of the crate changes.
//
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
use std::sync::{Arc, Mutex};

#[cfg(windows)]
use ledgerr_host::chat::{
    assistant_decision_log, build_rig_prompt_preview, render_rig_exchange_log, render_transcript,
    send_chat_message, user_request_log, ChatRole, ChatTurn, ReviewLog, DEFAULT_RHAI_RULE_MODEL,
    RHAI_RULE_SYSTEM_PROMPT,
};
#[cfg(windows)]
use ledgerr_host::internal_openai::{
    cloud_chat_settings, docs_playbook_status, foundry_local_chat_settings, foundry_local_status,
    internal_phi_backend_status, internal_phi_chat_settings, open_internal_docs_in_browser,
    start_default_internal_openai_endpoint, InternalOpenAiError, InternalOpenAiHandle,
    FOUNDRY_LOCAL_MODEL, INTERNAL_OPENAI_CHAT_URL,
};
#[cfg(windows)]
use ledgerr_host::settings::{default_settings_path, ChatSettings, SettingsStore};

#[cfg(windows)]
slint::slint! {
    import { Button, LineEdit, ScrollView, TextEdit } from "std-widgets.slint";

    // ── Sidebar navigation item ────────────────────────────────────────────────
    component NavItem inherits Rectangle {
        in property <string> label;
        in property <string> mark;
        in property <bool> selected;
        in property <bool> collapsed;
        callback clicked();

        height: 34px;
        border-radius: 5px;
        background: root.selected ? #e8f2ff : #00000000;

        HorizontalLayout {
            padding-left: 8px;
            padding-right: 8px;
            spacing: 8px;
            alignment: start;

            Rectangle {
                width: 3px;
                height: 18px;
                border-radius: 2px;
                background: root.selected ? #2f7dde : #00000000;
            }

            Text {
                text: root.mark;
                color: root.selected ? #17416f : #a9bfd1;
                font-size: 12px;
                vertical-alignment: center;
            }

            Text {
                visible: !root.collapsed;
                text: root.label;
                color: root.selected ? #12324f : #e7f0f8;
                font-size: 13px;
                vertical-alignment: center;
            }
        }

        TouchArea { clicked => { root.clicked(); } }
    }

    // ── Tab selector used in the Logs panel ───────────────────────────────────
    component LogSelector inherits Rectangle {
        in property <string> label;
        in property <bool> selected;
        callback clicked();

        width: 120px;
        height: 30px;
        border-radius: 4px;
        border-width: 1px;
        border-color: root.selected ? #2f7dde : #c8d4df;
        background: root.selected ? #e8f2ff : #ffffff;

        Text {
            text: root.label;
            color: root.selected ? #17416f : #405466;
            font-size: 12px;
            horizontal-alignment: center;
            vertical-alignment: center;
        }

        TouchArea { clicked => { root.clicked(); } }
    }

    // ── Model selector pill used in the Chat header bar ───────────────────────
    // `active` highlights the pill to indicate this is the currently selected model.
    component ModelPill inherits Rectangle {
        in property <string> label;
        in property <bool> active;
        in property <bool> enabled: true;
        callback clicked();

        height: 28px;
        border-radius: 14px;
        border-width: 1px;
        border-color: root.active ? #2f7dde : #c8d4df;
        background: root.active ? #e0ecfb : (root.enabled ? #f4f8fc : #f0f0f0);

        Text {
            text: root.label;
            color: root.active ? #17416f : (root.enabled ? #607080 : #aaaaaa);
            font-size: 12px;
            horizontal-alignment: center;
            vertical-alignment: center;
        }

        TouchArea {
            enabled: root.enabled;
            clicked => { root.clicked(); }
        }
    }

    export component HostWindow inherits Window {
        // Size and position are set from Rust via window.set_size() /
        // set_position() after reading the OS work area — this keeps the window
        // resizable (hardcoding width/height in the DSL locks the size) and
        // prevents overflow on any screen resolution.
        title: "l3dg3rr";

        in-out property <string> version_text: "Version";
        in-out property <string> status_text: "Ready";
        in-out property <string> endpoint_text;
        in-out property <string> model_text;
        in-out property <string> api_key_text;
        in-out property <string> system_prompt_text;
        in-out property <string> transcript_text;
        in-out property <string> review_log_text;
        in-out property <string> rig_prompt_preview_text;
        in-out property <string> draft_message_text;
        in-out property <bool> busy: false;
        in-out property <bool> sidebar_collapsed: false;
        in-out property <int> active_panel: 0;
        in-out property <int> active_log_panel: 0;
        in-out property <string> docs_status_text;

        // Derived: true when the endpoint is the built-in local Phi-4 server.
        // Used to highlight the correct ModelPill without extra Rust state.
        property <bool> using_internal_phi: root.model_text == "phi-4-mini-reasoning" && root.api_key_text == "local-tool-tray";
        property <bool> using_foundry_local: root.model_text == "phi-4-mini" && root.api_key_text == "local-foundry";

        callback save_settings();
        callback send_message();
        callback load_rhai_rule_prompt();
        callback use_internal_phi();
        callback use_foundry_local();
        callback use_cloud_model();
        callback open_docs_playbook();

        // ── Root background fills the entire window ───────────────────────────
        Rectangle {
            background: #f3f7fb;

            HorizontalLayout {

                // ── Sidebar ───────────────────────────────────────────────────
                Rectangle {
                    width: root.sidebar_collapsed ? 68px : 220px;
                    background: #203647;

                    VerticalLayout {
                        padding: 10px;
                        spacing: 8px;

                        NavItem {
                            mark: root.sidebar_collapsed ? ">" : "<";
                            label: "Collapse";
                            collapsed: root.sidebar_collapsed;
                            selected: false;
                            clicked => { root.sidebar_collapsed = !root.sidebar_collapsed; }
                        }

                        NavItem {
                            mark: "AI";
                            label: "Chat";
                            collapsed: root.sidebar_collapsed;
                            selected: root.active_panel == 0;
                            clicked => { root.active_panel = 0; }
                        }

                        NavItem {
                            mark: "LG";
                            label: "Logs";
                            collapsed: root.sidebar_collapsed;
                            selected: root.active_panel == 1;
                            clicked => { root.active_panel = 1; }
                        }

                        NavItem {
                            mark: "ST";
                            label: "Settings";
                            collapsed: root.sidebar_collapsed;
                            selected: root.active_panel == 2;
                            clicked => { root.active_panel = 2; }
                        }

                        NavItem {
                            mark: "DK";
                            label: "Docs Playbook";
                            collapsed: root.sidebar_collapsed;
                            selected: root.active_panel == 3;
                            clicked => { root.active_panel = 3; }
                        }

                        Rectangle { height: 1px; background: #4c6476; }

                        Text {
                            visible: !root.sidebar_collapsed;
                            text: root.version_text;
                            color: #d7e7f2;
                            wrap: word-wrap;
                        }
                    }
                }

                // ── Main content column ───────────────────────────────────────
                VerticalLayout {
                    padding: 16px;
                    spacing: 8px;

                    // App header row: title left, status right
                    HorizontalLayout {
                        height: 36px;
                        alignment: space-between;

                        Text {
                            text: "l3dg3rr tool tray";
                            color: #0f2d4a;
                            font-size: 22px;
                            vertical-alignment: center;
                        }

                        Text {
                            text: root.status_text;
                            color: #405566;
                            font-size: 12px;
                            horizontal-alignment: right;
                            vertical-alignment: center;
                            overflow: elide;
                        }
                    }

                    // Panel switcher: all panels share one stacking Rectangle so
                    // visible:false on a sibling does not consume layout space.
                    // vertical-stretch: 1 makes this Rectangle fill the remaining
                    // window height so all panels scale when the window is resized.
                    Rectangle {
                        vertical-stretch: 1;

                        // ── Chat ─────────────────────────────────────────────
                        Rectangle {
                            visible: root.active_panel == 0;
                            width: parent.width;
                            height: parent.height;
                            background: #ffffff;
                            border-color: #d4dfe9;
                            border-width: 1px;
                            border-radius: 8px;

                            VerticalLayout {
                                padding: 16px;
                                spacing: 10px;

                                // ── Chat header: title + active-model badge ───
                                HorizontalLayout {
                                    height: 36px;
                                    alignment: space-between;

                                    Text {
                                        text: "Chat";
                                        color: #0f2d4a;
                                        font-size: 20px;
                                        vertical-alignment: center;
                                    }

                                    // Active model badge — shows exactly which model
                                    // will receive the next message.
                                    Rectangle {
                                        width: 280px;
                                        height: 28px;
                                        border-radius: 14px;
                                        border-width: 1px;
                                        border-color: #2f7dde;
                                        background: root.using_internal_phi ? #e0ecfb : (root.using_foundry_local ? #e8f8ef : #fff8e8);

                                        HorizontalLayout {
                                            padding-left: 12px;
                                            padding-right: 12px;
                                            spacing: 6px;

                                            Text {
                                                text: root.using_internal_phi ? "⚡" : (root.using_foundry_local ? "WA" : "☁");
                                                font-size: 13px;
                                                vertical-alignment: center;
                                            }

                                            Text {
                                                text: root.model_text == "" ? "No model — go to Settings" : root.model_text;
                                                color: #17416f;
                                                font-size: 12px;
                                                vertical-alignment: center;
                                                overflow: elide;
                                            }
                                        }
                                    }
                                }

                                // ── Model quick-select bar ────────────────────
                                // Switch models without leaving the Chat panel.
                                // The highlighted pill is the currently active model.
                                HorizontalLayout {
                                    height: 32px;
                                    spacing: 8px;
                                    alignment: start;

                                    Text {
                                        text: "Model:";
                                        color: #607080;
                                        font-size: 12px;
                                        vertical-alignment: center;
                                        width: 48px;
                                    }

                                    ModelPill {
                                        width: 180px;
                                        label: "⚡ Internal Phi-4 Mini";
                                        active: root.using_internal_phi;
                                        enabled: !root.busy;
                                        clicked => { root.use_internal_phi(); }
                                    }

                                    ModelPill {
                                        width: 190px;
                                        label: "Windows AI / Foundry";
                                        active: root.using_foundry_local;
                                        enabled: !root.busy;
                                        clicked => { root.use_foundry_local(); }
                                    }

                                    ModelPill {
                                        width: 150px;
                                        label: "☁ Cloud / OpenAI";
                                        active: !root.using_internal_phi && !root.using_foundry_local && root.model_text != "";
                                        enabled: !root.busy;
                                        clicked => { root.use_cloud_model(); }
                                    }

                                    Text {
                                        visible: !root.using_internal_phi && !root.using_foundry_local && root.model_text != "";
                                        text: "— edit endpoint & key in Settings";
                                        color: #8099aa;
                                        font-size: 11px;
                                        vertical-alignment: center;
                                    }
                                }

                                // ── Transcript (stretches to fill) ────────────
                                Rectangle {
                                    vertical-stretch: 1;
                                    background: #f6f9fd;
                                    border-color: #d4dfe9;
                                    border-width: 1px;
                                    border-radius: 8px;

                                    VerticalLayout {
                                        padding: 12px;
                                        spacing: 4px;

                                        Text {
                                            text: "Transcript";
                                            color: #607080;
                                            font-size: 11px;
                                        }

                                        ScrollView {
                                            vertical-stretch: 1;
                                            Text {
                                                width: parent.width;
                                                text: root.transcript_text;
                                                color: #1a2b3c;
                                                font-size: 13px;
                                                wrap: word-wrap;
                                            }
                                        }
                                    }
                                }

                                // ── Input area (fixed height at bottom) ───────
                                HorizontalLayout {
                                    spacing: 12px;
                                    height: 120px;

                                    TextEdit {
                                        horizontal-stretch: 1;
                                        text <=> root.draft_message_text;
                                        enabled: !root.busy;
                                    }

                                    VerticalLayout {
                                        spacing: 8px;
                                        width: 160px;
                                        alignment: start;

                                        Button {
                                            text: root.busy ? "Sending…" : "Send";
                                            enabled: !root.busy;
                                            clicked => { root.send_message(); }
                                        }

                                        Button {
                                            text: "Rhai Rule Prompt";
                                            enabled: !root.busy;
                                            clicked => { root.load_rhai_rule_prompt(); }
                                        }
                                    }
                                }
                            }
                        }

                        // ── Logs ─────────────────────────────────────────────
                        Rectangle {
                            visible: root.active_panel == 1;
                            width: parent.width;
                            height: parent.height;
                            background: #ffffff;
                            border-color: #d4dfe9;
                            border-width: 1px;
                            border-radius: 8px;

                            VerticalLayout {
                                padding: 16px;
                                spacing: 10px;

                                Text {
                                    text: "Logs";
                                    color: #0f2d4a;
                                    font-size: 20px;
                                }

                                HorizontalLayout {
                                    spacing: 6px;
                                    height: 32px;
                                    alignment: start;

                                    LogSelector {
                                        label: "Transport";
                                        selected: root.active_log_panel == 0;
                                        clicked => { root.active_log_panel = 0; }
                                    }

                                    LogSelector {
                                        label: "Review";
                                        selected: root.active_log_panel == 1;
                                        clicked => { root.active_log_panel = 1; }
                                    }
                                }

                                // Log sub-panels use the same stacking pattern and
                                // stretch to fill remaining space in the outer panel.
                                Rectangle {
                                    vertical-stretch: 1;

                                    Rectangle {
                                        visible: root.active_log_panel == 0;
                                        width: parent.width;
                                        height: parent.height;
                                        background: #f7f8fb;
                                        border-color: #d4dfe9;
                                        border-width: 1px;
                                        border-radius: 8px;

                                        VerticalLayout {
                                            padding: 12px;
                                            spacing: 6px;

                                            Text { text: "Rig / OpenAI Transport"; color: #445566; font-size: 12px; }

                                            ScrollView {
                                                vertical-stretch: 1;
                                                Text {
                                                    width: parent.width;
                                                    text: root.rig_prompt_preview_text;
                                                    color: #223344;
                                                    font-size: 13px;
                                                    wrap: word-wrap;
                                                }
                                            }
                                        }
                                    }

                                    Rectangle {
                                        visible: root.active_log_panel == 1;
                                        width: parent.width;
                                        height: parent.height;
                                        background: #fbfaf7;
                                        border-color: #ddd4c6;
                                        border-width: 1px;
                                        border-radius: 8px;

                                        VerticalLayout {
                                            padding: 12px;
                                            spacing: 6px;

                                            Text { text: "Review Diffsets"; color: #665533; font-size: 12px; }

                                            ScrollView {
                                                vertical-stretch: 1;
                                                Text {
                                                    width: parent.width;
                                                    text: root.review_log_text;
                                                    color: #332a1c;
                                                    font-size: 13px;
                                                    wrap: word-wrap;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // ── Settings ──────────────────────────────────────────
                        Rectangle {
                            visible: root.active_panel == 2;
                            width: parent.width;
                            height: parent.height;
                            background: #e8f0f8;
                            border-color: #c7d8ea;
                            border-width: 1px;
                            border-radius: 8px;

                            VerticalLayout {
                                padding: 16px;
                                spacing: 8px;

                                Text {
                                    text: "Settings";
                                    color: #0f2d4a;
                                    font-size: 20px;
                                }

                                Text { text: "Endpoint URL"; color: #445566; font-size: 12px; }
                                LineEdit { text <=> root.endpoint_text; enabled: !root.busy; }

                                Text { text: "Model"; color: #445566; font-size: 12px; }
                                LineEdit { text <=> root.model_text; enabled: !root.busy; }

                                Text { text: "API Key"; color: #445566; font-size: 12px; }
                                LineEdit { text <=> root.api_key_text; enabled: !root.busy; }

                                Text { text: "System Prompt"; color: #445566; font-size: 12px; }
                                TextEdit {
                                    text <=> root.system_prompt_text;
                                    enabled: !root.busy;
                                    vertical-stretch: 1;
                                    min-height: 120px;
                                }

                                HorizontalLayout {
                                    spacing: 8px;
                                    alignment: start;

                                    Button {
                                        text: "Use Internal Phi-4";
                                        enabled: !root.busy;
                                        clicked => { root.use_internal_phi(); }
                                    }

                                    Button {
                                        text: "Use Windows AI";
                                        enabled: !root.busy;
                                        clicked => { root.use_foundry_local(); }
                                    }

                                    Button {
                                        text: "Use Cloud Model";
                                        enabled: !root.busy;
                                        clicked => { root.use_cloud_model(); }
                                    }

                                    Button {
                                        text: root.busy ? "Working…" : "Save Settings";
                                        enabled: !root.busy;
                                        clicked => { root.save_settings(); }
                                    }
                                }
                            }
                        }

                        // ── Docs Playbook ─────────────────────────────────────
                        Rectangle {
                            visible: root.active_panel == 3;
                            width: parent.width;
                            height: parent.height;
                            background: #ffffff;
                            border-color: #d4dfe9;
                            border-width: 1px;
                            border-radius: 8px;

                            VerticalLayout {
                                padding: 16px;
                                spacing: 10px;

                                Text {
                                    text: "Docs Playbook";
                                    color: #0f2d4a;
                                    font-size: 20px;
                                }

                                Text {
                                    text: root.docs_status_text;
                                    color: #405466;
                                    wrap: word-wrap;
                                }

                                HorizontalLayout {
                                    spacing: 8px;
                                    height: 36px;
                                    alignment: start;

                                    Button {
                                        text: "Open Docs Playbook";
                                        enabled: !root.busy;
                                        clicked => { root.open_docs_playbook(); }
                                    }

                                    Button {
                                        text: "Load Rhai Mutation Prompt";
                                        enabled: !root.busy;
                                        clicked => {
                                            root.active_panel = 0;
                                            root.load_rhai_rule_prompt();
                                        }
                                    }
                                }

                                Rectangle {
                                    vertical-stretch: 1;
                                    background: #f6f9fd;
                                    border-color: #d4dfe9;
                                    border-width: 1px;
                                    border-radius: 6px;

                                    VerticalLayout {
                                        padding: 10px;

                                        ScrollView {
                                            vertical-stretch: 1;
                                            Text {
                                                width: parent.width;
                                                text: root.rig_prompt_preview_text;
                                                color: #223344;
                                                font-size: 13px;
                                                wrap: word-wrap;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(windows)]
fn main() -> Result<(), slint::PlatformError> {
    let store = SettingsStore::new(default_settings_path());
    let mut settings = store
        .load()
        .map_err(|error| slint::PlatformError::from(error.to_string()))?;
    if settings.chat.model.trim().is_empty() || settings.chat.api_key.trim().is_empty() {
        settings.chat = internal_phi_chat_settings(settings.chat.system_prompt.clone());
    }

    let app = HostWindow::new()?;
    apply_initial_window_geometry(&app.window());
    app.set_version_text(format!("Version {}", env!("CARGO_PKG_VERSION")).into());
    app.set_status_text(format!("Editing {}", store.path().display()).into());
    app.set_endpoint_text(settings.chat.endpoint_url.clone().into());
    app.set_model_text(settings.chat.model.clone().into());
    app.set_api_key_text(settings.chat.api_key.clone().into());
    app.set_system_prompt_text(settings.chat.system_prompt.clone().into());
    app.set_transcript_text(
        "Tool tray chat is ready.\n\nSave the endpoint, model, and API key, then send a message."
            .into(),
    );
    app.set_review_log_text("No review log entries yet.".into());
    app.set_rig_prompt_preview_text(
        format!("No request sent yet.\n\n{}", internal_phi_backend_status()).into(),
    );
    app.set_draft_message_text(ledgerr_host::chat::rhai_rule_prompt_seed().into());
    app.set_docs_status_text(docs_playbook_status().into());

    let store = Arc::new(store);
    let history = Arc::new(Mutex::new(Vec::<ChatTurn>::new()));
    let review_log = Arc::new(Mutex::new(ReviewLog::default()));
    let internal_endpoint = Arc::new(Mutex::new(None::<InternalOpenAiHandle>));

    {
        let app_handle = app.as_weak();
        let store = Arc::clone(&store);
        app.on_save_settings(move || {
            let Some(app) = app_handle.upgrade() else {
                return;
            };

            let mut settings = match store.load() {
                Ok(settings) => settings,
                Err(error) => {
                    app.set_status_text(format!("Failed to load settings: {error}").into());
                    return;
                }
            };

            settings.chat = chat_settings_from_window(&app);
            match store.save(&settings) {
                Ok(()) => app.set_status_text(
                    format!("Saved chat settings to {}", store.path().display()).into(),
                ),
                Err(error) => {
                    app.set_status_text(format!("Failed to save settings: {error}").into())
                }
            }
        });
    }

    {
        let app_handle = app.as_weak();
        let internal_endpoint = Arc::clone(&internal_endpoint);
        app.on_use_internal_phi(move || {
            let Some(app) = app_handle.upgrade() else {
                return;
            };

            match ensure_internal_endpoint(&internal_endpoint) {
                Ok(status) => {
                    let chat = internal_phi_chat_settings(app.get_system_prompt_text().to_string());
                    apply_chat_settings_to_window(&app, &chat);
                    app.set_rig_prompt_preview_text(internal_phi_backend_status().into());
                    app.set_status_text(format!("{status} Chat is set to internal Phi-4.").into());
                }
                Err(error) => {
                    app.set_status_text(
                        format!("Failed to start internal Phi-4 endpoint: {error}").into(),
                    );
                }
            }
        });
    }

    {
        let app_handle = app.as_weak();
        app.on_use_foundry_local(move || {
            let Some(app) = app_handle.upgrade() else {
                return;
            };

            match foundry_local_chat_settings(app.get_system_prompt_text().to_string()) {
                Ok(chat) => {
                    apply_chat_settings_to_window(&app, &chat);
                    app.set_rig_prompt_preview_text(foundry_local_status().into());
                    app.set_status_text(
                        format!(
                            "Chat is set to Windows AI / Foundry Local with model {FOUNDRY_LOCAL_MODEL}."
                        )
                        .into(),
                    );
                }
                Err(error) => {
                    app.set_status_text(
                        format!(
                            "Failed to discover Windows AI / Foundry Local: {error}. Run `just windows-ai-install` then `just windows-ai-setup`."
                        )
                        .into(),
                    );
                }
            }
        });
    }

    {
        let app_handle = app.as_weak();
        app.on_use_cloud_model(move || {
            let Some(app) = app_handle.upgrade() else {
                return;
            };

            let chat = cloud_chat_settings(app.get_system_prompt_text().to_string());
            apply_chat_settings_to_window(&app, &chat);
            app.set_status_text("Chat is set to a cloud OpenAI-compatible endpoint. Enter model and API key before sending.".into());
        });
    }

    {
        let app_handle = app.as_weak();
        let internal_endpoint = Arc::clone(&internal_endpoint);
        app.on_open_docs_playbook(move || {
            let Some(app) = app_handle.upgrade() else {
                return;
            };

            match ensure_internal_endpoint(&internal_endpoint).and_then(|status| {
                open_internal_docs_in_browser()
                    .map(|()| status)
                    .map_err(|error| error.to_string())
            }) {
                Ok(status) => app.set_status_text(
                    format!("{status} Opened local docs playbook in the Windows browser.").into(),
                ),
                Err(error) => {
                    app.set_status_text(format!("Failed to open docs playbook: {error}").into())
                }
            }
        });
    }

    {
        let app_handle = app.as_weak();
        let review_log = Arc::clone(&review_log);
        app.on_load_rhai_rule_prompt(move || {
            let Some(app) = app_handle.upgrade() else {
                return;
            };

            let previous_model = app.get_model_text().to_string();
            let previous_system_prompt = app.get_system_prompt_text().to_string();
            let entry =
                ledgerr_host::chat::rhai_rule_prompt_seed_log(&previous_model, &previous_system_prompt);

            app.set_system_prompt_text(RHAI_RULE_SYSTEM_PROMPT.into());
            if app.get_model_text().trim().is_empty() {
                app.set_model_text(DEFAULT_RHAI_RULE_MODEL.into());
            }
            app.set_draft_message_text(ledgerr_host::chat::rhai_rule_prompt_seed().into());
            let review_text = {
                let mut review_log = review_log.lock().expect("review log poisoned");
                review_log.push(entry);
                review_log.render()
            };
            app.set_review_log_text(review_text.into());
            app.set_status_text(
                "Loaded a Rhai rule mutation prompt seed. Edit it, then send through the configured model."
                    .into(),
            );
        });
    }

    {
        let app_handle = app.as_weak();
        let store = Arc::clone(&store);
        let history = Arc::clone(&history);
        let review_log = Arc::clone(&review_log);
        let internal_endpoint = Arc::clone(&internal_endpoint);
        app.on_send_message(move || {
            let Some(app) = app_handle.upgrade() else {
                return;
            };

            let draft_message = app.get_draft_message_text().to_string();
            if draft_message.trim().is_empty() {
                app.set_status_text("Enter a message before sending.".into());
                return;
            }

            let mut settings = match store.load() {
                Ok(settings) => settings,
                Err(error) => {
                    app.set_status_text(format!("Failed to load settings: {error}").into());
                    return;
                }
            };
            settings.chat = chat_settings_from_window(&app);
            if settings.chat.endpoint_url.trim() == INTERNAL_OPENAI_CHAT_URL {
                if let Err(error) = ensure_internal_endpoint(&internal_endpoint) {
                    app.set_status_text(
                        format!("Failed to start internal Phi-4 endpoint: {error}").into(),
                    );
                    return;
                }
            }
            if let Err(error) = store.save(&settings) {
                app.set_status_text(format!("Failed to save settings: {error}").into());
                return;
            }

            let user_turn = ChatTurn {
                role: ChatRole::User,
                content: draft_message.trim().to_string(),
            };
            let previous_rhai = user_turn.content.clone();
            let history_snapshot = {
                let mut history = history.lock().expect("chat history poisoned");
                history.push(user_turn.clone());
                history.clone()
            };
            let review_text = {
                let mut review_log = review_log.lock().expect("review log poisoned");
                review_log.push(user_request_log(&user_turn.content));
                review_log.render()
            };

            app.set_busy(true);
            app.set_status_text(
                format!(
                    "Sending to {} with model {}",
                    settings.chat.endpoint_url, settings.chat.model
                )
                .into(),
            );
            app.set_transcript_text(render_transcript(&history_snapshot).into());
            app.set_review_log_text(review_text.into());
            let request_preview = build_rig_prompt_preview(
                &settings.chat,
                &history_snapshot[..history_snapshot.len() - 1],
                &user_turn.content,
            );
            let backend_status = internal_phi_backend_status();
            app.set_rig_prompt_preview_text(
                render_rig_exchange_log(&request_preview, &backend_status, None, None).into(),
            );

            let app_handle = app.as_weak();
            let history = Arc::clone(&history);
            let review_log = Arc::clone(&review_log);
            std::thread::spawn(move || {
                let result = send_chat_message(
                    &settings.chat,
                    &history_snapshot[..history_snapshot.len() - 1],
                    &user_turn.content,
                );

                let _ = slint::invoke_from_event_loop(move || {
                    let Some(app) = app_handle.upgrade() else {
                        return;
                    };

                    let mut history = history.lock().expect("chat history poisoned");
                    match result {
                        Ok(response) => {
                            let review_text = {
                                let mut review_log =
                                    review_log.lock().expect("review log poisoned");
                                review_log.push(assistant_decision_log(&previous_rhai, &response));
                                review_log.render()
                            };
                            app.set_rig_prompt_preview_text(
                                render_rig_exchange_log(
                                    &request_preview,
                                    &backend_status,
                                    Some(&response),
                                    None,
                                )
                                .into(),
                            );
                            history.push(ChatTurn {
                                role: ChatRole::Assistant,
                                content: response,
                            });
                            app.set_transcript_text(render_transcript(&history).into());
                            app.set_review_log_text(review_text.into());
                            app.set_draft_message_text("".into());
                            app.set_status_text("Remote chat response received.".into());
                        }
                        Err(error) => {
                            app.set_transcript_text(render_transcript(&history).into());
                            app.set_rig_prompt_preview_text(
                                render_rig_exchange_log(
                                    &request_preview,
                                    &backend_status,
                                    None,
                                    Some(&error.to_string()),
                                )
                                .into(),
                            );
                            app.set_status_text(format!("Chat request failed: {error}").into());
                        }
                    }
                    app.set_busy(false);
                });
            });
        });
    }

    app.run()
}

#[cfg(windows)]
fn chat_settings_from_window(app: &HostWindow) -> ChatSettings {
    ChatSettings {
        endpoint_url: app.get_endpoint_text().trim().to_string(),
        model: app.get_model_text().trim().to_string(),
        api_key: app.get_api_key_text().trim().to_string(),
        system_prompt: app.get_system_prompt_text().trim().to_string(),
    }
}

#[cfg(windows)]
fn apply_chat_settings_to_window(app: &HostWindow, settings: &ChatSettings) {
    app.set_endpoint_text(settings.endpoint_url.clone().into());
    app.set_model_text(settings.model.clone().into());
    app.set_api_key_text(settings.api_key.clone().into());
    app.set_system_prompt_text(settings.system_prompt.clone().into());
}

#[cfg(windows)]
fn ensure_internal_endpoint(
    internal_endpoint: &Arc<Mutex<Option<InternalOpenAiHandle>>>,
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

/// Set window size and center it within the OS work area (screen minus taskbar).
///
/// Hardcoding `width`/`height` in the Slint DSL freezes the window at that size
/// and prevents the user from resizing it. Setting size programmatically via
/// `window.set_size()` leaves the resize handle active while still giving the
/// window a sensible initial footprint.
///
/// We read the Windows work area via `SystemParametersInfoW(SPI_GETWORKAREA)`
/// to get the usable rectangle (excludes taskbar and any docked toolbars), then
/// size the window to 88 % of that area (capped at 1280×840) and center it.
/// The Slint `scale_factor()` converts physical pixels → logical coordinates so
/// the math is correct on any DPI / scaling configuration.
#[cfg(windows)]
#[allow(unsafe_code)]
fn apply_initial_window_geometry(window: &slint::Window) {
    #[repr(C)]
    #[derive(Default)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[link(name = "user32")]
    extern "system" {
        fn SystemParametersInfoW(
            action: u32,
            param: u32,
            pv: *mut std::ffi::c_void,
            ini: u32,
        ) -> i32;
    }

    const SPI_GETWORKAREA: u32 = 0x0030;
    let mut work = Rect::default();
    // SAFETY: Rect is repr(C) and sized exactly as RECT; pointer is valid for the call duration.
    let ok = unsafe { SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut work as *mut _ as *mut _, 0) };

    // Fall back to a safe default if the API call fails.
    if ok == 0 {
        work = Rect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1040,
        };
    }

    let scale = window.scale_factor();
    let work_w = (work.right - work.left) as f32 / scale;
    let work_h = (work.bottom - work.top) as f32 / scale;
    let work_x = work.left as f32 / scale;
    let work_y = work.top as f32 / scale;

    let win_w = (work_w * 0.88).min(1280.0).max(800.0);
    let win_h = (work_h * 0.88).min(840.0).max(600.0);
    let win_x = work_x + (work_w - win_w) / 2.0;
    let win_y = work_y + (work_h - win_h) / 2.0;

    window.set_size(slint::WindowSize::Logical(slint::LogicalSize::new(
        win_w, win_h,
    )));
    window.set_position(slint::WindowPosition::Logical(slint::LogicalPosition::new(
        win_x, win_y,
    )));
}

#[cfg(not(windows))]
fn main() {
    eprintln!("host-window is currently supported on Windows builds only");
    std::process::exit(1);
}
