use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

use ledger_core::ingest::TransactionInput;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::ToolError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountPeriodCoverage {
    pub account_id: String,
    pub period: String,
    pub opening_balance: String,
    pub closing_balance: String,
    pub posting_sum: String,
    pub source_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Discontinuity {
    pub account_id: String,
    pub from_period: String,
    pub to_period: String,
    pub expected_opening: String,
    pub actual_opening: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverageReport {
    pub complete: Vec<AccountPeriodCoverage>,
    pub gaps: Vec<(String, String)>,
    pub duplicates: Vec<AccountPeriodCoverage>,
    pub discontinuities: Vec<Discontinuity>,
    pub intra_period_failures: Vec<AccountPeriodCoverage>,
    pub has_gaps: bool,
    pub has_duplicates: bool,
    pub has_discontinuities: bool,
    pub has_intra_period_failures: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverageRequest {
    pub account_ids: Vec<String>,
    pub tax_year: i32,
}

fn year_month_from_date(date: &str) -> Result<(i32, u32), ToolError> {
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() < 2 {
        return Err(ToolError::InvalidInput(format!(
            "cannot parse year-month from date: {date}"
        )));
    }
    let year = parts[0]
        .parse::<i32>()
        .map_err(|_| ToolError::InvalidInput(format!("invalid year in date: {date}")))?;
    let month = parts[1]
        .parse::<u32>()
        .map_err(|_| ToolError::InvalidInput(format!("invalid month in date: {date}")))?;
    Ok((year, month))
}

pub fn assert_account_coverage(
    request: &CoverageRequest,
    tx_rows: &BTreeMap<String, TransactionInput>,
) -> Result<CoverageReport, ToolError> {
    let account_set: BTreeSet<&str> = request.account_ids.iter().map(String::as_str).collect();

    let mut by_period: BTreeMap<(String, i32, u32), AccountMonthAccum> = BTreeMap::new();

    for (_tx_id, tx) in tx_rows {
        if !account_set.contains(tx.account_id.as_str()) {
            continue;
        }
        let (year, month) = year_month_from_date(&tx.date)?;
        if year != request.tax_year {
            continue;
        }
        let amount = Decimal::from_str(&tx.amount)
            .map_err(|_| ToolError::InvalidInput(format!("bad amount: {}", tx.amount)))?;
        let entry = by_period
            .entry((tx.account_id.clone(), year, month))
            .or_insert_with(|| AccountMonthAccum {
                account_id: tx.account_id.clone(),
                year,
                month,
                posting_sum: Decimal::ZERO,
                source_refs: Vec::new(),
                opening_balance: Decimal::ZERO,
                closing_balance: Decimal::ZERO,
            });
        entry.posting_sum += amount;
        if !entry.source_refs.contains(&tx.source_ref) {
            entry.source_refs.push(tx.source_ref.clone());
        }
    }

    let mut complete = Vec::new();
    let mut gaps = Vec::new();
    let mut duplicates = Vec::new();
    let mut discontinuities = Vec::new();
    let mut intra_period_failures = Vec::new();

    let expected_months: BTreeSet<(i32, u32)> =
        (1..=12).map(|m| (request.tax_year, m)).collect();

    for account_id in &request.account_ids {
        let present: BTreeSet<(i32, u32)> = by_period
            .keys()
            .filter(|(aid, _, _)| aid == account_id)
            .map(|(_, y, m)| (*y, *m))
            .collect();

        let missing: Vec<(i32, u32)> = expected_months
            .difference(&present)
            .copied()
            .collect();
        for (year, month) in &missing {
            gaps.push((
                account_id.clone(),
                format!("{:04}-{:02}", year, month),
            ));
        }

        let mut months_for_account: Vec<&AccountMonthAccum> = by_period
            .iter()
            .filter(|((aid, _, _), _)| aid == account_id)
            .map(|(_, v)| v)
            .collect();
        months_for_account.sort_by(|a, b| a.year.cmp(&b.year).then(a.month.cmp(&b.month)));

        for acc in &months_for_account {
            let ym = format!("{:04}-{:02}", acc.year, acc.month);
            let cov = AccountPeriodCoverage {
                account_id: acc.account_id.clone(),
                period: ym,
                opening_balance: acc.opening_balance.to_string(),
                closing_balance: acc.closing_balance.to_string(),
                posting_sum: acc.posting_sum.to_string(),
                source_refs: acc.source_refs.clone(),
            };

            if acc.posting_sum < Decimal::ZERO {
                intra_period_failures.push(cov.clone());
            }

            if acc.source_refs.len() > 1 {
                duplicates.push(cov.clone());
            }

            complete.push(cov);
        }

        for window in months_for_account.windows(2) {
            let prev = window[0];
            let next = window[1];
            if (next.year == prev.year && next.month == prev.month + 1)
                || (next.year == prev.year + 1 && prev.month == 12 && next.month == 1)
            {
                if prev.closing_balance != next.opening_balance {
                    discontinuities.push(Discontinuity {
                        account_id: account_id.clone(),
                        from_period: format!("{:04}-{:02}", prev.year, prev.month),
                        to_period: format!("{:04}-{:02}", next.year, next.month),
                        expected_opening: prev.closing_balance.to_string(),
                        actual_opening: next.opening_balance.to_string(),
                    });
                }
            }
        }
    }

    gaps.sort();
    complete.sort_by(|a, b| {
        a.account_id
            .cmp(&b.account_id)
            .then_with(|| a.period.cmp(&b.period))
    });

    Ok(CoverageReport {
        has_gaps: !gaps.is_empty(),
        has_duplicates: !duplicates.is_empty(),
        has_discontinuities: !discontinuities.is_empty(),
        has_intra_period_failures: !intra_period_failures.is_empty(),
        complete,
        gaps,
        duplicates,
        discontinuities,
        intra_period_failures,
    })
}

#[derive(Debug, Clone)]
struct AccountMonthAccum {
    account_id: String,
    year: i32,
    month: u32,
    posting_sum: Decimal,
    source_refs: Vec<String>,
    opening_balance: Decimal,
    closing_balance: Decimal,
}

pub fn coverage_request_from_json(args: &serde_json::Value) -> Result<CoverageRequest, ToolError> {
    let account_ids: Vec<String> = args
        .get("account_ids")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .ok_or_else(|| {
            ToolError::InvalidInput("coverage requires account_ids array".to_string())
        })?;
    let tax_year = args
        .get("tax_year")
        .and_then(|v| v.as_i64())
        .map(|y| y as i32)
        .ok_or_else(|| {
            ToolError::InvalidInput("coverage requires tax_year integer".to_string())
        })?;
    Ok(CoverageRequest {
        account_ids,
        tax_year,
    })
}
