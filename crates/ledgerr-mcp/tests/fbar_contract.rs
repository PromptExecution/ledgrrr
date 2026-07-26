use ledgerr_mcp::fbar::{compute_fbar, DailyBalance, FbarInput, ForeignAccountInput};

fn single_status_input() -> FbarInput {
    FbarInput {
        tax_year: 2024,
        filing_status: "single".to_string(),
        living_abroad: false,
        accounts: vec![],
    }
}

#[test]
fn mid_year_spike_exceeds_threshold() {
    let input = FbarInput {
        accounts: vec![ForeignAccountInput {
            account_id: "CHK-EUR".to_string(),
            institution: "Deutsche Bank".to_string(),
            country: "DE".to_string(),
            currency: "EUR".to_string(),
            daily_balances: vec![
                DailyBalance {
                    date: "2024-01-01".to_string(),
                    balance: "1000".to_string(),
                },
                DailyBalance {
                    date: "2024-06-15".to_string(),
                    balance: "25000".to_string(),
                },
                DailyBalance {
                    date: "2024-12-31".to_string(),
                    balance: "500".to_string(),
                },
            ],
            year_end_rate: Some("1.05".to_string()),
        }],
        ..single_status_input()
    };

    let result = compute_fbar(&input);

    assert!(!result.incomplete_accounts.contains(&"CHK-EUR".to_string()));
    assert_eq!(result.accounts[0].max_balance_native, "25000");
    assert_eq!(result.accounts[0].max_balance_date, "2024-06-15");
    let max_usd: f64 = result.accounts[0].max_balance_usd.parse().unwrap();
    assert!((max_usd - 26250.0).abs() < 0.01);
    assert!(result.filing_required);
}

#[test]
fn aggregate_threshold_multiple_accounts() {
    let input = FbarInput {
        accounts: vec![
            ForeignAccountInput {
                account_id: "CHK-EUR".to_string(),
                institution: "Deutsche Bank".to_string(),
                country: "DE".to_string(),
                currency: "EUR".to_string(),
                daily_balances: vec![DailyBalance {
                    date: "2024-12-31".to_string(),
                    balance: "3000".to_string(),
                }],
                year_end_rate: Some("1.05".to_string()),
            },
            ForeignAccountInput {
                account_id: "SAV-JPY".to_string(),
                institution: "Mitsubishi UFJ".to_string(),
                country: "JP".to_string(),
                currency: "JPY".to_string(),
                daily_balances: vec![DailyBalance {
                    date: "2024-12-31".to_string(),
                    balance: "500000".to_string(),
                }],
                year_end_rate: Some("0.0067".to_string()),
            },
        ],
        ..single_status_input()
    };

    let result = compute_fbar(&input);

    assert_eq!(result.accounts.len(), 2);
    let total_usd: f64 = result.aggregate_max_usd.parse().unwrap();
    let expected_eur = 3000.0 * 1.05;
    let expected_jpy = 500000.0 * 0.0067;
    assert!((total_usd - (expected_eur + expected_jpy)).abs() < 0.01);
    assert!(result.filing_required);
}

#[test]
fn incomplete_data_returns_false_filing_required() {
    let input = FbarInput {
        accounts: vec![
            ForeignAccountInput {
                account_id: "CHK-EUR".to_string(),
                institution: "Deutsche Bank".to_string(),
                country: "DE".to_string(),
                currency: "EUR".to_string(),
                daily_balances: vec![],
                year_end_rate: Some("1.05".to_string()),
            },
            ForeignAccountInput {
                account_id: "SAV-JPY".to_string(),
                institution: "Mitsubishi UFJ".to_string(),
                country: "JP".to_string(),
                currency: "JPY".to_string(),
                daily_balances: vec![DailyBalance {
                    date: "2024-12-31".to_string(),
                    balance: "500".to_string(),
                }],
                year_end_rate: None,
            },
        ],
        ..single_status_input()
    };

    let result = compute_fbar(&input);

    assert_eq!(result.accounts.len(), 0);
    assert!(!result.filing_required);
    assert_eq!(result.incomplete_accounts.len(), 2);
    assert!(result.incomplete_accounts.contains(&"CHK-EUR".to_string()));
    assert!(result.incomplete_accounts.contains(&"SAV-JPY".to_string()));
    assert_eq!(result.form_8938_filing_required, None);
}

#[test]
fn form_8938_mfs_living_abroad_threshold() {
    let input = FbarInput {
        filing_status: "mfs".to_string(),
        living_abroad: true,
        accounts: vec![ForeignAccountInput {
            account_id: "CHK-EUR".to_string(),
            institution: "Deutsche Bank".to_string(),
            country: "DE".to_string(),
            currency: "EUR".to_string(),
            daily_balances: vec![DailyBalance {
                date: "2024-12-31".to_string(),
                balance: "250000".to_string(),
            }],
            year_end_rate: Some("1.00".to_string()),
        }],
        ..single_status_input()
    };

    let result = compute_fbar(&input);

    assert!(result.filing_required);
    assert_eq!(result.form_8938_filing_required, Some(true));
    assert_eq!(
        result.form_8938_threshold_used,
        Some("$200,000/$400,000".to_string())
    );
}
