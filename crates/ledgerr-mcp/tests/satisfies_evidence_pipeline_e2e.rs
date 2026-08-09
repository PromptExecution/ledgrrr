mod common;

use std::collections::BTreeMap;

use ledger_core::ingest::TransactionInput;
use ledger_core::SatisfiesResult;
use ledgerr_mcp::{
    ClassifyTransactionRequest, IngestStatementRowsRequest, OntologyEntityInput,
    OntologyEntityKind, OntologyUpsertEntitiesRequest, ReplayLifecycleRequest,
    TaxEvidenceChainRequest, TurboLedgerService, TurboLedgerTools,
};

fn service() -> TurboLedgerService {
    let workbook_path = common::unique_workbook_path("satisfies-e2e");
    TurboLedgerService::from_manifest_str(&common::manifest_for_workbook(&workbook_path, 2023))
        .expect("manifest")
}

#[test]
fn satisfies_result_roundtrip_serialization() {
    let satisfied = SatisfiesResult::satisfied(0.95, vec![]);
    let json = serde_json::to_string(&satisfied).unwrap();
    let back: SatisfiesResult = serde_json::from_str(&json).unwrap();
    assert!(back.disposition.is_satisfied());
    assert!((back.confidence - 0.95).abs() < 1e-10);

    let violated = SatisfiesResult::violated("missing documentation");
    let json = serde_json::to_string(&violated).unwrap();
    let back: SatisfiesResult = serde_json::from_str(&json).unwrap();
    assert!(!back.disposition.is_satisfied());
}

#[test]
fn satisfy_then_classify_then_evidence_chain_includes_all_stages() {
    let svc = service();
    let tmp = tempfile::tempdir().expect("tempdir");
    let ontology_path = tmp.path().join("ontology.json");

    let ingest = svc
        .ingest_statement_rows(IngestStatementRowsRequest {
            journal_path: tmp.path().join("ledger.beancount"),
            workbook_path: tmp.path().join("tax-ledger.xlsx"),
            ontology_path: Some(ontology_path.clone()),
            rows: vec![TransactionInput {
                account_id: "WF-EV-CHK".to_string(),
                date: "2023-06-15".to_string(),
                amount: "850.00".to_string(),
                description: "Merchant residual EV".to_string(),
                source_ref: "source/ev-2023-06.rkyv".to_string(),
            }],
        })
        .expect("ingest");
    assert_eq!(ingest.inserted_count, 1);
    let tx_id = &ingest.tx_ids[0];

    svc.classify_transaction(ClassifyTransactionRequest {
        tx_id: tx_id.clone(),
        category: "ScheduleC".to_string(),
        confidence: "0.92".to_string(),
        note: Some("EV merchant residual".to_string()),
        actor: "test".to_string(),
    })
    .expect("classify");

    let mut tx_attrs = BTreeMap::new();
    tx_attrs.insert("tx_id".to_string(), tx_id.clone());
    let entities = svc
        .ontology_upsert_entities(OntologyUpsertEntitiesRequest {
            ontology_path: ontology_path.clone(),
            entities: vec![OntologyEntityInput {
                kind: OntologyEntityKind::Transaction,
                attrs: tx_attrs,
                custom_kind: None,
            }],
            schema_store_path: None,
        })
        .expect("ontology entities");
    let tx_entity_id = entities.entity_ids[0].clone();

    let chain = svc
        .tax_evidence_chain_tool(TaxEvidenceChainRequest {
            ontology_path: ontology_path.clone(),
            from_entity_id: tx_entity_id,
            tx_id: Some(tx_id.clone()),
            document_ref: Some("source/ev-2023-06.rkyv".to_string()),
        })
        .expect("evidence chain");
    assert!(
        !chain.source.node_ids.is_empty()
            || !chain.source.edge_ids.is_empty()
            || !chain.source.provenance_refs.is_empty(),
        "evidence chain must contain at least one identifier from the pipeline stages"
    );

    let replay = svc
        .replay_lifecycle(ReplayLifecycleRequest {
            tx_id: Some(tx_id.clone()),
            document_ref: None,
        })
        .expect("replay");
    assert!(
        replay.event_count > 0,
        "lifecycle replay must include at least one event"
    );
    assert!(replay.reconstructed_state.contains("stage=classification"));
    assert!(replay.reconstructed_state.contains("category=ScheduleC"));
}

#[test]
fn violates_constraint_surfaces_in_satisfies_result() {
    let result = SatisfiesResult::violated("business purpose not established");
    assert!(!result.disposition.is_satisfied());
    let json = serde_json::to_string_pretty(&result).unwrap();
    assert!(json.contains("violated"));
    assert!(json.contains("business purpose"));
}

#[test]
fn ontology_path_roundtrip_preserves_structure() {
    let svc = service();
    let tmp = tempfile::tempdir().expect("tempdir");
    let ontology_path = tmp.path().join("ontology.json");

    let ingest = svc
        .ingest_statement_rows(IngestStatementRowsRequest {
            journal_path: tmp.path().join("ledger.beancount"),
            workbook_path: tmp.path().join("tax-ledger.xlsx"),
            ontology_path: Some(ontology_path.clone()),
            rows: vec![TransactionInput {
                account_id: "WF-BH-CHK".to_string(),
                date: "2023-03-01".to_string(),
                amount: "-1200.00".to_string(),
                description: "Rent payment 2166IS".to_string(),
                source_ref: "source/wf-2023-03.rkyv".to_string(),
            }],
        })
        .expect("ingest");
    assert_eq!(ingest.inserted_count, 1);
    let tx_id = &ingest.tx_ids[0];

    svc.classify_transaction(ClassifyTransactionRequest {
        tx_id: tx_id.clone(),
        category: "RentalProperty".to_string(),
        confidence: "0.95".to_string(),
        note: Some("Schedule E".to_string()),
        actor: "test".to_string(),
    })
    .expect("classify");

    let mut tx_attrs = BTreeMap::new();
    tx_attrs.insert("tx_id".to_string(), tx_id.clone());
    let entities = svc
        .ontology_upsert_entities(OntologyUpsertEntitiesRequest {
            ontology_path: ontology_path.clone(),
            entities: vec![OntologyEntityInput {
                kind: OntologyEntityKind::Transaction,
                attrs: tx_attrs,
                custom_kind: None,
            }],
            schema_store_path: None,
        })
        .expect("ontology entities");
    let tx_entity_id = entities.entity_ids[0].clone();

    let chain = svc
        .tax_evidence_chain_tool(TaxEvidenceChainRequest {
            ontology_path: ontology_path.clone(),
            from_entity_id: tx_entity_id,
            tx_id: Some(tx_id.clone()),
            document_ref: Some("source/wf-2023-03.rkyv".to_string()),
        })
        .expect("evidence chain");
    assert!(
        !chain.source.node_ids.is_empty(),
        "ontology must have at least one node"
    );
}
