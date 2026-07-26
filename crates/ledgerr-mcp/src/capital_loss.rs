use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilingStatus {
    Single,
    MarriedFilingJointly,
    MarriedFilingSeparately,
    HeadOfHousehold,
    QualifyingSurvivingSpouse,
}

impl FilingStatus {
    pub fn capital_loss_limit(&self) -> Decimal {
        match self {
            FilingStatus::MarriedFilingSeparately => Decimal::from_str_exact("1500").unwrap(),
            _ => Decimal::from_str_exact("3000").unwrap(),
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "single" => Some(Self::Single),
            "married_filing_jointly" => Some(Self::MarriedFilingJointly),
            "married_filing_separately" => Some(Self::MarriedFilingSeparately),
            "head_of_household" => Some(Self::HeadOfHousehold),
            "qualifying_surviving_spouse" => Some(Self::QualifyingSurvivingSpouse),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::MarriedFilingJointly => "married_filing_jointly",
            Self::MarriedFilingSeparately => "married_filing_separately",
            Self::HeadOfHousehold => "head_of_household",
            Self::QualifyingSurvivingSpouse => "qualifying_surviving_spouse",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CapitalLossCarryforward {
    pub tax_year: u16,
    pub short_term: String,
    pub long_term: String,
    pub applied_this_year: String,
    pub carried_forward: String,
}

pub struct CapitalLossInput {
    pub tax_year: u16,
    pub filing_status: FilingStatus,
    pub short_term_losses: String,
    pub long_term_losses: String,
    pub short_term_gains: String,
    pub long_term_gains: String,
    pub prior_short_term_carryforward: String,
    pub prior_long_term_carryforward: String,
    pub nonbusiness_bad_debt: Option<String>,
}

#[derive(Serialize)]
pub struct CapitalLossOutcome {
    pub net_short_term: String,
    pub net_long_term: String,
    pub total_net_loss: String,
    pub deductible_amount: String,
    pub carryforward_short_term: String,
    pub carryforward_long_term: String,
    pub filing_status_used: String,
    pub warnings: Vec<String>,
}

fn d(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap_or(Decimal::ZERO)
}

fn fmt(d: Decimal) -> String {
    format!("{:.2}", d.round_dp(2))
}

pub fn compute_capital_loss(input: &CapitalLossInput) -> CapitalLossOutcome {
    let mut st_losses = d(&input.short_term_losses);
    let lt_losses = d(&input.long_term_losses);
    let st_gains = d(&input.short_term_gains);
    let lt_gains = d(&input.long_term_gains);
    let prior_st_cf = d(&input.prior_short_term_carryforward);
    let prior_lt_cf = d(&input.prior_long_term_carryforward);
    let limit = input.filing_status.capital_loss_limit();

    if let Some(bad_debt) = &input.nonbusiness_bad_debt {
        st_losses += d(bad_debt);
    }

    let net_short = st_gains - st_losses - prior_st_cf;
    let net_long = lt_gains - lt_losses - prior_lt_cf;
    let total = net_short + net_long;
    let is_loss = total.is_sign_negative();
    let abs_loss = total.abs();

    let deductible = if is_loss {
        if abs_loss <= limit { abs_loss } else { limit }
    } else {
        Decimal::ZERO
    };

    let remaining_loss = if is_loss { abs_loss - deductible } else { Decimal::ZERO };

    let (cf_st, cf_lt) = if is_loss && !total.is_zero() {
        let st_abs = net_short.abs();
        let weight_st = st_abs / abs_loss;
        let weight_lt = Decimal::ONE - weight_st;
        ((remaining_loss * weight_st).round_dp(2), (remaining_loss * weight_lt).round_dp(2))
    } else {
        (Decimal::ZERO, Decimal::ZERO)
    };

    let mut warnings = Vec::new();
    if remaining_loss > limit * Decimal::from_str_exact("20").unwrap() {
        warnings.push(format!(
            "Carryforward of ${:.2} exceeds approximately 20 years of absorption at ${:.2}/year limit",
            remaining_loss, limit
        ));
    }

    CapitalLossOutcome {
        net_short_term: fmt(net_short),
        net_long_term: fmt(net_long),
        total_net_loss: fmt(total),
        deductible_amount: fmt(deductible),
        carryforward_short_term: fmt(cf_st),
        carryforward_long_term: fmt(cf_lt),
        filing_status_used: input.filing_status.as_str().to_string(),
        warnings,
    }
}

pub fn handle_compute_capital_loss(
    tax_year: u16,
    filing_status: &str,
    short_term_losses: &str,
    long_term_losses: &str,
    short_term_gains: &str,
    long_term_gains: &str,
    prior_short_term_carryforward: Option<&str>,
    prior_long_term_carryforward: Option<&str>,
) -> Value {
    let status = match FilingStatus::from_str(filing_status) {
        Some(s) => s,
        None => return json!({"error": format!("unknown filing_status: {filing_status}")}),
    };

    let input = CapitalLossInput {
        tax_year,
        filing_status: status,
        short_term_losses: short_term_losses.to_string(),
        long_term_losses: long_term_losses.to_string(),
        short_term_gains: short_term_gains.to_string(),
        long_term_gains: long_term_gains.to_string(),
        prior_short_term_carryforward: prior_short_term_carryforward.unwrap_or("0").to_string(),
        prior_long_term_carryforward: prior_long_term_carryforward.unwrap_or("0").to_string(),
        nonbusiness_bad_debt: None,
    };

    let outcome = compute_capital_loss(&input);
    json!(outcome)
}
