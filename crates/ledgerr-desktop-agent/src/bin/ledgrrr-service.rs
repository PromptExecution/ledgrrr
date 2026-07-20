//! `ledgrrr-service` — Phase 1 user-level long-lived background process.
//!
//! This is intentionally not a real OS service (no systemd unit, no Windows
//! Service Control Manager registration — PRD-10 §3.2 assigns that to the
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

use std::time::Duration;

use ledgerr_desktop_agent::state;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);

fn main() {
    let pid = std::process::id();
    let started_at = state::now();

    loop {
        // Best-effort: if the state dir is unwritable, retry on the next
        // tick rather than crash a background daemon.
        let _ = state::write_heartbeat(pid, started_at);
        std::thread::sleep(HEARTBEAT_INTERVAL);
    }
}
