use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Document,
    Account,
    Institution,
    Transaction,
    TaxCategory,
    EvidenceReference,
    XeroContact,
    XeroBankAccount,
    XeroInvoice,
    WorkflowTag,
    ModelJob,
    ModelProposal,
    WorkbookRow,
    AuditEvent,
    ValidationIssue,
    DocumentChunk,
    ClassificationOutcome,
    BootDatum,
}

impl ArtifactKind {
    pub fn canonical_name(self) -> &'static str {
        match self {
            Self::Document => "document",
            Self::Account => "account",
            Self::Institution => "institution",
            Self::Transaction => "transaction",
            Self::TaxCategory => "tax_category",
            Self::EvidenceReference => "evidence_reference",
            Self::XeroContact => "xero_contact",
            Self::XeroBankAccount => "xero_bank_account",
            Self::XeroInvoice => "xero_invoice",
            Self::WorkflowTag => "workflow_tag",
            Self::ModelJob => "model_job",
            Self::ModelProposal => "model_proposal",
            Self::WorkbookRow => "workbook_row",
            Self::AuditEvent => "audit_event",
            Self::ValidationIssue => "validation_issue",
            Self::DocumentChunk => "document_chunk",
            Self::ClassificationOutcome => "classification_outcome",
            Self::BootDatum => "boot_datum",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    DocumentsTransaction,
    LinksEvidence,
    LinksTaxCategory,
    DerivedFrom,
    ClassifiedAs,
    ValidatedBy,
    ProjectsTo,
    RecordedIn,
    ProposedByModel,
    ReviewedByOperator,
    ApprovedAs,
    RejectedAs,
    LinkedToXero,
    TaggedAs,
    References,
    RelatedTo,
    DelegatedTo,
}

impl RelationKind {
    pub fn canonical_name(self) -> &'static str {
        match self {
            Self::DocumentsTransaction => "documents_transaction",
            Self::LinksEvidence => "links_evidence",
            Self::LinksTaxCategory => "links_tax_category",
            Self::DerivedFrom => "derived_from",
            Self::ClassifiedAs => "classified_as",
            Self::ValidatedBy => "validated_by",
            Self::ProjectsTo => "projects_to",
            Self::RecordedIn => "recorded_in",
            Self::ProposedByModel => "proposed_by_model",
            Self::ReviewedByOperator => "reviewed_by_operator",
            Self::ApprovedAs => "approved_as",
            Self::RejectedAs => "rejected_as",
            Self::LinkedToXero => "linked_to_xero",
            Self::TaggedAs => "tagged_as",
            Self::References => "references",
            Self::RelatedTo => "related_to",
            Self::DelegatedTo => "delegated_to",
        }
    }
}

impl std::str::FromStr for RelationKind {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "documents_transaction" => Ok(Self::DocumentsTransaction),
            "links_evidence" => Ok(Self::LinksEvidence),
            "links_tax_category" => Ok(Self::LinksTaxCategory),
            "derived_from" => Ok(Self::DerivedFrom),
            "classified_as" => Ok(Self::ClassifiedAs),
            "validated_by" => Ok(Self::ValidatedBy),
            "projects_to" => Ok(Self::ProjectsTo),
            "recorded_in" => Ok(Self::RecordedIn),
            "proposed_by_model" => Ok(Self::ProposedByModel),
            "reviewed_by_operator" => Ok(Self::ReviewedByOperator),
            "approved_as" => Ok(Self::ApprovedAs),
            "rejected_as" => Ok(Self::RejectedAs),
            "linked_to_xero" => Ok(Self::LinkedToXero),
            "tagged_as" => Ok(Self::TaggedAs),
            "references" => Ok(Self::References),
            "related_to" => Ok(Self::RelatedTo),
            "delegated_to" => Ok(Self::DelegatedTo),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub id: String,
    pub kind: ArtifactKind,
    pub attrs: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Relation {
    pub id: String,
    pub from: String,
    pub to: String,
    pub relation: String,
    pub provenance: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvenanceRef {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OntologySnapshot {
    pub artifacts: Vec<Artifact>,
    pub relations: Vec<Relation>,
}

impl OntologySnapshot {
    pub fn sort_deterministic(&mut self) {
        self.artifacts
            .sort_by(|a, b| (a.kind, &a.id).cmp(&(b.kind, &b.id)));
        self.relations.sort_by(|a, b| {
            (&a.relation, &a.from, &a.to, &a.id).cmp(&(&b.relation, &b.from, &b.to, &b.id))
        });
    }

    pub fn to_pretty_json_stable(&self) -> Result<String, serde_json::Error> {
        let mut snapshot = self.clone();
        snapshot.sort_deterministic();
        serde_json::to_string_pretty(&snapshot)
    }

    pub fn to_rhai_dsl(&self) -> String {
        let mut snapshot = self.clone();
        snapshot.sort_deterministic();
        let artifacts = snapshot
            .artifacts
            .iter()
            .map(|artifact| (artifact.id.as_str(), artifact))
            .collect::<BTreeMap<_, _>>();
        let mut lines = Vec::new();

        for relation in &snapshot.relations {
            let Some(from) = artifacts.get(relation.from.as_str()) else {
                continue;
            };
            let Some(to) = artifacts.get(relation.to.as_str()) else {
                continue;
            };

            let from_label = artifact_diagram_label(from);
            let to_label = artifact_diagram_label(to);
            let relation_label = relation_diagram_label(relation);
            lines.push(format!("fn {from_label}() -> {relation_label}"));
            lines.push(format!("fn {relation_label}() -> {to_label}"));

            if relation.provenance.is_empty() {
                let warning =
                    diagram_token(&format!("missing_provenance_{}", short_id(&relation.id)));
                lines.push(format!("fn {relation_label}() -> {warning}"));
            }
        }

        lines.join("\n")
    }
}

fn artifact_diagram_label(artifact: &Artifact) -> String {
    let primary = artifact
        .attrs
        .get("tx_id")
        .or_else(|| artifact.attrs.get("source_ref"))
        .or_else(|| artifact.attrs.get("category"))
        .or_else(|| artifact.attrs.get("id"))
        .or_else(|| artifact.attrs.get("label"))
        .map(String::as_str)
        .unwrap_or_else(|| artifact.id.as_str());
    diagram_token(&format!(
        "{}_{}_{}",
        artifact.kind.canonical_name(),
        primary,
        short_id(&artifact.id)
    ))
}

fn relation_diagram_label(relation: &Relation) -> String {
    diagram_token(&format!(
        "relation_{}_{}",
        relation.relation,
        short_id(&relation.id)
    ))
}

fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}

fn diagram_token(raw: &str) -> String {
    let mut token = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    while token.contains("__") {
        token = token.replace("__", "_");
    }
    token.trim_matches('_').to_string()
}

pub fn artifact_content_hash(kind: ArtifactKind, attrs: &BTreeMap<String, String>) -> String {
    let mut canonical = format!("entity|{}", kind.canonical_name());
    for (key, value) in attrs {
        canonical.push('|');
        canonical.push_str(key);
        canonical.push('=');
        canonical.push_str(value);
    }
    content_hash(&canonical)
}

pub fn relation_content_hash(
    from: &str,
    to: &str,
    relation: &str,
    provenance: &BTreeMap<String, String>,
) -> String {
    let mut canonical = format!("edge|{}|{}|{}", from, to, relation);
    for (key, value) in provenance {
        canonical.push('|');
        canonical.push_str(key);
        canonical.push('=');
        canonical.push_str(value);
    }
    content_hash(&canonical)
}

pub fn content_hash(canonical: &str) -> String {
    blake3::hash(canonical.as_bytes()).to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ontology_id_is_stable() {
        let mut attrs = BTreeMap::new();
        attrs.insert("tx_id".to_string(), "tx-001".to_string());

        let first = artifact_content_hash(ArtifactKind::Transaction, &attrs);
        let second = artifact_content_hash(ArtifactKind::Transaction, &attrs);

        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
    }

    #[test]
    fn ontology_snapshot_sorting_is_deterministic() {
        let mut tx_attrs = BTreeMap::new();
        tx_attrs.insert("tx_id".to_string(), "tx-001".to_string());
        let mut doc_attrs = BTreeMap::new();
        doc_attrs.insert("source_ref".to_string(), "statement.pdf".to_string());

        let tx = Artifact {
            id: artifact_content_hash(ArtifactKind::Transaction, &tx_attrs),
            kind: ArtifactKind::Transaction,
            attrs: tx_attrs,
        };
        let doc = Artifact {
            id: artifact_content_hash(ArtifactKind::Document, &doc_attrs),
            kind: ArtifactKind::Document,
            attrs: doc_attrs,
        };

        let mut snapshot_a = OntologySnapshot {
            artifacts: vec![tx.clone(), doc.clone()],
            relations: vec![Relation {
                id: relation_content_hash(
                    &doc.id,
                    &tx.id,
                    "documents_transaction",
                    &BTreeMap::new(),
                ),
                from: doc.id.clone(),
                to: tx.id.clone(),
                relation: "documents_transaction".to_string(),
                provenance: BTreeMap::new(),
            }],
        };
        let mut snapshot_b = OntologySnapshot {
            artifacts: vec![doc, tx],
            relations: snapshot_a.relations.clone(),
        };

        snapshot_a.sort_deterministic();
        snapshot_b.sort_deterministic();

        assert_eq!(snapshot_a, snapshot_b);
        assert_eq!(
            snapshot_a.to_pretty_json_stable().unwrap(),
            snapshot_b.to_pretty_json_stable().unwrap()
        );
    }

    #[test]
    fn ontology_snapshot_to_rhai_dsl_is_deterministic() {
        let mut tx_attrs = BTreeMap::new();
        tx_attrs.insert("tx_id".to_string(), "tx-001".to_string());
        let mut doc_attrs = BTreeMap::new();
        doc_attrs.insert(
            "source_ref".to_string(),
            "2023/WF statement.pdf".to_string(),
        );
        let doc = Artifact {
            id: artifact_content_hash(ArtifactKind::Document, &doc_attrs),
            kind: ArtifactKind::Document,
            attrs: doc_attrs,
        };
        let tx = Artifact {
            id: artifact_content_hash(ArtifactKind::Transaction, &tx_attrs),
            kind: ArtifactKind::Transaction,
            attrs: tx_attrs,
        };
        let mut provenance = BTreeMap::new();
        provenance.insert(
            "source_ref".to_string(),
            "2023/WF statement.pdf".to_string(),
        );
        let relation = Relation {
            id: relation_content_hash(&doc.id, &tx.id, "documents_transaction", &provenance),
            from: doc.id.clone(),
            to: tx.id.clone(),
            relation: "documents_transaction".to_string(),
            provenance,
        };
        let snapshot_a = OntologySnapshot {
            artifacts: vec![tx.clone(), doc.clone()],
            relations: vec![relation.clone()],
        };
        let snapshot_b = OntologySnapshot {
            artifacts: vec![doc, tx],
            relations: vec![relation],
        };

        let expected = format!(
            "fn document_2023_wf_statement_pdf_{}() -> relation_documents_transaction_{}\nfn relation_documents_transaction_{}() -> transaction_tx_001_{}",
            short_id(&snapshot_b.artifacts[0].id),
            short_id(&snapshot_b.relations[0].id),
            short_id(&snapshot_b.relations[0].id),
            short_id(&snapshot_b.artifacts[1].id),
        );

        assert_eq!(snapshot_a.to_rhai_dsl(), snapshot_b.to_rhai_dsl());
        assert_eq!(snapshot_a.to_rhai_dsl(), expected);
    }

    #[test]
    fn ontology_snapshot_to_rhai_dsl_marks_missing_provenance() {
        let mut tx_attrs = BTreeMap::new();
        tx_attrs.insert("tx_id".to_string(), "tx-001".to_string());
        let mut doc_attrs = BTreeMap::new();
        doc_attrs.insert("source_ref".to_string(), "statement.pdf".to_string());
        let doc = Artifact {
            id: artifact_content_hash(ArtifactKind::Document, &doc_attrs),
            kind: ArtifactKind::Document,
            attrs: doc_attrs,
        };
        let tx = Artifact {
            id: artifact_content_hash(ArtifactKind::Transaction, &tx_attrs),
            kind: ArtifactKind::Transaction,
            attrs: tx_attrs,
        };
        let relation = Relation {
            id: relation_content_hash(&doc.id, &tx.id, "documents_transaction", &BTreeMap::new()),
            from: doc.id.clone(),
            to: tx.id.clone(),
            relation: "documents_transaction".to_string(),
            provenance: BTreeMap::new(),
        };
        let snapshot = OntologySnapshot {
            artifacts: vec![doc, tx],
            relations: vec![relation],
        };

        assert!(snapshot.to_rhai_dsl().contains("missing_provenance_"));
    }
}

/// Bridge from ledger-core's ArtifactKind to arc-kit-au's NodeType.
/// Satisfies PRD-4 AC-4.1.1: canonical ontology types in ledger-core
/// can be mapped to evidence graph node types. ledger-core serves as
/// the canonical import path for NodeType.
#[cfg(feature = "arc-kit-au")]
mod arc_kit_bridge {
    use arc_kit_au::node::NodeType;

    use crate::ontology::ArtifactKind;

    impl From<ArtifactKind> for NodeType {
        fn from(kind: ArtifactKind) -> Self {
            match kind {
                ArtifactKind::Transaction => NodeType::Transaction,
                ArtifactKind::Document => NodeType::SourceDoc,
                ArtifactKind::ClassificationOutcome => NodeType::Classification,
                ArtifactKind::ValidationIssue => NodeType::ValidationIssue,
                ArtifactKind::ModelProposal => NodeType::ModelProposal,
                ArtifactKind::WorkbookRow => NodeType::WorkbookRow,
                ArtifactKind::EvidenceReference => NodeType::ExtractedRow,
                ArtifactKind::AuditEvent => NodeType::OperatorApproval,
                _ => NodeType::Unknown,
            }
        }
    }
}

#[cfg(feature = "arc-kit-au")]
pub use arc_kit_au::{
    EdgeType, EvidenceGraph, EvidenceNode, EvidenceStore, NodeId, ProvenanceBadge,
};
#[cfg(feature = "arc-kit-au")]
pub use arc_kit_au::{
    EvidenceBuilder, EvidenceChain, EvidenceTracer, ProvenanceGap, ProvenanceScanner,
};
