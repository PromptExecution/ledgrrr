//! Append-only action log with Blake3-hashed records.
//!
//! `ImmutableActionLog` guarantees that once a record is appended it cannot
//! be mutated or removed. The hash of each `ActionRecord` is computed at
//! append time over `(action_kind as u8, authorized_by, timestamp_ms,
//! payload_hash)` and stored in the `id` field.

use serde::{Deserialize, Serialize};

/// Discriminates the kind of action recorded in the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    /// A process step was authorized and transitioned.
    StepAuthorized,
    /// A holon node was created.
    HolonCreated,
    /// A holon node was linked to a parent.
    HolonLinked,
    /// An external audit event was recorded.
    AuditEvent,
}

/// A single immutable action record.
///
/// The `id` field holds the Blake3 hash of the record's content, computed at
/// append time — it is not caller-supplied.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRecord {
    /// Blake3 hash of `(action_kind as u8 || authorized_by bytes || timestamp_ms le-bytes || payload_hash)`.
    pub id: [u8; 32],
    /// Semantic kind of this action.
    pub action_kind: ActionKind,
    /// Identity of the authorizing agent or user (non-empty).
    pub authorized_by: String,
    /// Unix timestamp in milliseconds at which the record was appended.
    pub timestamp_ms: u64,
    /// Blake3 hash of the action payload (caller-supplied; all-zeros if no payload).
    pub payload_hash: [u8; 32],
}

/// Append-only log of `ActionRecord`s.
///
/// The only write path is `append` — there is no remove or replace operation.
/// Iteration is exposed via `iter()` and `len()`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImmutableActionLog {
    records: Vec<ActionRecord>,
}

impl ImmutableActionLog {
    /// Create an empty log.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a new record to the log.
    ///
    /// The `id` is computed deterministically over the record's content fields.
    /// Returns a reference to the appended record.
    pub fn append(
        &mut self,
        action_kind: ActionKind,
        authorized_by: impl Into<String>,
        timestamp_ms: u64,
        payload_hash: [u8; 32],
    ) -> &ActionRecord {
        let authorized_by = authorized_by.into();
        let id = compute_record_id(action_kind, &authorized_by, timestamp_ms, &payload_hash);
        let record = ActionRecord {
            id,
            action_kind,
            authorized_by,
            timestamp_ms,
            payload_hash,
        };
        self.records.push(record);
        // Safe: we just pushed so the vec is non-empty.
        self.records
            .last()
            .expect("records vec non-empty after push")
    }

    /// Number of records in the log.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Returns `true` if the log contains no records.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Iterate over all records in append order.
    pub fn iter(&self) -> impl Iterator<Item = &ActionRecord> {
        self.records.iter()
    }

    /// Retrieve a record by its Blake3 `id` digest.
    pub fn find_by_id(&self, id: &[u8; 32]) -> Option<&ActionRecord> {
        self.records.iter().find(|r| &r.id == id)
    }
}

/// Compute the Blake3 ID for an `ActionRecord`.
///
/// Input layout (concatenated):
/// ```text
/// action_kind_byte (1 byte)
/// || authorized_by UTF-8 bytes
/// || 0x00 (separator)
/// || timestamp_ms as little-endian u64 (8 bytes)
/// || payload_hash (32 bytes)
/// ```
pub(crate) fn compute_record_id(
    action_kind: ActionKind,
    authorized_by: &str,
    timestamp_ms: u64,
    payload_hash: &[u8; 32],
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[action_kind as u8]);
    hasher.update(authorized_by.as_bytes());
    hasher.update(&[0x00]); // separator
    hasher.update(&timestamp_ms.to_le_bytes());
    hasher.update(payload_hash);
    *hasher.finalize().as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_is_deterministic() {
        let payload = [0u8; 32];
        let id_a = compute_record_id(ActionKind::AuditEvent, "alice", 1_000, &payload);
        let id_b = compute_record_id(ActionKind::AuditEvent, "alice", 1_000, &payload);
        assert_eq!(id_a, id_b);
    }

    #[test]
    fn different_inputs_produce_different_ids() {
        let payload = [0u8; 32];
        let id_a = compute_record_id(ActionKind::AuditEvent, "alice", 1_000, &payload);
        let id_b = compute_record_id(ActionKind::AuditEvent, "bob", 1_000, &payload);
        assert_ne!(id_a, id_b);
    }

    #[test]
    fn log_is_append_only_and_ordered() {
        let mut log = ImmutableActionLog::new();
        let payload = [0u8; 32];
        log.append(ActionKind::HolonCreated, "agent-1", 1, payload);
        log.append(ActionKind::HolonLinked, "agent-1", 2, payload);
        assert_eq!(log.len(), 2);
        let kinds: Vec<_> = log.iter().map(|r| r.action_kind).collect();
        assert_eq!(kinds, vec![ActionKind::HolonCreated, ActionKind::HolonLinked]);
    }
}
