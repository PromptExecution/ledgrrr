//! Typed Rust mirror of `reqif-opa-mcp`'s `DocumentGraph` JSON schema
//! (`reqif_ingest_cli/models.py`), so extraction output — whether piped
//! through a subprocess or returned by the `ledgrrr-docling` NATS Micro
//! Service (`reqif_ingest_cli/nats_docling_service.py`) — can be
//! deserialized directly instead of re-parsed field-by-field ad hoc.
//!
//! Supersedes `rule_registry::DocumentChunk`, which declared a similar
//! intent ("maps to reqif-opa-mcp's DocumentNode") but was never actually
//! deserialized anywhere and doesn't match the real anchor shape (the
//! Python `SourceAnchor` dataclass has named fields — page/sheet/row/
//! column/cell/paragraph/heading_path/semantic_id — not a `[u32; 2]` pair).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Mirrors `reqif_ingest_cli.models.ArtifactRecord`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DoclingArtifact {
    pub artifact_id: String,
    pub source_uri: Option<String>,
    pub sha256: String,
    pub document_profile: String,
}

/// Mirrors `reqif_ingest_cli.models.SourceAnchor`. Every field beyond `kind`
/// is format-specific (PDF pages vs. XLSX cells) so all are optional.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DoclingAnchor {
    pub kind: String,
    pub artifact_id: String,
    #[serde(default)]
    pub page: Option<u32>,
    #[serde(default)]
    pub sheet: Option<String>,
    #[serde(default)]
    pub row: Option<u32>,
    #[serde(default)]
    pub column: Option<u32>,
    #[serde(default)]
    pub cell: Option<String>,
    #[serde(default)]
    pub paragraph: Option<u32>,
    #[serde(default)]
    pub heading_path: Vec<String>,
    #[serde(default)]
    pub semantic_id: String,
}

/// Mirrors `reqif_ingest_cli.models.DocumentNode`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DoclingNode {
    pub node_id: String,
    pub node_type: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    pub semantic_id: String,
    #[serde(default)]
    pub attributes: HashMap<String, Value>,
    #[serde(default)]
    pub anchors: Vec<DoclingAnchor>,
}

impl DoclingNode {
    /// First page number among this node's anchors, if any.
    pub fn first_page(&self) -> Option<u32> {
        self.anchors.iter().find_map(|a| a.page)
    }

    /// Heading path of this node's first anchor, if any — the closest
    /// analogue to a "section" for a PDF-sourced node.
    pub fn heading_path(&self) -> &[String] {
        self.anchors.first().map(|a| a.heading_path.as_slice()).unwrap_or(&[])
    }
}

/// Mirrors `reqif_ingest_cli.models.DocumentGraph` — the full JSON payload
/// returned by `reqif_ingest_cli extract` and by the NATS service's
/// `ledgrrr.extract` endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DoclingDocumentGraph {
    pub schema: String,
    pub artifact: DoclingArtifact,
    pub profile: String,
    pub nodes: Vec<DoclingNode>,
    #[serde(default)]
    pub metadata: HashMap<String, Value>,
}

impl DoclingDocumentGraph {
    /// Iterate only nodes that carry non-empty text — the vast majority of
    /// classification/extraction logic operates on these.
    pub fn text_nodes(&self) -> impl Iterator<Item = &DoclingNode> {
        self.nodes
            .iter()
            .filter(|n| n.text.as_deref().is_some_and(|t| !t.trim().is_empty()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real shape observed from a live `reqif_ingest_cli extract --pretty`
    /// run against samples/standards/upstream/owasp-asvs/OWASP_ASVS_5.0.0_en.pdf
    /// (2026-08-23), trimmed to one node.
    const SAMPLE_JSON: &str = r#"{
        "schema": "document_graph/1",
        "artifact": {
            "artifact_id": "artifact-d92ba680322b",
            "source_uri": null,
            "sha256": "deadbeef",
            "document_profile": "pdf_docling_v1"
        },
        "profile": "pdf_docling_v1",
        "nodes": [
            {
                "node_id": "paragraph-abc123",
                "node_type": "paragraph",
                "text": "05/01 Check 1042 -$120.00 $4,880.00",
                "parent_id": null,
                "semantic_id": "semantic-eac1ea760e01",
                "attributes": {"label": "pdf_text", "extractor": "pypdf"},
                "anchors": [
                    {
                        "kind": "pdf_page_paragraph",
                        "artifact_id": "artifact-d92ba680322b",
                        "page": 2,
                        "sheet": null,
                        "row": null,
                        "column": null,
                        "cell": null,
                        "paragraph": 1,
                        "heading_path": [],
                        "semantic_id": "semantic-eac1ea760e01"
                    }
                ]
            }
        ],
        "metadata": {"extractor": "pypdf", "fallback_reason": null}
    }"#;

    #[test]
    fn deserializes_real_extraction_shape() {
        let graph: DoclingDocumentGraph = serde_json::from_str(SAMPLE_JSON).unwrap();
        assert_eq!(graph.profile, "pdf_docling_v1");
        assert_eq!(graph.nodes.len(), 1);
        let node = &graph.nodes[0];
        assert_eq!(node.first_page(), Some(2));
        assert!(node.text.as_deref().unwrap().contains("Check 1042"));
    }

    #[test]
    fn text_nodes_filters_empty() {
        let mut graph: DoclingDocumentGraph = serde_json::from_str(SAMPLE_JSON).unwrap();
        graph.nodes.push(DoclingNode {
            node_id: "empty".into(),
            node_type: "paragraph".into(),
            text: Some("   ".into()),
            parent_id: None,
            semantic_id: "empty".into(),
            attributes: HashMap::new(),
            anchors: vec![],
        });
        assert_eq!(graph.text_nodes().count(), 1);
    }
}
