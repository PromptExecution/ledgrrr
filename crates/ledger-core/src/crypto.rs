use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use ufo_types::{
    iso::{Currency, Lei},
    satisfies::{Constraint, Satisfies, SatisfiesResult},
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Chain {
    Bitcoin,
    Ethereum,
    Solana,
    Cardano,
    Polkadot,
    Avalanche,
    Polygon,
    Arbitrum,
    Optimism,
    Bsc,
    Other(String),
}

impl Chain {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "bitcoin" | "btc" => Some(Self::Bitcoin),
            "ethereum" | "eth" => Some(Self::Ethereum),
            "solana" | "sol" => Some(Self::Solana),
            "cardano" | "ada" => Some(Self::Cardano),
            "polkadot" | "dot" => Some(Self::Polkadot),
            "avalanche" | "avax" => Some(Self::Avalanche),
            "polygon" | "matic" => Some(Self::Polygon),
            "arbitrum" | "arb" => Some(Self::Arbitrum),
            "optimism" | "op" => Some(Self::Optimism),
            "bsc" | "bnb" => Some(Self::Bsc),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Bitcoin => "Bitcoin",
            Self::Ethereum => "Ethereum",
            Self::Solana => "Solana",
            Self::Cardano => "Cardano",
            Self::Polkadot => "Polkadot",
            Self::Avalanche => "Avalanche",
            Self::Polygon => "Polygon",
            Self::Arbitrum => "Arbitrum",
            Self::Optimism => "Optimism",
            Self::Bsc => "BSC",
            Self::Other(s) => s.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CostBasisMethod {
    Fifo,
    Lifo,
    Hifo,
    Acb,
    SpecificIdentification { lot_refs: Vec<String> },
}

impl CostBasisMethod {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "fifo" => Some(Self::Fifo),
            "lifo" => Some(Self::Lifo),
            "hifo" => Some(Self::Hifo),
            "acb" => Some(Self::Acb),
            "specific_id" | "specific_identification" => {
                Some(Self::SpecificIdentification { lot_refs: vec![] })
            }
            _ => None,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Fifo => "FIFO",
            Self::Lifo => "LIFO",
            Self::Hifo => "HIFO",
            Self::Acb => "ACB",
            Self::SpecificIdentification { .. } => "SpecificIdentification",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TaxJurisdiction {
    Us,
    Au,
}

impl TaxJurisdiction {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "us" => Self::Us,
            _ => Self::Au,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Us => "US",
            Self::Au => "AU",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TxType {
    Buy,
    Sell,
    Staking,
    Airdrop,
    Spend,
    Transfer,
}

impl TxType {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "buy" => Self::Buy,
            "staking" => Self::Staking,
            "airdrop" => Self::Airdrop,
            "spend" => Self::Spend,
            "transfer" => Self::Transfer,
            _ => Self::Sell,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CryptoWallet {
    pub lei: Lei,
    pub address: String,
    pub chain: Chain,
    pub jurisdiction: TaxJurisdiction,
    pub cost_basis_method: CostBasisMethod,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CryptoTx {
    pub tx_hash: String,
    pub wallet: CryptoWallet,
    pub tx_type: TxType,
    pub gross_proceeds: Decimal,
    pub cost_basis: Decimal,
    pub date: chrono::NaiveDate,
    pub acquisition_date: Option<chrono::NaiveDate>,
    pub currency: Currency,
}

impl CryptoTx {
    pub fn gain_loss(&self) -> Decimal {
        self.gross_proceeds - self.cost_basis
    }

    pub fn au_discount_eligible(&self) -> bool {
        match self.acquisition_date {
            Some(acq) => (self.date - acq).num_days() > 365,
            None => false,
        }
    }

    pub fn au_taxable_gain(&self) -> Decimal {
        if self.au_discount_eligible() {
            self.gain_loss() * Decimal::new(5, 1)
        } else {
            self.gain_loss()
        }
    }
}

pub struct CryptoCostBasisRules;

impl Constraint for CryptoCostBasisRules {}

impl Satisfies<CryptoCostBasisRules> for CryptoTx {
    fn satisfies(&self, _rules: &CryptoCostBasisRules) -> SatisfiesResult {
        match &self.wallet.cost_basis_method {
            CostBasisMethod::SpecificIdentification { lot_refs } => {
                if lot_refs.is_empty() {
                    SatisfiesResult::violated(
                        "SpecificIdentification requires at least one lot reference",
                    )
                } else {
                    SatisfiesResult::satisfied(0.95, vec![])
                }
            }
            _ => SatisfiesResult::satisfied(1.0, vec![]),
        }
    }
}
