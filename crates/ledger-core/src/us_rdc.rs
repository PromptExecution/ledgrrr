//! Core domain types for the IRC §41 four-part qualified-research test.

use serde::{Deserialize, Serialize};
use ufo_types::{
    iso::Lei,
    satisfies::{Constraint, Satisfies, SatisfiesResult},
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QreActivity {
    pub lei: Lei,
    pub activity_id: String,
    pub activity_name: String,
    pub technical_in_nature: bool,
    pub permits_experimentation: bool,
    pub technological_uncertainty: bool,
    pub systematic_process: bool,
}

pub struct UsRdcFourPartTest;

impl Constraint for UsRdcFourPartTest {}

impl Satisfies<UsRdcFourPartTest> for QreActivity {
    fn satisfies(&self, _constraint: &UsRdcFourPartTest) -> SatisfiesResult {
        let mut missing = Vec::new();
        if !self.technical_in_nature {
            missing.push("technical_in_nature");
        }
        if !self.permits_experimentation {
            missing.push("permits_experimentation");
        }
        if !self.technological_uncertainty {
            missing.push("technological_uncertainty");
        }
        if !self.systematic_process {
            missing.push("systematic_process");
        }

        if missing.is_empty() {
            SatisfiesResult::satisfied(1.0, vec![])
        } else {
            SatisfiesResult::violated(format!(
                "IRC §41 four-part test not satisfied: {}",
                missing.join(", ")
            ))
        }
    }
}

pub struct UsRdcCredit;

#[cfg(test)]
mod tests {
    use super::*;
    use ufo_types::satisfies::Disposition;

    fn activity() -> QreActivity {
        QreActivity {
            lei: Lei::new("5493001KJTIIGC8Y1R12").expect("valid LEI"),
            activity_id: "synthetic-rdc-1".to_string(),
            activity_name: "Synthetic research activity".to_string(),
            technical_in_nature: true,
            permits_experimentation: true,
            technological_uncertainty: true,
            systematic_process: true,
        }
    }

    #[test]
    fn all_four_elements_are_required() {
        let mut candidate = activity();
        assert!(matches!(
            candidate.satisfies(&UsRdcFourPartTest).disposition,
            Disposition::Satisfied
        ));

        candidate.systematic_process = false;
        assert!(matches!(
            candidate.satisfies(&UsRdcFourPartTest).disposition,
            Disposition::Violated { .. }
        ));
    }
}
