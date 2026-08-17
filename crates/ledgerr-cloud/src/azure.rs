//! Azure Consumption Budgets provider — ports the bash datum's
//! `az consumption budget list` check.

use rust_decimal::Decimal;
use serde_json::Value;
use tokio::process::Command;

use crate::error::ProviderError;
use crate::provider::{BudgetInfo, BudgetProvider};

/// Default Azure subscription ID, ported verbatim from the original
/// `_b00t_/cloud-budget.tomllmd` bash datum.
pub const DEFAULT_AZURE_SUBSCRIPTION_ID: &str = "a5a9206f";

#[derive(Debug, Clone)]
pub struct AzureProvider {
    pub subscription_id: String,
    pub usd_per_cake: Decimal,
}

impl AzureProvider {
    pub fn new(usd_per_cake: Decimal) -> Self {
        Self {
            subscription_id: DEFAULT_AZURE_SUBSCRIPTION_ID.to_string(),
            usd_per_cake,
        }
    }

    pub fn with_subscription_id(mut self, subscription_id: impl Into<String>) -> Self {
        self.subscription_id = subscription_id.into();
        self
    }
}

impl BudgetProvider for AzureProvider {
    async fn check_auth(&self) -> Result<(), ProviderError> {
        let output = Command::new("az").args(["account", "show"]).output().await?;
        if !output.status.success() {
            return Err(ProviderError::AuthRequired(format!(
                "az account show failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(())
    }

    async fn fetch_budget(&self) -> Result<Option<BudgetInfo>, ProviderError> {
        let output = Command::new("az")
            .args([
                "consumption",
                "budget",
                "list",
                "--subscription",
                &self.subscription_id,
            ])
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("AADSTS") || stderr.contains("az login") || stderr.contains("expired")
            {
                return Err(ProviderError::AuthRequired(stderr.into_owned()));
            }
            return Err(ProviderError::Parse(format!(
                "az consumption budget list failed: {stderr}"
            )));
        }

        parse_azure_budgets(&stdout, self.usd_per_cake)
    }
}

/// Parse `az consumption budget list` stdout (a JSON array of Azure Budget
/// resources, each with a top-level numeric `amount` field) into an
/// optional [`BudgetInfo`], summing across all entries found.
pub(crate) fn parse_azure_budgets(
    stdout: &str,
    usd_per_cake: Decimal,
) -> Result<Option<BudgetInfo>, ProviderError> {
    let json: Value = serde_json::from_str(stdout)
        .map_err(|e| ProviderError::Parse(format!("invalid az consumption budget JSON: {e}")))?;

    let Some(budgets) = json.as_array() else {
        return Ok(None);
    };
    if budgets.is_empty() {
        return Ok(None);
    }

    let mut total_usd = Decimal::ZERO;
    let mut found = 0usize;
    for budget in budgets {
        if let Some(amount) = budget.get("amount") {
            let usd = value_to_decimal(amount).ok_or_else(|| {
                ProviderError::Parse(format!("invalid azure budget amount: {amount}"))
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
        source: format!("az consumption budget list ({found} budgets found)"),
    }))
}

fn value_to_decimal(v: &Value) -> Option<Decimal> {
    if let Some(s) = v.as_str() {
        s.parse().ok()
    } else if let Some(f) = v.as_f64() {
        Decimal::from_f64_retain(f)
    } else {
        None
    }
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
        let stdout = fixture("azure_budgets_found.json");
        let info = parse_azure_budgets(&stdout, Decimal::new(1000, 2))
            .expect("parse ok")
            .expect("some budget");
        assert!(info.authoritative);
        assert_eq!(info.cap_cake, Decimal::new(4000, 2)); // 400/10 = 40.00 cake
    }

    #[test]
    fn empty_array_is_unset() {
        let info = parse_azure_budgets("[]", Decimal::new(1000, 2)).expect("parse ok");
        assert!(info.is_none());
    }
}
