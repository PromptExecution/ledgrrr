use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

fn make_graph_json() -> String {
    let graph = holon_viz::CytoscapeGraph {
        nodes: vec![
            holon_viz::CytoscapeNode {
                data: holon_viz::cytoscape::CytoscapeNodeData {
                    id: "w1".into(),
                    label: "WasmNode1".into(),
                    kind: "struct".into(),
                    parent: None,
                    z_layer: Some("Pipeline".into()),
                    semantic_type: Some("Document".into()),
                },
            },
            holon_viz::CytoscapeNode {
                data: holon_viz::cytoscape::CytoscapeNodeData {
                    id: "w2".into(),
                    label: "WasmNode2".into(),
                    kind: "enum".into(),
                    parent: None,
                    z_layer: Some("Legal".into()),
                    semantic_type: None,
                },
            },
            holon_viz::CytoscapeNode {
                data: holon_viz::cytoscape::CytoscapeNodeData {
                    id: "w3".into(),
                    label: "WasmNode3".into(),
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
                    id: "we1".into(),
                    source: "w1".into(),
                    target: "w2".into(),
                    label: "implements".into(),
                },
            },
            holon_viz::CytoscapeEdge {
                data: holon_viz::cytoscape::CytoscapeEdgeData {
                    id: "we2".into(),
                    source: "w2".into(),
                    target: "w3".into(),
                    label: "verifies".into(),
                },
            },
        ],
    };
    serde_json::to_string(&graph).unwrap()
}

#[wasm_bindgen_test]
fn filter_nodes_by_text_browser() {
    let json = make_graph_json();
    let result = holon_viz_wasm::filter_nodes_by_text(&json, "node2").unwrap();
    let filtered: holon_viz::CytoscapeGraph = serde_json::from_str(&result).unwrap();
    assert!(filtered.nodes.iter().any(|n| n.data.id == "w2"));
}

#[wasm_bindgen_test]
fn filter_nodes_by_z_layer_browser() {
    let json = make_graph_json();
    let result = holon_viz_wasm::filter_nodes_by_z_layer(&json, "Legal").unwrap();
    let filtered: holon_viz::CytoscapeGraph = serde_json::from_str(&result).unwrap();
    assert!(filtered.nodes.iter().any(|n| n.data.id == "w2"));
}

#[wasm_bindgen_test]
fn filter_edges_by_label_browser() {
    let json = make_graph_json();
    let result = holon_viz_wasm::filter_edges_by_label(&json, "verifies").unwrap();
    let filtered: holon_viz::CytoscapeGraph = serde_json::from_str(&result).unwrap();
    assert_eq!(filtered.edges.len(), 1);
    assert_eq!(filtered.edges[0].data.label, "verifies");
}

#[wasm_bindgen_test]
fn get_unique_edge_labels_browser() {
    let json = make_graph_json();
    let result = holon_viz_wasm::get_unique_edge_labels(&json).unwrap();
    let labels: Vec<String> = serde_json::from_str(&result).unwrap();
    assert_eq!(labels, vec!["implements", "verifies"]);
}

#[wasm_bindgen_test]
fn get_graph_stats_browser() {
    let json = make_graph_json();
    let result = holon_viz_wasm::get_graph_stats(&json).unwrap();
    let v: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(v["node_count"], 3);
    assert_eq!(v["edge_count"], 2);
}
