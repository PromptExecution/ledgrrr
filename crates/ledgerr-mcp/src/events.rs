use std::collections::BTreeMap;

use crate::ToolError;
use serde::{Deserialize, Serialize};

const EVENT_TYPES: &[&str] = &["ingest", "classification", "reconciliation", "adjustment", "b00t_cost", "b00t_delegate"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleEvent {
    pub event_id: String,
    pub sequence: u64,
    pub event_type: String,
    pub tx_id: Option<String>,
    pub document_ref: Option<String>,
    pub occurred_at: String,
    pub payload: BTreeMap<String, String>,
    pub identity_inputs: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppendEventResult {
    pub event_id: String,
    pub sequence: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventHistoryFilter {
    pub tx_id: Option<String>,
    pub document_ref: Option<String>,
    pub time_start: Option<String>,
    pub time_end: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EventHistoryResponse {
    pub events: Vec<LifecycleEvent>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReplayProjection {
    pub reconstructed_state: String,
    pub event_count: usize,
    pub diagnostics: Vec<String>,
}

pub trait LifecycleEventStore {
    fn append_event(
        &mut self,
        event_type: &str,
        tx_id: Option<String>,
        document_ref: Option<String>,
        payload: BTreeMap<String, String>,
    ) -> Result<AppendEventResult, ToolError>;

    fn list_events(&self, filter: EventHistoryFilter) -> Result<EventHistoryResponse, ToolError>;
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InMemoryLifecycleEventStore {
    events: Vec<LifecycleEvent>,
    next_sequence: u64,
}

impl LifecycleEventStore for InMemoryLifecycleEventStore {
    fn append_event(
        &mut self,
        event_type: &str,
        tx_id: Option<String>,
        document_ref: Option<String>,
        payload: BTreeMap<String, String>,
    ) -> Result<AppendEventResult, ToolError> {
        let normalized_event_type = normalize_event_type(event_type)?;
        let normalized_tx_id = normalize_optional(tx_id);
        let normalized_document_ref = normalize_optional(document_ref);
        let normalized_payload = normalize_payload(payload);

        let sequence = self.next_sequence.saturating_add(1);
        self.next_sequence = sequence;
        let identity_inputs = build_identity_inputs(
            &normalized_event_type,
            normalized_tx_id.as_deref(),
            normalized_document_ref.as_deref(),
            &normalized_payload,
        );
        let event_id = event_id_from_identity(&identity_inputs);
        let occurred_at = normalized_payload
            .get("occurred_at")
            .or_else(|| normalized_payload.get("date"))
            .cloned()
            .unwrap_or_else(|| format!("sequence:{sequence:020}"));

        self.events.push(LifecycleEvent {
            event_id: event_id.clone(),
            sequence,
            event_type: normalized_event_type,
            tx_id: normalized_tx_id,
            document_ref: normalized_document_ref,
            occurred_at,
            payload: normalized_payload,
            identity_inputs,
        });

        Ok(AppendEventResult { event_id, sequence })
    }

    fn list_events(&self, filter: EventHistoryFilter) -> Result<EventHistoryResponse, ToolError> {
        let normalized = normalize_filter(filter)?;
        let events = self
            .events
            .iter()
            .filter(|event| match normalized.tx_id.as_deref() {
                Some(tx_id) => event.tx_id.as_deref() == Some(tx_id),
                None => true,
            })
            .filter(|event| match normalized.document_ref.as_deref() {
                Some(document_ref) => event.document_ref.as_deref() == Some(document_ref),
                None => true,
            })
            .filter(|event| match normalized.time_start.as_deref() {
                Some(time_start) => event.occurred_at.as_str() >= time_start,
                None => true,
            })
            .filter(|event| match normalized.time_end.as_deref() {
                Some(time_end) => event.occurred_at.as_str() <= time_end,
                None => true,
            })
            .cloned()
            .collect::<Vec<_>>();
        Ok(EventHistoryResponse { events })
    }
}

fn normalize_event_type(event_type: &str) -> Result<String, ToolError> {
    let normalized = event_type.trim().to_ascii_lowercase();
    if EVENT_TYPES.iter().any(|candidate| *candidate == normalized) {
        Ok(normalized)
    } else {
        Err(ToolError::InvalidInput(format!(
            "event_type must be one of: {}",
            EVENT_TYPES.join(",")
        )))
    }
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn normalize_payload(payload: BTreeMap<String, String>) -> BTreeMap<String, String> {
    payload
        .into_iter()
        .map(|(key, value)| (key.trim().to_string(), value.trim().to_string()))
        .collect()
}

fn normalize_filter(filter: EventHistoryFilter) -> Result<EventHistoryFilter, ToolError> {
    let normalized = EventHistoryFilter {
        tx_id: normalize_optional(filter.tx_id),
        document_ref: normalize_optional(filter.document_ref),
        time_start: normalize_optional(filter.time_start),
        time_end: normalize_optional(filter.time_end),
    };
    if let (Some(start), Some(end)) = (&normalized.time_start, &normalized.time_end) {
        if start > end {
            return Err(ToolError::InvalidInput(
                "time_start must be <= time_end".to_string(),
            ));
        }
    }
    Ok(normalized)
}

fn build_identity_inputs(
    event_type: &str,
    tx_id: Option<&str>,
    document_ref: Option<&str>,
    payload: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut inputs = BTreeMap::new();
    inputs.insert("event_type".to_string(), event_type.to_string());
    inputs.insert("tx_id".to_string(), tx_id.unwrap_or("").to_string());
    inputs.insert(
        "document_ref".to_string(),
        document_ref.unwrap_or("").to_string(),
    );
    inputs.insert(
        "payload_hash".to_string(),
        payload_hash(payload).to_string(),
    );
    inputs
}

fn payload_hash(payload: &BTreeMap<String, String>) -> blake3::Hash {
    let canonical_payload = payload
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("|");
    blake3::hash(canonical_payload.as_bytes())
}

fn event_id_from_identity(identity_inputs: &BTreeMap<String, String>) -> String {
    let canonical = identity_inputs
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("|");
    blake3::hash(canonical.as_bytes()).to_hex().to_string()
}

pub fn reconstruct_lifecycle(events: &[LifecycleEvent]) -> ReplayProjection {
    let mut ordered = events.to_vec();
    ordered.sort_by(|left, right| {
        left.sequence
            .cmp(&right.sequence)
            .then_with(|| left.event_id.cmp(&right.event_id))
    });

    let mut expected_sequence = 1u64;
    let mut diagnostics = Vec::new();
    let mut stage_by_tx = BTreeMap::<String, String>::new();
    let mut category_by_tx = BTreeMap::<String, String>::new();

    for event in &ordered {
        if event.sequence != expected_sequence {
            diagnostics.push(format!(
                "sequence_gap:expected={expected_sequence},actual={}",
                event.sequence
            ));
            expected_sequence = event.sequence.saturating_add(1);
        } else {
            expected_sequence = expected_sequence.saturating_add(1);
        }

        let tx_id = event.tx_id.clone().unwrap_or_else(|| "_stream".to_string());
        let current_stage = stage_by_tx.get(&tx_id).cloned();
        if current_stage.is_none() && event.event_type != "ingest" {
            diagnostics.push(format!(
                "missing_predecessor:tx_id={tx_id},event_type={}",
                event.event_type
            ));
        }

        if !transition_allowed(current_stage.as_deref(), &event.event_type) {
            diagnostics.push(format!(
                "invalid_transition:tx_id={tx_id},from={},to={}",
                current_stage.unwrap_or_else(|| "none".to_string()),
                event.event_type
            ));
        }

        stage_by_tx.insert(tx_id.clone(), event.event_type.clone());
        if let Some(category) = event.payload.get("category") {
            category_by_tx.insert(tx_id, category.clone());
        }
    }

    diagnostics.sort();
    diagnostics.dedup();

    let reconstructed_state = stage_by_tx
        .iter()
        .map(|(tx_id, stage)| {
            let category = category_by_tx.get(tx_id).cloned().unwrap_or_default();
            format!("tx_id={tx_id};stage={stage};category={category}")
        })
        .collect::<Vec<_>>()
        .join("|");

    ReplayProjection {
        reconstructed_state: if reconstructed_state.is_empty() {
            "empty".to_string()
        } else {
            reconstructed_state
        },
        event_count: ordered.len(),
        diagnostics,
    }
}

fn transition_allowed(previous: Option<&str>, next: &str) -> bool {
    matches!(
        (previous, next),
        (None, "ingest")
            | (Some("ingest"), "classification")
            | (Some("classification"), "reconciliation")
            | (Some("classification"), "adjustment")
            | (Some("reconciliation"), "adjustment")
            | (Some("adjustment"), "adjustment")
    )
}
