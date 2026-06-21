use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ledger_core::observability::{OTelSignal, RotelEndpoint, RotelExportPlan};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::settings::ChatSettings;

pub const INTERNAL_OPENAI_ADDR: &str = "127.0.0.1:15115";
pub const INTERNAL_OPENAI_CHAT_URL: &str = "http://127.0.0.1:15115/v1/chat/completions";
pub const INTERNAL_DOCS_URL: &str = "http://127.0.0.1:15115/docs/";
pub const INTERNAL_ROTEL_HEALTH_URL: &str = "http://127.0.0.1:15115/rotel/health";
pub const INTERNAL_ROTEL_EXPORT_PLAN_URL: &str = "http://127.0.0.1:15115/rotel/export-plan";
pub const INTERNAL_ROTEL_LOGS_URL: &str = "http://127.0.0.1:15115/v1/logs";
pub const INTERNAL_ROTEL_METRICS_URL: &str = "http://127.0.0.1:15115/v1/metrics";
pub const INTERNAL_ROTEL_TRACES_URL: &str = "http://127.0.0.1:15115/v1/traces";
pub const INTERNAL_PHI_MODEL: &str = "phi-4-mini-reasoning";
pub const INTERNAL_LOCAL_API_KEY: &str = "local-tool-tray";
pub const DEFAULT_CLOUD_CHAT_URL: &str = "https://api.openai.com/v1/chat/completions";
pub const FOUNDRY_LOCAL_MODEL: &str = "phi-4-mini";
pub const FOUNDRY_LOCAL_API_KEY: &str = "local-foundry";
pub const FOUNDRY_LOCAL_DEFAULT_CHAT_URL: &str = "http://localhost:5272/v1/chat/completions";

/// Operator-facing model provider label.
///
/// This label is shown in the host UI instead of the technical backend name.
/// Each label maps to a readiness state and a setup path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ModelProviderLabel {
    /// Private local inference. Works immediately. May use a deterministic stub if no GGUF is configured.
    LocalDemo,
    /// Private local inference via Windows AI / Foundry Local. Requires setup first.
    WindowsAi,
    /// Explicit external API call. Requires operator-supplied endpoint and key.
    Cloud,
}

impl ModelProviderLabel {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::LocalDemo => "Local Demo",
            Self::WindowsAi => "Windows AI",
            Self::Cloud => "Cloud",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::LocalDemo => "Works immediately. Private. May use a deterministic fallback if no GGUF model is configured.",
            Self::WindowsAi => "Private. Requires Windows AI / Foundry Local setup first.",
            Self::Cloud => "Explicit external call. Requires endpoint and API key.",
        }
    }

    pub fn chat_settings(&self, system_prompt: impl Into<String>) -> Result<ChatSettings, String> {
        match self {
            Self::LocalDemo => Ok(local_demo_chat_settings(system_prompt)),
            Self::WindowsAi => windows_ai_chat_settings(system_prompt),
            Self::Cloud => Ok(cloud_chat_settings(system_prompt)),
        }
    }

    /// Readiness for this provider. Requires AppSettings for accurate cloud detection.
    pub fn readiness(&self, settings: &crate::settings::AppSettings) -> ProviderReadiness {
        match self {
            Self::LocalDemo => local_demo_readiness(),
            Self::WindowsAi => windows_ai_readiness(),
            Self::Cloud => cloud_readiness(settings),
        }
    }
}

/// Readiness state for a model provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ProviderReadiness {
    /// Provider can send requests now.
    Ready,
    /// Provider needs one setup step before use.
    SetupNeeded { next_command: String },
    /// Provider cannot be used in the current environment.
    Unavailable { reason: String },
    /// Provider endpoint exists but a smoke test or model load failed.
    Diagnostic { reason: String },
}

impl std::fmt::Display for ProviderReadiness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ready => write!(f, "Ready"),
            Self::SetupNeeded { next_command } => write!(f, "Setup Needed — run: {next_command}"),
            Self::Unavailable { reason } => write!(f, "Unavailable — {reason}"),
            Self::Diagnostic { reason } => write!(f, "Diagnostic — {reason}"),
        }
    }
}

/// Combined provider info for the host UI.
///
/// Returned by `provider_status()` to populate the model-mode selector.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ProviderInfo {
    pub label: ModelProviderLabel,
    pub display_name: String,
    pub description: String,
    pub readiness: ProviderReadiness,
    pub is_default: bool,
}

/// Returns the readiness state for the Local Demo provider.
fn local_demo_readiness() -> ProviderReadiness {
    #[cfg(feature = "mistralrs-llm")]
    {
        if default_phi4_model_path().is_some() {
            return ProviderReadiness::Ready;
        }
    }
    ProviderReadiness::Ready
}

/// Returns the readiness state for the Windows AI provider.
fn windows_ai_readiness() -> ProviderReadiness {
    match discover_foundry_local_endpoint() {
        Ok(Some(_)) => ProviderReadiness::Ready,
        Ok(None) => ProviderReadiness::SetupNeeded {
            next_command: "just windows-ai-install && just windows-ai-setup".to_string(),
        },
        Err(error) => {
            if error.contains("not found") || error.contains("cannot find") {
                ProviderReadiness::SetupNeeded {
                    next_command: "just windows-ai-install".to_string(),
                }
            } else {
                ProviderReadiness::SetupNeeded {
                    next_command: "just windows-ai-install && just windows-ai-setup".to_string(),
                }
            }
        }
    }
}

/// Returns the readiness state for the Cloud provider.
fn cloud_readiness(settings: &crate::settings::AppSettings) -> ProviderReadiness {
    let endpoint = settings.chat.endpoint_url.trim();
    let api_key = settings.chat.api_key.trim();
    let model = settings.chat.model.trim();

    // Reject known internal/local endpoints — these are LocalDemo or WindowsAi addresses.
    let is_external_endpoint = !endpoint.is_empty()
        && !endpoint.starts_with("http://127.")
        && !endpoint.starts_with("http://localhost")
        && !endpoint.starts_with("http://[::1]");

    // Reject known internal API key placeholders.
    let is_real_key = !api_key.is_empty()
        && api_key != INTERNAL_LOCAL_API_KEY
        && api_key != FOUNDRY_LOCAL_API_KEY;

    if is_external_endpoint && is_real_key && !model.is_empty() {
        return ProviderReadiness::Ready;
    }
    ProviderReadiness::SetupNeeded {
        next_command: "Configure endpoint and API key in Settings".to_string(),
    }
}

/// Returns provider info for all three operator labels.
///
/// Requires AppSettings so cloud readiness reflects actual configuration.
/// Use this to populate the model-mode selector in the host UI.
pub fn provider_status(settings: &crate::settings::AppSettings) -> Vec<ProviderInfo> {
    let labels = [
        (ModelProviderLabel::LocalDemo, true),
        (ModelProviderLabel::WindowsAi, false),
        (ModelProviderLabel::Cloud, false),
    ];
    labels
        .into_iter()
        .map(|(label, is_default)| {
            let readiness = label.readiness(settings);
            ProviderInfo {
                display_name: label.display_name().to_string(),
                description: label.description().to_string(),
                readiness,
                is_default,
                label,
            }
        })
        .collect()
}

/// Returns ChatSettings pre-configured for the Local Demo provider.
///
/// Uses the internal localhost endpoint when mistralrs is not compiled,
/// or the GGUF runtime path when it is. May produce a deterministic stub response
/// if no GGUF model is available.
pub fn local_demo_chat_settings(system_prompt: impl Into<String>) -> ChatSettings {
    internal_phi_chat_settings(system_prompt)
}

/// Returns ChatSettings pre-configured for the Windows AI provider.
///
/// Requires Foundry Local to be running. Returns an error if the foundry
/// binary is not found or the service status cannot be determined.
pub fn windows_ai_chat_settings(system_prompt: impl Into<String>) -> Result<ChatSettings, String> {
    foundry_local_chat_settings(system_prompt)
}

#[derive(Debug, Error)]
pub enum InternalOpenAiError {
    #[error("failed to bind internal OpenAI endpoint at {addr}: {source}")]
    Bind {
        addr: String,
        source: std::io::Error,
    },
    #[error("internal endpoint thread failed: {0}")]
    Thread(String),
}

#[derive(Debug)]
pub struct InternalOpenAiHandle {
    addr: String,
    shutdown_tx: mpsc::Sender<()>,
    join: Option<thread::JoinHandle<()>>,
}

impl InternalOpenAiHandle {
    pub fn chat_url(&self) -> String {
        format!("http://{}/v1/chat/completions", self.addr)
    }

    pub fn docs_url(&self) -> String {
        format!("http://{}/docs/", self.addr)
    }

    pub fn rotel_health_url(&self) -> String {
        format!("http://{}/rotel/health", self.addr)
    }

    pub fn rotel_export_plan_url(&self) -> String {
        format!("http://{}/rotel/export-plan", self.addr)
    }

    pub fn rotel_logs_url(&self) -> String {
        format!("http://{}/v1/logs", self.addr)
    }

    pub fn rotel_metrics_url(&self) -> String {
        format!("http://{}/v1/metrics", self.addr)
    }

    pub fn rotel_traces_url(&self) -> String {
        format!("http://{}/v1/traces", self.addr)
    }
}

impl Drop for InternalOpenAiHandle {
    fn drop(&mut self) {
        let _ = self.shutdown_tx.send(());
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

pub trait InternalChatBackend: std::fmt::Debug + Send + Sync + 'static {
    fn complete(&self, request: &OpenAiChatRequest) -> Result<String, String>;
}

pub fn internal_phi_chat_settings(system_prompt: impl Into<String>) -> ChatSettings {
    ChatSettings {
        endpoint_url: INTERNAL_OPENAI_CHAT_URL.to_string(),
        api_key: INTERNAL_LOCAL_API_KEY.to_string(),
        model: INTERNAL_PHI_MODEL.to_string(),
        system_prompt: system_prompt.into(),
    }
}

pub fn cloud_chat_settings(system_prompt: impl Into<String>) -> ChatSettings {
    ChatSettings {
        endpoint_url: DEFAULT_CLOUD_CHAT_URL.to_string(),
        api_key: String::new(),
        model: String::new(),
        system_prompt: system_prompt.into(),
    }
}

pub fn foundry_local_chat_settings(
    system_prompt: impl Into<String>,
) -> Result<ChatSettings, String> {
    let endpoint = match discover_foundry_local_endpoint()? {
        Some(ep) => ep,
        None => {
            return Err(
                "Foundry Local not running. Run: just windows-ai-install && just windows-ai-setup"
                    .to_string(),
            );
        }
    };

    Ok(ChatSettings {
        endpoint_url: foundry_chat_url(&endpoint),
        api_key: FOUNDRY_LOCAL_API_KEY.to_string(),
        model: FOUNDRY_LOCAL_MODEL.to_string(),
        system_prompt: system_prompt.into(),
    })
}

pub fn foundry_local_status() -> String {
    match discover_foundry_local_endpoint() {
        Ok(Some(endpoint)) => format!(
            "foundry_local: available\nmodel: {FOUNDRY_LOCAL_MODEL}\nopenai_endpoint: {}",
            foundry_chat_url(&endpoint)
        ),
        Ok(None) => format!(
            "foundry_local: status command returned no endpoint\nmodel: {FOUNDRY_LOCAL_MODEL}\nfallback_endpoint: {FOUNDRY_LOCAL_DEFAULT_CHAT_URL}"
        ),
        Err(error) => format!(
            "foundry_local: unavailable ({error})\nmodel: {FOUNDRY_LOCAL_MODEL}\nsetup: just windows-ai-install && just windows-ai-setup"
        ),
    }
}

pub fn discover_foundry_local_endpoint() -> Result<Option<String>, String> {
    if let Ok(endpoint) = std::env::var("LEDGERR_FOUNDRY_LOCAL_ENDPOINT") {
        let trimmed = endpoint.trim();
        if !trimmed.is_empty() {
            return Ok(Some(normalize_foundry_endpoint(trimmed)));
        }
    }

    let output = Command::new("foundry")
        .args(["service", "status"])
        .output()
        .map_err(|error| format!("failed to run `foundry service status`: {error}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}\n{stderr}");

    if !output.status.success() {
        return Err(format!(
            "`foundry service status` exited with {}: {}",
            output.status,
            combined.trim()
        ));
    }

    let Some(endpoint) = parse_foundry_endpoint(&combined) else {
        return Ok(None);
    };

    Ok(Some(
        discover_foundry_rest_endpoint(&endpoint).unwrap_or(endpoint),
    ))
}

pub(crate) fn parse_foundry_endpoint(raw: &str) -> Option<String> {
    raw.split(|ch: char| ch.is_whitespace() || matches!(ch, '"' | '\'' | ',' | '[' | ']'))
        .find_map(|token| {
            let endpoint = token
                .trim_matches(|ch| matches!(ch, '.' | ';' | ')' | '('))
                .trim_end_matches('/');
            if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
                Some(normalize_foundry_endpoint(endpoint))
            } else {
                None
            }
        })
}

fn discover_foundry_rest_endpoint(endpoint: &str) -> Option<String> {
    use std::time::Duration;

    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct FoundryStatus {
        endpoints: Vec<String>,
    }

    let status_url = format!("{}/openai/status", normalize_foundry_endpoint(endpoint));
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(2))
        .connect_timeout(Duration::from_secs(1))
        .build()
        .ok()?;
    let status = client
        .get(&status_url)
        .send()
        .ok()?
        .json::<FoundryStatus>()
        .ok()?;
    status
        .endpoints
        .into_iter()
        .find(|endpoint| endpoint.starts_with("http://") || endpoint.starts_with("https://"))
        .map(|endpoint| normalize_foundry_endpoint(&endpoint))
}

fn normalize_foundry_endpoint(endpoint: &str) -> String {
    endpoint
        .trim()
        .trim_end_matches('/')
        .trim_end_matches("/v1/chat/completions")
        .trim_end_matches("/v1")
        .trim_end_matches("/openai")
        .to_string()
}

fn foundry_chat_url(endpoint: &str) -> String {
    format!(
        "{}/v1/chat/completions",
        normalize_foundry_endpoint(endpoint)
    )
}

pub fn internal_phi_backend_status() -> String {
    let mut lines = vec![
        format!("model: {INTERNAL_PHI_MODEL}"),
        format!("openai_endpoint: {INTERNAL_OPENAI_CHAT_URL}"),
        "rig_client: RigAgentRuntime".to_string(),
    ];

    #[cfg(feature = "mistralrs-llm")]
    {
        let model_path = default_phi4_model_path()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "not found".to_string());
        lines.push(format!("mistralrs: compiled, phi4_gguf: {model_path}"));
    }

    #[cfg(not(feature = "mistralrs-llm"))]
    lines.push("mistralrs: not compiled in this build".to_string());

    #[cfg(feature = "local-llm")]
    lines.push("candle: compiled in this build".to_string());

    #[cfg(not(feature = "local-llm"))]
    lines.push("candle: not compiled in this build".to_string());

    lines.push(
        "fallback: deterministic Phi-4-compatible local endpoint when model runtime is unavailable"
            .to_string(),
    );
    lines.join("\n")
}

pub fn docs_playbook_status() -> String {
    match default_docs_root() {
        Some(root) if root.join("index.html").exists() => {
            format!("Docs playbook ready at {INTERNAL_DOCS_URL}\nroot: {}", root.display())
        }
        Some(root) => format!(
            "Docs playbook root exists but index.html is missing at {}. Run `just docgen` to rebuild the mdBook output.",
            root.display()
        ),
        None => "Docs playbook is not built. Run `just docgen` to generate book/book before opening the local docs route.".to_string(),
    }
}

pub fn internal_rotel_status() -> String {
    let plan = RotelExportPlan::from_endpoint(&internal_rotel_endpoint(INTERNAL_OPENAI_ADDR));
    [
        "rotel: embedded in internal OpenAI-compatible listener".to_string(),
        format!("health_endpoint: {INTERNAL_ROTEL_HEALTH_URL}"),
        format!("logs_endpoint: {}", plan.logs_url),
        format!("metrics_endpoint: {}", plan.metrics_url),
        format!("traces_endpoint: {}", plan.traces_url),
        format!("arrow_connector_enabled: {}", plan.arrow_connector_enabled),
    ]
    .join("\n")
}

#[derive(Debug, Clone)]
pub struct InternalServerConfig {
    pub addr: String,
    pub docs_root: Option<PathBuf>,
}

impl Default for InternalServerConfig {
    fn default() -> Self {
        Self {
            addr: INTERNAL_OPENAI_ADDR.to_string(),
            docs_root: default_docs_root(),
        }
    }
}

#[derive(Debug, Default)]
pub struct Phi4LocalFallbackBackend;

impl InternalChatBackend for Phi4LocalFallbackBackend {
    fn complete(&self, request: &OpenAiChatRequest) -> Result<String, String> {
        let user = request
            .messages
            .iter()
            .rev()
            .find(|message| message.role == "user")
            .map(|message| message.content_text())
            .unwrap_or_default();

        let response = if user.contains("audit_playbook")
            || user.contains("audit playbook")
            || user.contains("visual evidence graph")
        {
            serde_json::json!({
                "playbook": "audit_playbook",
                "mode": "deterministic_fallback",
                "steps": [
                    "ingest_rows",
                    "classify_transactions",
                    "phi4_edge_proposals",
                    "operator_review",
                    "workbook_export",
                    "evidence_chain",
                    "visual_audit_graph"
                ],
                "requires_model_assets": false
            })
            .to_string()
        } else if user.contains("\"job\":\"classify_transaction\"")
            || user.contains("classify_transaction") && user.contains("return_schema")
        {
            serde_json::json!({
                "category": "Meals",
                "confidence": 0.72,
                "reason": "deterministic Phi-4 fallback classification",
                "suggested_tags": ["#phi4-fallback"]
            })
            .to_string()
        } else if user.contains("fn ") || user.contains("if ") || user.contains("match ") {
            [
                "fn classify_rows() -> score_confidence",
                "if confidence > 0.85 -> commit_workbook",
                "if confidence > 0.60 -> review_flag",
                "if confidence <= 0.60 -> escalate_operator",
                "fn review_flag() -> commit_workbook",
                "",
                "The internal phi-4-mini-reasoning endpoint preserved the supported Rhai DSL and added a review-safe medium-confidence lane.",
            ]
            .join("\n")
        } else {
            format!(
                "Internal phi-4-mini-reasoning endpoint is online. Received {} message(s). Build with the local model feature and configure the GGUF path to replace this deterministic fallback with real Phi-4 inference.",
                request.messages.len()
            )
        };

        Ok(response)
    }
}

#[derive(Debug, Deserialize)]
pub struct OpenAiChatRequest {
    pub model: String,
    #[serde(default)]
    pub messages: Vec<OpenAiChatMessage>,
    #[serde(default)]
    pub max_tokens: Option<usize>,
    #[serde(default)]
    pub stream: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct OpenAiChatMessage {
    pub role: String,
    pub content: OpenAiMessageContent,
}

impl OpenAiChatMessage {
    fn content_text(&self) -> String {
        self.content.text()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(untagged)]
pub enum OpenAiMessageContent {
    Text(String),
    Parts(Vec<serde_json::Value>),
}

impl OpenAiMessageContent {
    fn text(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Parts(parts) => parts
                .iter()
                .filter_map(|part| match part {
                    serde_json::Value::String(text) => Some(text.as_str()),
                    serde_json::Value::Object(object) => {
                        object.get("text").and_then(serde_json::Value::as_str)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }
}

impl From<String> for OpenAiMessageContent {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<&str> for OpenAiMessageContent {
    fn from(value: &str) -> Self {
        Self::Text(value.to_string())
    }
}

#[derive(Debug, Serialize)]
struct OpenAiChatResponse {
    id: String,
    object: &'static str,
    created: u64,
    model: String,
    choices: Vec<OpenAiChoice>,
    usage: OpenAiUsage,
}

#[derive(Debug, Serialize)]
struct OpenAiChoice {
    index: usize,
    message: OpenAiChatMessage,
    finish_reason: &'static str,
}

#[derive(Debug, Serialize)]
struct OpenAiUsage {
    prompt_tokens: usize,
    completion_tokens: usize,
    total_tokens: usize,
}

#[derive(Debug, Serialize)]
struct OpenAiModelList {
    object: &'static str,
    data: Vec<OpenAiModel>,
}

#[derive(Debug, Serialize)]
struct OpenAiModel {
    id: &'static str,
    object: &'static str,
    owned_by: &'static str,
}

pub fn spawn_internal_openai_endpoint(
    addr: impl Into<String>,
    backend: Arc<dyn InternalChatBackend>,
) -> Result<InternalOpenAiHandle, InternalOpenAiError> {
    spawn_internal_openai_endpoint_with_config(
        InternalServerConfig {
            addr: addr.into(),
            docs_root: default_docs_root(),
        },
        backend,
    )
}

pub fn spawn_internal_openai_endpoint_with_config(
    config: InternalServerConfig,
    backend: Arc<dyn InternalChatBackend>,
) -> Result<InternalOpenAiHandle, InternalOpenAiError> {
    let addr = config.addr;
    let listener = TcpListener::bind(&addr).map_err(|source| InternalOpenAiError::Bind {
        addr: addr.clone(),
        source,
    })?;
    listener
        .set_nonblocking(true)
        .map_err(|source| InternalOpenAiError::Bind {
            addr: addr.clone(),
            source,
        })?;

    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let thread_addr = addr.clone();
    let join = thread::Builder::new()
        .name("ledgerr-internal-openai".to_string())
        .spawn(move || serve_loop(listener, shutdown_rx, backend, config.docs_root))
        .map_err(|error| InternalOpenAiError::Thread(error.to_string()))?;

    Ok(InternalOpenAiHandle {
        addr: thread_addr,
        shutdown_tx,
        join: Some(join),
    })
}

pub fn start_default_internal_openai_endpoint() -> Result<InternalOpenAiHandle, InternalOpenAiError>
{
    spawn_internal_openai_endpoint_with_config(
        InternalServerConfig::default(),
        default_internal_backend(),
    )
}

fn default_internal_backend() -> Arc<dyn InternalChatBackend> {
    #[cfg(feature = "mistralrs-llm")]
    {
        if let Some(path) = default_phi4_model_path() {
            if let Ok(runtime) = crate::local_llm_mistral::LocalMistralRuntime::new(path) {
                return Arc::new(Phi4MistralBackend { runtime });
            }
        }
    }

    Arc::new(Phi4LocalFallbackBackend)
}

#[cfg(feature = "mistralrs-llm")]
#[derive(Debug)]
struct Phi4MistralBackend {
    runtime: crate::local_llm_mistral::LocalMistralRuntime,
}

#[cfg(feature = "mistralrs-llm")]
impl InternalChatBackend for Phi4MistralBackend {
    fn complete(&self, request: &OpenAiChatRequest) -> Result<String, String> {
        use crate::agent_runtime::{AgentRuntime, ModelRequest, ModelRole, ModelTurn};

        let mut system_prompt = None;
        let mut history = Vec::new();
        let mut user_message = None;

        for message in &request.messages {
            match message.role.as_str() {
                "system" if system_prompt.is_none() => {
                    system_prompt = Some(message.content_text());
                }
                "assistant" => history.push(ModelTurn {
                    role: ModelRole::Assistant,
                    content: message.content_text(),
                }),
                "user" => {
                    if let Some(previous_user) = user_message.replace(message.content_text()) {
                        history.push(ModelTurn {
                            role: ModelRole::User,
                            content: previous_user,
                        });
                    }
                }
                _ => {}
            }
        }

        let mut model_request =
            ModelRequest::text(user_message.unwrap_or_else(|| "Continue.".to_string()))
                .with_history(history);
        if let Some(system_prompt) = system_prompt {
            model_request = model_request.with_system_prompt(system_prompt);
        }
        if let Some(max_tokens) = request.max_tokens {
            model_request = model_request.with_max_tokens(max_tokens);
        }

        AgentRuntime::complete(&self.runtime, model_request)
            .map(|response| response.assistant_text)
            .map_err(|error| error.to_string())
    }
}

fn serve_loop(
    listener: TcpListener,
    shutdown_rx: mpsc::Receiver<()>,
    backend: Arc<dyn InternalChatBackend>,
    docs_root: Option<PathBuf>,
) {
    let server_addr = listener
        .local_addr()
        .map(|addr| addr.to_string())
        .unwrap_or_else(|_| INTERNAL_OPENAI_ADDR.to_string());
    loop {
        if shutdown_rx.try_recv().is_ok() {
            break;
        }
        match listener.accept() {
            Ok((stream, _)) => {
                handle_stream(stream, backend.as_ref(), docs_root.as_deref(), &server_addr)
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(25));
            }
            Err(_) => break,
        }
    }
}

fn handle_stream(
    mut stream: TcpStream,
    backend: &dyn InternalChatBackend,
    docs_root: Option<&Path>,
    server_addr: &str,
) {
    let mut buffer = Vec::with_capacity(8192);
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

    let response = route_http_request_for_addr(&buffer, backend, docs_root, server_addr);
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn request_complete(buffer: &[u8]) -> bool {
    let Some(header_end) = find_header_end(buffer) else {
        return false;
    };
    let headers = String::from_utf8_lossy(&buffer[..header_end]);
    let content_length = parse_content_length(&headers).unwrap_or_default();
    buffer.len() >= header_end + 4 + content_length
}

#[cfg(test)]
fn route_http_request(
    raw: &[u8],
    backend: &dyn InternalChatBackend,
    docs_root: Option<&Path>,
) -> String {
    route_http_request_for_addr(raw, backend, docs_root, INTERNAL_OPENAI_ADDR)
}

fn route_http_request_for_addr(
    raw: &[u8],
    backend: &dyn InternalChatBackend,
    docs_root: Option<&Path>,
    server_addr: &str,
) -> String {
    let Some(header_end) = find_header_end(raw) else {
        return json_response(400, &serde_json::json!({ "error": "invalid request" }));
    };
    let headers = String::from_utf8_lossy(&raw[..header_end]);
    let request_line = headers.lines().next().unwrap_or_default();
    let body = &raw[header_end + 4..];

    if request_line.starts_with("GET /docs ") {
        return redirect_response("/docs/");
    }

    if request_line.starts_with("GET /docs/ ") || request_line.starts_with("GET /docs/") {
        return match docs_root {
            Some(root) => serve_docs_request(request_line, root),
            None => docs_missing_response(),
        };
    }

    if request_line.starts_with("GET /v1/models ") {
        let payload = OpenAiModelList {
            object: "list",
            data: vec![OpenAiModel {
                id: INTERNAL_PHI_MODEL,
                object: "model",
                owned_by: "l3dg3rr",
            }],
        };
        return json_response(200, &payload);
    }

    if request_line.starts_with("GET /rotel/health ") {
        return json_response(200, &rotel_health_payload(server_addr));
    }

    if request_line.starts_with("GET /rotel/export-plan ") {
        let plan = RotelExportPlan::from_endpoint(&internal_rotel_endpoint(server_addr));
        return json_response(200, &plan);
    }

    if let Some(signal) = otlp_signal_from_request_line(request_line) {
        return rotel_otlp_response(signal, body);
    }

    if !request_line.starts_with("POST /v1/chat/completions ") {
        return json_response(404, &serde_json::json!({ "error": "not found" }));
    }

    let request = match serde_json::from_slice::<OpenAiChatRequest>(body) {
        Ok(request) => request,
        Err(error) => {
            return json_response(
                400,
                &serde_json::json!({ "error": format!("invalid chat request: {error}") }),
            );
        }
    };

    if request.stream {
        return json_response(
            400,
            &serde_json::json!({ "error": "streaming responses are not supported by the internal endpoint yet" }),
        );
    }

    let assistant_text = match backend.complete(&request) {
        Ok(text) => text,
        Err(error) => return json_response(500, &serde_json::json!({ "error": error })),
    };
    json_response(200, &chat_response(&request, assistant_text))
}

fn internal_rotel_endpoint(addr: &str) -> RotelEndpoint {
    RotelEndpoint {
        otlp_http_endpoint: format!("http://{addr}"),
        otlp_grpc_endpoint: "internal-listener-disabled".to_string(),
        arrow_connector_enabled: true,
    }
}

fn rotel_health_payload(addr: &str) -> serde_json::Value {
    let endpoint = internal_rotel_endpoint(addr);
    let plan = RotelExportPlan::from_endpoint(&endpoint);
    serde_json::json!({
        "status": "ok",
        "service": "rotel-embedded",
        "listener": "l3dg3rr-internal-openai",
        "otlp_http_endpoint": endpoint.otlp_http_endpoint,
        "otlp_grpc_endpoint": endpoint.otlp_grpc_endpoint,
        "arrow_connector_enabled": endpoint.arrow_connector_enabled,
        "routes": {
            "logs": plan.logs_url,
            "metrics": plan.metrics_url,
            "traces": plan.traces_url,
            "export_plan": format!("http://{addr}/rotel/export-plan")
        }
    })
}

fn otlp_signal_from_request_line(request_line: &str) -> Option<OTelSignal> {
    if request_line.starts_with("POST /v1/logs ") {
        Some(OTelSignal::Log)
    } else if request_line.starts_with("POST /v1/metrics ") {
        Some(OTelSignal::Metric)
    } else if request_line.starts_with("POST /v1/traces ") {
        Some(OTelSignal::Trace)
    } else {
        None
    }
}

fn rotel_otlp_response(signal: OTelSignal, body: &[u8]) -> String {
    let payload = match serde_json::from_slice::<serde_json::Value>(body) {
        Ok(value) => value,
        Err(error) => {
            return json_response(
                400,
                &serde_json::json!({ "error": format!("invalid OTLP JSON payload: {error}") }),
            );
        }
    };
    let resource_count = otlp_resource_count(signal, &payload);
    json_response(
        202,
        &serde_json::json!({
            "accepted": true,
            "service": "rotel-embedded",
            "listener": "l3dg3rr-internal-openai",
            "signal": signal.as_str(),
            "content_type": "application/json",
            "resource_count": resource_count,
            "payload_bytes": body.len(),
            "arrow_connector_enabled": true,
            "classification_columns": ledger_core::observability::TelemetryArrowBatch::classification_columns(),
        }),
    )
}

fn otlp_resource_count(signal: OTelSignal, payload: &serde_json::Value) -> usize {
    let key = match signal {
        OTelSignal::Log => "resourceLogs",
        OTelSignal::Metric => "resourceMetrics",
        OTelSignal::Trace => "resourceSpans",
    };
    payload
        .get(key)
        .and_then(serde_json::Value::as_array)
        .map(Vec::len)
        .unwrap_or_default()
}

pub fn open_internal_docs_in_browser() -> std::io::Result<()> {
    open_url_in_browser(INTERNAL_DOCS_URL)
}

pub fn open_url_in_browser(url: &str) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
            .map(|_| ())
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(url).spawn().map(|_| ())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open").arg(url).spawn().map(|_| ())
    }
}

fn serve_docs_request(request_line: &str, docs_root: &Path) -> String {
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/docs/")
        .trim_start_matches("/docs/");
    let relative = if path.is_empty() { "index.html" } else { path };
    let Some(safe_path) = safe_join_docs_path(docs_root, relative) else {
        return json_response(400, &serde_json::json!({ "error": "invalid docs path" }));
    };
    let file_path = if safe_path.is_dir() {
        safe_path.join("index.html")
    } else {
        safe_path
    };
    match std::fs::read(&file_path) {
        Ok(bytes) => bytes_response(200, mime_type(&file_path), &bytes),
        Err(_) if relative == "index.html" => docs_missing_response(),
        Err(_) => json_response(404, &serde_json::json!({ "error": "docs asset not found" })),
    }
}

fn docs_missing_response() -> String {
    bytes_response(
        404,
        "text/html; charset=utf-8",
        br#"<!doctype html>
<html>
<head><meta charset="utf-8"><title>l3dg3rr docs playbook</title></head>
<body style="font-family: system-ui, sans-serif; margin: 2rem; line-height: 1.5;">
<h1>Docs playbook is not built</h1>
<p>The internal docs route is active, but <code>book/book/index.html</code> was not found.</p>
<p>Run <code>just docgen</code>, then reload <code>http://127.0.0.1:15115/docs/</code>.</p>
</body>
</html>"#,
    )
}

fn safe_join_docs_path(root: &Path, relative: &str) -> Option<PathBuf> {
    let mut out = root.to_path_buf();
    for component in Path::new(relative).components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(out)
}

fn mime_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
    {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "svg" => "image/svg+xml",
        "json" => "application/json",
        "wasm" => "application/wasm",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        _ => "application/octet-stream",
    }
}

fn redirect_response(location: &str) -> String {
    format!(
        "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )
}

fn bytes_response(status: u16, content_type: &str, body: &[u8]) -> String {
    let reason = match status {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "OK",
    };
    let mut response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(body);
    String::from_utf8_lossy(&response).into_owned()
}

fn chat_response(request: &OpenAiChatRequest, assistant_text: String) -> OpenAiChatResponse {
    let prompt_tokens = request
        .messages
        .iter()
        .map(|message| estimate_tokens(&message.content_text()))
        .sum::<usize>();
    let completion_tokens = estimate_tokens(&assistant_text);

    OpenAiChatResponse {
        id: format!("chatcmpl-l3dg3rr-{}", unix_timestamp()),
        object: "chat.completion",
        created: unix_timestamp(),
        model: if request.model.trim().is_empty() {
            INTERNAL_PHI_MODEL.to_string()
        } else {
            request.model.trim().to_string()
        },
        choices: vec![OpenAiChoice {
            index: 0,
            message: OpenAiChatMessage {
                role: "assistant".to_string(),
                content: assistant_text.into(),
            },
            finish_reason: "stop",
        }],
        usage: OpenAiUsage {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
        },
    }
}

fn json_response(status: u16, payload: &impl Serialize) -> String {
    let body = serde_json::to_string(payload)
        .unwrap_or_else(|_| "{\"error\":\"serialization failure\"}".to_string());
    let reason = match status {
        200 => "OK",
        202 => "Accepted",
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

fn estimate_tokens(text: &str) -> usize {
    text.split_whitespace().count().max(1)
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn default_docs_root() -> Option<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .map(|workspace| workspace.join("book/book"))?;
    root.exists().then_some(root)
}

#[cfg(feature = "mistralrs-llm")]
fn default_phi4_model_path() -> Option<PathBuf> {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .map(Path::to_path_buf)?;
    let repo_model =
        workspace.join("models/unsloth/Phi-4-mini-reasoning-GGUF/Phi-4-mini-reasoning-Q3_K_M.gguf");
    if repo_model.exists() {
        return Some(repo_model);
    }

    let d_drive_model = PathBuf::from(
        "/mnt/d/models/unsloth/Phi-4-mini-reasoning-GGUF/Phi-4-mini-reasoning-Q3_K_M.gguf",
    );
    d_drive_model.exists().then_some(d_drive_model)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_runtime::{
        ClassifyTransactionJob, TransactionClassificationOutput, PHI4_TYPED_JOB_SYSTEM_PROMPT,
    };

    #[derive(Debug)]
    struct FixedBackend;

    impl InternalChatBackend for FixedBackend {
        fn complete(&self, request: &OpenAiChatRequest) -> Result<String, String> {
            Ok(format!(
                "model={} messages={}",
                request.model,
                request.messages.len()
            ))
        }
    }

    #[test]
    fn chat_completion_returns_openai_compatible_response() {
        let body = serde_json::json!({
            "model": "phi-4-mini-reasoning",
            "messages": [
                { "role": "system", "content": "be brief" },
                { "role": "user", "content": "hello" }
            ],
            "max_tokens": 64
        })
        .to_string();
        let raw = format!(
            "POST /v1/chat/completions HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );

        let response = route_http_request(raw.as_bytes(), &FixedBackend, None);

        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("\"object\":\"chat.completion\""));
        assert!(response.contains("\"model\":\"phi-4-mini-reasoning\""));
        assert!(response.contains("model=phi-4-mini-reasoning messages=2"));
    }

    #[test]
    fn chat_completion_accepts_openai_content_part_arrays() {
        let body = serde_json::json!({
            "model": "phi-4-mini-reasoning",
            "messages": [
                {
                    "role": "user",
                    "content": [
                        { "type": "text", "text": "fn classify_rows() -> score_confidence" }
                    ]
                }
            ]
        })
        .to_string();
        let raw = format!(
            "POST /v1/chat/completions HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );

        let response = route_http_request(raw.as_bytes(), &Phi4LocalFallbackBackend, None);

        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(
            response.contains("if confidence &gt; 0.60 -> review_flag")
                || response.contains("if confidence > 0.60 -> review_flag")
        );
    }

    #[test]
    fn models_route_lists_internal_phi_model() {
        let raw = "GET /v1/models HTTP/1.1\r\nHost: localhost\r\n\r\n";

        let response = route_http_request(raw.as_bytes(), &FixedBackend, None);

        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("\"id\":\"phi-4-mini-reasoning\""));
    }

    #[test]
    fn rotel_health_route_is_hosted_on_internal_listener() {
        let raw = "GET /rotel/health HTTP/1.1\r\nHost: localhost\r\n\r\n";

        let response = route_http_request(raw.as_bytes(), &FixedBackend, None);

        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("\"service\":\"rotel-embedded\""));
        assert!(response.contains("\"listener\":\"l3dg3rr-internal-openai\""));
        assert!(response.contains("\"logs\":\"http://127.0.0.1:15115/v1/logs\""));
        assert!(response.contains("\"arrow_connector_enabled\":true"));
    }

    #[test]
    fn rotel_export_plan_reuses_core_otlp_shape() {
        let raw = "GET /rotel/export-plan HTTP/1.1\r\nHost: localhost\r\n\r\n";

        let response = route_http_request(raw.as_bytes(), &FixedBackend, None);

        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("\"logs_url\":\"http://127.0.0.1:15115/v1/logs\""));
        assert!(response.contains("\"metrics_url\":\"http://127.0.0.1:15115/v1/metrics\""));
        assert!(response.contains("\"traces_url\":\"http://127.0.0.1:15115/v1/traces\""));
        assert!(response.contains("\"abstract_regex_type\""));
    }

    #[test]
    fn rotel_export_plan_reflects_bound_listener_address() {
        let raw = "GET /rotel/export-plan HTTP/1.1\r\nHost: localhost\r\n\r\n";

        let response =
            route_http_request_for_addr(raw.as_bytes(), &FixedBackend, None, "127.0.0.1:18081");

        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("\"logs_url\":\"http://127.0.0.1:18081/v1/logs\""));
        assert!(response.contains("\"metrics_url\":\"http://127.0.0.1:18081/v1/metrics\""));
        assert!(response.contains("\"traces_url\":\"http://127.0.0.1:18081/v1/traces\""));
    }

    #[test]
    fn rotel_otlp_logs_route_accepts_json_payloads() {
        let body = serde_json::json!({
            "resourceLogs": [
                {
                    "resource": { "attributes": [] },
                    "scopeLogs": []
                }
            ]
        })
        .to_string();
        let raw = format!(
            "POST /v1/logs HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );

        let response = route_http_request(raw.as_bytes(), &FixedBackend, None);

        assert!(response.starts_with("HTTP/1.1 202 Accepted"));
        assert!(response.contains("\"accepted\":true"));
        assert!(response.contains("\"signal\":\"log\""));
        assert!(response.contains("\"resource_count\":1"));
        assert!(response.contains("\"classification_columns\""));
    }

    #[test]
    fn rotel_otlp_route_rejects_invalid_json() {
        let raw =
            "POST /v1/metrics HTTP/1.1\r\nHost: localhost\r\nContent-Length: 8\r\n\r\nnot-json";

        let response = route_http_request(raw.as_bytes(), &FixedBackend, None);

        assert!(response.starts_with("HTTP/1.1 400 Bad Request"));
        assert!(response.contains("invalid OTLP JSON payload"));
    }

    #[test]
    fn internal_openai_handle_exposes_rotel_listener_urls() {
        let (shutdown_tx, _shutdown_rx) = mpsc::channel();
        let handle = InternalOpenAiHandle {
            addr: "127.0.0.1:18080".to_string(),
            shutdown_tx,
            join: None,
        };

        assert_eq!(
            handle.rotel_health_url(),
            "http://127.0.0.1:18080/rotel/health"
        );
        assert_eq!(
            handle.rotel_export_plan_url(),
            "http://127.0.0.1:18080/rotel/export-plan"
        );
        assert_eq!(handle.rotel_logs_url(), "http://127.0.0.1:18080/v1/logs");
        assert_eq!(
            handle.rotel_metrics_url(),
            "http://127.0.0.1:18080/v1/metrics"
        );
        assert_eq!(
            handle.rotel_traces_url(),
            "http://127.0.0.1:18080/v1/traces"
        );
    }

    #[test]
    fn fallback_backend_generates_review_safe_rhai_when_prompt_contains_rules() {
        let request = OpenAiChatRequest {
            model: INTERNAL_PHI_MODEL.to_string(),
            messages: vec![OpenAiChatMessage {
                role: "user".to_string(),
                content: "fn classify_rows() -> score_confidence".into(),
            }],
            max_tokens: Some(128),
            stream: false,
        };

        let response = Phi4LocalFallbackBackend::default()
            .complete(&request)
            .expect("fallback should respond");

        assert!(response.contains("if confidence > 0.60 -> review_flag"));
        assert!(response.contains("review-safe"));
    }

    #[test]
    fn fallback_backend_generates_valid_typed_classification_json() {
        let job = ClassifyTransactionJob {
            tx_id: "tx_123".to_string(),
            account_id: "WF-BH-CHK".to_string(),
            date: "2024-01-31".to_string(),
            amount: "-12.34".to_string(),
            description: "Cafe lunch".to_string(),
        };
        let model_request = job.to_model_request().expect("model request");
        let request = OpenAiChatRequest {
            model: INTERNAL_PHI_MODEL.to_string(),
            messages: vec![
                OpenAiChatMessage {
                    role: "system".to_string(),
                    content: PHI4_TYPED_JOB_SYSTEM_PROMPT.into(),
                },
                OpenAiChatMessage {
                    role: "user".to_string(),
                    content: model_request.user_message.into(),
                },
            ],
            max_tokens: model_request.max_tokens,
            stream: false,
        };

        let response = Phi4LocalFallbackBackend
            .complete(&request)
            .expect("fallback should respond");
        let output: TransactionClassificationOutput =
            serde_json::from_str(&response).expect("typed json");

        output.validate().expect("valid typed output");
        assert_eq!(output.category, "Meals");
        assert_eq!(output.suggested_tags, ["#phi4-fallback"]);
    }

    #[test]
    fn internal_phi_fallback_runs_audit_playbook_prompt() {
        let request = OpenAiChatRequest {
            model: INTERNAL_PHI_MODEL.to_string(),
            messages: vec![OpenAiChatMessage {
                role: "user".to_string(),
                content: "Run the audit playbook and return the visual evidence graph steps."
                    .into(),
            }],
            max_tokens: Some(256),
            stream: false,
        };

        let response = Phi4LocalFallbackBackend
            .complete(&request)
            .expect("fallback should respond");
        let payload: serde_json::Value = serde_json::from_str(&response).expect("json response");

        assert_eq!(payload["playbook"], "audit_playbook");
        assert_eq!(payload["mode"], "deterministic_fallback");
        assert_eq!(payload["requires_model_assets"], false);
        assert!(payload["steps"]
            .as_array()
            .expect("steps")
            .iter()
            .any(|step| step == "visual_audit_graph"));
    }

    #[test]
    fn provider_switch_settings_point_to_internal_or_cloud_endpoint() {
        let internal = internal_phi_chat_settings("system");
        assert_eq!(internal.endpoint_url, INTERNAL_OPENAI_CHAT_URL);
        assert_eq!(internal.model, INTERNAL_PHI_MODEL);
        assert_eq!(internal.api_key, INTERNAL_LOCAL_API_KEY);

        let cloud = cloud_chat_settings("system");
        assert_eq!(cloud.endpoint_url, DEFAULT_CLOUD_CHAT_URL);
        assert!(cloud.model.is_empty());
        assert!(cloud.api_key.is_empty());
    }

    #[test]
    fn foundry_endpoint_parser_accepts_cli_and_rest_status_shapes() {
        let cli = r#"
            Foundry Local service is running
            Endpoint: http://localhost:58123
        "#;
        assert_eq!(
            parse_foundry_endpoint(cli).as_deref(),
            Some("http://localhost:58123")
        );

        let rest = r#"{ "Endpoints": ["http://127.0.0.1:5272"], "PipeName": "inference_agent" }"#;
        assert_eq!(
            parse_foundry_endpoint(rest).as_deref(),
            Some("http://127.0.0.1:5272")
        );
    }

    #[test]
    fn foundry_endpoint_parser_normalizes_openai_paths() {
        assert_eq!(
            parse_foundry_endpoint("endpoint: http://localhost:5272/v1/chat/completions")
                .as_deref(),
            Some("http://localhost:5272")
        );
        assert_eq!(
            parse_foundry_endpoint("endpoint: http://localhost:5272/openai").as_deref(),
            Some("http://localhost:5272")
        );
    }

    #[test]
    fn backend_status_names_rig_phi4_mistralrs_and_candle() {
        let status = internal_phi_backend_status();

        assert!(status.contains("model: phi-4-mini-reasoning"));
        assert!(status.contains("openai_endpoint: http://127.0.0.1:15115/v1/chat/completions"));
        assert!(status.contains("rig_client: RigAgentRuntime"));
        assert!(status.contains("mistralrs:"));
        assert!(status.contains("candle:"));
    }

    #[test]
    fn docs_route_serves_index_from_configured_root() {
        let temp = tempfile::tempdir().expect("temp dir");
        std::fs::write(temp.path().join("index.html"), "<h1>Playbook</h1>").expect("write docs");
        let raw = "GET /docs/ HTTP/1.1\r\nHost: localhost\r\n\r\n";

        let response = route_http_request(raw.as_bytes(), &FixedBackend, Some(temp.path()));

        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("text/html"));
        assert!(response.contains("<h1>Playbook</h1>"));
    }

    #[test]
    fn docs_route_renders_html_diagnostic_when_book_is_missing() {
        let raw = "GET /docs/ HTTP/1.1\r\nHost: localhost\r\n\r\n";

        let response = route_http_request(raw.as_bytes(), &FixedBackend, None);

        assert!(response.starts_with("HTTP/1.1 404 Not Found"));
        assert!(response.contains("text/html"));
        assert!(response.contains("Docs playbook is not built"));
    }

    #[test]
    fn docs_route_rejects_parent_traversal() {
        let raw = "GET /docs/../settings.json HTTP/1.1\r\nHost: localhost\r\n\r\n";

        let response = route_http_request(raw.as_bytes(), &FixedBackend, Some(Path::new(".")));

        assert!(response.starts_with("HTTP/1.1 400 Bad Request"));
    }

    #[test]
    fn provider_label_display_names_match_prd5() {
        assert_eq!(ModelProviderLabel::LocalDemo.display_name(), "Local Demo");
        assert_eq!(ModelProviderLabel::WindowsAi.display_name(), "Windows AI");
        assert_eq!(ModelProviderLabel::Cloud.display_name(), "Cloud");
    }

    #[test]
    fn provider_label_descriptions_explain_privacy_and_setup() {
        let local = ModelProviderLabel::LocalDemo.description();
        assert!(local.contains("Private"));
        assert!(local.contains("fallback"));

        let windows = ModelProviderLabel::WindowsAi.description();
        assert!(windows.contains("Private"));
        assert!(windows.contains("setup"));

        let cloud = ModelProviderLabel::Cloud.description();
        assert!(cloud.contains("external"));
        assert!(cloud.contains("endpoint"));
    }

    #[test]
    fn local_demo_readiness_is_ready() {
        let settings = crate::settings::AppSettings::default();
        let readiness = ModelProviderLabel::LocalDemo.readiness(&settings);
        assert!(matches!(readiness, ProviderReadiness::Ready));
    }

    #[test]
    fn cloud_readiness_needs_configured_endpoint_and_key() {
        let settings = crate::settings::AppSettings::default();
        let readiness = ModelProviderLabel::Cloud.readiness(&settings);
        assert!(matches!(readiness, ProviderReadiness::SetupNeeded { .. }));

        // With filled settings, cloud should be Ready.
        let configured = crate::settings::AppSettings {
            chat: crate::settings::ChatSettings {
                endpoint_url: "https://my-model.example.com/v1/chat/completions".to_string(),
                api_key: "sk-xxx".to_string(),
                model: "gpt-4o".to_string(),
                system_prompt: "be brief".to_string(),
            },
            ..crate::settings::AppSettings::default()
        };
        let ready = ModelProviderLabel::Cloud.readiness(&configured);
        assert!(matches!(ready, ProviderReadiness::Ready));
    }

    #[test]
    fn provider_status_returns_three_entries() {
        let settings = crate::settings::AppSettings::default();
        let providers = provider_status(&settings);
        assert_eq!(providers.len(), 3);
        let labels: Vec<_> = providers.iter().map(|p| p.label).collect();
        assert!(labels.contains(&ModelProviderLabel::LocalDemo));
        assert!(labels.contains(&ModelProviderLabel::WindowsAi));
        assert!(labels.contains(&ModelProviderLabel::Cloud));
        let default = providers
            .iter()
            .find(|p| p.is_default)
            .expect("default provider");
        assert_eq!(default.label, ModelProviderLabel::LocalDemo);
    }

    #[test]
    fn local_demo_chat_settings_uses_internal_endpoint() {
        let settings = local_demo_chat_settings("test prompt");
        assert_eq!(settings.endpoint_url, INTERNAL_OPENAI_CHAT_URL);
        assert_eq!(settings.model, INTERNAL_PHI_MODEL);
        assert_eq!(settings.api_key, INTERNAL_LOCAL_API_KEY);
        assert_eq!(settings.system_prompt, "test prompt");
    }

    #[test]
    fn cloud_chat_settings_uses_default_cloud_url_with_empty_auth() {
        let settings = cloud_chat_settings("test prompt");
        assert_eq!(settings.endpoint_url, DEFAULT_CLOUD_CHAT_URL);
        assert!(settings.model.is_empty());
        assert!(settings.api_key.is_empty());
        assert_eq!(settings.system_prompt, "test prompt");
    }

    /// Resolve active ChatSettings from the AppSettings model_provider field.
    ///
    /// Returns the resolved settings and an optional warning if a fallback occurred.
    /// The caller (Slint settings panel, chat sender) decides whether to surface
    /// the warning or swallow it.
    #[test]
    fn resolve_chat_settings_uses_cloud_when_cloud_selected() {
        use crate::settings::AppSettings;
        let settings = AppSettings {
            model_provider: ModelProviderLabel::Cloud,
            ..AppSettings::default()
        };
        let (cs, warning) = resolve_chat_settings(&settings);
        assert_eq!(cs.endpoint_url, DEFAULT_CLOUD_CHAT_URL);
        assert!(cs.model.is_empty());
        assert!(cs.api_key.is_empty());
        assert!(warning.is_none());
    }

    #[test]
    fn resolve_chat_settings_falls_back_to_local_demo_for_windows_ai_when_not_installed() {
        use crate::settings::AppSettings;
        let settings = AppSettings {
            model_provider: ModelProviderLabel::WindowsAi,
            ..AppSettings::default()
        };
        let (cs, warning) = resolve_chat_settings(&settings);
        assert!(!cs.endpoint_url.is_empty());
        assert!(warning.is_some());
    }
}

/// Resolve active ChatSettings from the AppSettings model_provider field.
///
/// Returns the resolved settings and an optional warning if a fallback occurred.
/// The caller decides whether to surface the warning or swallow it.
pub fn resolve_chat_settings(
    settings: &crate::settings::AppSettings,
) -> (ChatSettings, Option<ProviderReadiness>) {
    match settings
        .model_provider
        .chat_settings(settings.chat.system_prompt.clone())
    {
        Ok(cs) => (cs, None),
        Err(_) => {
            let fallback = local_demo_chat_settings(settings.chat.system_prompt.clone());
            let warning = Some(ProviderReadiness::Diagnostic {
                reason: format!(
                    "{} unavailable, fell back to Local Demo. {}",
                    settings.model_provider.display_name(),
                    settings.model_provider.readiness(settings),
                ),
            });
            (fallback, warning)
        }
    }
}
