//! # t00n — b00t's implementation of TOON (Token-Oriented Object Notation)
//!
//! Polyseme: toon|t00n — both refer to the same format.
//! Canonical spec: https://github.com/toon-format/spec (v3.0)
//!
//! Serializes FOCUS CostAndUsage records to TOON format for
//! token-efficient LLM consumption and contract validation.
//!
//! ## Example output
//! ```toon
//! # reqif.yaml: focus-contract/v1
//! focus_records[3]{BillingAccountId,ServiceName,BilledCost,EffectiveCost,ChargeCategory,x_Variant}:
//!   b00t-hive,experiment-eval,100.00,85.00,Usage,control
//!   b00t-hive,experiment-eval,150.00,127.50,Usage,treatment
//!   b00t-hive,experiment-eval,200.00,170.00,Usage,control
//! ```

use crate::{CostAndUsageRow, FocusDelta};
use std::collections::HashMap;
use std::sync::LazyLock;

/// Serialize FOCUS CostAndUsage rows to TOON tabular format.
/// Output includes the `# reqif.yaml` header attribution.
pub fn focus_rows_to_t00n(
    rows: &[CostAndUsageRow],
    reqif_schema: &str,
    dataset_label: &str,
) -> String {
    let mut out = String::new();

    // reqif.yaml header attribution
    out.push_str(&format!("# reqif.yaml: {}\n", reqif_schema));

    if rows.is_empty() {
        out.push_str(&format!("{}[0]:\n", dataset_label));
        return out;
    }

    // Collect all non-None x_ fields as TOON field names + standard FOCUS fields
    let focus_fields = &[
        "BillingAccountId",
        "ServiceProviderName",
        "ServiceName",
        "BilledCost",
        "EffectiveCost",
        "ChargeCategory",
        "ChargeFrequency",
    ];
    let custom_fields = &[
        (
            "x_ExperimentId",
            rows.iter().any(|r| r.x_experiment_id.is_some()),
        ),
        ("x_Variant", rows.iter().any(|r| r.x_variant.is_some())),
        (
            "x_Personality",
            rows.iter().any(|r| r.x_personality.is_some()),
        ),
        ("x_AgentId", rows.iter().any(|r| r.x_agent_id.is_some())),
    ];

    let active_fields: Vec<&str> = focus_fields
        .iter()
        .copied()
        .chain(
            custom_fields
                .iter()
                .filter(|(_, present)| *present)
                .map(|(name, _)| *name),
        )
        .collect();

    // TOON tabular header: key[N]{f1,f2,...}:
    out.push_str(&format!(
        "{}[{}]{{{}}}:\n",
        dataset_label,
        rows.len(),
        active_fields.join(","),
    ));

    // Rows
    for row in rows {
        let vals: Vec<String> = active_fields
            .iter()
            .map(|field| match *field {
                "BillingAccountId" => t00n_escape(&row.billing_account_id),
                "ServiceProviderName" => t00n_escape(&row.service_provider_name),
                "ServiceName" => t00n_escape(&row.service_name),
                "BilledCost" => format!("{:.2}", row.billed_cost),
                "EffectiveCost" => format!("{:.2}", row.effective_cost),
                "ChargeCategory" => format!("{:?}", row.charge_category),
                "ChargeFrequency" => format!("{:?}", row.charge_frequency),
                "x_ExperimentId" => t00n_opt_str(row.x_experiment_id.as_deref()),
                "x_Variant" => t00n_opt_str(row.x_variant.as_deref()),
                "x_Personality" => t00n_opt_str(row.x_personality.as_deref()),
                "x_AgentId" => t00n_opt_str(row.x_agent_id.as_deref()),
                _ => String::new(),
            })
            .collect();
        out.push_str(&format!("  {}\n", vals.join(",")));
    }

    out
}

/// Serialize a FocusDelta to TOON format.
pub fn focus_delta_to_t00n(delta: &FocusDelta) -> String {
    let mut out = String::new();
    out.push_str("# reqif.yaml: focus-delta/v1\n");
    out.push_str(
        "focus_delta{experiment_id,control_billed,treatment_billed,delta_billed,recommendation}:\n",
    );
    out.push_str(&format!(
        "  {},{:.2},{:.2},{:.2},{}\n",
        delta.experiment_id,
        delta.control_billed_cost,
        delta.treatment_billed_cost,
        delta.delta_billed,
        delta.recommendation,
    ));

    if !delta.dimension_deltas.is_empty() {
        out.push_str(&format!(
            "dimension_deltas[{}]{{dimension,delta}}:\n",
            delta.dimension_deltas.len(),
        ));
        for (dim, val) in &delta.dimension_deltas {
            out.push_str(&format!("  {},{:.4}\n", dim, val));
        }
    }
    out
}

/// Serialize experiment comparison to TOON for sm0l validation.
pub fn experiment_comparison_to_t00n(
    experiment_id: &str,
    control_scores: &HashMap<String, f64>,
    treatment_scores: &HashMap<String, f64>,
    recommendation: &str,
) -> String {
    let mut out = String::new();
    out.push_str("# reqif.yaml: experiment-score/v1\n");
    out.push_str(&format!("experiment_id: {}\n", t00n_escape(experiment_id)));
    out.push_str("variants[2]{variant,roi,cost,time,accuracy,utility,risk}:\n");

    let c_roi = control_scores.get("roi").copied().unwrap_or(0.0);
    let c_cost = control_scores.get("cost").copied().unwrap_or(0.0);
    let c_time = control_scores.get("time").copied().unwrap_or(0.0);
    let c_acc = control_scores.get("accuracy").copied().unwrap_or(0.0);
    let c_util = control_scores.get("utility").copied().unwrap_or(0.0);
    let c_risk = control_scores.get("risk").copied().unwrap_or(0.0);
    out.push_str(&format!(
        "  control,{:.2},{:.0},{:.0},{:.2},{:.2},{:.2}\n",
        c_roi, c_cost, c_time, c_acc, c_util, c_risk
    ));

    let t_roi = treatment_scores.get("roi").copied().unwrap_or(0.0);
    let t_cost = treatment_scores.get("cost").copied().unwrap_or(0.0);
    let t_time = treatment_scores.get("time").copied().unwrap_or(0.0);
    let t_acc = treatment_scores.get("accuracy").copied().unwrap_or(0.0);
    let t_util = treatment_scores.get("utility").copied().unwrap_or(0.0);
    let t_risk = treatment_scores.get("risk").copied().unwrap_or(0.0);
    out.push_str(&format!(
        "  treatment,{:.2},{:.0},{:.0},{:.2},{:.2},{:.2}\n",
        t_roi, t_cost, t_time, t_acc, t_util, t_risk
    ));

    out.push_str(&format!(
        "recommendation: {}\n",
        t00n_escape(recommendation)
    ));
    out
}

/// Validate TOON tabular row count against declared [N].
/// Returns Ok(count) if valid, Err with diagnostic if mismatch.
pub fn validate_t00n_row_count(t00n: &str) -> Result<usize, String> {
    static ROW_COUNT_RE: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"\[(\d+)\]").expect("TOON row-count regex is valid"));

    let lines: Vec<&str> = t00n
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim().starts_with('#'))
        .collect();
    if lines.is_empty() {
        return Err("empty t00n document".into());
    }

    // Find header: matches key[N]{...}:
    let header_line = lines[0];
    let declared: usize = ROW_COUNT_RE
        .captures(header_line)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse().ok())
        .ok_or_else(|| format!("no [N] header found in: {header_line}"))?;

    // count data rows (indented with 2 spaces)
    let rows: Vec<&str> = lines[1..]
        .iter()
        .filter(|l| l.starts_with("  ") && !l.trim().is_empty())
        .copied()
        .collect();

    let actual = rows.len();
    if actual != declared {
        return Err(format!(
            "TOON validation: declared [{}] but got {} rows",
            declared, actual
        ));
    }
    Ok(actual)
}

fn t00n_escape(s: &str) -> String {
    if s.is_empty()
        || s.contains(',')
        || s.contains(':')
        || s.contains('"')
        || s.contains('[')
        || s.contains(']')
        || s.contains('{')
        || s.contains('}')
        || s.contains('\n')
        || s.contains('\r')
        || s.contains('\t')
    {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

fn t00n_opt_str(s: Option<&str>) -> String {
    match s {
        Some(v) => t00n_escape(v),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChargeCategory, ChargeFrequency};
    use chrono::DateTime;
    use rust_decimal::prelude::FromPrimitive;
    use rust_decimal::Decimal;

    fn sample_row(exp: &str, variant: &str, cost: f64) -> CostAndUsageRow {
        CostAndUsageRow {
            billing_account_id: "b00t-hive".into(),
            billing_account_name: None,
            billing_currency: "FOCUS".into(),
            billing_period_start: DateTime::from_timestamp_nanos(0),
            billing_period_end: DateTime::from_timestamp_nanos(1_000_000_000),
            charge_period_start: DateTime::from_timestamp_nanos(0),
            charge_period_end: DateTime::from_timestamp_nanos(1_000_000_000),
            charge_category: ChargeCategory::Usage,
            charge_frequency: ChargeFrequency::UsageBased,
            billed_cost: Decimal::from_f64(cost).unwrap_or_default(),
            effective_cost: Decimal::from_f64(cost * 0.85).unwrap_or_default(),
            service_provider_name: "ledgrrr".into(),
            service_name: "experiment-eval".into(),
            sku_id: "exp-eval-sm0l".into(),
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
            service_category: None,
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
            x_experiment_id: Some(exp.into()),
            x_variant: Some(variant.into()),
            x_personality: None,
            x_experiment_score: None,
            x_agent_id: None,
            x_reasoning_review: None,
        }
    }

    #[test]
    fn test_focus_rows_to_t00n_basic() {
        let rows = vec![
            sample_row("exp-001", "control", 100.0),
            sample_row("exp-001", "treatment", 150.0),
        ];
        let t00n = focus_rows_to_t00n(&rows, "focus-contract/v1", "focus_records");
        assert!(t00n.starts_with("# reqif.yaml: focus-contract/v1"));
        assert!(t00n.contains("focus_records[2]"));
        assert!(t00n.contains("BillingAccountId"));
        assert!(t00n.contains("b00t-hive"));
        assert!(t00n.contains("100.00"));
        assert!(t00n.contains("150.00"));
        assert!(t00n.contains("control"));
        assert!(t00n.contains("treatment"));
    }

    #[test]
    fn test_focus_rows_to_t00n_empty() {
        let rows = vec![];
        let t00n = focus_rows_to_t00n(&rows, "focus-contract/v1", "focus_records");
        assert!(t00n.contains("focus_records[0]:"));
    }

    #[test]
    fn test_focus_delta_to_t00n() {
        let mut dim_deltas = HashMap::new();
        dim_deltas.insert("roi".into(), Default::default());

        let delta = FocusDelta {
            experiment_id: "exp-001".into(),
            control_billed_cost: Default::default(),
            treatment_billed_cost: Default::default(),
            control_effective_cost: Default::default(),
            treatment_effective_cost: Default::default(),
            delta_billed: Default::default(),
            delta_effective: Default::default(),
            dimension_deltas: dim_deltas,
            recommendation: "treatment".into(),
        };
        let t00n = focus_delta_to_t00n(&delta);
        assert!(t00n.contains("focus_delta"));
        assert!(t00n.contains("dimension_deltas"));
    }

    #[test]
    fn test_experiment_comparison_to_t00n() {
        let mut cs = HashMap::new();
        cs.insert("roi".into(), 0.82);
        cs.insert("cost".into(), 1423.0);
        cs.insert("time".into(), 4521.0);
        cs.insert("accuracy".into(), 0.91);
        cs.insert("utility".into(), 0.78);
        cs.insert("risk".into(), 0.12);

        let mut ts = HashMap::new();
        ts.insert("roi".into(), 0.91);
        ts.insert("cost".into(), 1892.0);
        ts.insert("time".into(), 5102.0);
        ts.insert("accuracy".into(), 0.95);
        ts.insert("utility".into(), 0.88);
        ts.insert("risk".into(), 0.09);

        let t00n = experiment_comparison_to_t00n("exp-001", &cs, &ts, "treatment");
        assert!(t00n.contains("experiment_id: exp-001"));
        assert!(t00n.contains("control,0.82"));
        assert!(t00n.contains("treatment,0.91"));
        assert!(t00n.contains("recommendation: treatment"));
    }

    #[test]
    fn test_validate_t00n_row_count_matches() {
        let rows = vec![sample_row("exp-001", "control", 100.0)];
        let t00n = focus_rows_to_t00n(&rows, "test/v1", "data");
        assert!(validate_t00n_row_count(&t00n).is_ok());
    }

    #[test]
    fn test_validate_t00n_row_count_mismatch() {
        let t00n = "data[5]{a,b}:\n  1,2\n  3,4\n";
        assert!(validate_t00n_row_count(t00n).is_err());
    }

    #[test]
    fn test_t00n_escape_quotes_when_needed() {
        assert_eq!(t00n_escape("hello"), "hello");
        assert_eq!(t00n_escape(""), "\"\"");
        assert!(t00n_escape("a,b").contains('"'));
        assert!(t00n_escape("a:b").contains('"'));
    }
}
