// Tests for query_transactions feature

mod common;

use ledgerr_mcp::{
    contract::{DateRange, SortDirection, SortField, SortSpec, PaginationSpec, TransactionFilters},
    TurboLedgerService, TurboLedgerTools, QueryTransactionsRequest, IngestStatementRowsRequest,
};
use ledger_core::ingest::TransactionInput;

fn create_test_service() -> (TurboLedgerService, std::path::PathBuf) {
    let workbook_path = common::unique_workbook_path("query_transactions_test");
    let manifest = common::manifest_for_workbook(&workbook_path, 2023);
    (TurboLedgerService::from_manifest_str(&manifest).unwrap(), workbook_path)
}

#[test]
fn test_query_transactions_returns_filtered_results() {
    // Create a service with sample data
    let (service, workbook_path) = create_test_service();
    
    // Ingest some test transactions
    let tx1 = TransactionInput {
        account_id: "ACCT1".to_string(),
        date: "2023-01-15".to_string(),
        amount: "100.00".to_string(),
        description: "Coffee Shop".to_string(),
        source_ref: "stmt1.pdf".to_string(),
    };
    
    let tx2 = TransactionInput {
        account_id: "ACCT2".to_string(),
        date: "2023-01-20".to_string(),
        amount: "200.00".to_string(),
        description: "Grocery Store".to_string(),
        source_ref: "stmt1.pdf".to_string(),
    };
    
    let tx3 = TransactionInput {
        account_id: "ACCT1".to_string(),
        date: "2023-02-10".to_string(),
        amount: "150.00".to_string(),
        description: "Gas Station".to_string(),
        source_ref: "stmt2.pdf".to_string(),
    };
    
    // Ingest transactions
    let _ = service.ingest_statement_rows(IngestStatementRowsRequest {
        journal_path: PathBuf::from("test.journal"),
        workbook_path: service.workbook_path().to_path_buf(),
        ontology_path: None,
        rows: vec![tx1.clone(), tx2.clone(), tx3.clone()],
    });
    
    // Test filter by account_id
    let filters = TransactionFilters {
        account_id: Some("ACCT1".to_string()),
        date_range: None,
        category: None,
        amount_range: None,
        source_ref: None,
        description_contains: None,
    };
    
    let response = service.query_transactions(QueryTransactionsRequest {
        filters: filters.clone(),
        sort: None,
        pagination: None,
    }).unwrap();
    
    assert_eq!(response.transactions.len(), 2);
    assert!(response.transactions.iter().all(|tx| tx.account_id == "ACCT1"));
    
    // Test filter by date range
    let filters = TransactionFilters {
        account_id: None,
        date_range: Some(DateRange {
            start: "2023-01-01".to_string(),
            end: "2023-01-31".to_string(),
        }),
        category: None,
        amount_range: None,
        source_ref: None,
        description_contains: None,
    };
    
    let response = service.query_transactions(QueryTransactionsRequest {
        filters,
        sort: None,
        pagination: None,
    }).unwrap();
    
    assert_eq!(response.transactions.len(), 2);
}

#[test]
fn test_query_transactions_applies_sorting() {
    // Create a service with sample data
    let (service, workbook_path) = create_test_service();
    
    // Ingest transactions with different dates and amounts
    let tx1 = TransactionInput {
        account_id: "ACCT1".to_string(),
        date: "2023-01-15".to_string(),
        amount: "300.00".to_string(),
        description: "Zebra".to_string(),
        source_ref: "stmt1.pdf".to_string(),
    };
    
    let tx2 = TransactionInput {
        account_id: "ACCT1".to_string(),
        date: "2023-01-10".to_string(),
        amount: "100.00".to_string(),
        description: "Apple".to_string(),
        source_ref: "stmt1.pdf".to_string(),
    };
    
    let tx3 = TransactionInput {
        account_id: "ACCT1".to_string(),
        date: "2023-01-20".to_string(),
        amount: "200.00".to_string(),
        description: "Banana".to_string(),
        source_ref: "stmt1.pdf".to_string(),
    };
    
    let _ = service.ingest_statement_rows(IngestStatementRowsRequest {
        journal_path: PathBuf::from("test.journal"),
        workbook_path: service.workbook_path().to_path_buf(),
        ontology_path: None,
        rows: vec![tx1, tx2, tx3],
    });
    
    let filters = TransactionFilters {
        account_id: None,
        date_range: None,
        category: None,
        amount_range: None,
        source_ref: None,
        description_contains: None,
    };
    
    // Test sort by date ascending
    let response = service.query_transactions(QueryTransactionsRequest {
        filters: filters.clone(),
        sort: Some(SortSpec {
            field: SortField::Date,
            direction: SortDirection::Asc,
        }),
        pagination: None,
    }).unwrap();
    
    assert_eq!(response.transactions[0].date, "2023-01-10");
    assert_eq!(response.transactions[1].date, "2023-01-15");
    assert_eq!(response.transactions[2].date, "2023-01-20");
    
    // Test sort by amount ascending
    let response = service.query_transactions(QueryTransactionsRequest {
        filters: filters.clone(),
        sort: Some(SortSpec {
            field: SortField::Amount,
            direction: SortDirection::Asc,
        }),
        pagination: None,
    }).unwrap();
    
    assert_eq!(response.transactions[0].amount, "100.00");
    assert_eq!(response.transactions[1].amount, "200.00");
    assert_eq!(response.transactions[2].amount, "300.00");
    
    // Test sort by description ascending
    let response = service.query_transactions(QueryTransactionsRequest {
        filters,
        sort: Some(SortSpec {
            field: SortField::Description,
            direction: SortDirection::Asc,
        }),
        pagination: None,
    }).unwrap();
    
    assert_eq!(response.transactions[0].description, "Apple");
    assert_eq!(response.transactions[1].description, "Banana");
    assert_eq!(response.transactions[2].description, "Zebra");
}

#[test]
fn test_query_transactions_enforces_pagination_limits() {
    // Create a service with many transactions
    let (service, workbook_path) = create_test_service();
    
    // Create 1500 transactions
    let mut transactions = Vec::new();
    for i in 0..1500 {
        transactions.push(TransactionInput {
            account_id: "ACCT1".to_string(),
            date: format!("2023-01-{:02}", (i % 28) + 1),
            amount: format!("{}.00", i),
            description: format!("Transaction {}", i),
            source_ref: "stmt1.pdf".to_string(),
        });
    }
    
    let _ = service.ingest_statement_rows(IngestStatementRowsRequest {
        journal_path: PathBuf::from("test.journal"),
        workbook_path: service.workbook_path().to_path_buf(),
        ontology_path: None,
        rows: transactions,
    });
    
    let filters = TransactionFilters {
        account_id: None,
        date_range: None,
        category: None,
        amount_range: None,
        source_ref: None,
        description_contains: None,
    };
    
    // Test that limit is capped at 1000
    let response = service.query_transactions(QueryTransactionsRequest {
        filters: filters.clone(),
        sort: None,
        pagination: Some(PaginationSpec {
            limit: 2000, // Request more than max
            offset: 0,
        }),
    }).unwrap();
    
    assert_eq!(response.transactions.len(), 1000); // Should be capped at 1000
    assert_eq!(response.total_count, 1500);
    
    // Test offset behavior
    let response = service.query_transactions(QueryTransactionsRequest {
        filters: filters.clone(),
        sort: None,
        pagination: Some(PaginationSpec {
            limit: 100,
            offset: 100,
        }),
    }).unwrap();
    
    assert_eq!(response.transactions.len(), 100);
    assert_eq!(response.total_count, 1500);
    
    // Test offset beyond total
    let response = service.query_transactions(QueryTransactionsRequest {
        filters,
        sort: None,
        pagination: Some(PaginationSpec {
            limit: 100,
            offset: 2000, // Beyond total
        }),
    }).unwrap();
    
    assert_eq!(response.transactions.len(), 0);
    assert_eq!(response.total_count, 1500);
}

#[test]
fn test_query_transactions_deterministic_ordering() {
    // Create a service
    let (service, workbook_path) = create_test_service();
    
    // Create transactions with deterministic content
    let tx1 = TransactionInput {
        account_id: "ACCT1".to_string(),
        date: "2023-01-15".to_string(),
        amount: "100.00".to_string(),
        description: "Coffee Shop".to_string(),
        source_ref: "stmt1.pdf".to_string(),
    };
    
    let tx2 = TransactionInput {
        account_id: "ACCT1".to_string(),
        date: "2023-01-20".to_string(),
        amount: "200.00".to_string(),
        description: "Grocery Store".to_string(),
        source_ref: "stmt1.pdf".to_string(),
    };
    
    // Ingest the same transactions twice and verify consistent results
    let _ = service.ingest_statement_rows(IngestStatementRowsRequest {
        journal_path: PathBuf::from("test.journal"),
        workbook_path: service.workbook_path().to_path_buf(),
        ontology_path: None,
        rows: vec![tx1.clone(), tx2.clone()],
    });
    
    let filters = TransactionFilters {
        account_id: Some("ACCT1".to_string()),
        date_range: None,
        category: None,
        amount_range: None,
        source_ref: None,
        description_contains: None,
    };
    
    let sort = SortSpec {
        field: SortField::Date,
        direction: SortDirection::Desc,
    };
    
    // Query twice
    let response1 = service.query_transactions(QueryTransactionsRequest {
        filters: filters.clone(),
        sort: Some(sort.clone()),
        pagination: None,
    }).unwrap();
    
    let response2 = service.query_transactions(QueryTransactionsRequest {
        filters,
        sort: Some(sort),
        pagination: None,
    }).unwrap();
    
    // Verify results are identical
    assert_eq!(response1.transactions.len(), response2.transactions.len());
    for (tx1, tx2) in response1.transactions.iter().zip(response2.transactions.iter()) {
        assert_eq!(tx1.tx_id, tx2.tx_id);
        assert_eq!(tx1.account_id, tx2.account_id);
        assert_eq!(tx1.date, tx2.date);
        assert_eq!(tx1.amount, tx2.amount);
        assert_eq!(tx1.description, tx2.description);
    }
}

#[test]
fn mcp_query_transactions_advertises_action() {
    use ledgerr_mcp::contract::{PUBLISHED_TOOLS, REVIEW_TOOL};
    
    // Find the REVIEW_TOOL in the published tools
    let review_tool = PUBLISHED_TOOLS.iter().find(|t| t.name == REVIEW_TOOL);
    
    assert!(review_tool.is_some(), "REVIEW_TOOL not found in PUBLISHED_TOOLS");
    
    let review_tool = review_tool.unwrap();
    
    // Verify that query_transactions is in the actions list
    assert!(review_tool.actions.contains(&"query_transactions"), 
            "query_transactions not found in REVIEW_TOOL actions");
}
