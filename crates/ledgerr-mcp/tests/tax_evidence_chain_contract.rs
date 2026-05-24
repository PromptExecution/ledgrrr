mod common;

use std::collections::BTreeMap;

use ledger_core::ingest::TransactionInput;
use ledgerr_mcp::{
    ClassifyTransactionRequest, IngestStatementRowsRequest, OntologyEdgeInput, OntologyEntityInput,
    OntologyEntityKind, OntologyUpsertEdgesRequest, OntologyUpsertEntitiesRequest,
    ReconcileExcelClassificationRequest, TaxEvidenceChainRequest, TurboLedgerService,
    TurboLedgerTools,
};

fn service() -> TurboLedgerService {
    let workbook_path = common::unique_workbook_path("tax-evidence");
    TurboLedgerService::from_manifest_str(&common::manifest_for_workbook(&workbook_path, 2023))
        .expect("manifest")
}

fn seed_chain_fixture(
    svc: &TurboLedgerService,
    ontology_path: &std::path::Path,
) -> (String, String, String) {
    let temp = tempfile::tempdir().expect("tempdir");
    let ingest = svc
        .ingest_statement_rows(IngestStatementRowsRequest {
            journal_path: temp.path().join("ledger.beancount"),
            workbook_path: temp.path().join("tax-ledger.xlsx"),
            ontology_path: None,
            rows: vec![TransactionInput {
                account_id: "WF-BH-CHK".to_string(),
                date: "2023-01-15".to_string(),
                amount: "-42.11".to_string(),
                description: "Coffee Shop".to_string(),
                source_ref: "source/wf-2023-01.rkyv".to_string(),
            }],
        })
        .expect("ingest");
    let tx_id = ingest.tx_ids[0].clone();
    svc.classify_transaction(ClassifyTransactionRequest {
        tx_id: tx_id.clone(),
        category: "Meals".to_string(),
        confidence: "0.91".to_string(),
        note: Some("classification".to_string()),
        actor: "agent".to_string(),
    })
    .expect("classify");
    svc.reconcile_excel_classification(ReconcileExcelClassificationRequest {
        tx_id: tx_id.clone(),
        category: "OfficeSupplies".to_string(),
        confidence: "0.88".to_string(),
        actor: "excel-user".to_string(),
        note: Some("reconcile".to_string()),
    })
    .expect("reconcile");

    let mut tx_attrs = BTreeMap::new();
    tx_attrs.insert("tx_id".to_string(), tx_id.clone());
    let mut doc_attrs = BTreeMap::new();
    doc_attrs.insert("doc_ref".to_string(), "source/wf-2023-01.rkyv".to_string());
    let mut review_attrs = BTreeMap::new();
    review_attrs.insert("queue".to_string(), "tax-review".to_string());

    let entities = svc
        .ontology_upsert_entities(OntologyUpsertEntitiesRequest {
            ontology_path: ontology_path.to_path_buf(),
            entities: vec![
                OntologyEntityInput {
                    kind: OntologyEntityKind::Transaction,
                    attrs: tx_attrs,
                    custom_kind: None,
                },
                OntologyEntityInput {
                    kind: OntologyEntityKind::Document,
                    attrs: doc_attrs,
                    custom_kind: None,
                },
                OntologyEntityInput {
                    kind: OntologyEntityKind::EvidenceReference,
                    attrs: review_attrs,
                    custom_kind: None,
                },
            ],
            schema_store_path: None,
        })
        .expect("ontology entities");
    let tx_entity_id = entities.entity_ids[0].clone();
    let doc_entity_id = entities.entity_ids[1].clone();
    let review_entity_id = entities.entity_ids[2].clone();

    let mut source_provenance = BTreeMap::new();
    source_provenance.insert(
        "source_ref".to_string(),
        "source/wf-2023-01.rkyv".to_string(),
    );
    let mut ambiguity_provenance = BTreeMap::new();
    ambiguity_provenance.insert(
        "source_ref".to_string(),
        "source/wf-2023-01.rkyv".to_string(),
    );
    ambiguity_provenance.insert("reason".to_string(), "classification_conflict".to_string());
    svc.ontology_upsert_edges(OntologyUpsertEdgesRequest {
        ontology_path: ontology_path.to_path_buf(),
        edges: vec![
            OntologyEdgeInput {
                from: tx_entity_id.clone(),
                to: doc_entity_id,
                relation: "source_document".to_string(),
                provenance: source_provenance,
            },
            OntologyEdgeInput {
                from: tx_entity_id.clone(),
                to: review_entity_id,
                relation: "ambiguity".to_string(),
                provenance: ambiguity_provenance,
            },
        ],
    })
    .expect("ontology edges");

    (tx_entity_id, tx_id, "source/wf-2023-01.rkyv".to_string())
}

#[test]
fn taxa_02_chain_exposes_source_events_and_current_state_sections() {
    let svc = service();
    let temp = tempfile::tempdir().expect("tempdir");
    let ontology_path = temp.path().join("ontology.json");
    let (from_entity_id, tx_id, document_ref) = seed_chain_fixture(&svc, &ontology_path);

    let response = svc
        .tax_evidence_chain_tool(TaxEvidenceChainRequest {
            ontology_path,
            from_entity_id,
            tx_id: Some(tx_id),
            document_ref: Some(document_ref),
        })
        .expect("evidence chain");

    assert!(!response.source.node_ids.is_empty());
    assert!(!response.source.edge_ids.is_empty());
    assert!(!response.events.is_empty());
    assert!(!response.current_state.reconstructed_state.is_empty());
}

#[test]
fn taxa_02_chain_order_and_state_are_stable_across_repeated_calls() {
    let svc = service();
    let temp = tempfile::tempdir().expect("tempdir");
    let ontology_path = temp.path().join("ontology.json");
    let (from_entity_id, tx_id, document_ref) = seed_chain_fixture(&svc, &ontology_path);

    let first = svc
        .tax_evidence_chain_tool(TaxEvidenceChainRequest {
            ontology_path: ontology_path.clone(),
            from_entity_id: from_entity_id.clone(),
            tx_id: Some(tx_id.clone()),
            document_ref: Some(document_ref.clone()),
        })
        .expect("first call");
    let second = svc
        .tax_evidence_chain_tool(TaxEvidenceChainRequest {
            ontology_path,
            from_entity_id,
            tx_id: Some(tx_id),
            document_ref: Some(document_ref),
        })
        .expect("second call");

    assert_eq!(first.source, second.source);
    assert_eq!(first.events, second.events);
    assert_eq!(first.current_state, second.current_state);
}

#[test]
fn taxa_02_chain_preserves_provenance_refs_and_ambiguity_links() {
    let svc = service();
    let temp = tempfile::tempdir().expect("tempdir");
    let ontology_path = temp.path().join("ontology.json");
    let (from_entity_id, tx_id, document_ref) = seed_chain_fixture(&svc, &ontology_path);

    let response = svc
        .tax_evidence_chain_tool(TaxEvidenceChainRequest {
            ontology_path,
            from_entity_id,
            tx_id: Some(tx_id),
            document_ref: Some(document_ref),
        })
        .expect("evidence chain");

    assert_eq!(
        response.source.provenance_refs,
        vec!["source/wf-2023-01.rkyv".to_string()]
    );
    assert_eq!(response.ambiguity.len(), 1);
    assert_eq!(response.ambiguity[0].review_state, "needs_review");
    assert_eq!(response.ambiguity[0].reason, "ambiguous_tax_treatment");
}
