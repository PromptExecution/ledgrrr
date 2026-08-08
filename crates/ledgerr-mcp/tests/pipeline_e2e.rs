mod common;

use std::collections::BTreeMap;
use std::path::Path;

use ledgerr_mcp::{
    IngestStatementRowsRequest, OntologyEdgeInput, OntologyEntityInput, OntologyEntityKind,
    OntologyStore, OntologyUpsertEdgesRequest, OntologyUpsertEntitiesRequest,
    SchemaStore, TaxAssistRequest, TurboLedgerService, TurboLedgerTools,
};

fn service() -> TurboLedgerService {
    let workbook_path = common::unique_workbook_path("pipeline-e2e");
    TurboLedgerService::from_manifest_str(&common::manifest_for_workbook(&workbook_path, 2023))
        .expect("manifest")
}

// ── SchemaStore roundtrip ────────────────────────────────────────────────────

#[test]
fn pipe_schema_store_roundtrip_persist_and_load_custom_kinds() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let schema_path = tmp.path().join("schema.json");

    // Register a custom kind and persist
    let mut store = SchemaStore::default();
    store
        .register_kind("custom_entity", "A custom pipeline entity", BTreeMap::new())
        .expect("register custom kind");
    assert!(store.is_known_kind("custom_entity"));
    assert_eq!(store.kinds.custom.len(), 1);
    store.persist(&schema_path).expect("persist schema store");

    // Load from disk and verify kind is preserved
    let loaded = SchemaStore::load(&schema_path).expect("load schema store");
    assert!(loaded.is_known_kind("custom_entity"));
    assert_eq!(loaded.kinds.custom.len(), 1);
    assert_eq!(loaded.kinds.custom[0].name, "custom_entity");
    assert!(loaded.is_known_kind("document"));
    assert!(loaded.is_known_kind("transaction"));
}

#[test]
fn pipe_schema_store_upsert_entities_with_schema_validation() {
    let svc = service();
    let tmp = tempfile::tempdir().expect("tempdir");
    let ontology_path = tmp.path().join("ontology.json");
    let schema_path = tmp.path().join("schema.json");

    // Set up a SchemaStore with a custom kind registered
    let mut schema_store = SchemaStore::default();
    schema_store
        .register_kind("custom_entity", "A custom pipeline entity", BTreeMap::new())
        .expect("register kind");
    assert!(!schema_store.is_known_kind("unknown_type"));
    schema_store.persist(&schema_path).expect("persist schema");

    // Upsert with schema_store_path should succeed for known kinds
    let mut attrs = BTreeMap::new();
    attrs.insert("id".to_string(), "custom-001".to_string());
    let result = svc
        .ontology_upsert_entities(OntologyUpsertEntitiesRequest {
            ontology_path: ontology_path.clone(),
            entities: vec![OntologyEntityInput {
                kind: OntologyEntityKind::Document,
                attrs,
                custom_kind: Some("custom_entity".to_string()),
            }],
            schema_store_path: Some(schema_path.clone()),
        })
        .expect("upsert entities with schema validation");
    assert_eq!(result.inserted_count, 1);
}

#[test]
fn pipe_schema_store_upsert_without_schema_skips_validation() {
    let svc = service();
    let tmp = tempfile::tempdir().expect("tempdir");
    let ontology_path = tmp.path().join("ontology.json");

    // Upsert with schema_store_path: None should succeed for any kind
    let mut attrs = BTreeMap::new();
    attrs.insert("id".to_string(), "any-001".to_string());
    let result = svc
        .ontology_upsert_entities(OntologyUpsertEntitiesRequest {
            ontology_path: ontology_path.clone(),
            entities: vec![OntologyEntityInput {
                kind: OntologyEntityKind::Transaction,
                attrs,
                custom_kind: Some("unknown_type".to_string()),
            }],
            schema_store_path: None,
        })
        .expect("upsert entities without schema");
    assert_eq!(result.inserted_count, 1);
}

// ── Basic pipeline: ingest → classify → ontology flow ────────────────────────

#[test]
fn pipe_ingest_classify_ontology_flow_produces_deterministic_results() {
    let svc = service();
    let tmp = tempfile::tempdir().expect("tempdir");
    let journal_path = tmp.path().join("ledger.beancount");
    let workbook_path = tmp.path().join("tax-ledger.xlsx");
    let ontology_path = tmp.path().join("ontology.json");

    // Step 1: Ingest a statement row
    let ingest = svc
        .ingest_statement_rows(IngestStatementRowsRequest {
            journal_path: journal_path.clone(),
            workbook_path: workbook_path.clone(),
            ontology_path: Some(ontology_path.clone()),
            rows: vec![ledger_core::ingest::TransactionInput {
                account_id: "WF-BH-CHK".to_string(),
                date: "2023-01-15".to_string(),
                amount: "-42.11".to_string(),
                description: "Office Supplies".to_string(),
                source_ref: "source/wf-ctx.rkyv".to_string(),
            }],
        })
        .expect("ingest rows");
    assert_eq!(ingest.inserted_count, 1);
    assert_eq!(ingest.tx_ids.len(), 1);
    let tx_id = &ingest.tx_ids[0];

    // Step 2: Classify the transaction
    let classify = svc
        .classify_transaction(ledgerr_mcp::ClassifyTransactionRequest {
            tx_id: tx_id.clone(),
            category: "OfficeSupplies".to_string(),
            confidence: "0.95".to_string(),
            note: Some("e2e test classification".to_string()),
            actor: "pipeline-test".to_string(),
        })
        .expect("classify transaction");
    assert_eq!(classify.tx_id, *tx_id);
    assert!(!classify.audit_entries.is_empty());

    // Step 3: Verify ontology was populated by ingest
    let store = OntologyStore::load(&ontology_path).expect("load ontology");
    assert!(
        !store.artifacts.is_empty(),
        "ontology should have entities"
    );
    let document_entities: Vec<_> = store
        .artifacts
        .iter()
        .filter(|e| e.kind == OntologyEntityKind::Document)
        .collect();
    assert_eq!(
        document_entities.len(),
        1,
        "one document entity from ingest"
    );
    let tx_entities: Vec<_> = store
        .artifacts
        .iter()
        .filter(|e| e.kind == OntologyEntityKind::Transaction)
        .collect();
    assert_eq!(
        tx_entities.len(),
        1,
        "one transaction entity from ingest"
    );
    assert_eq!(
        store.relations.len(),
        1,
        "one edge from document to transaction"
    );
    assert_eq!(store.relations[0].relation, "documents_transaction");
}

// ── Viz-manifest entry count verification ────────────────────────────────────

#[test]
fn pipe_viz_manifest_entry_count_matches_registered_types() {
    // The checked-in ui/docs/public/viz-manifest.json is a build artifact
    // generated by `cargo run -p xtask-mcpb -- export-viz-manifest` from the
    // `impl HasVisualization` blocks in `ledger_core::iso_objects`. It must be
    // regenerated (and this count updated) whenever an impl is added, removed,
    // or registered/deregistered in xtask/src/viz_manifest.rs.
    const EXPECTED_ENTRY_COUNT: usize = 28;

    let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("ui")
        .join("docs")
        .join("public")
        .join("viz-manifest.json");

    if manifest_path.exists() {
        let raw = std::fs::read_to_string(&manifest_path).expect("read viz-manifest.json");
        let manifest: serde_json::Value =
            serde_json::from_str(&raw).expect("parse viz-manifest.json");
        let objects = manifest["objects"]
            .as_array()
            .expect("objects array in manifest");
        assert_eq!(
            objects.len(),
            EXPECTED_ENTRY_COUNT,
            "VizManifest entry count drifted from xtask/src/viz_manifest.rs — \
             regenerate via `cargo run -p xtask-mcpb -- export-viz-manifest` \
             and update EXPECTED_ENTRY_COUNT if this is an intentional change"
        );

        // Verify representative entries from each domain area exist.
        let names: Vec<&str> = objects
            .iter()
            .filter_map(|o| o["type_name"].as_str())
            .collect();
        for expected in [
            "PipelineState<Ingested>",
            "KasuariSolver",
            "AuRdActivity",
            "UsRdcCredit",
            "CryptoWallet",
        ] {
            assert!(names.contains(&expected), "missing {expected} entry");
        }
    }
}

// ── Pipeline state machine integration ───────────────────────────────────────

#[test]
fn pipe_tax_assist_requires_reconciliation_before_proceeding() {
    let svc = service();
    let tmp = tempfile::tempdir().expect("tempdir");
    let ontology_path = tmp.path().join("ontology.json");

    // Seed minimal ontology
    let mut doc_attrs = BTreeMap::new();
    doc_attrs.insert("doc_ref".to_string(), "source/statement.rkyv".to_string());
    let mut tx_attrs = BTreeMap::new();
    tx_attrs.insert("tx_id".to_string(), "pipeline-tx-001".to_string());
    let entities = svc
        .ontology_upsert_entities(OntologyUpsertEntitiesRequest {
            ontology_path: ontology_path.to_path_buf(),
            entities: vec![
                OntologyEntityInput {
                    kind: OntologyEntityKind::Document,
                    attrs: doc_attrs,
                    custom_kind: None,
                },
                OntologyEntityInput {
                    kind: OntologyEntityKind::Transaction,
                    attrs: tx_attrs,
                    custom_kind: None,
                },
            ],
            schema_store_path: None,
        })
        .expect("seed entities for pipeline");
    let doc_id = entities.entity_ids[0].clone();
    let tx_id = entities.entity_ids[1].clone();

    // Link with an edge
    svc.ontology_upsert_edges(OntologyUpsertEdgesRequest {
        ontology_path: ontology_path.clone(),
        edges: vec![OntologyEdgeInput {
            from: doc_id,
            to: tx_id.clone(),
            relation: "documents_transaction".to_string(),
            provenance: BTreeMap::new(),
        }],
    })
    .expect("seed edge");

    // Attempt tax assist with mismatched totals — should be blocked
    let blocked = svc
        .tax_assist_tool(TaxAssistRequest {
            ontology_path: ontology_path.clone(),
            from_entity_id: tx_id,
            max_depth: Some(4),
            reconciliation: ledgerr_mcp::ReconciliationStageRequest {
                source_total: "100.00".to_string(),
                extracted_total: "99.00".to_string(),
                posting_amounts: vec!["-100.00".to_string(), "100.00".to_string()],
            },
        })
        .expect("tax assist with mismatch");
    assert_eq!(blocked.status, "blocked");
    assert!(
        blocked.blocked_reasons.contains(&"totals_mismatch".to_string()),
        "expected totals_mismatch in blocked reasons: {:?}",
        blocked.blocked_reasons
    );
}
