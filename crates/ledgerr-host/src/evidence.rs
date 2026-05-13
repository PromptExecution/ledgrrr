use crate::internal_openai::{provider_status, ProviderReadiness};
use crate::settings::AppSettings;
use arc_kit_au::{
    node::NodeType, EvidenceGraph, EvidenceTracer, ProvenanceBadge, ProvenanceScanner,
};

#[derive(Debug, Default)]
pub struct EvidenceState {
    pub graph: EvidenceGraph,
    pub gaps: Vec<arc_kit_au::ProvenanceGap>,
    pub checked: bool,
}

impl EvidenceState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tx_count(&self) -> usize {
        self.graph.nodes_of_type(NodeType::Transaction).len()
    }

    pub fn gap_count(&self) -> usize {
        self.gaps.len()
    }

    pub fn refresh_gaps(&mut self) {
        self.gaps = self.graph.find_missing_provenance();
        self.checked = true;
    }

    pub fn trace(&self, tx_id: &str) -> Option<arc_kit_au::EvidenceChain> {
        self.graph.trace_transaction(tx_id)
    }

    pub fn provenance_badge(&self, tx_id: &str) -> ProvenanceBadge {
        match self.graph.trace_transaction(tx_id) {
            Some(chain) => ProvenanceBadge::from(&chain),
            None => ProvenanceBadge::NotFound,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct TodayQueue {
    pub providers: Vec<crate::internal_openai::ProviderInfo>,
    pub ready_to_review: u32,
    pub blocked: u32,
    pub exported: u32,
    /// Transactions that have ValidationIssue nodes attached.
    pub with_validation_issues: u32,
    pub last_action_summary: String,
    pub next_actions: Vec<String>,
}

impl TodayQueue {
    pub fn from_state(evidence: &EvidenceState, settings: &AppSettings) -> Self {
        let providers = provider_status(settings);
        let summary = evidence.graph.work_queue_summary();
        let blocked = saturating_u32(summary.blocked);
        let ready = saturating_u32(summary.ready_to_review);
        let exported = saturating_u32(summary.exported);
        let validation = saturating_u32(summary.with_validation_issues);

        let last_action_summary = if evidence.checked {
            if blocked == 0 && ready == 0 && validation == 0 {
                "All evidence chains are complete.".to_string()
            } else {
                let mut parts = vec![];
                if blocked > 0 {
                    parts.push(format!("{} blocked (critical gaps)", blocked));
                }
                if ready > 0 {
                    parts.push(format!("{} ready for review", ready));
                }
                if validation > 0 {
                    parts.push(format!("{} with validation issues", validation));
                }
                format!("{} transactions need attention.", parts.join(", "))
            }
        } else {
            "Provenance check has not run yet.".to_string()
        };

        let mut next_actions = vec![];
        if blocked > 0 {
            next_actions.push(format!(
                "Review {} transactions with critical gaps.",
                blocked
            ));
        }
        if ready > 0 {
            next_actions.push(format!(
                "Review {} transactions with partial evidence.",
                ready
            ));
        }
        if next_actions.is_empty() && evidence.checked {
            next_actions.push("No review items — ready to export workbook.".to_string());
        }
        next_actions.push("Ingest documents via ledgerr_documents".to_string());

        for provider in &providers {
            if provider.label == settings.model_provider {
                match &provider.readiness {
                    ProviderReadiness::SetupNeeded { next_command } => {
                        next_actions.insert(0, format!("Model setup needed: {next_command}"));
                    }
                    ProviderReadiness::Diagnostic { reason } => {
                        next_actions.insert(0, format!("Model diagnostic: {reason}"));
                    }
                    _ => {}
                }
            }
        }

        Self {
            providers,
            ready_to_review: ready,
            blocked,
            exported,
            with_validation_issues: validation,
            last_action_summary,
            next_actions,
        }
    }
}

fn saturating_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal_openai::ModelProviderLabel;

    fn test_settings() -> AppSettings {
        AppSettings::default()
    }

    #[test]
    fn empty_evidence_state_starts_unchecked() {
        let state = EvidenceState::new();
        assert!(!state.checked);
        assert_eq!(state.tx_count(), 0);
        assert_eq!(state.gap_count(), 0);
    }

    #[test]
    fn today_queue_returns_empty_state_for_new_evidence() {
        let state = EvidenceState::new();
        let queue = TodayQueue::from_state(&state, &test_settings());
        assert_eq!(queue.ready_to_review, 0);
        assert_eq!(queue.blocked, 0);
        assert_eq!(queue.exported, 0);
        assert!(!queue.next_actions.is_empty());
    }

    #[test]
    fn provenance_badge_returns_not_found_for_missing_tx() {
        let state = EvidenceState::new();
        assert_eq!(
            state.provenance_badge("nonexistent"),
            ProvenanceBadge::NotFound
        );
    }

    #[test]
    fn refresh_gaps_updates_checked_flag() {
        let mut state = EvidenceState::new();
        state.refresh_gaps();
        assert!(state.checked);
        // Empty graph should have no gaps
        assert_eq!(state.gap_count(), 0);
    }

    #[test]
    fn today_queue_flags_model_setup_when_cloud_selected() {
        let mut settings = test_settings();
        settings.model_provider = ModelProviderLabel::Cloud;
        let state = EvidenceState::new();
        let queue = TodayQueue::from_state(&state, &settings);
        let has_model_action = queue
            .next_actions
            .iter()
            .any(|a| a.contains("Model setup needed") || a.contains("Configure"));
        assert!(has_model_action);
    }

    #[test]
    fn with_validation_issues_field_is_populated() {
        use arc_kit_au::node::{EvidenceNode, ValidationIssue};
        use chrono::Utc;
        let mut state = EvidenceState::new();
        let vi = ValidationIssue {
            tx_id: "tx_abc".to_string(),
            rule: "amount_check".to_string(),
            severity: "error".to_string(),
            message: "amount out of range".to_string(),
            actor: "pipeline".to_string(),
            raised_at: Utc::now(),
            resolved: false,
        };
        state.graph.add_node(EvidenceNode::ValidationIssue(vi)).unwrap();
        state.refresh_gaps();
        let queue = TodayQueue::from_state(&state, &test_settings());
        assert_eq!(queue.with_validation_issues, 1);
        // All chains are not "complete" while validation issues remain
        assert!(!queue.last_action_summary.contains("All evidence chains are complete"));
    }

    #[test]
    fn all_complete_requires_zero_validation_issues() {
        let mut state = EvidenceState::new();
        state.refresh_gaps();
        let queue = TodayQueue::from_state(&state, &test_settings());
        // Empty graph has no gaps and no validation issues — should report all complete
        assert_eq!(queue.with_validation_issues, 0);
        assert_eq!(queue.blocked, 0);
        assert_eq!(queue.ready_to_review, 0);
        assert!(queue.last_action_summary.contains("All evidence chains are complete"));
    }
}
