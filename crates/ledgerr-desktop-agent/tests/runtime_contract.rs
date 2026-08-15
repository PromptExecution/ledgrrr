//! End-to-end contract for the durable local runtime boundary.

use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ledgerr_desktop_agent::{runtime_client, state};

#[test]
fn runtime_requires_authenticated_health_and_stops_gracefully() {
    let dir = std::env::temp_dir().join(format!(
        "ledgrrr-runtime-test-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    std::fs::create_dir_all(&dir).expect("create runtime state directory");
    std::env::set_var("LEDGRRR_STATE_DIR", &dir);
    let mut child = Command::new(env!("CARGO_BIN_EXE_ledgrrr-service"))
        .env("LEDGRRR_STATE_DIR", &dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start runtime");

    let mut health = None;
    for _ in 0..30 {
        match runtime_client::health() {
            Ok(found) => {
                health = Some(found);
                break;
            }
            Err(_) => thread::sleep(Duration::from_millis(100)),
        }
    }
    let health = health.expect("runtime must become ready with a descriptor and bearer token");
    assert!(health.ready);
    assert_eq!(health.pid, child.id());
    assert_eq!(health.mode, "per_user");

    let stopped = runtime_client::stop().expect("authenticated shutdown");
    assert_eq!(stopped.pid, child.id());
    let exit = child.wait().expect("wait for runtime");
    assert!(exit.success());
    assert!(state::read_runtime_descriptor().is_none());
    std::env::remove_var("LEDGRRR_STATE_DIR");
    std::fs::remove_dir_all(dir).expect("remove runtime test state");
}
