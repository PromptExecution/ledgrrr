//! `ledgrrr-mcp` — the Claude Desktop MCPB stdio controller binary.
//!
//! Hand-rolled JSON-RPC-over-stdio loop, mirroring
//! `crates/ledgerr-mcp/src/bin/ledgerr-mcp-server.rs` so the two MCP
//! surfaces (`ledgerr_*` domain tools, `ledgrrr_*` desktop controller tools)
//! stay structurally consistent. See PRD-11 §3.1 for the tool contract and
//! §7 for why mutating tools here return plans rather than executing
//! privileged actions directly.

use std::io::{self, BufRead, Write};

use ledgerr_desktop_agent::contract;
use serde_json::{json, Value};

fn main() {
    if std::env::args().any(|arg| arg == "--once") {
        let mut input = String::new();
        if std::io::stdin().read_line(&mut input).is_ok() {
            if let Ok(request) = serde_json::from_str::<Value>(&input) {
                if let Some(response) = handle_request(request) {
                    if let Ok(serialized) = serde_json::to_string(&response) {
                        println!("{serialized}");
                    }
                }
            }
        }
        return;
    }
    serve(io::stdin().lock(), io::stdout());
}

fn serve<R: BufRead, W: Write>(reader: R, mut writer: W) {
    for line in reader.lines() {
        let Ok(raw) = line else { continue };
        if raw.trim().is_empty() {
            continue;
        }
        let Ok(request) = serde_json::from_str::<Value>(&raw) else {
            continue;
        };
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
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name": "ledgrrr-mcp",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        })),
        "notifications/initialized" => None,
        "tools/list" => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "tools": contract::tool_descriptors() }
        })),
        "tools/call" => {
            let params = request.get("params").cloned().unwrap_or(Value::Null);
            let tool_name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
            match contract::dispatch(tool_name, &arguments) {
                Ok(result) => Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "content": [{ "type": "text", "text": result.to_string() }],
                        "structuredContent": result
                    }
                })),
                Err(err) => Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": { "code": -32000, "message": err.to_string() }
                })),
            }
        }
        _ => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32601, "message": format!("method not found: {method}") }
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_list_returns_all_eleven_tools() {
        let response = handle_request(json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/list"
        }))
        .expect("response");
        let tools = response["result"]["tools"].as_array().expect("tools array");
        assert_eq!(tools.len(), contract::TOOL_REGISTRY.len());
    }

    #[test]
    fn status_call_round_trips_through_json_rpc() {
        let response = handle_request(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": { "name": contract::STATUS_TOOL, "arguments": {} }
        }))
        .expect("response");
        assert!(
            response.get("error").is_none(),
            "unexpected error: {response:?}"
        );
        assert!(response["result"]["structuredContent"]["b00t"].is_object());
    }

    #[test]
    fn unknown_tool_returns_json_rpc_error() {
        let response = handle_request(json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": { "name": "not_a_real_tool", "arguments": {} }
        }))
        .expect("response");
        assert_eq!(response["error"]["code"], -32000);
    }
}
