//! Typed error boundary for cloud budget providers.

use thiserror::Error;

/// Errors a [`crate::provider::BudgetProvider`] implementation may return.
///
/// Every variant is auditable text — no silently discarded provider output.
#[derive(Debug, Error)]
pub enum ProviderError {
    /// The provider has no budget/billing API at all (structural, not transient).
    /// HuggingFace Jobs is the canonical example — always returns this from
    /// `check_auth`, since there is nothing to authenticate a budget check against.
    #[error("provider has no billing/budget API")]
    NoApi,

    /// The provider's CLI ran but reported the caller is not authenticated
    /// (or a required credential env var is absent). The `String` carries the
    /// CLI's own diagnostic text for audit purposes.
    #[error("authentication required: {0}")]
    AuthRequired(String),

    /// Spawning or waiting on the provider CLI process failed at the OS level.
    #[error("provider CLI process error: {0}")]
    Io(#[from] std::io::Error),

    /// The CLI ran and reported success, but its output could not be parsed
    /// into the expected shape.
    #[error("failed to parse provider output: {0}")]
    Parse(String),
}
