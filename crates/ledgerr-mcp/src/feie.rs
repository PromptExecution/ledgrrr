use rust_decimal::Decimal;
use serde::Serialize;
use serde_json::{json, Value};
use std::str::FromStr;

use crate::ToolError;

pub enum ForeignResidenceTest {
    BonaFideResidence { start: String, end: Option<String> },
    PhysicalPresence { qualifying_days: u16, window: (String, String) },
}

pub struct FeieInput {
    pub tax_year: u16,
    pub test: ForeignResidenceTest,
    pub foreign_earned_income: String,
    pub days_qualified: u16,
    pub housing_exclusion: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FeieOutcome {
    pub excluded_amount: String,
    pub income_subject_to_income_tax: String,
    pub income_subject_to_se_tax: String,
    pub warnings: Vec<String>,
}

fn exclusion_limit_for_year(year: u16) -> Decimal {
    match year {
        2023 => Decimal::new(120000, 0),
        2024 => Decimal::new(126500, 0),
        2025 => Decimal::new(130000, 0),
        _ => Decimal::new(120000, 0),
    }
}

pub fn compute_feie(input: &FeieInput) -> FeieOutcome {
    let limit = exclusion_limit_for_year(input.tax_year);
    let days_in_year = Decimal::new(365, 0);
    let days_qualified = Decimal::from(input.days_qualified);
    let pro_rated_limit = limit * days_qualified / days_in_year;

    let foreign_income =
        Decimal::from_str(&input.foreign_earned_income).unwrap_or(Decimal::ZERO);

    let excluded_amount = foreign_income.min(pro_rated_limit);
    let income_subject_to_income_tax = foreign_income - excluded_amount;
    let income_subject_to_se_tax = foreign_income;

    let mut warnings = Vec::new();
    if income_subject_to_se_tax > Decimal::ZERO {
        warnings.push(
            "FEIE does not reduce self-employment tax. SE tax applies to foreign earned income even when FEIE excludes it from income tax.".to_string()
        );
    }

    FeieOutcome {
        excluded_amount: excluded_amount.to_string(),
        income_subject_to_income_tax: income_subject_to_income_tax.to_string(),
        income_subject_to_se_tax: income_subject_to_se_tax.to_string(),
        warnings,
    }
}

pub fn compute_feie_from_json(args: &serde_json::Value) -> Result<FeieOutcome, ToolError> {
    let tax_year = args
        .get("tax_year")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| ToolError::InvalidInput("missing or invalid `tax_year`".to_string()))?
        as u16;

    let test_str = args
        .get("test")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::InvalidInput("missing or invalid `test`".to_string()))?;

    let test = match test_str {
        "bona_fide" => ForeignResidenceTest::BonaFideResidence {
            start: args
                .get("test_start")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    ToolError::InvalidInput(
                        "missing `test_start` for bona_fide test".to_string(),
                    )
                })?
                .to_string(),
            end: args
                .get("test_end")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        },
        "physical_presence" => ForeignResidenceTest::PhysicalPresence {
            qualifying_days: args
                .get("qualifying_days")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| {
                    ToolError::InvalidInput(
                        "missing `qualifying_days` for physical_presence test".to_string(),
                    )
                })? as u16,
            window: {
                let start = args
                    .get("window_start")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ToolError::InvalidInput(
                            "missing `window_start` for physical_presence test".to_string(),
                        )
                    })?
                    .to_string();
                let end = args
                    .get("window_end")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ToolError::InvalidInput(
                            "missing `window_end` for physical_presence test".to_string(),
                        )
                    })?
                    .to_string();
                (start, end)
            },
        },
        _ => {
            return Err(ToolError::InvalidInput(format!(
                "unknown test variant: {test_str}"
            )))
        }
    };

    let foreign_earned_income = args
        .get("foreign_earned_income")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            ToolError::InvalidInput("missing `foreign_earned_income`".to_string())
        })?
        .to_string();

    let days_qualified = args
        .get("days_qualified")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| ToolError::InvalidInput("missing `days_qualified`".to_string()))?
        as u16;

    let housing_exclusion = args
        .get("housing_exclusion")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok(compute_feie(&FeieInput {
        tax_year,
        test,
        foreign_earned_income,
        days_qualified,
        housing_exclusion,
    }))
}

pub fn handle_compute_feie(
    tax_year: u16,
    foreign_earned_income: &str,
    days_qualified: u16,
    housing_exclusion: Option<&str>,
    test: &str,
    test_start: &str,
    test_end: Option<&str>,
    qualifying_days: Option<u16>,
    window_start: Option<&str>,
    window_end: Option<&str>,
) -> Value {
    let feie_test = match test {
        "bona_fide" => ForeignResidenceTest::BonaFideResidence {
            start: test_start.to_string(),
            end: test_end.map(|s| s.to_string()),
        },
        "physical_presence" => {
            let qd = match qualifying_days {
                Some(d) => d,
                None => {
                    return json!({ "error": "qualifying_days required for physical_presence test" })
                }
            };
            let ws = match window_start {
                Some(s) => s.to_string(),
                None => {
                    return json!({ "error": "window_start required for physical_presence test" })
                }
            };
            let we = match window_end {
                Some(s) => s.to_string(),
                None => {
                    return json!({ "error": "window_end required for physical_presence test" })
                }
            };
            ForeignResidenceTest::PhysicalPresence {
                qualifying_days: qd,
                window: (ws, we),
            }
        }
        _ => return json!({ "error": format!("unknown test: {test}") }),
    };

    let input = FeieInput {
        tax_year,
        test: feie_test,
        foreign_earned_income: foreign_earned_income.to_string(),
        days_qualified,
        housing_exclusion: housing_exclusion.map(|s| s.to_string()),
    };

    let outcome = compute_feie(&input);
    json!(outcome)
}
