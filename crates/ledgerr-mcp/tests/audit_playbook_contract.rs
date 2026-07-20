mod common;

use calamine::Reader;
use ledger_core::ingest::{deterministic_tx_id, TransactionInput};
use ledgerr_mcp::{
    ontology, ClassifyTransactionRequest, EventHistoryFilter, ExportCpaWorkbookRequest,
    IngestStatementRowsRequest, TurboLedgerService, TurboLedgerTools,
};

fn service(workbook_path: &std::path::Path) -> TurboLedgerService {
    TurboLedgerService::from_manifest_str(&format!(
        "{}\n[accounts.WF-BH-CHK]\ninstitution=\"Wise\"\ntype=\"checking\"\ncurrency=\"USD\"\n",
        common::manifest_for_workbook(workbook_path, 2023)
    ))
    .expect("manifest")
}

fn cell_text<T>(range: &calamine::Range<T>, row: usize, col: usize) -> Option<String>
where
    T: calamine::CellType + ToString,
{
    range.get((row, col)).map(ToString::to_string)
}

#[test]
fn audit_playbook_ids_match_across_workbook_ontology_and_events() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workbook_path = temp.path().join("tax-ledger.xlsx");
    let journal_path = temp.path().join("ledger.beancount");
    let ontology_path = temp.path().join("ontology.json");
    let cpa_path = temp.path().join("cpa.xlsx");
    let svc = service(&workbook_path);
    let row = TransactionInput {
        account_id: "WF-BH-CHK".to_string(),
        date: "2023-01-15".to_string(),
        amount: "-42.11".to_string(),
        description: "Coffee Shop".to_string(),
        source_ref: temp.path().join("wf-2023-01.rkyv").display().to_string(),
    };
    let expected_tx_id = deterministic_tx_id(&row);

    let ingest = svc
        .ingest_statement_rows(IngestStatementRowsRequest {
            journal_path,
            workbook_path,
            ontology_path: Some(ontology_path.clone()),
            rows: vec![row],
        })
        .expect("ingest rows");
    assert_eq!(ingest.tx_ids, vec![expected_tx_id.clone()]);

    svc.classify_transaction(ClassifyTransactionRequest {
        tx_id: expected_tx_id.clone(),
        category: "Meals".to_string(),
        confidence: "0.91".to_string(),
        note: Some("audit playbook deterministic fallback".to_string()),
        actor: "agent".to_string(),
    })
    .expect("classify");

    svc.export_cpa_workbook(ExportCpaWorkbookRequest {
        workbook_path: cpa_path.clone(),
    })
    .expect("export workbook");

    let mut workbook = calamine::open_workbook_auto(cpa_path).expect("workbook opens");
    let tx_sheet = workbook.worksheet_range("TX.WF-BH-CHK").expect("TX sheet");
    assert_eq!(cell_text(&tx_sheet, 1, 0), Some(expected_tx_id.clone()));
    assert_eq!(cell_text(&tx_sheet, 1, 4), Some("Meals".to_string()));

    let audit = workbook.worksheet_range("AUDIT.log").expect("AUDIT.log");
    assert!(
        (1..audit.height()).any(|row| cell_text(&audit, row, 2) == Some(expected_tx_id.clone())),
        "AUDIT.log should include tx_id {expected_tx_id}"
    );

    let store = ontology::load_store(&ontology_path).expect("ontology store");
    assert!(store.artifacts.iter().any(|entity| {
        entity
            .attrs
            .get("tx_id")
            .map(|tx_id| tx_id == &expected_tx_id)
            .unwrap_or(false)
    }));
    assert!(store.relations.iter().any(|edge| {
        edge.relation == "documents_transaction"
            && edge
                .provenance
                .get("tx_id")
                .map(|tx_id| tx_id == &expected_tx_id)
                .unwrap_or(false)
    }));

    let history = svc
        .event_history(EventHistoryFilter {
            tx_id: Some(expected_tx_id.clone()),
            document_ref: None,
            time_start: None,
            time_end: None,
        })
        .expect("event history");
    assert!(history.events.iter().any(
        |event| event.event_type == "ingest" && event.tx_id.as_deref() == Some(&expected_tx_id)
    ));
}
