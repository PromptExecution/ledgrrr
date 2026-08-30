#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

#[cfg(target_os = "windows")]
mod commands;
#[cfg(target_os = "windows")]
mod remote_pilot;
#[cfg(target_os = "windows")]
mod state;
#[cfg(target_os = "windows")]
mod tray;

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
            format!(
                "TAURI_TEST_KILL_DELAY={delay}\npid={}\n",
                std::process::id()
            ),
        );
    }
    if let Ok(shots) = std::env::var("TAURI_TEST_SCREENSHOT_PATH") {
        eprintln!("[telemetry] TAURI_TEST_SCREENSHOT_PATH={shots}");
    }

    // --timeout <seconds>: test/debug mode. Arms a title-bar countdown and
    // starts the remote-pilot HTTP interface (remote_pilot::REMOTE_PILOT_ADDR)
    // so a driver (human or LLM) can evaluate JS / screenshot / switch panels
    // via CDP and read back what was sent. At 0 the session log (chat
    // history, review log, every remote-pilot command received) is dumped to
    // the OS temp dir and the app exits. Absent --timeout, none of this
    // starts and the app behaves exactly as it did before — no new port
    // opens for a normal end-user launch.
    let cli_timeout_secs: Option<u64> = {
        let args: Vec<String> = std::env::args().collect();
        args.iter()
            .position(|a| a == "--timeout")
            .and_then(|i| args.get(i + 1).and_then(|v| v.parse::<u64>().ok()))
    };
    let remote_pilot_state = std::sync::Arc::new(remote_pilot::RemotePilotState::default());
    if let Some(secs) = cli_timeout_secs {
        remote_pilot_state.arm_timeout(std::time::Duration::from_secs(secs));
        if let Err(e) = remote_pilot::spawn(remote_pilot_state.clone()) {
            eprintln!(
                "[remote-pilot] failed to start on {}: {e}",
                remote_pilot::REMOTE_PILOT_ADDR
            );
        } else {
            eprintln!(
                "[remote-pilot] listening on {} — timeout {secs}s",
                remote_pilot::REMOTE_PILOT_ADDR
            );
        }
    }

    use ledgerr_host::chat::{ChatTurn, ReviewLog};
    use ledgerr_host::evidence::EvidenceState;
    use ledgerr_host::internal_openai::InternalOpenAiHandle;
    use state::AppState;
    use std::sync::{Arc, Mutex};

    let store = Arc::new(ledgerr_host::settings_client::SettingsClient::new());
    let history: Arc<Mutex<Vec<ChatTurn>>> = Arc::new(Mutex::new(Vec::new()));
    let review_log: Arc<Mutex<ReviewLog>> = Arc::new(Mutex::new(ReviewLog::default()));
    let internal_endpoint: Arc<Mutex<Option<InternalOpenAiHandle>>> = Arc::new(Mutex::new(None));
    let evidence: Arc<Mutex<EvidenceState>> = Arc::new(Mutex::new(EvidenceState::new()));
    let pending_tool_loop: Arc<Mutex<Option<state::PendingToolLoopSession>>> =
        Arc::new(Mutex::new(None));

    let app_state = AppState {
        store,
        history,
        review_log,
        internal_endpoint,
        evidence,
        pending_tool_loop,
    };

    // Enable CDP remote debugging port — the launcher should set WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS
    // before launching. The Rust code reads TAURI_CDP_PORT for logging only.
    let cdp_port = std::env::var("TAURI_CDP_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(0);
    if cdp_port > 0 {
        eprintln!(
            "[cdp] port={cdp_port} (launcher must set WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS)"
        );
    }

    use specta_typescript::Typescript;
    use tauri::Manager;
    use tauri_specta::{collect_commands, Builder as SpectaBuilder};

    let specta_builder = SpectaBuilder::<tauri::Wry>::new().commands(collect_commands![
        commands::get_initial_state,
        commands::save_settings,
        commands::send_message,
        commands::confirm_pending_tool_call,
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
        commands::get_type_graph,
        commands::get_desktop_status,
        commands::start_desktop_runtime,
        commands::stop_desktop_runtime,
        commands::open_desktop_logs,
        commands::get_desktop_repair_plan,
        commands::get_foundry_local_install_plan,
        commands::foundry_local_install_action,
    ]);

    #[cfg(debug_assertions)]
    specta_builder
        .export(Typescript::default(), "../ui/bindings.ts")
        .expect("Failed to export TS bindings");

    let setup_remote_pilot_state = remote_pilot_state.clone();

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
            let w = tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::App("index.html".into()),
            )
            .title(&title)
            .inner_size(1400.0, 900.0)
            .min_inner_size(1100.0, 760.0)
            .center()
            .resizable(true)
            .decorations(true)
            .visible(true)
            .build()
            .expect("failed to build main window");
            let _: std::result::Result<(), _> = w.set_title(&title);
            if let Ok(settings) = app.state::<AppState>().store.load() {
                if settings.enable_tray {
                    tray::setup_tray(app);
                }
            }

            if setup_remote_pilot_state.remaining().is_some() {
                let app_handle = app.handle().clone();
                let pilot_state = setup_remote_pilot_state.clone();
                let base_title = title.clone();
                std::thread::spawn(move || loop {
                    let Some(remaining) = pilot_state.remaining() else {
                        break;
                    };
                    let secs = remaining.as_secs();
                    if let Some(window) = app_handle.get_webview_window("main") {
                        let _ = window.set_title(&format!("{base_title} — closing in {secs}s"));
                    }
                    if secs == 0 {
                        let state = app_handle.state::<AppState>();
                        let history_debug = format!("{:?}", state.history.lock().unwrap());
                        let review_log_debug = format!("{:?}", state.review_log.lock().unwrap());
                        let dump_path = remote_pilot::dump_session_log(
                            &pilot_state,
                            &history_debug,
                            &review_log_debug,
                        );
                        eprintln!(
                            "[remote-pilot] timeout reached — session log dumped to {}",
                            dump_path.display()
                        );
                        // Tauri (and its internal tokio runtime) don't tolerate `exit()`
                        // called from an arbitrary OS thread -- observed panic: "Cannot
                        // drop a runtime in a context where blocking is not allowed."
                        // Dispatch onto the main/event-loop thread instead.
                        let exit_handle = app_handle.clone();
                        let _ = app_handle.run_on_main_thread(move || exit_handle.exit(0));
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_secs(1));
                });
            }

            Ok(())
        })
        .invoke_handler(specta_builder.invoke_handler())
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
