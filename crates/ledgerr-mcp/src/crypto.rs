use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde_json::{json, Value};

use ledger_core::crypto::{
    Chain, CostBasisMethod, CryptoTx, CryptoWallet, CryptoCostBasisRules, TaxJurisdiction, TxType,
};
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
    cost_basis_method: &str,
    chain: &str,
    address: &str,
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

    let jx = TaxJurisdiction::from_str(jurisdiction);
    let ccy = match currency {
        "USD" => Currency::Usd,
        _ => Currency::Aud,
    };
    let tt = TxType::from_str(tx_type);

    let ch = match Chain::from_str(chain) {
        Some(c) => c,
        None => return json!({ "error": format!("unrecognized chain: {chain}") }),
    };
    let cbm = match CostBasisMethod::from_str(cost_basis_method) {
        Some(m) => m,
        None => {
            return json!({
                "error": format!("unrecognized cost_basis_method: {cost_basis_method}")
            })
        }
    };

    let wallet = CryptoWallet {
        lei,
        address: address.to_string(),
        chain: ch,
        jurisdiction: jx,
        cost_basis_method: cbm,
    };
    let tx = CryptoTx {
        tx_hash: tx_hash.to_string(),
        wallet,
        tx_type: tt,
        gross_proceeds: gross,
        cost_basis: cost,
        date: tx_date,
        acquisition_date: acq_date,
        currency: ccy,
    };

    json!({
        "result": tx.satisfies(&CryptoCostBasisRules),
        "gain_loss": tx.gain_loss(),
        "au_discount_eligible": tx.au_discount_eligible(),
        "au_taxable_gain": tx.au_taxable_gain(),
        "method_used": tx.wallet.cost_basis_method.as_str(),
        "chain": tx.wallet.chain.as_str(),
        "address": tx.wallet.address,
    })
}
