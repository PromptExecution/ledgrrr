//! Cytoscape.js JSON serialization types and deterministic graph conversion.
//!
//! The output format matches the Cytoscape.js `elements` JSON format:
//! `{ nodes: [...], edges: [...] }` where each element has a `data` field.
//! See: <https://js.cytoscape.org/#notation/elements-json>

use serde::{Deserialize, Serialize};

use crate::holon::Holon;

/// Data payload for a Cytoscape.js node element.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CytoscapeNodeData {
    pub id: String,
    pub label: String,
    pub kind: String,
    /// Optional parent for compound graphs (Cytoscape compound nodes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// `ZLayer` variant from `HasVisualization::viz_spec()`, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub z_layer: Option<String>,
    /// `SemanticType` variant from `HasVisualization::viz_spec()`, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_type: Option<String>,
}

/// A single Cytoscape.js node element.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CytoscapeNode {
    pub data: CytoscapeNodeData,
}

/// Data payload for a Cytoscape.js edge element.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CytoscapeEdgeData {
    pub id: String,
    pub source: String,
    pub target: String,
    pub label: String,
}

/// A single Cytoscape.js edge element.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CytoscapeEdge {
    pub data: CytoscapeEdgeData,
}

/// Serializable Cytoscape.js graph — nodes and edges with `data` fields.
///
/// Construct via [`HolonGraph::from_holons`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CytoscapeGraph {
    pub nodes: Vec<CytoscapeNode>,
    pub edges: Vec<CytoscapeEdge>,
}

impl CytoscapeGraph {
    /// Deterministically convert a slice of [`Holon`]s into a Cytoscape graph.
    ///
    /// Each holon becomes a node. Parent-child containment becomes a directed
    /// edge with label `"contains"`. Ordering is deterministic: nodes appear
    /// in input order; edges are emitted after all nodes.
    pub fn from_holons(holons: &[Holon]) -> Self {
        let nodes: Vec<CytoscapeNode> = holons
            .iter()
            .map(|h| CytoscapeNode {
                data: CytoscapeNodeData {
                    id: h.id.clone(),
                    label: h.label.clone(),
                    kind: format!("{:?}", h.kind),
                    parent: h.parent_id.clone(),
                    z_layer: None,
                    semantic_type: None,
                },
            })
            .collect();

        let edges: Vec<CytoscapeEdge> = holons
            .iter()
            .flat_map(|h| {
                h.children.iter().map(move |child_id| {
                    // Edge ID is deterministic: parent__child
                    let edge_id = format!("{}__contains__{}", h.id, child_id);
                    CytoscapeEdge {
                        data: CytoscapeEdgeData {
                            id: edge_id,
                            source: h.id.clone(),
                            target: child_id.clone(),
                            label: "contains".to_string(),
                        },
                    }
                })
            })
            .collect();

        Self { nodes, edges }
    }

    /// Serialize to compact JSON suitable for WebSocket / HTTP delivery.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Serialize to pretty-printed JSON (useful for debug dumps).
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}
