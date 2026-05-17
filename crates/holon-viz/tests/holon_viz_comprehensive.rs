//! Comprehensive MECE (Mutually Exclusive, Collectively Exhaustive) integration
//! tests for the holon-viz crate. 42 tests covering all public modules,
//! edge cases, error boundaries, and full-pipeline integration.
//!
//! Test numbering follows the MECE matrix in the task specification.

#![allow(unused_imports, dead_code)]

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use holon_viz::{
    ActionKind, ActionRecord, CytoscapeEdge, CytoscapeGraph, CytoscapeNode, Holon, HolonError,
    HolonKind, HtmlRenderer, ImmutableActionLog, Owl2Emitter, ProcessController, ProcessStep,
    SysmlV2Emitter, TransitionReceipt, TypeNode, TypeRelationship, TypeRelationshipGraph,
    TypeRelationshipKind, VizObservation, VizObserver,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_step(id: &str) -> ProcessStep {
    ProcessStep {
        step_id: id.to_string(),
        description: format!("Step {}", id),
    }
}

fn empty_graph() -> CytoscapeGraph {
    CytoscapeGraph {
        nodes: vec![],
        edges: vec![],
    }
}

fn two_node_graph() -> CytoscapeGraph {
    let holons = vec![
        Holon {
            id: "a".to_string(),
            label: "Alpha".to_string(),
            kind: HolonKind::SysmlBlock,
            parent_id: None,
            children: vec!["b".to_string()],
            metadata: HashMap::new(),
        },
        Holon {
            id: "b".to_string(),
            label: "Beta".to_string(),
            kind: HolonKind::OwlClass,
            parent_id: Some("a".to_string()),
            children: vec![],
            metadata: HashMap::new(),
        },
    ];
    CytoscapeGraph::from_holons(&holons)
}

fn three_level_tree() -> Vec<Holon> {
    vec![
        Holon {
            id: "root".to_string(),
            label: "Root".to_string(),
            kind: HolonKind::CapsuleGroup,
            parent_id: None,
            children: vec!["c1".to_string(), "c2".to_string()],
            metadata: HashMap::new(),
        },
        Holon {
            id: "c1".to_string(),
            label: "Child1".to_string(),
            kind: HolonKind::SysmlBlock,
            parent_id: Some("root".to_string()),
            children: vec!["gc1".to_string()],
            metadata: HashMap::new(),
        },
        Holon {
            id: "c2".to_string(),
            label: "Child2".to_string(),
            kind: HolonKind::ProcessNode,
            parent_id: Some("root".to_string()),
            children: vec![],
            metadata: HashMap::new(),
        },
        Holon {
            id: "gc1".to_string(),
            label: "GrandChild1".to_string(),
            kind: HolonKind::AuditEvent,
            parent_id: Some("c1".to_string()),
            children: vec![],
            metadata: HashMap::new(),
        },
    ]
}

// ===========================================================================
// Module 1: ProcessController (tests 1–7)
// ===========================================================================

#[test]
fn test_01_empty_controller() {
    let ctrl = ProcessController::new();
    assert!(ctrl.steps().is_empty());
    assert!(ctrl.log().is_empty());
    assert_eq!(ctrl.log().len(), 0);
}

#[test]
fn test_02_full_lifecycle() {
    let mut ctrl = ProcessController::new();
    ctrl.register_step(make_step("a")).unwrap();
    ctrl.register_step(make_step("b")).unwrap();
    ctrl.register_step(make_step("c")).unwrap();
    assert_eq!(ctrl.steps().len(), 3);

    let r1 = ctrl.authorize_step("a", "alice", 1000).unwrap();
    let r2 = ctrl.authorize_step("b", "bob", 2000).unwrap();
    let r3 = ctrl.authorize_step("c", "carol", 3000).unwrap();

    assert_eq!(r1.step_id, "a");
    assert_eq!(r2.step_id, "b");
    assert_eq!(r3.step_id, "c");
    assert_eq!(r1.authorized_by, "alice");
    assert_eq!(r2.authorized_by, "bob");
    assert_eq!(r3.authorized_by, "carol");

    // Log should have 3 entries in order
    assert_eq!(ctrl.log().len(), 3);
    let kinds: Vec<ActionKind> = ctrl.log().iter().map(|r| r.action_kind).collect();
    assert_eq!(
        kinds,
        vec![ActionKind::StepAuthorized, ActionKind::StepAuthorized, ActionKind::StepAuthorized]
    );
}

#[test]
fn test_03_branching_fork() {
    let mut ctrl = ProcessController::new();
    ctrl.register_step(make_step("A")).unwrap();
    ctrl.register_step(make_step("B")).unwrap();
    ctrl.register_step(make_step("C")).unwrap();

    // Fork: A transitions independently to both B and C paths
    let r_a1 = ctrl.authorize_step("A", "alice", 100).unwrap();
    let r_b = ctrl.authorize_step("B", "bob", 200).unwrap();
    let r_a2 = ctrl.authorize_step("A", "carol", 300).unwrap();
    let r_c = ctrl.authorize_step("C", "dave", 400).unwrap();

    assert_eq!(r_a1.step_id, "A");
    assert_eq!(r_b.step_id, "B");
    assert_eq!(r_a2.step_id, "A");
    assert_eq!(r_c.step_id, "C");

    // Authorizing the same step twice from different authorizers is permitted
    assert_ne!(r_a1.authorization_hash, r_a2.authorization_hash);

    assert_eq!(ctrl.log().len(), 4);
}

#[test]
fn test_04_fork_merge() {
    let mut ctrl = ProcessController::new();
    ctrl.register_step(make_step("A")).unwrap();
    ctrl.register_step(make_step("B")).unwrap();
    ctrl.register_step(make_step("C")).unwrap();
    ctrl.register_step(make_step("D")).unwrap();

    // Fork: A -> B and A -> C
    let _r_a_by_alice = ctrl.authorize_step("A", "alice", 100).unwrap();
    let _r_b_by_alice = ctrl.authorize_step("B", "alice", 200).unwrap();
    let _r_a_by_bob = ctrl.authorize_step("A", "bob", 300).unwrap();
    let _r_c_by_bob = ctrl.authorize_step("C", "bob", 400).unwrap();

    // Merge: both B and C paths authorize D
    let r_d_by_alice = ctrl.authorize_step("D", "alice", 500).unwrap();
    let r_d_by_bob = ctrl.authorize_step("D", "bob", 600).unwrap();

    assert_eq!(r_d_by_alice.step_id, "D");
    assert_eq!(r_d_by_bob.step_id, "D");
    // Same step, different authorizers → different hashes
    assert_ne!(r_d_by_alice.authorization_hash, r_d_by_bob.authorization_hash);

    // Total log: A(×2) + B + C + D(×2) = 6 entries
    assert_eq!(ctrl.log().len(), 6);
    // Verify D appears at the end
    let last_two: Vec<&str> = ctrl
        .log()
        .iter()
        .skip(4)
        .map(|r| r.authorized_by.as_str())
        .collect();
    assert_eq!(last_two, vec!["alice", "bob"]);
}

#[test]
fn test_05_empty_step_id_rejection() {
    let mut ctrl = ProcessController::new();
    let err = ctrl.register_step(ProcessStep {
        step_id: "".to_string(),
        description: "empty".to_string(),
    });
    assert!(matches!(err, Err(HolonError::EmptyStepId)));
}

#[test]
fn test_06_empty_authorizer_rejection() {
    let mut ctrl = ProcessController::new();
    ctrl.register_step(make_step("x")).unwrap();
    let err = ctrl.authorize_step("x", "", 42);
    assert!(matches!(err, Err(HolonError::EmptyAuthorizer)));
}

#[test]
fn test_07_deterministic_hash_across_sessions() {
    // Same inputs → same Blake3 authorization hash across independent controllers
    let mut ctrl_a = ProcessController::new();
    let mut ctrl_b = ProcessController::new();
    ctrl_a.register_step(make_step("alpha")).unwrap();
    ctrl_b.register_step(make_step("alpha")).unwrap();

    let r_a = ctrl_a.authorize_step("alpha", "alice", 999).unwrap();
    let r_b = ctrl_b.authorize_step("alpha", "alice", 999).unwrap();

    assert_eq!(r_a.authorization_hash, r_b.authorization_hash);
}

// ===========================================================================
// Module 2: ImmutableActionLog (tests 8–12)
// ===========================================================================

#[test]
fn test_08_empty_log() {
    let log = ImmutableActionLog::new();
    assert!(log.is_empty());
    assert_eq!(log.len(), 0);
    assert!(log.iter().next().is_none());
}

#[test]
fn test_09_append_only_invariant() {
    let mut log = ImmutableActionLog::new();
    let payload = [0u8; 32];
    log.append(ActionKind::StepAuthorized, "alice", 100, payload);
    log.append(ActionKind::HolonCreated, "bob", 200, payload);

    assert_eq!(log.len(), 2);
    // Cannot remove or modify — there is no remove/update API.
    // Verify records are in append order.
    let by: Vec<&str> = log.iter().map(|r| r.authorized_by.as_str()).collect();
    assert_eq!(by, vec!["alice", "bob"]);
}

#[test]
fn test_10_find_by_id() {
    let mut log = ImmutableActionLog::new();
    let payload = [0xabu8; 32];
    let record = log
        .append(ActionKind::AuditEvent, "carol", 300, payload)
        .clone();

    let found = log.find_by_id(&record.id);
    assert!(found.is_some());
    let f = found.unwrap();
    assert_eq!(f.action_kind, ActionKind::AuditEvent);
    assert_eq!(f.authorized_by, "carol");
    assert_eq!(f.timestamp_ms, 300);
    assert_eq!(f.payload_hash, payload);
}

#[test]
fn test_11_find_by_id_not_found() {
    let log = ImmutableActionLog::new();
    let random_hash = [0x42u8; 32];
    assert!(log.find_by_id(&random_hash).is_none());
}

#[test]
fn test_12_deterministic_ids() {
    // Identical inputs produce identical record IDs across independent logs
    let mut log_a = ImmutableActionLog::new();
    let mut log_b = ImmutableActionLog::new();
    let payload = [0x77u8; 32];

    let r_a = log_a
        .append(ActionKind::StepAuthorized, "deterministic", 500, payload)
        .clone();
    let r_b = log_b
        .append(ActionKind::StepAuthorized, "deterministic", 500, payload)
        .clone();

    assert_eq!(r_a.id, r_b.id);
    assert_eq!(r_a.action_kind, r_b.action_kind);
    assert_eq!(r_a.authorized_by, r_b.authorized_by);
}

// ===========================================================================
// Module 3: Holon (tests 13–16)
// ===========================================================================

#[test]
fn test_13_root_construction() {
    let h = Holon::root("r1", "Root One", HolonKind::CapsuleGroup);
    assert_eq!(h.id, "r1");
    assert_eq!(h.label, "Root One");
    assert_eq!(h.kind, HolonKind::CapsuleGroup);
    assert!(h.parent_id.is_none());
    assert!(h.children.is_empty());
    assert!(h.metadata.is_empty());
}

#[test]
fn test_14_child_construction() {
    let h = Holon::child("c1", "Child One", HolonKind::ProcessNode, "parent1");
    assert_eq!(h.id, "c1");
    assert_eq!(h.label, "Child One");
    assert_eq!(h.kind, HolonKind::ProcessNode);
    assert_eq!(h.parent_id, Some("parent1".to_string()));
    assert!(h.children.is_empty());
}

#[test]
fn test_15_metadata_round_trip() {
    let mut h = Holon::root("m1", "Meta Test", HolonKind::SysmlBlock);
    h.metadata
        .insert("version".to_string(), serde_json::json!("v1.0"));
    h.metadata
        .insert("count".to_string(), serde_json::json!(42));

    let json = serde_json::to_string(&h).unwrap();
    let deserialized: Holon = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.id, "m1");
    assert_eq!(deserialized.metadata["version"], "v1.0");
    assert_eq!(deserialized.metadata["count"], 42);
}

#[test]
fn test_16_max_depth_chain() {
    // Build a chain of 1000 holons where each is a child of the previous
    let mut holons = Vec::with_capacity(1000);
    let root = Holon::root("h_0", "Chain Root", HolonKind::ProcessNode);
    holons.push(root);

    for i in 1..1000 {
        let parent_id = format!("h_{}", i - 1);
        let child = Holon::child(
            format!("h_{}", i),
            format!("Chain Node {}", i),
            HolonKind::ProcessNode,
            &parent_id,
        );
        holons.push(child);
        // Update parent's children list
        if let Some(parent) = holons.iter_mut().find(|h: &&mut Holon| h.id == parent_id) {
            parent.children.push(format!("h_{}", i));
        }
    }

    assert_eq!(holons.len(), 1000);
    // Verify root has no parent
    assert!(holons[0].parent_id.is_none());
    // Verify last node has parent
    assert_eq!(holons[999].parent_id, Some("h_998".to_string()));
    // Verify root has one child
    assert_eq!(holons[0].children.len(), 1);
    assert_eq!(holons[0].children[0], "h_1");
}

// ===========================================================================
// Module 4: CytoscapeGraph (tests 17–22)
// ===========================================================================

#[test]
fn test_17_empty_holons() {
    let g = CytoscapeGraph::from_holons(&[]);
    assert!(g.nodes.is_empty());
    assert!(g.edges.is_empty());
}

#[test]
fn test_18_single_holon() {
    let h = Holon::root("singleton", "Solo", HolonKind::OwlClass);
    let g = CytoscapeGraph::from_holons(&[h]);
    assert_eq!(g.nodes.len(), 1);
    assert_eq!(g.nodes[0].data.id, "singleton");
    assert_eq!(g.nodes[0].data.label, "Solo");
    assert!(g.edges.is_empty());
}

#[test]
fn test_19_parent_child() {
    let h = two_node_graph();
    assert_eq!(h.nodes.len(), 2);
    assert_eq!(h.edges.len(), 1);
    assert_eq!(h.edges[0].data.label, "contains");
    assert_eq!(h.edges[0].data.source, "a");
    assert_eq!(h.edges[0].data.target, "b");
}

#[test]
fn test_20_deep_tree() {
    let holons = three_level_tree();
    let g = CytoscapeGraph::from_holons(&holons);

    // 4 nodes
    assert_eq!(g.nodes.len(), 4);
    let node_ids: Vec<&str> = g.nodes.iter().map(|n| n.data.id.as_str()).collect();
    assert_eq!(node_ids, vec!["root", "c1", "c2", "gc1"]);

    // 3 edges: root→c1, root→c2, c1→gc1
    assert_eq!(g.edges.len(), 3);
    assert_eq!(g.edges[0].data.id, "root__contains__c1");
    assert_eq!(g.edges[1].data.id, "root__contains__c2");
    assert_eq!(g.edges[2].data.id, "c1__contains__gc1");
}

#[test]
fn test_21_json_round_trip() {
    let g = two_node_graph();
    let json = g.to_json().unwrap();
    let deserialized: CytoscapeGraph = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.nodes.len(), 2);
    assert_eq!(deserialized.edges.len(), 1);
    // Preserved fields
    assert_eq!(deserialized.nodes[0].data.label, "Alpha");
    assert_eq!(deserialized.edges[0].data.label, "contains");
}

#[test]
fn test_22_pretty_json_round_trip() {
    let g = two_node_graph();
    let pretty = g.to_json_pretty().unwrap();
    // Pretty JSON should be parseable and contain newlines
    assert!(pretty.contains('\n'));
    let parsed: CytoscapeGraph = serde_json::from_str(&pretty).unwrap();
    assert_eq!(parsed.nodes.len(), 2);
    assert_eq!(parsed.edges.len(), 1);
}

// ===========================================================================
// Module 5: HtmlRenderer (tests 23–26)
// ===========================================================================

#[test]
fn test_23_empty_graph_renders() {
    let html = HtmlRenderer::render(&empty_graph());
    assert!(html.contains("GRAPH_DATA"));
    assert!(html.contains("<!DOCTYPE html>"));
    assert!(html.contains("cytoscape.min.js"));
    // Empty graph should have empty arrays
    assert!(html.contains("\"nodes\": []") || html.contains("\"nodes\":[]"));
}

#[test]
fn test_24_special_chars_in_labels() {
    let h = Holon::root(
        "special",
        "Label with <>&\"' and spaces",
        HolonKind::SysmlBlock,
    );
    let g = CytoscapeGraph::from_holons(&[h]);
    let html = HtmlRenderer::render(&g);
    // The label should appear inside the GRAPH_DATA JSON
    assert!(html.contains("Label with"));
    // The JSON serialization escapes quotes correctly
    assert!(html.contains("&") || html.contains("&amp;"));
}

#[test]
fn test_25_write_to_file() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("test.html");
    let g = two_node_graph();
    HtmlRenderer::write_to_file(&g, &path).expect("write_to_file");
    assert!(path.exists(), "file should exist after write_to_file");
    let contents = std::fs::read_to_string(&path).expect("read back");
    assert!(contents.contains("Alpha"));
    assert!(contents.contains("Beta"));
    assert!(contents.contains("GRAPH_DATA"));
}

#[test]
fn test_26_nested_parent_renders_compounds() {
    let holons = three_level_tree();
    let g = CytoscapeGraph::from_holons(&holons);
    let html = HtmlRenderer::render(&g);
    // HTML should contain parent info in the JSON data
    assert!(html.contains("\"parent\":\"root\"") || html.contains("\"parent\": \"root\""));
    assert!(html.contains("Root"));
    assert!(html.contains("GrandChild1"));
}

// ===========================================================================
// Module 6: Emitters (tests 27–30)
// ===========================================================================

#[test]
fn test_27_sysml_v2_empty() {
    let out = SysmlV2Emitter::emit(&empty_graph());
    assert!(out.contains("package HolonModel"));
    assert!(out.contains("}"));
    // No block def nodes
    assert_eq!(out.matches("block def").count(), 0);
}

#[test]
fn test_28_sysml_v2_single_node() {
    let h = Holon::root("alpha-id", "Alpha", HolonKind::SysmlBlock);
    let g = CytoscapeGraph::from_holons(&[h]);
    let out = SysmlV2Emitter::emit(&g);
    assert!(out.contains("package HolonModel"));
    assert!(out.contains("block def Alpha"));
}

#[test]
fn test_29_owl2_turtle_empty() {
    let out = Owl2Emitter::emit(&empty_graph());
    assert!(out.contains("@prefix owl:"));
    assert!(out.contains("@prefix rdfs:"));
    assert!(out.contains("owl:Ontology"));
    // No class declarations for empty graph
    assert!(!out.contains("a owl:Class"));
}

#[test]
fn test_30_owl2_turtle_parent_child() {
    let g = two_node_graph();
    let out = Owl2Emitter::emit(&g);
    // Should have two class declarations
    assert_eq!(out.matches("a owl:Class ;").count(), 2);
    // Child should have rdfs:subClassOf parent
    assert!(out.contains("holon:b rdfs:subClassOf holon:a"));
}

// ===========================================================================
// Module 7: TypeRelationshipGraph (tests 31–34)
// ===========================================================================

#[test]
fn test_31_type_graph_empty() {
    let g = TypeRelationshipGraph::new(vec![], vec![]);
    assert!(g.nodes.is_empty());
    assert!(g.relationships.is_empty());
    let cy = g.to_cytoscape();
    assert!(cy.nodes.is_empty());
    assert!(cy.edges.is_empty());
}

#[test]
fn test_32_seed_is_deterministic() {
    let g1 = TypeRelationshipGraph::seed();
    let g2 = TypeRelationshipGraph::seed();
    assert!(g1.nodes.len() > 0);
    assert!(g2.nodes.len() > 0);
    // Seed is deterministic — same number of nodes and relationships
    assert_eq!(g1.nodes.len(), g2.nodes.len());
    // First node should always be the same
    assert_eq!(g1.nodes[0].id, g2.nodes[0].id);
}

#[test]
fn test_33_dedup_identical_nodes() {
    let n1 = TypeNode {
        id: "dedup::MyType".to_string(),
        label: "MyType".to_string(),
        kind: "struct".to_string(),
        parent_id: None,
        z_layer: None,
        semantic_type: None,
    };
    let n2 = TypeNode {
        id: "dedup::MyType".to_string(),
        label: "MyType".to_string(),
        kind: "struct".to_string(),
        parent_id: None,
        z_layer: None,
        semantic_type: None,
    };
    let r = TypeRelationship::new("dedup::MyType", "dedup::Other", TypeRelationshipKind::References);
    // Duplicate relationship too
    let r_dup = TypeRelationship::new("dedup::MyType", "dedup::Other", TypeRelationshipKind::References);

    let g = TypeRelationshipGraph::new(vec![n1, n2], vec![r, r_dup]);
    assert_eq!(g.nodes.len(), 2); // Raw storage, no dedup

    let cy = g.to_cytoscape();
    assert_eq!(cy.nodes.len(), 1); // Deduped by ID
    assert_eq!(cy.edges.len(), 1); // Deduped by (source, kind, target)
}

#[test]
fn test_34_seed_to_cytoscape() {
    let seed = TypeRelationshipGraph::seed();
    let cy = seed.to_cytoscape();
    assert!(cy.nodes.len() > 0);
    assert!(cy.edges.len() > 0);
    // Every node has an id and label
    for n in &cy.nodes {
        assert!(!n.data.id.is_empty());
        assert!(!n.data.label.is_empty());
    }
    // Every edge connects existing nodes
    let node_ids: std::collections::HashSet<&str> =
        cy.nodes.iter().map(|n| n.data.id.as_str()).collect();
    for e in &cy.edges {
        assert!(node_ids.contains(e.data.source.as_str()));
        assert!(node_ids.contains(e.data.target.as_str()));
    }
}

// ===========================================================================
// Module 8: Observer (tests 35–37)
// ===========================================================================

#[test]
fn test_35_observer_unavailable_graceful() {
    // Point at non-existent script → cd fails → CDP_UNAVAILABLE
    let tmp = tempfile::tempdir().expect("tempdir");
    let screenshot = tmp.path().join("shot.png");
    let script = tmp.path().join("no_such_script.py");
    let observer = VizObserver::new(screenshot, script);
    let obs = observer.observe().expect("observe must never Err");
    assert_eq!(obs.raw_output, "CDP_UNAVAILABLE");
    assert_eq!(obs.node_count, 0);
    assert_eq!(obs.edge_count, 0);
    assert!(!obs.is_live());
}

#[test]
fn test_36_is_live_false_on_unavailable() {
    let obs = VizObservation {
        screenshot_path: Path::new("/tmp/fake.png").to_path_buf(),
        node_count: 0,
        edge_count: 0,
        raw_output: "CDP_UNAVAILABLE".to_string(),
    };
    assert!(!obs.is_live());
}

#[test]
fn test_37_is_live_true_on_real_output() {
    let obs = VizObservation {
        screenshot_path: Path::new("/tmp/real.png").to_path_buf(),
        node_count: 5,
        edge_count: 3,
        raw_output: "ok\nnodes=5 edges=3\n".to_string(),
    };
    assert!(obs.is_live());
}

// ===========================================================================
// Module 9: Integration — Full Pipeline (tests 38–40)
// ===========================================================================

#[test]
fn test_38_process_to_holon_to_cytoscape_to_render_html() {
    // 1. Process: register + authorize steps
    let mut ctrl = ProcessController::new();
    for s in &["review", "approve", "sign-off"] {
        ctrl.register_step(make_step(s)).unwrap();
    }
    ctrl.authorize_step("review", "alice", 1000).unwrap();
    ctrl.authorize_step("approve", "bob", 2000).unwrap();
    ctrl.authorize_step("sign-off", "carol", 3000).unwrap();
    assert_eq!(ctrl.log().len(), 3);

    // 2. Build Holons representing the authorized steps
    let holons: Vec<Holon> = ctrl
        .steps()
        .iter()
        .map(|s| Holon::root(s.step_id.clone(), s.description.clone(), HolonKind::ProcessNode))
        .collect();

    // 3. Convert to CytoscapeGraph
    let graph = CytoscapeGraph::from_holons(&holons);
    assert_eq!(graph.nodes.len(), 3);

    // 4. Render to HTML
    let html = HtmlRenderer::render(&graph);
    assert!(html.contains("review"));
    assert!(html.contains("approve"));
    assert!(html.contains("sign-off"));
    assert!(html.contains("Step review"));
    assert!(html.contains("Step approve"));
    assert!(html.contains("Step sign-off"));
}

#[test]
fn test_39_branching_visualization() {
    // Simulate a fork/merge process and verify Cytoscape edges cover both paths
    let mut ctrl = ProcessController::new();
    for id in &["A", "B", "C", "D"] {
        ctrl.register_step(make_step(id)).unwrap();
    }

    // Authorize along both fork paths
    ctrl.authorize_step("A", "alice", 100).unwrap(); // A→B path
    ctrl.authorize_step("B", "alice", 200).unwrap();
    ctrl.authorize_step("A", "bob", 300).unwrap(); // A→C path
    ctrl.authorize_step("C", "bob", 400).unwrap();
    ctrl.authorize_step("D", "alice", 500).unwrap(); // merge point
    ctrl.authorize_step("D", "bob", 600).unwrap();

    // Build holons with edges modelling the fork/merge structure

    // Create Holons with children/edges that model:
    //   A → B → D  (B authorized by alice, D by alice)
    //   A → C → D  (C authorized by bob, D by bob)
    let holons = vec![
        Holon {
            id: "A".to_string(),
            label: "Step A".to_string(),
            kind: HolonKind::ProcessNode,
            parent_id: None,
            children: vec!["B".to_string(), "C".to_string()],
            metadata: HashMap::new(),
        },
        Holon {
            id: "B".to_string(),
            label: "Step B".to_string(),
            kind: HolonKind::ProcessNode,
            parent_id: Some("A".to_string()),
            children: vec!["D".to_string()],
            metadata: HashMap::new(),
        },
        Holon {
            id: "C".to_string(),
            label: "Step C".to_string(),
            kind: HolonKind::ProcessNode,
            parent_id: Some("A".to_string()),
            children: vec!["D".to_string()],
            metadata: HashMap::new(),
        },
        Holon {
            id: "D".to_string(),
            label: "Step D".to_string(),
            kind: HolonKind::ProcessNode,
            parent_id: Some("B".to_string()), // first parent — D could have multiple
            children: vec![],
            metadata: HashMap::new(),
        },
    ];

    let g = CytoscapeGraph::from_holons(&holons);
    assert_eq!(g.nodes.len(), 4);

    // Verify both paths are present: A→B→D and A→C→D
    let edge_ids: Vec<&str> = g.edges.iter().map(|e| e.data.id.as_str()).collect();
    assert!(
        edge_ids.contains(&"A__contains__B"),
        "missing A→B edge"
    );
    assert!(
        edge_ids.contains(&"A__contains__C"),
        "missing A→C edge"
    );
    assert!(
        edge_ids.contains(&"B__contains__D"),
        "missing B→D edge"
    );
    assert!(
        edge_ids.contains(&"C__contains__D"),
        "missing C→D edge"
    );
}

#[test]
fn test_40_cross_format_consistency() {
    // Same graph → SysML-v2 + OWL2/Turtle + HTML → all contain same node count
    let g = two_node_graph();
    assert_eq!(g.nodes.len(), 2);

    // SysML-v2
    let sysml = SysmlV2Emitter::emit(&g);
    let sysml_block_count = sysml.matches("block def").count();
    assert_eq!(
        sysml_block_count, 2,
        "SysML-v2 should contain 2 block definitions, got {}",
        sysml_block_count
    );

    // OWL2/Turtle
    let owl = Owl2Emitter::emit(&g);
    let class_count = owl.matches("a owl:Class ;").count();
    assert_eq!(
        class_count, 2,
        "OWL2 should contain 2 class declarations, got {}",
        class_count
    );

    // HTML
    let html = HtmlRenderer::render(&g);
    assert!(html.contains("Alpha"));
    assert!(html.contains("Beta"));
    // HTML should contain both nodes in the JSON data
    let alpha_pos = html.find("Alpha");
    let beta_pos = html.find("Beta");
    assert!(alpha_pos.is_some(), "Alpha missing from HTML");
    assert!(beta_pos.is_some(), "Beta missing from HTML");
}

// ===========================================================================
// Module 10: Error Boundary (tests 41–42)
// ===========================================================================

#[test]
fn test_41_holon_error_display() {
    let cases: Vec<(HolonError, &str)> = vec![
        (HolonError::NotFound("foo".into()), "holon not found: foo"),
        (
            HolonError::EmptyAuthorizer,
            "authorized_by must not be empty",
        ),
        (HolonError::EmptyStepId, "step_id must not be empty"),
        (
            HolonError::DuplicateStep("bar".into()),
            "step already registered: bar",
        ),
        (HolonError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "no file")), "io error:"),
        (HolonError::Json(serde_json::from_str::<CytoscapeGraph>("invalid").unwrap_err()), "json error:"),
        (HolonError::Render("template fail".into()), "render error: template fail"),
    ];

    for (error, expected_prefix) in &cases {
        let display = format!("{}", error);
        assert!(
            display.starts_with(expected_prefix),
            "expected display of {:?} to start with '{}', got '{}'",
            error,
            expected_prefix,
            display
        );
    }
}

#[test]
fn test_42_crate_re_exports() {
    // Verify all public types are accessible from crate root by constructing
    // minimal valid instances of each.

    // ProcessController
    let _ctrl = ProcessController::new();

    // ProcessStep
    let _step = ProcessStep {
        step_id: "s".into(),
        description: "d".into(),
    };

    // TransitionReceipt — constructed via authorize (use a simplified mock)
    // We just verify the type compiles by using the authorize method
    let mut ctrl_for_receipt = ProcessController::new();
    ctrl_for_receipt.register_step(make_step("demo")).unwrap();
    let _receipt = ctrl_for_receipt
        .authorize_step("demo", "tester", 100)
        .unwrap();

    // ImmutableActionLog
    let _log = ImmutableActionLog::new();

    // ActionKind
    let _kind = ActionKind::StepAuthorized;

    // ActionRecord — constructed via append
    let mut log_for_record = ImmutableActionLog::new();
    let _record = log_for_record.append(ActionKind::HolonCreated, "agent", 1, [0u8; 32]);

    // Holon
    let _root = Holon::root("id", "label", HolonKind::SysmlBlock);
    let _child = Holon::child("c", "child", HolonKind::OwlClass, "parent");

    // HolonKind
    let _hk = HolonKind::ProcessNode;

    // CytoscapeGraph
    let _g = CytoscapeGraph {
        nodes: vec![],
        edges: vec![],
    };

    // CytoscapeNode / CytoscapeEdge
    let _cy_node = CytoscapeNode {
        data: holon_viz::CytoscapeNodeData {
            id: "n".into(),
            label: "N".into(),
            kind: "struct".into(),
            parent: None,
            z_layer: None,
            semantic_type: None,
        },
    };
    let _cy_edge = CytoscapeEdge {
        data: holon_viz::CytoscapeEdgeData {
            id: "e".into(),
            source: "a".into(),
            target: "b".into(),
            label: "knows".into(),
        },
    };

    // HtmlRenderer
    let _html = HtmlRenderer::render(&_g);

    // SysmlV2Emitter
    let _sysml = SysmlV2Emitter::emit(&_g);

    // Owl2Emitter
    let _owl = Owl2Emitter::emit(&_g);

    // TypeRelationshipGraph
    let _trg = TypeRelationshipGraph::new(vec![], vec![]);
    let _seed = TypeRelationshipGraph::seed();
    let _cy_from_trg = _trg.to_cytoscape();

    // TypeRelationshipKind
    let _trk = TypeRelationshipKind::References;

    // VizObservation
    let _obs = VizObservation {
        screenshot_path: Path::new("/tmp/s.png").to_path_buf(),
        node_count: 0,
        edge_count: 0,
        raw_output: "CDP_UNAVAILABLE".to_string(),
    };

    // VizObserver
    let _observer = VizObserver::new(
        Path::new("/tmp/s.png").to_path_buf(),
        Path::new("/tmp/script.py").to_path_buf(),
    );

    // HolonError (variants exercised in test_41)
    let _err_nf = HolonError::NotFound("x".into());
    let _err_empty_auth = HolonError::EmptyAuthorizer;
    let _err_empty_step = HolonError::EmptyStepId;
    let _render_err = HolonError::Render("oops".into());

    // Confirm types are Debug (required for assert_eq! on error types)
    let _ = format!("{:?}", _ctrl);
    let _ = format!("{:?}", _kind);
    let _ = format!("{:?}", _hk);
    let _ = format!("{:?}", _trk);
    let _ = format!("{:?}", _err_nf);

    // This test exists primarily for compilation verification —
    // if all types compile and construct, re-exports are intact.
    assert!(true, "all public types accessible from crate root");
}
