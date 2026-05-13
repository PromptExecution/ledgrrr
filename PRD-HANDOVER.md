# l3dg3rr — PRD Handover & Session Log

> **Purpose:** Living handover document. Captures what shipped each session, operator corrections, and the working backlog. Maintained by coordinator; sub-agents must not edit this file directly.

---

## Session Log — 2026-05-13 (PM-2)

### Shipped

| Item | Details |
|------|---------|
| **cytoscape-dagre layout** | `crates/ledgerr-host/ui/index.html` — added `dagre@0.8.5` and `cytoscape-dagre@2.5.0` CDN scripts. `crates/ledgerr-host/ui/main.js` — `initVizPanel()` layout switched from `cose` to `dagre` (`rankDir:'TB'`, `nodeSep:50`, `rankSep:70`). Demo confirmed working in WebView2. |
| **Viz type architecture decision** | TRIZ/MECE + Eisenhower analysis completed. `specta`/`tauri-specta` approved by operator override for transport layer. `wasm-bindgen` deferred (no client-side filter UX in current PRD). |

### Architectural Ruling — Viz Type System (revised)

**Operator override recorded 2026-05-13 PM-3: specta IS approved.**

The viz layer type system has four MECE layers: Metamodel (SysML v2 / KerML) → Contract (Rust structs + specta derives) → Transport (tauri-specta generated TS bindings) → Render (Cytoscape.js).

**Approved — use now:**
- `specta` (`version = "2", features = ["derive"]`): `#[derive(specta::Type)]` on `CytoscapeNodeData`, `CytoscapeNode`, `CytoscapeEdgeData`, `CytoscapeEdge`, `CytoscapeGraph` in `crates/holon-viz/src/cytoscape.rs`.
- `tauri-specta` (`version = "2", features = ["derive", "typescript"]`): `#[specta::specta]` on Tauri commands; `Builder` in `main.rs` exports `ui/bindings.ts` automatically on debug builds.
- `get_holon_viz_graph` return type changes from `Result<String, String>` to `Result<CytoscapeGraph, String>` — Tauri serializes via serde, JS receives a typed object (no `JSON.parse` needed).

**Deferred — add only when client-side filter UX is a P1 item:**
- `wasm-bindgen` / `holon-viz-wasm` crate.

**Scheduled — after metamodel is stable:**
- KerML codegen to generate Rust structs from SysML v2 metamodel.

---

## Session Log — 2026-05-13 (PM-3)

### Shipped

| Item | Details |
|------|---------|
| **specta + tauri-specta wiring** | `crates/holon-viz/Cargo.toml` — `specta = "=2.0.0-rc.25"`. `crates/holon-viz/src/cytoscape.rs` — `specta::Type` derived on all 5 Cytoscape types. `crates/ledgerr-host/Cargo.toml` — `specta`, `specta-typescript`, `tauri-specta` added (windows target). `crates/ledgerr-host/src/bin/tauri/commands.rs` — `#[specta::specta]` on all 14 commands; `get_holon_viz_graph` return type changed from `Result<String, String>` to `Result<CytoscapeGraph, String>`; all 7 payload structs gain `specta::Type`. `crates/ledgerr-host/src/bin/tauri/main.rs` — `generate_handler!` replaced with `SpectaBuilder` + `collect_commands!`; debug builds export `ui/bindings.ts`. |
| **main.js JSON.parse removed** | `initVizPanel()` updated: `var data=JSON.parse(json)` removed; `data.nodes`/`data.edges` references changed to `json.nodes`/`json.edges` since Tauri now deserializes directly. |
| **AGENTS.md + PRD specta ruling updated** | Operator override recorded: specta is the approved transport layer tool. All “eliminated” language removed. |

### Build status
`cargo check -p ledgerr-host --bin host-tauri` — clean (7 new specta crates compiled, 0 errors, 0 new warnings).

---

## Session Log — 2026-05-13 (PM-4)

### Shipped

| Item | Details |
|------|---------|
| **TypeRelationshipGraph emitter** | `crates/holon-viz/src/type_graph.rs` — typed `TypeNode`, `TypeRelationship`, `TypeRelationshipKind`, and `TypeRelationshipGraph`; deterministic Cytoscape conversion with sorting/deduping; unit tests added. |
| **TypeGraphCommand** | `crates/ledgerr-host/src/bin/tauri/commands.rs` — `get_type_graph` command returns `CytoscapeGraph` through `TypeRelationshipGraph`; registered in `main.rs`; VZ panel now invokes `get_type_graph`. |
| **codebase-memory-mcp follow-up** | `PromptExecution/codebase-memory-mcp-b00t-ir0n-ledg3rr` has Issues disabled, so downstream tracking was filed at `https://github.com/PromptExecution/ledgrrr/issues/97`. Live graph-query population remains blocked until that MCP surface is stable. |

### Build status
`cargo test -p holon-viz type_graph` — clean, 4 tests passed.
`cargo check -p ledgerr-host --bin host-tauri` — clean.

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

Priority legend: **P1** = current sprint / unblocked, **P2** = next sprint, **P3** = future / nice-to-have, ~~strikethrough~~ = shipped.

| Priority | Item | Notes |
|----------|------|-------|
| ~~P1~~ | ~~`cytoscape-dagre` layout extension~~ | Shipped 2026-05-13 PM-2. dagre TB layout live in VZ panel. |
| ~~P1~~ | ~~`specta` + `tauri-specta` wiring~~ | Shipped 2026-05-13 PM-3. Add derives to `holon-viz/src/cytoscape.rs`, wire `tauri-specta` `Builder` in `main.rs`, export `ui/bindings.ts`. Change `get_holon_viz_graph` return type to `Result<CytoscapeGraph, String>`. Update `main.js` to drop `JSON.parse`. |
| ~~P1~~ | ~~`TypeRelationshipGraph` emitter (`holon-viz`)~~ | Shipped 2026-05-13 PM-4. Adds typed nodes/relationships plus deterministic Cytoscape conversion for `implements`, `contains`, `derives_from`, and `references`. |
| ~~P2~~ | ~~`TypeGraphCommand` Tauri command~~ | Shipped 2026-05-13 PM-4 as `get_type_graph`, registered with `tauri-specta`, consumed by VZ panel. Live codebase-memory-mcp population remains blocked by issue #97; command currently uses a typed seed graph. |
| P2 | KerML metamodel for domain types | Author KerML textual notation for core domain types. Codegen target: Rust structs + TS types from single source. Lives in `xtask` or dedicated `crates/ledger-kerm`. |
| P2 | `HasVisualization` trait wiring | Wire `HasVisualization` implementations from `ledger-core/src/iso_objects.rs` into Cytoscape node metadata (ZLayer → node color, SemanticType → node shape) |
| P3 | `holon-viz-wasm` crate | `wasm-bindgen` on `VizGraph` for client-side filtering (e.g., filter-by-kind). Add when filter UX is a P1 item. Do not add speculatively. |
| P3 | TypeScript build step for UI | `cytoscape@3` has built-in TS types; add `esbuild` build step to `ui/` when ready |

---

*Last updated: 2026-05-13 (PM-4 session)*

---

## Post-MVP Roadmap

### Viz Layer

The following initiatives extend the VZ panel beyond its MVP state. They are sequenced by dependency: layout legibility first, then structural data emission, then rich type wiring.

| Item | Priority | Effort | Depends On |
|------|----------|--------|------------|
| **`cytoscape-dagre` hierarchical layout** — swap `cose` for `dagre` (top-down) in `initVizPanel()`; one CDN script addition to `ui/index.html` and a layout param change. Makes type/trait inheritance graphs legible at a glance. | P1 | XS | `cytoscape@3` CDN already loaded |
| **`TypeRelationshipGraph` emitter** — typed `holon-viz` graph model for Rust type edges: `implements`, `contains`, `derives_from`, `references`, with deterministic Cytoscape conversion. | Done | S | Shipped PM-4 |
| **`TypeGraphCommand` Tauri command** — `get_type_graph` returns typed edges for datum/holon/trait relationships as `CytoscapeGraph` JSON and feeds the VZ panel. Live `codebase-memory-mcp` query population is tracked by issue #97. | Done | S | Shipped PM-4 |
| **`HasVisualization` wiring** — map `ZLayer` → Cytoscape node color, `SemanticType` → node shape for all 21 domain types in `ledger-core/src/iso_objects.rs`. Makes the pipeline state machine visible in the viz panel. | P2 | M | `TypeGraphCommand`, `iso_objects.rs` trait impls |
| **TypeScript build step** — `cytoscape@3` ships TypeScript types. Add `esbuild` to `ui/` when the panel logic grows beyond ~400 LOC. Not needed now; tracked as tech debt. | P3 | S | Panel logic maturity threshold |

### Architecture Notes

**Isolated viz rendering confirmed.** Cytoscape runs inside WebView2 (full Chromium engine); no WASM compilation of JS libraries is required. The `HasVisualization` isometric layer — Rhai DSL, `ZLayer`, and isometric projection math — remains architecturally separate and untouched by the viz panel work. The only integration point is the Tauri command boundary: `TypeGraphCommand` returns `CytoscapeGraph` JSON, and `initVizPanel()` consumes it. This keeps the rendering concern fully isolated from the domain model.

**Observer → kaizen loop.** Once `cytoscape-dagre` is wired, connect `VizObserver` (CDP screenshot → `tauri-vision-analyze.py`) to a `just test-holon-viz` assertion that verifies node layout is hierarchical — specifically that the top node has a lower Y coordinate than its children. This closes the automated visual regression loop and gives the kaizen workflow a stable signal for layout correctness without requiring manual inspection.
