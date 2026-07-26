use std::collections::BTreeMap;

use ledger_core::ontology::ArtifactKind;
use serde::{Deserialize, Serialize};

use crate::ontology::{OntologyEdgeInput, OntologyEntityInput, OntologyStore};
use crate::ToolError;

pub const SOURCE_SYSTEM: &str = "openmetadata";
pub const KIND_SERVICE: &str = "openmetadata_service";
pub const KIND_DATABASE: &str = "openmetadata_database";
pub const KIND_DATABASE_SCHEMA: &str = "openmetadata_database_schema";
pub const KIND_TABLE: &str = "openmetadata_table";
pub const KIND_COLUMN: &str = "openmetadata_column";
pub const KIND_TAG: &str = "openmetadata_tag";
pub const KIND_GLOSSARY_TERM: &str = "openmetadata_glossary_term";
pub const KIND_OWNER: &str = "openmetadata_owner";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenMetadataEntitySnapshot {
    pub entity_type: String,
    pub fully_qualified_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openmetadata_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub href: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database_schema: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<String>,
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub glossary_terms: Vec<String>,
    #[serde(default)]
    pub owners: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenMetadataRelationshipSnapshot {
    pub from_entity_type: String,
    pub from_fqn: String,
    pub to_entity_type: String,
    pub to_fqn: String,
    pub relation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relationship_source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenMetadataImportSnapshot {
    pub source_ref: String,
    #[serde(default)]
    pub schema_version: String,
    #[serde(default)]
    pub entities: Vec<OpenMetadataEntitySnapshot>,
    #[serde(default)]
    pub relationships: Vec<OpenMetadataRelationshipSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenMetadataOntologyImport {
    pub entities: Vec<OntologyEntityInput>,
    pub edges: Vec<OntologyEdgeInput>,
}

pub fn openmetadata_entity_kind(entity_type: &str) -> &'static str {
    match entity_type {
        "service" => KIND_SERVICE,
        "databaseService" | "messagingService" | "apiService" | "dashboardService"
        | "pipelineService" | "storageService" | "mlmodelService" | "metadataService"
        | "searchService" => KIND_SERVICE,
        "database" => KIND_DATABASE,
        "databaseSchema" => KIND_DATABASE_SCHEMA,
        "table" => KIND_TABLE,
        "column" => KIND_COLUMN,
        "tag" => KIND_TAG,
        "glossaryTerm" => KIND_GLOSSARY_TERM,
        "owner" | "user" | "team" => KIND_OWNER,
        _ => KIND_TABLE,
    }
}

pub fn import_snapshot(snapshot: &OpenMetadataImportSnapshot) -> OpenMetadataOntologyImport {
    let mut entities = Vec::new();
    let mut edges = Vec::new();

    for entity in &snapshot.entities {
        entities.push(entity_input(
            openmetadata_entity_kind(&entity.entity_type),
            &entity.entity_type,
            &entity.fully_qualified_name,
            entity.openmetadata_id.as_deref(),
            entity.href.as_deref(),
        ));

        for column in &entity.columns {
            let column_fqn = format!("{}.{}", entity.fully_qualified_name, column);
            entities.push(entity_input(KIND_COLUMN, "column", &column_fqn, None, None));
            edges.push(edge_input(
                &entity.entity_type,
                &entity.fully_qualified_name,
                "column",
                &column_fqn,
                "has_column",
                snapshot,
                Some("table.columns"),
            ));
        }

        for tag in &entity.tags {
            entities.push(entity_input(KIND_TAG, "tag", tag, None, None));
            edges.push(edge_input(
                &entity.entity_type,
                &entity.fully_qualified_name,
                "tag",
                tag,
                "tagged_as",
                snapshot,
                Some("tagLabel"),
            ));
        }

        for term in &entity.glossary_terms {
            entities.push(entity_input(
                KIND_GLOSSARY_TERM,
                "glossaryTerm",
                term,
                None,
                None,
            ));
            edges.push(edge_input(
                &entity.entity_type,
                &entity.fully_qualified_name,
                "glossaryTerm",
                term,
                "has_glossary_term",
                snapshot,
                Some("glossaryTerm"),
            ));
        }

        for owner in &entity.owners {
            entities.push(entity_input(KIND_OWNER, "owner", owner, None, None));
            edges.push(edge_input(
                &entity.entity_type,
                &entity.fully_qualified_name,
                "owner",
                owner,
                "owned_by",
                snapshot,
                Some("entityReference"),
            ));
        }

        add_hierarchy_edges(entity, snapshot, &mut entities, &mut edges);
    }

    for relationship in &snapshot.relationships {
        entities.push(entity_input(
            openmetadata_entity_kind(&relationship.from_entity_type),
            &relationship.from_entity_type,
            &relationship.from_fqn,
            None,
            None,
        ));
        entities.push(entity_input(
            openmetadata_entity_kind(&relationship.to_entity_type),
            &relationship.to_entity_type,
            &relationship.to_fqn,
            None,
            None,
        ));
        edges.push(edge_input(
            &relationship.from_entity_type,
            &relationship.from_fqn,
            &relationship.to_entity_type,
            &relationship.to_fqn,
            &normalize_relation(&relationship.relation),
            snapshot,
            relationship.relationship_source.as_deref(),
        ));
    }

    OpenMetadataOntologyImport { entities, edges }
}

pub fn apply_import(
    store: &mut OntologyStore,
    import: OpenMetadataOntologyImport,
) -> Result<(usize, usize), ToolError> {
    let entity_response = store.upsert_entities(import.entities, None)?;
    let edge_response = store.upsert_edges(import.edges)?;
    Ok((entity_response.inserted_count, edge_response.inserted_count))
}

fn add_hierarchy_edges(
    entity: &OpenMetadataEntitySnapshot,
    snapshot: &OpenMetadataImportSnapshot,
    entities: &mut Vec<OntologyEntityInput>,
    edges: &mut Vec<OntologyEdgeInput>,
) {
    if let (Some(service), Some(database)) = (&entity.service, &entity.database) {
        let service_fqn = service.clone();
        let database_fqn = format!("{service}.{database}");
        entities.push(entity_input(
            KIND_SERVICE,
            "service",
            &service_fqn,
            None,
            None,
        ));
        entities.push(entity_input(
            KIND_DATABASE,
            "database",
            &database_fqn,
            None,
            None,
        ));
        edges.push(edge_input(
            "service",
            &service_fqn,
            "database",
            &database_fqn,
            "contains",
            snapshot,
            Some("entityReference"),
        ));
    }

    if let (Some(database), Some(database_schema)) = (&entity.database, &entity.database_schema) {
        let database_fqn = entity
            .service
            .as_ref()
            .map(|service| format!("{service}.{database}"))
            .unwrap_or_else(|| database.clone());
        let schema_fqn = format!("{database_fqn}.{database_schema}");
        entities.push(entity_input(
            KIND_DATABASE,
            "database",
            &database_fqn,
            None,
            None,
        ));
        entities.push(entity_input(
            KIND_DATABASE_SCHEMA,
            "databaseSchema",
            &schema_fqn,
            None,
            None,
        ));
        edges.push(edge_input(
            "database",
            &database_fqn,
            "databaseSchema",
            &schema_fqn,
            "contains",
            snapshot,
            Some("entityReference"),
        ));
    }

    if let (Some(database_schema), Some(table)) = (&entity.database_schema, &entity.table) {
        let schema_fqn = match (&entity.service, &entity.database) {
            (Some(service), Some(database)) => format!("{service}.{database}.{database_schema}"),
            (_, Some(database)) => format!("{database}.{database_schema}"),
            _ => database_schema.clone(),
        };
        let table_fqn = if entity.fully_qualified_name.is_empty() {
            format!("{schema_fqn}.{table}")
        } else {
            entity.fully_qualified_name.clone()
        };
        entities.push(entity_input(
            KIND_DATABASE_SCHEMA,
            "databaseSchema",
            &schema_fqn,
            None,
            None,
        ));
        entities.push(entity_input(KIND_TABLE, "table", &table_fqn, None, None));
        edges.push(edge_input(
            "databaseSchema",
            &schema_fqn,
            "table",
            &table_fqn,
            "contains",
            snapshot,
            Some("entityReference"),
        ));
    }
}

fn entity_input(
    custom_kind: &str,
    entity_type: &str,
    fully_qualified_name: &str,
    _openmetadata_id: Option<&str>,
    _href: Option<&str>,
) -> OntologyEntityInput {
    let mut attrs = BTreeMap::new();
    attrs.insert("source_system".to_string(), SOURCE_SYSTEM.to_string());
    attrs.insert("entity_type".to_string(), entity_type.to_string());
    attrs.insert(
        "fully_qualified_name".to_string(),
        fully_qualified_name.to_string(),
    );
    OntologyEntityInput {
        kind: ArtifactKind::EvidenceReference,
        attrs,
        custom_kind: Some(custom_kind.to_string()),
    }
}

fn edge_input(
    from_entity_type: &str,
    from_fqn: &str,
    to_entity_type: &str,
    to_fqn: &str,
    relation: &str,
    snapshot: &OpenMetadataImportSnapshot,
    relationship_source: Option<&str>,
) -> OntologyEdgeInput {
    let mut provenance = BTreeMap::new();
    provenance.insert("source_system".to_string(), SOURCE_SYSTEM.to_string());
    provenance.insert("source_ref".to_string(), snapshot.source_ref.clone());
    if !snapshot.schema_version.is_empty() {
        provenance.insert(
            "schema_version".to_string(),
            snapshot.schema_version.clone(),
        );
    }
    if let Some(source) = relationship_source {
        provenance.insert("relationship_source".to_string(), source.to_string());
    }

    OntologyEdgeInput {
        from: entity_id(from_entity_type, from_fqn),
        to: entity_id(to_entity_type, to_fqn),
        relation: relation.to_string(),
        provenance,
    }
}

fn entity_id(entity_type: &str, fully_qualified_name: &str) -> String {
    let input = entity_input(
        openmetadata_entity_kind(entity_type),
        entity_type,
        fully_qualified_name,
        None,
        None,
    );
    crate::ontology::entity_content_hash_str(
        input
            .custom_kind
            .as_deref()
            .unwrap_or_else(|| input.kind.canonical_name()),
        &input.attrs,
    )
}

fn normalize_relation(relation: &str) -> String {
    match relation {
        "upstream" | "lineage_upstream_of" => "lineage_upstream_of".to_string(),
        "downstream" | "lineage_downstream_of" => "lineage_downstream_of".to_string(),
        "owns" | "owner" | "owned_by" => "owned_by".to_string(),
        other => other.to_string(),
    }
}
