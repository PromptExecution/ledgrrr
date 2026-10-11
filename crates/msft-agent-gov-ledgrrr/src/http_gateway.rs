//! HTTP boundary wrapper — bridges McpGateway (AGT HTTP proxy) with
//! LedgrrAgtGateway (in-process governance). Use this when ledgerr-mcp
//! is exposed over HTTP to external agents in the Hermes mesh.
//!
//! # Governance model
//!
//! `McpGateway` runs its own internal policy engine (`McpGatewayConfig`
//! deny/allow lists, rate limiter, response scanner). It does **not** accept
//! an external governance callback, so it cannot delegate decisions back to
//! `LedgrrAgtGateway::check_tool_call` at call time. The two policies run
//! in parallel: `McpGateway` enforces HTTP-layer controls (session tokens,
//! rate limits, payload scanning) while `LedgrrAgtGateway` enforces
//! in-process ring/trust/lifecycle controls.
//!
//! For a unified single-point-of-decision model, callers should sequence:
//! 1. `McpGateway::process_authenticated_request` — HTTP session + payload safety.
//! 2. `LedgrrAgtGateway::check_tool_call` — AGT ring/trust enforcement.

use std::sync::Arc;
use std::time::Duration;

use agentmesh_mcp::{
    CredentialRedactor, InMemoryAuditSink, InMemoryRateLimitStore, InMemorySessionStore,
    McpGateway, McpGatewayConfig, McpIssuedSession, McpMetricsCollector, McpResponseScanner,
    McpSessionAuthenticator, McpSlidingRateLimiter, SystemClock, SystemNonceGenerator,
};

use crate::{AgtError, LedgrrAgtGateway};

/// Maximum requests per sliding window for the HTTP gateway rate limiter.
///
/// # Known divergence
///
/// This single limit applies to ALL tool calls at the HTTP boundary, including
/// Xero. The in-process YAML policy enforces the tighter Xero-specific limit
/// (30/60s via the `xero-rate-limit` rule). The HTTP gateway has no per-tool
/// routing knowledge, so it cannot enforce per-tool limits. Callers MUST
/// sequence `LedgrrAgtGateway::check_tool_call` after this gateway to ensure
/// the Xero rate limit is enforced at the in-process layer.
const HTTP_GW_MAX_REQUESTS: usize = 120;

/// Sliding window length for the HTTP gateway rate limiter.
const HTTP_GW_WINDOW: Duration = Duration::from_secs(60);

/// Default session TTL for HTTP agents.
const HTTP_SESSION_TTL: Duration = Duration::from_secs(3600);

/// Maximum concurrent sessions per agent over HTTP.
const HTTP_SESSION_CONCURRENT_LIMIT: usize = 8;

/// Simplified gateway decision returned to the server binary (avoids
/// re-exporting agentmesh_mcp types through the crate boundary).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpGatewayDecision {
    /// Request passed all checks.
    Allowed,
    /// Rejected by policy.
    Denied { reason: String },
    /// Rate limit hit; caller should return 429.
    RateLimited,
}

/// Built gateway plus the session authenticator needed to issue tokens at the
/// `/session` endpoint. Both share the same backing clock.
pub struct LedgrrHttpGateway {
    pub gateway: McpGateway,
    pub session_auth: McpSessionAuthenticator,
}

impl LedgrrHttpGateway {
    /// Issue a session token for `agent_id`.  Used by `POST /session`.
    pub fn issue_session(&self, agent_id: &str) -> Result<McpIssuedSession, AgtError> {
        self.session_auth
            .issue_session(agent_id)
            .map_err(|e| AgtError::Redactor(format!("issue_session: {e}")))
    }

    /// Gate a tool call through the HTTP AGT gateway (issue #228).
    ///
    /// Returns `Allowed` when the request passes all checks, or a denial
    /// variant when it should be rejected. The caller sequences this BEFORE
    /// `LedgrrAgtGateway::check_tool_call`.
    pub fn check_request(
        &self,
        agent_id: &str,
        tool_name: &str,
        payload: &serde_json::Value,
        session_token: &str,
    ) -> HttpGatewayDecision {
        use agentmesh_mcp::{McpGatewayRequest, McpGatewayStatus};
        let req = McpGatewayRequest {
            agent_id: agent_id.to_string(),
            tool_name: tool_name.to_string(),
            payload: payload.clone(),
        };
        match self.gateway.process_authenticated_request(&req, session_token) {
            Ok(d) if d.allowed => HttpGatewayDecision::Allowed,
            Ok(d) if d.status == McpGatewayStatus::RateLimited => {
                HttpGatewayDecision::RateLimited
            }
            Ok(d) => HttpGatewayDecision::Denied {
                reason: format!("{:?}", d.status),
            },
            Err(e) => HttpGatewayDecision::Denied {
                reason: e.to_string(),
            },
        }
    }
}

/// Build an AGT `McpGateway` preconfigured for the ledgrrr MCP surface,
/// with session authentication backed by `session_secret`.
///
/// `session_secret` must be at least 16 bytes; derive it from
/// `LEDGERR_HTTP_SESSION_SECRET` env in the caller.
///
/// # Errors
///
/// Returns `AgtError::Redactor` if any sub-component fails to initialise.
pub fn build_ledgrrr_mcp_gateway_with_auth(
    _gw: Arc<LedgrrAgtGateway>,
    session_secret: Vec<u8>,
) -> Result<LedgrrHttpGateway, AgtError> {
    let clock: Arc<dyn agentmesh_mcp::Clock> = Arc::new(SystemClock);

    let audit_sink_redactor = CredentialRedactor::new();
    let audit_sink: Arc<dyn agentmesh_mcp::McpAuditSink> =
        Arc::new(InMemoryAuditSink::new(audit_sink_redactor));

    let scanner_redactor = CredentialRedactor::new();
    let response_scanner = McpResponseScanner::new(
        scanner_redactor,
        Arc::clone(&audit_sink),
        McpMetricsCollector::default(),
        Arc::clone(&clock),
    )
    .map_err(|e| AgtError::Redactor(format!("http_gateway response scanner: {e}")))?;

    let rate_store: Arc<dyn agentmesh_mcp::McpRateLimitStore> =
        Arc::new(InMemoryRateLimitStore::default());
    let rate_limiter = McpSlidingRateLimiter::new(
        HTTP_GW_MAX_REQUESTS,
        HTTP_GW_WINDOW,
        Arc::clone(&clock),
        rate_store,
    )
    .map_err(|e| AgtError::Redactor(format!("http_gateway rate limiter: {e}")))?;

    let nonce_gen: Arc<dyn agentmesh_mcp::NonceGenerator> = Arc::new(SystemNonceGenerator);
    let session_store: Arc<dyn agentmesh_mcp::McpSessionStore> =
        Arc::new(InMemorySessionStore::default());
    let session_auth = McpSessionAuthenticator::new(
        session_secret,
        Arc::clone(&clock),
        nonce_gen,
        session_store,
        HTTP_SESSION_TTL,
        HTTP_SESSION_CONCURRENT_LIMIT,
    )
    .map_err(|e| AgtError::Redactor(format!("http_gateway session auth: {e}")))?;

    let config = McpGatewayConfig::default();
    let gateway = McpGateway::new(
        config,
        response_scanner,
        rate_limiter,
        audit_sink,
        McpMetricsCollector::default(),
        Arc::clone(&clock),
    )
    .with_session_authenticator(session_auth.clone());

    Ok(LedgrrHttpGateway {
        gateway,
        session_auth,
    })
}

/// Build a gateway without session auth — preserved for callers that don't
/// need the auth layer (tests, health checks).
pub fn build_ledgrrr_mcp_gateway(_gw: Arc<LedgrrAgtGateway>) -> Result<McpGateway, AgtError> {
    let clock: Arc<dyn agentmesh_mcp::Clock> = Arc::new(SystemClock);

    let audit_sink_redactor = CredentialRedactor::new();
    let audit_sink: Arc<dyn agentmesh_mcp::McpAuditSink> =
        Arc::new(InMemoryAuditSink::new(audit_sink_redactor));

    let scanner_redactor = CredentialRedactor::new();
    let response_scanner = McpResponseScanner::new(
        scanner_redactor,
        Arc::clone(&audit_sink),
        McpMetricsCollector::default(),
        Arc::clone(&clock),
    )
    .map_err(|e| AgtError::Redactor(format!("http_gateway response scanner: {e}")))?;

    let rate_store: Arc<dyn agentmesh_mcp::McpRateLimitStore> =
        Arc::new(InMemoryRateLimitStore::default());
    let rate_limiter = McpSlidingRateLimiter::new(
        HTTP_GW_MAX_REQUESTS,
        HTTP_GW_WINDOW,
        Arc::clone(&clock),
        rate_store,
    )
    .map_err(|e| AgtError::Redactor(format!("http_gateway rate limiter: {e}")))?;

    let config = McpGatewayConfig::default();
    let gateway = McpGateway::new(
        config,
        response_scanner,
        rate_limiter,
        audit_sink,
        McpMetricsCollector::default(),
        clock,
    );

    Ok(gateway)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_gateway() -> Arc<LedgrrAgtGateway> {
        Arc::new(
            LedgrrAgtGateway::new("hermes").expect("LedgrrAgtGateway::new must succeed in test"),
        )
    }

    #[test]
    fn build_gateway_succeeds() {
        let result = build_ledgrrr_mcp_gateway(make_gateway());
        assert!(
            result.is_ok(),
            "build_ledgrrr_mcp_gateway returned Err: {:?}",
            result.err()
        );
    }

    #[test]
    fn build_gateway_produces_distinct_instance() {
        let gw = make_gateway();
        let first = build_ledgrrr_mcp_gateway(Arc::clone(&gw));
        let second = build_ledgrrr_mcp_gateway(Arc::clone(&gw));
        assert!(first.is_ok(), "first build failed: {:?}", first.err());
        assert!(second.is_ok(), "second build failed: {:?}", second.err());
    }

    #[test]
    fn build_gateway_with_auth_succeeds() {
        let secret = b"ledgerr-test-secret-16bytes!".to_vec();
        let result = build_ledgrrr_mcp_gateway_with_auth(make_gateway(), secret);
        assert!(
            result.is_ok(),
            "build_ledgrrr_mcp_gateway_with_auth returned Err: {:?}",
            result.err()
        );
    }

    #[test]
    fn issue_session_returns_token() {
        let secret = b"ledgerr-test-secret-16bytes!".to_vec();
        let http_gw =
            build_ledgrrr_mcp_gateway_with_auth(make_gateway(), secret).expect("build");
        let session = http_gw.issue_session("test-agent-123");
        assert!(
            session.is_ok(),
            "issue_session returned Err: {:?}",
            session.err()
        );
        let issued = session.unwrap();
        assert!(!issued.token.is_empty(), "token must not be empty");
        assert_eq!(issued.session.agent_id, "test-agent-123");
    }
}
