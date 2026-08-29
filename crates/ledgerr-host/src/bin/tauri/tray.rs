//! Tray setup for the ledgerr-tauri desktop host.
//!
//! Platform dispatch:
//! - **Windows**: Uses the native `Shell_NotifyIconW` tray via `ledgerr_host::tray::native`.
//!   The native tray spawns its own hidden window + message pump thread, so we just
//!   forward menu events (Show Window, Quit) to the Tauri `AppHandle`.
//! - **Linux / macOS**: Uses Tauri's built-in `TrayIconBuilder` + `MenuBuilder` API,
//!   which handles platform differences internally.

#[cfg(not(windows))]
use tauri::Emitter;
use tauri::Manager;

/// Setup the system tray icon for the application.
///
/// Call this during `tauri::Builder::default().setup()`.
#[cfg(windows)]
pub fn setup_tray(app: &tauri::App) {
    use ledgerr_host::settings::{default_settings_path, SettingsStore};

    let app_handle = app.handle().clone();

    std::thread::spawn(move || {
        let show_app_handle = app_handle.clone();
        let store = SettingsStore::new(default_settings_path());
        let result = ledgerr_host::tray::runtime::run(store, move || {
            if let Some(window) = show_app_handle.get_webview_window("main") {
                window
                    .show()
                    .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;
                window
                    .set_focus()
                    .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;
            }
            Ok(())
        });
        if let Err(e) = result {
            eprintln!("[tray] tray runtime exited with error: {e}");
        }
        app_handle.exit(0);
    });
}

/// Setup the system tray icon on non-Windows platforms (macOS, Linux).
#[cfg(not(windows))]
pub fn setup_tray(app: &tauri::App) {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::TrayIconBuilder;

    let version_text = format!("Version: {}", env!("CARGO_PKG_VERSION"));
    let desktop = ledgerr_desktop_agent::status::collect();
    let service_text = format!(
        "Runtime: {}{}",
        desktop.service.readiness,
        desktop
            .service
            .mode
            .as_deref()
            .map(|mode| format!(" ({mode})"))
            .unwrap_or_default()
    );
    let package_text = format!("Package: {}", desktop.desktop_package.state);
    let model_text = format!(
        "Model: {}",
        desktop
            .model_runtime
            .profile
            .as_deref()
            .unwrap_or("not configured")
    );
    let controller_text = format!(
        "Claude controller: {} tools",
        desktop.claude_controller.expected_tools
    );
    let b00t_text = if desktop.b00t.cli_found {
        format!(
            "b00t: {}",
            desktop
                .b00t
                .version
                .unwrap_or_else(|| "available".to_string())
        )
    } else {
        "b00t: not found".to_string()
    };

    let show = MenuItem::with_id(app, "show", "Show Window", true, None::<&str>)
        .expect("failed to build Show Window menu item");
    let version = MenuItem::with_id(app, "version", &version_text, false, None::<&str>)
        .expect("failed to build Version menu item");
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)
        .expect("failed to build Settings menu item");
    let service_status =
        MenuItem::with_id(app, "runtime-status", &service_text, false, None::<&str>)
            .expect("failed to build runtime status menu item");
    let package_status =
        MenuItem::with_id(app, "package-status", &package_text, false, None::<&str>)
            .expect("failed to build package status menu item");
    let model_status = MenuItem::with_id(app, "model-status", &model_text, false, None::<&str>)
        .expect("failed to build model status menu item");
    let controller_status = MenuItem::with_id(
        app,
        "controller-status",
        &controller_text,
        false,
        None::<&str>,
    )
    .expect("failed to build controller status menu item");
    let b00t_status = MenuItem::with_id(app, "b00t-status", &b00t_text, false, None::<&str>)
        .expect("failed to build b00t status menu item");
    let pending_privileged = MenuItem::with_id(
        app,
        "pending-privileged",
        "Privileged actions: use Install/Repair in Claude (plan + UAC)",
        false,
        None::<&str>,
    )
    .expect("failed to build pending privileged status menu item");
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)
        .expect("failed to build Quit menu item");

    let menu = Menu::with_items(
        app,
        &[
            &show,
            &version,
            &service_status,
            &package_status,
            &model_status,
            &controller_status,
            &b00t_status,
            &pending_privileged,
            &settings,
            &quit,
        ],
    )
    .expect("failed to build tray menu");

    TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => {
                let _ = app.emit("show_window", ());
            }
            "settings" => {
                let _ = app.emit("open-settings", ());
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .build(app)
        .expect("failed to build tray icon");
}
