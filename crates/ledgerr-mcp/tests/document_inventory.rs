use ledger_core::ingest::TransactionInput;
use ledgerr_mcp::{
    DocumentInventoryRequest, DocumentQueueStatusRequest, IngestStatementRowsRequest,
    TurboLedgerService, TurboLedgerTools,
};

fn service_for(workbook_path: &std::path::Path) -> TurboLedgerService {
    // Escape backslashes/quotes for TOML — Windows temp paths are full of
    // `\` separators, which are not valid TOML string escapes on their own
    // (e.g. `\U`, `\A`, ...) and would otherwise fail to parse.
    let escaped = workbook_path
        .display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let manifest = format!("[session]\nworkbook_path=\"{escaped}\"\nactive_year=2023\n");
    TurboLedgerService::from_manifest_str(&manifest).expect("manifest")
}

#[test]
fn document_inventory_lists_ready_ingested_and_invalid_documents_deterministically() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let workbook_path = tmp.path().join("tax-ledger.xlsx");
    let service = service_for(&workbook_path);
    let docs_dir = tmp.path().join("docs");
    std::fs::create_dir_all(&docs_dir).expect("docs dir");

    let ingested_pdf = docs_dir.join("WF--BH-CHK--2023-01--statement.pdf");
    let ready_pdf = docs_dir.join("WF--BH-CHK--2023-02--statement.pdf");
    let invalid_pdf = docs_dir.join("bad-name.pdf");
    let ingested_rkyv = ingested_pdf.with_extension("rkyv");

    std::fs::write(&ingested_pdf, b"pdf").expect("ingested pdf");
    std::fs::write(&ready_pdf, b"pdf").expect("ready pdf");
    std::fs::write(&invalid_pdf, b"pdf").expect("invalid pdf");
    std::fs::write(&ingested_rkyv, b"ctx").expect("ingested rkyv");

    service
        .ingest_statement_rows(IngestStatementRowsRequest {
            journal_path: tmp.path().join("ledger.beancount"),
            workbook_path: workbook_path.clone(),
            ontology_path: None,
            rows: vec![TransactionInput {
                account_id: "WF-BH-CHK".to_string(),
                date: "2023-01-15".to_string(),
                amount: "-42.11".to_string(),
                description: "Coffee Shop".to_string(),
                source_ref: ingested_rkyv.display().to_string(),
            }],
        })
        .expect("ingest rows");

    let response = service
        .document_inventory(DocumentInventoryRequest {
            directory: docs_dir.clone(),
            recursive: false,
            statuses: Vec::new(),
        })
        .expect("document inventory");

    assert_eq!(response.documents.len(), 3);
    assert_eq!(
        response.documents[0].file_name,
        "WF--BH-CHK--2023-01--statement.pdf"
    );
    assert_eq!(
        response.documents[0].status,
        DocumentQueueStatusRequest::Ingested
    );
    assert_eq!(response.documents[0].next_hint, "review_existing_rows");
    assert_eq!(response.documents[0].ingested_tx_ids.len(), 1);

    assert_eq!(
        response.documents[1].file_name,
        "WF--BH-CHK--2023-02--statement.pdf"
    );
    assert_eq!(
        response.documents[1].status,
        DocumentQueueStatusRequest::Ready
    );
    assert_eq!(response.documents[1].next_hint, "call_proxy_ingest_pdf");
    assert_eq!(response.documents[1].account_id.as_deref(), Some("BH-CHK"));
    assert_eq!(response.documents[1].year_month.as_deref(), Some("2023-02"));

    assert_eq!(response.documents[2].file_name, "bad-name.pdf");
    assert_eq!(
        response.documents[2].status,
        DocumentQueueStatusRequest::InvalidName
    );
    assert_eq!(
        response.documents[2].blocked_reason.as_deref(),
        Some("invalid_contract_name")
    );
    assert_eq!(response.documents[2].next_hint, "rename_then_retry");
    assert!(response.documents[2].account_id.is_none());
}

#[test]
fn document_inventory_filters_by_status() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let workbook_path = tmp.path().join("tax-ledger.xlsx");
    let service = service_for(&workbook_path);
    let docs_dir = tmp.path().join("docs");
    std::fs::create_dir_all(&docs_dir).expect("docs dir");

    std::fs::write(docs_dir.join("WF--BH-CHK--2023-03--statement.pdf"), b"pdf").expect("ready pdf");
    std::fs::write(docs_dir.join("bad-name.pdf"), b"pdf").expect("invalid pdf");

    let response = service
        .document_inventory(DocumentInventoryRequest {
            directory: docs_dir,
            recursive: false,
            statuses: vec![DocumentQueueStatusRequest::InvalidName],
        })
        .expect("document inventory");

    assert_eq!(response.documents.len(), 1);
    assert_eq!(response.documents[0].file_name, "bad-name.pdf");
    assert_eq!(
        response.documents[0].status,
        DocumentQueueStatusRequest::InvalidName
    );
}
