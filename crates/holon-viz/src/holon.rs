//! Recursive holarchy node type and kind discriminant.

use serde::{Deserialize, Serialize};

/// Discriminates the semantic role of a [`Holon`] within the holarchy.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HolonKind {
    /// A SysML v2 block definition.
    SysmlBlock,
    /// An OWL2 named class.
    OwlClass,
    /// A logical grouping of capsule-style agents or services.
    CapsuleGroup,
    /// A process step or workflow node.
    ProcessNode,
    /// An append-only audit event node.
    AuditEvent,
}

/// A holon — a node that is simultaneously a part and a whole (holarchy).
///
/// Each holon may reference a parent and own zero or more children, forming
/// a rooted tree. IDs are expected to be hex-encoded Blake3 digests computed
/// by the caller, but the type accepts any `String` for flexibility during
/// construction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Holon {
    /// Unique identifier for this holon (typically a Blake3 hex digest).
    pub id: String,
    /// Human-readable label.
    pub label: String,
    /// Semantic kind.
    pub kind: HolonKind,
    /// Optional parent holon ID. `None` for root holons.
    pub parent_id: Option<String>,
    /// IDs of child holons directly owned by this node.
    pub children: Vec<String>,
    /// Arbitrary key-value metadata for downstream emitters.
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

impl Holon {
    /// Construct a root holon (no parent).
    pub fn root(id: impl Into<String>, label: impl Into<String>, kind: HolonKind) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            kind,
            parent_id: None,
            children: Vec::new(),
            metadata: std::collections::HashMap::new(),
        }
    }

    /// Construct a child holon with a known parent ID.
    pub fn child(
        id: impl Into<String>,
        label: impl Into<String>,
        kind: HolonKind,
        parent_id: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            kind,
            parent_id: Some(parent_id.into()),
            children: Vec::new(),
            metadata: std::collections::HashMap::new(),
        }
    }
}
