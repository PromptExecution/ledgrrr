use std::collections::HashSet;
use std::io::{self, BufRead, Write};
use std::sync::{Mutex, OnceLock};

use ledgerr_mcp::mcp_adapter;
use serde_json::{json, Value};

#[cfg(feature = "b00t")]
use ledgerr_mcp::providers::definitions::register_default_providers;
#[cfg(feature = "b00t")]
use ledgerr_mcp_core::McpProviderRegistry;

fn main() {
    // Pre-warm: construct and spawn the service actor on startup so all
    // tool calls route through the channel-based gate system.  The raw
    // service reference stays available for the existing mcp_adapter path;
    // future phases will retire the raw adapter in favor of actor dispatch.
    let _ = global_raw_service();

    #[cfg(feature = "b00t")]
    initialize_providers();

    #[cfg(feature = "http-transport")]
    if std::env::var("LEDGERR_MCP_TRANSPORT").as_deref() == Ok("http") {
        serve_http();
        return;
    }

    serve(io::stdin().lock(), io::stdout());
}

/// HTTP transport: same `handle_request` dispatcher as stdio, reached over
/// a minimal synchronous HTTP server instead of stdin/stdout. Built for
/// Azure Container Apps' HTTP-triggered scale-to-zero (see
/// PromptExecution/infrastructure#139/#141) — ACA needs an HTTP endpoint to
/// wake the app on, which stdio transport can't provide.
///
/// Deliberately does NOT adopt the `rmcp` SDK (tracked as the original ask
/// in this issue) — `handle_request` is already a pure, transport-agnostic
/// JSON-RPC dispatcher, so bridging it over HTTP needed zero changes to the
/// ~50 existing tool handlers or the well-tested stdio path. Migrating the
/// whole dispatcher onto `rmcp::ServerHandler` would be a much larger,
/// riskier rewrite (every tool handler + mcp_adapter's schema/routing) for
/// no functional gain over this — worth reconsidering only if a concrete
/// need for something `rmcp` provides (e.g. its SSE/StreamableHttp session
/// resumption) actually shows up.
///
/// Only two routes: `GET /health` (liveness probe, matches the
/// `standing-mcp-server` Terraform module's expectation) and `POST /`
/// (JSON-RPC request body in, JSON-RPC response body out). Single-threaded,
/// one request at a time — matches this binary's existing fully-synchronous
/// dispatch model (see the tokio-on-a-scratch-runtime comment on the
/// `tokio` dependency above). Revisit if concurrent request handling is
/// ever actually needed.
#[cfg(feature = "http-transport")]
fn serve_http() {
    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("0.0.0.0:{port}");
    let server = tiny_http::Server::http(&addr)
        .unwrap_or_else(|e| panic!("failed to bind {addr}: {e}"));
    tracing::info!(%addr, "ledgerr-mcp HTTP transport listening");

    for mut request in server.incoming_requests() {
        let response = match (request.method(), request.url()) {
            (tiny_http::Method::Get, "/health") => {
                tiny_http::Response::from_string("ok").with_status_code(200)
            }
            (tiny_http::Method::Post, "/") => {
                // Per-request caller identity (issue #224): HTTP is
                // stateless/multi-tenant per process, so the `X-Agent-Id`
                // header is read fresh for every request — unlike the stdio
                // transport's `LEDGERR_MCP_AGENT_ID` env var, there is no
                // single "the caller" for the life of the process here.
                let agent_id = agent_id_from_headers(request.headers());
                let mut body = String::new();
                if request.as_reader().read_to_string(&mut body).is_err() {
                    let _ = request.respond(
                        tiny_http::Response::from_string("failed to read body")
                            .with_status_code(400),
                    );
                    continue;
                }
                match serde_json::from_str::<Value>(&body) {
                    Ok(parsed) => match handle_request(parsed, agent_id.as_deref()) {
                        Some(resp) => match serde_json::to_string(&resp) {
                            Ok(s) => tiny_http::Response::from_string(s).with_status_code(200),
                            Err(e) => tiny_http::Response::from_string(format!(
                                "response serialization error: {e}"
                            ))
                            .with_status_code(500),
                        },
                        // Notifications (e.g. notifications/initialized) have no response body.
                        None => tiny_http::Response::from_string("").with_status_code(204),
                    },
                    Err(e) => tiny_http::Response::from_string(format!("invalid JSON: {e}"))
                        .with_status_code(400),
                }
            }
            _ => tiny_http::Response::from_string("not found").with_status_code(404),
        };
        let _ = request.respond(response);
    }
}

/// Extract the calling agent's identity from an `X-Agent-Id` request header
/// (issue #224). Header names are compared case-insensitively per HTTP
/// semantics (`tiny_http::HeaderField::equiv`). Absent header → `None`,
/// which `authorize_tool_call` treats identically to "no identity
/// configured" — same fail-open default as the stdio transport.
#[cfg(feature = "http-transport")]
fn agent_id_from_headers(headers: &[tiny_http::Header]) -> Option<String> {
    headers
        .iter()
        .find(|h| h.field.equiv("X-Agent-Id"))
        .map(|h| h.value.as_str().to_string())
}

#[cfg(feature = "b00t")]
fn initialize_providers() {
    let mut registry = McpProviderRegistry::new();
    register_default_providers(&mut registry, None, None);
    let results = registry.initialize_all();
    for (name, result) in &results {
        match result {
            Ok(info) => {
                tracing::info!(provider = %name, tools = info.tools.len(), "external provider registered")
            }
            Err(e) => tracing::warn!(provider = %name, error = %e, "external provider init failed"),
        }
    }
    // Store in both the local static (for handle_external_tool) and
    // the mcp_adapter global (for tools/list merging).
    ledgerr_mcp::mcp_adapter::set_global_provider_registry(registry);
}

fn serve<R: BufRead, W: Write>(reader: R, mut writer: W) {
    for line in reader.lines() {
        let Ok(raw) = line else { continue };
        let Ok(request) = serde_json::from_str::<Value>(&raw) else {
            continue;
        };
        // Per-request caller identity (issue #224): stdio is already one
        // process per agent, so `LEDGERR_MCP_AGENT_ID` is process-scoped —
        // read fresh each line for simplicity/symmetry with the HTTP path,
        // but it is expected to be stable for the process lifetime, matching
        // the existing `LEDGERR_MCP_MANIFEST` convention.
        let agent_id = env_agent_id();
        if let Some(response) = handle_request(request, agent_id.as_deref()) {
            if let Ok(serialized) = serde_json::to_string(&response) {
                let _ = writeln!(writer, "{serialized}");
                let _ = writer.flush();
            }
        }
    }
}

/// Read the calling agent's identity from `LEDGERR_MCP_AGENT_ID` (issue
/// #224). Unset or blank → `None`, meaning "no identity configured" — the
/// same fail-open default `authorize_tool_call` uses to leave `tools/call`
/// completely unenforced, matching pre-#224 behavior.
fn env_agent_id() -> Option<String> {
    std::env::var("LEDGERR_MCP_AGENT_ID")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Process-wide ring for `tools/list` visibility filtering, from the
/// `LEDGERR_MCP_RING` env var (`admin` | `standard` | `restricted` |
/// `sandboxed`, case-insensitive). Unset or unrecognized values return
/// `None`, which `filter_tools_for_ring` treats as "no filtering" — the
/// server's long-standing default of showing every tool to every caller.
///
/// This is a stand-in for real per-caller identity (see
/// `docs/progressive-tool-scoping-design.md`): neither the stdio nor the
/// additive HTTP transport (#223) currently carries a calling-agent
/// identity, so this option applies one ring to the whole server process
/// rather than gating per request. It exists to prove the
/// `RingEnforcer`-derived `tools/list` filtering path end-to-end ahead of
/// that larger, separately-tracked identity-plumbing work.
fn configured_ring() -> Option<msft_agent_gov_ledgrrr::Ring> {
    std::env::var("LEDGERR_MCP_RING")
        .ok()
        .and_then(|s| msft_agent_gov_ledgrrr::rings::ring_from_env_str(&s))
}

/// One shared `LedgrrAgtGateway` for the process (issue #224/#225).
///
/// `LedgrrAgtGateway::new(agent_id)` takes an `agent_id` only to seed the
/// gateway's *own* `AgentMeshClient` identity (its DID, audit-log actor,
/// etc.) — it is not the identity of any MCP caller. `check_tool_call` and
/// `register_agent` already take the calling agent's id as a per-call
/// parameter (see `authorize_tool_call` below), so a single gateway
/// instance is correctly multi-agent-capable; only the *caller* identity
/// needs to vary per request, not the gateway itself. Mirrors the existing
/// `global_raw_service()` `OnceLock` pattern in this file.
fn global_gateway() -> &'static msft_agent_gov_ledgrrr::LedgrrAgtGateway {
    static GATEWAY: OnceLock<msft_agent_gov_ledgrrr::LedgrrAgtGateway> = OnceLock::new();
    GATEWAY.get_or_init(|| {
        msft_agent_gov_ledgrrr::LedgrrAgtGateway::new("ledgerr-mcp-server")
            .expect("LedgrrAgtGateway::new must succeed with the default policy")
    })
}

/// Register `agent_id` with `gw` at most once per process, the first time
/// it's seen. `LedgrrAgtGateway::register_agent` is not a pure no-op on an
/// already-known agent — it unconditionally re-assigns `Ring::Standard` —
/// so calling it on every request would silently undo any later promotion
/// (e.g. `promote_to_admin`). A process-local "seen" set makes onboarding a
/// new caller idempotent without that clobbering risk.
fn auto_register_once(gw: &msft_agent_gov_ledgrrr::LedgrrAgtGateway, agent_id: &str) {
    static SEEN: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let seen = SEEN.get_or_init(|| Mutex::new(HashSet::new()));
    let mut seen = seen.lock().unwrap_or_else(|e| e.into_inner());
    if seen.insert(agent_id.to_string()) {
        gw.register_agent(agent_id);
    }
}

/// Top-level `ledgerr_*` tool-family names actually modeled by
/// `msft_agent_gov_ledgrrr`'s AGT policy contract — i.e.
/// `msft_agent_gov_ledgrrr::policy::PUBLISHED_TOOL_NAMES`, spelled out here
/// as the `mcp_adapter` constants already used in the `tools/call` match
/// below (`policy` is not a public module of `msft-agent-gov-ledgrrr`).
const AGT_GATED_TOOL_FAMILIES: &[&str] = &[
    mcp_adapter::DOCUMENTS_TOOL,
    mcp_adapter::REVIEW_TOOL,
    mcp_adapter::RECONCILIATION_TOOL,
    mcp_adapter::WORKFLOW_TOOL,
    mcp_adapter::AUDIT_TOOL,
    mcp_adapter::TAX_TOOL,
    mcp_adapter::ONTOLOGY_TOOL,
    mcp_adapter::XERO_TOOL,
    mcp_adapter::EVIDENCE_TOOL,
    mcp_adapter::FOCUS_TOOL,
];

/// Governance gate for `tools/call` (issue #225: "Wire
/// `LedgrrAgtGateway::check_tool_call` into `tools/call` dispatch").
///
/// Returns `None` when the call may proceed, `Some(error_result)` (an MCP
/// tool-result envelope with `isError: true`) when it's denied.
///
/// Scope, deliberately narrow:
/// - `agent_id: None` (no `LEDGERR_MCP_AGENT_ID` / `X-Agent-Id` configured)
///   → always `None` (proceed). Preserves exact pre-#224 behavior — identity
///   and enforcement are both strictly opt-in.
/// - Only tool names in `AGT_GATED_TOOL_FAMILIES` are gated. `ledgerr_schema`,
///   `ledgerr_manifest`, `ledgerr_budget`, every legacy `l3dg3rr_*` tool, and
///   external b00t-provider tools have no action-pattern mapping in
///   `rings.rs`/`policy.rs` at all — gating them here would be an
///   undocumented blanket deny bolted onto identity plumbing, not a real
///   authorization decision. This matches `rings::CORE_TOOL_FAMILIES`'s
///   existing precedent (schema/manifest are already "outside the AGT
///   policy contract" for `tools/list` filtering, per PR #232).
///
/// Known limitation, not fixed here (flagged in issue #225 itself):
/// `LedgrrAgtGateway::check_tool_call` does not consult
/// `RingEnforcer::check_access`'s per-ring permission table — the
/// Allow/Deny/RequiresApproval decision for Standard and Restricted rings
/// both come from the same ring-blind `LEDGERR_POLICY_YAML` evaluation, so
/// they are authorized identically here. This wiring is still a real
/// improvement (`tools/call` previously enforced *nothing at all*, for any
/// caller), but ring-differentiated call-time enforcement remains separate
/// follow-up work — seeding `RingEnforcer`'s configured permission lists
/// with the wildcard action-patterns from `rings.rs` also does not
/// glob-match today (`RingEnforcer::check_access` compares actions for
/// exact string equality), so wiring it in naively would misfire on every
/// `"family.*"`-pattern entry.
fn authorize_tool_call(agent_id: Option<&str>, tool_name: &str, params: &Value) -> Option<Value> {
    let agent_id = agent_id?;
    if !AGT_GATED_TOOL_FAMILIES.contains(&tool_name) {
        return None;
    }

    let gw = global_gateway();
    auto_register_once(gw, agent_id);

    let action = params
        .get("arguments")
        .and_then(|a| a.get("action"))
        .and_then(Value::as_str)
        .unwrap_or("");

    let decision = gw.check_tool_call(agent_id, tool_name, action);
    if decision.allowed {
        return None;
    }

    let reason = decision
        .reason
        .unwrap_or_else(|| format!("{:?}", decision.policy));
    Some(mcp_adapter::governance_denied_result(tool_name, &reason))
}

fn handle_request(request: Value, agent_id: Option<&str>) -> Option<Value> {
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
                    "name": "ledgerr-mcp",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        })),
        "notifications/initialized" => None,
        "tools/list" => {
            let tools = mcp_adapter::tool_descriptors();
            let tools = mcp_adapter::filter_tools_for_ring(tools, configured_ring());
            Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "tools": tools }
            }))
        }
        "tools/call" => {
            let params = request.get("params").cloned().unwrap_or(Value::Null);
            let tool_name = params.get("name").and_then(Value::as_str).unwrap_or("");

            // Issue #225: governance gate for tools/call. No-op (returns
            // None) when `agent_id` is `None` — same unenforced default as
            // before this change — or when `tool_name` isn't one of the
            // AGT-modeled `ledgerr_*` families. See `authorize_tool_call`.
            if let Some(denial) = authorize_tool_call(agent_id, tool_name, &params) {
                return Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": denial
                }));
            }

            let result = match tool_name {
                mcp_adapter::DOCUMENTS_TOOL => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_documents_tool(global_raw_service(), &arguments)
                }
                mcp_adapter::REVIEW_TOOL => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_review_tool(global_raw_service(), &arguments)
                }
                mcp_adapter::RECONCILIATION_TOOL => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_reconciliation_tool(global_raw_service(), &arguments)
                }
                mcp_adapter::WORKFLOW_TOOL => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_workflow_tool(global_raw_service(), &arguments)
                }
                mcp_adapter::AUDIT_TOOL => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_audit_tool(global_raw_service(), &arguments)
                }
                mcp_adapter::TAX_TOOL => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_tax_tool(global_raw_service(), &arguments)
                }
                mcp_adapter::ONTOLOGY_TOOL => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_ontology_tool(global_raw_service(), &arguments)
                }
                mcp_adapter::XERO_TOOL => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_xero_tool(global_raw_service(), &arguments)
                }
                mcp_adapter::EVIDENCE_TOOL => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_evidence_tool(global_raw_service(), &arguments)
                }
                mcp_adapter::FOCUS_TOOL => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_focus_tool(&arguments)
                }
                mcp_adapter::BUDGET_TOOL => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_budget_tool(&arguments)
                }
                "l3dg3rr_list_accounts" => mcp_adapter::handle_list_accounts(global_raw_service()),
                "l3dg3rr_get_pipeline_status" => {
                    let docling_ready = b00t_iface::docling::DoclingProcessSurface::new().is_ready();
                    mcp_adapter::handle_pipeline_status(true, true, docling_ready, Vec::new())
                }
                "proxy_docling_ingest_pdf" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_ingest_pdf(
                        global_raw_service(),
                        &arguments,
                        Some(format!("mcp-call-{id}")),
                    )
                }
                "proxy_rustledger_ingest_statement_rows" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_ingest_statement_rows(
                        global_raw_service(),
                        &arguments,
                        Some(format!("mcp-call-{id}")),
                    )
                }
                "l3dg3rr_get_raw_context" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_get_raw_context(global_raw_service(), &arguments)
                }
                "l3dg3rr_ontology_query_path" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_ontology_query_path(global_raw_service(), &arguments)
                }
                "l3dg3rr_ontology_export_snapshot" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_ontology_export_snapshot(global_raw_service(), &arguments)
                }
                "l3dg3rr_validate_reconciliation" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::dispatch_reconciliation(
                        global_raw_service(),
                        "validate",
                        &arguments,
                    )
                }
                "l3dg3rr_reconcile_postings" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::dispatch_reconciliation(
                        global_raw_service(),
                        "reconcile",
                        &arguments,
                    )
                }
                "l3dg3rr_commit_guarded" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::dispatch_reconciliation(global_raw_service(), "commit", &arguments)
                }
                "l3dg3rr_hsm_transition" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::dispatch_hsm(global_raw_service(), "transition", &arguments)
                }
                "l3dg3rr_hsm_status" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::dispatch_hsm(global_raw_service(), "status", &arguments)
                }
                "l3dg3rr_hsm_resume" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::dispatch_hsm(global_raw_service(), "resume", &arguments)
                }
                "l3dg3rr_event_history" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_event_history(global_raw_service(), &arguments)
                }
                "l3dg3rr_event_replay" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_event_replay(global_raw_service(), &arguments)
                }
                "l3dg3rr_tax_assist" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_tax_assist(global_raw_service(), &arguments)
                }
                "l3dg3rr_tax_evidence_chain" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_tax_evidence_chain(global_raw_service(), &arguments)
                }
                "l3dg3rr_tax_ambiguity_review" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_tax_ambiguity_review(global_raw_service(), &arguments)
                }
                "l3dg3rr_classify_ingested" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_classify_ingested(global_raw_service(), &arguments)
                }
                "l3dg3rr_query_flags" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_query_flags(global_raw_service(), &arguments)
                }
                "l3dg3rr_query_audit_log" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_query_audit_log(global_raw_service(), &arguments)
                }
                "l3dg3rr_classify_transaction" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_classify_transaction(global_raw_service(), &arguments)
                }
                "l3dg3rr_reconcile_excel_classification" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_reconcile_excel_classification(
                        global_raw_service(),
                        &arguments,
                    )
                }
                "l3dg3rr_get_schedule_summary" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_get_schedule_summary(global_raw_service(), &arguments)
                }
                "l3dg3rr_export_cpa_workbook" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_export_cpa_workbook(global_raw_service(), &arguments)
                }
                "l3dg3rr_ontology_upsert_entities" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_ontology_upsert_entities(global_raw_service(), &arguments)
                }
                "l3dg3rr_ontology_upsert_edges" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_ontology_upsert_edges(global_raw_service(), &arguments)
                }
                "l3dg3rr_plugin_info" => {
                    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
                    mcp_adapter::handle_workflow_tool(
                        global_raw_service(),
                        &json!({ "action": "plugin_info", "subcommand": arguments.get("subcommand").cloned().unwrap_or(Value::String("check".to_string())) }),
                    )
                }
                _ => {
                    #[cfg(feature = "b00t")]
                    {
                        // Registry is accessed internally by handle_external_tool
                        // via mcp_adapter's GLOBAL_PROVIDER_REGISTRY.
                        let ext_args = params.get("arguments").cloned().unwrap_or(Value::Null);
                        mcp_adapter::handle_external_tool(tool_name, &ext_args)
                    }
                    #[cfg(not(feature = "b00t"))]
                    {
                        mcp_adapter::unknown_tool_result(tool_name)
                    }
                }
            };
            Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": result
            }))
        }
        _ => Some(mcp_adapter::protocol_method_not_found(id, method)),
    }
}

/// Build the service and leak a raw reference for the adapter path.
fn build_service() -> &'static ledgerr_mcp::TurboLedgerService {
    let manifest = std::env::var("LEDGERR_MCP_MANIFEST").unwrap_or_else(|_| {
        "[session]\nworkbook_path=\"tax-ledger.xlsx\"\nactive_year=2023\n\n[accounts]\nWF-BH-CHK = { institution = \"Wells Fargo\", type = \"checking\", currency = \"USD\" }\n".to_string()
    });
    let raw = Box::new(
        ledgerr_mcp::TurboLedgerService::from_manifest_str(&manifest)
            .expect("default manifest must parse"),
    );
    Box::leak(raw)
}

fn global_raw_service() -> &'static ledgerr_mcp::TurboLedgerService {
    static SERVICE: OnceLock<&'static ledgerr_mcp::TurboLedgerService> = OnceLock::new();
    *SERVICE.get_or_init(build_service)
}
