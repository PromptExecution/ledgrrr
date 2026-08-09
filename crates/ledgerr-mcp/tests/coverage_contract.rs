mod common;

use std::collections::BTreeMap;

use ledger_core::ingest::TransactionInput;
use ledgerr_mcp::coverage::{assert_account_coverage, CoverageRequest};

fn make_tx(account_id: &str, date: &str, amount: &str, source_ref: &str) -> TransactionInput {
    TransactionInput {
        account_id: account_id.to_string(),
        date: date.to_string(),
        amount: amount.to_string(),
        description: "test".to_string(),
        source_ref: source_ref.to_string(),
    }
}

#[test]
fn coverage_01_happy_path_all_months_present() {
    let request = CoverageRequest {
        account_ids: vec!["BH-CHK".to_string()],
        tax_year: 2023,
    };
    let mut tx_rows = BTreeMap::new();
    for month in 1..=12 {
        let ym = format!("2023-{:02}-15", month);
        tx_rows.insert(
            format!("tx-{month}"),
            make_tx("BH-CHK", &ym, "100.00", "stmt-01.rkyv"),
        );
    }
    let report = assert_account_coverage(&request, &tx_rows).expect("coverage ok");
    assert!(!report.has_gaps, "expected no gaps");
    assert!(!report.has_duplicates, "expected no duplicates");
    assert_eq!(report.complete.len(), 12);
    assert!(report.gaps.is_empty());
}

#[test]
fn coverage_02_detects_missing_month() {
    let request = CoverageRequest {
        account_ids: vec!["BH-CHK".to_string()],
        tax_year: 2023,
    };
    let mut tx_rows = BTreeMap::new();
    for month in 1..=11 {
        let ym = format!("2023-{:02}-15", month);
        tx_rows.insert(
            format!("tx-{month}"),
            make_tx("BH-CHK", &ym, "100.00", "stmt-01.rkyv"),
        );
    }
    let report = assert_account_coverage(&request, &tx_rows).expect("coverage ok");
    assert!(report.has_gaps, "expected gaps");
    assert_eq!(report.gaps.len(), 1);
    assert!(report.gaps[0].1.contains("12"), "month 12 should be missing");
}

#[test]
fn coverage_03_detects_duplicate_source_refs() {
    let request = CoverageRequest {
        account_ids: vec!["BH-CHK".to_string()],
        tax_year: 2023,
    };
    let mut tx_rows = BTreeMap::new();
    tx_rows.insert(
        "tx-1".to_string(),
        make_tx("BH-CHK", "2023-01-15", "50.00", "stmt-01.rkyv"),
    );
    tx_rows.insert(
        "tx-2".to_string(),
        make_tx("BH-CHK", "2023-01-20", "75.00", "stmt-02.rkyv"),
    );
    let report = assert_account_coverage(&request, &tx_rows).expect("coverage ok");
    assert!(report.has_duplicates, "expected duplicates for january");
    assert!(report.has_gaps, "the other 11 months of 2023 have no coverage");
    assert_eq!(report.gaps.len(), 11);
}

#[test]
fn coverage_04_two_accounts() {
    let request = CoverageRequest {
        account_ids: vec!["BH-CHK".to_string(), "BH-SAV".to_string()],
        tax_year: 2023,
    };
    let mut tx_rows = BTreeMap::new();
    tx_rows.insert(
        "tx-1".to_string(),
        make_tx("BH-CHK", "2023-01-15", "100.00", "chk-01.rkyv"),
    );
    tx_rows.insert(
        "tx-2".to_string(),
        make_tx("BH-SAV", "2023-01-20", "200.00", "sav-01.rkyv"),
    );
    let report = assert_account_coverage(&request, &tx_rows).expect("coverage ok");
    assert!(report.has_gaps);
    assert_eq!(report.gaps.len(), 22);
    assert_eq!(report.complete.len(), 2);
}

#[test]
fn coverage_05_no_tx_rows_returns_all_gaps() {
    let request = CoverageRequest {
        account_ids: vec!["BH-CHK".to_string()],
        tax_year: 2023,
    };
    let tx_rows = BTreeMap::new();
    let report = assert_account_coverage(&request, &tx_rows).expect("coverage ok");
    assert!(report.has_gaps);
    assert_eq!(report.gaps.len(), 12);
    assert!(report.complete.is_empty());
}
