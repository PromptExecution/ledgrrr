//! Text emitters for SysML-v2 block definitions and OWL2/Turtle fragments.
//!
//! Both emitters are deterministic: given the same `CytoscapeGraph` they
//! produce identical output regardless of call order.

use crate::cytoscape::CytoscapeGraph;

/// Emits SysML-v2 textual block definition fragments from a [`CytoscapeGraph`].
///
/// Output is a minimal SysML-v2 `package` block containing one `block def`
/// per node and `part def` references for containment edges.
pub struct SysmlV2Emitter;

impl SysmlV2Emitter {
    /// Emit a SysML-v2 textual representation of the graph.
    ///
    /// The output is a syntactically valid SysML-v2 fragment but is intentionally
    /// minimal — full attribute typing and constraints are out of scope here.
    pub fn emit(graph: &CytoscapeGraph) -> String {
        let mut out = String::from("package HolonModel {\n");

        for node in &graph.nodes {
            let safe_label = sanitize_sysml_id(&node.data.label);
            out.push_str(&format!(
                "    block def {} {{ // id: {}, kind: {} }}\n",
                safe_label, node.data.id, node.data.kind
            ));
        }

        for edge in &graph.edges {
            // Find source/target labels for readability; fall back to IDs.
            let src_label = find_label(graph, &edge.data.source);
            let tgt_label = find_label(graph, &edge.data.target);
            out.push_str(&format!(
                "    part def {} : {} {{ // edge: {} }}\n",
                sanitize_sysml_id(&tgt_label),
                sanitize_sysml_id(&src_label),
                edge.data.id
            ));
        }

        out.push_str("}\n");
        out
    }
}

/// Emits an OWL2/Turtle fragment from a [`CytoscapeGraph`].
///
/// Produces a valid Turtle serialization with `owl:Class` declarations and
/// `rdfs:subClassOf` triples for containment edges.
pub struct Owl2Emitter;

impl Owl2Emitter {
    /// Emit an OWL2/Turtle representation of the graph.
    pub fn emit(graph: &CytoscapeGraph) -> String {
        let mut out = String::new();

        // Prefixes
        out.push_str("@prefix owl: <http://www.w3.org/2002/07/owl#> .\n");
        out.push_str("@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n");
        out.push_str("@prefix holon: <urn:holon-viz:> .\n\n");

        // Ontology declaration
        out.push_str("<urn:holon-viz:ontology> a owl:Ontology .\n\n");

        // Class declarations
        for node in &graph.nodes {
            let safe_id = sanitize_turtle_local(&node.data.id);
            out.push_str(&format!(
                "holon:{} a owl:Class ;\n    rdfs:label \"{}\" ;\n    rdfs:comment \"kind: {}\" .\n\n",
                safe_id, node.data.label, node.data.kind
            ));
        }

        // Containment as rdfs:subClassOf (parent contains child → child rdfs:subClassOf parent)
        for edge in &graph.edges {
            let src = sanitize_turtle_local(&edge.data.source);
            let tgt = sanitize_turtle_local(&edge.data.target);
            out.push_str(&format!(
                "holon:{} rdfs:subClassOf holon:{} . # {}\n",
                tgt, src, edge.data.id
            ));
        }

        out
    }
}

/// Sanitize a string to a valid SysML-v2 identifier token (alphanumeric + underscore).
fn sanitize_sysml_id(s: &str) -> String {
    let sanitized: String = s
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect();
    // Identifiers must not start with a digit.
    if sanitized.starts_with(|c: char| c.is_ascii_digit()) {
        format!("_{}", sanitized)
    } else if sanitized.is_empty() {
        "_empty".to_string()
    } else {
        sanitized
    }
}

/// Sanitize a string to a valid Turtle local name (no spaces or special chars).
fn sanitize_turtle_local(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect()
}

/// Look up a node's label by ID; returns the ID itself if not found.
fn find_label<'a>(graph: &'a CytoscapeGraph, id: &'a str) -> String {
    graph
        .nodes
        .iter()
        .find(|n| n.data.id == id)
        .map(|n| n.data.label.clone())
        .unwrap_or_else(|| id.to_string())
}
