#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let store =
        ledgerr_host::settings::SettingsStore::new(ledgerr_host::settings::default_settings_path());
    ledgerr_host::tray::runtime::run(store, || {
        let current_exe = std::env::current_exe()?;
        let host_window = current_exe.with_file_name("host-window.exe");
        std::process::Command::new(host_window).spawn()?;
        Ok(())
    })
}

#[cfg(not(windows))]
fn main() {
    eprintln!("host-tray is currently supported on Windows builds only");
    std::process::exit(1);
}
