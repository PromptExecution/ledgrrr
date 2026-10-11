// DEPRECATED, REFERENCE ONLY — not part of the build (no [[bin]] entry,
// and this directory is outside Cargo's src/bin/*.rs auto-discovery).
// Superseded by host-tauri.exe, whose Windows tray now uses the same
// tray::runtime::run() this binary used. Kept for reference only; may
// bit-rot as the rest of the crate changes (e.g. run()'s signature grew a
// second `show_window` parameter after this file stopped compiling).
//
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = ledgerr_host::settings_client::SettingsClient::new();
    ledgerr_host::tray::runtime::run(client)
}

#[cfg(not(windows))]
fn main() {
    eprintln!("host-tray is currently supported on Windows builds only");
    std::process::exit(1);
}
