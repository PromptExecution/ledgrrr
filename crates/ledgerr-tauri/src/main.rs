#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod commands;
mod state;
mod tray;

use std::sync::{Arc, Mutex};

use ledgerr_host::chat::{ChatTurn, ReviewLog};
use ledgerr_host::internal_openai::InternalOpenAiHandle;
use ledgerr_host::settings::{default_settings_path, SettingsStore};

use state::AppState;

fn main() {
    let store = Arc::new(SettingsStore::new(default_settings_path()));
    let history: Arc<Mutex<Vec<ChatTurn>>> = Arc::new(Mutex::new(Vec::new()));
    let review_log: Arc<Mutex<ReviewLog>> = Arc::new(Mutex::new(ReviewLog::default()));
    let internal_endpoint: Arc<Mutex<Option<InternalOpenAiHandle>>> = Arc::new(Mutex::new(None));

    let app_state = AppState {
        store,
        history,
        review_log,
        internal_endpoint,
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
        .setup(|app| {
            tray::setup_tray(app);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_initial_state,
            commands::save_settings,
            commands::send_message,
            commands::load_rhai_rule_prompt,
            commands::use_internal_phi,
            commands::use_foundry_local,
            commands::use_cloud_model,
            commands::open_docs_playbook,
        ])
        .run(tauri::generate_context!())
        .expect("error while running ledgerr-tauri application");
}
