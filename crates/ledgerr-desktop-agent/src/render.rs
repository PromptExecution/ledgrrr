//! Diagram rendering — PRD-11 §6.1.
//!
//! Mermaid and canonical JSON are implemented for real. SVG uses a minimal
//! deterministic vertical-stack layout (no external layout engine) so it is
//! honest about its fidelity rather than faking a graph layout. PNG is left
//! unsupported: it needs a rasterizer dependency this crate does not carry
//! yet, and PRD-11 §6.1 marks it a "Future" format.

use crate::playbook::PlaybookModel;

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("unsupported diagram format: {0} (supported: mermaid, state-machine, json, svg)")]
    UnsupportedFormat(String),
    #[error(transparent)]
    Playbook(#[from] crate::playbook::PlaybookError),
}

fn mermaid_node_id(id: &str) -> String {
    // Mermaid node ids must avoid bare spaces/quotes; playbook ids are
    // expected to already be identifier-safe, but escape defensively.
    id.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn mermaid_escape_label(label: &str) -> String {
    label.replace('"', "'")
}

pub fn render_mermaid(model: &PlaybookModel) -> Result<String, RenderError> {
    model.validate()?;
    let mut out = String::from("flowchart TD\n");
    for node in &model.nodes {
        let id = mermaid_node_id(&node.id);
        let label = mermaid_escape_label(&node.label);
        let shape = match node.kind {
            crate::playbook::NodeKind::Start | crate::playbook::NodeKind::End => {
                format!("(\"{label}\")")
            }
            crate::playbook::NodeKind::Gate => format!("{{\"{label}\"}}"),
            crate::playbook::NodeKind::Task => format!("[\"{label}\"]"),
        };
        out.push_str(&format!("    {id}{shape}\n"));
    }
    for edge in &model.edges {
        let from = mermaid_node_id(&edge.from);
        let to = mermaid_node_id(&edge.to);
        match &edge.label {
            Some(label) => out.push_str(&format!(
                "    {from} -->|{}| {to}\n",
                mermaid_escape_label(label)
            )),
            None => out.push_str(&format!("    {from} --> {to}\n")),
        }
    }
    Ok(out)
}

/// State-machine projection of the same canonical playbook. This makes
/// execution outcomes first-class labels rather than merely diagram arrows.
pub fn render_state_machine(model: &PlaybookModel) -> Result<String, RenderError> {
    model.validate()?;
    let mut out = String::from("stateDiagram-v2\n");
    for node in &model.nodes {
        if node.kind == crate::playbook::NodeKind::Start {
            out.push_str(&format!("    [*] --> {}\n", mermaid_node_id(&node.id)));
        }
    }
    for edge in &model.edges {
        let label = edge
            .outcome
            .as_deref()
            .or(edge.label.as_deref())
            .unwrap_or("transition");
        out.push_str(&format!(
            "    {} --> {} : {}\n",
            mermaid_node_id(&edge.from),
            mermaid_node_id(&edge.to),
            mermaid_escape_label(label)
        ));
    }
    for node in &model.nodes {
        if node.kind == crate::playbook::NodeKind::End {
            out.push_str(&format!("    {} --> [*]\n", mermaid_node_id(&node.id)));
        }
    }
    Ok(out)
}

pub fn render_json(model: &PlaybookModel) -> Result<String, RenderError> {
    model.validate()?;
    // to_string_pretty over a struct with only String/Vec fields (no
    // HashMap) is already key-order-deterministic — struct field order is
    // fixed by the type definition.
    serde_json::to_string_pretty(model).map_err(|e| RenderError::UnsupportedFormat(e.to_string()))
}

/// Minimal deterministic SVG: nodes stacked top-to-bottom in declaration
/// order, edges drawn as straight vertical connectors between stated
/// from/to y-centers. This is not a graph layout solver — it is a
/// legible, always-renderable fallback.
pub fn render_svg(model: &PlaybookModel) -> Result<String, RenderError> {
    model.validate()?;
    const ROW_HEIGHT: u32 = 60;
    const WIDTH: u32 = 320;
    const BOX_HEIGHT: u32 = 36;

    let mut y_of: std::collections::BTreeMap<&str, u32> = std::collections::BTreeMap::new();
    for (i, node) in model.nodes.iter().enumerate() {
        y_of.insert(node.id.as_str(), i as u32 * ROW_HEIGHT + BOX_HEIGHT / 2);
    }
    let height = model.nodes.len() as u32 * ROW_HEIGHT + ROW_HEIGHT;

    let mut out = String::new();
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{WIDTH}\" height=\"{height}\" viewBox=\"0 0 {WIDTH} {height}\">\n"
    ));
    for edge in &model.edges {
        if let (Some(&from_y), Some(&to_y)) =
            (y_of.get(edge.from.as_str()), y_of.get(edge.to.as_str()))
        {
            out.push_str(&format!(
                "  <line x1=\"{}\" y1=\"{from_y}\" x2=\"{}\" y2=\"{to_y}\" stroke=\"#666\" stroke-width=\"1\" />\n",
                WIDTH / 2,
                WIDTH / 2
            ));
        }
    }
    for (i, node) in model.nodes.iter().enumerate() {
        let y = i as u32 * ROW_HEIGHT;
        let label = node
            .label
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        out.push_str(&format!(
            "  <rect x=\"20\" y=\"{y}\" width=\"{}\" height=\"{BOX_HEIGHT}\" fill=\"#eef\" stroke=\"#334\" />\n",
            WIDTH - 40
        ));
        out.push_str(&format!(
            "  <text x=\"{}\" y=\"{}\" text-anchor=\"middle\" font-size=\"12\">{label}</text>\n",
            WIDTH / 2,
            y + BOX_HEIGHT / 2 + 4
        ));
    }
    out.push_str("</svg>\n");
    Ok(out)
}

pub fn render(model: &PlaybookModel, format: &str) -> Result<String, RenderError> {
    match format {
        "mermaid" => render_mermaid(model),
        "state-machine" | "state_machine" => render_state_machine(model),
        "json" => render_json(model),
        "svg" => render_svg(model),
        other => Err(RenderError::UnsupportedFormat(other.to_string())),
    }
}
