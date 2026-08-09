use ledgerr_mcp::capital_loss::{compute_capital_loss, CapitalLossInput, FilingStatus};

#[test]
fn mfs_limit_is_1500_not_3000() {
    let input = CapitalLossInput {
        tax_year: 2024,
        filing_status: FilingStatus::MarriedFilingSeparately,
        short_term_losses: "5000".to_string(),
        long_term_losses: "0".to_string(),
        short_term_gains: "0".to_string(),
        long_term_gains: "0".to_string(),
        prior_short_term_carryforward: "0".to_string(),
        prior_long_term_carryforward: "0".to_string(),
        nonbusiness_bad_debt: None,
    };
    let result = compute_capital_loss(&input);
    assert_eq!(result.deductible_amount, "1500.00");
    assert_eq!(result.filing_status_used, "married_filing_separately");
}

#[test]
fn single_limit_is_3000() {
    let input = CapitalLossInput {
        tax_year: 2024,
        filing_status: FilingStatus::Single,
        short_term_losses: "10000".to_string(),
        long_term_losses: "0".to_string(),
        short_term_gains: "0".to_string(),
        long_term_gains: "0".to_string(),
        prior_short_term_carryforward: "0".to_string(),
        prior_long_term_carryforward: "0".to_string(),
        nonbusiness_bad_debt: None,
    };
    let result = compute_capital_loss(&input);
    assert_eq!(result.deductible_amount, "3000.00");
    assert_eq!(result.filing_status_used, "single");
}

#[test]
fn carryforward_preserves_st_lt_character() {
    let input = CapitalLossInput {
        tax_year: 2024,
        filing_status: FilingStatus::Single,
        short_term_losses: "2000".to_string(),
        long_term_losses: "4000".to_string(),
        short_term_gains: "0".to_string(),
        long_term_gains: "0".to_string(),
        prior_short_term_carryforward: "0".to_string(),
        prior_long_term_carryforward: "0".to_string(),
        nonbusiness_bad_debt: None,
    };
    let result = compute_capital_loss(&input);
    assert_eq!(result.deductible_amount, "3000.00");
    assert_eq!(result.total_net_loss, "-6000.00");
    let cf_total: f64 = result.carryforward_short_term.parse::<f64>().unwrap()
        + result.carryforward_long_term.parse::<f64>().unwrap();
    assert!((cf_total - 3000.0).abs() < 0.01, "carryforward should be ~3000, got {cf_total}");
    assert!(result.carryforward_short_term.parse::<f64>().unwrap() > 0.0);
    assert!(result.carryforward_long_term.parse::<f64>().unwrap() > 0.0);
}

#[test]
fn nonbusiness_bad_debt_is_short_term() {
    let input = CapitalLossInput {
        tax_year: 2024,
        filing_status: FilingStatus::Single,
        short_term_losses: "0".to_string(),
        long_term_losses: "5000".to_string(),
        short_term_gains: "0".to_string(),
        long_term_gains: "0".to_string(),
        prior_short_term_carryforward: "0".to_string(),
        prior_long_term_carryforward: "0".to_string(),
        nonbusiness_bad_debt: Some("5000".to_string()),
    };
    let result = compute_capital_loss(&input);
    assert_eq!(result.net_short_term, "-5000.00");
    assert_eq!(result.net_long_term, "-5000.00");
    assert_eq!(result.total_net_loss, "-10000.00");
}

#[test]
fn warning_on_carryforward_exceeds_20_years() {
    let input = CapitalLossInput {
        tax_year: 2024,
        filing_status: FilingStatus::Single,
        short_term_losses: "100000".to_string(),
        long_term_losses: "0".to_string(),
        short_term_gains: "0".to_string(),
        long_term_gains: "0".to_string(),
        prior_short_term_carryforward: "0".to_string(),
        prior_long_term_carryforward: "0".to_string(),
        nonbusiness_bad_debt: None,
    };
    let result = compute_capital_loss(&input);
    let has_20yr_warning = result.warnings.iter().any(|w| w.contains("20 years"));
    assert!(has_20yr_warning, "expected warning about 20-year horizon, got: {:?}", result.warnings);
}
