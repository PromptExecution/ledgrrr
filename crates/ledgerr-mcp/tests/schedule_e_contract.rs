use ledgerr_mcp::schedule_e::{compute_depreciation, DepreciationInput};

#[test]
fn sed_01_twentyseven_point_five_year_straight_line_mid_month() {
    let result = compute_depreciation(&DepreciationInput {
        tax_year: 2023,
        placed_in_service: "2023-01-15".to_string(),
        total_basis: "330000.00".to_string(),
        land_value: "0".to_string(),
        improvements: vec![],
        prior_accumulated: "0".to_string(),
    });
    assert_eq!(result.tax_year, 2023);
    assert_eq!(result.depreciable_basis, "330000.00");
    let expected_annual = 330000.00 / 27.5;
    let expected_first_year = expected_annual * 11.5 / 12.0;
    let current: f64 = result.current_year.parse().unwrap();
    let diff = (current - expected_first_year).abs();
    assert!(diff < 0.02, "current_year {current} differs from {expected_first_year} by {diff}");
    assert_eq!(result.accumulated_prior, "0.00");
    assert_eq!(result.accumulated_end, result.current_year);
    assert_eq!(result.remaining_life_months, 318);
}

#[test]
fn sed_02_land_excluded_from_depreciable_basis() {
    let result = compute_depreciation(&DepreciationInput {
        tax_year: 2023,
        placed_in_service: "2023-06-15".to_string(),
        total_basis: "500000.00".to_string(),
        land_value: "100000.00".to_string(),
        improvements: vec![],
        prior_accumulated: "0".to_string(),
    });
    assert_eq!(result.depreciable_basis, "400000.00");
    let expected_depreciation = 400000.00 / 27.5 / 12.0 * 6.5;
    let current: f64 = result.current_year.parse().unwrap();
    let diff = (current - expected_depreciation).abs();
    assert!(diff < 0.02, "current_year {current} differs from {expected_depreciation} by {diff}");
}

#[test]
fn sed_03_cross_year_accumulated_continuity() {
    let year1 = compute_depreciation(&DepreciationInput {
        tax_year: 2023,
        placed_in_service: "2023-01-15".to_string(),
        total_basis: "330000.00".to_string(),
        land_value: "0".to_string(),
        improvements: vec![],
        prior_accumulated: "0".to_string(),
    });
    let acc_end: f64 = year1.accumulated_end.parse().unwrap();
    let year2 = compute_depreciation(&DepreciationInput {
        tax_year: 2024,
        placed_in_service: "2023-01-15".to_string(),
        total_basis: "330000.00".to_string(),
        land_value: "0".to_string(),
        improvements: vec![],
        prior_accumulated: acc_end.to_string(),
    });
    let year2_expected = 330000.00 / 27.5;
    let y2_current: f64 = year2.current_year.parse().unwrap();
    let diff = (y2_current - year2_expected).abs();
    assert!(diff < 0.02, "year 2 current {y2_current} differs from full-year {year2_expected} by {diff}");
    let y2_end: f64 = year2.accumulated_end.parse().unwrap();
    let expected_end = acc_end + y2_current;
    assert!((y2_end - expected_end).abs() < 0.02);
    assert_eq!(year2.remaining_life_months, 306);
}

#[test]
fn sed_04_capital_improvement_mid_chain() {
    let result = compute_depreciation(&DepreciationInput {
        tax_year: 2025,
        placed_in_service: "2023-01-15".to_string(),
        total_basis: "330000.00".to_string(),
        land_value: "0".to_string(),
        improvements: vec![("2024-07-01".to_string(), "60000.00".to_string())],
        prior_accumulated: "12800.00".to_string(),
    });
    assert_eq!(result.depreciable_basis, "390000.00");
    let base_current = 330000.00 / 27.5;
    let imp_current = 60000.00 / 27.5;
    let expected_current = base_current + imp_current;
    let current: f64 = result.current_year.parse().unwrap();
    let diff = (current - expected_current).abs();
    assert!(diff < 0.02, "current_year {current} differs from {expected_current} by {diff}");
    let expected_end = 12800.00 + current;
    let end: f64 = result.accumulated_end.parse().unwrap();
    assert!((end - expected_end).abs() < 0.02);
}
