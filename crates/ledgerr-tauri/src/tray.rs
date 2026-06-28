//! Tray setup for the ledgerr-tauri desktop host.
//!
//! Platform dispatch:
//! - **Windows**: Uses the native `Shell_NotifyIconW` tray via `ledgerr_host::tray::native`.
//!   The native tray spawns its own hidden window + message pump thread, so we just
//!   forward menu events (Show Window, Quit) to the Tauri `AppHandle`.
//! - **Linux / macOS**: Uses Tauri's built-in `TrayIconBuilder` + `MenuBuilder` API,
//!   which handles platform differences internally.

use std::time::Duration;
use tauri::Emitter;

/// Setup the system tray icon for the application.
///
/// Call this during `tauri::Builder::default().setup()`.
#[cfg(windows)]
pub fn setup_tray(app: &tauri::App) {
    use ledgerr_host::tray::native::{
        make_icon_data, NativeTrayPlatform, TrayEvent, CMD_EXIT, CMD_SHOW_WINDOW,
    };
    use ledgerr_host::tray::TrayMenuLabels;

    let app_handle = app.handle().clone();
    let (rgba, width, height) = make_icon_data();

    // Build labels for the native tray menu.
    // Most notification-specific items are left empty since this is a minimal
    // tray surface — Show Window and Exit are the active commands.
    let labels = TrayMenuLabels {
        version: format!("Version: {}", env!("CARGO_PKG_VERSION")),
        show_window: "Show Window",
        exit: "Exit",
        // Default/placeholder labels for notification-related items
        backend: String::new(),
        cycle_backend: "",
        last_test: String::new(),
        toast_enabled: "",
        start_minimized_to_tray: "",
        window_visible_on_start: "",
        notify_approval_required: "",
        notify_transaction_submitted: "",
        notify_run_failed: "",
        notify_run_completed: "",
        test_toast: "",
        status: String::new(),
    };

    // Spawn the native tray on a background thread.
    // The tray has its own Win32 message pump and forwards events via mpsc channel.
    std::thread::spawn(move || {
        match NativeTrayPlatform::spawn(
            &format!("l3dg3rr {}", env!("CARGO_PKG_VERSION")),
            rgba,
            width,
            height,
            &labels,
        ) {
            Ok(mut tray) => loop {
                if let Ok(event) = tray.event_rx.recv_timeout(Duration::from_millis(250)) {
                    match event {
                        TrayEvent::MenuCommand(cmd) => match cmd {
                            CMD_SHOW_WINDOW => {
                                if let Some(window) = app_handle.get_window("main") {
                                    let _ = window.show();
                                    let _ = window.set_focus();
                                }
                            }
                            CMD_EXIT => {
                                app_handle.exit(0);
                            }
                            _ => {}
                        },
                    }
                }
            },
            Err(e) => {
                eprintln!("[tray] Failed to create native tray: {e}");
            }
        }
    });
}

/// Setup the system tray icon on non-Windows platforms (macOS, Linux).
#[cfg(not(windows))]
pub fn setup_tray(app: &tauri::App) {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::TrayIconBuilder;

    let version_text = format!("Version: {}", env!("CARGO_PKG_VERSION"));

    let show = MenuItem::with_id(app, "show", "Show Window", true, None::<&str>)
        .expect("failed to build Show Window menu item");
    let version = MenuItem::with_id(app, "version", &version_text, false, None::<&str>)
        .expect("failed to build Version menu item");
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)
        .expect("failed to build Settings menu item");
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)
        .expect("failed to build Quit menu item");

    let menu = Menu::with_items(app, &[&show, &version, &settings, &quit])
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
