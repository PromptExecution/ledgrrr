//! Crate-owned configuration for cloud budget reconciliation.
//!
//! Deliberately does **not** read `_b00t_/cloud-budget.tomllmd` — that file
//! lives in a different repository (`elasticdotventures/_b00t_`) and is not
//! accessible from this build. Defaults here mirror its documented values;
//! env vars remain the only runtime override surface.

use rust_decimal::Decimal;

/// USD-per-cake env override, mirroring the bash datum's `CAKE_USD_RATE`.
pub const CAKE_USD_RATE_ENV: &str = "CAKE_USD_RATE";
/// Declared (non-authoritative) monthly cake cap env override, mirroring
/// the bash datum's `CAKE_MONTHLY_CAP`.
pub const CAKE_MONTHLY_CAP_ENV: &str = "CAKE_MONTHLY_CAP";

/// Default USD-per-cake ratio: 1🎂 = $10.00 USD.
fn default_usd_per_cake() -> Decimal {
    Decimal::new(1000, 2)
}

/// Default HuggingFace `a100_large` hourly rate: 2.50🎂/hour.
fn default_hf_a100_large_rate() -> Decimal {
    Decimal::new(250, 2)
}

/// Default warn threshold: 75% of cap.
fn default_warn_threshold() -> Decimal {
    Decimal::new(75, 2)
}

/// Default gate threshold: 90% of cap.
fn default_gate_threshold() -> Decimal {
    Decimal::new(90, 2)
}

/// Typed configuration for budget reconciliation, with env-var overrides
/// resolved once at construction (via [`CloudBudgetConfig::default`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudBudgetConfig {
    /// USD value of one cake unit. Override via `CAKE_USD_RATE`.
    pub usd_per_cake: Decimal,
    /// Declared, non-authoritative monthly cake ceiling. Override via
    /// `CAKE_MONTHLY_CAP`. `None` when unset.
    pub monthly_cap_cake: Option<Decimal>,
    /// HuggingFace `a100_large` hourly rate, in cake/hour.
    pub hf_a100_large_rate: Decimal,
    /// Fraction of cap at which to warn (0.0-1.0).
    pub warn_threshold: Decimal,
    /// Fraction of cap at which to gate further spend (0.0-1.0).
    pub gate_threshold: Decimal,
}

impl Default for CloudBudgetConfig {
    fn default() -> Self {
        let usd_per_cake = std::env::var(CAKE_USD_RATE_ENV)
            .ok()
            .and_then(|raw| raw.trim().parse::<Decimal>().ok())
            .unwrap_or_else(default_usd_per_cake);

        let monthly_cap_cake = std::env::var(CAKE_MONTHLY_CAP_ENV)
            .ok()
            .and_then(|raw| raw.trim().parse::<Decimal>().ok());

        Self {
            usd_per_cake,
            monthly_cap_cake,
            hf_a100_large_rate: default_hf_a100_large_rate(),
            warn_threshold: default_warn_threshold(),
            gate_threshold: default_gate_threshold(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Both tests below mutate process-global env vars; serialize them so
    // cargo test's default parallel execution can't interleave the two.
    static ENV_TEST_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn defaults_match_datum_documented_values() {
        let _lock = ENV_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var(CAKE_USD_RATE_ENV);
        std::env::remove_var(CAKE_MONTHLY_CAP_ENV);
        let cfg = CloudBudgetConfig::default();
        assert_eq!(cfg.usd_per_cake, Decimal::new(1000, 2));
        assert_eq!(cfg.monthly_cap_cake, None);
        assert_eq!(cfg.hf_a100_large_rate, Decimal::new(250, 2));
        assert_eq!(cfg.warn_threshold, Decimal::new(75, 2));
        assert_eq!(cfg.gate_threshold, Decimal::new(90, 2));
    }

    #[test]
    fn env_overrides_are_honored() {
        let _lock = ENV_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var(CAKE_USD_RATE_ENV, "12.50");
        std::env::set_var(CAKE_MONTHLY_CAP_ENV, "40");
        let cfg = CloudBudgetConfig::default();
        assert_eq!(cfg.usd_per_cake, Decimal::new(1250, 2));
        assert_eq!(cfg.monthly_cap_cake, Some(Decimal::new(40, 0)));
        std::env::remove_var(CAKE_USD_RATE_ENV);
        std::env::remove_var(CAKE_MONTHLY_CAP_ENV);
    }
}
