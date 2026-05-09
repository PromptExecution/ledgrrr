# PRD-10: Financial Pipeline — Ingestion, Workbook Write, and AGT Governance Wiring

**Status:** Draft | **Priority:** P1 (Core Value Delivery) | **Date:** 2026-05-09
**Depends on:** PRD-6 (type system), `msft-agent-gov-ledgrrr` (all 12 gaps closed, merged)

---

## 1. Problem Statement

The governance layer (`msft-agent-gov-ledgrrr`) is complete and merged. The Rhai classification waterfall (`RuleRegistry::classify_waterfall`) is implemented. Neither is connected to anything that moves real data.

The three load-bearing gaps preventing end-to-end pipeline operation:

| Gap | Location | State |
|---|---|---|
| PDF ingestion from disk to classified transactions | `ledger-core/src/ledger_ops.rs` `PdfIngestOp` | Phase 2 stub, `NotImplemented` |
| Transaction rows written to Excel workbook | `ledger-core/src/workbook.rs` | 71 lines, sheet init only |
| AGT governance gating MCP tool dispatch | `ledgerr-mcp/src/gate.rs` | Zero references to `LedgrrAgtGateway` |

Secondary gaps that must land in the same phase to avoid regression:

| Gap | Location | State |
|---|---|---|
| OpaGateOp pre-commit policy gate | `ledger-core/src/ledger_ops.rs` `OpaGateOp` | Phase 3 stub; replace with Cedar/AGT |
| Rhai rule hot-reload on file change | `ledger-core/src/rule_registry.rs` | `notify` crate present, not wired |
| Semantic rule selector | `ledger-core/src/rule_registry.rs` `SemanticRuleSelector` | Trait defined, impl panics — fallback to deterministic |

---

## 2. Scope

### In scope

- `PdfIngestOp`: spawn Docling/reqif-opa-mcp subprocess, parse NDJSON output, dedup via Blake3, drive `classify_waterfall`, hand off to workbook write
- `WorkbookWriter`: `ClassificationOutcome` → schedule sheet row, mutation history append, data validation dropdowns from `TaxCategory` strum enum
- `LedgrrAgtGateway` wired into `ledgerr-mcp/gate.rs`: ring-gate each tool call before dispatch
- `OpaGateOp` replaced with Cedar/AGT `ComplianceEngine::check_policy` (feature-gated `cedar-policy`)
- `notify` watcher: hot-reload `RuleRegistry` on `.rhai` rule file changes; trigger ingest agent on new PDFs in watched directory

### Out of scope

- Semantic rule selector (embedding infrastructure: `candle`/`fastembed-rs`/ONNX) — deterministic waterfall is sufficient for Phase 2
- HelixDB graph projection — remains Phase 3+
- DataFusion analytics layer — remains Phase 3+
- UI changes beyond surfacing pipeline status in existing Tauri host

---

## 3. Architecture

### 3.1 Data flow (end-to-end)

```
PDF file (VENDOR--ACCOUNT--YYYY-MM--DOCTYPE naming)
  │
  ▼  [notify watcher or operator-triggered MCP tool]
PdfIngestOp
  │  spawn: reqif-opa-mcp ingest --file <path> --output ndjson
  │  read NDJSON → Vec<ReqIfCandidate>
  │  Blake3 content-hash dedup (skip known tx_ids)
  ▼
RuleRegistry::classify_waterfall(engine, &tx) → ClassificationOutcome
  │
  ▼  [Cedar/AGT gate — replaces OpaGateOp stub]
ComplianceEngine::check_policy(&outcome) → allow | flag
  │
  ├─ allow → WorkbookWriter::append_row(schedule_sheet, &row)
  │            + mutation_history::append(tx_id, timestamp, agent_id, action)
  │
  └─ flag  → WorkbookWriter::append_flag(flags_sheet, &row, reason)
```

### 3.2 AGT governance layer in MCP dispatch

```
MCP tool call arrives at ledgerr-mcp/gate.rs
  │
  ▼
LedgrrAgtGateway::check_tool_call(agent_id, tool_name, input_json)
  │  PolicyDecision::Allow → proceed
  │  PolicyDecision::Deny(reason) → return ToolError::PolicyDenied
  │  PolicyDecision::RequiresApproval → queue for operator review
  │  PolicyDecision::RateLimited{retry_after_secs} → return ToolError::RateLimited
  ▼
existing gate.rs dispatch (unchanged)
```

Ring enforcement per tool:

| Tool | Minimum ring |
|---|---|
| `ingest_pdf` | Standard |
| `classify_transaction` | Standard |
| `edit_rhai_rule` | Admin |
| `commit_workbook` | Standard |
| `promote_agent` | Admin |

---

## 4. Component Specifications

### 4.1 `PdfIngestOp` — `ledger-core/src/ledger_ops.rs`

**Trigger:** Called by the MCP `ingest_pdf` tool handler after `check_tool_call` returns `Allow`.

**Inputs:**
- `input_path: PathBuf` — validated against `VENDOR--ACCOUNT--YYYY-MM--DOCTYPE` filename pattern (existing `filename.rs`)
- `rule_dir: PathBuf` — directory of `.rhai` rule files; loaded into `RuleRegistry`
- `workbook_path: PathBuf` — target `.xlsx`; created if absent

**Subprocess contract:**
```
reqif-opa-mcp ingest --file <input_path> --output ndjson
```
- One JSON object per stdout line; each line deserializes as `ReqIfCandidate`
- Non-zero exit → `LedgerOpError::ExternalProcessFailed { exit_code, stderr }`
- Subprocess timeout: 120 seconds; kill and return `LedgerOpError::Timeout` if exceeded

**Dedup:** For each `ReqIfCandidate`, compute `blake3::hash(account_id + date + amount + description)` as `tx_id`. Check against `rkyv` sidecar (`<workbook_path>.rkyv`); skip if present.

**Classification:** `RuleRegistry::classify_waterfall(engine, &tx)` using `select_rules_deterministic`. Store outcome + `tx_id` in `rkyv` sidecar before workbook write (write-ahead for crash recovery).

**Error handling:** Per-row errors are collected and returned as `Vec<IngestRowError>` — partial ingest succeeds; caller decides whether to surface failures as warnings or abort.

**Idempotency:** Re-running on the same PDF with the same rules must produce identical workbook rows. Blake3 dedup + rkyv sidecar enforce this.

### 4.2 `WorkbookWriter` — `ledger-core/src/workbook.rs`

Replace the 71-line skeleton with a write-capable struct. The existing `initialize_workbook` is kept; the writer wraps an open `calamine`-read / `rust_xlsxwriter`-write session.

**Sheet layout:**

| Sheet | Content |
|---|---|
| `TRANSACTIONS` | One row per classified transaction: `tx_id`, `date`, `vendor`, `account`, `amount` (Decimal formatted), `category`, `confidence`, `needs_review`, `flag` |
| `FLAGS.open` | Transactions flagged for review: all TRANSACTIONS columns + `flag_reason`, `flagged_by` |
| `FLAGS.resolved` | Resolved flags: above + `resolved_by`, `resolved_at`, `resolution_note` |
| `MUTATION_HISTORY` | Append-only audit log: `timestamp`, `tx_id`, `agent_id`, `ring`, `action`, `before`, `after` |
| `SCHEDULES.*` | One sheet per tax schedule (B, C, D, SE…); rows are aggregations from TRANSACTIONS filtered by `TaxCategory` |

**Data validation:** `TaxCategory` strum enum → `strum::VariantNames::VARIANTS` → Excel dropdown on `category` column. Same for `Flag` enum on `flag` column.

**Append semantics:** `append_row` and `append_flag` use `calamine` to read existing row count, then `rust_xlsxwriter` to write the next row. No full-file rewrite on each append.

**`Decimal` formatting:** Written as string `"1234.56"` via `Decimal::to_string()` — never `f64`. Column format set to `@` (text) to prevent Excel from converting.

### 4.3 AGT wiring — `ledgerr-mcp/src/gate.rs` + `Cargo.toml`

**Dependency:** Add `msft-agent-gov-ledgrrr = { path = "../msft-agent-gov-ledgrrr" }` to `ledgerr-mcp/Cargo.toml`.

**Gateway init:** `LedgrrAgtGateway::with_persist_path` called once at MCP server startup; `Arc<LedgrrAgtGateway>` threaded through the actor/gate state.

**Dispatch wrapper:** Before each tool handler call in `gate.rs`, call:
```rust
gw.check_tool_call(agent_id, tool_name, &input_json)?;
```
Map `PolicyDecision::Deny` → `ToolError::PolicyDenied(reason)`, `RateLimited` → `ToolError::RateLimited { retry_after_secs }`.

**`arc-kit-au` provenance:** After each successful tool dispatch, emit a provenance edge:
```rust
arc_kit_au::trace(tx_id, source_doc, tool_name, agent_id, ring);
```

### 4.4 Cedar/AGT gate replacing `OpaGateOp` — `ledger-core/src/ledger_ops.rs`

Remove `OpaGateOp`; replace call sites with:
```rust
gw.attest_z3_proof(tx_id, &outcome.category, outcome.confidence)?;
```
`ComplianceGrade::Full` → proceed to workbook write.
`ComplianceGrade::Partial` → write to `FLAGS.open` with `reason = "compliance_partial"`.

Feature-gated: `#[cfg(feature = "cedar-policy")]`. Without the feature, gate is a no-op (existing behavior).

### 4.5 `notify` watcher — new `ledger-core/src/watcher.rs`

```rust
pub struct PipelineWatcher {
    pub rule_dir: PathBuf,
    pub ingest_dir: PathBuf,
    pub registry: Arc<RwLock<RuleRegistry>>,
    pub ingest_tx: tokio::sync::mpsc::Sender<PathBuf>,
}

impl PipelineWatcher {
    pub fn spawn(self) -> notify::RecommendedWatcher;
}
```

- `.rhai` file change in `rule_dir` → debounce 500ms → `RuleRegistry::load_from_dir` reload
- New `.pdf` in `ingest_dir` (create event only) → send path to `ingest_tx` channel → `PdfIngestOp`
- Debounce: `notify::event::ModifyKind::Data` only; ignore renames and metadata changes

---

## 5. File Change Map

| File | Change |
|---|---|
| `crates/ledger-core/src/ledger_ops.rs` | Implement `PdfIngestOp::execute`; remove `OpaGateOp` (replace with Cedar/AGT call) |
| `crates/ledger-core/src/workbook.rs` | Replace skeleton with `WorkbookWriter` struct + `append_row`, `append_flag`, `append_mutation` |
| `crates/ledger-core/src/watcher.rs` | New file: `PipelineWatcher` |
| `crates/ledger-core/src/lib.rs` | `pub mod watcher` |
| `crates/ledgerr-mcp/Cargo.toml` | Add `msft-agent-gov-ledgrrr` dependency |
| `crates/ledgerr-mcp/src/gate.rs` | Add `Arc<LedgrrAgtGateway>` field; wrap each dispatch with `check_tool_call` |
| `crates/ledgerr-mcp/src/mcp_adapter.rs` | Thread gateway into gate actor init; resolve existing `TODO` at line 140 |

---

## 6. Acceptance Criteria

### `PdfIngestOp`
- [ ] Given a fixture PDF with known transactions, `execute` produces `ClassificationOutcome` rows matching expected categories
- [ ] Re-running on the same PDF produces zero new rows (Blake3 dedup)
- [ ] Subprocess non-zero exit returns `LedgerOpError::ExternalProcessFailed` with captured stderr
- [ ] Subprocess timeout after 120s returns `LedgerOpError::Timeout`

### `WorkbookWriter`
- [ ] `append_row` writes a `ClassificationOutcome` row to `TRANSACTIONS` sheet; re-opening the file with `calamine` reads the same data
- [ ] `amount` column contains string value `"1234.56"` — not a float
- [ ] `category` column has Excel data validation dropdown matching `TaxCategory::VARIANTS`
- [ ] `MUTATION_HISTORY` is append-only: calling `append_row` twice results in two history rows, not one overwrite
- [ ] `initialize_workbook` followed by two `append_row` calls produces a valid `.xlsx` with all required sheets

### AGT governance wiring
- [ ] MCP tool call from a `Sandboxed` ring agent attempting `ingest_pdf` returns `ToolError::PolicyDenied`
- [ ] MCP tool call from a `Standard` ring agent attempting `ingest_pdf` proceeds to handler
- [ ] MCP tool call from a `Standard` ring agent attempting `edit_rhai_rule` returns `ToolError::PolicyDenied`
- [ ] MCP tool call from an `Admin` ring agent attempting `edit_rhai_rule` proceeds to handler
- [ ] `arc-kit-au` provenance edge emitted with correct `tx_id`, `agent_id`, `ring` after successful dispatch

### Cedar/AGT gate (feature = `cedar-policy`)
- [ ] Transaction with `ComplianceGrade::Full` routes to `TRANSACTIONS` sheet
- [ ] Transaction with `ComplianceGrade::Partial` routes to `FLAGS.open` with `reason = "compliance_partial"`
- [ ] Without `cedar-policy` feature, gate is a no-op — existing test suite passes unchanged

### `notify` watcher
- [ ] Modifying a `.rhai` file in `rule_dir` causes `RuleRegistry` to reload within 600ms
- [ ] Dropping a new `.pdf` into `ingest_dir` sends path on `ingest_tx` within 600ms
- [ ] Watcher does not trigger on metadata-only changes (touch without write)

---

## 7. Dependencies and Risks

| Item | Risk | Mitigation |
|---|---|---|
| `reqif-opa-mcp` subprocess availability | Not installed → `PdfIngestOp` fails at runtime | Return `LedgerOpError::SubprocessNotFound` with install hint; document in AGENTS.md |
| `calamine` + `rust_xlsxwriter` on same file | Both open the same `.xlsx` simultaneously | Use read-then-write pattern: `calamine` reads row count, drops handle, `rust_xlsxwriter` appends |
| `notify` on WSL2 | `inotify` events from Windows-side file drops may be delayed or missed | Document known WSL2 limitation; add a manual `poll` fallback trigger via MCP tool `poll_ingest_dir` |
| Cedar policy authoring | Policies must be written before the gate has any effect | Ship a `default.cedar` policy file that mirrors the existing YAML policy semantics |
| `arc-kit-au` API stability | Provenance crate is in-repo but may lack stable public API | Wrap in a thin `ledger_core::provenance` adapter so gate.rs doesn't couple directly |
