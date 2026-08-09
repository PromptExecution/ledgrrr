use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::str::FromStr;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyBalance {
    pub date: String,
    pub balance: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignAccountInput {
    pub account_id: String,
    pub institution: String,
    pub country: String,
    pub currency: String,
    pub daily_balances: Vec<DailyBalance>,
    pub year_end_rate: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FbarInput {
    pub tax_year: u16,
    pub filing_status: String,
    pub living_abroad: bool,
    pub accounts: Vec<ForeignAccountInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignAccountYear {
    pub account_id: String,
    pub institution: String,
    pub country: String,
    pub currency: String,
    pub max_balance_native: String,
    pub max_balance_date: String,
    pub treasury_year_end_rate: String,
    pub max_balance_usd: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FbarDetermination {
    pub tax_year: u16,
    pub accounts: Vec<ForeignAccountYear>,
    pub aggregate_max_usd: String,
    pub filing_required: bool,
    pub incomplete_accounts: Vec<String>,
    pub form_8938_filing_required: Option<bool>,
    pub form_8938_threshold_used: Option<String>,
}

fn form_8938_threshold(filing_status: &str, living_abroad: bool) -> &'static str {
    match (living_abroad, filing_status) {
        (true, "mfj") => "$400,000/$600,000",
        (true, _) => "$200,000/$400,000",
        (false, "mfj") | (false, "qss") => "$100,000/$150,000",
        (false, _) => "$50,000/$75,000",
    }
}

fn form_8938_filing_required(
    aggregate_usd: &Decimal,
    filing_status: &str,
    living_abroad: bool,
) -> bool {
    let threshold = match (living_abroad, filing_status) {
        (true, "mfj") => (
            Decimal::from_str("400000").unwrap(),
            Decimal::from_str("600000").unwrap(),
        ),
        (true, _) => (
            Decimal::from_str("200000").unwrap(),
            Decimal::from_str("400000").unwrap(),
        ),
        (false, "mfj") | (false, "qss") => (
            Decimal::from_str("100000").unwrap(),
            Decimal::from_str("150000").unwrap(),
        ),
        (false, _) => (
            Decimal::from_str("50000").unwrap(),
            Decimal::from_str("75000").unwrap(),
        ),
    };
    *aggregate_usd >= threshold.0 || *aggregate_usd >= threshold.1
}

fn parse_decimal(s: &str) -> Option<Decimal> {
    Decimal::from_str(s).ok()
}

pub fn compute_fbar(input: &FbarInput) -> FbarDetermination {
    let mut accounts = Vec::new();
    let mut incomplete_accounts = Vec::new();
    let mut aggregate_usd = Decimal::ZERO;

    for acct in &input.accounts {
        if acct.daily_balances.is_empty() || acct.year_end_rate.is_none() {
            incomplete_accounts.push(acct.account_id.clone());
            continue;
        }

        let max_balance = acct
            .daily_balances
            .iter()
            .filter_map(|db| {
                let bal = parse_decimal(&db.balance)?;
                Some((db.date.clone(), bal))
            })
            .max_by(|(_, a), (_, b)| a.cmp(b));

        let year_end_rate = acct.year_end_rate.as_deref().unwrap_or("1.0");
        let rate = parse_decimal(year_end_rate).unwrap_or(Decimal::ONE);

        let (max_date, max_native) = match max_balance {
            Some((d, b)) => (d, b),
            None => {
                incomplete_accounts.push(acct.account_id.clone());
                continue;
            }
        };

        let max_usd = max_native * rate;

        accounts.push(ForeignAccountYear {
            account_id: acct.account_id.clone(),
            institution: acct.institution.clone(),
            country: acct.country.clone(),
            currency: acct.currency.clone(),
            max_balance_native: max_native.to_string(),
            max_balance_date: max_date,
            treasury_year_end_rate: year_end_rate.to_string(),
            max_balance_usd: max_usd.to_string(),
        });

        aggregate_usd += max_usd;
    }

    let fbar_threshold = Decimal::from_str("10000").unwrap();
    let filing_required = aggregate_usd > fbar_threshold;

    let (form_8938_filing_required, form_8938_threshold_used) = if accounts.is_empty() {
        // No complete accounts to sum — either nothing was submitted, or every
        // submitted account is missing data. Either way there's no aggregate
        // balance to test against a threshold, so no determination is possible.
        (None, None)
    } else {
        let required =
            form_8938_filing_required(&aggregate_usd, &input.filing_status, input.living_abroad);
        let threshold = form_8938_threshold(&input.filing_status, input.living_abroad).to_string();
        (Some(required), Some(threshold))
    };

    FbarDetermination {
        tax_year: input.tax_year,
        accounts,
        aggregate_max_usd: aggregate_usd.to_string(),
        filing_required,
        incomplete_accounts,
        form_8938_filing_required,
        form_8938_threshold_used,
    }
}
