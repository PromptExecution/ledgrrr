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

### Required developer tools

```sh
# Rust toolchain
rustup update stable

# Conventional commits, changelog, and version bump automation
cargo install cocogitto

# Workspace version management — required by cog pre_bump_hooks
# (cargo set-version updates Cargo.toml on every cog bump)
cargo install cargo-edit

# MCP bundle + registry publish automation
cargo install --path xtask
```

### Releasing a new version

```sh
cog bump --auto          # calculates next semver from commits, runs pre_bump_hooks
                         # (which calls cargo set-version), updates CHANGELOG.md,
                         # creates bump commit + vX.Y.Z tag in one shot
git push --follow-tags
```
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



<!-- GSD:profile-start -->
## Developer Profile

> This section is managed by `generate-claude-profile` -- do not edit manually.

| Dimension | Value |
|-----------|-------|
| **Role** | Orchestrator/director — directs agents, reviews output, makes final calls. Does not implement. |
| **Domain expertise** | Rust/systems, AI/agent systems |
| **Relies on agents for** | Tax/accounting domain details, TypeScript/JS, DevOps specifics |
| **Top frustrations** | Under-delegation (coordinator does inline implementation) and context exhaustion (both equally) |
| **Review style** | Narrative summary — explain what changed and why; does not need raw diff narration |
| **Session style** | Marathon — push through until a phase is complete |
| **Definition of done** | Demo'd live in Tauri app (CDP test passing), not just green tests + commit |
<!-- GSD:profile-end -->
