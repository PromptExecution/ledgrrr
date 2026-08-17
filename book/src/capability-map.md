# Capability Map

This chapter provides a complete status map of the l3dg3rr system — every major capability, its current implementation state, and what is needed to complete it.

## System Architecture Diagram

```rhai
fn filename_routing() -> blake3_ids
fn filename_routing() -> docling_bridge
fn docling_bridge() -> document_graph
fn document_graph() -> reqif_candidates
fn reqif_candidates() -> rule_registry
fn rule_files() -> rule_registry
fn rule_registry() -> keyword_selector
fn rule_registry() -> semantic_selector
fn keyword_selector() -> classify_waterfall
fn semantic_selector() -> classify_waterfall
fn classify_waterfall() -> classification_engine
fn classification_engine() -> review_flags
fn classification_engine() -> legal_solver
fn legal_solver() -> workbook_output
fn review_flags() -> workbook_output
fn workbook_output() -> audit_trail
fn audit_trail() -> sidecar_state
fn workflow_toml() -> mermaid_generation
fn pipeline_hsm() -> agent_runtime
fn issue_source() -> agent_runtime
fn reqif_opa_bridge() -> agent_runtime
fn xero_catalog() -> reconciliation_candidates
fn ledgerr_mcp_contract() -> agent_runtime
```

## PRD-4 Phase 1: Canonical Ontology Core

The first PRD-4 implementation slice makes `ledger-core` the canonical owner of
ontology artifact and relation primitives while preserving the legacy MCP storage
shape for compatibility.

```rhai
fn ledger_core_types() -> artifact_kind
fn ledger_core_types() -> relation_kind
fn artifact_kind() -> ontology_snapshot
fn relation_kind() -> ontology_snapshot
fn ontology_snapshot() -> mcp_transport_adapter
fn ontology_snapshot() -> visual_audit_graph
```

## PRD-4 Phase 2: Automatic Artifact Relationship Emission

The second PRD-4 implementation slice makes ontology facts emerge from normal
pipeline work: ingest, classification, validation, workbook projection, audit
events, and integration links emit typed artifact relationships instead of
requiring manual ontology upserts.

```rhai
fn ingest_pdf() -> document_artifact
fn ingest_pdf() -> raw_context_artifact
fn document_artifact() -> extracted_row_artifact
fn extracted_row_artifact() -> transaction_artifact
fn transaction_artifact() -> classification_artifact
fn classification_artifact() -> validation_artifact
fn validation_artifact() -> workbook_row_artifact
fn workbook_row_artifact() -> audit_event_artifact
```

## PRD-4 Phase 3: Visual Audit Graph

The third PRD-4 implementation slice turns canonical ontology snapshots into the
supported Rhai diagram DSL so the same graph can be rendered by Mermaid and the
isometric live editor without hand-written diagram source.

```rhai
fn ontology_snapshot() -> filter_graph
match filter.kind => Transaction -> transaction_evidence_view
match filter.kind => Document -> document_lineage_view
match filter.kind => XeroEntity -> xero_reconciliation_view
match filter.kind => ModelJob -> model_proposal_view
match filter.kind => _ -> full_snapshot_view
fn transaction_evidence_view() -> mermaid_2d
fn transaction_evidence_view() -> isometric_3d
```

## PRD-4 Phase 4: Typed Phi-4 Job Runtime

The fourth PRD-4 implementation slice keeps model integration host-owned and
schema-bound: Phi-4 receives a typed job request, returns only JSON, and the host
validates the response before any ontology proposal can become an auditable fact.

```rhai
fn typed_model_job() -> host_agent_runtime
fn host_agent_runtime() -> phi4_local_endpoint
fn phi4_local_endpoint() -> structured_json_response
fn structured_json_response() -> schema_validation
fn schema_validation() -> invariant_validation
fn invariant_validation() -> ontology_proposal
fn ontology_proposal() -> operator_or_policy_gate
fn operator_or_policy_gate() -> committed_ontology_edge
```

## PRD-4 Phase 5: Proposal Review and Commit

The fifth PRD-4 implementation slice gives model-suggested ontology relations a
deterministic lifecycle. Phi-4 can propose an edge, but Rust validation, policy
thresholds, and operator approval decide whether that edge becomes committed
ontology state.

```rhai
fn phi4_proposal() -> parse_typed_output
fn parse_typed_output() -> rust_invariant_check
if confidence >= 0.90 -> policy_gate
if confidence < 0.90 -> operator_review
fn policy_gate() -> committed_edge
fn operator_review() -> approved_edge
fn operator_review() -> rejected_proposal
fn approved_edge() -> committed_edge
fn rejected_proposal() -> audit_event
```

## PRD-4 Phase 6: Local Semantic Retrieval

The sixth PRD-4 implementation slice introduces a local retrieval index for rule
and evidence candidates. The first implementation uses deterministic lexical
records as the fallback index so later model embeddings can improve ranking
without changing candidate IDs, provenance, or Rhai authority.

```rhai
fn document_chunk() -> embedding_record
fn transaction_description() -> embedding_record
fn rule_registry() -> embedding_record
fn embedding_record() -> local_vector_index
fn local_vector_index() -> candidate_context
fn candidate_context() -> phi4_typed_job
fn phi4_typed_job() -> validated_classification
fn validated_classification() -> ontology_edge
```

## PRD-4 Phase 7: End-to-End Audit Playbook

The seventh PRD-4 implementation slice ties a sample statement flow to the
operator-facing proof surface: ingest creates the transaction identity, workbook
export and audit events preserve it, ontology explains it, and the visual graph
shows the path for CPA review.

```rhai
fn sample_statement() -> ingest_rows
fn ingest_rows() -> classify_transactions
fn classify_transactions() -> phi4_edge_proposals
fn phi4_edge_proposals() -> operator_review
fn operator_review() -> workbook_export
fn workbook_export() -> evidence_chain
fn evidence_chain() -> visual_audit_graph
fn visual_audit_graph() -> cpa_review
```

## Component Status Table

| Component | Module | Status | Notes |
|---|---|---|---|
| Filename routing | `filename.rs` | Implemented | `VENDOR--ACCT--YYYY-MM--DOCTYPE` parser |
| Blake3 hash IDs | `ingest.rs` | Implemented | `deterministic_tx_id`, idempotent dedup |
| IngestedLedger | `ingest.rs` | Implemented | Journal + workbook ingest pipeline |
| DocType enum | `document.rs` | Implemented | Document type classification |
| DocumentGraph types | `document.rs` | Implemented | Graph node/edge types defined |
| Pipeline HSM | `pipeline.rs` | Implemented | Type-state + statig state machine |
| Verb trait | `pipeline.rs` | Implemented | DetectVerb, ValidateVerb |
| ClassificationEngine | `classify.rs` | Implemented | Rhai rule execution |
| ClassificationOutcome | `classify.rs` | Implemented | category, confidence, reason |
| ReviewFlag | `classify.rs` | Implemented | Flag upsert, query by year/status |
| Rhai rule files | `rules/` | Implemented | foreign_income, self_employment, fallback |
| Jurisdiction enum | `legal.rs` | Implemented | US, AU, UK |
| LegalRule + Z3 formulas | `legal.rs` | Implemented | Hard predicate checks for AU GST and US Schedule C |
| LegalSolver | `legal.rs` | Implemented | Uses pinned `z3 = 0.8` for violation satisfiability checks behind `legal-z3`; default builds use the same deterministic result semantics without native Z3 |
| Proposer/Reviewer LLM | `verify.rs` | Partial | Pattern defined, no real LLM calls |
| WorkflowToml DSL | `workflow.rs` | Implemented | TOML → Rhai FSM + Mermaid |
| IssueSource::RhaiRule | `validation.rs` | Implemented | Validation layer with rule source |
| Workbook write | `workbook.rs` | Implemented | `rust_xlsxwriter` tx projection |
| Workbook read-back | `workbook.rs` | Implemented | `calamine` round-trip |
| Journal | `journal.rs` | Implemented | NDJSON append and replay |
| Audit trail (MetaCtx) | `pipeline.rs` | Implemented | Mutation log per pipeline state |
| MCP contract | `ledgerr-mcp/src/contract.rs` | Implemented | 8 advertised `ledgerr_*` capability families |
| MCP adapter | `ledgerr-mcp/src/mcp_adapter.rs` | Implemented | Dispatches contract actions to `TurboLedgerService` |
| Ontology store | `ledgerr-mcp/src/ontology.rs` | Implemented | Entity/edge upsert and path query surface |
| Xero service | `ledgerr-mcp/src/xero_service.rs` | Partial | Supervised catalog/link actions; credentials remain host-owned |
| Mermaid auto-generation | `workflow.rs` | Implemented | rhai DSL → diagram blocks |
| RuleRegistry | `rule_registry.rs` | Implemented | Loads transaction `.rhai` rules and optional ReqIF sidecars |
| Keyword rule selection | `rule_registry.rs` | Implemented | Deterministic keyword fallback; semantic selector remains planned |
| Waterfall orchestration | `rule_registry.rs` | Implemented | First non-`Unclassified` result wins; fallback outcome is preserved |
| ReqIfCandidate (Rust) | `rule_registry.rs` | Stub | Type defined, sidecar bridge missing |
| DocumentChunk | `rule_registry.rs` | Stub | Type defined, bridge missing |
| SemanticRuleSelector | `rule_registry.rs` | Stub | Trait defined, embeddings not wired |
| Docling extraction bridge | — | Missing | Python subprocess call not written |
| reqif-opa-mcp MCP wiring | — | Missing | No Rust MCP client for sidecar |
| Vector embedding index | — | Missing | No embedding model or HNSW index |
| File watcher (notify) | — | Missing | `notify` crate not yet wired |

## North Star Pipeline (Rhai DSL)

The following DSL block describes the full intended end-to-end system flow — the "north star" that all stub work is building toward.

```rhai
fn document_ingest() -> reqif_extract
fn reqif_extract() -> opa_gate
fn opa_gate() -> rule_registry
fn rule_registry() -> classify_waterfall
fn classify_waterfall() -> legal_verify
fn legal_verify() -> workbook_commit
fn workbook_commit() -> audit_trail
```

Each step in this pipeline has a corresponding Rust type or trait. The deterministic rule-registry waterfall is now implemented; the remaining ingestion-side gap is the `reqif-opa-mcp` bridge and semantic rule-selection infrastructure. Z3 is wired for the first hard legal predicates; broader solver coverage is still a roadmap item.

## Next Steps

The five highest-value missing capabilities to implement, in priority order:

1. **Docling extraction bridge** — Write the Rust `std::process::Command` call to invoke the Python sidecar, parse its NDJSON stdout, and deserialize into `DocumentChunk` / `ReqIfCandidate`. This is the critical path for Phase 2 document intelligence. Estimated scope: new `sidecar.rs` module, ~100 lines.

2. **Wire `ClassifyTransactionsOp` to `RuleRegistry`** — Replace the current operation stub with registry loading, transaction iteration, waterfall classification, and review-flag emission.

3. **Expand `LegalSolver` coverage** — Add hard Z3 checks for FBAR/FATCA thresholds, mutually exclusive categories, and reconciliation/workbook invariants. The initial Z3 integration covers AU GST and US Schedule C predicates.

4. **File watcher via `notify`** — Add a debounced `notify` watcher on the workbook path and the `rules/` directory. This enables live rule reloading and human Excel-edit detection without polling. Estimated scope: new `watcher.rs` module, ~60 lines.

5. **`SemanticRuleSelector` embedding index** — Wire a local `fastembed-rs` or ONNX embedding model to encode transaction descriptions and `ReqIfCandidate` texts. Build an HNSW index over candidate embeddings for cosine-similarity rule selection. Estimated scope: ~200 lines, depends on item 2.
