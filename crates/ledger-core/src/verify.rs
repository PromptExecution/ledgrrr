//! Multi-model verification for rule repair.
//! Implements the proposer/reviewer loop from the design document.
//!
//! Flow:
//! 1. Proposer model suggests a fix to validation issues
//! 2. Reviewer model evaluates the proposal
//! 3. If reviewer agrees, present to tray for human approval
//! 4. If approved, apply fix and retry pipeline

use serde::{Deserialize, Serialize};

// ModelClient / MockModelClient / MultiModelConfig are generic orchestration —
// canonical home is ufo-types (per this project's consolidation precedent).
pub use ufo_types::{MockModelClient, ModelClient, MultiModelConfig};

/// Result from the proposer model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairProposal {
    pub rule_id: String,
    pub proposed_fix: String,
    pub reasoning: String,
    pub confidence: f32,
}

/// Result from the reviewer model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewResult {
    pub approved: bool,
    pub concerns: Vec<String>,
    pub suggestions: Vec<String>,
    pub confidence: f32,
}

/// Multi-model verifier coordinator.
/// Wraps `ufo_types::MultiModelVerifier` and adds domain-specific prompt
/// construction for rule-repair proposals and reviews.
pub struct MultiModelVerifier<C: ModelClient> {
    inner: ufo_types::MultiModelVerifier<C>,
}

impl<C: ModelClient> MultiModelVerifier<C> {
    pub fn new(proposer: C, reviewer: C, config: MultiModelConfig) -> Self {
        Self {
            inner: ufo_types::MultiModelVerifier::new(proposer, reviewer, config),
        }
    }

    /// Propose a fix for validation issues.
    pub fn propose_fix(
        &self,
        rule_id: &str,
        issues_json: &str,
        context: &str,
    ) -> anyhow::Result<RepairProposal> {
        let prompt = format!(
            "Given these validation issues:\n{}\n\nContext: {}\n\nPropose a fix for rule {}. Return JSON: {{\"rule_id\": \"{}\", \"proposed_fix\": \"...\", \"reasoning\": \"...\", \"confidence\": 0.0-1.0}}",
            issues_json, context, rule_id, rule_id
        );
        self.inner.propose::<RepairProposal>(&prompt)
    }

    /// Review a proposed fix.
    pub fn review(&self, proposal: &RepairProposal) -> anyhow::Result<ReviewResult> {
        let prompt = format!(
            "Review this proposed fix:\nRule: {}\nFix: {}\nReasoning: {}\nConfidence: {}\n\nReturn JSON: {{\"approved\": bool, \"concerns\": [], \"suggestions\": [], \"confidence\": 0.0-1.0}}",
            proposal.rule_id, proposal.proposed_fix, proposal.reasoning, proposal.confidence
        );
        let result = self.inner.review_raw::<ReviewResult>(&prompt)?;
        if !self.inner.meets_confidence_threshold(result.confidence) {
            return Ok(ReviewResult {
                approved: false,
                concerns: vec![format!(
                    "Reviewer confidence {} below threshold {}",
                    result.confidence,
                    self.inner.config().min_reviewer_confidence
                )],
                ..result
            });
        }
        Ok(result)
    }

    /// Full verification loop: propose -> review -> decide.
    pub fn verify(
        &self,
        rule_id: &str,
        issues_json: &str,
        context: &str,
    ) -> anyhow::Result<VerificationOutcome> {
        let proposal = self.propose_fix(rule_id, issues_json, context)?;
        let review = self.review(&proposal)?;
        let outcome = if review.approved && self.inner.meets_confidence_threshold(review.confidence)
        {
            VerificationOutcome::Approved { proposal, review }
        } else {
            VerificationOutcome::Rejected { proposal, review }
        };
        Ok(outcome)
    }
}

/// Outcome of the verification loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VerificationOutcome {
    Approved {
        proposal: RepairProposal,
        review: ReviewResult,
    },
    Rejected {
        proposal: RepairProposal,
        review: ReviewResult,
    },
}

impl VerificationOutcome {
    pub fn is_approved(&self) -> bool {
        matches!(self, Self::Approved { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_proposer() {
        let json = r#"{"rule_id":"test","proposed_fix":"fix content","reasoning":"because","confidence":0.85}"#;
        let mock = MockModelClient::default().with_response(json);

        let result: RepairProposal = mock.extract("prompt").unwrap();

        assert_eq!(result.rule_id, "test");
        assert_eq!(result.confidence, 0.85);
    }

    #[test]
    fn test_mock_reviewer_approved() {
        let json = r#"{"approved":true,"concerns":[],"suggestions":[],"confidence":0.9}"#;
        let mock = MockModelClient::default().with_response(json);

        let result: ReviewResult = mock.extract("prompt").unwrap();

        assert!(result.approved);
        assert_eq!(result.confidence, 0.9);
    }

    #[test]
    fn test_verification_approved() {
        let proposer_json =
            r#"{"rule_id":"test-rule","proposed_fix":"x = 1","reasoning":"fix","confidence":0.85}"#;
        let reviewer_json = r#"{"approved":true,"concerns":[],"suggestions":[],"confidence":0.9}"#;

        let proposer = MockModelClient::default().with_response(proposer_json);
        let reviewer = MockModelClient::default().with_response(reviewer_json);

        let verifier = MultiModelVerifier::new(proposer, reviewer, MultiModelConfig::default());
        let result = verifier
            .verify("test-rule", r#"{"issue":"validation failed"}"#, "test context")
            .unwrap();

        assert!(result.is_approved());
    }

    #[test]
    fn test_verification_rejected_low_confidence() {
        let proposer_json =
            r#"{"rule_id":"test-rule","proposed_fix":"x = 1","reasoning":"fix","confidence":0.85}"#;
        let reviewer_json =
            r#"{"approved":true,"concerns":[],"suggestions":[],"confidence":0.5}"#;

        let proposer = MockModelClient::default().with_response(proposer_json);
        let reviewer = MockModelClient::default().with_response(reviewer_json);

        let config = MultiModelConfig::default().with_threshold(0.80);
        let verifier = MultiModelVerifier::new(proposer, reviewer, config);
        let result = verifier
            .verify("test-rule", r#"{"issue":"low confidence"}"#, "test context")
            .unwrap();

        assert!(!result.is_approved());
    }

    #[test]
    fn test_config_threshold() {
        let config = MultiModelConfig::default();
        assert_eq!(config.min_reviewer_confidence, 0.80);

        let config = MultiModelConfig::default().with_threshold(0.90);
        assert_eq!(config.min_reviewer_confidence, 0.90);
    }

    #[test]
    fn test_config_new() {
        let config = MultiModelConfig::new("gpt-4", "gpt-3.5");
        assert_eq!(config.proposer_model, "gpt-4");
        assert_eq!(config.reviewer_model, "gpt-3.5");
    }
}
