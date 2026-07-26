use std::io::{self, BufRead, Write};

use serde_json::{json, Value};

fn main() {
    serve(io::stdin().lock(), io::stdout());
}

fn serve<R: BufRead, W: Write>(reader: R, mut writer: W) {
    for line in reader.lines() {
        let Ok(raw) = line else { continue };
        let Ok(request) = serde_json::from_str::<Value>(&raw) else { continue };
        if let Some(response) = handle_request(request) {
            if let Ok(serialized) = serde_json::to_string(&response) {
                let _ = writeln!(writer, "{serialized}");
                let _ = writer.flush();
            }
        }
    }
}

fn handle_request(request: Value) -> Option<Value> {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");

    match method {
        "initialize" => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "ledgerr-desktop-agent",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        })),
        "notifications/initialized" => None,
        "tools/list" => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "tools": [
                    {
                        "name": "desktop_status",
                        "description": "Report desktop agent status, version, and uptime",
                        "inputSchema": {
                            "type": "object",
                            "properties": {}
                        }
                    },
                    {
                        "name": "desktop_ping",
                        "description": "Health check — returns pong with timestamp",
                        "inputSchema": {
                            "type": "object",
                            "properties": {}
                        }
                    }
                ]
            }
        })),
        "tools/call" => {
            let params = request.get("params").cloned().unwrap_or(Value::Null);
            let tool_name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let result = match tool_name {
                "desktop_status" => json!({
                    "agent": "ledgerr-desktop-agent",
                    "version": env!("CARGO_PKG_VERSION"),
                    "status": "running",
                    "transport": "stdio",
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }),
                "desktop_ping" => json!({
                    "pong": true,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }),
                _ => json!({
                    "isError": true,
                    "content": [{
                        "type": "text",
                        "text": format!("unknown tool: {tool_name}")
                    }]
                }),
            };
            Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": result
            }))
        }
        _ => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "isError": true,
                "content": [{
                    "type": "text",
                    "text": format!("method not found: {method}")
                }]
            }
        })),
    }
}
