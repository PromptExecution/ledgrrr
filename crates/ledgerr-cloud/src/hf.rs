//! HuggingFace Jobs provider — structurally has no billing/budget API.
//!
//! Ports the bash datum's two-part HF behavior:
//! - `check_auth` / `fetch_budget`'s authoritative path: always
//!   [`ProviderError::NoApi`] / a declared (non-authoritative) cap, since
//!   HF Jobs has nothing to be authoritative about.
//! - A separate spend *estimate* from `hf jobs ps --all`, exposed via
//!   [`HfProvider::estimate_spend_cake`] (not part of [`BudgetProvider`] —
//!   AWS/GCP/Azure have no equivalent "estimate current spend" concept, so
//!   it doesn't belong on the shared trait).

use rust_decimal::Decimal;
use tokio::process::Command;

use crate::config::CAKE_MONTHLY_CAP_ENV;
use crate::error::ProviderError;
use crate::provider::{BudgetInfo, BudgetProvider};

/// TSV column index of job status in `hf jobs ps --all` output.
const STATUS_COLUMN: usize = 4;
/// TSV column index of job duration in `hf jobs ps --all` output.
const DURATION_COLUMN: usize = 5;

#[derive(Debug, Clone)]
pub struct HfProvider {
    /// cake/hour rate for the `a100_large` instance type.
    pub a100_large_rate: Decimal,
}

impl HfProvider {
    pub fn new(a100_large_rate: Decimal) -> Self {
        Self { a100_large_rate }
    }

    /// Estimate current-period spend from `hf jobs ps --all`: sums each
    /// non-`CANCELED`/`ERROR` job's duration (column 5) at `a100_large_rate`
    /// cake/hour. Not part of [`BudgetProvider`] — see module docs.
    pub async fn estimate_spend_cake(&self) -> Result<Decimal, ProviderError> {
        let output = Command::new("hf").args(["jobs", "ps", "--all"]).output().await?;
        if !output.status.success() {
            return Err(ProviderError::Parse(format!(
                "hf jobs ps --all failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        estimate_spend_from_ps_output(&stdout, self.a100_large_rate)
    }
}

impl BudgetProvider for HfProvider {
    /// HF Jobs has no billing/budget API — always structurally [`ProviderError::NoApi`].
    async fn check_auth(&self) -> Result<(), ProviderError> {
        Err(ProviderError::NoApi)
    }

    /// Returns the declared (non-authoritative) `CAKE_MONTHLY_CAP` ceiling,
    /// if set — `Ok(None)` otherwise. This is the only "cap" HF has; it is
    /// never authoritative, since there is no billing API behind it.
    async fn fetch_budget(&self) -> Result<Option<BudgetInfo>, ProviderError> {
        let Ok(raw) = std::env::var(CAKE_MONTHLY_CAP_ENV) else {
            return Ok(None);
        };
        let cap = raw.trim().parse::<Decimal>().map_err(|e| {
            ProviderError::Parse(format!("invalid {CAKE_MONTHLY_CAP_ENV} '{raw}': {e}"))
        })?;
        Ok(Some(BudgetInfo {
            cap_cake: cap,
            authoritative: false,
            source: format!("hf ({CAKE_MONTHLY_CAP_ENV} declared)"),
        }))
    }
}

/// Parse `hf jobs ps --all` TSV stdout (first line is a header, skipped)
/// and sum billable hours × `rate_cake_per_hour`.
pub(crate) fn estimate_spend_from_ps_output(
    stdout: &str,
    rate_cake_per_hour: Decimal,
) -> Result<Decimal, ProviderError> {
    let mut total_hours = Decimal::ZERO;
    for line in stdout.lines().skip(1) {
        if line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        let status = cols.get(STATUS_COLUMN).copied().unwrap_or("");
        let duration = cols.get(DURATION_COLUMN).copied().unwrap_or("");
        if status.eq_ignore_ascii_case("CANCELED") || status.eq_ignore_ascii_case("ERROR") {
            continue;
        }
        total_hours += parse_duration_to_hours(duration)?;
    }
    Ok(total_hours * rate_cake_per_hour)
}

/// Parse a duration string like `"1h 23m 45s"` (any of the three parts
/// optional, whitespace-separated) into fractional hours.
fn parse_duration_to_hours(s: &str) -> Result<Decimal, ProviderError> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Ok(Decimal::ZERO);
    }

    let mut total_seconds: i64 = 0;
    let mut digits = String::new();
    for ch in trimmed.chars() {
        if ch.is_ascii_digit() {
            digits.push(ch);
        } else if ch.is_whitespace() {
            continue;
        } else if matches!(ch, 'h' | 'm' | 's') {
            if digits.is_empty() {
                return Err(ProviderError::Parse(format!(
                    "duration unit '{ch}' with no preceding digits in '{s}'"
                )));
            }
            let n: i64 = digits
                .parse()
                .map_err(|e| ProviderError::Parse(format!("bad duration digits in '{s}': {e}")))?;
            digits.clear();
            let seconds = match ch {
                'h' => n.checked_mul(3600),
                'm' => n.checked_mul(60),
                's' => Some(n),
                _ => None,
            }
            .ok_or_else(|| ProviderError::Parse(format!("duration overflow in '{s}'")))?;
            total_seconds = total_seconds
                .checked_add(seconds)
                .ok_or_else(|| ProviderError::Parse(format!("duration overflow in '{s}'")))?;
        } else {
            return Err(ProviderError::Parse(format!(
                "unexpected character '{ch}' in duration '{s}'"
            )));
        }
    }
    if !digits.is_empty() {
        return Err(ProviderError::Parse(format!(
            "trailing digits with no unit in duration '{s}'"
        )));
    }

    Ok(Decimal::from(total_seconds) / Decimal::from(3600))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_parses_full_hms() {
        let hours = parse_duration_to_hours("1h 23m 45s").expect("parses");
        // 1 + 23/60 + 45/3600 hours
        let expected = Decimal::from(3600 + 23 * 60 + 45) / Decimal::from(3600);
        assert_eq!(hours, expected);
    }

    #[test]
    fn duration_parses_partial_forms() {
        assert_eq!(
            parse_duration_to_hours("45s").expect("parses"),
            Decimal::from(45) / Decimal::from(3600)
        );
        assert_eq!(
            parse_duration_to_hours("23m").expect("parses"),
            Decimal::from(23 * 60) / Decimal::from(3600)
        );
        assert_eq!(parse_duration_to_hours("2h").expect("parses"), Decimal::from(2));
    }

    #[test]
    fn duration_empty_is_zero() {
        assert_eq!(parse_duration_to_hours("").expect("parses"), Decimal::ZERO);
    }

    #[test]
    fn duration_rejects_garbage() {
        assert!(parse_duration_to_hours("banana").is_err());
    }

    fn fixture(name: &str) -> String {
        let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read fixture {path}: {e}"))
    }

    #[test]
    fn ps_output_excludes_canceled_and_error_rows() {
        let stdout = fixture("hf_jobs_ps.tsv");
        // rate = 2.50 cake/hour, matching the documented a100_large default.
        let spend = estimate_spend_from_ps_output(&stdout, Decimal::new(250, 2)).expect("parses");
        // Only the RUNNING (1h) and COMPLETED (30m) rows count:
        // 1.5h * 2.50 = 3.75 cake. CANCELED/ERROR rows are excluded.
        assert_eq!(spend, Decimal::new(375, 2));
    }
}
