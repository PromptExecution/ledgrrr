//! HTTP settings server for `ledgrrr-service` — same hand-rolled,
//! nonblocking-`TcpListener` style as `ledgerr-host`'s `internal_openai.rs`
//! endpoint. GET /settings returns the current `AppSettings` as JSON;
//! POST /settings replaces them. No async runtime.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use ledgrrr_settings::{AppSettings, SettingsStore};

pub const SETTINGS_SERVER_ADDR: &str = "127.0.0.1:15116";

fn json_response(status: u16, payload: &impl serde::Serialize) -> String {
    let body = serde_json::to_string(payload)
        .unwrap_or_else(|_| "{\"error\":\"serialization failure\"}".to_string());
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "OK",
    };
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn parse_content_length(headers: &str) -> Option<usize> {
    headers.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("content-length")
            .then(|| value.trim().parse().ok())
            .flatten()
    })
}

fn route_request(raw: &[u8], store: &SettingsStore) -> String {
    let Some(header_end) = find_header_end(raw) else {
        return json_response(400, &serde_json::json!({ "error": "invalid request" }));
    };
    let headers = String::from_utf8_lossy(&raw[..header_end]);
    let request_line = headers.lines().next().unwrap_or_default();
    let body = &raw[header_end + 4..];

    if request_line.starts_with("GET /settings ") || request_line.starts_with("GET /settings HTTP") {
        return match store.load() {
            Ok(settings) => json_response(200, &settings),
            Err(error) => json_response(500, &serde_json::json!({ "error": error.to_string() })),
        };
    }

    if request_line.starts_with("POST /settings ") || request_line.starts_with("POST /settings HTTP") {
        let settings: AppSettings = match serde_json::from_slice(body) {
            Ok(settings) => settings,
            Err(error) => {
                return json_response(
                    400,
                    &serde_json::json!({ "error": format!("invalid settings body: {error}") }),
                );
            }
        };
        return match store.save(&settings) {
            Ok(()) => json_response(200, &settings),
            Err(error) => json_response(500, &serde_json::json!({ "error": error.to_string() })),
        };
    }

    json_response(404, &serde_json::json!({ "error": "not found" }))
}

fn request_complete(buffer: &[u8]) -> bool {
    let Some(header_end) = find_header_end(buffer) else {
        return false;
    };
    let headers = String::from_utf8_lossy(&buffer[..header_end]);
    let content_length = parse_content_length(&headers).unwrap_or_default();
    buffer.len() >= header_end + 4 + content_length
}

fn handle_stream(mut stream: TcpStream, store: &SettingsStore) {
    let mut buffer = Vec::with_capacity(4096);
    let mut chunk = [0_u8; 2048];
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buffer.extend_from_slice(&chunk[..n]);
                if request_complete(&buffer) {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let response = route_request(&buffer, store);
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

/// Bind the settings server and return the live listener, already set to
/// nonblocking so the caller can interleave `accept()` polling with other
/// periodic work (the heartbeat write in `ledgrrr-service`'s main loop).
pub fn bind() -> std::io::Result<TcpListener> {
    let listener = TcpListener::bind(SETTINGS_SERVER_ADDR)?;
    listener.set_nonblocking(true)?;
    Ok(listener)
}

/// Poll the listener once. Call this in a loop; returns immediately if no
/// connection is pending (`WouldBlock`) rather than blocking, so the caller
/// stays free to also run heartbeat writes on the same thread.
pub fn accept_once(listener: &TcpListener, store: &SettingsStore) {
    match listener.accept() {
        Ok((stream, _)) => handle_stream(stream, store),
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
        Err(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `SettingsStore::new`'s registry backend (on Windows) ignores its
    /// `path` argument and always targets the one fixed production key —
    /// correct for real callers, but it would make every test here share
    /// one mutable global registry key. Use an explicit `JsonFileBackend`
    /// over a tempdir instead, for genuine per-test isolation.
    fn store_with_defaults() -> SettingsStore {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        SettingsStore::with_backend(
            path.clone(),
            Box::new(ledgrrr_settings::backend::JsonFileBackend::new(path)),
        )
    }

    #[test]
    fn get_settings_returns_defaults_as_json() {
        let store = store_with_defaults();
        let response = route_request(b"GET /settings HTTP/1.1\r\n\r\n", &store);
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        let body_start = response.find("\r\n\r\n").unwrap() + 4;
        let parsed: AppSettings = serde_json::from_str(&response[body_start..]).unwrap();
        assert!(parsed.toast_enabled);
    }

    #[test]
    fn post_settings_persists_and_get_reflects_it() {
        let store = store_with_defaults();
        let mut updated = store.load().unwrap();
        updated.toast_enabled = false;
        let body = serde_json::to_string(&updated).unwrap();
        let request = format!(
            "POST /settings HTTP/1.1\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );

        let post_response = route_request(request.as_bytes(), &store);
        assert!(post_response.starts_with("HTTP/1.1 200 OK"));

        let get_response = route_request(b"GET /settings HTTP/1.1\r\n\r\n", &store);
        let body_start = get_response.find("\r\n\r\n").unwrap() + 4;
        let parsed: AppSettings = serde_json::from_str(&get_response[body_start..]).unwrap();
        assert!(!parsed.toast_enabled);
    }

    #[test]
    fn post_settings_rejects_malformed_json() {
        let store = store_with_defaults();
        let body = "{not json";
        let request = format!(
            "POST /settings HTTP/1.1\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let response = route_request(request.as_bytes(), &store);
        assert!(response.starts_with("HTTP/1.1 400 Bad Request"));
    }

    #[test]
    fn unknown_route_returns_404() {
        let store = store_with_defaults();
        let response = route_request(b"GET /nope HTTP/1.1\r\n\r\n", &store);
        assert!(response.starts_with("HTTP/1.1 404 Not Found"));
    }
}
