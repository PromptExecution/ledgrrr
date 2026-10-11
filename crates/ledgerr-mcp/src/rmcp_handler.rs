//! rmcp `ServerHandler` wrapper for `TurboLedgerService`.
//!
//! Bridges the existing `mcp_adapter::handle_request` / schema machinery onto
//! the official rmcp SDK. Both transports (stdio via `transport-io`, SSE via
//! `transport-sse-server`) are available under the `rmcp-transport` feature.
//!
//! Migration guide: replace the hand-rolled `serve()` loop in
//! `ledgerr-mcp-server.rs` with `LedgrrHandler::new(service).serve_stdio()`.

#![cfg(feature = "rmcp-transport")]

use std::sync::Arc;

use rmcp::handler::server::ServerHandler;
use rmcp::model::{
    CallToolRequestParam, CallToolResult, Content, InitializeResult, ListToolsResult,
    PaginatedRequestParam, ServerCapabilities, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{Error as McpError, RoleServer};
use serde_json::{Map, Value};

use crate::mcp_adapter;
use crate::TurboLedgerService;

/// rmcp `ServerHandler` implementation backed by `TurboLedgerService`.
#[derive(Clone)]
pub struct LedgrrHandler {
    service: Arc<TurboLedgerService>,
    /// Cached tool list (name, description, schema). Built once on construction.
    tools: Arc<Vec<(String, String, Map<String, Value>)>>,
}

impl LedgrrHandler {
    pub fn new(service: Arc<TurboLedgerService>) -> Self {
        let tools = build_tool_list();
        Self {
            service,
            tools: Arc::new(tools),
        }
    }
}

/// Build tool descriptors from `mcp_adapter::tool_descriptors()` once.
/// Returns (name, description, schema) triples.
fn build_tool_list() -> Vec<(String, String, Map<String, Value>)> {
    mcp_adapter::tool_descriptors()
        .into_iter()
        .filter_map(|tool| {
            let name = tool.get("name")?.as_str()?.to_string();
            let description = tool
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let schema = tool.get("inputSchema")?.as_object()?.clone();
            Some((name, description, schema))
        })
        .collect()
}

impl ServerHandler for LedgrrHandler {
    fn get_info(&self) -> InitializeResult {
        InitializeResult {
            protocol_version: Default::default(),
            capabilities: ServerCapabilities {
                tools: Some(rmcp::model::ToolsCapability {
                    list_changed: Some(false),
                }),
                ..Default::default()
            },
            server_info: rmcp::model::Implementation {
                name: "ledgerr-mcp".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            instructions: Some(
                "ledgrrr financial document intelligence MCP server. \
                 Use tools/list to discover available tools."
                    .into(),
            ),
        }
    }

    async fn list_tools(
        &self,
        _request: PaginatedRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let tools: Vec<Tool> = self
            .tools
            .iter()
            .map(|(name, description, schema)| {
                Tool::new(name.clone(), description.clone(), schema.clone())
            })
            .collect();
        Ok(ListToolsResult {
            tools,
            next_cursor: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let tool_name = request.name.as_ref();
        let arguments: Value = request
            .arguments
            .map(Value::Object)
            .unwrap_or(Value::Null);

        // Route through the existing mcp_adapter dispatch.
        let result = mcp_adapter::dispatch_tool(&self.service, tool_name, &arguments);

        // Convert the {content:[{type:"text",text:"…"}], isError:bool} envelope
        // to rmcp's CallToolResult.
        let is_error = result
            .get("isError")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let content: Vec<Content> = result
            .get("content")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        // Only "text" content items are produced by the existing adapter.
                        if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                            let text = item
                                .get("text")
                                .and_then(|t| t.as_str())
                                .unwrap_or("")
                                .to_string();
                            Some(Content::text(text))
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_else(|| {
                vec![Content::text(
                    serde_json::to_string(&result).unwrap_or_default(),
                )]
            });

        if is_error {
            Ok(CallToolResult::error(content))
        } else {
            Ok(CallToolResult::success(content))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_tool_list_is_non_empty() {
        let tools = build_tool_list();
        assert!(
            !tools.is_empty(),
            "tool_descriptors() must return at least one tool"
        );
        for (name, _desc, schema) in &tools {
            assert!(!name.is_empty(), "tool name must not be empty");
            assert!(!schema.is_empty(), "tool schema must not be empty");
        }
    }
}
