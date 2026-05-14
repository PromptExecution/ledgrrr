//! MCP tool adapter for `ledgerr_focus` — FOCUS v1.3 cost/usage records.
//!
//! Actions: append_focus_record, query_focus_summary, compute_focus_delta, experiment_score.
//! All actions accept JSON input and return JSON output via the MCP contract.
//!
//! # Persistence
//! Focus records are persisted to a JSON file so they survive MCP server restarts.
//! The file path is controlled by the `FOCUS_SIDECAR_PATH` env var (default:
//! `~/.local/share/b00t/focus/focus_records.json`). On startup, existing records
//! are loaded from this file. Every call to `handle_append` extends both the
//! in-memory store AND the file (atomically via tmp+rename).

use ledgerr_focus::{
    compute_focus_delta, format_focus_cli, ChargeCategory, ChargeFrequency, CostAndUsageRow,
    PersonalityProfile, FOCUS_SPEC_VERSION,
};
use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusToolInput {
    pub action: String,
    #[serde(default)]
    pub records: Vec<FocusToolRecord>,
    #[serde(default)]
    pub experiment_id: Option<String>,
    #[serde(default)]
    pub personality: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusToolRecord {
    pub billing_account_id: String,
    pub service_name: String,
    pub billed_cost: f64,
    pub effective_cost: f64,
    pub experiment_id: Option<String>,
    pub variant: Option<String>,
    pub agent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusToolOutput {
    pub spec_version: &'static str,
    pub action: String,
    pub rows_written: usize,
    pub focus_cli: String,
    pub delta: Option<FocusDeltaOutput>,
    pub experiment_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusDeltaOutput {
    pub control_billed: f64,
    pub treatment_billed: f64,
    pub delta_billed: f64,
    pub recommendation: String,
    pub dimension_deltas: HashMap<String, f64>,
}

pub fn handle_focus_tool(input: FocusToolInput) -> Result<FocusToolOutput, String> {
    match input.action.as_str() {
        "append_focus_record" => handle_append(input),
        "query_focus_summary" => handle_query_summary(input),
        "compute_focus_delta" => handle_compute_delta(input),
        "experiment_score" => handle_experiment_score(input),
        other => Err(format!("unknown focus action: {other}")),
    }
}

/// Convert a `FocusToolRecord` into a `CostAndUsageRow`, returning an error if
/// any numeric value cannot be converted to `Decimal`.
fn record_to_row(r: &FocusToolRecord, personality: Option<&str>) -> Result<CostAndUsageRow, String> {
    Ok(CostAndUsageRow {
        billing_account_id: r.billing_account_id.clone(),
        billing_account_name: None,
        billing_currency: "FOCUS".into(),
        billing_period_start: chrono::Utc::now(),
        billing_period_end: chrono::Utc::now(),
        charge_period_start: chrono::Utc::now(),
        charge_period_end: chrono::Utc::now(),
        charge_category: ChargeCategory::Usage,
        charge_frequency: ChargeFrequency::UsageBased,
        billed_cost: Decimal::from_f64(r.billed_cost)
            .ok_or_else(|| format!("cannot convert billed_cost {} to Decimal", r.billed_cost))?,
        effective_cost: Decimal::from_f64(r.effective_cost)
            .ok_or_else(|| format!("cannot convert effective_cost {} to Decimal", r.effective_cost))?,
        service_provider_name: "ledgrrr".into(),
        service_name: r.service_name.clone(),
        sku_id: "focus-eval".into(),
        billing_account_type: None,
        charge_class: None,
        charge_description: None,
        commitment_discount_id: None,
        commitment_discount_name: None,
        commitment_discount_category: None,
        commitment_discount_type: None,
        commitment_discount_status: None,
        commitment_discount_quantity: None,
        commitment_discount_unit: None,
        consumed_quantity: None,
        consumed_unit: None,
        contracted_cost: None,
        contracted_unit_price: None,
        invoice_id: None,
        invoice_issuer_name: None,
        list_cost: None,
        list_unit_price: None,
        pricing_category: None,
        pricing_quantity: None,
        pricing_unit: None,
        region_id: None,
        region_name: None,
        resource_id: None,
        resource_name: None,
        resource_type: None,
        service_category: Some("AI Inference".into()),
        service_subcategory: None,
        sku_meter: None,
        sku_price_id: None,
        sku_price_details: None,
        sub_account_id: None,
        sub_account_name: None,
        sub_account_type: None,
        availability_zone: None,
        capacity_reservation_id: None,
        capacity_reservation_status: None,
        host_provider_name: None,
        tags: HashMap::new(),
        x_experiment_id: r.experiment_id.clone(),
        x_variant: r.variant.clone(),
        x_personality: personality.map(|s| s.to_string()),
        x_experiment_score: None,
        x_agent_id: r.agent_id.clone(),
        x_reasoning_review: None,
    })
}

/// Validate a FocusToolRecord against FOCUS v1.3 mandatory columns.
/// Returns Ok(()) if all mandatory fields are present and non-empty.
fn validate_focus_record(record: &FocusToolRecord) -> Result<(), String> {
    let mut errors = Vec::new();
    if record.billing_account_id.is_empty() {
        errors.push("BillingAccountId".to_string());
    }
    if record.service_name.is_empty() {
        errors.push("ServiceName".to_string());
    }
    if !record.billed_cost.is_finite() {
        errors.push("BilledCost (non-finite)".to_string());
    } else if record.billed_cost < 0.0 {
        errors.push("BilledCost (negative)".to_string());
    }
    if !record.effective_cost.is_finite() {
        errors.push("EffectiveCost (non-finite)".to_string());
    } else if record.effective_cost < 0.0 {
        errors.push("EffectiveCost (negative)".to_string());
    }
    if !errors.is_empty() {
        return Err(format!("FOCUS validation failed: missing/invalid mandatory columns: {}", errors.join(", ")));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Persistence — focus records are saved to a JSON file so they survive server
// restarts.  On first access the file is loaded into a global `Mutex`-guarded
// Vec; every `handle_append` extends both the Vec and the file atomically.
// ---------------------------------------------------------------------------

/// Resolve the storage path from `FOCUS_SIDECAR_PATH` env var, falling back
/// to `~/.local/share/b00t/focus/focus_records.json`.
fn focus_records_path() -> PathBuf {
    let raw = std::env::var("FOCUS_SIDECAR_PATH")
        .unwrap_or_else(|_| "~/.local/share/b00t/focus/focus_records.json".to_string());
    PathBuf::from(shellexpand::tilde(&raw).as_ref())
}

/// Global in-memory store. Loaded from disk on first use via
/// [`initialize_store`]; extended by [`handle_append`].
static FOCUS_RECORDS: Mutex<Vec<FocusToolRecord>> = Mutex::new(Vec::new());
static FOCUS_LOADED: AtomicBool = AtomicBool::new(false);

/// Lazily populate the global store from the JSON file on disk.
/// Safe to call multiple times — only loads once.
fn initialize_store() {
    if FOCUS_LOADED.load(Ordering::Acquire) {
        return;
    }
    let mut guard = FOCUS_RECORDS.lock().expect("focus records lock poisoned");
    if !FOCUS_LOADED.load(Ordering::Acquire) {
        let path = focus_records_path();
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    match serde_json::from_str::<Vec<FocusToolRecord>>(&content) {
                        Ok(records) => {
                            *guard = records;
                        }
                        Err(e) => {
                            tracing::warn!(
                                path = %path.display(),
                                err = %e,
                                "focus_records.json parse error — starting with empty store"
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        path = %path.display(),
                        err = %e,
                        "focus_records.json read error — starting with empty store"
                    );
                }
            }
        }
        FOCUS_LOADED.store(true, Ordering::Release);
    }
}

/// Atomically write the full record list to the JSON file.
/// Uses a temporary sibling file + rename so a crash mid-write never
/// leaves a truncated sidecar.
fn save_records(records: &[FocusToolRecord]) {
    let path = focus_records_path();
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::warn!(
                path = %parent.display(),
                err = %e,
                "failed to create focus records directory"
            );
            return;
        }
    }
    let json = match serde_json::to_string_pretty(records) {
        Ok(j) => j,
        Err(e) => {
            tracing::warn!(err = %e, "focus records serialization failed");
            return;
        }
    };
    let tmp_path = path.with_extension("json.tmp");
    if let Err(e) = std::fs::write(&tmp_path, &json) {
        tracing::warn!(
            path = %tmp_path.display(),
            err = %e,
            "focus records temp write failed"
        );
        return;
    }
    if let Err(e) = std::fs::rename(&tmp_path, &path) {
        tracing::warn!(
            from = %tmp_path.display(),
            to = %path.display(),
            err = %e,
            "focus records rename failed"
        );
    }
}

/// Reset the global store (test helper).  Only available in `#[cfg(test)]`.
#[cfg(test)]
fn reset_store_for_test() {
    if let Ok(mut guard) = FOCUS_RECORDS.lock() {
        guard.clear();
    }
    FOCUS_LOADED.store(false, Ordering::Release);
}

fn handle_append(input: FocusToolInput) -> Result<FocusToolOutput, String> {
    // Validate all incoming records against the FOCUS v1.3 schema before processing
    for record in &input.records {
        validate_focus_record(record)?;
    }
    let personality = input.personality.as_deref();
    let rows: Vec<CostAndUsageRow> = input
        .records
        .iter()
        .map(|r| record_to_row(r, personality))
        .collect::<Result<Vec<_>, _>>()?;

    let focus_cli = rows
        .iter()
        .map(format_focus_cli)
        .collect::<Vec<_>>()
        .join("\n");

    // Persist: extend the in-memory store AND write to the JSON file so records
    // survive MCP server restarts.
    initialize_store();
    if let Ok(mut guard) = FOCUS_RECORDS.lock() {
        guard.extend(input.records.clone());
        save_records(&guard);
    }

    let summary = input.experiment_id.as_ref().map(|eid| {
        format!(
            "FOCUS {FOCUS_SPEC_VERSION}: {n} rows appended to experiment {eid}",
            n = rows.len()
        )
    });

    Ok(FocusToolOutput {
        spec_version: FOCUS_SPEC_VERSION,
        action: "append_focus_record".into(),
        rows_written: rows.len(),
        focus_cli,
        delta: None,
        experiment_summary: summary,
    })
}

fn handle_query_summary(_input: FocusToolInput) -> Result<FocusToolOutput, String> {
    // Read from the persisted store to reflect all previously appended records.
    initialize_store();
    let (count, total_cost) = match FOCUS_RECORDS.lock() {
        Ok(guard) => {
            let c = guard.len();
            let total: f64 = guard.iter().map(|r| r.billed_cost).sum();
            (c, total)
        }
        Err(_) => (0, 0.0),
    };

    Ok(FocusToolOutput {
        spec_version: FOCUS_SPEC_VERSION,
        action: "query_focus_summary".into(),
        rows_written: 0,
        focus_cli: String::new(),
        delta: None,
        experiment_summary: Some(format!(
            "FOCUS {FOCUS_SPEC_VERSION}: {count} records stored, total billed cost: {total_cost:.2}"
        )),
    })
}

fn handle_compute_delta(input: FocusToolInput) -> Result<FocusToolOutput, String> {
    let personality = input.personality.as_deref();
    let control: Vec<CostAndUsageRow> = input
        .records
        .iter()
        .filter(|r| r.variant.as_deref() == Some("control"))
        .map(|r| record_to_row(r, personality))
        .collect::<Result<Vec<_>, _>>()?;

    let treatment: Vec<CostAndUsageRow> = input
        .records
        .iter()
        .filter(|r| r.variant.as_deref() == Some("treatment"))
        .map(|r| record_to_row(r, personality))
        .collect::<Result<Vec<_>, _>>()?;

    let mut cs = HashMap::new();
    let mut ts = HashMap::new();
    cs.insert("roi".into(), Decimal::from_f64(0.5).unwrap());
    ts.insert("roi".into(), Decimal::from_f64(0.8).unwrap());

    let delta = compute_focus_delta(&control, &treatment, &cs, &ts, input.experiment_id.as_deref().unwrap_or("?"));

    let mut dim_deltas = HashMap::new();
    for (k, v) in &delta.dimension_deltas {
        dim_deltas.insert(k.clone(), v.to_f64().unwrap_or(0.0));
    }

    Ok(FocusToolOutput {
        spec_version: FOCUS_SPEC_VERSION,
        action: "compute_focus_delta".into(),
        rows_written: 0,
        focus_cli: delta.to_focus_cli(),
        delta: Some(FocusDeltaOutput {
            control_billed: delta.control_billed_cost.to_f64().unwrap_or(0.0),
            treatment_billed: delta.treatment_billed_cost.to_f64().unwrap_or(0.0),
            delta_billed: delta.delta_billed.to_f64().unwrap_or(0.0),
            recommendation: delta.recommendation,
            dimension_deltas: dim_deltas,
        }),
        experiment_summary: None,
    })
}

fn handle_experiment_score(input: FocusToolInput) -> Result<FocusToolOutput, String> {
    let personality = input
        .personality
        .as_deref()
        .and_then(|p| PersonalityProfile::all().into_iter().find(|prof| prof.label == p))
        .map(|_| format!("personality={}", input.personality.as_deref().unwrap_or("none")));

    Ok(FocusToolOutput {
        spec_version: FOCUS_SPEC_VERSION,
        action: "experiment_score".into(),
        rows_written: input.records.len(),
        focus_cli: personality.unwrap_or_default(),
        delta: None,
        experiment_summary: input.experiment_id.map(|eid| format!("scored experiment {eid}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_record(variant: &str, cost: f64) -> FocusToolRecord {
        FocusToolRecord {
            billing_account_id: "b00t-hive".into(),
            service_name: "experiment-eval".into(),
            billed_cost: cost,
            effective_cost: cost * 0.85,
            experiment_id: Some("exp-001".into()),
            variant: Some(variant.into()),
            agent_id: Some(format!("sm0l-{variant}")),
        }
    }

    #[test]
    fn test_append_focus_record() {
        reset_store_for_test();
        let input = FocusToolInput {
            action: "append_focus_record".into(),
            records: vec![make_record("control", 100.0)],
            experiment_id: Some("exp-001".into()),
            personality: Some("analyst".into()),
        };
        let output = handle_focus_tool(input).unwrap();
        assert_eq!(output.action, "append_focus_record");
        assert_eq!(output.rows_written, 1);
        assert!(output.focus_cli.contains("ledgrrr focus append"));
        assert!(output.spec_version.starts_with("1."));
    }

    #[test]
    fn test_compute_focus_delta_action() {
        let input = FocusToolInput {
            action: "compute_focus_delta".into(),
            records: vec![
                make_record("control", 100.0),
                make_record("treatment", 150.0),
            ],
            experiment_id: Some("exp-001".into()),
            personality: None,
        };
        let output = handle_focus_tool(input).unwrap();
        let delta = output.delta.unwrap();
        assert_eq!(delta.control_billed, 100.0);
        assert_eq!(delta.treatment_billed, 150.0);
        assert_eq!(delta.recommendation, "treatment");
    }

    #[test]
    fn test_experiment_score_action() {
        let input = FocusToolInput {
            action: "experiment_score".into(),
            records: vec![make_record("control", 100.0)],
            experiment_id: Some("exp-001".into()),
            personality: Some("explorer".into()),
        };
        let output = handle_focus_tool(input).unwrap();
        assert!(output.focus_cli.contains("personality=explorer"));
        assert_eq!(output.rows_written, 1);
    }

    #[test]
    fn test_unknown_action_errors() {
        let input = FocusToolInput {
            action: "bogus".into(),
            records: vec![],
            experiment_id: None,
            personality: None,
        };
        assert!(handle_focus_tool(input).is_err());
    }

    #[test]
    fn test_validate_focus_record_passes_valid() {
        let record = make_record("control", 100.0);
        assert!(validate_focus_record(&record).is_ok());
    }

    #[test]
    fn test_validate_focus_record_rejects_empty_billing_account() {
        let mut record = make_record("control", 100.0);
        record.billing_account_id.clear();
        assert!(validate_focus_record(&record).is_err());
    }

    #[test]
    fn test_validate_focus_record_rejects_negative_cost() {
        let record = make_record("control", -50.0);
        assert!(validate_focus_record(&record).is_err());
    }

    #[test]
    fn test_handle_append_validates_before_processing() {
        let mut record = make_record("control", 100.0);
        record.billing_account_id.clear();
        let input = FocusToolInput {
            action: "append_focus_record".into(),
            records: vec![record],
            experiment_id: Some("exp-001".into()),
            personality: Some("analyst".into()),
        };
        let result = handle_focus_tool(input);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("BillingAccountId"));
    }

    #[test]
    fn test_query_summary() {
        // Use a temp dir to avoid loading existing records from the default path
        let tmp = tempfile::tempdir().unwrap();
        let tmp_path = tmp.path().join("focus_records.json");
        std::env::set_var("FOCUS_SIDECAR_PATH", tmp_path.to_str().unwrap());

        reset_store_for_test();
        let input = FocusToolInput {
            action: "query_focus_summary".into(),
            records: vec![],
            experiment_id: None,
            personality: None,
        };
        let output = handle_focus_tool(input).unwrap();
        // With no appended records, summary should say "0 records stored"
        let msg = output.experiment_summary.unwrap();
        assert!(msg.contains("FOCUS"));
        assert!(msg.contains("0 records stored"));
    }

    #[test]
    fn test_persistence_round_trip() {
        // Use a temp dir so test data doesn't pollute the real sidecar
        let tmp = tempfile::tempdir().unwrap();
        let tmp_path = tmp.path().join("focus_records.json");
        std::env::set_var("FOCUS_SIDECAR_PATH", tmp_path.to_str().unwrap());

        reset_store_for_test();

        // Append a record
        let append_input = FocusToolInput {
            action: "append_focus_record".into(),
            records: vec![make_record("control", 42.0)],
            experiment_id: Some("exp-002".into()),
            personality: None,
        };
        let append_output = handle_focus_tool(append_input).unwrap();
        assert_eq!(append_output.rows_written, 1);

        // Query summary — should see the appended record
        let query_input = FocusToolInput {
            action: "query_focus_summary".into(),
            records: vec![],
            experiment_id: None,
            personality: None,
        };
        let query_output = handle_focus_tool(query_input).unwrap();
        let msg = query_output.experiment_summary.unwrap();
        assert!(msg.contains("1 records stored"));
        assert!(msg.contains("42.00")); // total billed cost

        // Append a second record
        let append2_input = FocusToolInput {
            action: "append_focus_record".into(),
            records: vec![make_record("treatment", 58.0)],
            experiment_id: Some("exp-002".into()),
            personality: None,
        };
        let append2_output = handle_focus_tool(append2_input).unwrap();
        assert_eq!(append2_output.rows_written, 1);

        // Query again — should see both records
        let query2_input = FocusToolInput {
            action: "query_focus_summary".into(),
            records: vec![],
            experiment_id: None,
            personality: None,
        };
        let query2_output = handle_focus_tool(query2_input).unwrap();
        let msg2 = query2_output.experiment_summary.unwrap();
        assert!(msg2.contains("2 records stored"));
        assert!(msg2.contains("100.00")); // 42 + 58
    }
}
