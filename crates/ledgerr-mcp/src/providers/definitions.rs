use std::path::PathBuf;
use std::sync::Arc;

use serde_json::Value;

use crate::provider::{
    BoxedProvider, HttpMcpProvider, McpProvider, ProviderInfo, ProviderResult, StdioMcpProvider,
    ToolDescriptor,
};

pub struct B00tProvider {
    inner: BoxedProvider,
    b00t_home: PathBuf,
}

impl B00tProvider {
    pub fn new(b00t_home: Option<PathBuf>) -> ProviderResult<Self> {
        let home = b00t_home.unwrap_or_else(|| {
            let p = PathBuf::from(
                std::env::var("B00T_HOME")
                    .or_else(|_| std::env::var("HOME").map(|h| format!("{h}/.b00t")))
                    .unwrap_or_else(|_| "~/.b00t".to_string()),
            );
            if p.starts_with("~") {
                shellexpand::tilde(&p.to_string_lossy()).as_ref().into()
            } else {
                p
            }
        });
        let inner: BoxedProvider = Arc::new(StdioMcpProvider::new(
            &home.join("target/release/b00t-mcp").to_string_lossy(),
            &["--stdio".to_string()],
        )?);
        Ok(Self {
            inner,
            b00t_home: home,
        })
    }

    pub fn b00t_home(&self) -> &PathBuf {
        &self.b00t_home
    }
}

impl McpProvider for B00tProvider {
    fn name(&self) -> &str {
        "b00t"
    }

    fn initialize(&self) -> ProviderResult<crate::provider::ProviderInfo> {
        self.inner.initialize()
    }

    fn call_tool(&self, name: &str, arguments: Value) -> ProviderResult<Value> {
        self.inner.call_tool(name, arguments)
    }

    fn shutdown(&self) {
        self.inner.shutdown();
    }
}

pub struct JustProvider {
    inner: BoxedProvider,
}

impl JustProvider {
    pub fn new(project_root: Option<PathBuf>) -> ProviderResult<Self> {
        let _root = project_root.unwrap_or_else(|| {
            PathBuf::from(
                std::env::var("CARGO_MANIFEST_DIR")
                    .or_else(|_| std::env::var("PWD"))
                    .unwrap_or_else(|_| ".".to_string()),
            )
        });

        let inner: BoxedProvider = Arc::new(StdioMcpProvider::new("just", &["--mcp".to_string()])?);
        Ok(Self { inner })
    }
}

impl McpProvider for JustProvider {
    fn name(&self) -> &str {
        "just"
    }

    fn initialize(&self) -> ProviderResult<crate::provider::ProviderInfo> {
        self.inner.initialize()
    }

    fn call_tool(&self, name: &str, arguments: Value) -> ProviderResult<Value> {
        self.inner.call_tool(name, arguments)
    }

    fn shutdown(&self) {
        self.inner.shutdown();
    }
}

pub struct Ir0ntologyProvider {
    inner: BoxedProvider,
}

impl Ir0ntologyProvider {
    pub fn new(ir0ntology_home: Option<PathBuf>) -> ProviderResult<Self> {
        let home = ir0ntology_home.unwrap_or_else(|| {
            PathBuf::from(
                std::env::var("IRONTOLOGY_HOME")
                    .unwrap_or_else(|_| "/usr/local/bin/ir0ntology-mcp".to_string()),
            )
        });

        let inner: BoxedProvider = Arc::new(StdioMcpProvider::new(
            &home.to_string_lossy(),
            &["--stdio".to_string()],
        )?);
        Ok(Self { inner })
    }
}

impl McpProvider for Ir0ntologyProvider {
    fn name(&self) -> &str {
        "ir0ntology"
    }

    fn initialize(&self) -> ProviderResult<crate::provider::ProviderInfo> {
        self.inner.initialize()
    }

    fn call_tool(&self, name: &str, arguments: Value) -> ProviderResult<Value> {
        self.inner.call_tool(name, arguments)
    }

    fn shutdown(&self) {
        self.inner.shutdown();
    }
}

pub struct OpenMetadataProvider {
    inner: BoxedProvider,
}

impl OpenMetadataProvider {
    pub const TOOL_PREFIX: &'static str = "openmetadata__";

    pub fn from_env() -> Option<ProviderResult<Self>> {
        let endpoint = std::env::var("OPENMETADATA_MCP_URL")
            .or_else(|_| std::env::var("OPENMETADATA_URL"))
            .ok()?;
        let token = std::env::var("OPENMETADATA_MCP_BEARER_TOKEN")
            .or_else(|_| std::env::var("OPENMETADATA_JWT_TOKEN"))
            .ok();
        Some(Self::new(endpoint, token))
    }

    pub fn new(endpoint: impl Into<String>, bearer_token: Option<String>) -> ProviderResult<Self> {
        let inner: BoxedProvider = Arc::new(HttpMcpProvider::new(
            "openmetadata",
            endpoint.into(),
            bearer_token,
        )?);
        Ok(Self { inner })
    }

    fn prefixed_tool_name(tool_name: &str) -> String {
        format!("{}{tool_name}", Self::TOOL_PREFIX)
    }

    fn remote_tool_name(tool_name: &str) -> &str {
        tool_name
            .strip_prefix(Self::TOOL_PREFIX)
            .unwrap_or(tool_name)
    }
}

impl McpProvider for OpenMetadataProvider {
    fn name(&self) -> &str {
        "openmetadata"
    }

    fn initialize(&self) -> ProviderResult<ProviderInfo> {
        let mut info = self.inner.initialize()?;
        info.name = self.name().to_string();
        info.tools = info
            .tools
            .into_iter()
            .map(|tool| ToolDescriptor {
                name: Self::prefixed_tool_name(&tool.name),
                input_schema: tool.input_schema,
            })
            .collect();
        Ok(info)
    }

    fn call_tool(&self, name: &str, arguments: Value) -> ProviderResult<Value> {
        self.inner
            .call_tool(Self::remote_tool_name(name), arguments)
    }

    fn shutdown(&self) {
        self.inner.shutdown();
    }
}

pub fn register_default_providers(
    registry: &mut crate::provider::McpProviderRegistry,
    b00t_home: Option<PathBuf>,
    project_root: Option<PathBuf>,
) {
    match JustProvider::new(project_root) {
        Ok(p) => {
            registry.register(Arc::new(p) as BoxedProvider);
        }
        Err(e) => {
            tracing::warn!("just mcp provider unavailable: {e}");
        }
    }

    match B00tProvider::new(b00t_home) {
        Ok(p) => {
            registry.register(Arc::new(p) as BoxedProvider);
        }
        Err(e) => {
            tracing::warn!("b00t mcp provider unavailable: {e}");
        }
    }

    match Ir0ntologyProvider::new(None) {
        Ok(p) => {
            registry.register(Arc::new(p) as BoxedProvider);
        }
        Err(e) => {
            tracing::warn!("ir0ntology mcp provider unavailable: {e}");
        }
    }

    if let Some(provider) = OpenMetadataProvider::from_env() {
        match provider {
            Ok(p) => {
                registry.register(Arc::new(p) as BoxedProvider);
            }
            Err(e) => {
                tracing::warn!("openmetadata mcp provider unavailable: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_b00t_provider_name() {
        let provider = B00tProvider::new(Some(PathBuf::from("/tmp/fake-b00t")));
        match provider {
            Ok(p) => assert_eq!(p.name(), "b00t"),
            Err(e) => {
                // Acceptable: real process not available in test env
                assert!(
                    e.to_string().contains("spawn failed")
                        || e.to_string().contains("No such file")
                        || e.to_string().contains("not found"),
                    "unexpected error: {e}"
                );
            }
        }
    }

    #[test]
    fn test_just_provider_name() {
        let provider = JustProvider::new(Some(PathBuf::from("/tmp")));
        match provider {
            Ok(p) => assert_eq!(p.name(), "just"),
            Err(e) => {
                assert!(
                    e.to_string().contains("spawn failed") || e.to_string().contains("not found"),
                    "unexpected error: {e}"
                );
            }
        }
    }

    #[test]
    fn test_ir0ntology_provider_name() {
        let provider = Ir0ntologyProvider::new(Some(PathBuf::from("/tmp/fake-mcp-binary")));
        match provider {
            Ok(p) => assert_eq!(p.name(), "ir0ntology"),
            Err(e) => {
                assert!(
                    e.to_string().contains("spawn failed")
                        || e.to_string().contains("No such file")
                        || e.to_string().contains("not found"),
                    "unexpected error: {e}"
                );
            }
        }
    }

    #[test]
    fn test_register_default_does_not_panic() {
        let mut registry = crate::provider::McpProviderRegistry::new();
        // Should not panic even when providers are unavailable
        register_default_providers(
            &mut registry,
            Some(PathBuf::from("/tmp/fake-b00t")),
            Some(PathBuf::from("/tmp")),
        );
        // Registry may be empty if none of the subprocesses exist
        let results = registry.initialize_all();
        // This is fine — all providers may fail to spawn
        assert!(results.len() <= 3);
    }

    #[test]
    fn test_registry_with_defaults_graceful_degradation() {
        let mut registry = crate::provider::McpProviderRegistry::new();
        register_default_providers(
            &mut registry,
            Some(PathBuf::from("/nonexistent/b00t-home")),
            Some(PathBuf::from("/nonexistent/project")),
        );
        // Should always return without panic
        for (name, result) in registry.initialize_all() {
            if let Err(e) = result {
                // Expected: any error indicating graceful degradation
                let msg = e.to_string();
                assert!(!msg.is_empty(), "unexpected empty error for {name}: {e}");
            }
        }
    }
}
