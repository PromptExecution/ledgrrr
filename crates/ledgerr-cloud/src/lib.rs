//! `ledgerr-cloud` — GPU-training cloud budget reconciliation.
//!
//! Ports the `_b00t_/cloud-budget.tomllmd` bash/python-embedded-script
//! datum's provider CLI-shell-out logic into typed, panic-free Rust. Each
//! provider shells out to its already-installed CLI (`aws`, `gcloud`, `az`,
//! `hf`) via `tokio::process::Command` — no cloud SDK dependencies, per this
//! repo's Postel's-Law-on-tools convention of using already-present CLI
//! tooling over new heavy deps.
//!
//! # Cake units
//! 1🎂 (cake) = `usd_per_cake` USD (default $10.00, see
//! [`config::CloudBudgetConfig`]) — a soft internal budget-tracking unit,
//! not real currency, but modeled as `rust_decimal::Decimal` per this
//! project's money-typing convention.
//!
//! # See also
//! GitHub issues #107-#111 describe this subsystem prospectively; #111 is
//! the tracking issue for the real implementation built here.

pub mod aws;
pub mod azure;
pub mod config;
pub mod error;
pub mod gcp;
pub mod hf;
pub mod provider;
pub mod reconcile;

pub use aws::AwsProvider;
pub use azure::AzureProvider;
pub use config::CloudBudgetConfig;
pub use error::ProviderError;
pub use gcp::GcpProvider;
pub use hf::HfProvider;
pub use provider::{BudgetInfo, BudgetProvider};
pub use reconcile::{ProviderStatus, ReconcileReport, ReconcileRunner, ReconcileStatus};
