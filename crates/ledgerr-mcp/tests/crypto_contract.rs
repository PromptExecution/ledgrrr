mod common;

fn check(
    cost_basis_method: &str,
    chain: &str,
    address: &str,
) -> serde_json::Value {
    ledgerr_mcp::crypto::handle_crypto_cost_basis_check(
        "529900T8BMF4KJ2H7K41",
        "0xabc123",
        "sell",
        "10000.00",
        "6000.00",
        "2024-06-15",
        Some("2023-01-10"),
        "US",
        "USD",
        cost_basis_method,
        chain,
        address,
    )
}

#[test]
fn fifo_btc_no_address() {
    let v = check("fifo", "btc", "");
    assert!(v["error"].is_null(), "unexpected error: {:?}", v["error"]);
    assert_eq!(v["method_used"], "FIFO");
    assert_eq!(v["chain"], "Bitcoin");
    assert_eq!(v["address"], "");
    assert_eq!(v["gain_loss"], 4000.0);
}

#[test]
fn hifo_eth_with_address() {
    let v = check("hifo", "eth", "0xdeadbeef");
    assert!(v["error"].is_null(), "unexpected error: {:?}", v["error"]);
    assert_eq!(v["method_used"], "HIFO");
    assert_eq!(v["chain"], "Ethereum");
    assert_eq!(v["address"], "0xdeadbeef");
}

#[test]
fn lifo_solana_address() {
    let v = check("lifo", "sol", "Sol111111111111111111111111111111111111111");
    assert!(v["error"].is_null());
    assert_eq!(v["method_used"], "LIFO");
    assert_eq!(v["chain"], "Solana");
}

#[test]
fn specific_id_without_lots_fails() {
    let v = check("specific_identification", "btc", "bc1abc");
    assert!(v["error"].is_null(), "unexpected error: {:?}", v["error"]);
    assert_eq!(v["method_used"], "SpecificIdentification");
    assert_eq!(
        v["result"]["disposition"]["reason"],
        "SpecificIdentification requires at least one lot reference"
    );
}

#[test]
fn unrecognized_method_returns_error() {
    let v = check("bogus", "btc", "");
    assert!(!v["error"].is_null());
    assert!(v["error"].as_str().unwrap().contains("bogus"));
}

#[test]
fn unrecognized_chain_returns_error() {
    let v = check("fifo", "nonexistent_chain", "");
    assert!(!v["error"].is_null());
    assert!(v["error"].as_str().unwrap().contains("nonexistent_chain"));
}

#[test]
fn au_discount_eligible_long_hold() {
    let v = ledgerr_mcp::crypto::handle_crypto_cost_basis_check(
        "529900T8BMF4KJ2H7K41",
        "0xdef",
        "sell",
        "20000.00",
        "5000.00",
        "2024-06-15",
        Some("2020-01-01"),
        "AU",
        "AUD",
        "fifo",
        "eth",
        "0xaaaa",
    );
    assert!(v["error"].is_null());
    assert!(v["au_discount_eligible"].as_bool().unwrap_or(false));
    assert!(v["au_taxable_gain"].as_f64().unwrap_or(0.0) < 15000.0);
}

#[test]
fn au_no_discount_short_hold() {
    let v = ledgerr_mcp::crypto::handle_crypto_cost_basis_check(
        "529900T8BMF4KJ2H7K41",
        "0xdef",
        "sell",
        "20000.00",
        "5000.00",
        "2024-06-15",
        Some("2024-06-01"),
        "AU",
        "AUD",
        "fifo",
        "eth",
        "0xbbbb",
    );
    assert!(v["error"].is_null());
    assert!(!v["au_discount_eligible"].as_bool().unwrap_or(true));
    assert_eq!(v["au_taxable_gain"].as_f64().unwrap_or(0.0), 15000.0);
}
