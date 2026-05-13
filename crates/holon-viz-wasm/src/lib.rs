use std::collections::BTreeSet;

use holon_viz::CytoscapeGraph;
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[derive(Serialize, serde::Deserialize)]
struct GraphStats {
    node_count: usize,
    edge_count: usize,
    z_layers: Vec<String>,
    semantic_types: Vec<String>,
    edge_labels: Vec<String>,
}

fn parse_graph(json: &str) -> Result<CytoscapeGraph, JsValue> {
    serde_json::from_str(json).map_err(|e| JsError::new(&format!("invalid graph JSON: {e}")).into())
}

fn connected_node_ids(edges: &[holon_viz::CytoscapeEdge], matched_ids: &BTreeSet<String>) -> BTreeSet<String> {
    let mut result = matched_ids.clone();
    for edge in edges {
        if result.contains(&edge.data.source) || result.contains(&edge.data.target) {
            result.insert(edge.data.source.clone());
            result.insert(edge.data.target.clone());
        }
    }
    result
}

#[wasm_bindgen]
pub fn filter_nodes_by_text(graph_json: &str, search: &str) -> Result<String, JsValue> {
    let graph = parse_graph(graph_json)?;
    let lower = search.to_ascii_lowercase();
    let matched: BTreeSet<String> = graph
        .nodes
        .iter()
        .filter(|n| {
            n.data.label.to_ascii_lowercase().contains(&lower)
                || n.data.id.to_ascii_lowercase().contains(&lower)
        })
        .map(|n| n.data.id.clone())
        .collect();
    let alive = connected_node_ids(&graph.edges, &matched);
    let nodes: Vec<_> = graph
        .nodes
        .into_iter()
        .filter(|n| alive.contains(&n.data.id))
        .collect();
    let edges: Vec<_> = graph
        .edges
        .into_iter()
        .filter(|e| alive.contains(&e.data.source) && alive.contains(&e.data.target))
        .collect();
    serde_json::to_string(&CytoscapeGraph { nodes, edges })
        .map_err(|e| JsError::new(&format!("serialization error: {e}")).into())
}

#[wasm_bindgen]
pub fn filter_nodes_by_z_layer(graph_json: &str, z_layer: &str) -> Result<String, JsValue> {
    let graph = parse_graph(graph_json)?;
    let matched: BTreeSet<String> = graph
        .nodes
        .iter()
        .filter(|n| n.data.z_layer.as_deref() == Some(z_layer))
        .map(|n| n.data.id.clone())
        .collect();
    let alive = connected_node_ids(&graph.edges, &matched);
    let nodes: Vec<_> = graph
        .nodes
        .into_iter()
        .filter(|n| alive.contains(&n.data.id))
        .collect();
    let edges: Vec<_> = graph
        .edges
        .into_iter()
        .filter(|e| alive.contains(&e.data.source) && alive.contains(&e.data.target))
        .collect();
    serde_json::to_string(&CytoscapeGraph { nodes, edges })
        .map_err(|e| JsError::new(&format!("serialization error: {e}")).into())
}

#[wasm_bindgen]
pub fn filter_nodes_by_semantic_type(graph_json: &str, semantic_type: &str) -> Result<String, JsValue> {
    let graph = parse_graph(graph_json)?;
    let matched: BTreeSet<String> = graph
        .nodes
        .iter()
        .filter(|n| n.data.semantic_type.as_deref() == Some(semantic_type))
        .map(|n| n.data.id.clone())
        .collect();
    let alive = connected_node_ids(&graph.edges, &matched);
    let nodes: Vec<_> = graph
        .nodes
        .into_iter()
        .filter(|n| alive.contains(&n.data.id))
        .collect();
    let edges: Vec<_> = graph
        .edges
        .into_iter()
        .filter(|e| alive.contains(&e.data.source) && alive.contains(&e.data.target))
        .collect();
    serde_json::to_string(&CytoscapeGraph { nodes, edges })
        .map_err(|e| JsError::new(&format!("serialization error: {e}")).into())
}

#[wasm_bindgen]
pub fn filter_edges_by_label(graph_json: &str, label: &str) -> Result<String, JsValue> {
    let graph = parse_graph(graph_json)?;
    let lower = label.to_ascii_lowercase();
    let edges: Vec<_> = graph
        .edges
        .into_iter()
        .filter(|e| e.data.label.to_ascii_lowercase().contains(&lower))
        .collect();
    serde_json::to_string(&CytoscapeGraph {
        nodes: graph.nodes,
        edges,
    })
    .map_err(|e| JsError::new(&format!("serialization error: {e}")).into())
}

#[wasm_bindgen]
pub fn get_unique_edge_labels(graph_json: &str) -> Result<String, JsValue> {
    let graph = parse_graph(graph_json)?;
    let labels: BTreeSet<String> = graph.edges.iter().map(|e| e.data.label.clone()).collect();
    serde_json::to_string(&labels)
        .map_err(|e| JsError::new(&format!("serialization error: {e}")).into())
}

#[wasm_bindgen]
pub fn get_unique_z_layers(graph_json: &str) -> Result<String, JsValue> {
    let graph = parse_graph(graph_json)?;
    let layers: BTreeSet<String> = graph
        .nodes
        .iter()
        .filter_map(|n| n.data.z_layer.clone())
        .collect();
    serde_json::to_string(&layers)
        .map_err(|e| JsError::new(&format!("serialization error: {e}")).into())
}

#[wasm_bindgen]
pub fn get_unique_semantic_types(graph_json: &str) -> Result<String, JsValue> {
    let graph = parse_graph(graph_json)?;
    let types: BTreeSet<String> = graph
        .nodes
        .iter()
        .filter_map(|n| n.data.semantic_type.clone())
        .collect();
    serde_json::to_string(&types)
        .map_err(|e| JsError::new(&format!("serialization error: {e}")).into())
}

#[wasm_bindgen]
pub fn get_graph_stats(graph_json: &str) -> Result<String, JsValue> {
    let graph = parse_graph(graph_json)?;
    let stats = GraphStats {
        node_count: graph.nodes.len(),
        edge_count: graph.edges.len(),
        z_layers: graph
            .nodes
            .iter()
            .filter_map(|n| n.data.z_layer.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        semantic_types: graph
            .nodes
            .iter()
            .filter_map(|n| n.data.semantic_type.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        edge_labels: graph
            .edges
            .iter()
            .map(|e| e.data.label.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
    };
    serde_json::to_string(&stats)
        .map_err(|e| JsError::new(&format!("serialization error: {e}")).into())
}

#[cfg(test)]
fn make_graph_json() -> String {
    let graph = CytoscapeGraph {
        nodes: vec![
            holon_viz::CytoscapeNode {
                data: holon_viz::cytoscape::CytoscapeNodeData {
                    id: "n1".into(),
                    label: "Alpha".into(),
                    kind: "struct".into(),
                    parent: None,
                    z_layer: Some("Pipeline".into()),
                    semantic_type: Some("Document".into()),
                },
            },
            holon_viz::CytoscapeNode {
                data: holon_viz::cytoscape::CytoscapeNodeData {
                    id: "n2".into(),
                    label: "Beta".into(),
                    kind: "enum".into(),
                    parent: None,
                    z_layer: Some("Constraint".into()),
                    semantic_type: None,
                },
            },
            holon_viz::CytoscapeNode {
                data: holon_viz::cytoscape::CytoscapeNodeData {
                    id: "n3".into(),
                    label: "Gamma".into(),
                    kind: "trait".into(),
                    parent: None,
                    z_layer: None,
                    semantic_type: Some("FormalProof".into()),
                },
            },
        ],
        edges: vec![
            holon_viz::CytoscapeEdge {
                data: holon_viz::cytoscape::CytoscapeEdgeData {
                    id: "e1".into(),
                    source: "n1".into(),
                    target: "n2".into(),
                    label: "contains".into(),
                },
            },
            holon_viz::CytoscapeEdge {
                data: holon_viz::cytoscape::CytoscapeEdgeData {
                    id: "e2".into(),
                    source: "n2".into(),
                    target: "n3".into(),
                    label: "references".into(),
                },
            },
        ],
    };
    serde_json::to_string(&graph).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_nodes_by_text_finds_label_match() {
        let json = make_graph_json();
        let result = filter_nodes_by_text(&json, "alpha").unwrap();
        let filtered: CytoscapeGraph = serde_json::from_str(&result).unwrap();
        assert_eq!(filtered.nodes.len(), 3);
        assert!(filtered.edges.len() >= 1);
    }

    #[test]
    fn filter_nodes_by_text_finds_id_match() {
        let json = make_graph_json();
        let result = filter_nodes_by_text(&json, "n3").unwrap();
        let filtered: CytoscapeGraph = serde_json::from_str(&result).unwrap();
        assert!(filtered.nodes.iter().any(|n| n.data.id == "n3"));
    }

    #[test]
    fn filter_nodes_by_z_layer_works() {
        let json = make_graph_json();
        let result = filter_nodes_by_z_layer(&json, "Pipeline").unwrap();
        let filtered: CytoscapeGraph = serde_json::from_str(&result).unwrap();
        assert!(filtered.nodes.iter().any(|n| n.data.id == "n1"));
    }

    #[test]
    fn filter_edges_by_label_works() {
        let json = make_graph_json();
        let result = filter_edges_by_label(&json, "contains").unwrap();
        let filtered: CytoscapeGraph = serde_json::from_str(&result).unwrap();
        assert_eq!(filtered.edges.len(), 1);
        assert_eq!(filtered.edges[0].data.label, "contains");
    }

    #[test]
    fn get_unique_edge_labels_works() {
        let json = make_graph_json();
        let result = get_unique_edge_labels(&json).unwrap();
        let labels: Vec<String> = serde_json::from_str(&result).unwrap();
        assert_eq!(labels, vec!["contains", "references"]);
    }

    #[test]
    fn get_graph_stats_works() {
        let json = make_graph_json();
        let result = get_graph_stats(&json).unwrap();
        let stats: GraphStats = serde_json::from_str(&result).unwrap();
        assert_eq!(stats.node_count, 3);
        assert_eq!(stats.edge_count, 2);
    }
}
