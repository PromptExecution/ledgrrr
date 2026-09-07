use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use syn::{visit::Visit, ImplItem, Item, Type};
use walkdir::WalkDir;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TermClassification {
    Entity { kind: String },
    Action { kind: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RustTerm {
    pub name: String,
    pub classification: TermClassification,
    pub file: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OntologyNode {
    pub id: String,
    pub file: String,
    pub kind: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OntologyGraph {
    pub nodes: Vec<OntologyNode>,
}

impl OntologyGraph {
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).context("failed to serialize ontology graph")
    }
}

#[derive(Debug, Default)]
pub struct RustAstExtractor;

impl RustAstExtractor {
    pub fn new() -> Self {
        Self
    }

    pub async fn extract_rust_idioms(&self, crate_path: &Path) -> Result<Vec<RustTerm>> {
        let crate_path = crate_path.to_path_buf();
        tokio::task::spawn_blocking(move || {
            let mut terms = Vec::new();

            for entry in WalkDir::new(&crate_path)
                .into_iter()
                .filter_entry(|entry| entry.file_name() != "target")
            {
                let entry = entry.context("failed to walk crate path")?;
                if !entry.file_type().is_file()
                    || entry.path().extension().and_then(|ext| ext.to_str()) != Some("rs")
                {
                    continue;
                }

                let source = fs::read_to_string(entry.path())
                    .with_context(|| format!("failed to read {}", entry.path().display()))?;
                let syntax = syn::parse_file(&source)
                    .with_context(|| format!("failed to parse {}", entry.path().display()))?;

                let file = entry
                    .path()
                    .strip_prefix(&crate_path)
                    .unwrap_or(entry.path())
                    .display()
                    .to_string();
                let mut visitor = TermCollector {
                    file: file.clone(),
                    terms: Vec::new(),
                };
                visitor.visit_file(&syntax);
                terms.extend(visitor.terms);
            }

            // Sort and dedup by (file, name, classification) so items that share a
            // name in different Rust namespaces (e.g. `struct Foo` and `fn Foo()`)
            // are preserved as distinct entries.
            terms.sort_unstable_by(|left, right| {
                (
                    left.file.as_str(),
                    left.name.as_str(),
                    left.classification.kind(),
                )
                    .cmp(&(
                        right.file.as_str(),
                        right.name.as_str(),
                        right.classification.kind(),
                    ))
            });
            terms.dedup_by(|left, right| {
                left.file == right.file
                    && left.name == right.name
                    && left.classification == right.classification
            });

            Ok(terms)
        })
        .await
        .context("blocking extraction task panicked")?
    }

    pub fn build_ontology_graph(&self, terms: &[RustTerm]) -> OntologyGraph {
        let mut graph = OntologyGraph {
            nodes: terms
                .iter()
                .map(|term| OntologyNode {
                    id: format!("{}::{}", term.file, term.name),
                    file: term.file.clone(),
                    kind: term.classification.kind().to_string(),
                    label: term.name.clone(),
                })
                .collect(),
        };
        graph.nodes.sort_by(|left, right| left.id.cmp(&right.id));
        graph
    }
}

impl TermClassification {
    pub fn kind(&self) -> &str {
        match self {
            Self::Entity { kind } | Self::Action { kind } => kind.as_str(),
        }
    }
}

struct TermCollector {
    file: String,
    terms: Vec<RustTerm>,
}

impl<'ast> Visit<'ast> for TermCollector {
    fn visit_item(&mut self, item: &'ast Item) {
        match item {
            Item::Struct(item) => self.push_entity(&item.ident.to_string(), "struct"),
            Item::Enum(item) => self.push_entity(&item.ident.to_string(), "enum"),
            Item::Trait(item) => self.push_entity(&item.ident.to_string(), "trait"),
            Item::Type(item) => self.push_entity(&item.ident.to_string(), "type"),
            Item::Const(item) => self.push_entity(&item.ident.to_string(), "const"),
            Item::Static(item) => self.push_entity(&item.ident.to_string(), "static"),
            Item::Fn(item) => self.push_action(&item.sig.ident.to_string(), "function"),
            Item::Mod(item) => self.push_entity(&item.ident.to_string(), "module"),
            Item::Impl(item) => {
                for impl_item in &item.items {
                    if let ImplItem::Fn(method) = impl_item {
                        let owner =
                            type_name(item.self_ty.as_ref()).unwrap_or_else(|| "impl".to_string());
                        self.push_action(&format!("{owner}::{}", method.sig.ident), "method");
                    }
                }
            }
            _ => {}
        }

        syn::visit::visit_item(self, item);
    }
}

impl TermCollector {
    fn push_entity(&mut self, name: &str, kind: &str) {
        self.terms.push(RustTerm {
            name: name.to_string(),
            classification: TermClassification::Entity {
                kind: kind.to_string(),
            },
            file: self.file.clone(),
        });
    }

    fn push_action(&mut self, name: &str, kind: &str) {
        self.terms.push(RustTerm {
            name: name.to_string(),
            classification: TermClassification::Action {
                kind: kind.to_string(),
            },
            file: self.file.clone(),
        });
    }
}

fn type_name(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(type_path) => type_path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string()),
        _ => None,
    }
}
