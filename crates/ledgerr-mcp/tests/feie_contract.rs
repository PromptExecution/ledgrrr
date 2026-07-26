use ledgerr_mcp::{
    feie::{compute_feie, FeieInput, ForeignResidenceTest},
    FeieOutcome,
};

#[test]
fn feie_01_full_year_excludes_correctly() {
    let outcome = compute_feie(&FeieInput {
        tax_year: 2024,
        test: ForeignResidenceTest::BonaFideResidence {
            start: "2024-01-01".to_string(),
            end: None,
        },
        foreign_earned_income: "150000".to_string(),
        days_qualified: 365,
        housing_exclusion: None,
    });

    assert_eq!(outcome.excluded_amount, "126500");
    assert_eq!(outcome.income_subject_to_income_tax, "23500");
    assert_eq!(outcome.income_subject_to_se_tax, "150000");
    assert!(outcome.warnings.iter().any(|w| w.contains("SE tax")));
}

#[test]
fn feie_02_se_tax_not_reduced_by_exclusion() {
    let outcome = compute_feie(&FeieInput {
        tax_year: 2024,
        test: ForeignResidenceTest::PhysicalPresence {
            qualifying_days: 330,
            window: ("2024-01-01".to_string(), "2024-12-31".to_string()),
        },
        foreign_earned_income: "100000".to_string(),
        days_qualified: 365,
        housing_exclusion: None,
    });

    // Full exclusion since income < limit
    assert_eq!(outcome.excluded_amount, "100000");
    assert_eq!(outcome.income_subject_to_income_tax, "0");

    // HARD GUARD: SE tax base is NOT reduced by FEIE
    assert_eq!(outcome.income_subject_to_se_tax, "100000");
    assert!(outcome.warnings.iter().any(|w| w.contains("SE tax")));
}

#[test]
fn feie_03_partial_year_pro_rata() {
    let outcome = compute_feie(&FeieInput {
        tax_year: 2024,
        test: ForeignResidenceTest::BonaFideResidence {
            start: "2024-07-01".to_string(),
            end: None,
        },
        foreign_earned_income: "80000".to_string(),
        days_qualified: 184,
        housing_exclusion: None,
    });

    // 184/365 * 126500 with Decimal precision
    assert_eq!(outcome.excluded_amount, "63769.86301369863013698630137");
    assert_eq!(outcome.income_subject_to_income_tax, "16230.13698630136986301369863");
    // Hard guard still applies
    assert_eq!(outcome.income_subject_to_se_tax, "80000");
}
