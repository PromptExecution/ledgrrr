/// A type-graph node emitted by `#[derive(HolonEmit)]`.
///
/// Consumers convert to their own graph node type via `From<HolonNode>`.
/// Defined in this companion crate (not the proc-macro crate itself) because
/// Rust proc-macro crates cannot export non-macro items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HolonNode {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub z_layer: Option<String>,
    pub semantic_type: Option<String>,
}
