//! `ledgrrr-service` — durable local runtime with authenticated loopback IPC.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use ledgerr_desktop_agent::state;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);

fn token() -> Result<String, String> {
    let mut entropy = [0_u8; 32];
    getrandom::fill(&mut entropy)
        .map_err(|error| format!("OS random source unavailable: {error}"))?;
    Ok(blake3::hash(&entropy).to_hex().to_string())
}

fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
}

fn handle_connection(mut stream: TcpStream, bearer_token: &str, pid: u32, mode: &str) -> bool {
    let mut request = [0_u8; 8192];
    let Ok(read) = stream.read(&mut request) else {
        return false;
    };
    let request = String::from_utf8_lossy(&request[..read]);
    let authorized = request
        .lines()
        .any(|line| line == format!("Authorization: Bearer {bearer_token}"));
    if !authorized {
        respond(&mut stream, "401 Unauthorized", r#"{"ready":false}"#);
        return false;
    }

    let health = serde_json::json!({
        "ready": true,
        "pid": pid,
        "mode": mode,
        "schema_version": 1
    })
    .to_string();
    if request.starts_with("GET /health ") {
        respond(&mut stream, "200 OK", &health);
        false
    } else if request.starts_with("POST /shutdown ") {
        respond(&mut stream, "200 OK", &health);
        true
    } else {
        respond(&mut stream, "404 Not Found", r#"{"ready":false}"#);
        false
    }
}

fn main() {
    let pid = std::process::id();
    let started_at = state::now();
    let mode = std::env::var("LEDGRRR_RUNTIME_MODE").unwrap_or_else(|_| "per_user".to_string());
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) => {
            state::audit("runtime", "start", "failed", error.to_string());
            return;
        }
    };
    let endpoint = match listener.local_addr() {
        Ok(address) => address.to_string(),
        Err(error) => {
            state::audit("runtime", "start", "failed", error.to_string());
            return;
        }
    };
    let bearer_token = match token() {
        Ok(token) => token,
        Err(error) => {
            state::audit("runtime", "start", "failed", error);
            return;
        }
    };
    let descriptor = state::RuntimeDescriptor {
        schema_version: 1,
        pid,
        endpoint,
        bearer_token: bearer_token.clone(),
        started_at_unix: started_at,
        mode: mode.clone(),
    };
    if state::write_runtime_descriptor(&descriptor).is_err() {
        return;
    }
    let _ = listener.set_nonblocking(true);
    state::audit("runtime", "start", "ok", format!("mode={mode}"));

    let mut last_heartbeat = std::time::Instant::now() - HEARTBEAT_INTERVAL;
    loop {
        if last_heartbeat.elapsed() >= HEARTBEAT_INTERVAL {
            let _ = state::write_heartbeat(pid, started_at);
            last_heartbeat = std::time::Instant::now();
        }
        match listener.accept() {
            Ok((stream, _)) => {
                if handle_connection(stream, &bearer_token, pid, &mode) {
                    break;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(error) => {
                state::audit("runtime", "accept", "failed", error.to_string());
                break;
            }
        }
    }
    state::remove_runtime_descriptor();
    state::remove_heartbeat();
    state::audit("runtime", "stop", "ok", "runtime stopped");
}
