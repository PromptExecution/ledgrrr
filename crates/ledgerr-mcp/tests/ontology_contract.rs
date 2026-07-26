mod common;

use std::collections::BTreeMap;

use ledger_core::ingest::{deterministic_tx_id, TransactionInput};
use ledger_core::ontology::Relation;
use ledger_core::proposal::{
    ModelMetadata, OntologyEdgeProposal, ProposalPolicy, ProposalState, ProposalValidation,
};
use ledgerr_mcp::{
    mcp_adapter, IngestStatementRowsRequest, OntologyEdgeInput, OntologyEntityInput,
    OntologyEntityKind, OntologyQueryPathRequest, OntologyStore, OntologyUpsertEdgesRequest,
    OntologyUpsertEntitiesRequest, TurboLedgerService, TurboLedgerTools,
};
use rust_decimal::Decimal;
use serde_json::json;
use tempfile::tempdir;

fn service() -> TurboLedgerService {
    let manifest =
        common::manifest_for_workbook(&common::unique_workbook_path("ontology-contract"), 2023);

    TurboLedgerService::from_manifest_str(&manifest).expect("manifest should parse")
}

// ONTO-01 (D-02, D-03, D-04): valid ontology entities and edges persist with stable content-hash IDs.
#[test]
fn onto_01_persistence_integrity_persists_entities_and_edges_with_stable_ids() {
    let service = service();
    let tmp = tempdir().expect("tempdir");
    let ontology_path = tmp.path().join("ontology.json");

    let mut doc_attrs = BTreeMap::new();
    doc_attrs.insert(
        "source_ref".to_string(),
        "2023/WF--BH-CHK--2023-01--statement.pdf".to_string(),
    );

    let mut tx_attrs = BTreeMap::new();
    tx_attrs.insert("tx_id".to_string(), "tx-001".to_string());

    let entities = service
        .ontology_upsert_entities(OntologyUpsertEntitiesRequest {
            ontology_path: ontology_path.clone(),
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
        .expect("entities should upsert");

    assert_eq!(entities.inserted_count, 2);
    assert_eq!(entities.entity_ids.len(), 2);

    let edges = service
        .ontology_upsert_edges(OntologyUpsertEdgesRequest {
            ontology_path: ontology_path.clone(),
            edges: vec![OntologyEdgeInput {
                from: entities.entity_ids[0].clone(),
                to: entities.entity_ids[1].clone(),
                relation: "documents_transaction".to_string(),
                provenance: BTreeMap::new(),
            }],
        })
        .expect("edge should upsert");

    assert_eq!(edges.inserted_count, 1);
    assert_eq!(edges.edge_ids.len(), 1);

    let replay_entities = service
        .ontology_upsert_entities(OntologyUpsertEntitiesRequest {
            ontology_path: ontology_path.clone(),
            entities: vec![
                OntologyEntityInput {
                    kind: OntologyEntityKind::Document,
                    attrs: {
                        let mut attrs = BTreeMap::new();
                        attrs.insert(
                            "source_ref".to_string(),
                            "2023/WF--BH-CHK--2023-01--statement.pdf".to_string(),
                        );
                        attrs
                    },
                    custom_kind: None,
                },
                OntologyEntityInput {
                    kind: OntologyEntityKind::Transaction,
                    attrs: {
                        let mut attrs = BTreeMap::new();
                        attrs.insert("tx_id".to_string(), "tx-001".to_string());
                        attrs
                    },
                    custom_kind: None,
                },
            ],
            schema_store_path: None,
        })
        .expect("replay should succeed");

    assert_eq!(replay_entities.inserted_count, 0);
    assert_eq!(replay_entities.entity_ids, entities.entity_ids);
}

// ONTO-01 (D-03): edge upsert deterministically rejects missing from/to references.
#[test]
fn onto_01_missing_ref_rejected_deterministically() {
    let service = service();
    let tmp = tempdir().expect("tempdir");
    let ontology_path = tmp.path().join("ontology.json");

    let err = service
        .ontology_upsert_edges(OntologyUpsertEdgesRequest {
            ontology_path,
            edges: vec![OntologyEdgeInput {
                from: "missing-document".to_string(),
                to: "missing-transaction".to_string(),
                relation: "documents_transaction".to_string(),
                provenance: BTreeMap::new(),
            }],
        })
        .expect_err("invalid edge should fail");

    assert_eq!(
        err.to_string(),
        "invalid input: missing_ref: edge endpoints must reference existing entities"
    );
}

// ONTO-02 (D-03): deterministic traversal from document to transaction to evidence/tax nodes.
#[test]
fn onto_02_relationship_query_returns_ordered_document_chain() {
    let service = service();
    let tmp = tempdir().expect("tempdir");
    let ontology_path = tmp.path().join("ontology.json");

    let entities = service
        .ontology_upsert_entities(OntologyUpsertEntitiesRequest {
            ontology_path: ontology_path.clone(),
            entities: vec![
                OntologyEntityInput {
                    kind: OntologyEntityKind::Document,
                    attrs: {
                        let mut attrs = BTreeMap::new();
                        attrs.insert("source_ref".to_string(), "wf-statement.pdf".to_string());
                        attrs
                    },
                    custom_kind: None,
                },
                OntologyEntityInput {
                    kind: OntologyEntityKind::Transaction,
                    attrs: {
                        let mut attrs = BTreeMap::new();
                        attrs.insert("tx_id".to_string(), "tx-001".to_string());
                        attrs
                    },
                    custom_kind: None,
                },
                OntologyEntityInput {
                    kind: OntologyEntityKind::EvidenceReference,
                    attrs: {
                        let mut attrs = BTreeMap::new();
                        attrs.insert("rkyv_ref".to_string(), "wf-ctx.rkyv".to_string());
                        attrs
                    },
                    custom_kind: None,
                },
                OntologyEntityInput {
                    kind: OntologyEntityKind::TaxCategory,
                    attrs: {
                        let mut attrs = BTreeMap::new();
                        attrs.insert("category".to_string(), "OfficeSupplies".to_string());
                        attrs
                    },
                    custom_kind: None,
                },
            ],
            schema_store_path: None,
        })
        .expect("entities should upsert");

    let doc = entities.entity_ids[0].clone();
    let tx = entities.entity_ids[1].clone();
    let evidence = entities.entity_ids[2].clone();
    let tax = entities.entity_ids[3].clone();

    service
        .ontology_upsert_edges(OntologyUpsertEdgesRequest {
            ontology_path: ontology_path.clone(),
            edges: vec![
                OntologyEdgeInput {
                    from: doc.clone(),
                    to: tx.clone(),
                    relation: "documents_transaction".to_string(),
                    provenance: BTreeMap::new(),
                },
                OntologyEdgeInput {
                    from: tx.clone(),
                    to: evidence.clone(),
                    relation: "links_evidence".to_string(),
                    provenance: BTreeMap::new(),
                },
                OntologyEdgeInput {
                    from: tx.clone(),
                    to: tax.clone(),
                    relation: "links_tax_category".to_string(),
                    provenance: BTreeMap::new(),
                },
            ],
        })
        .expect("edges should upsert");

    let chain = service
        .ontology_query_path_tool(OntologyQueryPathRequest {
            ontology_path,
            from_entity_id: doc.clone(),
            max_depth: Some(4),
        })
        .expect("path query should succeed");

    assert_eq!(
        chain
            .nodes
            .iter()
            .map(|node| node.id.clone())
            .collect::<Vec<_>>(),
        vec![doc, tx, evidence, tax]
    );

    assert_eq!(
        chain
            .edges
            .iter()
            .map(|edge| (edge.from.clone(), edge.to.clone(), edge.relation.clone()))
            .collect::<Vec<_>>(),
        vec![
            (
                chain.nodes[0].id.clone(),
                chain.nodes[1].id.clone(),
                "documents_transaction".to_string(),
            ),
            (
                chain.nodes[1].id.clone(),
                chain.nodes[2].id.clone(),
                "links_evidence".to_string(),
            ),
            (
                chain.nodes[1].id.clone(),
                chain.nodes[3].id.clone(),
                "links_tax_category".to_string(),
            ),
        ]
    );
}

// ONTO-04 / PRD-4 Phase 1: MCP store converts to the canonical ledger-core snapshot
// without changing legacy IDs or persisted entity/edge shape.
#[test]
fn ontology_core_snapshot_conversion_preserves_legacy_ids() {
    let mut doc_attrs = BTreeMap::new();
    doc_attrs.insert("source_ref".to_string(), "wf-statement.pdf".to_string());
    let mut tx_attrs = BTreeMap::new();
    tx_attrs.insert("tx_id".to_string(), "tx-001".to_string());

    let mut store = OntologyStore::default();
    let entities = store
        .upsert_entities(vec![
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
        ], None)
        .expect("entities");

    let doc_id = entities.entity_ids[0].clone();
    let tx_id = entities.entity_ids[1].clone();
    store
        .upsert_edges(vec![OntologyEdgeInput {
            from: doc_id.clone(),
            to: tx_id.clone(),
            relation: "documents_transaction".to_string(),
            provenance: BTreeMap::new(),
        }])
        .expect("edge");

    let snapshot = store.to_core_snapshot();
    assert_eq!(snapshot.artifacts.len(), 2);
    assert_eq!(snapshot.relations.len(), 1);
    assert!(snapshot
        .artifacts
        .iter()
        .any(|artifact| artifact.id == doc_id));
    assert!(snapshot
        .artifacts
        .iter()
        .any(|artifact| artifact.id == tx_id));
    assert_eq!(snapshot.relations[0].relation, "documents_transaction");
}

// ONTO-04 / PRD-4 Phase 1: the advertised entity kinds map through the legacy MCP
// adapter into the canonical core kind aliases.
#[test]
fn ontology_legacy_payload_maps_to_core_types() {
    let service = service();
    let tmp = tempdir().expect("tempdir");
    let ontology_path = tmp.path().join("ontology.json");

    let response = mcp_adapter::handle_ontology_tool(
        &service,
        &json!({
            "action": "upsert_entities",
            "ontology_path": ontology_path.display().to_string(),
            "entities": [
                {"kind": "XeroContact", "properties": {"xero_id": "contact-1"}},
                {"kind": "XeroBankAccount", "properties": {"xero_id": "bank-1"}},
                {"kind": "XeroInvoice", "properties": {"xero_id": "invoice-1"}},
                {"kind": "WorkflowTag", "properties": {"tag": "#needs-review"}}
            ]
        }),
    );

    assert_eq!(response["isError"], false);
    let payload: serde_json::Value = serde_json::from_str(
        response["content"][0]["text"]
            .as_str()
            .expect("text payload"),
    )
    .expect("json payload");
    assert_eq!(payload["upserted"], 4);

    let store = OntologyStore::load(&ontology_path).expect("store");
    let mut actual: Vec<_> = store
        .entities
        .iter()
        .map(|entity| entity.kind)
        .collect();
    actual.sort();
    let mut expected = vec![
        OntologyEntityKind::XeroContact,
        OntologyEntityKind::XeroBankAccount,
        OntologyEntityKind::XeroInvoice,
        OntologyEntityKind::WorkflowTag,
    ];
    expected.sort();
    assert_eq!(actual, expected);
}

// ONTO-05 / PRD-4 Phase 2: row ingest emits deterministic source-to-transaction
// ontology facts without requiring a separate manual ontology upsert.
#[test]
fn ingest_rows_emits_document_to_transaction_ontology_edges() {
    let service = service();
    let tmp = tempdir().expect("tempdir");
    let journal_path = tmp.path().join("ledger.beancount");
    let workbook_path = tmp.path().join("tax-ledger.xlsx");
    let ontology_path = tmp.path().join("ontology.json");
    let row = TransactionInput {
        account_id: "WF-BH-CHK".to_string(),
        date: "2023-01-15".to_string(),
        amount: "-42.11".to_string(),
        description: "Coffee Shop".to_string(),
        source_ref: "source/wf-ctx.rkyv".to_string(),
    };
    let tx_id = deterministic_tx_id(&row);

    let first = service
        .ingest_statement_rows(IngestStatementRowsRequest {
            journal_path: journal_path.clone(),
            workbook_path: workbook_path.clone(),
            ontology_path: Some(ontology_path.clone()),
            rows: vec![row.clone()],
        })
        .expect("first ingest");
    let second = service
        .ingest_statement_rows(IngestStatementRowsRequest {
            journal_path,
            workbook_path: workbook_path.clone(),
            ontology_path: Some(ontology_path.clone()),
            rows: vec![row],
        })
        .expect("second ingest");

    assert_eq!(first.inserted_count, 1);
    assert_eq!(second.inserted_count, 0);

    let store = OntologyStore::load(&ontology_path).expect("ontology store");
    assert_eq!(store.entities.len(), 2);
    assert_eq!(store.edges.len(), 1);
    assert_eq!(store.edges[0].relation, "documents_transaction");
    assert_eq!(store.edges[0].provenance.get("tx_id"), Some(&tx_id));

    let snapshot = store.to_core_snapshot();
    assert_eq!(snapshot.artifacts.len(), 2);
    assert_eq!(snapshot.relations.len(), 1);
}

#[test]
fn semantic_context_refs_are_added_to_model_provenance() {
    let service = service();
    let tmp = tempdir().expect("tempdir");
    let ontology_path = tmp.path().join("ontology.json");

    let entities = service
        .ontology_upsert_entities(OntologyUpsertEntitiesRequest {
            ontology_path: ontology_path.clone(),
            entities: vec![
                OntologyEntityInput {
                    kind: OntologyEntityKind::Transaction,
                    attrs: {
                        let mut attrs = BTreeMap::new();
                        attrs.insert("tx_id".to_string(), "tx-semantic-001".to_string());
                        attrs
                    },
                    custom_kind: None,
                },
                OntologyEntityInput {
                    kind: OntologyEntityKind::TaxCategory,
                    attrs: {
                        let mut attrs = BTreeMap::new();
                        attrs.insert("category".to_string(), "SelfEmployment".to_string());
                        attrs
                    },
                    custom_kind: None,
                },
            ],
            schema_store_path: None,
        })
        .expect("entities should upsert");

    let proposal = OntologyEdgeProposal {
        proposal_id: "proposal-semantic-001".to_string(),
        proposed_relation: Relation {
            id: "draft-edge".to_string(),
            from: entities.entity_ids[0].clone(),
            to: entities.entity_ids[1].clone(),
            relation: "links_tax_category".to_string(),
            provenance: BTreeMap::new(),
        },
        confidence: Decimal::new(95, 2),
        source_artifact_ids: vec![entities.entity_ids[0].clone()],
        semantic_context_ids: vec!["semantic-rule-a".to_string(), "semantic-rule-b".to_string()],
        model_metadata: ModelMetadata {
            provider: "internal".to_string(),
            model: "phi-4-mini-reasoning".to_string(),
            endpoint_url: "http://127.0.0.1:15115/v1/chat/completions".to_string(),
        },
        validation: Some(ProposalValidation {
            validator: "ledger-core".to_string(),
            passed: true,
            checked_at: "2026-04-28T00:00:00Z".to_string(),
            notes: "semantic context is traceable".to_string(),
        }),
        approval: None,
        state: ProposalState::Validated,
    };
    let relation = proposal
        .commit_relation(&ProposalPolicy::default())
        .expect("validated high-confidence proposal commits");

    service
        .ontology_upsert_edges(OntologyUpsertEdgesRequest {
            ontology_path: ontology_path.clone(),
            edges: vec![OntologyEdgeInput {
                from: relation.from,
                to: relation.to,
                relation: relation.relation,
                provenance: relation.provenance,
            }],
        })
        .expect("edge should upsert");

    let store = OntologyStore::load(&ontology_path).expect("ontology store");
    let edge = store
        .edges
        .iter()
        .find(|edge| edge.relation == "links_tax_category")
        .expect("committed semantic edge");
    assert_eq!(
        edge.provenance
            .get("semantic_context_ids")
            .map(String::as_str),
        Some("semantic-rule-a,semantic-rule-b")
    );
    assert_eq!(
        edge.provenance.get("model_name").map(String::as_str),
        Some("phi-4-mini-reasoning")
    );
}
