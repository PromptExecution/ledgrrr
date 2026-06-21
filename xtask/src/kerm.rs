/// KerML-profile codegen: parse `types/domain.kerm` and emit `generated_seed()` Rust source.
use serde::Deserialize;
use std::fmt::Write;
use std::path::Path;

/// A node (type) entry in `domain.kerm`.
#[derive(Deserialize)]
pub struct KermType {
    pub id: String,
    pub label: Option<String>,
    pub kind: String,
    pub z_layer: Option<String>,
    pub semantic_type: Option<String>,
}

/// A directed relationship entry in `domain.kerm`.
#[derive(Deserialize)]
pub struct KermRel {
    pub from: String,
    pub to: String,
    pub kind: String,
}

/// Top-level structure of a `.kerm` TOML file.
#[derive(Deserialize)]
pub struct KermDomain {
    #[serde(rename = "type")]
    pub types: Vec<KermType>,
    pub rel: Vec<KermRel>,
}

/// Load and parse a `.kerm` TOML file from disk.
pub fn load(path: &Path) -> Result<KermDomain, Box<dyn std::error::Error>> {
    let src = std::fs::read_to_string(path)?;
    Ok(toml::from_str(&src)?)
}

/// Emit a `generated_seed()` Rust source string from a parsed `KermDomain`.
///
/// The output module is `crates/holon-viz/src/gen.rs`.
/// Uses `type_node`, `typed_node`, and `rel` helpers from `crate::type_graph`,
/// which must be `pub(crate)`.
pub fn codegen(domain: &KermDomain) -> String {
    let mut out = String::new();

    writeln!(out, "// @generated — do not edit. Source: types/domain.kerm").unwrap();
    writeln!(out, "// Regenerate with: just gen-kerm").unwrap();
    writeln!(
        out,
        "use crate::type_graph::{{TypeRelationshipGraph, TypeRelationshipKind}};"
    )
    .unwrap();
    writeln!(
        out,
        "use crate::type_graph::{{type_node, typed_node, rel}};"
    )
    .unwrap();
    writeln!(out).unwrap();
    writeln!(out, "pub fn generated_seed() -> TypeRelationshipGraph {{").unwrap();
    writeln!(out, "    let nodes = vec![").unwrap();

    for t in &domain.types {
        let label = t.label.as_deref().unwrap_or(&t.id);
        match (&t.z_layer, &t.semantic_type) {
            (Some(z), Some(s)) => {
                writeln!(
                    out,
                    "        typed_node({id:?}, {label:?}, {kind:?}, {z:?}, {s:?}),",
                    id = t.id,
                    label = label,
                    kind = t.kind,
                    z = z,
                    s = s,
                )
                .unwrap();
            }
            _ => {
                writeln!(
                    out,
                    "        type_node({id:?}, {label:?}, {kind:?}),",
                    id = t.id,
                    label = label,
                    kind = t.kind,
                )
                .unwrap();
            }
        }
    }

    writeln!(out, "    ];").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "    let relationships = vec![").unwrap();

    for r in &domain.rel {
        let variant = rel_kind_variant(&r.kind);
        writeln!(
            out,
            "        rel({from:?}, {to:?}, TypeRelationshipKind::{variant}),",
            from = r.from,
            to = r.to,
        )
        .unwrap();
    }

    writeln!(out, "    ];").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "    TypeRelationshipGraph::new(nodes, relationships)").unwrap();
    writeln!(out, "}}").unwrap();
    out
}

/// Map a snake_case relationship kind string to the `TypeRelationshipKind` variant name.
fn rel_kind_variant(kind: &str) -> &str {
    match kind {
        "implements" => "Implements",
        "contains" => "Contains",
        "advances_to" => "AdvancesTo",
        "validated_by" => "ValidatedBy",
        "produces" => "Produces",
        "verifies" => "Verifies",
        "references" => "References",
        "constrains" => "Constrains",
        "attests" => "Attests",
        "projects_to" => "ProjectsTo",
        "classified_as" => "ClassifiedAs",
        "records_in" => "RecordsIn",
        "derives_from" => "DerivesFrom",
        other => panic!("unknown rel kind in domain.kerm: {other}"),
    }
}
