use serde_json::{json, Value};

const LIFE_MONTHS: f64 = 27.5 * 12.0;

pub struct DepreciationInput {
    pub tax_year: u16,
    pub placed_in_service: String,
    pub total_basis: String,
    pub land_value: String,
    pub improvements: Vec<(String, String)>,
    pub prior_accumulated: String,
}

pub struct DepreciationSchedule {
    pub tax_year: u16,
    pub depreciable_basis: String,
    pub current_year: String,
    pub accumulated_prior: String,
    pub accumulated_end: String,
    pub remaining_life_months: u16,
}

fn parse_ymd(s: &str) -> (u16, u8) {
    let parts: Vec<&str> = s.split('-').collect();
    let year = parts[0].parse().expect("valid year");
    let month = parts[1].parse().expect("valid month");
    (year, month)
}

fn months_in_tax_year(placed_year: u16, placed_month: u8, tax_year: u16) -> f64 {
    if tax_year < placed_year {
        return 0.0;
    }
    if tax_year == placed_year {
        12.5 - placed_month as f64
    } else {
        12.0
    }
}

fn total_months_elapsed(placed_year: u16, placed_month: u8, tax_year: u16) -> f64 {
    if tax_year < placed_year {
        return 0.0;
    }
    let first_year = 12.5 - placed_month as f64;
    if tax_year == placed_year {
        first_year
    } else {
        let full_years = (tax_year - placed_year - 1) as f64;
        first_year + full_years * 12.0 + 12.0
    }
}

pub fn compute_depreciation(input: &DepreciationInput) -> DepreciationSchedule {
    let tax_year = input.tax_year;
    let (place_year, place_month) = parse_ymd(&input.placed_in_service);
    let total_basis: f64 = input.total_basis.parse().expect("valid total_basis");
    let land_value: f64 = input.land_value.parse().expect("valid land_value");
    let prior_accumulated: f64 = input.prior_accumulated.parse().expect("valid prior_accumulated");

    let base_depreciable = total_basis - land_value;
    let base_months = months_in_tax_year(place_year, place_month, tax_year);

    let mut total_depreciable = base_depreciable;
    let mut current_year_total = base_depreciable / LIFE_MONTHS * base_months;

    for (imp_date, imp_amount_str) in &input.improvements {
        let amount: f64 = imp_amount_str.parse().expect("valid improvement amount");
        total_depreciable += amount;
        let (imp_year, imp_month) = parse_ymd(imp_date);
        if tax_year >= imp_year {
            let imp_months = months_in_tax_year(imp_year, imp_month, tax_year);
            current_year_total += amount / LIFE_MONTHS * imp_months;
        }
    }

    let accumulated_end = prior_accumulated + current_year_total;
    let total_elapsed = total_months_elapsed(place_year, place_month, tax_year);
    let remaining = (LIFE_MONTHS - total_elapsed).floor() as u16;

    DepreciationSchedule {
        tax_year,
        depreciable_basis: format!("{:.2}", total_depreciable),
        current_year: format!("{:.2}", current_year_total),
        accumulated_prior: format!("{:.2}", prior_accumulated),
        accumulated_end: format!("{:.2}", accumulated_end),
        remaining_life_months: remaining,
    }
}

pub fn handle_compute_depreciation(
    tax_year: u16,
    placed_in_service: String,
    total_basis: String,
    land_value: String,
    improvements: Option<Vec<Vec<String>>>,
    prior_accumulated: Option<String>,
) -> Value {
    let improvements = improvements
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| {
            if v.len() == 2 {
                Some((v[0].clone(), v[1].clone()))
            } else {
                None
            }
        })
        .collect();
    let input = DepreciationInput {
        tax_year,
        placed_in_service,
        total_basis,
        land_value,
        improvements,
        prior_accumulated: prior_accumulated.unwrap_or_else(|| "0".to_string()),
    };
    let schedule = compute_depreciation(&input);
    json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string(&json!({
                "tax_year": schedule.tax_year,
                "depreciable_basis": schedule.depreciable_basis,
                "current_year": schedule.current_year,
                "accumulated_prior": schedule.accumulated_prior,
                "accumulated_end": schedule.accumulated_end,
                "remaining_life_months": schedule.remaining_life_months,
            })).unwrap_or_default()
        }],
        "isError": false
    })
}
