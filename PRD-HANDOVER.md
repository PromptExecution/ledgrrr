# Operator Handover — l3dg3rr / ledgrrr

**Date:** 2026-05-13  
**From:** Claude Sonnet 4.6 (coordinator)  
**Branch state:** `main` is clean and ahead of last release. Feature branch `feat/holonic-viz-sysml-owl2-cytoscape` is open.

---

## 1. What This Project Is

`l3dg3rr` is a **local-first agentic governance proxy** for supervised AI workflows. Its immediate application domain is retroactive U.S. expat tax preparation from raw PDF statements, but its governance, audit, and visualization layers are general-purpose.

**The system does three things:**
1. Ingests financial PDFs → classifies transactions → writes a CPA-auditable Excel workbook
2. Mediates every AI tool call through a policy gate (AGT/Cedar) with an append-only audit log
3. Visualizes the entire pipeline — data state, governance decisions, formal proofs — in a deterministic isometric/graph rendering layer

**Non-negotiables (from CLAUDE.md):**
- `rust_decimal::Decimal` only for money — no `f64`
- Blake3 content-hash IDs everywhere — idempotent ingest, dedup safety
- No `unwrap()` / unchecked indexing in financial or pipeline paths
- Excel workbook is the canonical human/audit interface — CPA signs off there
- Local-first — no mandatory cloud services

---

## 2. Current State (as of 2026-05-13)

### What is production-ready on `main`

| Capability | Crate | Status |
|---|---|---|
| MCP server with 10 governed tools | `ledgerr-mcp` | ✅ Shipping |
| Transaction ingest + classify (rule waterfall) | `ledger-core` | ✅ |
| Excel workbook write + mutation history | `ledger-core/workbook.rs` | ✅ |
| Arc-kit-au evidence graph (Blake3 nodes) | `arc-kit-au` | ✅ |
| Rhai runtime rule engine | `ledger-core` | ✅ |
| Legal Z3 solver (`legal-z3` feature) | `ledger-core/legal.rs` | ✅ |
| Kani formal verification harnesses | `kani-proofs/` | ✅ CI runs on push |
| Isometric 3D visualization (`HasVisualization` trait) | `ledger-core/iso.rs` | ✅ |
| Mermaid 2D visualization | `ledger-core/visualize.rs` | ✅ |
| OTLP telemetry + web dashboard | `rotel-visual` | ✅ |
| Tauri desktop host + tray + notifications | `ledgerr-host` | ✅ |
| mdBook documentation site | `book/` | ✅ CI-validated |
| McpProvider trait + b00t/just/ir0ntology providers | `ledgerr-mcp-core` | ✅ |
| ledger-attest proc-macro skeleton | `ledger-attest` | ⚠️ Lint skeleton only |

### What is scaffolded but not implemented

| Crate | State | Next step |
|---|---|---|
| `holon-viz` | Types compile, no rendering logic wired | See §4 |
| `ledgerr-model-server` | Config + stub start() only | See §4 |
| Docling PDF bridge (`PdfIngestOp`) | Stub, `NotImplemented` | Issue #60 |
| Semantic rule selector (`SemanticRuleSelector`) | Trait defined, impl panics — waterfall fallback active | PRD-10 |

### Release state

| Tag | GitHub Release | Notes |
|---|---|---|
| `v1.8.0` | ✅ Published (latest) | Last stable even minor |
| `v1.8.1` | ❌ Tag exists locally only, not pushed or released | Orphaned patch bump |
| `v1.9.0` | ❌ Blocked — not cut | See §3 |

---

## 3. Immediate Blockers

### 3.1 v1.9.0 release — flaky sort tests

Two tests fail under full parallel `--all-features` run but pass in isolation:

```
ledgerr-mcp/tests/query_transactions_tests.rs
  test_query_transactions_applies_sorting         FAIL (parallel only)
  test_query_transactions_deterministic_ordering  FAIL (parallel only)
```

**Root cause:** `TurboLedgerService::query_transactions` sort is not fully deterministic — no secondary sort key. Under parallel ingest from other tests, two transactions with the same primary key (date or amount) land in non-deterministic order.

**Fix:** Add `tx_id` as tie-break secondary sort key in `query_transactions`. One-line change in `ledgerr-mcp/src/actor.rs` or wherever the sort closure is.

**Verification:** `cargo test -p ledgerr-mcp --all-features -- test_query_transactions` must pass 5 consecutive times.

**Then cut the release:**
```bash
git checkout main
just release minor   # → v1.9.0 (dev/experimental per odd/even policy, no GitHub release)
# For next stable release: just release minor again → v1.10.0 (even, GitHub release created)
```

### 3.2 v1.8.1 local tag not pushed

`git tag | grep v1.8` shows `v1.8.1` locally. Either push it with a GitHub release or delete it to avoid confusion:
```bash
# Option A: clean up the orphan
git tag -d v1.8.1

# Option B: create the GitHub release for it
git push origin v1.8.1
gh release create v1.8.1 --title "v1.8.1 (patch)" --notes "See CHANGELOG.md"
```

---

## 4. Active Feature Branch — `feat/holonic-viz-sysml-owl2-cytoscape`

### Goal

Add a deterministic, locally-executable visualization layer that renders modelling language artifacts (SysML-v2 blocks, OWL2 class hierarchies, holonic compositions, capsule network topologies) as interactive Cytoscape.js graphs — with an automated observer/test loop using local screenshot + vision analysis.

### What is scaffolded (compiling on the branch)

**`crates/holon-viz`**
- `Holon` — recursive part/whole node with `HolonKind` enum (`SysmlBlock`, `OwlClass`, `CapsuleGroup`, `ProcessNode`, `AuditEvent`)
- `CytoscapeGraph` — deterministic Cytoscape.js JSON serialization
- `SysmlV2Emitter` + `Owl2Emitter` — text fragment generators
- `ProcessController` — append-only authorized transitions with Blake3 receipts
- `ImmutableActionLog` — append-only AI action record with Blake3 identity

**`crates/ledgerr-model-server`**
- `ModelServerConfig` + `ModelServerMcp::start()` stub
- `bin/model_server` entry point
- `msi-installer` feature gate (empty, WiX scaffolding to follow)

### What needs to be built next

**Step 1 — HTML renderer + Cytoscape.js embed**

Create `crates/holon-viz/src/renderer.rs`:
```rust
pub struct HtmlRenderer;
impl HtmlRenderer {
    pub fn render(graph: &CytoscapeGraph) -> String { /* self-contained HTML with inline Cytoscape.js CDN */ }
    pub fn write_to_file(graph: &CytoscapeGraph, path: &Path) -> Result<()> { ... }
}
```
The rendered HTML must be self-contained (inline JS, no external deps beyond CDN) so it can be opened in the Tauri WebView or a headless browser for testing.

**Step 2 — Observer / test pattern**

Create `crates/holon-viz/src/observer.rs`:
```rust
pub struct VizObserver {
    pub screenshot_path: PathBuf,   // where CDP screenshot lands
    pub vision_script: PathBuf,     // scripts/tauri-vision-analyze.py
}

pub struct VizObservation {
    pub caption: String,
    pub model: String,
    pub inference_ms: u64,
}

impl VizObserver {
    pub fn analyze(&self) -> Result<VizObservation> { /* uv run python script */ }
}
```

Two test tiers (see AGENTS.md §Observer/Test Pattern):
- **Fast:** headless Cytoscape JSON round-trip — verify topology from serialized graph data, no pixels
- **Slow:** CDP screenshot → `scripts/tauri-vision-analyze.py` (Florence-2-base) → `VizObservation` → assert caption contains expected node labels

Fast tier must run in CI. Slow tier gated on `TAURI_TEST_SCREENSHOT_PATH` env var.

**Step 3 — SysML-v2 parser**

The emitter exists (`SysmlV2Emitter::emit`). Add the inverse: a parser for the SysML-v2 textual notation that produces a `CytoscapeGraph`. This closes the round-trip and lets operators import `.sysml` files directly.

**Step 4 — `holon-viz` wired into `ledger-core` visualization**

Extend `HasVisualization` in `ledger-core/src/iso.rs` so that types implementing it can optionally emit a `CytoscapeGraph` alongside their `VisualizationSpec`. The Cytoscape path is additive — it does not replace Mermaid or isometric.

**Step 5 — MSI installer scaffolding**

In `crates/ledgerr-model-server`, behind the `msi-installer` feature gate, wire a WiX toolset invocation via `xtask`. The installer should package `model-server` binary + a default `ModelServerConfig.toml`. See the existing `scripts/tauri-build.ps1` and `scripts/tauri-release-install.ps1` for the Windows packaging pattern.

---

## 5. Backlog (Prioritised)

| Priority | Item | Location | Notes |
|---|---|---|---|
| P0 | Fix flaky sort tests, cut v1.9.0 | `ledgerr-mcp/src/actor.rs` | Unblocks release |
| P0 | Docling PDF bridge | `ledger-core/src/ledger_ops.rs` | Issue #60 — core value gap |
| P1 | `holon-viz` HTML renderer + observer | `crates/holon-viz/src/` | Feature branch §4 |
| P1 | Hermes OpenAgent integration | `ledgerr-mcp-core/src/provider.rs` | Dogfood first use case |
| P1 | Semantic rule selector impl | `ledger-core/src/rule_registry.rs` | Currently panics → waterfall fallback |
| P2 | `ProofAttestation` type + arc-kit-au link | `ledger-attest` + `arc-kit-au` | PRD-6-FUTURE |
| P2 | SysML-v2 parser (round-trip) | `crates/holon-viz/src/` | Closes SysML import |
| P2 | FBAR / Schedule-C Z3 predicates | `ledger-core/src/legal.rs` | PRD-6-FUTURE |
| P2 | WiX MSI installer | `crates/ledgerr-model-server` | Behind `msi-installer` feature gate |
| P3 | HelixDB graph projection | Phase 3 per PRD-10 | Fallback: `heed` + `petgraph` |
| P3 | Local fine-tuning pipeline (Phi-4-mini) | `ledgerr-llm` | PRD roadmap §2026-Q3 |
| P3 | Xero McpProvider | `ledgerr-mcp-core` + `ledgerr-xero` | After Hermes integration |

---

## 6. Architecture Decisions in Force

| Decision | Rationale | Do Not Reverse |
|---|---|---|
| Excel workbook = canonical human/audit layer | CPA workflow constraint | Yes |
| Blake3 content-hash IDs | Idempotent re-ingest | Yes |
| Rhai for classification rules | Agent/human editable without recompile | Yes |
| Cytoscape.js (JS) for graph rendering | Not a Rust dep — emit JSON, consume in WebView | Yes |
| `b00t` is alphaware, test scaffolding only | Operator-stated explicitly 2026-05-13 | Yes |
| `holon-viz` is upstream of `b00t` | b00t will consume holon-viz for test animation | Yes |
| Odd/even minor version policy | Even = stable + GitHub release; odd = dev only | See Justfile `release` recipe |
| Secrets stay in Windows Credential Manager | Local-first, no cloud credential storage | Yes |

---

## 7. How to Start Work

```bash
# Verify state
git checkout main && git log --oneline -5
git checkout feat/holonic-viz-sysml-owl2-cytoscape && git log --oneline -5

# Run fast tests to confirm baseline
cargo test -p holon-viz -p ledgerr-model-server

# Fix release blocker (delegate to rust-craftsman)
# Prompt: "Fix non-deterministic sort in TurboLedgerService::query_transactions.
#  Failing tests: test_query_transactions_applies_sorting,
#  test_query_transactions_deterministic_ordering in
#  ledgerr-mcp/tests/query_transactions_tests.rs.
#  Root cause: no secondary sort key — add tx_id as tie-break.
#  Verify: cargo test -p ledgerr-mcp --all-features 5 consecutive passes."

# Cut the release (delegate to general-purpose agent, background)
# git checkout main && just release minor

# Continue feature work
git checkout feat/holonic-viz-sysml-owl2-cytoscape
# Next: crates/holon-viz/src/renderer.rs + observer.rs
```

**Delegation rules (mandatory — see AGENTS.md §2026-05-13):**
- CI polling → background agent, never inline
- `cargo test --all-features` / `just release` → background agent
- Code structure queries → `codebase-memory-mcp`, never grep/Read
- Test fixes → `rust-craftsman` agent with exact test name + failure message

---

## 8. Key Files for Orientation

| File | Purpose |
|---|---|
| `AGENTS.md` | Operational rules, session learnings, delegation patterns |
| `CLAUDE.md` | Project constraints, stack, non-negotiables |
| `PRD-10.md` | Financial pipeline gaps (Docling, AGT gate, rule hot-reload) |
| `PRD-9.md` | Isometric visualization layer model |
| `PRD-6-FUTURE.md` | Type attestation system concept |
| `crates/holon-viz/src/lib.rs` | Holonic viz engine entrypoint |
| `crates/ledger-core/src/iso.rs` | `HasVisualization` trait, `VisualizationSpec` |
| `crates/ledger-core/src/visualize.rs` | Mermaid/AST viz generation |
| `scripts/tauri-vision-analyze.py` | Florence-2-base screenshot analysis |
| `scripts/tauri-cdp-test.ps1` | CDP harness for WebView2 |
| `Justfile` | All canonical build/test/release recipes |
