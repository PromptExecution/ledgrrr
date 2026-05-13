#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

#[cfg(target_os = "windows")]
mod commands;
#[cfg(target_os = "windows")]
mod state;

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("host-tauri: this binary is Windows-only");
    std::process::exit(0);
}

#[cfg(target_os = "windows")]
fn main() {
    let _ = std::fs::write(
        std::env::temp_dir().join("host-tauri-windows-main.txt"),
        format!("windows main running\n"),
    );
    use std::panic;
    panic::set_hook(Box::new(|info| {
        let msg = format!("panic: {info}");
        eprintln!("{msg}");
        let _ = std::fs::write(std::env::temp_dir().join("host-tauri-panic.txt"), &msg);
    }));

    if let Ok(uuid) = std::env::var("TAURI_TEST_UUID") {
        eprintln!("[telemetry] TAURI_TEST_UUID={uuid}");
        let _ = std::fs::write(
            std::env::temp_dir().join("host-tauri-telemetry-signal.txt"),
            format!("TAURI_TEST_UUID={uuid}\n"),
        );
    }
    if let Ok(delay) = std::env::var("TAURI_TEST_KILL_DELAY") {
        eprintln!("[telemetry] TAURI_TEST_KILL_DELAY={delay}");
        let _ = std::fs::write(
            std::env::temp_dir().join("host-tauri-kill-delay.txt"),
            format!("TAURI_TEST_KILL_DELAY={delay}\npid={}\n", std::process::id()),
        );
    }
    if let Ok(shots) = std::env::var("TAURI_TEST_SCREENSHOT_PATH") {
        eprintln!("[telemetry] TAURI_TEST_SCREENSHOT_PATH={shots}");
    }

    use std::sync::{Arc, Mutex};
    use ledgerr_host::chat::{ChatTurn, ReviewLog};
    use ledgerr_host::evidence::EvidenceState;
    use ledgerr_host::internal_openai::InternalOpenAiHandle;
    use ledgerr_host::settings::{default_settings_path, SettingsStore};
    use tauri::Manager;
    use state::AppState;

    let store = Arc::new(SettingsStore::new(default_settings_path()));
    let history: Arc<Mutex<Vec<ChatTurn>>> = Arc::new(Mutex::new(Vec::new()));
    let review_log: Arc<Mutex<ReviewLog>> = Arc::new(Mutex::new(ReviewLog::default()));
    let internal_endpoint: Arc<Mutex<Option<InternalOpenAiHandle>>> = Arc::new(Mutex::new(None));
    let evidence: Arc<Mutex<EvidenceState>> = Arc::new(Mutex::new(EvidenceState::new()));

    let app_state = AppState {
        store,
        history,
        review_log,
        internal_endpoint,
        evidence,
    };

    // Enable CDP remote debugging port — the launcher should set WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS
    // before launching. The Rust code reads TAURI_CDP_PORT for logging only.
    let cdp_port = std::env::var("TAURI_CDP_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(0);
    if cdp_port > 0 {
        eprintln!("[cdp] port={cdp_port} (launcher must set WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS)");
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
        .setup(move |app| {
            let _ = std::fs::write(
                std::env::temp_dir().join("host-tauri-setup-ok.txt"),
                format!("setup hook ran at {}\n", std::process::id()),
            );
            let build = env!("TAURI_BUILD_NUMBER");
            let title = format!("ledgrrr v{}+b{}", env!("CARGO_PKG_VERSION"), build);
            let w = tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("index.html".into()))
                .title(&title)
                .inner_size(1100.0, 760.0)
                .center()
                .resizable(true)
                .decorations(true)
                .visible(true)
                .build()
                .expect("failed to build main window");
            let _: std::result::Result<(), _> = w.set_title(&title);
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
            commands::get_evidence_dashboard,
            commands::get_tx_provenance,
            commands::get_test_harness_config,
            commands::write_dom_dump,
            commands::get_cargo_pkg_version,
            commands::get_holon_viz_graph,
        ])
        .build(tauri::generate_context!())
        .unwrap_or_else(|e| {
            eprintln!("[build error] {e}");
            let _ = std::fs::write(
                std::env::temp_dir().join("host-tauri-build-error.txt"),
                format!("{e}\n"),
            );
            std::process::exit(1);
        })
        .run(|_handle, event| {
            if let tauri::RunEvent::Exit = event {
                eprintln!("[run event] Exit");
            }
        });
}
