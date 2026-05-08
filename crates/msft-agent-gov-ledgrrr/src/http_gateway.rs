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
//! 1. `McpGateway::process_request` — HTTP session + payload safety checks.
//! 2. `LedgrrAgtGateway::check_tool_call` — AGT ring/trust enforcement.

use std::sync::Arc;
use std::time::Duration;

use agentmesh_mcp::{
    CredentialRedactor, InMemoryAuditSink, InMemoryRateLimitStore, McpGateway, McpGatewayConfig,
    McpMetricsCollector, McpResponseScanner, McpSlidingRateLimiter, SystemClock,
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

/// Build an AGT `McpGateway` preconfigured for the ledgrrr MCP surface.
///
/// The returned gateway validates session tokens, scans incoming tool
/// schemas for threats, and rate-limits external agents over the HTTP
/// boundary.
///
/// # Governance note
///
/// `McpGateway` has its own internal policy engine and does not accept an
/// external decision callback; it therefore runs **parallel** governance
/// alongside `LedgrrAgtGateway`, not delegated governance. Callers that
/// need in-process ring/trust enforcement must invoke
/// `LedgrrAgtGateway::check_tool_call` separately after this gateway
/// passes the request.
///
/// # Errors
///
/// Returns `AgtError::Redactor` if any sub-component (redactor, scanner,
/// rate limiter) fails to initialise.
pub fn build_ledgrrr_mcp_gateway(
    _gw: Arc<LedgrrAgtGateway>,
) -> Result<McpGateway, AgtError> {
    let clock: Arc<dyn agentmesh_mcp::Clock> = Arc::new(SystemClock);

    // Shared audit sink — used by both the response scanner and the gateway.
    let audit_sink_redactor = CredentialRedactor::new()
        .map_err(|e| AgtError::Redactor(format!("http_gateway audit redactor: {e}")))?;
    let audit_sink: Arc<dyn agentmesh_mcp::McpAuditSink> =
        Arc::new(InMemoryAuditSink::new(audit_sink_redactor));

    // Response scanner — checks outbound MCP payloads for injection / leakage.
    let scanner_redactor = CredentialRedactor::new()
        .map_err(|e| AgtError::Redactor(format!("http_gateway scanner redactor: {e}")))?;
    let response_scanner = McpResponseScanner::new(
        scanner_redactor,
        Arc::clone(&audit_sink),
        McpMetricsCollector::default(),
        Arc::clone(&clock),
    )
    .map_err(|e| AgtError::Redactor(format!("http_gateway response scanner: {e}")))?;

    // Rate limiter — sliding window, in-memory store.
    let rate_store: Arc<dyn agentmesh_mcp::McpRateLimitStore> =
        Arc::new(InMemoryRateLimitStore::default());
    let rate_limiter = McpSlidingRateLimiter::new(
        HTTP_GW_MAX_REQUESTS,
        HTTP_GW_WINDOW,
        Arc::clone(&clock),
        rate_store,
    )
    .map_err(|e| AgtError::Redactor(format!("http_gateway rate limiter: {e}")))?;

    // Gateway config — deny nothing by default; callers can layer policy on top.
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
            LedgrrAgtGateway::new("hermes")
                .expect("LedgrrAgtGateway::new must succeed in test"),
        )
    }

    #[test]
    fn build_gateway_succeeds() {
        let result = build_ledgrrr_mcp_gateway(make_gateway());
        assert!(result.is_ok(), "build_ledgrrr_mcp_gateway returned Err: {:?}", result.err());
    }

    #[test]
    fn build_gateway_produces_distinct_instance() {
        let gw = make_gateway();
        let first = build_ledgrrr_mcp_gateway(Arc::clone(&gw));
        let second = build_ledgrrr_mcp_gateway(Arc::clone(&gw));
        assert!(first.is_ok(), "first build failed: {:?}", first.err());
        assert!(second.is_ok(), "second build failed: {:?}", second.err());
        // Both succeed independently — no shared mutable state prevents
        // constructing multiple gateway instances from the same backing gateway.
    }
}
