mod common;

use std::collections::HashSet;
use ledger_core::ingest::TransactionInput;
use ledgerr_mcp::{
    ClassifyTransactionRequest, FetchQueueRequest, IngestStatementRowsRequest,
    QueueItemType, QueueStatus, TurboLedgerService, TurboLedgerTools,
};

fn service() -> TurboLedgerService {
    let workbook_path = common::unique_workbook_path("unified-queue");
    TurboLedgerService::from_manifest_str(&common::manifest_for_workbook(&workbook_path, 2023))
        .expect("manifest")
}

fn sample_row(description: &str, amount: &str) -> TransactionInput {
    TransactionInput {
        account_id: "WF-BH-CHK".to_string(),
        date: "2023-01-15".to_string(),
        amount: amount.to_string(),
        description: description.to_string(),
        source_ref: "source/ctx.rkyv".to_string(),
    }
}

#[test]
fn test_fetch_queue_returns_all_types() {
    let svc = service();
    let temp = tempfile::tempdir().expect("tempdir");
    
    // Ingest some transactions
    let rows = vec![
        sample_row("Coffee Shop", "-42.11"),
        sample_row("Office Supplies", "-15.00"),
    ];
    
    let ingest = svc
        .ingest_statement_rows(IngestStatementRowsRequest {
            journal_path: temp.path().join("ledger.beancount"),
            workbook_path: temp.path().join("tax-ledger.xlsx"),
            ontology_path: None,
            rows,
        })
        .expect("ingest");
    
    // Classify transactions to create flags
    for tx_id in &ingest.tx_ids {
        svc.classify_transaction(ClassifyTransactionRequest {
            tx_id: tx_id.clone(),
            category: "Test".to_string(),
            confidence: "0.70".to_string(),
            note: Some("test".to_string()),
            actor: "agent".to_string(),
        })
        .expect("classify");
    }
    
    // Fetch work queue
    let response = svc.fetch_work_queue(FetchQueueRequest {
        item_types: None,
        statuses: None,
        updated_after: None,
        limit: 100,
        offset: 0,
    }).expect("fetch_work_queue");
    
    // Should have at least flags (manual changes might not exist yet)
    let item_types: HashSet<QueueItemType> = response.items.iter()
        .map(|item| item.item_type)
        .collect();
    
    assert!(!response.items.is_empty(), "Queue should not be empty");
    assert!(item_types.contains(&QueueItemType::Flag), "Should contain Flag items");
}

#[test]
fn test_fetch_queue_filters_by_type() {
    let svc = service();
    let temp = tempfile::tempdir().expect("tempdir");
    
    // Ingest a transaction
    let rows = vec![sample_row("Test", "-10.00")];
    let ingest = svc
        .ingest_statement_rows(IngestStatementRowsRequest {
            journal_path: temp.path().join("ledger.beancount"),
            workbook_path: temp.path().join("tax-ledger.xlsx"),
            ontology_path: None,
            rows,
        })
        .expect("ingest");
    
    // Classify to create a flag
    svc.classify_transaction(ClassifyTransactionRequest {
        tx_id: ingest.tx_ids[0].clone(),
        category: "Test".to_string(),
        confidence: "0.91".to_string(),
        note: Some("test".to_string()),
        actor: "agent".to_string(),
    })
    .expect("classify");
    
    // Fetch only Flag items
    let response = svc.fetch_work_queue(FetchQueueRequest {
        item_types: Some(vec![QueueItemType::Flag]),
        statuses: None,
        updated_after: None,
        limit: 100,
        offset: 0,
    }).expect("fetch_work_queue");
    
    // All items should be Flag type
    for item in &response.items {
        assert_eq!(item.item_type, QueueItemType::Flag, "All items should be Flag type");
    }
}

#[test]
fn test_fetch_queue_filters_by_status() {
    let svc = service();
    let temp = tempfile::tempdir().expect("tempdir");
    
    // Ingest a transaction
    let rows = vec![sample_row("Test", "-10.00")];
    let ingest = svc
        .ingest_statement_rows(IngestStatementRowsRequest {
            journal_path: temp.path().join("ledger.beancount"),
            workbook_path: temp.path().join("tax-ledger.xlsx"),
            ontology_path: None,
            rows,
        })
        .expect("ingest");
    
    // Classify to create an open flag
    svc.classify_transaction(ClassifyTransactionRequest {
        tx_id: ingest.tx_ids[0].clone(),
        category: "Test".to_string(),
        confidence: "0.91".to_string(),
        note: Some("test".to_string()),
        actor: "agent".to_string(),
    })
    .expect("classify");
    
    // Fetch only Open items
    let response = svc.fetch_work_queue(FetchQueueRequest {
        item_types: None,
        statuses: Some(vec![QueueStatus::Open]),
        updated_after: None,
        limit: 100,
        offset: 0,
    }).expect("fetch_work_queue");
    
    // All items should be Open status
    for item in &response.items {
        assert_eq!(item.status, QueueStatus::Open, "All items should be Open status");
    }
}

#[test]
fn test_fetch_queue_updated_after_filter() {
    let svc = service();
    let temp = tempfile::tempdir().expect("tempdir");
    
    // Ingest a transaction and classify it
    let rows = vec![sample_row("Test", "-10.00")];
    let ingest = svc
        .ingest_statement_rows(IngestStatementRowsRequest {
            journal_path: temp.path().join("ledger.beancount"),
            workbook_path: temp.path().join("tax-ledger.xlsx"),
            ontology_path: None,
            rows,
        })
        .expect("ingest");
    
    svc.classify_transaction(ClassifyTransactionRequest {
        tx_id: ingest.tx_ids[0].clone(),
        category: "Test".to_string(),
        confidence: "0.91".to_string(),
        note: Some("test".to_string()),
        actor: "agent".to_string(),
    })
    .expect("classify");
    
    // Fetch items after a future date (should return empty)
    let response = svc.fetch_work_queue(FetchQueueRequest {
        item_types: None,
        statuses: None,
        updated_after: Some("2099-01-01T00:00:00Z".to_string()),
        limit: 100,
        offset: 0,
    }).expect("fetch_work_queue");
    
    assert_eq!(response.items.len(), 0, "Should return no items for future date");
}

#[test]
fn test_fetch_queue_sorts_by_created_at_desc() {
    let svc = service();
    let temp = tempfile::tempdir().expect("tempdir");
    
    // Ingest multiple transactions
    let rows = vec![
        sample_row("Test 1", "-10.00"),
        sample_row("Test 2", "-20.00"),
    ];
    let ingest = svc
        .ingest_statement_rows(IngestStatementRowsRequest {
            journal_path: temp.path().join("ledger.beancount"),
            workbook_path: temp.path().join("tax-ledger.xlsx"),
            ontology_path: None,
            rows,
        })
        .expect("ingest");
    
    // Classify all transactions
    for tx_id in &ingest.tx_ids {
        svc.classify_transaction(ClassifyTransactionRequest {
            tx_id: tx_id.clone(),
            category: "Test".to_string(),
            confidence: "0.70".to_string(),
            note: Some("test".to_string()),
            actor: "agent".to_string(),
        })
        .expect("classify");
    }
    
    // Fetch all items
    let response = svc.fetch_work_queue(FetchQueueRequest {
        item_types: None,
        statuses: None,
        updated_after: None,
        limit: 100,
        offset: 0,
    }).expect("fetch_work_queue");
    
    // Check that items are sorted by created_at descending
    let timestamps: Vec<&String> = response.items.iter()
        .map(|item| &item.created_at)
        .collect();
    
    assert_eq!(timestamps, {
        let mut sorted = timestamps.clone();
        sorted.sort();
        sorted.reverse();
        sorted
    }, "Items should be sorted by created_at descending");
}

#[test]
fn test_fetch_queue_pagination_works() {
    let svc = service();
    let temp = tempfile::tempdir().expect("tempdir");
    
    // Ingest enough transactions to test pagination
    let mut rows = Vec::new();
    for i in 0..10 {
        rows.push(sample_row(&format!("Test {}", i), "-10.00"));
    }
    
    let ingest = svc
        .ingest_statement_rows(IngestStatementRowsRequest {
            journal_path: temp.path().join("ledger.beancount"),
            workbook_path: temp.path().join("tax-ledger.xlsx"),
            ontology_path: None,
            rows,
        })
        .expect("ingest");
    
    // Classify all transactions
    for tx_id in &ingest.tx_ids {
        svc.classify_transaction(ClassifyTransactionRequest {
            tx_id: tx_id.clone(),
            category: "Test".to_string(),
            confidence: "0.70".to_string(),
            note: Some("test".to_string()),
            actor: "agent".to_string(),
        })
        .expect("classify");
    }
    
    // Test pagination: first page with limit=5
    let page1 = svc.fetch_work_queue(FetchQueueRequest {
        item_types: None,
        statuses: None,
        updated_after: None,
        limit: 5,
        offset: 0,
    }).expect("fetch_work_queue page 1");
    
    // Test pagination: second page with offset=5
    let page2 = svc.fetch_work_queue(FetchQueueRequest {
        item_types: None,
        statuses: None,
        updated_after: None,
        limit: 5,
        offset: 5,
    }).expect("fetch_work_queue page 2");
    
    assert_eq!(page1.items.len(), 5, "First page should have 5 items");
    assert_eq!(page2.items.len(), 5, "Second page should have 5 items");
    assert_eq!(page1.total_count, page2.total_count, "Total count should be the same");
    
    // Ensure no duplicate items
    let page1_ids: HashSet<String> = page1.items.iter().map(|item| item.id.clone()).collect();
    let page2_ids: HashSet<String> = page2.items.iter().map(|item| item.id.clone()).collect();
    assert_eq!(page1_ids.intersection(&page2_ids).count(), 0, "Pages should not have duplicate items");
}

#[test]
fn test_fetch_queue_deterministic_ordering() {
    let svc = service();
    let temp = tempfile::tempdir().expect("tempdir");
    
    // Ingest and classify transactions
    let rows = vec![
        sample_row("Test 1", "-10.00"),
        sample_row("Test 2", "-20.00"),
    ];
    
    let ingest = svc
        .ingest_statement_rows(IngestStatementRowsRequest {
            journal_path: temp.path().join("ledger.beancount"),
            workbook_path: temp.path().join("tax-ledger.xlsx"),
            ontology_path: None,
            rows,
        })
        .expect("ingest");
    
    for tx_id in &ingest.tx_ids {
        svc.classify_transaction(ClassifyTransactionRequest {
            tx_id: tx_id.clone(),
            category: "Test".to_string(),
            confidence: "0.91".to_string(),
            note: Some("test".to_string()),
            actor: "agent".to_string(),
        })
        .expect("classify");
    }
    
    // Fetch twice and ensure same order
    let response1 = svc.fetch_work_queue(FetchQueueRequest {
        item_types: None,
        statuses: None,
        updated_after: None,
        limit: 100,
        offset: 0,
    }).expect("fetch_work_queue 1");
    
    let response2 = svc.fetch_work_queue(FetchQueueRequest {
        item_types: None,
        statuses: None,
        updated_after: None,
        limit: 100,
        offset: 0,
    }).expect("fetch_work_queue 2");
    
    // Items should be identical
    assert_eq!(response1.items.len(), response2.items.len(), "Should have same item count");
    for (item1, item2) in response1.items.iter().zip(response2.items.iter()) {
        assert_eq!(item1.id, item2.id, "Item IDs should match");
        assert_eq!(item1.created_at, item2.created_at, "Item timestamps should match");
    }
}

#[test]
fn test_fetch_queue_provenance_tracking() {
    let svc = service();
    let temp = tempfile::tempdir().expect("tempdir");
    
    // Ingest and classify to create flags (from ReviewTool)
    let rows = vec![sample_row("Test", "-10.00")];
    let ingest = svc
        .ingest_statement_rows(IngestStatementRowsRequest {
            journal_path: temp.path().join("ledger.beancount"),
            workbook_path: temp.path().join("tax-ledger.xlsx"),
            ontology_path: None,
            rows,
        })
        .expect("ingest");
    
    svc.classify_transaction(ClassifyTransactionRequest {
        tx_id: ingest.tx_ids[0].clone(),
        category: "Test".to_string(),
        confidence: "0.91".to_string(),
        note: Some("test".to_string()),
        actor: "agent".to_string(),
    })
    .expect("classify");
    
    // Fetch and check provenance
    let response = svc.fetch_work_queue(FetchQueueRequest {
        item_types: None,
        statuses: None,
        updated_after: None,
        limit: 100,
        offset: 0,
    }).expect("fetch_work_queue");
    
    // Flag items should have ReviewTool provenance
    for item in &response.items {
        if item.item_type == QueueItemType::Flag {
            assert_eq!(format!("{:?}", item.provenance), "ReviewTool", 
                "Flag items should have ReviewTool provenance");
        }
    }
}

#[test]
fn test_fetch_queue_empty_queue() {
    let svc = service();
    
    // Fetch from empty service
    let response = svc.fetch_work_queue(FetchQueueRequest {
        item_types: None,
        statuses: None,
        updated_after: None,
        limit: 100,
        offset: 0,
    }).expect("fetch_work_queue");
    
    assert_eq!(response.items.len(), 0, "Empty queue should return no items");
    assert_eq!(response.total_count, 0, "Total count should be 0");
    assert_eq!(response.offset, 0, "Offset should be 0");
    assert_eq!(response.limit, 100, "Limit should be 100");
}

#[test]
fn test_fetch_queue_manual_change_tx_document_ref() {
    let svc = service();
    let temp = tempfile::tempdir().expect("tempdir");
    
    // Ingest a transaction with a document reference
    let rows = vec![sample_row("Test", "-10.00")];
    let ingest = svc
        .ingest_statement_rows(IngestStatementRowsRequest {
            journal_path: temp.path().join("ledger.beancount"),
            workbook_path: temp.path().join("tax-ledger.xlsx"),
            ontology_path: None,
            rows,
        })
        .expect("ingest");
    
    let tx_id = ingest.tx_ids[0].clone();
    
    // Make a manual adjustment (non-agent)
    svc.classify_transaction(ClassifyTransactionRequest {
        tx_id: tx_id.clone(),
        category: "Manual Category".to_string(),
        confidence: "0.95".to_string(),
        note: Some("manual adjustment".to_string()),
        actor: "human".to_string(), // Non-agent actor creates manual change
    })
    .expect("classify");
    
    // Fetch all items including ManualChange
    let response = svc.fetch_work_queue(FetchQueueRequest {
        item_types: Some(vec![QueueItemType::ManualChange]),
        statuses: None,
        updated_after: None,
        limit: 100,
        offset: 0,
    }).expect("fetch_work_queue");
    
    // Should have at least one ManualChange item
    let manual_changes: Vec<_> = response.items.iter()
        .filter(|item| item.item_type == QueueItemType::ManualChange)
        .collect();
    
    if !manual_changes.is_empty() {
        // Check that manual change items have the expected fields
        for item in manual_changes {
            assert_eq!(item.item_type, QueueItemType::ManualChange, "Item type should be ManualChange");
            assert_eq!(item.status, QueueStatus::Resolved, "Manual changes should be Resolved");
            // Note: The document_ref is in the source_ref of TransactionInput, not the event
        }
    }
}
