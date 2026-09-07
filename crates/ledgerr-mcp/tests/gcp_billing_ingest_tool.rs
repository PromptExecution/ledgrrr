//! Unit-level coverage for the `ledgerr_gcp_billing` MCP tool (GCP BigQuery
//! Billing Export FOCUS ingestion, epic PRD-013).
//!
//! No live GCP BigQuery Billing Export exists yet (enabling it is a manual
//! GCP console step that has not been performed) and the `bq` CLI is not
//! assumed present in this environment either. These tests therefore only
//! exercise:
//! - `dry_run_map_row`, which never touches BigQuery at all.
//! - `query_last_run`, which is a static, no-IO response.
//! - `ingest_since`'s input-validation and missing-config error paths,
//!   which return before any subprocess is spawned.
//! - the MCP tool contract (`PUBLISHED_TOOLS`) shape.
//!
//! Mirrors `tests/budget_reconcile_tool.rs`'s pattern of calling the
//! handler fn directly (bypassing JSON-RPC framing) on a background thread
//! with a timeout, and asserting the response envelope shape.

use serde_json::{json, Value};
use std::sync::mpsc;
use std::time::Duration;

/// Run `handle_gcp_billing_tool` on a background thread with a generous
/// timeout, so an unexpected hang (e.g. a real `bq` binary present and
/// itself hanging on auth) fails the test fast instead of blocking the
/// suite indefinitely.
fn call_gcp_billing_tool_with_timeout(arguments: Value) -> Value {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = ledgerr_mcp::mcp_adapter::handle_gcp_billing_tool(&arguments);
        let _ = tx.send(result);
    });
    rx.recv_timeout(Duration::from_secs(30))
        .expect("handle_gcp_billing_tool did not return within 30s")
}

fn sample_raw_row() -> Value {
    json!({
        "BillingAccountId": "012345-6789AB-CDEF01",
        "BillingAccountName": "Acme Corp Billing",
        "BillingCurrency": "USD",
        "BillingPeriodStart": "2024-06-01T00:00:00Z",
        "BillingPeriodEnd": "2024-07-01T00:00:00Z",
        "ChargePeriodStart": "2024-06-15T00:00:00Z",
        "ChargePeriodEnd": "2024-06-16T00:00:00Z",
        "ChargeCategory": "Usage",
        "ChargeFrequency": "Usage-Based",
        "BilledCost": "42.00",
        "EffectiveCost": "40.00",
        "ServiceProviderName": "Google Cloud",
        "ServiceName": "Compute Engine",
        "SkuId": "SKU-0001",
        "SubAccountId": "987654321000",
        "Tags": { "env": "prod" }
    })
}

#[test]
fn dry_run_map_row_maps_a_valid_row() {
    let response = call_gcp_billing_tool_with_timeout(json!({
        "action": "dry_run_map_row",
        "raw_row_json": sample_raw_row(),
    }));

    assert_eq!(response["isError"], Value::Bool(false));
    let text = response["content"][0]["text"]
        .as_str()
        .expect("text content");
    let mapped: Value = serde_json::from_str(text).expect("mapped row is valid JSON");

    assert_eq!(mapped["billing_account_id"], json!("012345-6789AB-CDEF01"));
    assert_eq!(mapped["service_name"], json!("Compute Engine"));
    assert_eq!(mapped["sub_account_id"], json!("987654321000"));
    assert_eq!(mapped["tags"]["env"], json!("prod"));
}

#[test]
fn dry_run_map_row_rejects_row_missing_mandatory_field() {
    let mut row = sample_raw_row();
    row.as_object_mut().unwrap().remove("BilledCost");

    let response = call_gcp_billing_tool_with_timeout(json!({
        "action": "dry_run_map_row",
        "raw_row_json": row,
    }));

    assert_eq!(response["isError"], Value::Bool(true));
    let text = response["content"][0]["text"]
        .as_str()
        .expect("text content");
    assert!(text.contains("BilledCost"));
}

#[test]
fn dry_run_map_row_rejects_unknown_action() {
    let response = call_gcp_billing_tool_with_timeout(json!({ "action": "not_a_real_action" }));
    assert_eq!(response["isError"], Value::Bool(true));
}

#[test]
fn query_last_run_reports_no_persisted_watermark() {
    let response = call_gcp_billing_tool_with_timeout(json!({ "action": "query_last_run" }));

    assert_eq!(response["isError"], Value::Bool(false));
    let text = response["content"][0]["text"]
        .as_str()
        .expect("text content");
    let payload: Value = serde_json::from_str(text).expect("payload is valid JSON");
    assert!(payload["last_run"].is_null());
}

#[test]
fn ingest_since_rejects_invalid_timestamp() {
    let response = call_gcp_billing_tool_with_timeout(json!({
        "action": "ingest_since",
        "since": "not-a-timestamp",
    }));

    assert_eq!(response["isError"], Value::Bool(true));
    let text = response["content"][0]["text"]
        .as_str()
        .expect("text content");
    assert!(text.contains("RFC 3339"));
}

/// `ingest_since` with a valid timestamp but no `GCP_BILLING_*` env config
/// must fail with a clear `InvalidInput`, not panic or attempt a query.
///
/// Run as a single test (rather than split across parallel `#[test]` fns)
/// because it mutates process-wide environment variables that a sibling
/// test could otherwise race against.
#[test]
fn ingest_since_env_config_paths() {
    // Ensure a clean slate regardless of ambient environment.
    std::env::remove_var("GCP_BILLING_PROJECT_ID");
    std::env::remove_var("GCP_BILLING_DATASET");
    std::env::remove_var("GCP_BILLING_TABLE");

    let missing_config = call_gcp_billing_tool_with_timeout(json!({
        "action": "ingest_since",
        "since": "2024-06-01T00:00:00Z",
    }));
    assert_eq!(missing_config["isError"], Value::Bool(true));
    let text = missing_config["content"][0]["text"]
        .as_str()
        .expect("text content");
    assert!(text.contains("GCP_BILLING_PROJECT_ID"));

    // With config present but no `bq` binary installed in this environment,
    // the subprocess spawn itself fails — still a well-formed error
    // envelope, never a panic or a hang.
    std::env::set_var("GCP_BILLING_PROJECT_ID", "acme-billing");
    std::env::set_var("GCP_BILLING_DATASET", "billing_export");
    std::env::set_var("GCP_BILLING_TABLE", "gcp_billing_export_resource_v1");

    let response = call_gcp_billing_tool_with_timeout(json!({
        "action": "ingest_since",
        "since": "2024-06-01T00:00:00Z",
    }));

    std::env::remove_var("GCP_BILLING_PROJECT_ID");
    std::env::remove_var("GCP_BILLING_DATASET");
    std::env::remove_var("GCP_BILLING_TABLE");

    // Whether this environment happens to have a `bq` binary on PATH is not
    // something this test controls; either a well-formed success or a
    // well-formed error envelope is acceptable — a panic is not.
    assert!(response["isError"].is_boolean());
    assert!(response["content"][0]["text"].is_string());
}

#[test]
fn gcp_billing_tool_is_published_with_expected_actions() {
    use ledgerr_mcp::contract::{GCP_BILLING_TOOL, PUBLISHED_TOOLS};

    let spec = PUBLISHED_TOOLS
        .iter()
        .find(|t| t.name == GCP_BILLING_TOOL)
        .expect("ledgerr_gcp_billing must be published");
    assert!(spec.actions.contains(&"ingest_since"));
    assert!(spec.actions.contains(&"query_last_run"));
    assert!(spec.actions.contains(&"dry_run_map_row"));
}
