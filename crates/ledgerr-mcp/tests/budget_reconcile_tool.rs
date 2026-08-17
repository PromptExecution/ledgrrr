//! Unit-level coverage for the `ledgerr_budget` MCP tool (gh#111 Phase 3).
//!
//! `handle_budget_tool` drives `ledgerr_cloud::ReconcileRunner` — which
//! itself shells out to the `aws`/`gcloud`/`az`/`hf` CLIs — but per
//! `ReconcileRunner::run`'s own contract, no single provider's failure (nor
//! the total absence of a CLI/credentials) can panic or produce a malformed
//! report. These tests assert the MCP envelope/report *shape* only; they do
//! not require any real cloud credentials to pass, and use a background
//! thread + timeout so an unexpectedly slow CLI call in some environment
//! fails the test instead of hanging it.

use serde_json::{json, Value};
use std::sync::mpsc;
use std::time::Duration;

/// Run `handle_budget_tool` on a background thread with a generous timeout,
/// so a CLI hang in an unusual environment fails fast instead of blocking
/// the test suite indefinitely.
fn call_budget_tool_with_timeout(arguments: Value) -> Value {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = ledgerr_mcp::mcp_adapter::handle_budget_tool(&arguments);
        let _ = tx.send(result);
    });
    rx.recv_timeout(Duration::from_secs(30))
        .expect("handle_budget_tool did not return within 30s")
}

#[test]
fn budget_reconcile_returns_well_formed_report_shape() {
    let response = call_budget_tool_with_timeout(json!({ "action": "reconcile" }));

    assert_eq!(response["isError"], Value::Bool(false));
    let text = response["content"][0]["text"]
        .as_str()
        .expect("text content");
    let report: Value = serde_json::from_str(text).expect("report is valid JSON");

    let providers = report["providers"].as_array().expect("providers array");
    assert_eq!(providers.len(), 4, "expected one entry per provider");

    let names: Vec<&str> = providers
        .iter()
        .map(|p| p["provider"].as_str().expect("provider name"))
        .collect();
    assert_eq!(names, ["aws", "gcp", "azure", "hf"]);

    for p in providers {
        let status = p["status"].as_str().expect("status string");
        assert!(
            matches!(status, "pass" | "fail" | "skip"),
            "unexpected status value: {status}"
        );
        // authoritative and error must always be present (bool / null-or-string).
        assert!(p.get("authoritative").and_then(Value::as_bool).is_some());
        assert!(p.get("cap_cake").is_some());
        assert!(p.get("error").is_some());
    }
}

#[test]
fn budget_reconcile_rejects_unknown_action() {
    let response = call_budget_tool_with_timeout(json!({ "action": "not_a_real_action" }));
    assert_eq!(response["isError"], Value::Bool(true));
}

#[test]
fn budget_tool_is_published_with_reconcile_action() {
    use ledgerr_mcp::contract::{BUDGET_TOOL, PUBLISHED_TOOLS};

    let spec = PUBLISHED_TOOLS
        .iter()
        .find(|t| t.name == BUDGET_TOOL)
        .expect("ledgerr_budget must be published");
    assert!(spec.actions.contains(&"reconcile"));
}
