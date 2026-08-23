//! The `BudgetProvider` trait — one impl per cloud/service budget source.

use rust_decimal::Decimal;

use crate::error::ProviderError;

/// A resolved budget cap, denominated in 🎂 (cake — this project's soft
/// internal spend-tracking unit; see [`crate::config::CloudBudgetConfig`]
/// for the USD conversion ratio).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetInfo {
    /// Budget cap in cake units.
    pub cap_cake: Decimal,
    /// `true` when this cap came from the provider's own authoritative
    /// billing API (AWS Budgets, GCP Billing Budgets, Azure Consumption
    /// Budgets). `false` when it is a declared, non-authoritative ceiling
    /// (e.g. HuggingFace's `CAKE_MONTHLY_CAP` override — HF Jobs has no
    /// budget API to be authoritative about).
    pub authoritative: bool,
    /// Human-readable provenance, e.g. `"aws budgets describe-budgets"`.
    pub source: String,
}

/// One budget-checkable provider (AWS, GCP, Azure, HuggingFace Jobs, ...).
///
/// Implementations shell out to an already-installed CLI via
/// `tokio::process::Command` rather than linking a cloud SDK — see the
/// crate-level docs for why.
pub trait BudgetProvider {
    /// Verify the CLI is authenticated (or, for providers with no budget
    /// API at all, return [`ProviderError::NoApi`] unconditionally).
    fn check_auth(&self) -> impl std::future::Future<Output = Result<(), ProviderError>> + Send;

    /// Fetch the current budget cap, if one is configured upstream.
    /// `Ok(None)` means "reachable, authenticated, but no budget configured"
    /// (the datum's UNSET case) — distinct from an `Err`, which means the
    /// check itself failed.
    fn fetch_budget(
        &self,
    ) -> impl std::future::Future<Output = Result<Option<BudgetInfo>, ProviderError>> + Send;
}
