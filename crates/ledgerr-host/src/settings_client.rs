//! HTTP client for `ledgrrr-service`'s settings endpoint (see
//! `ledgerr_desktop_agent::settings_server` for the server side). Replaces
//! the local `SettingsStore` this binary used to own directly — settings
//! are now `ledgrrr-service`'s state, not `host-tauri`'s.

use ledgrrr_settings::AppSettings;

const DEFAULT_SETTINGS_SERVER_URL: &str = "http://127.0.0.1:15116";

#[derive(Debug, thiserror::Error)]
pub enum SettingsClientError {
    #[error("request to ledgrrr-service failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("ledgrrr-service returned an error: {0}")]
    Server(String),
}

pub struct SettingsClient {
    base_url: String,
    client: reqwest::blocking::Client,
}

impl SettingsClient {
    pub fn new() -> Self {
        Self::with_base_url(DEFAULT_SETTINGS_SERVER_URL.to_string())
    }

    pub fn with_base_url(base_url: String) -> Self {
        Self {
            base_url,
            client: reqwest::blocking::Client::new(),
        }
    }

    pub fn load(&self) -> Result<AppSettings, SettingsClientError> {
        let response = self
            .client
            .get(format!("{}/settings", self.base_url))
            .send()?;
        if !response.status().is_success() {
            return Err(SettingsClientError::Server(response.status().to_string()));
        }
        Ok(response.json()?)
    }

    pub fn save(&self, settings: &AppSettings) -> Result<(), SettingsClientError> {
        let response = self
            .client
            .post(format!("{}/settings", self.base_url))
            .json(settings)
            .send()?;
        if !response.status().is_success() {
            return Err(SettingsClientError::Server(response.status().to_string()));
        }
        Ok(())
    }
}

impl Default for SettingsClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// Minimal fake server: accepts exactly one connection, replies with a
    /// fixed body, then stops. Enough to test the client without depending
    /// on ledgerr-desktop-agent (which would create a dependency cycle risk
    /// — ledgerr-host must not depend on ledgerr-desktop-agent).
    fn fake_server_returning(body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0_u8; 1024];
            let _ = stream.read(&mut buf);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
        });
        format!("http://{addr}")
    }

    #[test]
    fn load_parses_settings_from_server_response() {
        let defaults = AppSettings::default();
        let body = serde_json::to_string(&defaults).unwrap();
        let base_url = fake_server_returning(Box::leak(body.into_boxed_str()));
        let client = SettingsClient::with_base_url(base_url);
        let loaded = client.load().unwrap();
        assert_eq!(loaded, defaults);
    }
}
