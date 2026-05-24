use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProviderError {
    #[error("spawn failed: {0}")]
    Spawn(String),
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("json error: {0}")]
    Json(String),
}

impl Clone for ProviderError {
    fn clone(&self) -> Self {
        match self {
            Self::Spawn(s) => Self::Spawn(s.clone()),
            Self::Protocol(s) => Self::Protocol(s.clone()),
            Self::Io(s) => Self::Io(s.clone()),
            Self::Json(s) => Self::Json(s.clone()),
        }
    }
}

impl From<std::io::Error> for ProviderError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

impl From<serde_json::Error> for ProviderError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e.to_string())
    }
}

pub type ProviderResult<T> = Result<T, ProviderError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolDescriptor {
    pub name: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub name: String,
    pub version: String,
    pub tools: Vec<ToolDescriptor>,
}

pub trait McpProvider: Send + Sync {
    fn name(&self) -> &str;
    fn initialize(&self) -> ProviderResult<ProviderInfo>;
    fn call_tool(&self, name: &str, arguments: Value) -> ProviderResult<Value>;
    fn shutdown(&self);
}

struct StdinTransport {
    child: Arc<Mutex<Child>>,
}

impl StdinTransport {
    fn spawn(command: &str, args: &[String]) -> ProviderResult<Self> {
        let child = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| ProviderError::Spawn(format!("{command}: {e}")))?;
        Ok(Self {
            child: Arc::new(Mutex::new(child)),
        })
    }

    fn send_request(&self, request: &Value) -> ProviderResult<Value> {
        let mut child = self
            .child
            .lock()
            .map_err(|e| ProviderError::Protocol(format!("lock: {e}")))?;

        let raw = serde_json::to_string(request)?;

        if let Some(stdin) = child.stdin.as_mut() {
            writeln!(stdin, "{raw}")?;
            stdin.flush()?;
        } else {
            return Err(ProviderError::Protocol("no stdin".into()));
        }

        let mut line = String::new();
        if let Some(stdout) = child.stdout.as_mut() {
            let mut reader = BufReader::new(stdout);
            reader
                .read_line(&mut line)
                .map_err(|e| ProviderError::Protocol(format!("read: {e}")))?;
        } else {
            return Err(ProviderError::Protocol("no stdout".into()));
        }

        let response: Value = serde_json::from_str(&line)?;
        Ok(response)
    }
}

impl Drop for StdinTransport {
    fn drop(&mut self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub struct StdioMcpProvider {
    name: String,
    transport: StdinTransport,
    next_id: Arc<Mutex<u64>>,
}

impl StdioMcpProvider {
    pub fn new(command: &str, args: &[String]) -> ProviderResult<Self> {
        let transport = StdinTransport::spawn(command, args)?;
        Ok(Self {
            name: format!(
                "mcp-{}",
                PathBuf::from(command)
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
            ),
            transport,
            next_id: Arc::new(Mutex::new(1)),
        })
    }

    fn next_id(&self) -> u64 {
        let mut id = self.next_id.lock().unwrap_or_else(|e| e.into_inner());
        let curr = *id;
        *id += 1;
        curr
    }

    fn json_rpc(&self, method: &str, params: Option<Value>) -> ProviderResult<Value> {
        let mut request = json!({
            "jsonrpc": "2.0",
            "id": self.next_id(),
            "method": method,
        });
        if let Some(p) = params {
            request["params"] = p;
        }
        let response = self.transport.send_request(&request)?;
        if let Some(error) = response.get("error") {
            return Err(ProviderError::Protocol(format!(
                "json-rpc error: {}",
                error
            )));
        }
        Ok(response["result"].clone())
    }
}

impl McpProvider for StdioMcpProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn initialize(&self) -> ProviderResult<ProviderInfo> {
        let init_result = self.json_rpc("initialize", None)?;
        let version = init_result
            .get("serverInfo")
            .and_then(|v| v.get("version"))
            .and_then(Value::as_str)
            .unwrap_or("0.0.0")
            .to_string();

        self.json_rpc("notifications/initialized", None).ok();

        let tools_result = self.json_rpc("tools/list", None)?;
        let tools_arr = tools_result
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let tools = tools_arr
            .into_iter()
            .map(|t| ToolDescriptor {
                name: t["name"].as_str().unwrap_or("unknown").to_string(),
                input_schema: t.get("inputSchema").cloned().unwrap_or(json!({})),
            })
            .collect();

        Ok(ProviderInfo {
            name: self.name.clone(),
            version,
            tools,
        })
    }

    fn call_tool(&self, name: &str, arguments: Value) -> ProviderResult<Value> {
        let params = json!({
            "name": name,
            "arguments": arguments,
        });
        self.json_rpc("tools/call", Some(params))
    }

    fn shutdown(&self) {
        let _ = self.json_rpc("shutdown", None);
    }
}

pub struct HttpMcpProvider {
    name: String,
    endpoint: String,
    bearer_token: Option<String>,
    client: reqwest::blocking::Client,
    next_id: Arc<Mutex<u64>>,
}

impl HttpMcpProvider {
    pub fn new(
        name: impl Into<String>,
        endpoint: impl Into<String>,
        bearer_token: Option<String>,
    ) -> ProviderResult<Self> {
        let endpoint = normalize_mcp_endpoint(&endpoint.into());
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| ProviderError::Io(e.to_string()))?;
        Ok(Self {
            name: name.into(),
            endpoint,
            bearer_token,
            client,
            next_id: Arc::new(Mutex::new(1)),
        })
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    fn next_id(&self) -> u64 {
        let mut id = self.next_id.lock().unwrap_or_else(|e| e.into_inner());
        let curr = *id;
        *id += 1;
        curr
    }

    fn json_rpc(&self, method: &str, params: Option<Value>) -> ProviderResult<Value> {
        let mut request = json!({
            "jsonrpc": "2.0",
            "id": self.next_id(),
            "method": method,
        });
        if let Some(p) = params {
            request["params"] = p;
        }

        let mut builder = self.client.post(&self.endpoint).json(&request);
        if let Some(token) = self
            .bearer_token
            .as_deref()
            .filter(|token| !token.is_empty())
        {
            builder = builder.bearer_auth(token);
        }

        let response = builder
            .send()
            .map_err(|e| ProviderError::Io(e.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(ProviderError::Protocol(format!(
                "http status {} from {}",
                status, self.endpoint
            )));
        }
        let response: Value = response
            .json()
            .map_err(|e| ProviderError::Json(e.to_string()))?;
        if let Some(error) = response.get("error") {
            return Err(ProviderError::Protocol(format!(
                "json-rpc error: {}",
                error
            )));
        }
        Ok(response["result"].clone())
    }
}

impl McpProvider for HttpMcpProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn initialize(&self) -> ProviderResult<ProviderInfo> {
        let init_result = self.json_rpc(
            "initialize",
            Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {},
                    "prompts": {},
                    "resources": {
                        "subscribe": true,
                        "listChanged": true
                    }
                },
                "clientInfo": {
                    "name": "ledgrrr",
                    "version": env!("CARGO_PKG_VERSION")
                }
            })),
        )?;
        let version = init_result
            .get("serverInfo")
            .and_then(|v| v.get("version"))
            .and_then(Value::as_str)
            .unwrap_or("0.0.0")
            .to_string();

        self.json_rpc("notifications/initialized", None).ok();

        let tools_result = self.json_rpc("tools/list", None)?;
        let tools_arr = tools_result
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let tools = tools_arr
            .into_iter()
            .map(|t| ToolDescriptor {
                name: t["name"].as_str().unwrap_or("unknown").to_string(),
                input_schema: t.get("inputSchema").cloned().unwrap_or(json!({})),
            })
            .collect();

        Ok(ProviderInfo {
            name: self.name.clone(),
            version,
            tools,
        })
    }

    fn call_tool(&self, name: &str, arguments: Value) -> ProviderResult<Value> {
        self.json_rpc(
            "tools/call",
            Some(json!({
                "name": name,
                "arguments": arguments,
            })),
        )
    }

    fn shutdown(&self) {
        let _ = self.json_rpc("shutdown", None);
    }
}

pub fn normalize_mcp_endpoint(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.ends_with("/mcp") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/mcp")
    }
}

pub type BoxedProvider = Arc<dyn McpProvider + 'static>;

pub struct McpProviderRegistry {
    providers: Vec<BoxedProvider>,
}

impl McpProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    pub fn register(&mut self, provider: BoxedProvider) {
        self.providers.push(provider);
    }

    pub fn register_stdio(&mut self, command: &str, args: &[String]) -> ProviderResult<()> {
        let provider = StdioMcpProvider::new(command, args)?;
        self.register(Arc::new(provider));
        Ok(())
    }

    pub fn initialize_all(&self) -> Vec<(String, ProviderResult<ProviderInfo>)> {
        self.providers
            .iter()
            .map(|p| {
                let name = p.name().to_string();
                let result = p.initialize();
                (name, result)
            })
            .collect()
    }

    pub fn all_tool_descriptors(&self) -> Vec<ToolDescriptor> {
        let mut descriptors = Vec::new();
        for provider in &self.providers {
            if let Ok(info) = provider.initialize() {
                for tool in info.tools {
                    descriptors.push(tool);
                }
            }
        }
        descriptors
    }

    pub fn call_tool(
        &self,
        provider_name: &str,
        tool_name: &str,
        arguments: Value,
    ) -> ProviderResult<Value> {
        for provider in &self.providers {
            if provider.name() == provider_name {
                return provider.call_tool(tool_name, arguments);
            }
        }
        for provider in &self.providers {
            if let Ok(info) = provider.initialize() {
                if info.tools.iter().any(|t| t.name == tool_name) {
                    return provider.call_tool(tool_name, arguments);
                }
            }
        }
        Err(ProviderError::Protocol(format!(
            "no provider found for tool: {tool_name}"
        )))
    }

    pub fn find_provider(&self, tool_name: &str) -> Option<BoxedProvider> {
        for provider in &self.providers {
            if let Ok(info) = provider.initialize() {
                if info.tools.iter().any(|t| t.name == tool_name) {
                    return Some(Arc::clone(provider));
                }
            }
        }
        None
    }
}

impl Default for McpProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Mock provider for testing
// ============================================================================

pub mod mock {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    pub struct MockProvider {
        pub name: String,
        pub tool_name: String,
        pub tool_input_schema: Value,
        pub init_ok: bool,
        pub call_count: AtomicU64,
        pub call_result: ProviderResult<Value>,
    }

    impl MockProvider {
        pub fn new(name: &str, tool_name: &str) -> Self {
            Self {
                name: name.to_string(),
                tool_name: tool_name.to_string(),
                tool_input_schema: json!({}),
                init_ok: true,
                call_count: AtomicU64::new(0),
                call_result: Ok(json!({"status": "ok"})),
            }
        }

        pub fn failing(name: &str) -> Self {
            Self {
                name: name.to_string(),
                tool_name: "none".to_string(),
                tool_input_schema: json!({}),
                init_ok: false,
                call_count: AtomicU64::new(0),
                call_result: Err(ProviderError::Protocol("mock failure".into())),
            }
        }
    }

    impl McpProvider for MockProvider {
        fn name(&self) -> &str {
            &self.name
        }

        fn initialize(&self) -> ProviderResult<ProviderInfo> {
            if !self.init_ok {
                return Err(ProviderError::Protocol("mock init failure".into()));
            }
            Ok(ProviderInfo {
                name: self.name.clone(),
                version: "0.0.0-test".into(),
                tools: vec![ToolDescriptor {
                    name: self.tool_name.clone(),
                    input_schema: self.tool_input_schema.clone(),
                }],
            })
        }

        fn call_tool(&self, _name: &str, _arguments: Value) -> ProviderResult<Value> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            self.call_result.as_ref().map_err(|e| e.clone()).cloned()
        }

        fn shutdown(&self) {}
    }
}

#[cfg(test)]
mod tests {
    use super::mock::MockProvider;
    use super::*;
    use std::io::Read;
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    #[test]
    fn test_mock_provider_initialize_ok() {
        let provider = MockProvider::new("test-provider", "test-tool");
        let info = provider.initialize().expect("init should succeed");
        assert_eq!(info.name, "test-provider");
        assert_eq!(info.tools.len(), 1);
        assert_eq!(info.tools[0].name, "test-tool");
    }

    #[test]
    fn test_mock_provider_initialize_fails() {
        let provider = MockProvider::failing("dead-provider");
        let result = provider.initialize();
        assert!(result.is_err());
    }

    #[test]
    fn test_mock_provider_call_tool() {
        let provider = MockProvider::new("calc", "add");
        let result = provider.call_tool("add", json!({"a": 1, "b": 2}));
        assert!(result.is_ok());
        assert_eq!(
            provider
                .call_count
                .load(std::sync::atomic::Ordering::SeqCst),
            1
        );
    }

    #[test]
    fn test_normalize_mcp_endpoint_appends_mcp_once() {
        assert_eq!(
            normalize_mcp_endpoint("https://metadata.example.com"),
            "https://metadata.example.com/mcp"
        );
        assert_eq!(
            normalize_mcp_endpoint("https://metadata.example.com/mcp"),
            "https://metadata.example.com/mcp"
        );
        assert_eq!(
            normalize_mcp_endpoint("https://metadata.example.com/"),
            "https://metadata.example.com/mcp"
        );
    }

    #[test]
    fn test_http_mcp_provider_against_local_reference_server() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local reference MCP server");
        let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
        let request_count = Arc::new(AtomicUsize::new(0));
        let request_count_for_server = Arc::clone(&request_count);

        let server = thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = stream.expect("accept local MCP request");
                handle_reference_mcp_connection(&mut stream, &request_count_for_server);
                if request_count_for_server.load(Ordering::SeqCst) >= 4 {
                    break;
                }
            }
        });

        let provider =
            HttpMcpProvider::new("reference", endpoint, Some("unit-token".to_string())).unwrap();
        let info = provider.initialize().expect("initialize should succeed");
        assert_eq!(info.version, "0.0.1-reference");
        assert_eq!(info.tools[0].name, "search_metadata");

        let result = provider
            .call_tool("search_metadata", json!({"query": "orders"}))
            .expect("tools/call should succeed");
        assert_eq!(result["content"][0]["text"], "orders");

        server.join().expect("reference MCP server should exit");
        assert_eq!(request_count.load(Ordering::SeqCst), 4);
    }

    fn handle_reference_mcp_connection(stream: &mut TcpStream, request_count: &AtomicUsize) {
        loop {
            let Some((header, body)) = read_http_request(stream) else {
                break;
            };
            assert!(
                header
                    .lines()
                    .any(|line| line.eq_ignore_ascii_case("authorization: bearer unit-token")),
                "reference server should receive bearer auth"
            );
            let request: Value = serde_json::from_slice(&body).expect("valid json-rpc request");
            let method = request["method"].as_str().unwrap_or("");
            let id = request.get("id").cloned().unwrap_or(Value::Null);
            let current = request_count.fetch_add(1, Ordering::SeqCst) + 1;

            let result = match method {
                "initialize" => json!({
                    "protocolVersion": "2024-11-05",
                    "serverInfo": {
                        "name": "reference-http-mcp",
                        "version": "0.0.1-reference"
                    }
                }),
                "notifications/initialized" => json!({}),
                "tools/list" => json!({
                    "tools": [{
                        "name": "search_metadata",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "query": { "type": "string" }
                            },
                            "required": ["query"]
                        }
                    }]
                }),
                "tools/call" => {
                    assert_eq!(
                        request["params"]["name"], "search_metadata",
                        "provider should pass the remote tool name"
                    );
                    assert_eq!(
                        request["params"]["arguments"]["query"], "orders",
                        "provider should pass tool arguments"
                    );
                    json!({"content": [{"type": "text", "text": "orders"}]})
                }
                other => panic!("unexpected json-rpc method: {other}"),
            };
            write_http_json(
                stream,
                &json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": result,
                }),
            );

            if current >= 4 {
                break;
            }
        }
    }

    fn read_http_request(stream: &mut TcpStream) -> Option<(String, Vec<u8>)> {
        let mut header = Vec::new();
        let mut buf = [0u8; 1];
        while !header.ends_with(b"\r\n\r\n") {
            if stream.read_exact(&mut buf).is_err() {
                return None;
            }
            header.push(buf[0]);
        }
        let header = String::from_utf8(header).expect("http header should be utf8");
        let content_length = header
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .expect("content-length header");
        let mut body = vec![0u8; content_length];
        stream.read_exact(&mut body).expect("read http body");
        Some((header, body))
    }

    fn write_http_json(stream: &mut TcpStream, body: &Value) {
        let body = serde_json::to_vec(body).expect("serialize response body");
        write!(
            stream,
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: keep-alive\r\n\r\n",
            body.len()
        )
        .expect("write response header");
        stream.write_all(&body).expect("write response body");
        stream.flush().expect("flush response");
    }

    #[test]
    fn test_registry_empty() {
        let registry = McpProviderRegistry::new();
        let results = registry.initialize_all();
        assert!(results.is_empty());
        let descriptors = registry.all_tool_descriptors();
        assert!(descriptors.is_empty());
    }

    #[test]
    fn test_registry_register_and_initialize() {
        let mut registry = McpProviderRegistry::new();
        registry.register(Arc::new(MockProvider::new("b00t", "b00t_status")));
        registry.register(Arc::new(MockProvider::new("just", "just_list")));

        let results = registry.initialize_all();
        assert_eq!(results.len(), 2);
        let ok_names: Vec<_> = results
            .iter()
            .filter(|(_, r)| r.is_ok())
            .map(|(n, _)| n.as_str())
            .collect();
        assert!(ok_names.contains(&"b00t"));
        assert!(ok_names.contains(&"just"));
    }

    #[test]
    fn test_registry_call_tool_by_name() {
        let mut registry = McpProviderRegistry::new();
        registry.register(Arc::new(MockProvider::new("b00t", "status")));
        registry.register(Arc::new(MockProvider::new("just", "list")));

        let result = registry.call_tool("b00t", "status", json!({}));
        assert!(result.is_ok());

        let result = registry.call_tool("just", "list", json!({}));
        assert!(result.is_ok());
    }

    #[test]
    fn test_registry_call_tool_by_tool_name() {
        let mut registry = McpProviderRegistry::new();
        registry.register(Arc::new(MockProvider::new("b00t", "b00t_status")));

        let result = registry.call_tool("", "b00t_status", json!({}));
        assert!(result.is_ok());
    }

    #[test]
    fn test_registry_call_unknown_tool() {
        let mut registry = McpProviderRegistry::new();
        registry.register(Arc::new(MockProvider::new("b00t", "b00t_status")));

        let result = registry.call_tool("", "nonexistent", json!({}));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("no provider found"));
    }

    #[test]
    fn test_registry_find_provider() {
        let mut registry = McpProviderRegistry::new();
        registry.register(Arc::new(MockProvider::new("b00t", "b00t_status")));

        let found = registry.find_provider("b00t_status");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name(), "b00t");

        let not_found = registry.find_provider("nonexistent");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_registry_failing_provider() {
        let mut registry = McpProviderRegistry::new();
        registry.register(Arc::new(MockProvider::failing("broken")));

        let results = registry.initialize_all();
        assert_eq!(results.len(), 1);
        assert!(results[0].1.is_err());

        let descriptors = registry.all_tool_descriptors();
        assert!(descriptors.is_empty());
    }

    #[test]
    fn test_registry_call_tool_uses_fallback_find() {
        let mut registry = McpProviderRegistry::new();
        registry.register(Arc::new(MockProvider::new("my-provider", "my-tool")));

        let result = registry.call_tool("", "my-tool", json!({}));
        assert!(
            result.is_ok(),
            "should find tool by name across providers: {:?}",
            result
        );
    }

    #[test]
    fn test_tool_descriptor_struct() {
        let desc = ToolDescriptor {
            name: "test_tool".into(),
            input_schema: json!({"type": "object"}),
        };
        assert_eq!(desc.name, "test_tool");
        assert_eq!(desc.input_schema["type"], "object");
    }

    #[test]
    fn test_provider_info_display() {
        let info = ProviderInfo {
            name: "test".into(),
            version: "1.0".into(),
            tools: vec![ToolDescriptor {
                name: "tool1".into(),
                input_schema: json!({}),
            }],
        };
        assert_eq!(info.name, "test");
        assert_eq!(info.version, "1.0");
        assert_eq!(info.tools.len(), 1);
    }

    #[test]
    fn test_provider_error_display() {
        let err = ProviderError::Spawn("command not found".into());
        assert!(err.to_string().contains("command not found"));

        let err = ProviderError::Protocol("timeout".into());
        assert!(err.to_string().contains("timeout"));
    }
}
