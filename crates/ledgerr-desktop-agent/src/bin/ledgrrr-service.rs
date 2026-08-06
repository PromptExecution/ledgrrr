//! `ledgrrr-service` — Phase 1 user-level long-lived background process.
//!
//! This is intentionally not a real OS service (no systemd unit, no Windows
//! Service Control Manager registration — PRD-11 §3.2 assigns that to the
//! native installer, not built yet). It exists so `ledgrrr_start_service` /
//! `ledgrrr_stop_service` / `ledgrrr_status` have something real to control
//! and observe: it writes a heartbeat file on a fixed interval.
//!
//! No signal handler is installed on purpose: `status::detect_service`
//! checks both heartbeat freshness *and* whether the pid is still alive
//! (via `sysinfo`), so a `kill`'d process is reported stopped immediately
//! on the next status check, before the heartbeat would even go stale.
//! Catching SIGTERM to delete the heartbeat file a few seconds earlier
//! isn't worth doing unsafe, signal-unsafe file I/O for.

use ledgerr_desktop_agent::{settings_server, state};
use ledgrrr_settings::{default_settings_path, SettingsStore};
use std::time::{Duration, Instant};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(25);

fn main() {
    let pid = std::process::id();
    let started_at = state::now();
    let store = SettingsStore::new(default_settings_path());

    let listener = match settings_server::bind() {
        Ok(listener) => Some(listener),
        Err(error) => {
            eprintln!(
                "ledgrrr-service: failed to bind settings server on {}: {error} — heartbeat only, no settings HTTP surface this run",
                settings_server::SETTINGS_SERVER_ADDR
            );
            None
        }
    };

    let mut last_heartbeat = Instant::now() - HEARTBEAT_INTERVAL;
    loop {
        if last_heartbeat.elapsed() >= HEARTBEAT_INTERVAL {
            let _ = state::write_heartbeat(pid, started_at);
            last_heartbeat = Instant::now();
        }
        if let Some(listener) = &listener {
            settings_server::accept_once(listener, &store);
        }
        std::thread::sleep(ACCEPT_POLL_INTERVAL);
    }
}
