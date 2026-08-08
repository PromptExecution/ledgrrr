use std::collections::BTreeMap;
use std::path::Path;

use ledger_core::ontology::{
    artifact_content_hash, relation_content_hash, Artifact, OntologySnapshot, PathQueryResult,
    Relation, RelationInput, RelationUpsertResult,
};
use serde::{Deserialize, Serialize};

use crate::ToolError;

pub use ledger_core::ontology::ArtifactKind as OntologyEntityKind;

pub type OntologyStore = OntologySnapshot;
pub type OntologyEntity = Artifact;
pub type OntologyEdge = Relation;

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

fn to_upsert_edges_response(result: RelationUpsertResult) -> OntologyUpsertEdgesResponse {
    OntologyUpsertEdgesResponse {
        inserted_count: result.inserted_count,
        edge_ids: result.ids,
    }
}

fn to_path_query_response(result: PathQueryResult) -> OntologyQueryPathResponse {
    OntologyQueryPathResponse {
        nodes: result.artifacts,
        edges: result.relations,
    }
}

pub fn load_store(path: &Path) -> Result<OntologyStore, ToolError> {
    OntologyStore::load(path).map_err(|e| ToolError::Internal(e.to_string()))
}

pub fn persist_store(store: &OntologyStore, path: &Path) -> Result<(), ToolError> {
    store.persist(path).map_err(|e| ToolError::Internal(e.to_string()))
}

pub fn upsert_entities(
    store: &mut OntologyStore,
    inputs: Vec<OntologyEntityInput>,
) -> Result<OntologyUpsertEntitiesResponse, ToolError> {
    let mut inserted_count = 0usize;
    let mut entity_ids = Vec::with_capacity(inputs.len());

    for input in inputs {
        let id = if let Some(user_id) = input.attrs.get("id") {
            user_id.clone()
        } else if let Some(custom_kind) = input.custom_kind.as_deref() {
            entity_content_hash_str(custom_kind, &input.attrs)
        } else {
            entity_content_hash(input.kind, &input.attrs)
        };

        entity_ids.push(id.clone());
        if store.artifacts.iter().any(|existing| existing.id == id) {
            continue;
        }

        store.artifacts.push(Artifact {
            id,
            kind: input.kind,
            attrs: input.attrs,
        });
        inserted_count += 1;
    }

    store.sort_deterministic();
    Ok(OntologyUpsertEntitiesResponse {
        inserted_count,
        entity_ids,
    })
}

pub fn upsert_edges(
    store: &mut OntologyStore,
    inputs: Vec<OntologyEdgeInput>,
) -> Result<OntologyUpsertEdgesResponse, ToolError> {
    let core_inputs = inputs
        .into_iter()
        .map(|i| RelationInput {
            from: i.from,
            to: i.to,
            relation: i.relation,
            provenance: i.provenance,
        })
        .collect();
    let result = store
        .upsert_relations(core_inputs)
        .map_err(|e| ToolError::Internal(e.to_string()))?;
    Ok(to_upsert_edges_response(result))
}

pub fn query_path(
    store: &OntologyStore,
    from_entity_id: &str,
    max_depth: Option<usize>,
) -> Result<OntologyQueryPathResponse, ToolError> {
    let result = store
        .query_path(from_entity_id, max_depth)
        .map_err(|e| ToolError::Internal(e.to_string()))?;
    Ok(to_path_query_response(result))
}
