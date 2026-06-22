//! Thin MCP handlers for AU R&D Tax Incentive (gh#516).
//!
//! Each function is a deserialize-call-serialize wrapper.
//! Domain logic lives entirely in ledger-core::au_rd.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde_json::{json, Value};

use ledger_core::au_rd::{
    ActivityType, AuRdActivity, AuRdCompliance, AuRdEligibility, AuRdExpenditure,
    AuRdOffset, ExpenditureCategory,
};
use ufo_types::{iso::Lei, satisfies::Satisfies};

pub fn handle_au_rd_check_eligibility(
    lei: &str,
    activity_id: &str,
    activity_name: &str,
    has_hypothesis: bool,
    has_technical_uncertainty: bool,
    is_systematic: bool,
    is_core: bool,
) -> Value {
    let lei = match Lei::new(lei) {
        Ok(l) => l,
        Err(e) => return json!({ "error": e.to_string() }),
    };
    let activity = AuRdActivity {
        lei,
        activity_id: activity_id.to_string(),
        activity_name: activity_name.to_string(),
        activity_type: if is_core { ActivityType::Core } else { ActivityType::Supporting },
        anzsic_code: String::new(),
        period_start: NaiveDate::from_ymd_opt(2024, 7, 1).unwrap(),
        period_end: NaiveDate::from_ymd_opt(2025, 6, 30).unwrap(),
        has_hypothesis,
        has_technical_uncertainty,
        is_systematic,
    };
    json!(activity.satisfies(&AuRdEligibility))
}

pub fn handle_au_rd_classify_expenditure(
    lei: &str,
    tx_id: &str,
    category: &str,
    amount_aud: &str,
) -> Value {
    let lei = match Lei::new(lei) {
        Ok(l) => l,
        Err(e) => return json!({ "error": e.to_string() }),
    };
    let cat = match category {
        "contractor" => ExpenditureCategory::Contractor,
        "salary" => ExpenditureCategory::Salary,
        "feedstock" => ExpenditureCategory::Feedstock,
        "decline_in_value" => ExpenditureCategory::DeclineInValue,
        _ => ExpenditureCategory::Other,
    };
    let amount = match Decimal::from_str_exact(amount_aud) {
        Ok(a) => a,
        Err(e) => return json!({ "error": format!("invalid amount: {e}") }),
    };
    let exp = AuRdExpenditure {
        lei,
        tx_id: tx_id.to_string(),
        category: cat,
        amount,
        currency: ufo_types::iso::Currency::Aud,
        date: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
        activity_id: String::new(),
    };
    json!({
        "result": exp.satisfies(&AuRdEligibility),
        "section_ref": cat.section_ref(),
        "directly_eligible": cat.is_directly_eligible(),
    })
}

pub fn handle_au_rd_calculate_offset(
    _lei: &str,
    total_eligible_aud: &str,
    is_refundable: bool,
) -> Value {
    let total = match Decimal::from_str_exact(total_eligible_aud) {
        Ok(a) => a,
        Err(e) => return json!({ "error": format!("invalid amount: {e}") }),
    };
    let offset = AuRdOffset::new(total, is_refundable);
    let compliance = offset.satisfies(&AuRdCompliance);
    json!({
        "offset": offset,
        "compliance": compliance,
    })
}
