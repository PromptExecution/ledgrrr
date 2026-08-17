//! Runs every configured [`BudgetProvider`] and produces a single report.
//!
//! **Contract**: one provider's failure must never abort the run or panic.
//! [`ReconcileRunner::run`] catches every provider `Result` internally and
//! maps failures to a [`ReconcileStatus::Fail`] (or `Skip`, for structural
//! `NoApi` providers) entry — nothing propagates via `?` out of `run`.

use rust_decimal::Decimal;

use crate::aws::AwsProvider;
use crate::azure::AzureProvider;
use crate::config::CloudBudgetConfig;
use crate::error::ProviderError;
use crate::gcp::GcpProvider;
use crate::hf::HfProvider;
use crate::provider::BudgetProvider;

/// Outcome of reconciling a single provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconcileStatus {
    /// Authenticated and checked successfully (a cap may or may not be set).
    Pass,
    /// Authentication or the budget check itself failed.
    Fail,
    /// The provider has no budget API to check (structural — see
    /// [`ProviderError::NoApi`]); any `cap_cake` present is a declared,
    /// non-authoritative value.
    Skip,
}

/// Per-provider reconciliation result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderStatus {
    pub provider: String,
    pub status: ReconcileStatus,
    pub cap_cake: Option<Decimal>,
    /// `true` when `cap_cake` came from the provider's own authoritative
    /// billing API; `false` for declared/non-authoritative caps or when
    /// `cap_cake` is `None`.
    pub authoritative: bool,
    pub error: Option<String>,
}

/// Full reconciliation report across all configured providers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconcileReport {
    pub providers: Vec<ProviderStatus>,
}

/// Runs AWS, GCP, Azure, and HuggingFace Jobs budget checks.
pub struct ReconcileRunner {
    aws: AwsProvider,
    gcp: GcpProvider,
    azure: AzureProvider,
    hf: HfProvider,
}

impl ReconcileRunner {
    /// Construct with all four providers using [`CloudBudgetConfig::default`].
    pub fn default_providers() -> Self {
        Self::with_config(&CloudBudgetConfig::default())
    }

    /// Construct all four providers from an explicit config.
    pub fn with_config(config: &CloudBudgetConfig) -> Self {
        Self {
            aws: AwsProvider::new(config.usd_per_cake),
            gcp: GcpProvider::new(config.usd_per_cake),
            azure: AzureProvider::new(config.usd_per_cake),
            hf: HfProvider::new(config.hf_a100_large_rate),
        }
    }

    /// Run all four providers. Never panics or short-circuits on a single
    /// provider's error — every provider gets its own [`ProviderStatus`].
    pub async fn run(&self) -> ReconcileReport {
        let providers = vec![
            run_one("aws", &self.aws).await,
            run_one("gcp", &self.gcp).await,
            run_one("azure", &self.azure).await,
            run_one("hf", &self.hf).await,
        ];
        ReconcileReport { providers }
    }
}

/// Run a single provider's `check_auth` + `fetch_budget`, catching every
/// `Result` into a [`ProviderStatus`]. Generic over `P: BudgetProvider`
/// rather than `dyn` — the trait's `impl Future` return position is not
/// object-safe, and no dynamic dispatch is needed for a fixed 4-provider set.
pub(crate) async fn run_one<P: BudgetProvider>(name: &str, provider: &P) -> ProviderStatus {
    match provider.check_auth().await {
        Ok(()) => match provider.fetch_budget().await {
            Ok(Some(info)) => ProviderStatus {
                provider: name.to_string(),
                status: ReconcileStatus::Pass,
                cap_cake: Some(info.cap_cake),
                authoritative: info.authoritative,
                error: None,
            },
            Ok(None) => ProviderStatus {
                provider: name.to_string(),
                status: ReconcileStatus::Pass,
                cap_cake: None,
                authoritative: false,
                error: None,
            },
            Err(e) => fail(name, &e),
        },
        Err(ProviderError::NoApi) => match provider.fetch_budget().await {
            Ok(Some(info)) => ProviderStatus {
                provider: name.to_string(),
                status: ReconcileStatus::Skip,
                cap_cake: Some(info.cap_cake),
                authoritative: info.authoritative,
                error: Some("no billing/budget API for this provider".to_string()),
            },
            Ok(None) => ProviderStatus {
                provider: name.to_string(),
                status: ReconcileStatus::Skip,
                cap_cake: None,
                authoritative: false,
                error: Some("no billing/budget API for this provider".to_string()),
            },
            Err(e) => ProviderStatus {
                provider: name.to_string(),
                status: ReconcileStatus::Skip,
                cap_cake: None,
                authoritative: false,
                error: Some(e.to_string()),
            },
        },
        Err(e) => fail(name, &e),
    }
}

fn fail(name: &str, e: &ProviderError) -> ProviderStatus {
    ProviderStatus {
        provider: name.to_string(),
        status: ReconcileStatus::Fail,
        cap_cake: None,
        authoritative: false,
        error: Some(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::BudgetInfo;
    use std::future::Future;

    /// A fake provider whose auth/fetch outcomes are set at construction —
    /// no real CLI is ever shelled out to in these unit tests.
    struct FakeProvider {
        auth_result: Result<(), ProviderError>,
        budget_result: Result<Option<BudgetInfo>, ProviderError>,
    }

    impl Clone for FakeProvider {
        fn clone(&self) -> Self {
            Self {
                auth_result: match &self.auth_result {
                    Ok(()) => Ok(()),
                    Err(ProviderError::NoApi) => Err(ProviderError::NoApi),
                    Err(ProviderError::AuthRequired(s)) => Err(ProviderError::AuthRequired(s.clone())),
                    Err(ProviderError::Parse(s)) => Err(ProviderError::Parse(s.clone())),
                    Err(ProviderError::Io(_)) => Err(ProviderError::Parse("io".to_string())),
                },
                budget_result: match &self.budget_result {
                    Ok(v) => Ok(v.clone()),
                    Err(ProviderError::NoApi) => Err(ProviderError::NoApi),
                    Err(ProviderError::AuthRequired(s)) => Err(ProviderError::AuthRequired(s.clone())),
                    Err(ProviderError::Parse(s)) => Err(ProviderError::Parse(s.clone())),
                    Err(ProviderError::Io(_)) => Err(ProviderError::Parse("io".to_string())),
                },
            }
        }
    }

    impl BudgetProvider for FakeProvider {
        fn check_auth(&self) -> impl Future<Output = Result<(), ProviderError>> + Send {
            let result = self.clone().auth_result;
            async move { result }
        }
        fn fetch_budget(&self) -> impl Future<Output = Result<Option<BudgetInfo>, ProviderError>> + Send {
            let result = self.clone().budget_result;
            async move { result }
        }
    }

    #[tokio::test]
    async fn passing_provider_reports_pass_with_cap() {
        let p = FakeProvider {
            auth_result: Ok(()),
            budget_result: Ok(Some(BudgetInfo {
                cap_cake: Decimal::new(1000, 2),
                authoritative: true,
                source: "fake".to_string(),
            })),
        };
        let status = run_one("fake", &p).await;
        assert_eq!(status.status, ReconcileStatus::Pass);
        assert_eq!(status.cap_cake, Some(Decimal::new(1000, 2)));
        assert!(status.authoritative);
        assert!(status.error.is_none());
    }

    #[tokio::test]
    async fn auth_failure_reports_fail_without_aborting() {
        let p = FakeProvider {
            auth_result: Err(ProviderError::AuthRequired("no creds".to_string())),
            budget_result: Ok(None),
        };
        let status = run_one("fake", &p).await;
        assert_eq!(status.status, ReconcileStatus::Fail);
        assert!(status.cap_cake.is_none());
        assert!(status.error.unwrap().contains("no creds"));
    }

    #[tokio::test]
    async fn no_api_provider_reports_skip_with_declared_cap() {
        let p = FakeProvider {
            auth_result: Err(ProviderError::NoApi),
            budget_result: Ok(Some(BudgetInfo {
                cap_cake: Decimal::new(4000, 2),
                authoritative: false,
                source: "hf (CAKE_MONTHLY_CAP declared)".to_string(),
            })),
        };
        let status = run_one("hf", &p).await;
        assert_eq!(status.status, ReconcileStatus::Skip);
        assert_eq!(status.cap_cake, Some(Decimal::new(4000, 2)));
        assert!(!status.authoritative); // declared, not authoritative — round-trips through the report
        assert!(status.error.is_some());
    }

    #[tokio::test]
    async fn no_api_provider_with_no_declared_cap_is_skip_without_cap() {
        let p = FakeProvider {
            auth_result: Err(ProviderError::NoApi),
            budget_result: Ok(None),
        };
        let status = run_one("hf", &p).await;
        assert_eq!(status.status, ReconcileStatus::Skip);
        assert!(status.cap_cake.is_none());
    }

    #[tokio::test]
    async fn one_provider_failing_does_not_affect_others() {
        let failing = FakeProvider {
            auth_result: Err(ProviderError::AuthRequired("boom".to_string())),
            budget_result: Ok(None),
        };
        let passing = FakeProvider {
            auth_result: Ok(()),
            budget_result: Ok(Some(BudgetInfo {
                cap_cake: Decimal::new(2000, 2),
                authoritative: true,
                source: "fake".to_string(),
            })),
        };

        let a = run_one("failing", &failing).await;
        let b = run_one("passing", &passing).await;

        assert_eq!(a.status, ReconcileStatus::Fail);
        assert_eq!(b.status, ReconcileStatus::Pass);
        assert_eq!(b.cap_cake, Some(Decimal::new(2000, 2)));
    }

    // `ReconcileRunner::run` against the real AWS/GCP/Azure/HF providers is
    // exercised in `tests/reconcile_integration.rs`, not here — this module
    // stays CLI-free per the "no real CLI shell-outs in unit tests" rule.
}
