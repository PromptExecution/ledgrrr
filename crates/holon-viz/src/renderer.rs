//! HTML renderer — converts a [`CytoscapeGraph`] to a self-contained HTML page.
//!
//! The output embeds the graph JSON as a JavaScript variable and initialises
//! Cytoscape.js from a CDN link. No Node.js or bundler required.

use crate::cytoscape::CytoscapeGraph;
use crate::HolonError;
use std::path::Path;

/// Renders a [`CytoscapeGraph`] as a self-contained HTML string.
pub struct HtmlRenderer;

impl HtmlRenderer {
    /// Return a complete HTML document that renders `graph` via Cytoscape.js.
    ///
    /// The graph JSON is inlined as `const GRAPH_DATA` so the page works as
    /// a single static file — no server required.
    pub fn render(graph: &CytoscapeGraph) -> String {
        let graph_json = graph
            .to_json_pretty()
            .unwrap_or_else(|_| "{ \"nodes\": [], \"edges\": [] }".to_string());

        format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>holon-viz</title>
  <script src="https://unpkg.com/cytoscape@3/dist/cytoscape.min.js"></script>
  <style>
    html, body {{ margin: 0; padding: 0; height: 100%; }}
    #cy {{ width: 100%; height: 100vh; display: block; }}
  </style>
</head>
<body>
  <div id="cy"></div>
  <script>
    const GRAPH_DATA = {graph_json};

    document.addEventListener('DOMContentLoaded', function () {{
      const elements = [];
      (GRAPH_DATA.nodes || []).forEach(function(n) {{ elements.push({{ data: n.data }}); }});
      (GRAPH_DATA.edges || []).forEach(function(e) {{ elements.push({{ data: e.data }}); }});

      window._cy = cytoscape({{
        container: document.getElementById('cy'),
        elements: elements,
        layout: {{ name: 'cose' }},
        style: [
          {{ selector: 'node', style: {{ 'label': 'data(label)', 'background-color': '#0074D9', 'color': '#fff', 'text-valign': 'center', 'text-halign': 'center' }} }},
          {{ selector: 'edge', style: {{ 'label': 'data(label)', 'curve-style': 'bezier', 'target-arrow-shape': 'triangle', 'line-color': '#aaa', 'target-arrow-color': '#aaa' }} }}
        ]
      }});
    }});
  </script>
</body>
</html>"#,
            graph_json = graph_json,
        )
    }

    /// Write `render(graph)` to `path`, creating parent directories as needed.
    pub fn write_to_file(graph: &CytoscapeGraph, path: &Path) -> Result<(), HolonError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let html = Self::render(graph);
        std::fs::write(path, html)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::holon::{Holon, HolonKind};

    fn two_node_graph() -> CytoscapeGraph {
        let holons = vec![
            Holon {
                id: "a".to_string(),
                label: "Alpha".to_string(),
                kind: HolonKind::ProcessNode,
                parent_id: None,
                children: vec!["b".to_string()],
                metadata: Default::default(),
            },
            Holon {
                id: "b".to_string(),
                label: "Beta".to_string(),
                kind: HolonKind::SysmlBlock,
                parent_id: Some("a".to_string()),
                children: vec![],
                metadata: Default::default(),
            },
        ];
        CytoscapeGraph::from_holons(&holons)
    }

    #[test]
    fn render_contains_cytoscape_cdn() {
        let graph = two_node_graph();
        let html = HtmlRenderer::render(&graph);
        assert!(html.contains("cytoscape.min.js"), "missing Cytoscape CDN link");
    }

    #[test]
    fn render_contains_graph_data() {
        let graph = two_node_graph();
        let html = HtmlRenderer::render(&graph);
        assert!(html.contains("GRAPH_DATA"), "missing GRAPH_DATA variable");
        assert!(html.contains("Alpha"), "node label Alpha missing");
        assert!(html.contains("Beta"), "node label Beta missing");
    }

    #[test]
    fn write_to_file_round_trip() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("test.html");
        let graph = two_node_graph();
        HtmlRenderer::write_to_file(&graph, &path).expect("write_to_file");
        let contents = std::fs::read_to_string(&path).expect("read back");
        assert!(contents.contains("GRAPH_DATA"));
    }
}
