## Agent Quickstart (Read This First)

This file is the persistent operator manual for future agents.  
For product scope and status, read `README.md` first, then use this file for execution rules and MCP usage patterns.

## System Identity — What ledgrrr Is

ledgrrr is a **local agentic governance proxy**: a memory-safe, deterministically executable knowledge-retrieval and poly-tool governance system for supervised AI/LLM workflows. It is a new class of software — not a CRM, not a cloud SaaS, not a plain ETL pipeline.

**Core capabilities (production-ready as of 2026-05)**:

| Capability | How it works |
|---|---|
| Deterministic knowledge retrieval | Blake3 content-hash IDs + idempotent ingest + rkyv sidecar snapshots — same source always produces the same tx_ids regardless of execution order |
| Poly-tool governance | 10 published `ledgerr_*` MCP tools, each a supervised capability family with required `action` parameters; agent calls go through the governance proxy, never directly to raw APIs |
| Workflow visualization | `HasVisualization` trait on 21+ domain types; isometric 3D + Mermaid 2D dual render paths; `WorkflowToml` compiles to operator Mermaid diagram + Rhai FSM execution graph |
| Runtime-editable policy | Rhai scripting for classification rules, flag heuristics, and workflow FSM steps — agent/human editable without recompile |
| Formal verification | `legal-z3` feature gate enables Z3-backed hard satisfiability checks on jurisdiction rules; Kasuari handles soft plausibility/layout constraints |
| Self-generating docs | `contract.rs` is the single source of truth; `regen-docs` binary regenerates `docs/mcp-capability-contract.md`, `docs/agent-mcp-runbook.md`, and `scripts/mcp_cli_demo.sh` — drift is a CI failure |
| Evidence traceability | arc-kit-au petgraph: source documents → extracted rows → transactions → classifications → proposals → approvals → workbook rows with deterministic Blake3 node identity |
| Inter-node capability trading | `b00t-iface` `CapabilityOffer` handshake — nodes advertise and negotiate capabilities before wiring |

**Local working recipe** (all components present in codebase):
- **Tauri host** (`crates/ledgrrr-host`) — desktop control plane: notifications, tray, credential manager, agent runtime, Rig-backed model calls
- **Local LLM chat** — `internal_openai.rs` OpenAI-compatible localhost server; selectable: `phi-4-mini-reasoning` (local), Foundry Local (Windows AI), or remote cloud
- **Semantic knowledge graph** — codebase-memory-mcp for structural graph queries; HelixDB (or `heed`+`petgraph` fallback) for workbook fact projection
- **KV cache** — rkyv sidecar archives per source document; JSON state sidecar for restart-visible service state (atomic rename pattern, fail-closed on corruption)
- **Memory-safe Rust package** — workspace-wide `forbid(unsafe_code)` except Tauri/Slint macro boundaries; `rust_decimal` for all monetary values; no f64 in domain paths
- **Rhai scripting** — classification rules via `fn classify(tx)`, workflow FSM compiler output, docs visualization DSL
- **Introspectable docs** — `mdbook-rhai-mermaid` preprocessor; live Rhai diagram editor in browser; executable code examples as integration tests; deployed at promptexecution.github.io/l3dg3rr

**Current phase: entering dogfood/integration.** Architecture is substantially complete. Next step is running ledgrrr inside the Hermes OpenAgent harness as the governance control surface (see roadmap below).

---

### Roadmap — Next Phase (2026-Q2/Q3)

**Hermes OpenAgent integration** (dogfood harness):
- Run ledgrrr as a governed MCP server inside the Hermes OpenAgent orchestration layer.
- Hermes provides the outer agent loop; ledgrrr provides the governance proxy, audit trail, credential mediation, and policy enforcement.
- Wire `McpProviderRegistry` into `ledgerr-mcp-server.rs` binary + add `handle_external_tool` dispatcher to route Hermes tool calls through the registry.
- Current `McpProvider` trait (`crates/ledgerr-mcp/src/provider.rs`) is the invariant — `B00tProvider`, `JustProvider`, `Ir0ntologyProvider` already registered; Xero next.
- Target: ledgrrr's own tax-ledger dataset is the first dogfood corpus.

**MBSE / SysML-v2 isometric expansion**:
- Extend the `HasVisualization` trait system to support MBSE meta-type modeling.
- Integration path: KerML textual notation as metamodel source of truth → codegen generating both Rust structs and TypeScript types simultaneously. Papyrus EMF bridge is an alternative ingestion path for existing SysML models.
- Type architecture ruling: SysML v2 / KerML is the canonical type source. `specta`/`tauri-specta` (Rust-first type generation) is eliminated — it creates lock-in that conflicts with model-first architecture. `wasm-bindgen` for client-side graph operations is deferred until a concrete UX item requires it.
- 3D workflow visualizations embedded in extensible domain types via the existing `ZLayer` stack (Document/Pipeline/Constraint/Legal/FormalProof/Attestation).
- `SemanticType` enum gains `SysML` variant; `RhaiDsl` captures SysML block/port/flow definitions.
- Output: operator can view a classified transaction as a SysML block diagram or an isometric pipeline diagram interchangeably.
- `crates/ledger-core/src/iso.rs` is the extension point; new `VisualizationSpec` fields are additive.

**Formal verification expansion**:
- `microsoft/rust-z3` bindings behind `legal-z3` feature are the production path for hard SAT obligations.
- Extend `Z3Result` / `LegalSolver` to cover: FBAR threshold proofs, Schedule C deductibility predicates, cross-jurisdiction double-taxation exclusion checks.
- Kasuari (soft Cassowary constraint layout) handles plausibility + visualization bounds — not interchangeable with Z3.
- Next: add `ProofAttestation` type with `HasVisualization` impl in the `ZLayer::Attestation` layer; link proof outcomes to arc-kit-au evidence nodes.

**Local fine-tuning path**:
- Phi-4-mini-reasoning is the default local model; fine-tuning target is tax classification heuristics from the operator's own transaction corpus.
- Training pipeline: classified transactions → JSONL export → local fine-tune job → updated model weights loaded into Foundry Local.
- Keep fine-tuning artifacts in the `rkyv` sidecar pattern so provenance is traceable.

---

### Current Direction (2026-05-05)

Architecture is shifting from build phase to integration/dogfood phase. Core governance proxy capabilities are production-ready; next milestones are Hermes integration and MBSE visualization expansion.

Current operating assumptions:
- `l3dg3rr` is the Rust host/control plane for agent execution, policy, approvals, audit, notifications, and credential access.
- Agent orchestration may run in a sidecar runtime (Hermes OpenAgent, LangGraph, etc.), but secrets and process supervision remain owned by `l3dg3rr`.
- Xero access is mediated through supervised MCP worker processes — not raw credentials to the model.
- Windows 11 desktop support is first-class: toast/app notifications, tray/menubar, persistent settings.
- Slint is the legacy UI shell; Tauri (`crates/ledgrrr-host`) is the primary desktop host. CI checks Tauri; Slint CI is opt-in only.
- arc-kit-au (`crates/arc-kit-au`) is the evidence traceability layer — petgraph-backed, deterministic Blake3 node identity, surfaced through `ledgerr_evidence` MCP tool.
- b00t-iface CapabilityOffer handshake enables inter-node capability negotiation before tool wiring.

Desktop control-plane milestones complete:
- persistent notification settings (`enabled/disabled`, backend health, last test result)
- tray icon with quick actions (`toast enabled`, `test toast`, `status`, `show window`, `quit`)
- notifier abstraction with Windows Credential Manager for long-lived secrets
- audit-friendly event flow from agent/tool execution to UI and notifications
- Tauri dashboard with PANELS-array-generated sidebar (no manually numbered panel indices)

### Purpose (non-duplicate)

`AGENTS.md` is intentionally operational. It should not restate the full product brief from the `## Project` section below.

### Capability / Usage Notes

- Treat `Justfile` as the canonical source of build, test, run, and host-launch commands.
- When a command changes, update `Justfile` first and then reference the recipe name here instead of copying the shell line.
- Prefer `just ...` recipes over ad hoc shell invocations for repeatable work, especially for Windows-host builds and tray validation.
- Keep capability and usage notes concise and action-oriented: say what exists, how to invoke it, and what not to assume.
- If a workflow depends on a repo capability, document the recipe name, not an inline transcript of the implementation.

### MCP Capability Training (Concrete)

Use `TurboLedgerService` in `crates/ledgerr-mcp/src/lib.rs` as the canonical contract.
Use `docs/mcp-capability-contract.md` as the canonical MCP surface map (tool names, arg contracts, service mapping, contrived usage flow).

Published MCP surface rule:
- Default `tools/list` exposes **10** top-level `ledgerr_*` capability families: `ledgerr_documents`, `ledgerr_review`, `ledgerr_reconciliation`, `ledgerr_workflow`, `ledgerr_audit`, `ledgerr_tax`, `ledgerr_ontology`, `ledgerr_xero`, `ledgerr_evidence`, `ledgerr_focus`.
- `ledgerr_evidence` — arc-kit-au evidence traceability graph (actions: `summary`, `list_nodes`, `node_detail`).
- `ledgerr_focus` — FOCUS (FinOps Cost Usage Spec) v1.3 cost/usage analysis (always-on core capability, no feature gate).
- Use required `action` parameters to expose sub-operations while keeping major capability families visible.
- Keep any legacy `l3dg3rr_*` or proxy-style names hidden compatibility aliases only; do not advertise them in the default tool catalog.
- Source of truth: `crates/ledgerr-mcp/src/contract.rs` → `PUBLISHED_TOOLS: [ToolContractSpec; 10]`. Drift from generated docs is a CI failure (`contract_docs_are_generated_from_rust_source`).

Core methods:
- `list_accounts` / `list_accounts_tool`: enumerate account ids from manifest.
- `validate_source_filename`: enforce `VENDOR--ACCOUNT--YYYY-MM--DOCTYPE.ext`.
- `ingest_statement_rows`: idempotent journal/workbook ingest; returns deterministic `tx_ids`.
- `ingest_pdf`: preflight filename + writes raw context bytes when missing + ingests rows.
- `get_raw_context`: read bytes from `rkyv_ref`.
- `run_rhai_rule`, `classify_ingested`, `query_flags`, `classify_transaction`, `reconcile_excel_classification`, `query_audit_log`.
- `export_cpa_workbook`, `get_schedule_summary`.

Concrete example 1 (account discovery):
```rust
let service = TurboLedgerService::from_manifest_str(manifest)?;
let response = service.list_accounts_tool(ListAccountsRequest)?;
assert_eq!(response.accounts[0].account_id, "WF-BH-CHK");
```

Concrete example 2 (idempotent ingest):
```rust
let first = service.ingest_statement_rows(IngestStatementRowsRequest {
    journal_path,
    workbook_path,
    rows,
})?;
let second = service.ingest_statement_rows(IngestStatementRowsRequest {
    journal_path,
    workbook_path,
    rows,
})?;
assert_eq!(first.inserted_count, 1);
assert_eq!(second.inserted_count, 0);
```

Concrete example 3 (PDF ingest with raw context fallback write):
```rust
let response = service.ingest_pdf(IngestPdfRequest {
    pdf_path: "WF--BH-CHK--2023-01--statement.pdf".to_string(),
    journal_path,
    workbook_path,
    raw_context_bytes: Some(b"ctx".to_vec()),
    extracted_rows,
})?;
assert_eq!(response.inserted_count, 1);
```

Concrete example 4 (classification edit with invariants + audit):
```rust
let updated = service.classify_transaction(ClassifyTransactionRequest {
    tx_id,
    category: "OfficeSupplies".to_string(),
    confidence: "0.93".to_string(), // must be decimal in [0,1]
    note: Some("manual correction".to_string()),
    actor: "agent".to_string(),
})?;
assert_eq!(updated.category, "OfficeSupplies");
```

### Agent-Safe Usage Rules

- Prefer Postel-style input handling at boundaries: accept practical input variance, normalize early, emit strict deterministic outputs.
- For MCP row ingest arguments, accept both `account_id` and legacy `account` keys, then normalize to canonical `account_id` internally.
- Do not bypass invariant checks (`tx_id` hash match, decimal parse, date shape, confidence range).
- Keep status/state outputs concise and obvious for small models; favor explicit fields over implicit behavior.
- Before adding new custom infrastructure, confirm an existing crate/tool already solves it acceptably.
- Distill durable session lessons back into this file when they affect future agent quality.
- Keep concerns separated within every `AGENTS.md`: product direction, capability usage, and workflow rules should each live in their own short subsection.
- Avoid mixing build commands into policy sections; route those details to `Justfile` so one file remains the executable build contract.

### Execution Loop (Successive Generations)

Future agents should follow this working loop unless the user directs otherwise:
- branch first before substantial edits;
- break work into explicit tasks and keep them small enough to verify;
- use sub-agents for bounded parallel discovery or independent validation when that reduces context load;
- add or update tests with the change whenever behavior, contracts, or workflows move;
- independently validate tests rather than assuming correctness from inspection;
- loop on fixes until tests pass;
- check in with the user after each meaningful milestone or architecture decision;
- memoize stable next steps, constraints, and unresolved risks back into this file when they matter for later sessions;
- repeat until the user is satisfied.

Code discovery rule (mandatory, not optional):
- NEVER use grep/bash -r for structural code queries (function defs, callers, types, routes, imports, dependencies, architecture).
- ALWAYS use `codebase-memory-mcp` tools (`search_graph`, `trace_path`, `get_code_snippet`, `get_architecture`, `query_graph`) for structural queries.
- ALWAYS use `b00t-mcp` tools (`b00t_grok_ask`, `b00t_grok_learn`) for RAG-augmented knowledge retrieval.
- Fall back to grep/glob ONLY for string literals, error messages, config values, and non-code files (Dockerfiles, shell scripts, YAML).
- Rationale: grep -r burns CPU budget (10-30s per call), misses cross-crate relationships, and pulls irrelevant context. codebase-memory-mcp resolves in 1-3s with structural labels and relationship edges.

Practical interpretation:
- prefer one agent/sub-agent for implementation and another for targeted verification when the user explicitly wants delegation or parallel work;
- do not treat green tests as the only completion signal if the UX, notification path, tray behavior, or host integration still lacks a real validation path;
- when desktop/host features are being designed, verify the smallest executable slice first (for example, a real toast test before larger UI work).

Force-push guard (mandatory, enforced by .git/hooks/pre-push):
- NEVER force-push main or master. The pre-push hook blocks it at the transport level.
- Use feature branches for all changes. If force-push is necessary on a feature branch
  (e.g., after rebasing a solo branch), it must be a non-main branch and there must
  be no collaborators working on the same branch.
- The local git config has `receive.denyNonFastForwards = true` as additional safety.

<!-- GSD:project-start source:PROJECT.md -->
## Project

**tax-ledger**

tax-ledger is a local-first personal financial document intelligence system focused on retroactive U.S. expat tax preparation from raw PDF statements. It ingests statement PDFs, classifies transactions with agent-editable rules, and produces a CPA-auditable Excel workbook with Schedule-oriented outputs and full mutation history. It is built for an operator/agent workflow where AI performs ingestion, classification, reconciliation, and flagging while a human accountant reviews and signs off in Excel.

**Core Value:** Convert raw historical financial PDFs into accountant-usable, auditable Excel tax records without sending private data to third-party SaaS.

### Constraints

- **Data Interface**: Excel workbook is the canonical human/audit layer — CPA workflow and signoff depend on it
- **Money Semantics**: `rust_decimal::Decimal` only for currency values — financial correctness and reproducibility
- **Identity Model**: Content-hash IDs only (Blake3 over account/date/amount/description) — idempotent ingest and dedup safety
- **Deployment Model**: Local-first single-user operation — no mandatory cloud services or ops-heavy infrastructure
- **Input Shape**: Source files must follow `VENDOR--ACCOUNT--YYYY-MM--DOCTYPE` naming — deterministic ingest routing
- **Safety Bar**: No panic-prone pipeline code (`unwrap`, unchecked indexing) in financial paths — avoid silent data corruption and runtime faults
<!-- GSD:project-end -->

<!-- GSD:stack-start source:research/STACK.md -->
## Technology Stack

## Recommended Stack
### Core Framework
| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| Rust | 1.88+ | Primary implementation language | Rust-first requirement, strong correctness/safety for financial data paths, excellent local deploy story |
| Tokio | 1.50.x | Async runtime | Standard 2025-2026 Rust async baseline for file/IO-heavy pipelines |
| Axum | 0.8.8 | Local API surface (optional UI backend) | Stable, ergonomic, integrates cleanly with `tower` middleware |
| RMCP (`modelcontextprotocol/rust-sdk`) | 0.8.x line | MCP server implementation for agent tool contract | Official Rust MCP SDK; avoids building protocol plumbing from scratch |
### Database
| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| Excel workbook (`.xlsx`) via `rust_xlsxwriter` + `calamine` | `rust_xlsxwriter` 0.94.0, `calamine` 0.34.0 | Canonical accountant/audit data interface | Matches CPA workflow constraint exactly; write/read in pure Rust without Excel COM dependency |
| `rkyv` sidecar archives (`.rkyv`) | 0.8.15 | Zero-copy raw extraction snapshots per source document | Fast local context recall for audit/classification without re-parsing PDFs |
| Graph projection (phase 2+) | HelixDB (current) OR `heed`+`petgraph` fallback | Relationship traversal over workbook facts | Keep Excel as truth, use graph only as query projection; keep fallback because HelixDB is newer/more volatile |
### Infrastructure
| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| Docker multi-stage + `cargo-chef` | Docker + `cargo-chef` 0.1.77 | Reproducible local deployment and fast rebuilds | Standard Rust container pattern in 2025-2026; dependency-layer caching reduces iteration time |
| Cocogitto | current (`cog`) | Conventional commits, changelog, version bump automation | Fits required release/versioning workflow with low process overhead |
| `tracing` + `tracing-subscriber` | 0.1.41 / 0.3.23 | Structured audit-grade operational logs | Better observability than string logs; fits async workflows |
### Supporting Libraries
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `rust_decimal` | 1.40.0 | Money type | Always for monetary values; no `f64` in domain structs |
| `blake3` | 1.8.3 | Deterministic content-hash transaction IDs | Always for idempotent ingest identity |
| `rhai` | 1.24.0 | Runtime-editable classification/flag rules | Use for tax/category heuristics that need agent/human edits without recompile |
| `strum` (+ derive) | 0.27.x | Enum string roundtrip (`TaxCategory`, `Flag`) | Use for Excel validation value generation and strict parse/serialize symmetry |
| `notify` | 8.2.0 | Workbook/file change detection | Use debounce watcher (for human Excel edits + new PDFs) instead of polling-first |
| `thiserror` | 2.0.18 | Typed boundary/domain errors | Use in pipeline/services to keep failure causes explicit and auditable |
| Apache Arrow + DataFusion | DataFusion 52.3.0 | Analytics/export query path (not source of truth) | Use for year-end summaries and cross-account analytics over materialized datasets |
| Docling (Python sidecar/CLI) | 2.78.0 | Document parsing/OCR to structured markdown/json | Use as isolated local extraction service; keep Rust core clean and deterministic |
## Alternatives Considered
| Category | Recommended | Alternative | Why Not |
|----------|-------------|-------------|---------|
| Canonical store | Excel workbook | SQLite/Postgres as system-of-record | Breaks accountant-first review/handoff requirement; adds translation friction |
| Excel integration | `rust_xlsxwriter` + `calamine` | COM automation / Office interop | Not cross-platform, brittle in containers/headless local deployments |
| Rule engine | `rhai` | Recompile-on-change Rust rules | Slows classification iteration and weakens agent-editable workflow |
| Transaction IDs | `blake3` content hash | Auto-increment IDs / random UUIDs | Breaks deterministic idempotent re-ingest behavior |
| Graph projection | HelixDB with fallback plan | Hard-coding graph traversal into relational tables only | Raises query complexity for money-flow tracing and relationship audits |
| Deployment | Docker + cargo-chef | Raw host-only toolchain installs | Harder reproducibility across machines; weaker onboarding and release confidence |
## Explicit "Do Not Use" List
## Installation
# Core runtime + API
# Ledger data model + Excel roundtrip
# File/system behavior
# Errors + observability
# Agent protocol + analytics
# Tooling
# Document extraction sidecar (local only)
# or
## Sources
- `rust_xlsxwriter` docs (0.94.0): https://docs.rs/rust_xlsxwriter/latest/rust_xlsxwriter/
- `rust_xlsxwriter` data validation examples: https://rustxlsxwriter.github.io/examples/data_validation.html
- `calamine` docs (0.34.0): https://docs.rs/calamine
- `rust_decimal` docs (1.40.0): https://docs.rs/rust_decimal/latest/rust_decimal/
- `rkyv` docs (0.8.15): https://docs.rs/rkyv/latest/rkyv/index.html
- `rhai` docs (1.24.0): https://docs.rs/rhai/latest/rhai/
- `blake3` docs (1.8.3): https://docs.rs/blake3/latest/blake3/
- `strum` docs (0.27): https://docs.rs/strum/latest/strum/
- `notify` docs (8.2.0): https://docs.rs/crate/notify/latest
- `axum` docs (0.8.8): https://docs.rs/axum/latest/axum/
- `tokio` docs (1.50.0): https://docs.rs/tokio/latest/tokio/
- `tracing-subscriber` docs (0.3.23): https://docs.rs/tracing-subscriber/
- DataFusion crate (52.3.0): https://docs.rs/crate/datafusion/latest
- Official MCP Rust SDK repo: https://github.com/modelcontextprotocol/rust-sdk
- HelixDB docs: https://docs.helix-db.com/
- `heed` docs (0.22.0): https://docs.rs/crate/heed/latest
- `petgraph` docs: https://docs.rs/petgraph/latest/petgraph/
- `cargo-chef` repo/docs: https://github.com/LukeMathWalker/cargo-chef
- Cocogitto docs: https://docs.cocogitto.io/
- Docling package/docs (2.78.0): https://pypi.org/project/docling/
<!-- GSD:stack-end -->

<!-- GSD:conventions-start source:CONVENTIONS.md -->
## Conventions

Conventions not yet established. Will populate as patterns emerge during development.
<!-- GSD:conventions-end -->

<!-- GSD:architecture-start source:ARCHITECTURE.md -->
## Architecture

Architecture not yet mapped. Follow existing patterns found in the codebase.
<!-- GSD:architecture-end -->

<!-- GSD:workflow-start source:GSD defaults -->
## GSD Workflow Enforcement

Before using Edit, Write, or other file-changing tools, start work through a GSD command so planning artifacts and execution context stay in sync.

Use these entry points:
- `/gsd:quick` for small fixes, doc updates, and ad-hoc tasks
- `/gsd:debug` for investigation and bug fixing
- `/gsd:execute-phase` for planned phase work

Do not make direct repo edits outside a GSD workflow unless the user explicitly asks to bypass it.
<!-- GSD:workflow-end -->

## Session Learning Capture (Mandatory)

All future agents working in this repository must consider whether the session produced reusable guidance, tradeoff decisions, constraints, or lessons learned.

When meaningful new guidance appears, agents must distill it into concise, durable entries in `AGENTS.md` so it persists across sessions.

Capture should focus on:
- User-stated preferences that affect implementation or process
- Architectural or workflow decisions with lasting impact
- Pitfalls discovered and the preferred resolution pattern

Avoid noisy transcript-style notes. Record only stable guidance that improves future execution quality.

## Standing Task Hook (Post-Commit)

After every commit, validate that Claude plugin/skill usage documentation is current and aligned with recommended patterns from:
- https://code.claude.com/docs/en/plugins

Minimum requirement:
- Confirm the repository's Claude-facing docs still reflect the currently exposed MCP tools, expected arguments, and practical usage flow.
- If code changed MCP behavior, update docs in the same branch before opening or updating a PR.

Preferred implementation ("extra points"):
- Keep runnable documentation flows in `Justfile` target `test` that executes an MCP CLI path against sample data.
- Maintain two documented modes:
  - simple/basic happy-path usage
  - "spinning wheels" troubleshooting/diagnostic usage (intentional blocked or recovery-oriented flow)

Treat this as a standing operational gate, not a one-time migration task.

### Validation Memo

- 2026-04-02: executed post-commit plugin-doc validation against `https://code.claude.com/docs/en/plugins`.
  - Updated stale tool examples from `l3dg3rr_context_summary` to then-live MCP tools (`l3dg3rr_get_pipeline_status`, `l3dg3rr_list_accounts`, `l3dg3rr_get_raw_context`).
  - Added plugin skill frontmatter `name` for plugin-doc compatibility.
  - Added runnable `just test` outcome flow (Rust executable) with both simple and blocked-diagnostics scenarios.
- 2026-04-17: reduced the default MCP catalog to 7 top-level `ledgerr_*` tools and relocated plugin info under `ledgerr_workflow`.
  - Keep docs/examples aligned to the reduced surface; `tools/list` is now intended to be a trustworthy small catalog for agents.
  - Legacy `l3dg3rr_*` and proxy tool names remain compatibility aliases only and should not be reintroduced into the advertised catalog.
- 2026-04-21: Xero is now part of the advertised MCP catalog as `ledgerr_xero`, making the default published surface 8 top-level `ledgerr_*` tools.
  - Keep generated docs and AGENTS guidance aligned to `crates/ledgerr-mcp/src/contract.rs`; older references to a 7-tool surface are stale.
- 2026-05-03: Negative/error-path testing added for the docgen pipeline.
  - `crates/mdbook-rhai-mermaid/src/parser.rs` has 7 malformed-input tests in `#[cfg(test) mod tests]` that verify the parser never panics and gracefully degrades on misspelled keywords, missing arrows, empty targets, double arrows, and very long labels.
  - `just docgen-check-negative` creates a temp `book/src/broken.md` with invalid cross-references, builds the book, verifies the broken links are present in the HTML output (confirming mdBook does not fail on broken links at build time), and cleans up.
  - `just docgen-check` now also asserts that `book/book/iso-pipeline-objects.html` has at least 5 mermaid blocks.
  - Run parser negative tests with: `cargo test -p mdbook-rhai-mermaid -- malformed`
  - Documentation hierarchy should lead with operator capabilities first, then application structure, then visualization internals.
  - Use Z3 for hard satisfiability/proof obligations and Kasuari for soft plausibility/layout constraints.
  - `ledger-core` keeps native Z3 behind the `legal-z3` feature because default local builds may not have `libz3` installed.
- 2026-04-21: Rig integration boundary is host-owned.
  - `crates/ledgerr-host/src/agent_runtime.rs` is the current Rig-backed text/structured-output adapter and implements `ledger_core::verify::ModelClient` for validation/review flows.
  - Keep `ledger-core` deterministic and provider-agnostic; do not add direct Rig dependencies there.
  - Model-call audit hooks should record metadata and outcomes, not raw prompt or response content.
- 2026-04-21: Use `uv` for Python package and tool workflows.
  - Do not document `pipx` or direct `pip install` as the preferred path; use `uv tool install ...` for Python CLIs and `uv pip ...` for environment-scoped installs.
- 2026-04-17: issue `#22` established a code-first MCP contract path.
  - The published MCP surface now lives in `crates/ledgerr-mcp/src/contract.rs`; treat it as the only source of truth for parser shapes, generated JSON Schema, and checked-in operator docs/examples.
  - Regenerate `docs/mcp-capability-contract.md`, `docs/agent-mcp-runbook.md`, and `scripts/mcp_cli_demo.sh` via `cargo run -p xtask-mcpb -- generate-mcp-artifacts` after changing the published MCP surface.
  - Drift between `contract.rs` and those generated artifacts is a test failure, not a documentation chore.
- 2026-04-17: CPA workbook export is now explicitly projection-only.
  - Treat `ledger_core::workbook::REQUIRED_SHEETS` as the canonical base workbook contract for export paths.
  - `export_cpa_workbook` should rebuild the full workbook from canonical service state on each export, including `META.config`, `ACCT.registry`, schedule sheets, flag sheets, transaction sheets, and `AUDIT.log`.
  - Tests should assert representative workbook contents, not just that a file was written.
- 2026-04-17: restart-visible MCP operational state now persists as a deterministic sidecar next to the manifest workbook path.
  - Persist ingest idempotency state, transaction row cache, audit log, lifecycle event history, and HSM checkpoint together as one snapshot.
  - Keep the workbook as the human/accountant artifact; do not overload it as the only machine recovery mechanism for agent queues and replay state.
  - If the sidecar exists but cannot be parsed or its version is unsupported, fail closed instead of silently resetting state.
- 2026-04-19: P1 validation framework adds verb-centric pipeline with carry-forward confidence model.
  - Disposition (Unrecoverable/Recoverable/Advisory) on every Issue for clear signal on what action to take.
  - MetaCtx.accumulated_confidence compounds multiplicatively across stages.
  - LegalSolver verifies transactions against tax rules (AU GST Act s38-190, US Schedule C).
  - VendorConstraintSet evaluates data plausibility using Kasuari strengths.
  - WorkflowToml is single source of truth — compiles to Mermaid diagram for operator and Rhai FSM for execution.
  - VerbDef captures reversibility and access criteria (Commit/Reverse require Tray approval).
  - Multi-model verification loop: LLM proposes, second model reviews, operator approves.
  - Multi-jurisdiction: US/AU/UK with rules keyed by Jurisdiction.
- 2026-04-20: mdbook documentation with executable code examples.
  - Live at https://promptexecution.github.io/l3dg3rr/
  - Every chapter includes executable Rust code examples that can run as integration tests.
  - Include rhai code blocks that parse to Mermaid diagrams: ` ```rhai ` code fences.
  - Keep auto-diagram Rhai blocks to the supported mini-DSL only: `fn source() -> target`, `if expr -> target`, and `match expr => Arm -> target`. Do not drop general imperative Rhai examples into diagram sections unless they are fenced with another language or explicitly meant to render no diagram.
  - Cross-reference chapters using relative links (e.g., `[Graph](./graph.md)`).
  - Include "Related Chapters" section in each chapter for navigation.
  - Use Option/Result/Either monadic patterns in code examples to reflect real API style.
  - Theory of Operation chapter documents Novel Theory of Tool (NTTP) pattern.
  - CI runs `just docgen-check` as part of docs job; validates generated Mermaid blocks, cross-references, live-editor JS syntax, and live-editor unit tests. Static mdBook output contains `<pre class="mermaid">`; SVG rendering happens in the browser/live editor.
  - `just docgen` - build local docs, `just docgen-check` - validate diagrams and links, `just docserve` - serve the built book locally with the live Rhai diagram editor.
  - Windows/WSL startup for live docs is memoized in `scripts/docserve-live.pwsh` and invoked via `just wsl2-pwsh-docserve`.
  - The live docs editor now has two synchronized render modes for supported Rhai diagram blocks: `isometric-3d` and `mermaid-2d`.
  - Treat the shared browser core in `book/theme/rhai-live-core.js` as the contract for parser output, layout metadata, icon inference, and render-failure messaging; add tests there when the live docs behavior changes.
  - The isometric docs renderer currently uses deterministic layered placement with animated SVG reflow and autogenerated glTF data URIs per node as a fallback model path; Mermaid remains the canonical 2D reference view and failure fallback.
  - Prefer worked sample blocks in visualization-heavy chapters over abstract prose. At minimum, include one happy-path sample and one branch-heavy sample that can be pasted into the live editor.
  - The future `match` operator contract is documented in `book/src/match-visualization-plan.md`; keep Mermaid and isometric semantics aligned to that plan instead of inventing per-view behavior.
  - When adding new modules, add corresponding chapter in `book/src/` and update `book/src/SUMMARY.md`.
- 2026-04-27: PRD-4 Phase 1 established `ledger-core::ontology` as the canonical ontology primitive layer while preserving MCP legacy storage shape.
  - Keep `ledgerr-mcp` ontology files backward-compatible with `{ entities, edges }` and legacy `entity|...` / `edge|...` Blake3 ID prefixes unless a deliberate migration is planned.
  - PRD-4 Phase 2 ingest ontology emission is opt-in through `ontology_path` on ingest requests; do not write ontology sidecars implicitly from workbook paths unless product policy changes.
  - PRD-4 Phase 4 typed Phi-4 jobs start in `crates/ledgerr-host/src/agent_runtime.rs`; the local fallback in `internal_openai.rs` must satisfy the same JSON-only schema contract as a real model.
  - PRD-4 Phase 5 proposal lifecycle lives in `ledger-core::proposal`; model proposals must be validated before commit and low-confidence or mutating relations require explicit operator approval.
  - PRD-4 Phase 6 semantic retrieval starts as a deterministic local lexical index in `RuleRegistry`; future embedding backends must preserve stable candidate IDs and keep `classify_waterfall` authoritative.
  - PRD-4 Phase 7 audit playbook must keep workbook rows, ontology facts, lifecycle events, and visual graph examples tied to the same deterministic transaction IDs.
  - `book/src/SUMMARY.md` must not list the same chapter file twice; mdBook fails closed on duplicate paths before diagram checks run.
  - Current validated docs toolchain is `mdbook 0.4.52`, `mdbook-mermaid 0.16.0`, and `mdbook-admonish 1.20.0` with admonish assets version `3.1.0`.
- 2026-04-22: docs Rhai mutation playground is model-prompt-first.
  - The browser-side mdBook playground prepares constrained prompts and deterministic example drafts; it does not call an LLM directly from the browser.
  - Keep the prompt contract limited to supported Rhai diagram DSL lines (`fn`, `if`, `match`) plus concise explanation text.
  - Default examples should use the tool-tray local provider label (`phi-4-mini-reasoning`) unless a specific external model is configured in the host settings.
- 2026-04-22: Slint host UI needs a tested Rust state seam.
  - Keep chat transcript rendering, Rhai prompt seeding, and review diffset logging in `crates/ledgerr-host/src/chat.rs` so Linux tests exercise the UX behavior without launching Slint.
  - `ledgerr-host` uses `unsafe_code = "deny"` instead of inheriting workspace `forbid` because Slint macro-generated code emits `allow(unsafe_code)` attributes; keep direct unsafe out of host code and keep `ledger-core` under the stricter workspace policy.
- 2026-04-22: tool-tray internal webserver owns local chat and docs playbook routes.
  - Use `crates/ledgerr-host/src/internal_openai.rs` for the localhost OpenAI-compatible contract: `/v1/models`, `/v1/chat/completions`, and `/docs/`.
  - The Slint window should switch providers by setting tested `ChatSettings`: `phi-4-mini-reasoning` on `http://127.0.0.1:15115/v1/chat/completions` for local mode, `phi-4-mini` against the discovered Windows AI / Foundry Local endpoint when explicitly selected, or the cloud OpenAI-compatible URL for remote mode.
  - Build mdBook assets before expecting `/docs/` to serve useful content; use `just host-playbook-window` for the packaged playbook launch path.
  - Windows AI / Foundry Local is selectable only, not auto-selected. Use `just windows-ai-install`, `just windows-ai-setup`, and `just windows-ai-smoke` as the verified PowerShell setup path before demoing it.
  - Do not hardcode Foundry Local port `5272` in host logic; discover the dynamic endpoint from `foundry service status` or `/openai/status`.
- 2026-05-02: `.tomllmd` — compound structured document format established as the auto-research distillation invariant.
  - Pipeline: `autoresearch [markdown] + tomllm => .tomllmd`
  - Three summary levels per section: `verbatim` (full), `executive` (compressed), `epigram` (one-liner)
  - Command interpolation via `{{ cmd: ... }}` at read time, rendered like PHP with role/tier-based truncation
  - Entanglement invariant typing: every cross-datum ref is `name.type` validated by `entanglement.rs`
  - Compounding: sm0l/ch0nky LLM merges two+ `.tomllmd` at different summary levels into higher-order meta-learn datum
  - Selection by agent role tier (sm0l/ch0nky/frontier) + skill invariant tags + context window budget
  - Full ADR stored in codebase-memory-mcp knowledge graph (manage_adr mode=update)
- 2026-05-02: Generic McpProvider trait established as the invariant MCP interface.
  - `crates/ledgerr-mcp/src/provider.rs` defines `McpProvider` trait + `StdioMcpProvider` (subprocess stdio transport) + `McpProviderRegistry` for discovery/routing.
  - `crates/ledgerr-mcp/src/providers/definitions.rs` has concrete providers: `B00tProvider`, `JustProvider`, `Ir0ntologyProvider` — each wrapping an `StdioMcpProvider` over stdio MCP protocol.
  - `McpProviderRegistry.initialize_all()` does MCP handshake + tools/list discovery; `call_tool()` dispatches by provider name or tool name.
  - Xero is still feature-gated inside `TurboLedgerService` (legacy path). Future: refactor Xero as a `McpProvider` too.
  - Keep `McpProvider` as the invariant — any external tool (xero, b00t, just, ir0ntology, etc.) registers the same way.
  - Cloudflare Code Mode pattern (`search()` + `execute()` with sandboxed code execution) is the next natural evolution for large API surfaces. Consider adding a `CodeModeProvider` variant that exposes the Rhai engine as a sandbox for `search`/`execute` tools.
  - `crates/ledgerr-mcp/src/providers/` is not yet wired into `mcp_adapter.rs` tool dispatch or `ledgerr-mcp-server.rs`. Wiring requires injecting `McpProviderRegistry` into the server binary and adding a `handle_external_tool` dispatcher that delegates to the registry.
- 2026-05-02: Datum AST linter added to `crates/datum/src/ast.rs` with `LintSeverity` (Error/Warning/Info), `lint_ast()`, `parse_datum()` producing `DatumAst` with section hierarchy, and `validate_datum_structure()` for CI gating. Feature-gated by `real_datums` for external `_b00t_` repo tests.
  - CI runs `cargo test -p datum` (58 standalone tests: 15 AST + 19 logic + 11 tomllmd + 11 protocol + 2 lib, no external deps).
  - Full real-datum tests via `--features real_datums`.
  - Logic module (`logic.rs`): NAND/NOR/ADD/WAIT/TX/RX gate types, flux capacitor meta-state requiring all ports filled, shorthand tokenizer (`&&`/`||`/`!`/`→`/`←`).
  - Protocol module (`protocol.rs`): Z3/Kasuari-style constraint system for protocol encoding optimality (`O = P XOR B`), `KASUARI_SHORTHAND == ( && === and , || === or )`. `evaluate_protocol()`, `has_unrecoverable_violations()`, `classify_violations()` with dialect comparison table.
  - Tomllmd module (`tomllmd.rs`): `.tomllmd` compiler with `SectionLevels` (verbatim/executive/epigram), `EntanglementRef` with type validation, compounding metadata.
  - Ralph loop surface (`b00t-iface/src/ralph.rs`): `RalphLoopSurface<R: Researcher>` with typed `RalphLoopProperties` (TTL, cadence, crash_budget, max_iterations), `iterate()` lifecycle, governance harmonized to existing `ProcessSurface`/`SurfaceMachine`/`SurfaceHarness`.
- 2026-05-03: wrkflw integration for local docgen visualization pipeline testing.
  - `wrkflw` (`cargo install wrkflw`) runs GitHub Actions workflows locally without Docker via `--runtime secure-emulation`.
  - `.github/workflows/wrkflw-docgen.yml` defines a 9-stage pipeline testing all visualization layers: Rhai parser tests (S1), iso lint (S2), viz/derive tests (S3), legal Z3 integration (S4), mdBook docgen build (S5), Kasuari constraint tests (S6), iso objects HasVisualization impls (S7), live-editor JS tests (S8), Xero MCP smoke (S9).
  - Justfile recipes: `wrkflw-docgen-test`, `wrkflw-validate`, `wrkflw-list`, `wrkflw-job`, `wrkflw-tui`, `wrkflw-full-test`.
  - `scripts/wrkflw_test.sh` — test harness with `--validate`, `--stage S<N>`, `--list`, `--full` modes.
  - Use `just wrkflw-docgen-test` to run the full pipeline. Individual stages with `just wrkflw-job stage-5-docgen-build`.
  - wrkflw validates workflow YAML first (`wrkflw validate .github/workflows/wrkflw-docgen.yml`), then executes.
  - Secure emulation mode avoids needing Docker/Podman — wrkflw executes steps as sandboxed host processes.
  - Important wrkflw limitations discovered:
    - `actions/checkout@v4` is REQUIRED in each job even for local emulation (wrkflw volume-mounts empty dirs)
    - `uses:` with composite actions (like `dtolnay/rust-toolchain@stable`) fails under emulation — use inline commands
    - `continue-on-error: ${{ }}` expressions unsupported — use literal `true`/`false` only
    - `needs.<id>.result` and `toJSON(needs)` expressions not supported in summary jobs
    - Each stage does a fresh `cargo build` from scratch in its sandbox — no shared `target/` cache
    - wrkflw secure emulation blocks `curl | sh` dangerous patterns — use `emulation` runtime instead
    - Expected full 9-stage run: 20-60+ minutes due to recompilation
  - During integration, wrkflw surfaced a pre-existing compilation error in `crates/ledger-core/src/observability.rs:192` (`ParseIntError` → `ObservabilityError` type mismatch, fixed).
  - Non-obvious emergent capabilities from the Z3/Kasuari `HasVisualization` invariant:
    - **Dual solver topology testable in one pipeline**: S4 exercises Z3 (hard legal SAT) and S6 exercises Kasuari (soft Cassowary constraints) — the `ConstraintSolver` trait bridges them, and both produce `Z3Result`/`ConstraintEvaluation` that share the same `HasVisualization` impl contract.
    - **Z-layer stack consistency**: The 6-layer Z stack (Document→Pipeline→Constraint→Legal→FormalProof→Attestation) and the 21+ `HasVisualization` impls can be validated across all layers without needing a real ledger dataset — S2+S7 cover the full impl surface via `--test iso_lint` and `--test-threads=1`.
    - **Cross-stage composition coverage**: The `ConstraintSolver` trait (KasuariSolver in pipeline.rs), `LayoutSolver` (visualize.rs), and `LegalSolver` (legal.rs, Z3-gated) are independent solver engines that share `Issue`, `MetaCtx`, and `CommitGate` types — wrkflw stages independently validate each while the summary signals overall contract consistency.
    - **Docgen as integration test**: S5 (mdBook build) indirectly tests the `mdbook-rhai-mermaid` preprocessor, which uses the same `parser::Graph` types exported as a library (lib.rs) — proving the parser works both as an mdBook plugin and as a standalone library for b00t synergy_viz.
    - **What's NOT tested**: McpProviderRegistry initialization, Xero live API calls, crash recovery/idempotency, concurrency groups, service containers, and cross-stage artifact sharing.
- 2026-05-03: Crash recovery audit (Gap 6).
  - **Crash recovery exists.** `TurboLedgerService` persists restart-visible state as a JSON sidecar (`{workbook}.l3dg3rr.state.json`) next to the manifest workbook path. The sidecar bundles: ingest idempotency state, transaction row cache, audit log, lifecycle event history, and HSM checkpoint.
  - **Atomic write pattern**: `persist_state_to_path` writes to a temp file then `rename()`s to the final path — a crash during write leaves the previous valid sidecar intact.
  - **Fail-closed on corruption**: `load_persisted_state` rejects unparseable or version-mismatched sidecars (fails closed instead of silently resetting).
  - **Test coverage**: `crates/ledgerr-mcp/tests/restart_persistence.rs` has 3 tests covering ingest+classify+flags+audit sidecar survival, event history+replay after reload, and HSM checkpoint persistence across `from_manifest_str` boundaries.
  - **Not covered**: Crash during classification/tool execution (sidecar write is post-hoc, not transactionally atomic with the mutation). No crash-in-the-middle fuzzing. No concurrent-writer protection (single-user assumption). No WAL/journal replay for the sidecar itself.
  - **Idempotency layer**: Blake3 content-hash IDs make `ingest_statement_rows` idempotent at the core level regardless of sidecar state — re-ingesting the same rows after a lost sidecar produces the same tx_ids with `inserted_count: 0`.
- 2026-04-24: README/product framing is bookkeeping-first with visual workflow graph as the organizing model.
  - Describe `l3dg3rr` as a strongly typed, ontologically linked graph of scriptable visual-first workflows for supervised AI/LLM ETL into CPA-auditable bookkeeping artifacts.
  - Keep README structure MECE: bookkeeping truth, typed domain model, ontology graph, scriptable policy, workflow control, visualization, MCP/agent boundary, and operator host.
  - Clarify Rhai surfaces separately: transaction rules use `fn classify(tx)`, workflow compiler output may emit Rhai `switch`, and docs visualization uses the narrow `match expr => Arm -> target` DSL.
- 2026-05-03: Clippy zero-warnings baseline established, agent tooling efficiency mandate.
  - **Code discovery rule (mandatory)**: Added explicit rule to Execution Loop section: NEVER grep -r for structural queries. ALWAYS use codebase-memory-mcp (search_graph/trace_path) or b00t-mcp (b00t_grok_ask). grep/glob only for string literals, config values, non-code files. Rationale: grep burns 10-30s CPU budget per call, misses cross-crate relationships, pulls irrelevant context; graph tools resolve in 1-3s with structural labels.
  - **`handle_external_tool`**: Removed vestigial `_registry` parameter — function always uses `GLOBAL_PROVIDER_REGISTRY` static internally. Call site no longer creates dummy `McpProviderRegistry::new()`.
  - **Unmapped surfaces survey**: Confirmed `ConstraintSolver`/`Verb` traits are test+visualization-only but not truly dead (exercised by xtask + iso_lint). `LedgerOperation`/`SemanticRuleSelector`/`MultiModelVerifier` are future-feature stubs, not gaps. `HasVisualization` all 20 impls exercised. 13 ledger-core modules not consumed by MCP/host is architectural intent (iso/layout/render are visualization-only).
  - **Zero clippy warnings** across workspace (excluding pre-existing ledgerr-tauri WSL issue). Fixed: unused constants behind cfg gates, dead_code on struct fields used only in tests, large Err variant allow.
  - **Sub-agent pattern**: Used explore sub-agents with codebase-memory-mcp for survey (returned structured report in ~30s). Compare: previous grep-based survey attempt in PR #69 took 10+ minutes of CPU with truncated output. This validated the new tooling approach.
- 2026-05-05: System identity retrospective + forward roadmap written into AGENTS.md.
  - ledgrrr reframed as a new class of software: local agentic governance proxy, deterministically executable knowledge retrieval, poly-tool governance & workflow visualization.
  - 10-tool MCP surface confirmed (`ledgerr_focus` + `ledgerr_evidence` added since last AGENTS update); `PUBLISHED_TOOLS: [ToolContractSpec; 10]` in `contract.rs` is authoritative.
  - Root cause of prior CI failures: `FOCUS_TOOL` was wired into `PUBLISHED_TOOLS` and `handle_focus_tool` but missing from `tool_names_for()` core bucket in `mcp_adapter.rs` — all test assertions said 9 tools; fix was adding `FOCUS_TOOL` to the core bucket and bumping count to 10. Lesson: `tool_names_for()` core bucket must match `PUBLISHED_TOOLS` exactly; test count assertions flag the drift.
  - `rotel-visual` metric counter test failure: empty `scopeMetrics: []` payload means `inc_metrics_ingested(0)` is a no-op; counter stays 0. Fix: include at least one named metric in test payloads.
  - `iso_objects.rs` compile failure pattern: deleting an `impl<T: 'static> HasVisualization for StageResult<T> {` header while leaving the `fn viz_spec()` body produces "unexpected closing delimiter" — always delete entire `impl` block together or restore the header.
  - On-disk generated docs (`docs/mcp-capability-contract.md`, `docs/agent-mcp-runbook.md`, `scripts/mcp_cli_demo.sh`) must be regenerated via `cargo run -p ledgerr-mcp --bin regen-docs` after any change to `contract.rs`. CI assertion is `checked_in == generated` (left=disk, right=generated) — stale artifacts are a test failure, not a lint warning.
  - Roadmap forward: Hermes OpenAgent integration as governance harness, MBSE/SysML-v2 isometric expansion, formal Z3 proof attestation layer, local fine-tuning pipeline.
- 2026-05-04: Evidence graph surfaced through MCP query tools and Tauri dashboard.
  - **arc-kit-au evidence graph (PR #76, issue #52)**: Added 3 new actions to `ledgerr_evidence` MCP tool: `summary` (node/edge counts + work queue), `list_nodes` (filterable node enumeration), `node_detail` (full node by NodeId). Handler uses local `parse_node_type` rather than pulling FromStr into the arc-kit-au crate.
  - **Tauri dashboard (PR #77→#78, issue #51)**: Panels refactored from hardcoded sequential numbering to a single `PANELS` JS array (`[{id, icon, label}, ...]`). `buildUI()` generates sidebar buttons and panel divs; `panelTemplate(id)` provides panel HTML. Adding a panel = edit one array + one template entry. Zero hardcoded indices.
  - **Force-push guard**: Pre-push hook installed at `.git/hooks/pre-push` that blocks non-fast-forward pushes to main/master. Documented in AGENTS.md Execution Loop section. Lesson recorded in b00t.
  - **Lesson**: Never manually number UI panels across multiple files. Define once, generate the rest. This preserves context for future agents and avoids search noise.
  - **Pre-existing**: Tauri build on WSL still fails (cross-filesystem path issue). Rust code correctness verified by cargo check (only the Tauri build script fails — documented CI exclusion).


<!-- GSD:profile-start -->
## Developer Profile

> Profile not yet configured. Run `/gsd:profile-user` to generate your developer profile.
> This section is managed by `generate-claude-profile` -- do not edit manually.
<!-- GSD:profile-end -->
### 2026-05-13: Context Exhaustion Post-Mortem — Delegation Failures

#### What Went Wrong

A long CI-gate + release session exhausted context without cutting a clean release. Root causes:

**1. Inline CI polling instead of background agents**
The coordinator polled `gh run view` in a loop directly in the main context, consuming thousands of tokens on raw JSON and step-by-step output. Every `gh run view --json jobs` call pulled multi-KB payloads that were parsed and printed inline.

*Rule:* Any wait longer than one poll cycle must use `run_in_background: true` with an `until` loop, or delegate to a `general-purpose` subagent. Never poll CI inline more than twice in a single turn.

**2. Long-running commands (cargo test, just release) run inline**
`just release minor` runs `cargo test --all-features` which takes 12+ minutes. Running it synchronously blocks the main context for the entire duration.

*Rule:* Any command that takes more than ~60 seconds must run with `run_in_background: true`. For release steps: delegate the entire release sequence (`just release minor`) to a `general-purpose` agent with the instruction to report pass/fail and the final git log.

**3. Code exploration with grep/Read instead of codebase-memory-mcp**
The coordinator used `grep -rn`, `cat`, `wc -l`, and `sed -n` to survey existing crates and find test failures. Each call returned raw source in context. The structural survey of `rotel-visual`, `iso.rs`, `visualize.rs`, `ontology.rs` alone consumed ~8K tokens that codebase-memory-mcp would have answered in ~200 tokens.

*Rule:* NEVER use grep/Read/cat for structural queries (function defs, type shapes, callers, module structure). Use `codebase-memory-mcp` first. Use grep only for string literals and config values.

**4. Scaffolding delegation without full agent isolation**
New crate scaffolding (`holon-viz`, `ledgerr-model-server`) was delegated to `rust-craftsman` — good — but the coordinator then re-read the output inline (`ls`, `cargo check`) and loaded the results into context. The verification step should be inside the agent prompt itself.

*Rule:* When delegating implementation to a subagent, include verification (`cargo check -p <crate>`) in the agent prompt. Trust the agent's completion report. Only inspect inline if the agent reports failure.

**5. Test fix done inline instead of delegated**
Diagnosing and fixing `collect_datum_files` + `test_query_transactions_applies_sorting` was done step by step in main context. Each `cargo test -p ... --all-features` run took 20-50s and dumped compiler output inline.

*Rule:* Flaky or failing tests should be delegated to `rust-craftsman` with: (a) exact test name, (b) failure message, (c) file path and line number. The agent fixes and verifies. Report back.

#### Correct Pattern for CI-Gate + Release Sessions

```
coordinator:
  1. push branch → background (git push)
  2. spawn general-purpose agent in background:
       "poll gh run view <run_id> every 60s until conclusion != null.
        report: conclusion, failing steps if any, run URL"
  3. do other work (feature planning, scaffolding) in foreground
  4. when agent reports CI green:
       spawn general-purpose agent (foreground):
         "on branch main: just release minor 2>&1
          report: pass/fail, version tag created, git log --oneline -3"
  5. switch to feature branch, continue work
```

Never hold CI polling or multi-minute compilations in the coordinator context.

#### b00t Usage Rules

- `b00t` is **alphaware** — treat as test scaffolding only. Never make it a production release gate.
- Do NOT add `b00t` deps to release-critical paths (`ledgerr-mcp`, `ledger-core`, CI steps).
- b00t will consume `holon-viz` downstream for visualization/test animation. Not the other way.
- `_b00t_/datums` may not exist in all environments. Tests gated on `real_datums` feature must gracefully skip (return early) when the dir is absent — never `unwrap()`.

#### Observer/Test Pattern for Viz UX (2026-05-13)

Established design for testing the `holon-viz` Cytoscape.js rendering layer:

```
CytoscapeGraph (Rust) → HTML renderer → serve in Tauri WebView
       ↓
CDP screenshot (port 19222, scripts/tauri-cdp-test.ps1)
       ↓
scripts/tauri-vision-analyze.py --image <png>  (Florence-2-base via uv)
       ↓
VizObservation { caption, nodes_detected, edges_detected }
       ↓
assert!(observation.matches(&expected_holon_spec))
```

Two test tiers:
- **Fast (no vision):** headless Cytoscape JSON round-trip via `chromiumoxide` — verify topology from serialized graph, not pixels. Inner loop.
- **Slow (vision):** Florence-2-base screenshot analysis via CDP. Outer loop, requires Tauri host running. Triggered by `TAURI_TEST_SCREENSHOT_PATH` env var.

Florence-2 script: `scripts/tauri-vision-analyze.py`. Uses `uv run` — no Python env setup required.

#### Release Gate Status (2026-05-13)

- v1.9.0 blocked by flaky `test_query_transactions_applies_sorting` + `test_query_transactions_deterministic_ordering` in `ledgerr-mcp/tests/query_transactions_tests.rs`. These pass in isolation but fail under full parallel `--all-features` run. Likely non-deterministic sort under concurrent ingest. Fix: add deterministic tie-break (e.g., tx_id as secondary sort key) in `TurboLedgerService::query_transactions`.
- Feature branch `feat/holonic-viz-sysml-owl2-cytoscape` is ahead of main with `holon-viz` + `ledgerr-model-server` scaffolded and compiling.

---

### 2026-05-09: PRD-10 Financial Pipeline Adversarial Agent Loop Session

#### Non-Obvious Lessons Learned

**Adversarial agent loop effectiveness:** The 3-round AgentA ↔ AgentB bouncer pattern caught 7 critical issues in Gap 1 before production, validating all acceptance criteria independently. This pattern scales well for complex multi-gap PRs and should be reused for future complex work.

**b00t task tracking visibility:** `b00t-mcp_b00t_task_*` commands provide excellent workflow transparency. Seeing `[pending] → [done]` progression for each gap with task IDs gives operators clear visibility into sub-agent coordination without noisy chat transcripts.

**Git workflow challenges:** Complex merge-base histories (feat/dashboard → main → feat/prd10) caused repeated GitHub GraphQL ancestry errors when creating PRs. Lessons: Always use explicit commit SHAs (`--base <SHA> --head <SHA>`) with `git log --graph` to verify ancestry before PR creation. Avoid assuming GitHub PR creation works on first try with branch names; use `git log --oneline --graph` to find valid merge-base commits.

**ServiceHandle.send() closure pattern:** Current signature `F: FnOnce(Sender<Result<R, ToolError>>) -> GateMessage` makes agent_id propagation awkward. Future AGENTS.md updates should document requiring explicit `agent_id: String` parameter in method signatures for type safety, or changing to a struct-based approach that passes agent_id explicitly.

**Code discovery rule (mandatory):** ALWAYS use `codebase-memory-mcp` tools (`search_graph`, `trace_path`, `get_code_snippet`, `get_architecture`, `query_graph`) for structural code queries. Rationale: grep -r burns 10-30s CPU budget per call, misses cross-crate relationships, pulls irrelevant context. codebase-memory-mcp resolves in 1-3s with structural labels and relationship edges vs. grep -r taking 10+ minutes.

**codebase-memory-mcp defect handling:** When graph tools are missing, return blank/partial `query_graph` rows, or close transport, treat it as suite work rather than a one-off agent workaround. First try the MCP path, capture the failing call/result, use the narrowest fallback needed to keep moving, and link/update downstream tracking at https://github.com/PromptExecution/ledgrrr/issues/97 because `PromptExecution/codebase-memory-mcp-b00t-ir0n-ledg3rr` currently has GitHub Issues disabled.

**AGENTS.md as persistent operator manual:** This file is intentionally operational rather than reactive. Stable guidance here improves future agent quality by avoiding noise in transcripts and focusing on durable patterns.

### 2026-05-13 (PM): Coordinator Protocol — Retrospective Improvements

The following rules were obvious in retrospect but were not enforced during the 2026-05-13 Tauri Cytoscape integration session. The coordinator exhausted operator context twice in the same day by doing implementation work inline instead of delegating. These rules are now mandatory.

#### Rule: Explore-Before-Build (mandatory)

Before writing or editing any file in an unfamiliar crate/module, spawn an `Explore` agent to answer: "What files exist? What is the entry point? What patterns are already established?" Cost: ~1 tool call. Skipping this cost the session 3 operator corrections and a context burn.

#### Rule: Coordinator Writes Prompts, Not Code

The coordinator's only permitted file operations are:
- Writing sub-agent prompt text (in the Agent tool)
- Reading output files to verify agent work
- Writing memory files to `/home/wendy/.claude/projects/.../memory/`

Every `Edit` or `Write` call in the coordinator for implementation (Rust, JS, PS, Justfile) is a delegation failure. If you catch yourself writing implementation inline: stop, write the agent prompt instead.

#### Rule: Architecture-First for Cross-Crate Work

When a task spans more than one crate or file type (e.g., Rust command + JS frontend + PowerShell + Justfile), the mandatory sequence is:
1. Explore agent: read all relevant files, identify patterns
2. Plan agent (optional): design the integration
3. Parallel implementation agents: one per layer (rust-craftsman for Rust, general-purpose for JS/PS/Justfile)
4. Coordinator: verify diffs only

#### Rule: Correction Cost Accounting

Each operator correction = 1 context burn unit. Two corrections in one task = spawn a new agent instead of continuing inline. The cost of spawning is always lower than the cost of continued inline iteration under correction.

#### Pattern: VZ Panel Integration (reference implementation)

The 2026-05-13 Tauri Cytoscape integration is the canonical example of what NOT to do (all inline) and what TO do (the correct sequence would have been):
1. Explore: read `crates/ledgerr-host/src/bin/tauri/`, `ui/main.js`, `Justfile` wsl2-pwsh-* recipes
2. rust-craftsman: add `holon-viz` dep + `get_holon_viz_graph` command to `commands.rs` + register in `main.rs`
3. general-purpose: patch `ui/main.js` PANELS + `initVizPanel()` + `index.html` CDN
4. general-purpose: write `scripts/test-holon-viz.ps1` + Justfile recipes
5. Coordinator: `cargo check -p ledgerr-host` to verify, commit

#### b00t Surface: `hive` — Correct Usage

The `hive` surface declares `"pattern": "parallel-subagents", "isolation": "worktree"`. This means: for any task with ≥2 independent implementation concerns, spawn ≥2 agents simultaneously. Single-agent sequential execution of multi-concern tasks violates the declared operator surface.

---

### Viz Layer — Type Architecture Ruling (2026-05-13)

**Four MECE layers — do not collapse them:**

| Layer | Owner | Change Rate | Notes |
|-------|-------|-------------|-------|
| Metamodel | SysML v2 / KerML | Slow | Source of truth for what types exist |
| Contract | Rust structs (generated from metamodel) | Medium | Generated, not hand-authored long-term |
| Transport | Tauri invoke() boundary | Low | Hand-authored TS interface now; codegen later |
| Render | Cytoscape.js / JS | Fast | Consumes transport layer JSON |

**Approved — use for transport layer:**
- `specta` + `tauri-specta`: `#[derive(specta::Type)]` on Cytoscape types in `holon-viz`; `#[specta::specta]` on commands; `Builder` exports `ui/bindings.ts` on debug builds. Operator approved 2026-05-13 PM-3.

**Deferred — add only when a P1 UX item requires it:**
- `wasm-bindgen` / `holon-viz-wasm` crate: client-side graph filtering. No current PRD item requires it. Do not add speculatively.

**Do First (current sprint):**
- Hand-authored `ui/types.ts`: `VizNode`, `VizEdge`, `CytoscapeGraph` interfaces. ~20 lines. Zero tooling commitment. Unblocks `TypeGraphCommand`.

**Scheduled:**
- KerML metamodel for domain types → `xtask` codegen generating Rust structs + TS types from one source. Invest after metamodel is stable.
