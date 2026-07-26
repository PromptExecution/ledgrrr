//! Thin MCP handlers for US R&D Tax Credit / IRC § 41 (gh#516).

use rust_decimal::Decimal;
use serde_json::{json, Value};

use ledger_core::us_rdc::{QreActivity, UsRdcFourPartTest};
use ufo_types::{iso::Lei, satisfies::Satisfies};

pub fn handle_us_rdc_four_part_test(
    lei: &str,
    activity_id: &str,
    activity_name: &str,
    technical_in_nature: bool,
    permits_experimentation: bool,
    technological_uncertainty: bool,
    systematic_process: bool,
) -> Value {
    let lei = match Lei::new(lei) {
        Ok(l) => l,
        Err(e) => return json!({ "error": e.to_string() }),
    };
    let activity = QreActivity {
        lei,
        activity_id: activity_id.to_string(),
        activity_name: activity_name.to_string(),
        technical_in_nature,
        permits_experimentation,
        technological_uncertainty,
        systematic_process,
    };
    json!(activity.satisfies(&UsRdcFourPartTest))
}
