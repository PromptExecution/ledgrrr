use ledger_core::ingest::TransactionInput;
use ledgerr_mcp::{
    ClassifyTransactionRequest, EventHistoryFilter, FlagStatusRequest, HsmResumeRequest,
    HsmStatusRequest, HsmTransitionRequest, IngestStatementRowsRequest, QueryAuditLogRequest,
    QueryFlagsRequest, ReplayLifecycleRequest, TurboLedgerService, TurboLedgerTools,
};

fn manifest_for(workbook_path: &std::path::Path) -> String {
    // Escape backslashes/quotes for TOML — Windows temp paths are full of
    // `\` separators, which are not valid TOML string escapes on their own
    // (e.g. `\U`, `\A`, ...) and would otherwise fail to parse.
    let escaped = workbook_path
        .display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    format!("[session]\nworkbook_path=\"{escaped}\"\nactive_year=2023\n")
}

fn sample_row(source_ref: &std::path::Path) -> TransactionInput {
    TransactionInput {
        account_id: "WF-BH-CHK".to_string(),
        date: "2023-01-15".to_string(),
        amount: "-42.11".to_string(),
        description: "Coffee Shop".to_string(),
        source_ref: source_ref.display().to_string(),
    }
}

#[test]
fn restart_01_persists_ingest_review_audit_state_and_idempotency() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workbook_path = temp.path().join("tax-ledger.xlsx");
    let journal_path = temp.path().join("ledger.beancount");
    let source_ref = temp.path().join("source").join("ctx.rkyv");
    std::fs::create_dir_all(source_ref.parent().expect("source parent")).expect("mkdir");
    std::fs::write(&source_ref, b"ctx").expect("write source");
    let manifest = manifest_for(&workbook_path);
    let row = sample_row(&source_ref);

    let tx_id = {
        let service = TurboLedgerService::from_manifest_str(&manifest).expect("manifest");
        let ingest = service
            .ingest_statement_rows(IngestStatementRowsRequest {
                journal_path: journal_path.clone(),
                workbook_path: workbook_path.clone(),
                ontology_path: None,
                rows: vec![row.clone()],
            })
            .expect("ingest");
        let tx_id = ingest.tx_ids[0].clone();
        service
            .classify_transaction(ClassifyTransactionRequest {
                tx_id: tx_id.clone(),
                category: "Uncategorized".to_string(),
                confidence: "0.40".to_string(),
                note: Some("restart durability".to_string()),
                actor: "agent".to_string(),
            })
            .expect("classify");
        tx_id
    };

    let reloaded = TurboLedgerService::from_manifest_str(&manifest).expect("reload manifest");
    let second_ingest = reloaded
        .ingest_statement_rows(IngestStatementRowsRequest {
            journal_path: journal_path.clone(),
            workbook_path: workbook_path.clone(),
            ontology_path: None,
            rows: vec![row],
        })
        .expect("reingest after restart");
    assert_eq!(second_ingest.inserted_count, 0);

    let flags = reloaded
        .query_flags(QueryFlagsRequest {
            year: 2023,
            status: FlagStatusRequest::Open,
        })
        .expect("flags after restart");
    assert_eq!(flags.flags.len(), 1);
    assert_eq!(flags.flags[0].tx_id, tx_id);

    let audit = reloaded
        .query_audit_log(QueryAuditLogRequest)
        .expect("audit after restart");
    assert_eq!(audit.entries.len(), 2);
    assert!(audit
        .entries
        .iter()
        .all(|entry| entry.tx_id == tx_id && entry.actor == "agent"));

    let reconciled = reloaded
        .classify_transaction(ClassifyTransactionRequest {
            tx_id: tx_id.clone(),
            category: "Meals".to_string(),
            confidence: "0.91".to_string(),
            note: Some("post-restart edit".to_string()),
            actor: "agent".to_string(),
        })
        .expect("classification after restart");
    assert_eq!(reconciled.tx_id, tx_id);
    assert_eq!(reconciled.category, "Meals");
}

#[test]
fn restart_02_persists_event_history_and_replay_state() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workbook_path = temp.path().join("tax-ledger.xlsx");
    let journal_path = temp.path().join("ledger.beancount");
    let source_ref = temp.path().join("source").join("ctx.rkyv");
    std::fs::create_dir_all(source_ref.parent().expect("source parent")).expect("mkdir");
    std::fs::write(&source_ref, b"ctx").expect("write source");
    let manifest = manifest_for(&workbook_path);
    let row = sample_row(&source_ref);

    let tx_id = {
        let service = TurboLedgerService::from_manifest_str(&manifest).expect("manifest");
        let ingest = service
            .ingest_statement_rows(IngestStatementRowsRequest {
                journal_path: journal_path.clone(),
                workbook_path: workbook_path.clone(),
                ontology_path: None,
                rows: vec![row.clone()],
            })
            .expect("ingest");
        let tx_id = ingest.tx_ids[0].clone();
        service
            .classify_transaction(ClassifyTransactionRequest {
                tx_id: tx_id.clone(),
                category: "Meals".to_string(),
                confidence: "0.91".to_string(),
                note: Some("classify".to_string()),
                actor: "agent".to_string(),
            })
            .expect("classify");
        tx_id
    };

    let reloaded = TurboLedgerService::from_manifest_str(&manifest).expect("reload manifest");
    let history = reloaded
        .event_history(EventHistoryFilter {
            tx_id: Some(tx_id.clone()),
            document_ref: Some(source_ref.display().to_string()),
            time_start: None,
            time_end: None,
        })
        .expect("history after restart");
    assert_eq!(history.events.len(), 2);
    assert_eq!(history.events[0].event_type, "ingest");
    assert_eq!(history.events[1].event_type, "classification");

    let replay = reloaded
        .replay_lifecycle(ReplayLifecycleRequest {
            tx_id: Some(tx_id),
            document_ref: Some(source_ref.display().to_string()),
        })
        .expect("replay after restart");
    assert!(replay.reconstructed_state.contains("stage=classification"));
    assert!(replay.reconstructed_state.contains("category=Meals"));
    assert_eq!(replay.event_count, 2);
    assert!(replay.diagnostics.is_empty());
}

#[test]
fn restart_03_persists_hsm_checkpoint_across_reload() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workbook_path = temp.path().join("tax-ledger.xlsx");
    let manifest = manifest_for(&workbook_path);

    {
        let service = TurboLedgerService::from_manifest_str(&manifest).expect("manifest");
        let response = service
            .hsm_transition_tool(HsmTransitionRequest {
                target_state: "normalize".to_string(),
                target_substate: "ready".to_string(),
            })
            .expect("transition");
        assert_eq!(response.state_marker, "normalize:ready:advanced");
    }

    let reloaded = TurboLedgerService::from_manifest_str(&manifest).expect("reload manifest");
    let status = reloaded.hsm_status_tool(HsmStatusRequest).expect("status");
    assert_eq!(status.display_state, "normalize.ready");
    assert_eq!(status.next_hint, "advance_to_validate");

    let resumed = reloaded
        .hsm_resume_tool(HsmResumeRequest {
            state_marker: "normalize:ready:advanced".to_string(),
        })
        .expect("resume");
    assert!(resumed.resumed);
    assert_eq!(resumed.resume_from, "normalize:ready:advanced");
}
