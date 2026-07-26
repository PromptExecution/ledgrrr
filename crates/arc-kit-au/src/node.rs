//! Evidence node types and identity.
//!
//! Each node represents a step in the bookkeeping evidence chain.
//! Node IDs are deterministic Blake3 hashes of their content.

use blake3::Hasher;
use chrono::{DateTime, Utc};
use ordered_float::OrderedFloat;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Confidence score — a total-ordered f64 that implements Eq, Ord, and Hash.
///
/// Uses `ordered_float::OrderedFloat` so confidence values can participate
/// in generic traits requiring Eq/Ord (e.g. BTreeMap keys, HashSet membership,
/// deterministic sorting in review queues).
pub type Confidence = OrderedFloat<f64>;

/// Deterministic node identity.
///
/// Format: `{type_prefix}:{blake3_hex}`
/// Examples: `doc:abc123...`, `tx:def456...`, `approval:789...`
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub String);

impl NodeId {
    pub fn new(node_type: NodeType, content_hash: &str) -> Self {
        Self(format!("{}:{}", node_type.prefix(), content_hash))
    }

    pub fn node_type(&self) -> NodeType {
        let prefix = self.0.split(':').next().unwrap_or("");
        match prefix {
            "doc" => NodeType::SourceDoc,
            "row" => NodeType::ExtractedRow,
            "tx" => NodeType::Transaction,
            "cls" => NodeType::Classification,
            "prop" => NodeType::ModelProposal,
            "approval" => NodeType::OperatorApproval,
            "wb" => NodeType::WorkbookRow,
            "vi" => NodeType::ValidationIssue,
            _ => NodeType::Unknown,
        }
    }

    pub fn hash(&self) -> &str {
        self.0.split_once(':').map(|x| x.1).unwrap_or(&self.0)
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Type of evidence node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    /// Source document (PDF, CSV, etc.)
    SourceDoc,
    /// Extracted row from document parsing
    ExtractedRow,
    /// Deterministic transaction record
    Transaction,
    /// Classification applied to transaction
    Classification,
    /// Model-generated classification proposal
    ModelProposal,
    /// Operator approval/rejection of proposal
    OperatorApproval,
    /// Final workbook row in CPA export
    WorkbookRow,
    /// Validation issue from the Rhai engine or pipeline check
    ValidationIssue,
    /// R&D activity registered under s.355-100 ITAA 1997.
    RndActivity,
    /// ATO tax offset claim or estimate.
    TaxOffset,
    /// Unknown or unrecognized type
    Unknown,
}

impl NodeType {
    pub fn prefix(&self) -> &'static str {
        match self {
            Self::SourceDoc => "doc",
            Self::ExtractedRow => "row",
            Self::Transaction => "tx",
            Self::Classification => "cls",
            Self::ModelProposal => "prop",
            Self::OperatorApproval => "approval",
            Self::WorkbookRow => "wb",
            Self::ValidationIssue => "vi",
            Self::RndActivity => "rnd",
            Self::TaxOffset => "tax",
            Self::Unknown => "unknown",
        }
    }
}

/// Content hash utility for deterministic node identity.
pub fn content_hash(parts: &[&str]) -> String {
    let mut hasher = Hasher::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update(b"\n");
    }
    hasher.finalize().to_hex().to_string()
}

/// Source document evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceDoc {
    /// Original filename (must follow VENDOR--ACCOUNT--YYYY-MM--DOCTYPE.ext)
    pub filename: String,
    /// Vendor/account/date parsed from filename
    pub vendor: String,
    pub account_id: String,
    pub statement_date: String,
    pub document_type: String,
    /// Blake3 hash of file content
    pub content_hash: String,
    /// Ingest timestamp
    pub ingested_at: DateTime<Utc>,
    /// Raw context path if written
    pub raw_context_path: Option<String>,
}

impl SourceDoc {
    pub fn node_id(&self) -> NodeId {
        NodeId::new(
            NodeType::SourceDoc,
            &content_hash(&[&self.filename, &self.content_hash]),
        )
    }
}

/// Extracted row from document parsing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractedRow {
    pub account_id: String,
    pub date: String,
    pub amount: Decimal,
    pub description: String,
    pub source_document: NodeId,
    pub extraction_confidence: Confidence,
}

impl ExtractedRow {
    pub fn node_id(&self) -> NodeId {
        NodeId::new(
            NodeType::ExtractedRow,
            &content_hash(&[
                &self.account_id,
                &self.date,
                &self.amount.to_string(),
                &self.description,
            ]),
        )
    }
}

/// Deterministic transaction record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transaction {
    /// Blake3 hash of account/date/amount/description
    pub tx_id: String,
    pub account_id: String,
    pub date: String,
    pub amount: String,
    pub description: String,
    pub source_rows: Vec<NodeId>,
}

impl Transaction {
    pub fn node_id(&self) -> NodeId {
        NodeId::new(NodeType::Transaction, &self.tx_id)
    }
}

/// Classification applied to transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Classification {
    pub tx_id: String,
    pub category: String,
    pub sub_category: Option<String>,
    pub confidence: Confidence,
    pub rule_used: Option<String>,
    pub actor: String,
    pub classified_at: DateTime<Utc>,
    pub note: Option<String>,
}

impl Classification {
    pub fn node_id(&self) -> NodeId {
        NodeId::new(
            NodeType::Classification,
            &content_hash(&[
                &self.tx_id,
                &self.category,
                &self.actor,
                &self.classified_at.to_rfc3339(),
            ]),
        )
    }
}

/// Model-generated classification proposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelProposal {
    pub tx_id: String,
    pub model_name: String,
    pub proposed_category: String,
    pub confidence: Confidence,
    pub reasoning: Option<String>,
    pub proposed_at: DateTime<Utc>,
    pub validated: bool,
}

impl ModelProposal {
    pub fn node_id(&self) -> NodeId {
        NodeId::new(
            NodeType::ModelProposal,
            &content_hash(&[
                &self.tx_id,
                &self.model_name,
                &self.proposed_category,
                &self.proposed_at.to_rfc3339(),
            ]),
        )
    }
}

/// Operator approval/rejection of model proposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperatorApproval {
    pub tx_id: String,
    pub operator_id: String,
    pub approved: bool,
    pub rationale: Option<String>,
    pub approved_at: DateTime<Utc>,
}

impl OperatorApproval {
    pub fn node_id(&self) -> NodeId {
        NodeId::new(
            NodeType::OperatorApproval,
            &content_hash(&[
                &self.tx_id,
                &self.operator_id,
                &self.approved.to_string(),
                &self.approved_at.to_rfc3339(),
            ]),
        )
    }
}

/// Validation issue from the Rhai engine or pipeline check.
///
/// Distinct from Classification — validation artifacts represent rule/constraint
/// failures, not categorization decisions. PRD-4 Phase 2 requires
/// classification_artifact → validation_artifact as a separate chain step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub tx_id: String,
    pub rule: String,
    pub severity: String,
    pub message: String,
    pub actor: String,
    pub raised_at: DateTime<Utc>,
    pub resolved: bool,
}

impl ValidationIssue {
    pub fn node_id(&self) -> NodeId {
        NodeId::new(
            NodeType::ValidationIssue,
            &content_hash(&[
                &self.tx_id,
                &self.rule,
                &self.severity,
                &self.raised_at.to_rfc3339(),
            ]),
        )
    }
}

/// Final workbook row in CPA export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkbookRow {
    pub tx_id: String,
    pub sheet_name: String,
    pub row_index: usize,
    pub category: String,
    pub amount: String,
    pub exported_at: DateTime<Utc>,
}

impl WorkbookRow {
    pub fn node_id(&self) -> NodeId {
        NodeId::new(
            NodeType::WorkbookRow,
            &content_hash(&[
                &self.tx_id,
                &self.sheet_name,
                &self.row_index.to_string(),
                &self.exported_at.to_rfc3339(),
            ]),
        )
    }
}

/// Unified evidence node enum.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EvidenceNode {
    SourceDoc(SourceDoc),
    ExtractedRow(ExtractedRow),
    Transaction(Transaction),
    Classification(Classification),
    ModelProposal(ModelProposal),
    OperatorApproval(OperatorApproval),
    WorkbookRow(WorkbookRow),
    ValidationIssue(ValidationIssue),
}

impl EvidenceNode {
    pub fn node_id(&self) -> NodeId {
        match self {
            Self::SourceDoc(doc) => doc.node_id(),
            Self::ExtractedRow(row) => row.node_id(),
            Self::Transaction(tx) => tx.node_id(),
            Self::Classification(cls) => cls.node_id(),
            Self::ModelProposal(prop) => prop.node_id(),
            Self::OperatorApproval(approval) => approval.node_id(),
            Self::WorkbookRow(wb) => wb.node_id(),
            Self::ValidationIssue(vi) => vi.node_id(),
        }
    }

    pub fn node_type(&self) -> NodeType {
        match self {
            Self::SourceDoc(_) => NodeType::SourceDoc,
            Self::ExtractedRow(_) => NodeType::ExtractedRow,
            Self::Transaction(_) => NodeType::Transaction,
            Self::Classification(_) => NodeType::Classification,
            Self::ModelProposal(_) => NodeType::ModelProposal,
            Self::OperatorApproval(_) => NodeType::OperatorApproval,
            Self::WorkbookRow(_) => NodeType::WorkbookRow,
            Self::ValidationIssue(_) => NodeType::ValidationIssue,
        }
    }

    pub fn tx_id(&self) -> Option<&str> {
        match self {
            Self::Transaction(tx) => Some(&tx.tx_id),
            Self::Classification(cls) => Some(&cls.tx_id),
            Self::ModelProposal(prop) => Some(&prop.tx_id),
            Self::OperatorApproval(approval) => Some(&approval.tx_id),
            Self::WorkbookRow(wb) => Some(&wb.tx_id),
            Self::ValidationIssue(vi) => Some(&vi.tx_id),
            Self::ExtractedRow(_) | Self::SourceDoc(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn node_id_format_matches_prefix_and_hash() {
        let id = NodeId::new(NodeType::SourceDoc, "abc123");
        assert_eq!(id.0, "doc:abc123");
        assert_eq!(id.node_type(), NodeType::SourceDoc);
        assert_eq!(id.hash(), "abc123");
    }

    #[test]
    fn node_id_type_round_trips_for_all_variants() {
        let cases = [
            (NodeType::SourceDoc, "doc"),
            (NodeType::ExtractedRow, "row"),
            (NodeType::Transaction, "tx"),
            (NodeType::Classification, "cls"),
            (NodeType::ModelProposal, "prop"),
            (NodeType::OperatorApproval, "approval"),
            (NodeType::WorkbookRow, "wb"),
            (NodeType::ValidationIssue, "vi"),
        ];
        for (expected_type, prefix) in cases {
            let id = NodeId(format!("{prefix}:somehash"));
            assert_eq!(
                id.node_type(),
                expected_type,
                "prefix '{prefix}' should map to {expected_type:?}"
            );
        }
        // Unrecognized prefix → Unknown
        assert_eq!(NodeId("zz:abc".to_string()).node_type(), NodeType::Unknown);
    }

    #[test]
    fn content_hash_is_deterministic() {
        let hash1 = content_hash(&["account1", "2024-01-31", "-12.34", "Cafe lunch"]);
        let hash2 = content_hash(&["account1", "2024-01-31", "-12.34", "Cafe lunch"]);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn content_hash_differs_with_different_input() {
        let hash1 = content_hash(&["account1", "2024-01-31", "-12.34", "Cafe lunch"]);
        let hash2 = content_hash(&["account2", "2024-01-31", "-12.34", "Cafe lunch"]);
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn source_doc_node_id_uses_filename_and_content_hash() {
        let doc = SourceDoc {
            filename: "WF--BH-CHK--2024-01--statement.pdf".to_string(),
            vendor: "WF".to_string(),
            account_id: "BH-CHK".to_string(),
            statement_date: "2024-01-31".to_string(),
            document_type: "statement".to_string(),
            content_hash: "deadbeef".to_string(),
            ingested_at: Utc.with_ymd_and_hms(2024, 2, 1, 10, 0, 0).unwrap(),
            raw_context_path: None,
        };
        let id = doc.node_id();
        assert_eq!(id.node_type(), NodeType::SourceDoc);
        assert!(id.0.starts_with("doc:"));
    }

    #[test]
    fn extracted_row_node_id_is_deterministic() {
        use rust_decimal::Decimal;
        let row = ExtractedRow {
            account_id: "BH-CHK".to_string(),
            date: "2024-01-15".to_string(),
            amount: Decimal::new(-1234, 2),
            description: "Cafe lunch".to_string(),
            source_document: NodeId::new(NodeType::SourceDoc, "abc123"),
            extraction_confidence: Confidence::from(0.95),
        };
        let id1 = row.node_id();
        let id2 = row.node_id();
        assert_eq!(id1, id2);
    }

    #[test]
    fn evidence_node_tx_id_returns_for_transaction_types() {
        let tx = Transaction {
            tx_id: "tx_123".to_string(),
            account_id: "BH-CHK".to_string(),
            date: "2024-01-15".to_string(),
            amount: "-12.34".to_string(),
            description: "Cafe lunch".to_string(),
            source_rows: vec![],
        };
        let node = EvidenceNode::Transaction(tx);
        assert_eq!(node.tx_id(), Some("tx_123"));
        assert_eq!(node.node_type(), NodeType::Transaction);

        let doc = EvidenceNode::SourceDoc(SourceDoc {
            filename: "test.pdf".to_string(),
            vendor: "V".to_string(),
            account_id: "A".to_string(),
            statement_date: "2024-01-31".to_string(),
            document_type: "statement".to_string(),
            content_hash: "hash".to_string(),
            ingested_at: Utc.with_ymd_and_hms(2024, 2, 1, 10, 0, 0).unwrap(),
            raw_context_path: None,
        });
        assert!(doc.tx_id().is_none());
    }
}
