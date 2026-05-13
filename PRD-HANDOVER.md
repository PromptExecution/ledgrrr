# l3dg3rr — PRD Handover & Session Log

> **Purpose:** Living handover document. Captures what shipped each session, operator corrections, and the working backlog. Maintained by coordinator; sub-agents must not edit this file directly.

---

## Session Log — 2026-05-13 (PM)

### Shipped

| Item | Details |
|------|---------|
| **v1.9.0 release** | Flaky sort tests root-caused to concurrent ZIP writes sharing `test.xlsx` in `IngestStatementRowsRequest` (not sort ordering). Secondary sort tie-break added to `apply_transaction_sort`. Pre-push hook fixed — inverted `--is-ancestor` args had blocked all fast-forward pushes. |
| **`HtmlRenderer`** | `crates/holon-viz/src/renderer.rs` — self-contained HTML + Cytoscape.js CDN output. |
| **`VizObserver`** | `crates/holon-viz/src/observer.rs` — CDP port 19222, graceful `CDP_UNAVAILABLE` fallback. |
| **VZ sidebar panel (Tauri)** | Cytoscape.js integrated into existing `host-tauri` app as a **VZ** panel. Wired via `get_holon_viz_graph` Tauri command in `crates/ledgerr-host/src/bin/tauri/commands.rs`. `ui/index.html` loads `cytoscape@3` CDN; `ui/main.js` adds VZ panel with `initVizPanel()`. |
| **CDP test** | `scripts/test-holon-viz.ps1` — CDP WebSocket test asserting `window._cy.nodes().length >= 5`. |
| **Justfile recipes** | `demo-viz`, `test-holon-viz`, `test-holon-viz-fast` added. |
| **b00t mesh v1.1.0** | 5 learned patterns incorporated; 4 new memory files written. |

---

### Operator Corrections — Delegation Failures (verbatim)

> These are recorded exactly as stated. They inform the standing sub-agent mandate below.

1. **Built `open_in_browser`/`wsl_to_win_path` in Rust** — wrong layer. Browser-open belongs in PowerShell/Justfile. Had to be corrected.

2. **Did not investigate existing architecture before building** — wrote a standalone demo binary with its own browser-open logic before understanding that `host-tauri.exe` already exists and the tray app already runs on Windows via `wsl2-pwsh-*` recipes.

3. **Failed to use sub-agents throughout session** — did all Tauri integration inline (`commands.rs` edit, `main.js` patch, `index.html` edit, Justfile, PowerShell script), exhausting operator context. Should have delegated to `rust-craftsman` (commands.rs) and `general-purpose` (JS/PS/Justfile edits) in parallel.

4. **Context exhaustion repeated** — same failure mode as previous session despite AGENTS.md update.

---

### Standing Rule — Sub-Agents Mandatory

**All implementation work must be delegated to sub-agents.**

The coordinator's role is:
- Write the delegation prompt
- Verify sub-agent output (compile, test, review diff)
- Record outcomes here

The coordinator must **never** edit implementation files inline (`.rs`, `.js`, `.html`, `.ps1`, `Justfile`). Violating this rule will exhaust context and repeat the failures above.

---

## Backlog

Priority legend: **P1** = current sprint / unblocked, **P2** = next sprint, **P3** = future / nice-to-have.

| Priority | Item | Notes |
|----------|------|-------|
| P1 | `cytoscape-dagre` layout extension | Add to VZ panel; switch from `cose` to `dagre` hierarchical layout for type/trait graphs |
| P1 | `TypeRelationshipGraph` emitter (`holon-viz`) | Models datum/holon trait edges (`implements`, `contains`, `derives_from`) queryable from `codebase-memory-mcp` |
| P2 | `TypeGraphCommand` Tauri command | Queries codebase graph, returns typed edges for the viz panel |
| P2 | `HasVisualization` trait wiring | Wire `HasVisualization` implementations from `ledger-core/src/iso_objects.rs` into Cytoscape node metadata (ZLayer → node color, SemanticType → node shape) |
| P3 | TypeScript build step for UI | `cytoscape@3` has built-in TS types; add `esbuild` build step to `ui/` when ready |

---

*Last updated: 2026-05-13 (PM session)*

---

## Post-MVP Roadmap

### Viz Layer

The following initiatives extend the VZ panel beyond its MVP state. They are sequenced by dependency: layout legibility first, then structural data emission, then rich type wiring.

| Item | Priority | Effort | Depends On |
|------|----------|--------|------------|
| **`cytoscape-dagre` hierarchical layout** — swap `cose` for `dagre` (top-down) in `initVizPanel()`; one CDN script addition to `ui/index.html` and a layout param change. Makes type/trait inheritance graphs legible at a glance. | P1 | XS | `cytoscape@3` CDN already loaded |
| **`TypeRelationshipGraph` emitter** — new type in `holon-viz` that models Rust type edges: `implements`, `contains`, `derives_from`, `references`. Feeds the VZ panel with structural codebase data queried from `codebase-memory-mcp`. | P1 | S | `codebase-memory-mcp` indexed |
| **`TypeGraphCommand` Tauri command** — calls `codebase-memory-mcp` graph query, returns typed edges for datum/holon/trait relationships as `CytoscapeGraph` JSON. Enables live "show me everything that implements `HasVisualization`" queries in the VZ panel. | P2 | S | `TypeRelationshipGraph` emitter |
| **`HasVisualization` wiring** — map `ZLayer` → Cytoscape node color, `SemanticType` → node shape for all 21 domain types in `ledger-core/src/iso_objects.rs`. Makes the pipeline state machine visible in the viz panel. | P2 | M | `TypeGraphCommand`, `iso_objects.rs` trait impls |
| **TypeScript build step** — `cytoscape@3` ships TypeScript types. Add `esbuild` to `ui/` when the panel logic grows beyond ~400 LOC. Not needed now; tracked as tech debt. | P3 | S | Panel logic maturity threshold |

### Architecture Notes

**Isolated viz rendering confirmed.** Cytoscape runs inside WebView2 (full Chromium engine); no WASM compilation of JS libraries is required. The `HasVisualization` isometric layer — Rhai DSL, `ZLayer`, and isometric projection math — remains architecturally separate and untouched by the viz panel work. The only integration point is the Tauri command boundary: `TypeGraphCommand` returns `CytoscapeGraph` JSON, and `initVizPanel()` consumes it. This keeps the rendering concern fully isolated from the domain model.

**Observer → kaizen loop.** Once `cytoscape-dagre` is wired, connect `VizObserver` (CDP screenshot → `tauri-vision-analyze.py`) to a `just test-holon-viz` assertion that verifies node layout is hierarchical — specifically that the top node has a lower Y coordinate than its children. This closes the automated visual regression loop and gives the kaizen workflow a stable signal for layout correctness without requiring manual inspection.
