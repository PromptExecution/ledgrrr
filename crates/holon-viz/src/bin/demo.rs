//! holon-viz demo — generates a sample tax-pipeline holarchy as a static HTML file.
//!
//! Usage (from project root):
//!   cargo run -p holon-viz --bin holon-viz-demo
//!
//! Writes: target/holon-viz-demo.html
//! Open via Justfile: just demo-viz  (builds + generates + opens via PowerShell)

use holon_viz::{CytoscapeGraph, Holon, HolonKind, HtmlRenderer};
use std::collections::HashMap;
use std::path::PathBuf;
use serde_json::Value;

fn main() {
    let holons = sample_tax_pipeline();
    let graph = CytoscapeGraph::from_holons(&holons);

    // Write alongside the cargo target dir so the Justfile PowerShell step
    // can find it at D:\Projects\l3dg3rr\target\holon-viz-demo.html
    let out_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/holon-viz-demo.html");

    match HtmlRenderer::write_to_file(&graph, &out_path) {
        Ok(()) => {
            let canonical = out_path.canonicalize().unwrap_or(out_path);
            println!("{}", canonical.display());
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}

/// Build a holarchy that represents the l3dg3rr tax-ledger pipeline.
fn sample_tax_pipeline() -> Vec<Holon> {
    let mut meta: HashMap<String, Value> = HashMap::new();
    meta.insert("version".to_string(), Value::String("v1.9.0".to_string()));

    vec![
        // ── Pipeline root ────────────────────────────────────────────────────
        Holon {
            id: "pipeline".to_string(),
            label: "Tax Ledger Pipeline".to_string(),
            kind: HolonKind::CapsuleGroup,
            parent_id: None,
            children: vec![
                "ingest".to_string(),
                "classify".to_string(),
                "reconcile".to_string(),
                "attest".to_string(),
            ],
            metadata: meta.clone(),
        },
        // ── Stage nodes ──────────────────────────────────────────────────────
        Holon {
            id: "ingest".to_string(),
            label: "Ingest PDFs".to_string(),
            kind: HolonKind::SysmlBlock,
            parent_id: Some("pipeline".to_string()),
            children: vec!["docling".to_string(), "blake3-id".to_string()],
            metadata: HashMap::<String, Value>::new(),
        },
        Holon {
            id: "classify".to_string(),
            label: "Classify Transactions".to_string(),
            kind: HolonKind::SysmlBlock,
            parent_id: Some("pipeline".to_string()),
            children: vec!["rhai-rules".to_string(), "flag-queue".to_string()],
            metadata: HashMap::<String, Value>::new(),
        },
        Holon {
            id: "reconcile".to_string(),
            label: "Reconcile & Export".to_string(),
            kind: HolonKind::SysmlBlock,
            parent_id: Some("pipeline".to_string()),
            children: vec!["excel-workbook".to_string()],
            metadata: HashMap::<String, Value>::new(),
        },
        Holon {
            id: "attest".to_string(),
            label: "Attest (CPA Sign-off)".to_string(),
            kind: HolonKind::SysmlBlock,
            parent_id: Some("pipeline".to_string()),
            children: vec!["audit-log".to_string()],
            metadata: HashMap::<String, Value>::new(),
        },
        // ── Leaf nodes ───────────────────────────────────────────────────────
        Holon {
            id: "docling".to_string(),
            label: "Docling OCR".to_string(),
            kind: HolonKind::ProcessNode,
            parent_id: Some("ingest".to_string()),
            children: vec![],
            metadata: HashMap::<String, Value>::new(),
        },
        Holon {
            id: "blake3-id".to_string(),
            label: "Blake3 Content ID".to_string(),
            kind: HolonKind::ProcessNode,
            parent_id: Some("ingest".to_string()),
            children: vec![],
            metadata: HashMap::<String, Value>::new(),
        },
        Holon {
            id: "rhai-rules".to_string(),
            label: "Rhai Rule Engine".to_string(),
            kind: HolonKind::ProcessNode,
            parent_id: Some("classify".to_string()),
            children: vec![],
            metadata: HashMap::<String, Value>::new(),
        },
        Holon {
            id: "flag-queue".to_string(),
            label: "Flag Queue".to_string(),
            kind: HolonKind::ProcessNode,
            parent_id: Some("classify".to_string()),
            children: vec![],
            metadata: HashMap::<String, Value>::new(),
        },
        Holon {
            id: "excel-workbook".to_string(),
            label: "Excel Workbook".to_string(),
            kind: HolonKind::OwlClass,
            parent_id: Some("reconcile".to_string()),
            children: vec![],
            metadata: HashMap::<String, Value>::new(),
        },
        Holon {
            id: "audit-log".to_string(),
            label: "Immutable Audit Log".to_string(),
            kind: HolonKind::AuditEvent,
            parent_id: Some("attest".to_string()),
            children: vec![],
            metadata: HashMap::<String, Value>::new(),
        },
    ]
}
