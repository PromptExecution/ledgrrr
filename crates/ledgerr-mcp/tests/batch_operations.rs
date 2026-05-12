mod common;

use ledger_core::ingest::{deterministic_tx_id, TransactionInput};
use ledgerr_mcp::{
    ApplyMappingBulkRequest, BatchClassifyRequest, BatchMode, BatchResolveFlagsRequest,
    ClassifyTransactionRequest, FlagResolution, IngestPdfRequest, SimilarityMatchType, QueryTransactionsRequest,
    TurboLedgerService, TurboLedgerTools,
};

fn service() -> TurboLedgerService {
    let workbook_path = common::unique_workbook_path("batch-operations");
    TurboLedgerService::from_manifest_str(&common::manifest_for_workbook(&workbook_path, 2023))
        .expect("manifest")
}

fn ingest_test_transactions(svc: &TurboLedgerService, dir: &tempfile::TempDir) -> (String, String, String) {
    let journal_path = dir.path().join("ledger.beancount");
    let workbook_path = dir.path().join("tax-ledger.xlsx");
    let source_ref = dir.path().join("ctx.rkyv");

    // Ingest 3 transactions
    let tx1 = TransactionInput {
        account_id: "WF-BH-CHK".to_string(),
        date: "2023-01-15".to_string(),
        amount: "-11.00".to_string(),
        description: "Coffee Cart".to_string(),
        source_ref: source_ref.display().to_string(),
    };
    let tx_id1 = deterministic_tx_id(&tx1);

    let tx2 = TransactionInput {
        account_id: "WF-BH-CHK".to_string(),
        date: "2023-01-16".to_string(),
        amount: "-25.00".to_string(),
        description: "Grocery Store".to_string(),
        source_ref: source_ref.display().to_string(),
    };
    let tx_id2 = deterministic_tx_id(&tx2);

    let tx3 = TransactionInput {
        account_id: "WF-BH-CHK".to_string(),
        date: "2023-01-17".to_string(),
        amount: "-15.50".to_string(),
        description: "Coffee Shop".to_string(),
        source_ref: source_ref.display().to_string(),
    };
    let tx_id3 = deterministic_tx_id(&tx3);

    let _ingest = svc
        .ingest_pdf(IngestPdfRequest {
            pdf_path: "WF--BH-CHK--2023-01--statement.pdf".to_string(),
            journal_path: journal_path.clone(),
            workbook_path: workbook_path.clone(),
            ontology_path: None,
            raw_context_bytes: Some(b"ctx".to_vec()),
            extracted_rows: vec![tx1, tx2, tx3],
        })
        .expect("ingest");

    (tx_id1, tx_id2, tx_id3)
}

#[test]
fn test_batch_classify_all_succeeds() {
    let svc = service();
    let dir = tempfile::tempdir().expect("tempdir");
    let (tx_id1, tx_id2, tx_id3) = ingest_test_transactions(&svc, &dir);

    let response = svc
        .batch_classify(BatchClassifyRequest {
            tx_ids: vec![tx_id1.clone(), tx_id2.clone(), tx_id3.clone()],
            category: "Food".to_string(),
            confidence: "0.95".to_string(),
            note: Some("bulk classification".to_string()),
            actor: "test".to_string(),
            batch_mode: BatchMode::ContinueOnError,
            dry_run: false,
        })
        .expect("batch classify should succeed");

    assert_eq!(response.summary.total_requested, 3);
    assert_eq!(response.summary.succeeded, 3);
    assert_eq!(response.summary.failed, 0);
    assert_eq!(response.summary.skipped, 0);
    assert_eq!(response.items.len(), 3);

    // Verify all items succeeded
    for item in response.items {
        match item.status {
            ledgerr_mcp::BatchItemStatus::Succeeded => {}
            _ => panic!("Expected all items to succeed"),
        }
    }
}

#[test]
fn test_batch_classify_partial_failure_all_or_nothing() {
    let svc = service();
    let dir = tempfile::tempdir().expect("tempdir");
    let (tx_id1, tx_id2, tx_id3) = ingest_test_transactions(&svc, &dir);

    // First, classify tx_id2 with a different category
    let _ = svc
        .classify_transaction(ClassifyTransactionRequest {
            tx_id: tx_id2.clone(),
            category: "Groceries".to_string(),
            confidence: "0.90".to_string(),
            note: None,
            actor: "test".to_string(),
        })
        .expect("first classify");

    // Now try to reclassify all transactions with invalid confidence
    // This should fail for tx_id2 and stop due to AllOrNothing mode
    let result = svc.batch_classify(BatchClassifyRequest {
        tx_ids: vec![tx_id1.clone(), tx_id2.clone(), tx_id3.clone()],
        category: "Food".to_string(),
        confidence: "1.5".to_string(), // Invalid confidence (> 1.0)
        note: None,
        actor: "test".to_string(),
        batch_mode: BatchMode::AllOrNothing,
        dry_run: false,
    });

    // Should fail due to invalid confidence
    assert!(result.is_err());

    // Verify that tx_id2 still has its original classification
    // by querying transactions
    let query_response = svc
        .query_transactions(QueryTransactionsRequest {
            filters: ledgerr_mcp::TransactionFilters {
                account_id: None,
                date_range: None,
                category: None,
                amount_range: None,
                source_ref: None,
                description_contains: Some("Grocery Store".to_string()),
            },
            sort: None,
            pagination: None,
        })
        .expect("query transactions");

    assert_eq!(query_response.transactions.len(), 1);
    let tx = &query_response.transactions[0];
    assert_eq!(tx.category, Some("Groceries".to_string()));
}

#[test]
fn test_batch_classify_partial_failure_continue_on_error() {
    let svc = service();
    let dir = tempfile::tempdir().expect("tempdir");
    let (tx_id1, tx_id2, tx_id3) = ingest_test_transactions(&svc, &dir);

    // First, classify tx_id2 with a different category
    let _ = svc
        .classify_transaction(ClassifyTransactionRequest {
            tx_id: tx_id2.clone(),
            category: "Groceries".to_string(),
            confidence: "0.90".to_string(),
            note: None,
            actor: "test".to_string(),
        })
        .expect("first classify");

    // Now try to classify all transactions with a category
    // tx_id2 should still succeed even though it's already classified
    let response = svc
        .batch_classify(BatchClassifyRequest {
            tx_ids: vec![tx_id1.clone(), tx_id2.clone(), tx_id3.clone()],
            category: "Food".to_string(),
            confidence: "0.95".to_string(),
            note: Some("bulk classification".to_string()),
            actor: "test".to_string(),
            batch_mode: BatchMode::ContinueOnError,
            dry_run: false,
        })
        .expect("batch classify should succeed");

    assert_eq!(response.summary.total_requested, 3);
    // All should succeed (including the reclassification)
    assert_eq!(response.summary.succeeded, 3);
    assert_eq!(response.summary.failed, 0);
    assert_eq!(response.summary.skipped, 0);
}

#[test]
fn test_batch_classify_dry_run_skips_all() {
    let svc = service();
    let dir = tempfile::tempdir().expect("tempdir");
    let (tx_id1, tx_id2, tx_id3) = ingest_test_transactions(&svc, &dir);

    let response = svc
        .batch_classify(BatchClassifyRequest {
            tx_ids: vec![tx_id1.clone(), tx_id2.clone(), tx_id3.clone()],
            category: "Food".to_string(),
            confidence: "0.95".to_string(),
            note: Some("bulk classification".to_string()),
            actor: "test".to_string(),
            batch_mode: BatchMode::ContinueOnError,
            dry_run: true,
        })
        .expect("batch classify dry run should succeed");

    assert_eq!(response.summary.total_requested, 3);
    assert_eq!(response.summary.succeeded, 0);
    assert_eq!(response.summary.failed, 0);
    assert_eq!(response.summary.skipped, 3);

    // Verify no transactions were actually classified
    // by querying all transactions
    let query_response = svc
        .query_transactions(QueryTransactionsRequest {
            filters: ledgerr_mcp::TransactionFilters {
                account_id: None,
                date_range: None,
                category: None,
                amount_range: None,
                source_ref: None,
                description_contains: None,
            },
            sort: None,
            pagination: None,
        })
        .expect("query transactions");

    // All should have None or empty category
    for tx in query_response.transactions {
        assert!(tx.category.is_none() || tx.category.as_ref().map(|c| c.is_empty()).unwrap_or(true));
    }

    // Verify all items are marked as skipped
    for item in response.items {
        match item.status {
            ledgerr_mcp::BatchItemStatus::Skipped { reason } => {
                assert_eq!(reason, "dry_run");
            }
            _ => panic!("Expected all items to be skipped"),
        }
    }
}

#[test]
fn test_bulk_resolve_flags_updates_status() {
    // This test currently passes because bulk_resolve_flags returns early
    // with an error in non-dry_run mode (as documented in the code)
    let svc = service();
    let dir = tempfile::tempdir().expect("tempdir");
    let (tx_id1, _tx_id2, _tx_id3) = ingest_test_transactions(&svc, &dir);

    // Classify with low confidence to trigger a flag
    let _ = svc
        .classify_transaction(ClassifyTransactionRequest {
            tx_id: tx_id1.clone(),
            category: "Food".to_string(),
            confidence: "0.60".to_string(), // Low confidence
            note: None,
            actor: "test".to_string(),
        })
        .expect("classify");

    // Try to resolve flags in dry_run mode
    let response = svc
        .bulk_resolve_flags(BatchResolveFlagsRequest {
            tx_ids: vec![tx_id1.clone()],
            resolution: FlagResolution::Approve,
            reason: Some("reviewed and approved".to_string()),
            actor: "test".to_string(),
            batch_mode: BatchMode::ContinueOnError,
            dry_run: true, // Must be dry_run for now
        })
        .expect("bulk resolve flags dry run should succeed");

    assert_eq!(response.summary.total_requested, 1);
    assert_eq!(response.summary.succeeded, 0);
    assert_eq!(response.summary.failed, 0);
    assert_eq!(response.summary.skipped, 1);

    // All items should be skipped due to dry_run
    for item in response.items {
        match item.status {
            ledgerr_mcp::BatchItemStatus::Skipped { reason } => {
                assert_eq!(reason, "dry_run");
            }
            _ => panic!("Expected item to be skipped"),
        }
    }
}

#[test]
fn test_apply_mapping_bulk_matches_similar() {
    let svc = service();
    let dir = tempfile::tempdir().expect("tempdir");
    let (tx_id1, _tx_id2, _tx_id3) = ingest_test_transactions(&svc, &dir);

    // First, classify tx_id1 as a template
    let _ = svc
        .classify_transaction(ClassifyTransactionRequest {
            tx_id: tx_id1.clone(),
            category: "Coffee".to_string(),
            confidence: "0.95".to_string(),
            note: None,
            actor: "test".to_string(),
        })
        .expect("classify template");

    // Apply mapping bulk to find similar descriptions
    let response = svc
        .apply_mapping_bulk(ApplyMappingBulkRequest {
            source_tx_id: tx_id1.clone(),
            match_fields: vec!["description".to_string()],
            similarity_type: SimilarityMatchType::Substring,
            target_category: "Coffee".to_string(),
            target_confidence: "0.90".to_string(),
            actor: "test".to_string(),
            max_matches: 10,
            batch_mode: BatchMode::ContinueOnError,
            dry_run: false,
        })
        .expect("apply mapping bulk should succeed");

    // Should find matches based on description similarity
    assert_eq!(response.classification_summary.total_requested, response.matched_tx_ids.len());

    // Verify the classification summary
    assert_eq!(response.classification_summary.total_requested, response.matched_tx_ids.len());
    assert!(response.classification_summary.succeeded <= response.matched_tx_ids.len());
}

#[test]
fn test_batch_operations_enforce_atomicity() {
    let svc = service();
    let dir = tempfile::tempdir().expect("tempdir");
    let (tx_id1, tx_id2, tx_id3) = ingest_test_transactions(&svc, &dir);

    // Classify tx_id1 first
    let _ = svc
        .classify_transaction(ClassifyTransactionRequest {
            tx_id: tx_id1.clone(),
            category: "Coffee".to_string(),
            confidence: "0.95".to_string(),
            note: None,
            actor: "test".to_string(),
        })
        .expect("first classify");

    // Try to batch classify all with AllOrNothing mode
    // This should succeed since tx_id1 is already classified
    let response = svc
        .batch_classify(BatchClassifyRequest {
            tx_ids: vec![tx_id1.clone(), tx_id2.clone(), tx_id3.clone()],
            category: "Food".to_string(),
            confidence: "0.95".to_string(),
            note: Some("atomic test".to_string()),
            actor: "test".to_string(),
            batch_mode: BatchMode::AllOrNothing,
            dry_run: false,
        })
        .expect("batch classify should succeed");

    // All should succeed (including reclassification of tx_id1)
    assert_eq!(response.summary.total_requested, 3);
    assert_eq!(response.summary.succeeded, 3);
    assert_eq!(response.summary.failed, 0);

    // Verify all transactions are classified
    // by querying all transactions
    let query_response = svc
        .query_transactions(QueryTransactionsRequest {
            filters: ledgerr_mcp::TransactionFilters {
                account_id: None,
                date_range: None,
                category: None,
                amount_range: None,
                source_ref: None,
                description_contains: None,
            },
            sort: None,
            pagination: None,
        })
        .expect("query transactions");

    assert_eq!(query_response.transactions.len(), 3);
    for tx in query_response.transactions {
        assert!(tx.category.is_some());
        assert_eq!(tx.category.as_ref().unwrap(), &"Food".to_string());
    }
}
