//! Integration tests for the l3dg3rr tax-ledger pipeline.
//!
//! All tests in this module are marked `#[ignore]` because they depend on
//! infrastructure or APIs not yet implemented. They are designed to:
//!   1. Compile without error against current types
//!   2. Fail at runtime (either via `unimplemented!()` panic or an assertion on
//!      `LedgerOpError::NotImplemented`)
//!   3. Document the desired behavior in detail so future implementors have
//!      unambiguous acceptance criteria
//!
//! Run all ignored tests (expect failures) with:
//!   cargo test -p ledger-core --test integration_tests -- --ignored

#[cfg(test)]
mod integration {
    use std::path::PathBuf;

    // -------------------------------------------------------------------------
    // Test #4 — Calendar drives OperationDispatcher
    // -------------------------------------------------------------------------

    /// Verify that a `BusinessCalendar` can be the sole source of truth for
    /// constructing an `OperationDispatcher`.
    ///
    /// # What needs to be built first
    /// `OperationDispatcher::from_scheduled_events(&[ScheduledEvent])` — a
    /// constructor that iterates the event list, maps each event's `operation`
    /// field to a concrete `Box<dyn LedgerOperation>`, and registers them so
    /// `run_by_id(event.id)` dispatches the correct op.
    #[test]
    fn test_calendar_drives_operation_dispatcher() {
        use crate::calendar::BusinessCalendar;
        use crate::ledger_ops::{OperationContext, OperationDispatcher};

        let cal = BusinessCalendar::us_tax_defaults();
        let dispatcher = OperationDispatcher::from_scheduled_events(&cal.events);

        let ctx = OperationContext::new(PathBuf::from("/tmp/working"), PathBuf::from("/tmp/rules"));

        let result = dispatcher.run_by_id("us-quarterly-estimated", &ctx);
        assert!(
            result.is_some(),
            "dispatcher should have an op keyed by event id 'us-quarterly-estimated'"
        );
        assert!(
            result.unwrap().is_ok(),
            "CheckTaxDeadlineOp should return Ok"
        );
    }

    // Tests #5a and #5b moved to crates/ledgerr-mcp/tests/tools.rs where they
    // can import TOOL_REGISTRY directly from the ledgerr-mcp crate.

    // -------------------------------------------------------------------------
    // Test #6 — PDF ingest via subprocess sidecar
    // -------------------------------------------------------------------------

    /// Verify that `IngestStatementOp::execute()` can process a fixture PDF via
    /// the Docling sidecar and produce at least one ingested transaction row.
    ///
    /// # What needs to be built first
    /// Phase-2 work: `IngestStatementOp::execute()` must:
    ///   - Spawn `docling --pdf <path> --output ndjson` (or equivalent)
    ///   - Parse NDJSON stdout into transaction rows
    ///   - Compute Blake3 content-hash IDs
    ///   - Return `OperationResult { success: true, items_processed: N }`
    ///
    /// Also requires: `tests/fixtures/sample_hsbc_statement.pdf`
    #[test]
    #[ignore = "requires IngestStatementOp::execute() subprocess wiring — phase-2 work; also needs fixture PDF"]
    fn test_ingest_statement_via_pdf_sidecar() {
        // DESIRED BEHAVIOR:
        // IngestStatementOp::execute() should:
        //   1. Glob ctx.working_dir / self.source_glob for PDF files
        //   2. For each file, spawn the Docling sidecar CLI:
        //        docling --pdf <path> --output ndjson
        //   3. Read NDJSON lines from stdout; deserialize each as a transaction row
        //   4. Compute Blake3 ID: blake3(account_id + date + amount + description)
        //   5. Upsert rows (skip duplicates by hash)
        //   6. Return OperationResult { success: true, items_processed: rows_seen,
        //                               items_flagged: rows_needing_review }
        //
        // The fixture at tests/fixtures/sample_hsbc_statement.pdf should contain
        // exactly one transaction line for deterministic test assertions.
        use crate::ledger_ops::{
            IngestStatementOp, LedgerOpError, LedgerOperation, OperationContext,
        };

        let op = IngestStatementOp {
            source_glob: "tests/fixtures/*.pdf".to_string(),
            vendor_hint: Some("HSBC".to_string()),
        };

        // Point working_dir at the repo root so the glob resolves correctly.
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap() // crates/ledger-core → crates
            .parent()
            .unwrap() // crates → repo root
            .to_path_buf();

        let ctx = OperationContext::new(repo_root, PathBuf::from("/tmp/rules"));

        let result = op.execute(&ctx);

        // Current expectation: returns NotImplemented (phase-1 stub)
        // Future expectation after phase-2: returns Ok with items_processed > 0
        match &result {
            Err(LedgerOpError::NotImplemented(_)) => {
                panic!(
                    "IngestStatementOp still returns NotImplemented — implement PDF sidecar \
                     subprocess call in phase-2 to make this test pass"
                );
            }
            Ok(op_result) if !op_result.success => {
                panic!("PDF ingest returned success=false: {:?}", op_result.issues);
            }
            Ok(op_result) => {
                assert!(
                    op_result.items_processed > 0,
                    "should have ingested at least one row from fixture PDF; got 0"
                );
            }
            Err(e) => panic!("unexpected error during PDF ingest: {e:?}"),
        }
    }

    // -------------------------------------------------------------------------
    // Test #7 — Cedar/AGT gate filters transactions by compliance grade
    // -------------------------------------------------------------------------

    /// Test CedarGateOp without cedar-policy feature (should be no-op).
    ///
    /// This test only runs when cedar-policy feature is DISABLED.
    #[cfg(not(feature = "cedar-policy"))]
    #[test]
    fn test_cedar_gate_no_op_without_feature() {
        use crate::ledger_ops::{CedarGateOp, LedgerOperation, OperationContext};

        // Without feature, CedarGateOp should be a no-op
        let op = CedarGateOp;
        let ctx = OperationContext::new(PathBuf::from("/tmp/working"), PathBuf::from("/tmp/rules"));

        let result = op.execute(&ctx);

        assert!(result.is_ok());
        let r = result.unwrap();
        assert!(r.success);
    }

    /// Test CedarGateOp with compliance feature enabled.
    ///
    /// This test requires the `cedar-policy` feature to be active.
    #[cfg(feature = "cedar-policy")]
    #[test]
    fn test_cedar_gate_with_full_compliance() {
        use std::sync::Arc;
        use crate::ledger_ops::{CedarGateOp, LedgerOperation, OperationContext};

        // Create gateway with default policy
        let gw = msft_agent_gov_ledgrrr::LedgrrAgtGateway::new("test-agent").unwrap();

        // Register a control and attest it to achieve Full grade
        gw.register_compliance_control("soc2-cc6.1");
        gw.attest_z3_proof("soc2-cc6.1", "abc123def456");

        let ctx = OperationContext::new(PathBuf::from("/tmp/working"), PathBuf::from("/tmp/rules"))
            .with_gateway(Arc::new(gw));

        let op = CedarGateOp;
        let result = op.execute(&ctx);

        assert!(result.is_ok());
        let r = result.unwrap();
        assert!(r.success);
        assert_eq!(r.items_processed, 0);
        assert_eq!(r.items_flagged, 0);
    }

    /// Test CedarGateOp with Partial compliance grade.
    #[cfg(feature = "cedar-policy")]
    #[test]
    fn test_cedar_gate_with_partial_compliance() {
        use std::sync::Arc;
        use crate::ledger_ops::{CedarGateOp, LedgerOperation, OperationContext};

        // Create gateway and register multiple controls but only attest one
        let gw = msft_agent_gov_ledgrrr::LedgrrAgtGateway::new("test-agent").unwrap();
        gw.register_compliance_control("soc2-cc6.1");
        gw.register_compliance_control("eu-ai-act-art-13");
        gw.attest_z3_proof("soc2-cc6.1", "abc123def456");
        // Note: eu-ai-act-art-13 is not attested, so grade will be Partial

        let ctx = OperationContext::new(PathBuf::from("/tmp/working"), PathBuf::from("/tmp/rules"))
            .with_gateway(Arc::new(gw));

        let op = CedarGateOp;
        let result = op.execute(&ctx);

        assert!(result.is_ok());
        let r = result.unwrap();
        assert!(r.success);
        assert_eq!(r.items_processed, 0);
        assert_eq!(r.items_flagged, 0);
        assert!(!r.issues.is_empty());
        assert!(r.issues[0].contains("Partial"));
    }

    /// Test CedarGateOp with Unknown compliance grade (no attestations).
    #[cfg(feature = "cedar-policy")]
    #[test]
    fn test_cedar_gate_with_unknown_compliance() {
        use std::sync::Arc;
        use crate::ledger_ops::{CedarGateOp, LedgerOperation, OperationContext};

        // Create gateway with no attestations
        let gw = msft_agent_gov_ledgrrr::LedgrrAgtGateway::new("test-agent").unwrap();

        let ctx = OperationContext::new(PathBuf::from("/tmp/working"), PathBuf::from("/tmp/rules"))
            .with_gateway(Arc::new(gw));

        let op = CedarGateOp;
        let result = op.execute(&ctx);

        assert!(result.is_ok());
        let r = result.unwrap();
        assert!(r.success);
        assert_eq!(r.items_processed, 0);
        assert_eq!(r.items_flagged, 0);
        assert!(!r.issues.is_empty());
        assert!(r.issues[0].contains("Unknown"));
    }

    /// Test CedarGateOp without gateway (should error).
    #[cfg(feature = "cedar-policy")]
    #[test]
    fn test_cedar_gate_without_gateway() {
        use crate::ledger_ops::{CedarGateOp, LedgerOpError, LedgerOperation, OperationContext};

        let ctx = OperationContext::new(PathBuf::from("/tmp/working"), PathBuf::from("/tmp/rules"));

        let op = CedarGateOp;
        let result = op.execute(&ctx);

        assert!(result.is_err());
        match result {
            Err(LedgerOpError::InvalidInput(msg)) => {
                assert!(msg.contains("gateway"));
            }
            Err(e) => panic!("expected InvalidInput, got: {e:?}"),
            Ok(_) => panic!("expected error when gateway is None"),
        }
    }

    // -------------------------------------------------------------------------
    // Test #8 — LLM verification proposes a repair for a classification outcome
    // -------------------------------------------------------------------------

    // Uses MockModelClient for deterministic coverage.
    // Replace proposer/reviewer with AnthropicModelClient for live LLM coverage.
    #[test]
    fn test_llm_verification_proposes_category() {
        use crate::verify::{
            MockModelClient, MultiModelConfig, MultiModelVerifier, VerificationOutcome,
        };
        let proposer_json = r#"{
            "rule_id": "ForeignIncome",
            "proposed_fix": "ForeignIncome",
            "reasoning": "Wire transfer from foreign employer matches ForeignIncome pattern",
            "confidence": 0.92
        }"#;
        let reviewer_json = r#"{"approved":true,"concerns":[],"suggestions":[],"confidence":0.90}"#;

        let proposer = MockModelClient::default().with_response(proposer_json);
        let reviewer = MockModelClient::default().with_response(reviewer_json);

        let config =
            MultiModelConfig::new("claude-haiku-4-5-20251001", "claude-haiku-4-5-20251001")
                .with_threshold(0.80);

        let verifier = MultiModelVerifier::new(proposer, reviewer, config);

        // issues_json represents a classification outcome that needs repair
        let issues_json = r#"[{"field":"category","value":"Unclassified","confidence":0.3}]"#;
        let context = "transaction: {account_id: HSBC-INTL-001, description: Wire transfer from DE employer, amount: 5000.00}";

        let outcome = verifier
            .verify("ForeignIncome", issues_json, context)
            .expect("verifier should not error with mock clients");

        assert!(
            outcome.is_approved(),
            "mock models should agree and approve; if using real models they may disagree — \
             that is expected behavior, not a bug"
        );

        match outcome {
            VerificationOutcome::Approved { proposal, review } => {
                assert!(
                    !proposal.rule_id.is_empty(),
                    "proposal.rule_id (category) must not be empty"
                );
                assert!(
                    proposal.confidence > 0.0 && proposal.confidence <= 1.0,
                    "proposal.confidence must be in (0, 1]"
                );
                assert!(
                    review.confidence > 0.0 && review.confidence <= 1.0,
                    "review.confidence must be in (0, 1]"
                );
            }
            VerificationOutcome::Rejected { proposal, review } => {
                panic!(
                    "verifier rejected proposal — proposer said {:?}, reviewer said {:?}",
                    proposal.rule_id, review.concerns
                );
            }
        }
    }

    // -------------------------------------------------------------------------
    // Test #9 — Semantic rule selector selects by embedding
    // -------------------------------------------------------------------------

    /// Verify that `SemanticRuleSelector::select_rules_semantic()` can match a
    /// German-language transaction description to the correct Rhai rule file
    /// without any keyword overlap.
    ///
    /// # What needs to be built first
    /// `load_from_dir`, `build_embedding_index`, and `select_rules_semantic` are
    /// all implemented and wired (lexical/Jaccard similarity). This test is kept
    /// ignored because it validates **cross-lingual** semantic matching — mapping
    /// the German "Auslandüberweisung" to English "foreign_income" without shared
    /// tokens — which requires real vector embeddings (fastembed-rs, candle, or
    /// an ONNX sidecar). Lexical similarity cannot satisfy this assertion.
    #[test]
    #[ignore = "cross-lingual semantic matching requires vector embedding infrastructure (fastembed-rs / candle / ONNX)"]
    fn test_semantic_rule_selector_selects_by_embedding() {
        // DESIRED BEHAVIOR (requires real embedding model):
        // 1. registry.build_embedding_index() must encode each rule file's content
        //    via a local embedding model into a shared vector space.
        //
        // 2. registry.select_rules_semantic(&tx, 3) must encode tx.description
        //    ("Auslandüberweisung von DE Arbeitgeber") and return the top-3 rule
        //    paths by cosine similarity. "Auslandüberweisung" (German: "foreign
        //    transfer") should match classify_foreign_income.rhai even though the
        //    German word shares no tokens with the English rule — proving semantic
        //    (not lexical) bridging.
        //
        // The test asserts that at least one returned path contains "foreign_income",
        // which Jaccard/lexical selection cannot guarantee.
        use crate::classify::SampleTransaction;
        use crate::rule_registry::{RuleRegistry, SemanticRuleSelector};

        let rule_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap() // crates/ledger-core → crates
            .parent()
            .unwrap() // crates → repo root
            .join("rules");

        let mut registry =
            RuleRegistry::load_from_dir(&rule_dir).expect("should load rules from rules/ dir");

        // build_embedding_index is implemented (lexical similarity); re-calling it
        // here is a no-op rebuild — the index was already built by load_from_dir.
        registry
            .build_embedding_index()
            .expect("should rebuild embedding index over rule files");

        let tx = SampleTransaction {
            tx_id: "test-semantic-001".to_string(),
            account_id: "HSBC-DE-001".to_string(),
            date: "2024-03-15".to_string(),
            amount: "3200.00".to_string(),
            description: "Auslandüberweisung von DE Arbeitgeber".to_string(),
        };

        // top_k = 5: return up to 5 most semantically similar rules
        let selected = registry.select_rules_semantic(&tx, 5);

        assert!(
            !selected.is_empty(),
            "semantic selector must return at least one rule for a foreign transfer description"
        );

        let names: Vec<&str> = selected
            .iter()
            .filter_map(|p| p.file_name()?.to_str())
            .collect();

        assert!(
            names.iter().any(|n| n.contains("foreign_income")),
            "expected classify_foreign_income.rhai in top-5 semantic matches for \
             'Auslandüberweisung von DE Arbeitgeber'; got: {names:?}\n\
             This means the embedding model did NOT map the German 'Auslandüberweisung' \
             close enough to the English 'foreign income' vector space."
        );
    }
}
