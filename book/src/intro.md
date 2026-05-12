# Introduction

<div class="admonition info">
<div class="admonition-title">Info</div>
<p><strong>l3dg3rr</strong> is designed for US expats who need to reconcile complex financial histories across multiple jurisdictions (US, AU, UK) without compromising privacy.</p>
</div>

`l3dg3rr` is a local-first financial document intelligence system
 for retroactive U.S. expat tax preparation. It ingests raw statements, classifies transactions with editable rules, verifies hard constraints, and exports an accountant-usable Excel workbook with audit history.

The system is built around an operator/agent workflow: agents do ingestion, classification, reconciliation, flagging, and evidence gathering; the human operator and CPA keep approval authority through Excel, notifications, and auditable review surfaces.

## Core Functional Shape

```rhai
fn source_documents() -> document_ingestion
fn document_ingestion() -> validation
fn validation() -> classification
fn classification() -> legal_verification
fn legal_verification() -> reconciliation
fn reconciliation() -> workbook_export
fn workbook_export() -> cpa_review
fn cpa_review() -> audit_history
```

## Product Guarantees

- **Local-first operation**: private financial data does not require third-party SaaS processing.
- **Excel-first audit layer**: the workbook is the CPA-facing review and signoff artifact.
- **Deterministic identity**: transaction IDs are content hashes, not random UUIDs.
- **Decimal money semantics**: currency values stay in `rust_decimal::Decimal` in financial paths.
- **Agent-visible but operator-governed tools**: MCP exposes capability families while l3dg3rr owns policy, audit, approvals, and credentials.

## How To Read This Book

Use [Capability Map](./capability-map.md) for the current implementation state. Then read the operator capability chapters for what the application does, followed by the application structure chapters for how it behaves internally.

The visualization chapters document the live mdBook diagram system. They are important, but they are no longer the top-level architecture of the whole application.

## Primary Surfaces

| Surface | Audience | Purpose |
|---|---|---|
| Excel workbook | CPA/operator | Review, correction, schedule summaries, audit signoff |
| MCP tools | agents | Controlled capability execution through `ledgerr_*` tool families |
| Sidecar state | host/service | Restart recovery, replay, idempotency cache, lifecycle state |
| Desktop host | operator | approvals, notifications, credentials, process supervision |
| mdBook docs | developers/operators | executable diagrams and technical behavior reference |

## Related Chapters

- [Capability Map](./capability-map.md)
- [MCP Surface](./mcp-surface.md)
- [Workbook & Audit](./workbook-audit.md)
- [Theory of Operation](./theory.md)
- [Graph Data Model](./graph.md)
