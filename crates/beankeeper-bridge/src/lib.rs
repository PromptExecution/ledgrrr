//! Bridge: OFX bank export transactions → beankeeper double-entry `Entry` pairs.
//!
//! # Accounting convention
//!
//! OFX signs amounts from the *account-holder* perspective:
//! - Positive amount (CREDIT / DirectDeposit etc.) = money **in**
//!   → Debit the asset account, Credit the offset (income) account.
//! - Negative amount (DEBIT / Check / POS etc.) = money **out**
//!   → Credit the asset account, Debit the offset (expense) account.
//!
//! # Design boundary
//!
//! Produces a [`BridgeResult`] with two balanced [`Entry`] values plus a content-addressed `id`.
//! Does NOT build a beankeeper `Transaction`/`JournalEntry` (those require a live `Ledger`).
//! Callers post via: `ledger.add_transaction([result.asset_entry(), result.offset_entry()])`.

use beankeeper::types::{Account, AccountCode, AccountType, Entry, EntryError, Money};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use ofx_rs::aggregates::StatementTransaction;
pub use ofx_rs::types::TransactionType;

/// All errors that can arise during OFX→beankeeper conversion.
#[derive(Debug, Error, Serialize, Deserialize)]
pub enum BridgeError {
    #[error("OFX transaction amount is zero — nothing to post")]
    ZeroAmount,
    #[error("OFX amount cannot be converted to minor units: {0}")]
    AmountConversion(String),
    #[error("beankeeper entry construction failed: {0}")]
    EntryConstruction(String),
    #[error("account code invalid: {0}")]
    AccountCode(String),
}

impl From<EntryError> for BridgeError {
    fn from(e: EntryError) -> Self { BridgeError::EntryConstruction(e.to_string()) }
}
impl From<beankeeper::types::AccountCodeError> for BridgeError {
    fn from(e: beankeeper::types::AccountCodeError) -> Self { BridgeError::AccountCode(e.to_string()) }
}

/// Which direction an offset account flows — serializable substitute for `beankeeper::AccountType`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OffsetKind {
    Revenue,
    Expense,
    Equity,
    Liability,
}

impl From<OffsetKind> for AccountType {
    fn from(k: OffsetKind) -> AccountType {
        match k {
            OffsetKind::Revenue   => AccountType::Revenue,
            OffsetKind::Expense   => AccountType::Expense,
            OffsetKind::Equity    => AccountType::Equity,
            OffsetKind::Liability => AccountType::Liability,
        }
    }
}

/// Account pair configuration — eliminates 6-parameter transposition bugs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversionConfig {
    /// Chart-of-accounts code for the bank asset (e.g. `"1000"`).
    pub asset_code: String,
    /// Human label for the asset account (e.g. `"Checking"`).
    pub asset_name: String,
    /// Chart-of-accounts code for the offset account (e.g. `"4000"` or `"6000"`).
    pub offset_code: String,
    /// Human label for the offset account.
    pub offset_name: String,
    /// Whether the offset is Revenue, Expense, Equity, or Liability.
    pub offset_kind: OffsetKind,
}

/// A pair of balanced double-entry [`Entry`] records derived from one OFX transaction.
///
/// Serializable end-to-end — no DTO wrapper needed — because beankeeper is built
/// with `features = ["serde"]` from PromptExecution/beankeeper-b00t.
///
/// Balance is enforced at construction via `debug_assert`.
#[derive(Debug, Serialize)]
pub struct BridgeResult {
    /// Content-addressed ID: `"ofx:<blake3-hex>"`.
    /// Preimage includes `fit_id` to prevent collision on same-day same-amount duplicates (C1).
    pub id: String,
    asset_entry: Entry,
    offset_entry: Entry,
}

impl BridgeResult {
    fn new(id: String, asset_entry: Entry, offset_entry: Entry) -> Self {
        debug_assert_eq!(
            asset_entry.amount(),
            offset_entry.amount(),
            "double-entry balance violated"
        );
        BridgeResult { id, asset_entry, offset_entry }
    }

    pub fn asset_entry(&self)  -> &Entry { &self.asset_entry }
    pub fn offset_entry(&self) -> &Entry { &self.offset_entry }
}

/// Convert a single [`StatementTransaction`] into a balanced [`BridgeResult`].
pub fn convert(
    txn: &StatementTransaction,
    config: &ConversionConfig,
) -> Result<BridgeResult, BridgeError> {
    let ofx_decimal = txn.amount().as_decimal();

    if ofx_decimal == Decimal::ZERO {
        return Err(BridgeError::ZeroAmount);
    }

    // Decimal → i128 minor units (cents). No f64.
    let cents_decimal = (ofx_decimal * Decimal::new(100, 0)).trunc();
    let cents_i128: i128 = cents_decimal
        .try_into()
        .map_err(|_| BridgeError::AmountConversion(ofx_decimal.to_string()))?;
    let abs_cents = cents_i128.unsigned_abs() as i128;
    let money = Money::usd(abs_cents);

    let asset_account  = Account::new(AccountCode::new(&config.asset_code)?,  &config.asset_name,  AccountType::Asset);
    let offset_account = Account::new(AccountCode::new(&config.offset_code)?, &config.offset_name, config.offset_kind.clone().into());

    // OFX positive = money in → Debit asset / Credit offset.
    let (asset_entry, offset_entry) = if ofx_decimal.is_sign_positive() {
        (Entry::debit(asset_account, money)?, Entry::credit(offset_account, money)?)
    } else {
        (Entry::credit(asset_account, money)?, Entry::debit(offset_account, money)?)
    };

    // C1: fit_id in preimage — prevents collision on same-day same-amount duplicate transactions.
    let preimage = format!(
        "{}:{}:{}:{}:{}:{}",
        config.asset_code, txn.fit_id(), txn.date_posted(),
        ofx_decimal, txn.name().unwrap_or(""), txn.memo().unwrap_or(""),
    );
    let id = format!("ofx:{}", blake3::hash(preimage.as_bytes()).to_hex());

    Ok(BridgeResult::new(id, asset_entry, offset_entry))
}

#[cfg(test)]
mod tests {
    use super::*;
    use beankeeper::types::DebitOrCredit;
    use ofx_rs::aggregates::StatementTransactionBuilder;
    use rust_decimal_macros::dec;

    fn cfg_revenue() -> ConversionConfig {
        ConversionConfig {
            asset_code: "1000".into(), asset_name: "Checking".into(),
            offset_code: "4000".into(), offset_name: "Revenue".into(),
            offset_kind: OffsetKind::Revenue,
        }
    }

    fn cfg_expense() -> ConversionConfig {
        ConversionConfig {
            asset_code: "1000".into(), asset_name: "Checking".into(),
            offset_code: "6000".into(), offset_name: "Rent Expense".into(),
            offset_kind: OffsetKind::Expense,
        }
    }

    fn make_txn(ttype: TransactionType, amount: &str, fit_id: &str, name: Option<&str>, memo: Option<&str>)
        -> StatementTransaction
    {
        let mut b = StatementTransactionBuilder::new()
            .transaction_type(ttype)
            .date_posted("20240115".parse().expect("date"))
            .amount(amount.parse().expect("amount"))
            .fit_id(fit_id.parse().expect("fit_id"));
        if let Some(n) = name { b = b.name(n.to_owned()); }
        if let Some(m) = memo { b = b.memo(m.to_owned()); }
        b.build().expect("build txn")
    }

    #[test]
    fn ofx_credit_maps_to_debit_account_entry() {
        let txn = make_txn(TransactionType::Credit, "1234.56", "FIT001", Some("Salary"), None);
        let r = convert(&txn, &cfg_revenue()).expect("convert");
        assert_eq!(r.asset_entry().direction(), DebitOrCredit::Debit);
        assert_eq!(r.offset_entry().direction(), DebitOrCredit::Credit);
    }

    #[test]
    fn ofx_debit_maps_to_credit_account_entry() {
        let txn = make_txn(TransactionType::Debit, "-500.00", "FIT002", Some("Rent"), Some("Office"));
        let r = convert(&txn, &cfg_expense()).expect("convert");
        assert_eq!(r.asset_entry().direction(), DebitOrCredit::Credit);
        assert_eq!(r.offset_entry().direction(), DebitOrCredit::Debit);
    }

    #[test]
    fn blake3_id_is_deterministic() {
        let txn = make_txn(TransactionType::Credit, "99.99", "FIT003", Some("Payroll"), Some("Net pay"));
        let r1 = convert(&txn, &cfg_revenue()).expect("first");
        let r2 = convert(&txn, &cfg_revenue()).expect("second");
        assert_eq!(r1.id, r2.id);
        assert!(r1.id.starts_with("ofx:"));
    }

    #[test]
    fn different_fit_id_produces_different_id() {
        // C1 regression: same-day same-amount same-vendor must not collide
        let txn_a = make_txn(TransactionType::Credit, "9.99", "FIT-A", Some("Netflix"), None);
        let txn_b = make_txn(TransactionType::Credit, "9.99", "FIT-B", Some("Netflix"), None);
        let ra = convert(&txn_a, &cfg_revenue()).expect("a");
        let rb = convert(&txn_b, &cfg_revenue()).expect("b");
        assert_ne!(ra.id, rb.id, "different fit_id must produce different blake3 ID");
    }

    #[test]
    fn zero_amount_is_error() {
        let txn = make_txn(TransactionType::Other, "0", "FIT004", None, None);
        assert!(matches!(convert(&txn, &cfg_revenue()), Err(BridgeError::ZeroAmount)));
    }

    #[test]
    fn amount_converted_to_minor_units() {
        let txn = make_txn(TransactionType::DirectDeposit, "42.50", "FIT005", Some("Dep"), None);
        let r = convert(&txn, &cfg_revenue()).expect("convert");
        assert_eq!(r.asset_entry().amount(), Money::usd(4250));
    }

    #[test]
    fn entries_are_balanced() {
        let txn = make_txn(TransactionType::Credit, "333.33", "FIT006", Some("Sales"), None);
        let r = convert(&txn, &cfg_revenue()).expect("convert");
        assert_eq!(r.asset_entry().amount(), r.offset_entry().amount());
    }

    #[test]
    fn bridge_result_is_directly_serializable() {
        // No DTO wrapper needed — beankeeper Entry is Serialize via fork feature
        let txn = make_txn(TransactionType::Credit, "100.00", "FIT007", None, None);
        let r = convert(&txn, &cfg_revenue()).expect("convert");
        let json = serde_json::to_string(&r).expect("BridgeResult must serialize directly");
        assert!(json.contains("ofx:"));
        assert!(json.contains("FIT007") || json.contains("1000")); // id preimage traces
    }

    #[test]
    fn dec_macro_sanity() {
        assert_eq!(dec!(42.50) * dec!(100), dec!(4250));
    }
}
