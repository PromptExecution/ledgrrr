//! Authenticated loopback client used by the controller to check and stop the
//! local runtime.  The endpoint never accepts unauthenticated requests.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::state;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeHealth {
    pub ready: bool,
    pub pid: u32,
    pub mode: String,
    pub schema_version: u8,
}

fn request(method: &str, path: &str) -> Result<RuntimeHealth, String> {
    let descriptor = state::read_runtime_descriptor()
        .ok_or_else(|| "runtime descriptor is missing".to_string())?;
    let mut stream = TcpStream::connect(&descriptor.endpoint)
        .map_err(|error| format!("runtime endpoint unavailable: {error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| error.to_string())?;
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nConnection: close\r\n\r\n",
        descriptor.bearer_token
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("runtime request failed: {error}"))?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|error| format!("runtime response failed: {error}"))?;
    let (head, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| "runtime returned malformed HTTP".to_string())?;
    if !head.starts_with("HTTP/1.1 200") {
        return Err(format!("runtime rejected {method} {path}: {head}"));
    }
    serde_json::from_str(body).map_err(|error| format!("invalid runtime health: {error}"))
}

pub fn health() -> Result<RuntimeHealth, String> {
    request("GET", "/health")
}

pub fn stop() -> Result<RuntimeHealth, String> {
    request("POST", "/shutdown")
}
