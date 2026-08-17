//! AWS Budgets provider — ports the bash datum's
//! `aws budgets describe-budgets --account-id <id>` check.

use rust_decimal::Decimal;
use serde_json::Value;
use tokio::process::Command;

use crate::error::ProviderError;
use crate::provider::{BudgetInfo, BudgetProvider};

/// Default AWS account ID, ported verbatim from the original
/// `_b00t_/cloud-budget.tomllmd` bash datum.
pub const DEFAULT_AWS_ACCOUNT_ID: &str = "968589500754";

#[derive(Debug, Clone)]
pub struct AwsProvider {
    pub account_id: String,
    pub usd_per_cake: Decimal,
}

impl AwsProvider {
    pub fn new(usd_per_cake: Decimal) -> Self {
        Self {
            account_id: DEFAULT_AWS_ACCOUNT_ID.to_string(),
            usd_per_cake,
        }
    }

    pub fn with_account_id(mut self, account_id: impl Into<String>) -> Self {
        self.account_id = account_id.into();
        self
    }
}

impl BudgetProvider for AwsProvider {
    async fn check_auth(&self) -> Result<(), ProviderError> {
        if std::env::var("AWS_ACCESS_KEY_ID").is_err() {
            return Err(ProviderError::AuthRequired(
                "AWS_ACCESS_KEY_ID is not set".to_string(),
            ));
        }
        let output = Command::new("aws")
            .args(["sts", "get-caller-identity"])
            .output()
            .await?;
        if !output.status.success() {
            return Err(ProviderError::AuthRequired(format!(
                "aws sts get-caller-identity failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(())
    }

    async fn fetch_budget(&self) -> Result<Option<BudgetInfo>, ProviderError> {
        let output = Command::new("aws")
            .args([
                "budgets",
                "describe-budgets",
                "--account-id",
                &self.account_id,
                "--output",
                "json",
            ])
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("No results") || stdout.contains("Budgets: []") {
                return Ok(None);
            }
            return Err(ProviderError::Parse(format!(
                "aws budgets describe-budgets failed: {stderr}"
            )));
        }

        parse_aws_budgets(&stdout, self.usd_per_cake)
    }
}

/// Parse `aws budgets describe-budgets --output json` stdout into an
/// optional [`BudgetInfo`].
///
/// - No `Budgets` array, or an empty one → `Ok(None)` (the datum's
///   `Budgets: []` UNSET case).
/// - First budget missing `BudgetName` → `Ok(None)` (defensive; matches the
///   datum's "presence of BudgetName" RECONCILED signal, inverted).
/// - Otherwise extracts `Budgets[0].BudgetLimit.Amount` (a decimal string in
///   the real AWS Budgets API shape) and converts USD → cake.
pub(crate) fn parse_aws_budgets(
    stdout: &str,
    usd_per_cake: Decimal,
) -> Result<Option<BudgetInfo>, ProviderError> {
    let json: Value = serde_json::from_str(stdout)
        .map_err(|e| ProviderError::Parse(format!("invalid aws budgets JSON: {e}")))?;

    let Some(budgets) = json.get("Budgets").and_then(Value::as_array) else {
        return Ok(None);
    };
    let Some(first) = budgets.first() else {
        return Ok(None);
    };
    if first.get("BudgetName").is_none() {
        return Ok(None);
    }

    let amount_str = first
        .get("BudgetLimit")
        .and_then(|v| v.get("Amount"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            ProviderError::Parse("missing Budgets[0].BudgetLimit.Amount".to_string())
        })?;

    let usd: Decimal = amount_str.parse().map_err(|e| {
        ProviderError::Parse(format!("invalid BudgetLimit.Amount '{amount_str}': {e}"))
    })?;

    if usd_per_cake.is_zero() {
        return Err(ProviderError::Parse("usd_per_cake is zero".to_string()));
    }

    Ok(Some(BudgetInfo {
        cap_cake: usd / usd_per_cake,
        authoritative: true,
        source: "aws budgets describe-budgets".to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let path = format!(
            "{}/tests/fixtures/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read fixture {path}: {e}"))
    }

    #[test]
    fn parses_found_budget() {
        let stdout = fixture("aws_budgets_found.json");
        let info = parse_aws_budgets(&stdout, Decimal::new(1000, 2))
            .expect("parse ok")
            .expect("some budget");
        assert!(info.authoritative);
        assert_eq!(info.cap_cake, Decimal::new(5000, 2)); // $500 / $10 = 50.00 cake
    }

    #[test]
    fn empty_budgets_array_is_unset() {
        let stdout = fixture("aws_budgets_empty.json");
        let info = parse_aws_budgets(&stdout, Decimal::new(1000, 2)).expect("parse ok");
        assert!(info.is_none());
    }

    #[test]
    fn malformed_json_is_parse_error() {
        let err = parse_aws_budgets("not json", Decimal::new(1000, 2)).unwrap_err();
        assert!(matches!(err, ProviderError::Parse(_)));
    }
}
