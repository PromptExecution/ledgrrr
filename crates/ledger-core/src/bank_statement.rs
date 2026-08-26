//! Deterministic bank-statement classification over a `DoclingDocumentGraph`.
//!
//! Fixes the previous placeholder bridge in `PdfIngestOp::execute`, which
//! built `TransactionInput { date: candidate.section, amount:
//! candidate.confidence.to_string(), .. } ` from a `ReqIfCandidate` — a
//! modal-verb-detected *normative requirement sentence*, which has no real
//! transaction date or dollar amount at all. That mapping type-checked but
//! was semantically nonsense (a requirement's confidence score is not a
//! transaction amount).
//!
//! This module classifies each `DoclingNode` procedurally — regex pattern
//! matching, never an LLM — against a fixed set of `NodeCategory` shapes,
//! and only nodes classified as `NodeCategory::TransactionRow` are ever
//! turned into a `TransactionInput`. This is the "walk the tree to
//! categorize & action it correctly" step: a deterministic constraint
//! solver over `ufo_types::Satisfies`, not a generative one.

use regex::Regex;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::LazyLock;
use ufo_types::satisfies::{Constraint, Disposition, NodeId, Satisfies, SatisfiesResult};

use crate::docling_bridge::{DoclingDocumentGraph, DoclingNode};
use crate::ingest::TransactionInput;

/// Categories a document node can be classified into. Deliberately closed
/// (not an open string) so every category has an explicit, reviewable
/// classification rule below — an unhandled category is a compile error,
/// not a silently-skipped node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeCategory {
    /// A single ledger line: date + description + signed dollar amount,
    /// optionally followed by a running balance.
    TransactionRow,
    /// Statement metadata: "Beginning balance" / "Ending balance" and
    /// similar period-summary lines.
    StatementHeader,
    /// Fee schedule / rate disclosure text — informational, never a
    /// transaction, but still worth tagging (not `Unclassified`).
    FeeSchedule,
    /// Legal boilerplate / disclaimers.
    Disclaimer,
    /// Did not match any known shape.
    Unclassified,
}

impl NodeCategory {
    /// Tag strings mirroring the `properties.subtypes` convention already
    /// used by `reqif-opa-mcp`'s SARIF producer (`reqif_mcp/sarif_producer.py`:
    /// `requirement.subtypes` flows into `rule.properties.subtypes` /
    /// `result.properties.subtypes`). Using the same dotted-namespace
    /// convention here means a future round-trip back into that SARIF
    /// pipeline needs no translation layer.
    pub fn sarif_subtypes(self) -> Vec<String> {
        match self {
            Self::TransactionRow => vec!["bank_statement.transaction_row".to_string()],
            Self::StatementHeader => vec!["bank_statement.header".to_string()],
            Self::FeeSchedule => vec!["bank_statement.fee_schedule".to_string()],
            Self::Disclaimer => vec!["bank_statement.disclaimer".to_string()],
            Self::Unclassified => vec![],
        }
    }
}

/// A `ufo_types::Satisfies` constraint: "does this node belong to category C?"
pub struct CategoryConstraint(pub NodeCategory);

impl Constraint for CategoryConstraint {}

// Wells Fargo-style transaction row: `MM/DD` or `MM/DD/YYYY`, then anything,
// then a signed dollar amount, optionally followed by a running balance.
// Matches the date-format conventions already detected in
// `document_shape.rs::detect_date_format` (`%m/%d/%Y`, `%m/%d`).
static TRANSACTION_ROW: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?x)
        ^\s*(?P<date>\d{1,2}/\d{1,2}(?:/\d{2,4})?)\s+
        (?P<description>.+?)\s+
        (?P<amount>-?\$?\d[\d,]*\.\d{2})
        (?:\s+\$?(?P<balance>\d[\d,]*\.\d{2}))?\s*$
    ").expect("static regex is valid")
});

static STATEMENT_HEADER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(beginning|opening|ending|closing)\s+balance").expect("static regex is valid")
});

static FEE_SCHEDULE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(monthly\s+service\s+fee|overdraft\s+fee|interest\s+rate|APY)").expect("static regex is valid")
});

static DISCLAIMER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(member\s+FDIC|equal\s+housing\s+lender|see\s+reverse\s+side)").expect("static regex is valid")
});

impl Satisfies<CategoryConstraint> for DoclingNode {
    fn satisfies(&self, constraint: &CategoryConstraint) -> SatisfiesResult {
        let Some(text) = self.text.as_deref() else {
            return SatisfiesResult::unknown();
        };
        let evidence = vec![NodeId::new(self.node_id.clone())];

        let matched = match constraint.0 {
            NodeCategory::TransactionRow => TRANSACTION_ROW.is_match(text),
            NodeCategory::StatementHeader => STATEMENT_HEADER.is_match(text),
            NodeCategory::FeeSchedule => FEE_SCHEDULE.is_match(text),
            NodeCategory::Disclaimer => DISCLAIMER.is_match(text),
            NodeCategory::Unclassified => false,
        };

        if matched {
            // Deterministic regex match: full confidence, no ambiguity band.
            SatisfiesResult {
                disposition: Disposition::Satisfied,
                confidence: 1.0,
                evidence_nodes: evidence,
                ufo_category: ufo_types::UfoStereotype::Mode("BankStatementNode".into()),
            }
        } else {
            SatisfiesResult::violated("no pattern match for this category")
        }
    }
}

/// A node paired with the (single, best) category it was classified into.
#[derive(Debug, Clone)]
pub struct ClassifiedNode<'a> {
    pub node: &'a DoclingNode,
    pub category: NodeCategory,
    pub result: SatisfiesResult,
}

/// Category check order matters only in that it is exhaustive and each
/// check is mutually exclusive by construction (a transaction row's regex
/// cannot also match the header/fee/disclaimer keyword patterns in
/// practice); ties are not possible today, so no priority scheme is needed.
const CATEGORY_ORDER: [NodeCategory; 4] = [
    NodeCategory::TransactionRow,
    NodeCategory::StatementHeader,
    NodeCategory::FeeSchedule,
    NodeCategory::Disclaimer,
];

/// Walk every text-bearing node in the graph and classify it. This is the
/// constraint-solver "walk the tree to categorize & action it correctly"
/// step: deterministic, procedural, and lint-able (every `ClassifiedNode`
/// carries the `SatisfiesResult` that justified its category, not just a
/// bare label).
pub fn classify_document(graph: &DoclingDocumentGraph) -> Vec<ClassifiedNode<'_>> {
    graph
        .text_nodes()
        .map(|node| {
            for &category in &CATEGORY_ORDER {
                let result = node.satisfies(&CategoryConstraint(category));
                if result.disposition.is_satisfied() {
                    return ClassifiedNode { node, category, result };
                }
            }
            ClassifiedNode {
                node,
                category: NodeCategory::Unclassified,
                result: SatisfiesResult::unknown(),
            }
        })
        .collect()
}

/// Error bridging a classified node into `TransactionInput`.
#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("node {0} was not classified as a transaction row")]
    NotATransactionRow(String),
    #[error("node {0} matched the transaction pattern but its amount failed to parse: {1}")]
    AmountParse(String, rust_decimal::Error),
}

/// Convert a `TransactionRow`-classified node into the flat
/// `TransactionInput` shape the rest of the ingest pipeline expects,
/// re-running the same regex to extract real date/description/amount
/// fields instead of the placeholder `section`/`confidence` mapping this
/// replaces.
pub fn node_to_transaction_input(
    classified: &ClassifiedNode<'_>,
    account_id: &str,
) -> Result<TransactionInput, BridgeError> {
    if classified.category != NodeCategory::TransactionRow {
        return Err(BridgeError::NotATransactionRow(classified.node.node_id.clone()));
    }
    let text = classified.node.text.as_deref().unwrap_or_default();
    let caps = TRANSACTION_ROW
        .captures(text)
        .ok_or_else(|| BridgeError::NotATransactionRow(classified.node.node_id.clone()))?;

    let date = caps["date"].to_string();
    let description = caps["description"].trim().to_string();
    let raw_amount = caps["amount"].replace(['$', ','], "");
    let amount = Decimal::from_str(&raw_amount)
        .map_err(|e| BridgeError::AmountParse(classified.node.node_id.clone(), e))?;

    Ok(TransactionInput {
        account_id: account_id.to_string(),
        date,
        amount: amount.to_string(),
        description,
        // Traceable back to the exact source node/page, unlike the old
        // filename-only source_ref — a page number is embedded when known.
        source_ref: match classified.node.first_page() {
            Some(page) => format!("{}#page={page}", classified.node.node_id),
            None => classified.node.node_id.clone(),
        },
    })
}

/// Statement-level period metadata, distinct from any single transaction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatementPeriod {
    /// Raw as extracted (e.g. `"05/01/2026"`) — parsed downstream by the
    /// same date-format machinery `document_shape.rs` already uses, kept
    /// as a string here for the same reason `TransactionInput::date` is a
    /// string: format detection is a separate, later pipeline stage.
    pub start: String,
    pub end: String,
}

/// Statement-level header metadata: opening/closing balance for the period.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatementHeader {
    pub opening_balance: Decimal,
    pub closing_balance: Decimal,
}

/// Scan classified `StatementHeader`-category nodes for beginning/ending
/// balance amounts. Returns `None` if either amount is missing — a partial
/// header is not a usable header, per the "procedurally lint-able" mandate
/// (callers should treat this as `NeedsReview`, not silently proceed with a
/// zeroed balance).
pub fn extract_statement_header(graph: &DoclingDocumentGraph) -> Option<StatementHeader> {
    static BALANCE_AMOUNT: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\$?(\d[\d,]*\.\d{2})").expect("static regex is valid"));

    let mut opening: Option<Decimal> = None;
    let mut closing: Option<Decimal> = None;

    for classified in classify_document(graph) {
        if classified.category != NodeCategory::StatementHeader {
            continue;
        }
        let text = classified.node.text.as_deref().unwrap_or_default();
        let Some(amount_caps) = BALANCE_AMOUNT.captures(text) else {
            continue;
        };
        let Ok(amount) = Decimal::from_str(&amount_caps[1].replace(',', "")) else {
            continue;
        };
        let lower = text.to_lowercase();
        if lower.contains("beginning") || lower.contains("opening") {
            opening = Some(amount);
        } else if lower.contains("ending") || lower.contains("closing") {
            closing = Some(amount);
        }
    }

    match (opening, closing) {
        (Some(opening_balance), Some(closing_balance)) => Some(StatementHeader {
            opening_balance,
            closing_balance,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docling_bridge::{DoclingAnchor, DoclingArtifact};
    use std::collections::HashMap;

    fn node(node_id: &str, text: &str, page: Option<u32>) -> DoclingNode {
        DoclingNode {
            node_id: node_id.to_string(),
            node_type: "paragraph".to_string(),
            text: Some(text.to_string()),
            parent_id: None,
            semantic_id: format!("semantic-{node_id}"),
            attributes: HashMap::new(),
            anchors: vec![DoclingAnchor {
                kind: "pdf_page_paragraph".to_string(),
                artifact_id: "artifact-1".to_string(),
                page,
                ..Default::default()
            }],
        }
    }

    fn graph(nodes: Vec<DoclingNode>) -> DoclingDocumentGraph {
        DoclingDocumentGraph {
            schema: "document_graph/1".to_string(),
            artifact: DoclingArtifact {
                artifact_id: "artifact-1".to_string(),
                source_uri: None,
                sha256: "deadbeef".to_string(),
                document_profile: "pdf_docling_v1".to_string(),
            },
            profile: "pdf_docling_v1".to_string(),
            nodes,
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn classifies_transaction_row() {
        let g = graph(vec![node("n1", "05/01 Check 1042 -$120.00 $4,880.00", Some(2))]);
        let classified = classify_document(&g);
        assert_eq!(classified.len(), 1);
        assert_eq!(classified[0].category, NodeCategory::TransactionRow);
        assert!(classified[0].result.disposition.is_satisfied());
    }

    #[test]
    fn classifies_statement_header() {
        let g = graph(vec![node("n1", "Beginning balance on 5/1 $5,000.00", None)]);
        let classified = classify_document(&g);
        assert_eq!(classified[0].category, NodeCategory::StatementHeader);
    }

    #[test]
    fn classifies_fee_schedule() {
        let g = graph(vec![node("n1", "Monthly service fee: $12.00 unless minimum balance met", None)]);
        assert_eq!(classify_document(&g)[0].category, NodeCategory::FeeSchedule);
    }

    #[test]
    fn classifies_disclaimer() {
        let g = graph(vec![node("n1", "Wells Fargo Bank, N.A. Member FDIC.", None)]);
        assert_eq!(classify_document(&g)[0].category, NodeCategory::Disclaimer);
    }

    #[test]
    fn unclassified_when_no_pattern_matches() {
        let g = graph(vec![node("n1", "Table of Contents", None)]);
        assert_eq!(classify_document(&g)[0].category, NodeCategory::Unclassified);
    }

    #[test]
    fn bridges_transaction_row_to_real_transaction_input_not_placeholder_garbage() {
        let g = graph(vec![node("n1", "05/01 Check 1042 -$120.00 $4,880.00", Some(2))]);
        let classified = classify_document(&g);
        let tx = node_to_transaction_input(&classified[0], "acct-123").unwrap();

        // The bug this replaces: date was `candidate.section` (a heading
        // path), amount was `candidate.confidence.to_string()` (e.g.
        // "0.85"). Assert the real values instead.
        assert_eq!(tx.date, "05/01");
        assert_eq!(tx.amount, "-120.00");
        assert_eq!(tx.description, "Check 1042");
        assert_eq!(tx.account_id, "acct-123");
        assert_eq!(tx.source_ref, "n1#page=2");
    }

    #[test]
    fn rejects_bridging_a_non_transaction_node() {
        let g = graph(vec![node("n1", "Member FDIC.", None)]);
        let classified = classify_document(&g);
        assert!(node_to_transaction_input(&classified[0], "acct-123").is_err());
    }

    #[test]
    fn extracts_statement_header_when_both_balances_present() {
        let g = graph(vec![
            node("n1", "Beginning balance on 5/1 $5,000.00", None),
            node("n2", "Ending balance on 5/31 $4,880.00", None),
        ]);
        let header = extract_statement_header(&g).unwrap();
        assert_eq!(header.opening_balance, Decimal::from_str("5000.00").unwrap());
        assert_eq!(header.closing_balance, Decimal::from_str("4880.00").unwrap());
    }

    #[test]
    fn no_header_when_only_one_balance_present() {
        let g = graph(vec![node("n1", "Beginning balance on 5/1 $5,000.00", None)]);
        assert!(extract_statement_header(&g).is_none());
    }

    #[test]
    fn sarif_subtypes_match_reqif_opa_mcp_convention() {
        assert_eq!(
            NodeCategory::TransactionRow.sarif_subtypes(),
            vec!["bank_statement.transaction_row".to_string()]
        );
        assert!(NodeCategory::Unclassified.sarif_subtypes().is_empty());
    }
}
