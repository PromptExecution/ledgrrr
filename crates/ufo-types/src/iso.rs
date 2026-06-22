//! ISO standard identifier types used across the tax-lawyer platform.
//!
//! - LEI: ISO 17442 — Legal Entity Identifier (20-char alphanumeric + Luhn mod-97 check)
//! - ISIN: ISO 6166 — International Securities Identification Number
//! - Currency: ISO 4217 fiat codes + common crypto tickers
//! - BankAccount: ISO 13616 IBAN / ISO 9362 BIC / ISO 17442 LEI bundle
//! - FinancialInstrument: IFRS 9 measurement categories

use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum IsoValidationError {
    #[error("LEI check digit invalid: got {got}, expected {expected}")]
    LeiCheckDigit { got: u8, expected: u8 },
    #[error("LEI must be exactly 20 alphanumeric characters, got {0}")]
    LeiLength(usize),
    #[error("ISIN must be exactly 12 alphanumeric characters, got {0}")]
    IsinLength(usize),
    #[error("ISIN Luhn check failed")]
    IsinLuhn,
}

/// ISO 17442 Legal Entity Identifier.
///
/// 20-character alphanumeric code. Characters 19–20 are a mod-97 check pair.
/// Validation uses the same algorithm as IBAN (ISO 13616 §5).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct Lei(String);

impl Lei {
    pub fn new(s: impl Into<String>) -> Result<Self, IsoValidationError> {
        let s = s.into().to_ascii_uppercase();
        if s.len() != 20 || !s.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(IsoValidationError::LeiLength(s.len()));
        }
        // Mod-97 check: move last 4 chars to front, convert A→10..Z→35, check % 97 == 1
        let rearranged = format!("{}{}", &s[..18], &s[18..]);
        if mod97(&rearranged) != 1 {
            let check = (98 - mod97(&s[..18].to_string())) % 97;
            return Err(IsoValidationError::LeiCheckDigit {
                got: s[18..].parse::<u8>().unwrap_or(0),
                expected: check as u8,
            });
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Lei {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// ISO 6166 International Securities Identification Number.
///
/// 12 alphanumeric characters; last character is a Luhn check digit.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct Isin(String);

impl Isin {
    pub fn new(s: impl Into<String>) -> Result<Self, IsoValidationError> {
        let s = s.into().to_ascii_uppercase();
        if s.len() != 12 || !s.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(IsoValidationError::IsinLength(s.len()));
        }
        if !luhn_isin_check(&s) {
            return Err(IsoValidationError::IsinLuhn);
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Isin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// ISO 4217 currency codes plus common crypto tickers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum Currency {
    // ISO 4217 fiat
    Aud,
    Usd,
    Eur,
    Gbp,
    Jpy,
    Cad,
    Chf,
    Nzd,
    Sgd,
    Hkd,
    // Crypto (de-facto tickers)
    Btc,
    Eth,
    Usdt,
    Usdc,
    Sol,
    Xrp,
}

impl Currency {
    /// ISO 4217 numeric code for fiat currencies; None for crypto.
    pub fn numeric_code(self) -> Option<u16> {
        match self {
            Currency::Aud => Some(36),
            Currency::Usd => Some(840),
            Currency::Eur => Some(978),
            Currency::Gbp => Some(826),
            Currency::Jpy => Some(392),
            Currency::Cad => Some(124),
            Currency::Chf => Some(756),
            Currency::Nzd => Some(554),
            Currency::Sgd => Some(702),
            Currency::Hkd => Some(344),
            _ => None,
        }
    }

    pub fn is_crypto(self) -> bool {
        self.numeric_code().is_none()
    }
}

impl std::fmt::Display for Currency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Currency::Aud => "AUD", Currency::Usd => "USD", Currency::Eur => "EUR",
            Currency::Gbp => "GBP", Currency::Jpy => "JPY", Currency::Cad => "CAD",
            Currency::Chf => "CHF", Currency::Nzd => "NZD", Currency::Sgd => "SGD",
            Currency::Hkd => "HKD", Currency::Btc => "BTC", Currency::Eth => "ETH",
            Currency::Usdt => "USDT", Currency::Usdc => "USDC", Currency::Sol => "SOL",
            Currency::Xrp => "XRP",
        };
        write!(f, "{s}")
    }
}

/// ISO 13616 IBAN / ISO 9362 BIC / ISO 17442 LEI bundle.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BankAccount {
    /// ISO 13616 International Bank Account Number.
    pub iban: String,
    /// ISO 9362 Bank Identifier Code (BIC/SWIFT).
    pub bic: String,
    /// ISO 17442 Legal Entity Identifier of the account-holding institution.
    pub lei: Option<Lei>,
}

/// IFRS 9 financial instrument measurement categories.
///
/// Determines how fair value changes flow through P&L vs OCI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FinancialInstrument {
    /// Fair Value through Profit or Loss — default for most trading assets.
    Fvpl,
    /// Fair Value through Other Comprehensive Income — for equity investments elected at inception.
    Fvoci,
    /// Amortized Cost — for assets held-to-collect contractual cash flows.
    AmortizedCost,
}

impl std::fmt::Display for FinancialInstrument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            FinancialInstrument::Fvpl => "FVPL",
            FinancialInstrument::Fvoci => "FVOCI",
            FinancialInstrument::AmortizedCost => "Amortized Cost",
        };
        write!(f, "{s}")
    }
}

// ─── private helpers ───────────────────────────────────────────────────────

fn mod97(s: &str) -> u64 {
    let mut acc: u64 = 0;
    for c in s.chars() {
        let digit = if c.is_ascii_digit() {
            c as u64 - '0' as u64
        } else if c.is_ascii_uppercase() {
            c as u64 - 'A' as u64 + 10
        } else {
            continue;
        };
        if digit >= 10 {
            acc = (acc * 100 + digit) % 97;
        } else {
            acc = (acc * 10 + digit) % 97;
        }
    }
    acc
}

fn luhn_isin_check(isin: &str) -> bool {
    // Expand alphanumeric ISIN to digit string (A→10, B→11, …, Z→35)
    let digits: String = isin.chars().flat_map(|c| {
        if c.is_ascii_digit() {
            vec![c]
        } else {
            let n = c as u32 - 'A' as u32 + 10;
            format!("{n}").chars().collect()
        }
    }).collect();

    let sum: u32 = digits.chars().rev().enumerate().map(|(i, c)| {
        let d = c.to_digit(10).unwrap_or(0);
        if i % 2 == 0 { d } else { let v = d * 2; if v > 9 { v - 9 } else { v } }
    }).sum();
    sum % 10 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lei_valid() {
        // Well-known LEI for Apple Inc.
        let lei = Lei::new("HWUPKR0MPOU8FGXBT394");
        assert!(lei.is_ok(), "expected valid LEI");
    }

    #[test]
    fn lei_wrong_length() {
        assert!(matches!(Lei::new("SHORT"), Err(IsoValidationError::LeiLength(_))));
    }

    #[test]
    fn isin_valid() {
        // US Apple Inc. ISIN
        assert!(Isin::new("US0378331005").is_ok());
    }

    #[test]
    fn isin_wrong_length() {
        assert!(matches!(Isin::new("US037"), Err(IsoValidationError::IsinLength(_))));
    }

    #[test]
    fn currency_crypto_flag() {
        assert!(Currency::Btc.is_crypto());
        assert!(!Currency::Aud.is_crypto());
        assert_eq!(Currency::Aud.numeric_code(), Some(36));
    }

    #[test]
    fn currency_display() {
        assert_eq!(Currency::Eth.to_string(), "ETH");
        assert_eq!(Currency::Eur.to_string(), "EUR");
    }

    #[test]
    fn financial_instrument_roundtrip() {
        let fi = FinancialInstrument::Fvoci;
        let json = serde_json::to_string(&fi).unwrap();
        let back: FinancialInstrument = serde_json::from_str(&json).unwrap();
        assert_eq!(fi, back);
    }
}
