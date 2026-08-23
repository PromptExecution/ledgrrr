//! GCP Cloud Billing Budgets provider — ports the bash datum's
//! `gcloud billing budgets list --format=json` check.

use rust_decimal::Decimal;
use serde_json::Value;
use tokio::process::Command;

use crate::error::ProviderError;
use crate::provider::{BudgetInfo, BudgetProvider};

#[derive(Debug, Clone, Default)]
pub struct GcpProvider {
    pub usd_per_cake: Decimal,
}

impl GcpProvider {
    pub fn new(usd_per_cake: Decimal) -> Self {
        Self { usd_per_cake }
    }
}

impl BudgetProvider for GcpProvider {
    async fn check_auth(&self) -> Result<(), ProviderError> {
        let output = Command::new("gcloud")
            .args(["auth", "print-access-token"])
            .output()
            .await?;
        if !output.status.success() {
            return Err(ProviderError::AuthRequired(format!(
                "gcloud auth print-access-token failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(())
    }

    async fn fetch_budget(&self) -> Result<Option<BudgetInfo>, ProviderError> {
        let output = Command::new("gcloud")
            .args(["billing", "budgets", "list", "--format=json"])
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("Reauth") || stderr.contains("auth login") || stderr.contains("expired")
            {
                return Err(ProviderError::AuthRequired(stderr.into_owned()));
            }
            return Err(ProviderError::Parse(format!(
                "gcloud billing budgets list failed: {stderr}"
            )));
        }

        parse_gcp_budgets(&stdout, self.usd_per_cake)
    }
}

/// Parse `gcloud billing budgets list --format=json` stdout into an
/// optional [`BudgetInfo`].
///
/// The datum's check was `output contains "amount"` → RECONCILED "with N
/// budgets found" (no single cap value specified). This parses the real
/// Cloud Billing Budget API shape (`amount.specifiedAmount.units`, a
/// decimal string) for each entry and sums them, reporting the count found
/// in `source` per the datum's phrasing.
pub(crate) fn parse_gcp_budgets(
    stdout: &str,
    usd_per_cake: Decimal,
) -> Result<Option<BudgetInfo>, ProviderError> {
    let json: Value = serde_json::from_str(stdout)
        .map_err(|e| ProviderError::Parse(format!("invalid gcloud budgets JSON: {e}")))?;

    let Some(budgets) = json.as_array() else {
        return Ok(None);
    };
    if budgets.is_empty() {
        return Ok(None);
    }

    let mut total_usd = Decimal::ZERO;
    let mut found = 0usize;
    for budget in budgets {
        let units = budget
            .get("amount")
            .and_then(|a| a.get("specifiedAmount"))
            .and_then(|a| a.get("units"))
            .and_then(Value::as_str);
        if let Some(units) = units {
            let usd: Decimal = units.parse().map_err(|e| {
                ProviderError::Parse(format!("invalid gcp budget units '{units}': {e}"))
            })?;
            total_usd += usd;
            found += 1;
        }
    }

    if found == 0 {
        return Ok(None);
    }
    if usd_per_cake.is_zero() {
        return Err(ProviderError::Parse("usd_per_cake is zero".to_string()));
    }

    Ok(Some(BudgetInfo {
        cap_cake: total_usd / usd_per_cake,
        authoritative: true,
        source: format!("gcloud billing budgets list ({found} budgets found)"),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read fixture {path}: {e}"))
    }

    #[test]
    fn parses_found_budgets_and_sums() {
        let stdout = fixture("gcp_budgets_found.json");
        let info = parse_gcp_budgets(&stdout, Decimal::new(1000, 2))
            .expect("parse ok")
            .expect("some budget");
        assert!(info.authoritative);
        assert_eq!(info.cap_cake, Decimal::new(8000, 2)); // (500+300)/10 = 80.00 cake
        assert!(info.source.contains("2 budgets found"));
    }

    #[test]
    fn empty_array_is_unset() {
        let info = parse_gcp_budgets("[]", Decimal::new(1000, 2)).expect("parse ok");
        assert!(info.is_none());
    }
}
