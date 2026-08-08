# tax-ledger

## What This Is

tax-ledger is a local-first personal financial document intelligence system focused on retroactive U.S. expat tax preparation from raw PDF statements. It ingests statement PDFs, classifies transactions with agent-editable rules, and produces a CPA-auditable Excel workbook with Schedule-oriented outputs and full mutation history. It is built for an operator/agent workflow where AI performs ingestion, classification, reconciliation, and flagging while a human accountant reviews and signs off in Excel.

## Core Value

Convert raw historical financial PDFs into accountant-usable, auditable Excel tax records without sending private data to third-party SaaS.

## Current Milestone: v1.3 Windows Distribution & Winget Packaging

**Goal:** Ship an installable, unsigned ledgrrr desktop package via winget.

**Target features:**
- Public, hash-verified GitHub Release built from the existing MSI/NSIS CI pipeline.
- Release automation: version tag → build → SHA256 → GitHub Release (unsigned).
- winget-pkgs manifest (installer/locale/version) and submission path to microsoft/winget-pkgs.

**Parked:** v1.2 Claude Connector Interop was roadmapped (phases 19-21) but never executed (0 phase directories created). It remains parked, not shipped or abandoned — resume later via `/gsd-plan-phase 19`. Its Active requirements below are unchanged and still belong to that parked milestone.

## Requirements

### Validated

- ✓ Contract-first workbook/session bootstrap and filename preflight — v1.0
- ✓ Deterministic ingest with source-evidence retrieval — v1.0
- ✓ Runtime Rhai classification and review queue workflows — v1.0
- ✓ Append-only mutation audit and reconciliation pathways — v1.0
- ✓ CPA workbook outputs and Schedule/FBAR summaries — v1.0
- ✓ MCP operator surface for ingest/classify/query/summary/context — v1.0
- ✓ Local containerized run path with CI/release automation — v1.0
- ✓ Enforceable MCP transport boundary with passthrough/proxy abstractions for Docling and Rustledger surfaces — Phase 13 (v1.1)

### Active

- [ ] Add Claude connector compatibility profile for l3dg3rr MCP tools, including explicit capability and scope metadata.
- [ ] Provide deterministic install/use paths for Claude, Cowork, Desktop, and remote MCP connector flows.
- [ ] Add connector-focused transport tests that prove tool discovery/call behavior under connector session constraints.
- [ ] Encode connector safety rails around read/write permission scope and operator-visible diagnostics.

### Out of Scope

- Cloud-first hosted accounting platform — local-first privacy and operator-controlled storage are required
- Replacing Excel as the primary accountant interface — CPA workflow requires workbook handoff and review in Excel
- General bookkeeping suite parity (e.g., full QuickBooks replacement) — focus is compliance-oriented document intelligence and auditability
- Automated PDF download from institutions — ingest assumes files already exist on disk with required naming convention

## Context

Primary use case remains catching up unfiled U.S. expat returns with CPA handoff constraints. v1.0 shipped the end-to-end local-first MVP: deterministic ingest, runtime classification, audit-safe mutation history, workbook outputs, schedule summaries, and CI/release readiness. Next evolution focuses on v2 graph/API/UI extension work while preserving workbook-as-truth and deterministic financial behavior.

Current state: v1.1 FDKMS closure (phases 13-18) is complete and archived. The next milestone focuses on connector interoperability and operator adoption on Claude/Cowork surfaces, not core financial-domain expansion.

## Constraints

- **Data Interface**: Excel workbook is the canonical human/audit layer — CPA workflow and signoff depend on it
- **Money Semantics**: `rust_decimal::Decimal` only for currency values — financial correctness and reproducibility
- **Identity Model**: Content-hash IDs only (Blake3 over account/date/amount/description) — idempotent ingest and dedup safety
- **Deployment Model**: Local-first single-user operation — no mandatory cloud services or ops-heavy infrastructure
- **Input Shape**: Source files must follow `VENDOR--ACCOUNT--YYYY-MM--DOCTYPE` naming — deterministic ingest routing
- **Safety Bar**: No panic-prone pipeline code (`unwrap`, unchecked indexing) in financial paths — avoid silent data corruption and runtime faults

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Excel is the primary audit and handoff interface | Matches CPA workflows and practical auditability expectations | ✓ Adopted in v1.0 |
| `rust_xlsxwriter` + `calamine` for workbook roundtrip | Strong write features plus pure-Rust cross-platform read path | ✓ Adopted in v1.0 |
| `rkyv` snapshot sidecars for parsed document context | Fast, bounded-context retrieval for agent verification workflows | ✓ Adopted in v1.0 |
| Rhai scripts for classification/flag logic | Runtime-editable, diffable rules without recompilation | ✓ Adopted in v1.0 |
| Rustledger-compatible plain-text journal as ingest persistence layer | Maximizes Git-native diffability and plain-text accounting interoperability | ✓ Adopted in v1.0 |
| HelixDB is projection over workbook truth | Relationship queries without displacing accountant-facing source of truth | — Pending v2 |
| Local-first architecture (no Postgres/cloud dependency) | Privacy and low-ops constraints from use case | ✓ Adopted in v1.0 |
| MCP wrapper is first-class integration contract | Agent operation depends on stable callable tool semantics | ✓ Adopted in v1.0 |
| Upstream MCP passthrough over reinvention | Reuse proven rustledger/docling interfaces and layer l3dg3rr abstractions above them | ✓ Adopted in Phase 13 |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `$gsd-transition`):
1. Requirements invalidated? -> Move to Out of Scope with reason
2. Requirements validated? -> Move to Validated with phase reference
3. New requirements emerged? -> Add to Active
4. Decisions to log? -> Add to Key Decisions
5. "What This Is" still accurate? -> Update if drifted

**After each milestone** (via `$gsd-complete-milestone`):
1. Full review of all sections
2. Core Value check -> still the right priority?
3. Audit Out of Scope -> reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-08-08 for v1.3 milestone kickoff*
