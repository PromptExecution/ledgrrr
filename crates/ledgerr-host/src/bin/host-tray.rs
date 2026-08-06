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
