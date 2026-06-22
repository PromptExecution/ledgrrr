//! Thin MCP handlers for crypto cost basis rules (gh#516).

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde_json::{json, Value};

use ledger_core::crypto::{Chain, CostBasisMethod, CryptoTx, CryptoWallet, CryptoCostBasisRules, TaxJurisdiction, TxType};
use ufo_types::{iso::{Currency, Lei}, satisfies::Satisfies};

pub fn handle_crypto_cost_basis_check(
    lei: &str,
    tx_hash: &str,
    tx_type: &str,
    gross_proceeds: &str,
    cost_basis: &str,
    date: &str,
    acquisition_date: Option<&str>,
    jurisdiction: &str,
    currency: &str,
) -> Value {
    let lei = match Lei::new(lei) {
        Ok(l) => l,
        Err(e) => return json!({ "error": e.to_string() }),
    };
    let gross = match Decimal::from_str_exact(gross_proceeds) {
        Ok(d) => d, Err(e) => return json!({ "error": format!("gross_proceeds: {e}") }),
    };
    let cost = match Decimal::from_str_exact(cost_basis) {
        Ok(d) => d, Err(e) => return json!({ "error": format!("cost_basis: {e}") }),
    };
    let tx_date = match NaiveDate::parse_from_str(date, "%Y-%m-%d") {
        Ok(d) => d, Err(e) => return json!({ "error": format!("date: {e}") }),
    };
    let acq_date = acquisition_date.and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

    let jx = match jurisdiction {
        "US" | "us" => TaxJurisdiction::Us,
        _ => TaxJurisdiction::Au,
    };
    let ccy = match currency {
        "USD" => Currency::Usd,
        _ => Currency::Aud,
    };
    let tt = match tx_type {
        "buy" => TxType::Buy, "staking" => TxType::Staking,
        "airdrop" => TxType::Airdrop, "spend" => TxType::Spend,
        "transfer" => TxType::Transfer,
        _ => TxType::Sell,
    };

    let wallet = CryptoWallet {
        lei, address: String::new(), chain: Chain::Bitcoin,
        jurisdiction: jx, cost_basis_method: CostBasisMethod::Fifo,
    };
    let tx = CryptoTx {
        tx_hash: tx_hash.to_string(), wallet, tx_type: tt,
        gross_proceeds: gross, cost_basis: cost,
        date: tx_date, acquisition_date: acq_date, currency: ccy,
    };

    json!({
        "result": tx.satisfies(&CryptoCostBasisRules),
        "gain_loss": tx.gain_loss(),
        "au_discount_eligible": tx.au_discount_eligible(),
        "au_taxable_gain": tx.au_taxable_gain(),
    })
}
