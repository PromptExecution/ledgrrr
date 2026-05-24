use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;

use ledger_core::ontology::{
    artifact_content_hash, relation_content_hash, Artifact, ArtifactKind, OntologySnapshot,
    Relation,
};
use serde::{Deserialize, Serialize};

use crate::ToolError;

pub type OntologyEntityKind = ArtifactKind;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OntologyEntityInput {
    pub kind: OntologyEntityKind,
    pub attrs: BTreeMap<String, String>,
    /// If set, overrides `kind` for custom (non-built-in) entity types.
    /// Only used when `kind` cannot represent the desired type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OntologyEdgeInput {
    pub from: String,
    pub to: String,
    pub relation: String,
    pub provenance: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OntologyEntity {
    pub id: String,
    pub kind: OntologyEntityKind,
    pub attrs: BTreeMap<String, String>,
    /// If set, this entity uses a custom (non-built-in) kind name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OntologyEdge {
    pub id: String,
    pub from: String,
    pub to: String,
    pub relation: String,
    pub provenance: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OntologyStore {
    pub entities: Vec<OntologyEntity>,
    pub edges: Vec<OntologyEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OntologyUpsertEntitiesRequest {
    pub ontology_path: std::path::PathBuf,
    pub entities: Vec<OntologyEntityInput>,
    /// Optional path to a schema store JSON file for kind validation.
    pub schema_store_path: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OntologyUpsertEntitiesResponse {
    pub inserted_count: usize,
    pub entity_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OntologyUpsertEdgesRequest {
    pub ontology_path: std::path::PathBuf,
    pub edges: Vec<OntologyEdgeInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OntologyUpsertEdgesResponse {
    pub inserted_count: usize,
    pub edge_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OntologyQueryPathRequest {
    pub ontology_path: std::path::PathBuf,
    pub from_entity_id: String,
    pub max_depth: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OntologyQueryPathResponse {
    pub nodes: Vec<OntologyEntity>,
    pub edges: Vec<OntologyEdge>,
}

impl OntologyStore {
    pub fn load(path: &Path) -> Result<Self, ToolError> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let raw = std::fs::read_to_string(path).map_err(|e| ToolError::Internal(e.to_string()))?;
        let mut store: Self =
            serde_json::from_str(&raw).map_err(|e| ToolError::Internal(e.to_string()))?;
        store.sort_deterministic();
        Ok(store)
    }

    pub fn persist(&mut self, path: &Path) -> Result<(), ToolError> {
        self.sort_deterministic();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ToolError::Internal(e.to_string()))?;
        }
        let payload =
            serde_json::to_string_pretty(self).map_err(|e| ToolError::Internal(e.to_string()))?;
        std::fs::write(path, payload).map_err(|e| ToolError::Internal(e.to_string()))
    }

    pub fn to_core_snapshot(&self) -> OntologySnapshot {
        let mut snapshot = OntologySnapshot {
            artifacts: self
                .entities
                .iter()
                .map(|entity| Artifact {
                    id: entity.id.clone(),
                    kind: entity.kind,
                    attrs: entity.attrs.clone(),
                })
                .collect(),
            relations: self
                .edges
                .iter()
                .map(|edge| Relation {
                    id: edge.id.clone(),
                    from: edge.from.clone(),
                    to: edge.to.clone(),
                    relation: edge.relation.clone(),
                    provenance: edge.provenance.clone(),
                })
                .collect(),
        };
        snapshot.sort_deterministic();
        snapshot
    }

    pub fn upsert_entities(
        &mut self,
        entities: Vec<OntologyEntityInput>,
        schema_store: Option<&crate::schema::SchemaStore>,
    ) -> Result<OntologyUpsertEntitiesResponse, ToolError> {
        let mut inserted_count = 0usize;
        let mut entity_ids = Vec::with_capacity(entities.len());

        for input in &entities {
            // Determine the kind name to use (custom or built-in).
            let kind_name = input
                .custom_kind
                .as_deref()
                .unwrap_or_else(|| input.kind.canonical_name());

            // If a SchemaStore is provided, validate that the kind is known.
            if let Some(store) = schema_store {
                if !store.is_known_kind(kind_name) {
                    return Err(ToolError::InvalidInput(format!(
                        "unknown entity kind '{kind_name}'. Use ledgerr_schema with register_kind action first."
                    )));
                }
            }
        }

        for input in entities {
            // Determine the kind name to use (custom or built-in).
            let kind_name = input
                .custom_kind
                .clone()
                .unwrap_or_else(|| input.kind.canonical_name().to_string());

            let id = if let Some(user_id) = input.attrs.get("id") {
                user_id.clone()
            } else {
                entity_content_hash_str(&kind_name, &input.attrs)
            };
            entity_ids.push(id.clone());
            if self.entities.iter().any(|existing| existing.id == id) {
                continue;
            }

            self.entities.push(OntologyEntity {
                id,
                kind: input.kind,
                attrs: input.attrs,
                custom_kind: input.custom_kind,
            });
            inserted_count += 1;
        }

        self.sort_deterministic();

        Ok(OntologyUpsertEntitiesResponse {
            inserted_count,
            entity_ids,
        })
    }

    /// Upsert entities with schema validation against a SchemaStore.
    /// Custom kind entities are validated against the store; built-in kinds pass through.
    /// Delegates to upsert_entities(entities, Some(schema_store)).
    pub fn upsert_entities_with_schema(
        &mut self,
        entities: Vec<OntologyEntityInput>,
        schema_store: &crate::schema::SchemaStore,
    ) -> Result<OntologyUpsertEntitiesResponse, ToolError> {
        self.upsert_entities(entities, Some(schema_store))
    }

    pub fn upsert_edges(
        &mut self,
        edges: Vec<OntologyEdgeInput>,
    ) -> Result<OntologyUpsertEdgesResponse, ToolError> {
        let entity_ids = self
            .entities
            .iter()
            .map(|entity| entity.id.clone())
            .collect::<BTreeSet<_>>();

        let mut inserted_count = 0usize;
        let mut edge_ids = Vec::with_capacity(edges.len());

        for input in edges {
            if !entity_ids.contains(&input.from) || !entity_ids.contains(&input.to) {
                return Err(ToolError::InvalidInput(
                    "missing_ref: edge endpoints must reference existing entities".to_string(),
                ));
            }

            let id = edge_content_hash(&input.from, &input.to, &input.relation, &input.provenance);
            edge_ids.push(id.clone());

            if self.edges.iter().any(|existing| existing.id == id) {
                continue;
            }

            self.edges.push(OntologyEdge {
                id,
                from: input.from,
                to: input.to,
                relation: input.relation,
                provenance: input.provenance,
            });
            inserted_count += 1;
        }

        self.sort_deterministic();

        Ok(OntologyUpsertEdgesResponse {
            inserted_count,
            edge_ids,
        })
    }

    pub fn query_path(
        &self,
        from_entity_id: &str,
        max_depth: Option<usize>,
    ) -> Result<OntologyQueryPathResponse, ToolError> {
        let entity_lookup = self
            .entities
            .iter()
            .cloned()
            .map(|entity| (entity.id.clone(), entity))
            .collect::<BTreeMap<_, _>>();

        let start = entity_lookup.get(from_entity_id).cloned().ok_or_else(|| {
            ToolError::InvalidInput(
                "missing_ref: from_entity_id must reference an existing entity".to_string(),
            )
        })?;

        let depth_limit = max_depth.unwrap_or(usize::MAX);
        let mut queue = VecDeque::new();
        queue.push_back((from_entity_id.to_string(), 0usize));

        let mut visited = BTreeSet::new();
        visited.insert(from_entity_id.to_string());

        let mut nodes = vec![start];
        let mut edges = Vec::new();

        while let Some((current_id, depth)) = queue.pop_front() {
            if depth >= depth_limit {
                continue;
            }

            let mut outgoing = self
                .edges
                .iter()
                .filter(|edge| edge.from == current_id)
                .cloned()
                .collect::<Vec<_>>();
            outgoing.sort_by(|a, b| (&a.relation, &a.to, &a.id).cmp(&(&b.relation, &b.to, &b.id)));

            for edge in outgoing {
                if visited.contains(&edge.to) {
                    continue;
                }

                if let Some(node) = entity_lookup.get(&edge.to) {
                    visited.insert(edge.to.clone());
                    queue.push_back((edge.to.clone(), depth + 1));
                    nodes.push(node.clone());
                    edges.push(edge);
                }
            }
        }

        Ok(OntologyQueryPathResponse { nodes, edges })
    }

    fn sort_deterministic(&mut self) {
        self.entities.sort_by(|a, b| {
            let a_kind = a
                .custom_kind
                .as_deref()
                .unwrap_or_else(|| a.kind.canonical_name());
            let b_kind = b
                .custom_kind
                .as_deref()
                .unwrap_or_else(|| b.kind.canonical_name());
            (a_kind, &a.id).cmp(&(b_kind, &b.id))
        });
        self.edges.sort_by(|a, b| {
            (&a.relation, &a.from, &a.to, &a.id).cmp(&(&b.relation, &b.from, &b.to, &b.id))
        });
    }
}

pub fn entity_content_hash(kind: OntologyEntityKind, attrs: &BTreeMap<String, String>) -> String {
    artifact_content_hash(kind, attrs)
}

/// Compute a deterministic content hash for an entity given its kind name as a string.
/// Works for both built-in ArtifactKind names and custom kind names.
pub fn entity_content_hash_str(kind_name: &str, attrs: &BTreeMap<String, String>) -> String {
    let mut canonical = format!("entity|{kind_name}");
    for (key, value) in attrs {
        canonical.push('|');
        canonical.push_str(key);
        canonical.push('=');
        canonical.push_str(value);
    }
    content_hash(&canonical)
}

pub fn edge_content_hash(
    from: &str,
    to: &str,
    relation: &str,
    provenance: &BTreeMap<String, String>,
) -> String {
    relation_content_hash(from, to, relation, provenance)
}

pub fn content_hash(canonical: &str) -> String {
    ledger_core::ontology::content_hash(canonical)
}
