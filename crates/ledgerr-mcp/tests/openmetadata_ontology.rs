use ledgerr_mcp::openmetadata::{
    apply_import, import_snapshot, OpenMetadataEntitySnapshot, OpenMetadataImportSnapshot,
    OpenMetadataRelationshipSnapshot, KIND_COLUMN, KIND_TABLE,
};
use ledgerr_mcp::OntologyStore;

fn sample_snapshot() -> OpenMetadataImportSnapshot {
    OpenMetadataImportSnapshot {
        source_ref: "openmetadata://unit-test".to_string(),
        schema_version: "1.12.x".to_string(),
        entities: vec![OpenMetadataEntitySnapshot {
            entity_type: "table".to_string(),
            fully_qualified_name: "bigquery.shop.public.orders".to_string(),
            openmetadata_id: Some("11111111-1111-1111-1111-111111111111".to_string()),
            href: Some("https://metadata.example.com/table/orders".to_string()),
            service: Some("bigquery".to_string()),
            database: Some("shop".to_string()),
            database_schema: Some("public".to_string()),
            table: Some("orders".to_string()),
            columns: vec!["order_id".to_string(), "customer_id".to_string()],
            tags: vec!["PII.Sensitive".to_string()],
            glossary_terms: vec!["BusinessGlossary.Customer".to_string()],
            owners: vec!["data-team".to_string()],
        }],
        relationships: vec![OpenMetadataRelationshipSnapshot {
            from_entity_type: "table".to_string(),
            from_fqn: "bigquery.shop.public.orders".to_string(),
            to_entity_type: "table".to_string(),
            to_fqn: "bigquery.shop.public.customers".to_string(),
            relation: "upstream".to_string(),
            relationship_source: Some("lineage".to_string()),
        }],
    }
}

#[test]
fn openmetadata_snapshot_import_is_idempotent() {
    let snapshot = sample_snapshot();
    let import = import_snapshot(&snapshot);
    assert!(import
        .entities
        .iter()
        .any(|entity| entity.custom_kind.as_deref() == Some(KIND_TABLE)));
    assert!(import
        .entities
        .iter()
        .any(|entity| entity.custom_kind.as_deref() == Some(KIND_COLUMN)));
    assert!(import
        .edges
        .iter()
        .any(|edge| edge.relation == "has_column"));
    assert!(import
        .edges
        .iter()
        .any(|edge| edge.relation == "lineage_upstream_of"));

    let mut store = OntologyStore::default();
    let first = apply_import(&mut store, import.clone()).expect("first import should apply");
    let second = apply_import(&mut store, import).expect("second import should apply");

    assert!(first.0 > 0, "first import should insert entities");
    assert!(first.1 > 0, "first import should insert edges");
    assert_eq!(second, (0, 0), "second import should be idempotent");
}

#[test]
fn openmetadata_import_keeps_volatile_fields_out_of_entity_identity() {
    let mut first = sample_snapshot();
    let mut second = sample_snapshot();
    first.entities[0].openmetadata_id = Some("first-id".to_string());
    first.entities[0].href = Some("https://metadata.example.com/first".to_string());
    second.entities[0].openmetadata_id = Some("second-id".to_string());
    second.entities[0].href = Some("https://metadata.example.com/second".to_string());

    let first_import = import_snapshot(&first);
    let second_import = import_snapshot(&second);
    let first_table = first_import
        .entities
        .iter()
        .find(|entity| entity.custom_kind.as_deref() == Some(KIND_TABLE))
        .expect("table entity should exist");
    let second_table = second_import
        .entities
        .iter()
        .find(|entity| entity.custom_kind.as_deref() == Some(KIND_TABLE))
        .expect("table entity should exist");

    assert_eq!(first_table.attrs, second_table.attrs);
}
