mod common;

use std::collections::BTreeMap;

use ledgerr_mcp::{
    OntologyEdgeInput, OntologyEntityInput, OntologyEntityKind, OntologyUpsertEdgesRequest,
    OntologyUpsertEntitiesRequest, ReconciliationStageRequest, TaxAssistRequest,
    TurboLedgerService,
};

fn service() -> TurboLedgerService {
    let workbook_path = common::unique_workbook_path("tax-assist");
    TurboLedgerService::from_manifest_str(&common::manifest_for_workbook(&workbook_path, 2023))
        .expect("manifest")
}

fn seed_tax_ontology(
    svc: &TurboLedgerService,
    ontology_path: &std::path::Path,
) -> (String, String, String) {
    let mut doc_attrs = BTreeMap::new();
    doc_attrs.insert("doc_ref".to_string(), "source/wf-2023-01.rkyv".to_string());
    let mut tx_attrs = BTreeMap::new();
    tx_attrs.insert("tx_id".to_string(), "tx-001".to_string());
    tx_attrs.insert("amount".to_string(), "-42.11".to_string());
    let mut tax_attrs = BTreeMap::new();
    tax_attrs.insert("code".to_string(), "OfficeSupplies".to_string());

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
                OntologyEntityInput {
                    kind: OntologyEntityKind::TaxCategory,
                    attrs: tax_attrs,
                    custom_kind: None,
                },
            ],
            schema_store_path: None,
        })
        .expect("seed entities");

    let document_id = entities.entity_ids[0].clone();
    let transaction_id = entities.entity_ids[1].clone();
    let tax_category_id = entities.entity_ids[2].clone();

    let mut schedule_provenance = BTreeMap::new();
    schedule_provenance.insert(
        "source_ref".to_string(),
        "source/wf-2023-01.rkyv".to_string(),
    );
    schedule_provenance.insert("schedule".to_string(), "ScheduleC".to_string());
    let mut fbar_provenance = BTreeMap::new();
    fbar_provenance.insert(
        "source_ref".to_string(),
        "source/wf-2023-01.rkyv".to_string(),
    );
    fbar_provenance.insert("schedule".to_string(), "FBAR".to_string());
    let mut ambiguous_provenance = BTreeMap::new();
    ambiguous_provenance.insert(
        "source_ref".to_string(),
        "source/wf-2023-01.rkyv".to_string(),
    );
    ambiguous_provenance.insert("reason".to_string(), "rule_conflict".to_string());

    svc.ontology_upsert_edges(OntologyUpsertEdgesRequest {
        ontology_path: ontology_path.to_path_buf(),
        edges: vec![
            OntologyEdgeInput {
                from: transaction_id.clone(),
                to: tax_category_id.clone(),
                relation: "schedule_c".to_string(),
                provenance: schedule_provenance,
            },
            OntologyEdgeInput {
                from: transaction_id.clone(),
                to: document_id.clone(),
                relation: "fbar_reportable".to_string(),
                provenance: fbar_provenance,
            },
            OntologyEdgeInput {
                from: transaction_id.clone(),
                to: tax_category_id,
                relation: "ambiguity".to_string(),
                provenance: ambiguous_provenance,
            },
        ],
    })
    .expect("seed edges");

    (document_id, transaction_id, entities.entity_ids[2].clone())
}

#[test]
fn taxa_01_blocks_tax_assist_until_reconciliation_is_ready() {
    let svc = service();
    let temp = tempfile::tempdir().expect("tempdir");
    let ontology_path = temp.path().join("ontology.json");
    let (_, tx_id, _) = seed_tax_ontology(&svc, &ontology_path);

    let blocked = svc
        .tax_assist_tool(TaxAssistRequest {
            ontology_path,
            from_entity_id: tx_id,
            max_depth: Some(4),
            reconciliation: ReconciliationStageRequest {
                source_total: "100.00".to_string(),
                extracted_total: "99.00".to_string(),
                posting_amounts: vec!["-100.00".to_string(), "100.00".to_string()],
            },
        })
        .expect("tax assist");

    assert_eq!(blocked.status, "blocked");
    assert_eq!(blocked.blocked_reasons, vec!["totals_mismatch".to_string()]);
    assert!(blocked.schedule_rows.is_empty());
    assert!(blocked.fbar_rows.is_empty());
}

#[test]
fn taxa_01_success_derives_deterministic_schedule_and_fbar_rows_from_ontology() {
    let svc = service();
    let temp = tempfile::tempdir().expect("tempdir");
    let ontology_path = temp.path().join("ontology.json");
    let (_, tx_id, _) = seed_tax_ontology(&svc, &ontology_path);

    let response = svc
        .tax_assist_tool(TaxAssistRequest {
            ontology_path,
            from_entity_id: tx_id.clone(),
            max_depth: Some(4),
            reconciliation: ReconciliationStageRequest {
                source_total: "100.00".to_string(),
                extracted_total: "100.00".to_string(),
                posting_amounts: vec!["-100.00".to_string(), "100.00".to_string()],
            },
        })
        .expect("tax assist");

    assert_eq!(response.status, "ready");
    assert_eq!(response.summary.source_entity_id, tx_id);
    assert_eq!(response.summary.schedule_row_count, 1);
    assert_eq!(response.summary.fbar_row_count, 1);
    assert_eq!(response.schedule_rows.len(), 1);
    assert_eq!(response.fbar_rows.len(), 1);
    assert_eq!(response.schedule_rows[0].section, "schedule");
    assert_eq!(response.schedule_rows[0].relation, "schedule_c");
    assert_eq!(response.fbar_rows[0].section, "fbar");
    assert_eq!(response.fbar_rows[0].relation, "fbar_reportable");
}

#[test]
fn taxa_03_ambiguity_payload_has_review_state_reason_and_provenance_links() {
    let svc = service();
    let temp = tempfile::tempdir().expect("tempdir");
    let ontology_path = temp.path().join("ontology.json");
    let (_, tx_id, _) = seed_tax_ontology(&svc, &ontology_path);

    let response = svc
        .tax_assist_tool(TaxAssistRequest {
            ontology_path,
            from_entity_id: tx_id,
            max_depth: Some(4),
            reconciliation: ReconciliationStageRequest {
                source_total: "100.00".to_string(),
                extracted_total: "100.00".to_string(),
                posting_amounts: vec!["-100.00".to_string(), "100.00".to_string()],
            },
        })
        .expect("tax assist");

    assert_eq!(response.summary.ambiguity_count, 1);
    assert_eq!(response.ambiguity.len(), 1);
    assert_eq!(response.ambiguity[0].review_state, "needs_review");
    assert_eq!(response.ambiguity[0].reason, "ambiguous_tax_treatment");
    assert_eq!(
        response.ambiguity[0].provenance_refs,
        vec!["source/wf-2023-01.rkyv".to_string()]
    );
}
