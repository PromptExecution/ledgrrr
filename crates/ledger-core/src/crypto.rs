//! Crypto cost basis types and constraint satisfaction.
//!
//! Implements multi-jurisdiction crypto capital gains treatment:
//! - **AU**: ATO QC 53725 (CGT event timing, 50% discount if held > 12 months)
//! - **US**: Rev. Proc. 2024-28 (wallet-by-wallet or aggregate accounting basis)
//!
//! Cost basis methods: FIFO, HIFO, LIFO, SpecID — applied per-wallet per-jurisdiction.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use ufo_types::{
    satisfies::{Constraint, Disposition, NodeId, SatisfiesResult, Satisfies},
    ufo::MomentStereotype,
    iso::{Currency, Lei},
};

// ─── Constraint ────────────────────────────────────────────────────────────

/// Constraint: crypto cost basis rules for the wallet's jurisdiction.
///
/// Dispatches to AU or US rules depending on `CryptoWallet::jurisdiction`.
pub struct CryptoCostBasisRules;
impl Constraint for CryptoCostBasisRules {}

// ─── Domain types ──────────────────────────────────────────────────────────

/// Blockchain network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Chain {
    Bitcoin,
    Ethereum,
    Solana,
    Ripple,
    Avalanche,
    Polygon,
    Other,
}

impl std::fmt::Display for Chain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Chain::Bitcoin => "Bitcoin", Chain::Ethereum => "Ethereum",
            Chain::Solana => "Solana", Chain::Ripple => "Ripple",
            Chain::Avalanche => "Avalanche", Chain::Polygon => "Polygon",
            Chain::Other => "Other",
        };
        write!(f, "{s}")
    }
}

/// Tax jurisdiction determining which rules apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TaxJurisdiction {
    /// Australia — ATO QC 53725.
    Au,
    /// United States — Rev. Proc. 2024-28.
    Us,
}

/// Cost basis tracking method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CostBasisMethod {
    /// First-In, First-Out — oldest lots disposed first.
    Fifo,
    /// Highest-In, First-Out — highest cost lots disposed first (minimises gain).
    Hifo,
    /// Last-In, First-Out — most recent lots disposed first.
    Lifo,
    /// Specific Identification — taxpayer specifies which lot is being sold.
    SpecId,
}

impl std::fmt::Display for CostBasisMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            CostBasisMethod::Fifo => "FIFO", CostBasisMethod::Hifo => "HIFO",
            CostBasisMethod::Lifo => "LIFO", CostBasisMethod::SpecId => "SpecID",
        };
        write!(f, "{s}")
    }
}

/// A crypto wallet or exchange account.
///
/// UFO stereotype: `Kind` (the wallet is an independently identifiable entity).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryptoWallet {
    /// ISO 17442 LEI of the beneficial owner.
    pub lei: Lei,
    /// On-chain address or exchange account identifier.
    pub address: String,
    /// Blockchain this wallet operates on.
    pub chain: Chain,
    /// Tax jurisdiction that governs gain/loss treatment.
    pub jurisdiction: TaxJurisdiction,
    /// Cost basis method elected for this wallet.
    pub cost_basis_method: CostBasisMethod,
}

/// Type of crypto transaction event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TxType {
    /// Acquisition in exchange for fiat or other crypto.
    Buy,
    /// Disposal in exchange for fiat or other crypto (CGT event).
    Sell,
    /// Staking/yield rewards received (ordinary income in AU/US).
    Staking,
    /// Airdrop or fork tokens received.
    Airdrop,
    /// Payment for goods/services (disposal at FMV — CGT event).
    Spend,
    /// Transfer between own wallets (not a CGT event).
    Transfer,
}

impl TxType {
    /// Whether this event triggers a CGT/capital gains realisation.
    pub fn is_cgt_event(self) -> bool {
        matches!(self, TxType::Sell | TxType::Spend)
    }
}

/// A single crypto transaction.
///
/// UFO stereotype: `Event` (a discrete, timestamped financial event).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryptoTx {
    /// On-chain transaction hash.
    pub tx_hash: String,
    /// Wallet from which this transaction originates.
    pub wallet: CryptoWallet,
    /// Transaction type / CGT event classification.
    pub tx_type: TxType,
    /// Gross proceeds in the wallet's reporting currency.
    pub gross_proceeds: Decimal,
    /// Cost basis of the asset(s) disposed, per the elected method.
    pub cost_basis: Decimal,
    /// Date of the transaction (settlement date for CGT purposes).
    pub date: NaiveDate,
    /// Date the asset was originally acquired (needed for discount eligibility).
    pub acquisition_date: Option<NaiveDate>,
    /// Reporting currency.
    pub currency: Currency,
}

impl CryptoTx {
    /// Capital gain or loss (positive = gain, negative = loss).
    pub fn gain_loss(&self) -> Decimal {
        self.gross_proceeds - self.cost_basis
    }

    /// AU: 50% CGT discount applies if asset held > 12 months (ATO QC 53725).
    pub fn au_discount_eligible(&self) -> bool {
        match self.acquisition_date {
            Some(acq) => {
                let held_days = (self.date - acq).num_days();
                held_days > 365
            }
            None => false,
        }
    }

    /// AU: taxable capital gain after applying the 50% discount where eligible.
    pub fn au_taxable_gain(&self) -> Decimal {
        let gain = self.gain_loss();
        if gain > Decimal::ZERO && self.au_discount_eligible() {
            gain / Decimal::from(2u32)
        } else {
            gain
        }
    }

    fn evidence_node(&self) -> NodeId {
        let hash = blake3::hash(self.tx_hash.as_bytes()).to_hex().to_string();
        NodeId::new(format!("tx:{hash}"))
    }
}

impl Satisfies<CryptoCostBasisRules> for CryptoTx {
    fn satisfies(&self, _constraint: &CryptoCostBasisRules) -> SatisfiesResult {
        let node = self.evidence_node();

        // Transfers are not CGT events — not subject to cost basis rules
        if self.tx_type == TxType::Transfer {
            return SatisfiesResult {
                disposition: Disposition::Satisfied,
                confidence: 1.0,
                evidence_nodes: vec![node],
                ufo_category: MomentStereotype::Mode,
            };
        }

        // Disposals require non-negative cost basis
        if self.tx_type.is_cgt_event() && self.cost_basis < Decimal::ZERO {
            return SatisfiesResult {
                disposition: Disposition::Violated {
                    reason: format!(
                        "Tx {} has negative cost basis {}; disposal events require cost basis ≥ 0",
                        self.tx_hash, self.cost_basis
                    ),
                },
                confidence: 0.0,
                evidence_nodes: vec![node],
                ufo_category: MomentStereotype::Mode,
            };
        }

        // AU: require acquisition_date for CGT discount calculation
        let confidence = match self.wallet.jurisdiction {
            TaxJurisdiction::Au => {
                if self.tx_type.is_cgt_event() && self.acquisition_date.is_none() {
                    return SatisfiesResult {
                        disposition: Disposition::Violated {
                            reason: format!(
                                "AU CGT: tx {} is a disposal but acquisition_date is missing; \
                                 required for ATO QC 53725 50% discount assessment",
                                self.tx_hash
                            ),
                        },
                        confidence: 0.0,
                        evidence_nodes: vec![node],
                        ufo_category: MomentStereotype::Mode,
                    };
                }
                0.92
            }
            TaxJurisdiction::Us => {
                // US: Rev. Proc. 2024-28 — wallet-by-wallet; acquisition_date preferred but not fatal
                if self.tx_type.is_cgt_event() && self.acquisition_date.is_none() {
                    0.75 // lower confidence without holding-period proof
                } else {
                    0.93
                }
            }
        };

        SatisfiesResult {
            disposition: Disposition::Satisfied,
            confidence,
            evidence_nodes: vec![node],
            ufo_category: MomentStereotype::Relator,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use rust_decimal_macros::dec;

    fn test_lei() -> Lei {
        Lei::new("HWUPKR0MPOU8FGXBT394").expect("test lei")
    }

    fn au_wallet() -> CryptoWallet {
        CryptoWallet {
            lei: test_lei(),
            address: "bc1q...".to_string(),
            chain: Chain::Bitcoin,
            jurisdiction: TaxJurisdiction::Au,
            cost_basis_method: CostBasisMethod::Hifo,
        }
    }

    fn btc_sell(gross: Decimal, cost: Decimal, held_days: i64) -> CryptoTx {
        let date = NaiveDate::from_ymd_opt(2026, 3, 15).unwrap();
        let acq_date = date - chrono::Duration::days(held_days);
        CryptoTx {
            tx_hash: "abc123".to_string(),
            wallet: au_wallet(),
            tx_type: TxType::Sell,
            gross_proceeds: gross,
            cost_basis: cost,
            date,
            acquisition_date: Some(acq_date),
            currency: Currency::Aud,
        }
    }

    #[test]
    fn au_cgt_50_percent_discount() {
        // 1.5 BTC: 180k proceeds, 120k cost, held > 12 months → 50% discount
        let tx = btc_sell(dec!(180_000), dec!(120_000), 400);
        assert!(tx.au_discount_eligible());
        assert_eq!(tx.gain_loss(), dec!(60_000));
        assert_eq!(tx.au_taxable_gain(), dec!(30_000));
    }

    #[test]
    fn au_cgt_no_discount_under_12_months() {
        let tx = btc_sell(dec!(180_000), dec!(120_000), 200);
        assert!(!tx.au_discount_eligible());
        assert_eq!(tx.au_taxable_gain(), dec!(60_000));
    }

    #[test]
    fn au_disposal_without_acq_date_violated() {
        let mut tx = btc_sell(dec!(100_000), dec!(80_000), 400);
        tx.acquisition_date = None;
        let result = tx.satisfies(&CryptoCostBasisRules);
        assert!(!result.disposition.is_satisfied());
        if let Disposition::Violated { reason } = &result.disposition {
            assert!(reason.contains("QC 53725"), "expected ATO reference in: {reason}");
        } else {
            panic!("expected Violated");
        }
    }

    #[test]
    fn au_valid_disposal_satisfied() {
        let tx = btc_sell(dec!(180_000), dec!(120_000), 400);
        let result = tx.satisfies(&CryptoCostBasisRules);
        assert!(result.disposition.is_satisfied());
        assert!(result.confidence >= 0.9);
    }

    #[test]
    fn transfer_always_satisfied() {
        let mut tx = btc_sell(dec!(0), dec!(0), 100);
        tx.tx_type = TxType::Transfer;
        let result = tx.satisfies(&CryptoCostBasisRules);
        assert!(result.disposition.is_satisfied());
        assert_eq!(result.confidence, 1.0);
    }

    #[test]
    fn negative_cost_basis_violated() {
        let mut tx = btc_sell(dec!(100_000), dec!(-1), 400);
        let result = tx.satisfies(&CryptoCostBasisRules);
        assert!(!result.disposition.is_satisfied());
    }

    #[test]
    fn us_disposal_without_acq_date_lower_confidence() {
        let mut tx = btc_sell(dec!(100_000), dec!(80_000), 400);
        tx.wallet.jurisdiction = TaxJurisdiction::Us;
        tx.acquisition_date = None;
        let result = tx.satisfies(&CryptoCostBasisRules);
        // US: no acq_date → satisfied but lower confidence
        assert!(result.disposition.is_satisfied());
        assert!(result.confidence < 0.9);
    }

    #[test]
    fn cost_basis_methods_display() {
        assert_eq!(CostBasisMethod::Fifo.to_string(), "FIFO");
        assert_eq!(CostBasisMethod::Hifo.to_string(), "HIFO");
        assert_eq!(CostBasisMethod::SpecId.to_string(), "SpecID");
    }

    #[test]
    fn staking_not_cgt_event() {
        assert!(!TxType::Staking.is_cgt_event());
        assert!(TxType::Sell.is_cgt_event());
        assert!(TxType::Spend.is_cgt_event());
    }
}
