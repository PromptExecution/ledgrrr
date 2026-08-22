# SysML v2 Tooling Survey (Epic, Part 1)

Status: survey complete, key decisions made — see "Decisions" below. DVC
integration remains open; everything else is ready to scope as follow-up
tasks.

## Purpose

Before writing any more of our own SysML v2 / KerML tooling, inventory what
already exists so we don't reimplement a parser, LSP, or MCP bridge that's
already maintained upstream. This doc is the output of that survey plus a
prioritized research list, and the decisions made once the tiered candidates
were reviewed.

## Constraints (given, non-negotiable unless noted)

1. **SysML v1 is never in scope.** Every candidate below targets SysML v2 /
   KerML textual notation specifically.
2. **Language priority for anything we build or wrap: Rust > Python >
   TypeScript.** A tool in Java or any other JVM language is only usable if
   it's packaged as a GraalVM native-image running inside a container — never
   installed on a host directly.
3. **MCP servers, LSPs, and linters must be shareable across many systems via
   b00t** — not bespoke per-repo integrations.
4. **New libraries we author get their own crate, hosted under the
   PromptExecution GitHub org** — matching the existing pattern
   (`ufo-types`, `holon-viz`, `b00t-reflect`, etc. all already live there).

## What already exists in this codebase (don't re-derive, build on it)

This is not a green-field epic — `ledgrrr` already has real SysML v2 /
ontology infrastructure:

- **`crates/holon-viz`** — emits SysML-v2 textual block definitions and
  OWL2/Turtle fragments from a `CytoscapeGraph` (`SysmlV2Emitter`,
  `Owl2Emitter`). This is an *emitter*, not a parser — it goes
  model → SysML-v2 text, not the reverse.
- **`crates/ufo-types`** — UFO ontology stereotypes (`Kind`/`SubKind`/`Role`/
  `Relator`/`Mode`), consumed downstream by `b00t-cli`
  (`Cargo.toml:55: ufo-types = { workspace = true }`) and 3+ other crates.
  Known gap tracked in `PromptExecution/ufo-types#2`: missing
  `Perdurant`/`Event`/`Situation`/`InformationArtifact` — the KerML
  `Performance`/`Action` (behavior) side of the hierarchy, needed for any
  pipeline/process modeling, not just structural types.
- **`AGENTS.md` (lines ~45-51, ~700-721)** — already rules KerML textual
  notation as the canonical metamodel source of truth, with codegen
  generating Rust structs + TypeScript types from one source as the target
  architecture. `specta`/`tauri-specta` is the approved Rust→TS bridge until
  that codegen lands. `wasm-bindgen` is explicitly deferred (no speculative
  work). Feature branch `feat/holonic-viz-sysml-owl2-cytoscape` already has
  `holon-viz` + `ledgerr-model-server` scaffolded and compiling.
- **`docs/ontological-implementation-spec.md`** — works out the OWL2 (open
  world) vs SysMLv2 (closed world) reconciliation in detail, proposes a
  layered hybrid model, and already links openCAESAR / Flexo-SysMLv2 talks.
- **`PromptExecution/ledgrrr#99`** — prior research note: SysML v2 (via
  KerML) is being aligned to RDF/OWL2 by the OMG community; openCAESAR /
  Flexo-SysMLv2 is the project doing that alignment (OWL + OML
  representations of the SysML v2 abstract syntax); no lossless OMG-standard
  SysML↔RDF transform exists yet.
- **b00t-cli MCP substrate** — `src/datum_mcp.rs`, `src/commands/mcp.rs`,
  `src/bin/mcp-user.rs` already implement the `DatumType` pattern used to
  expose typed capabilities as MCP tools (see the Podman `DatumType`
  precedent). A SysML capability would plug into this same pattern rather
  than needing a new MCP plumbing layer.

**Implication:** the KerML-parser and MCP/LSP-in-b00t work is not starting
from zero conceptually — it's already the stated next investment
(`AGENTS.md` line 721: "Invest after metamodel is stable"). This survey is
what "stable enough to invest" needs to be checked against.

## Candidate survey (from github.com/mycr0ft/awesome-sysml)

Tiered by "evaluate this first" — Tier 0 has the highest chance of
eliminating build work outright.

### Tier 0 — evaluate first, could remove most of the build

| Candidate | Lang | Why it's Tier 0 |
|---|---|---|
| [`sysml-v2-parser`](https://crates.io/crates/sysml-v2-parser) | **Rust** | Published crate, parses SysML v2/KerML textual syntax to AST, resilient editor mode (partial AST + diagnostics on invalid input — needed for an LSP). If this is healthy, it's the direct complement to `holon-viz`'s emitter: parse → model → `holon-viz` → SysML-v2 text/OWL2, closing the round-trip without us writing a parser. |
| [`sysmlpy`](https://github.com/mycr0ft/sysmlpy) | Python | Pure-Python ANTLR4 parser, 123/123 conformance claimed, Pint units, NetworkX/Kuzu graph backends. Same maintainer as the awesome-list itself. Note: `mycr0ft/sysmlpy#3` is an open bug (crash in `AnnotatingElement`, plus a "two-line fix causes silent data loss" note) — worth reading before depending on it. |
| [`sysml-v2-lsp`](https://github.com/daltskin/sysml-v2-lsp) | TypeScript | Full LSP (diagnostics, completion, goto-def, rename, semantic tokens) **plus a bundled MCP server** and Mermaid preview, MIT licensed. This is the closest existing match to "LSP + MCP, shareable" — question is whether we vendor/wrap it or treat it as a reference and build the b00t-native version in Rust per the language-priority rule. |
| [`sysml-v2-docs`](https://github.com/voidaliot/sysml-v2-docs) | Markdown | Full OMG SysML v2/KerML/API spec as plain markdown, explicitly designed for AI agent consumption. Zero engineering cost to adopt as a b00t skill/reference corpus or MCP resource — do this regardless of what else gets picked. |

### Tier 1 — Rust, second look

- [`Tessera`](https://github.com/jackhale98/Tessera) — Rust/Svelte CLI + Tauri
  desktop app, tolerance analysis + BOM management + SysML v2 import/export
  round-trip. Overlaps with our Tauri desktop (`ledgerr-host`) stack.
- [`sysmlv2-gui`](https://github.com/DeciSym/sysmlv2-gui) — Rust/egui native +
  WASM viewer, renders the OMG graphical notation. Relevant only if the
  Cytoscape.js render surface in `holon-viz` ever needs a native desktop
  alternative — not needed today.
- [`daltskin/sysml-v2-grammar`](https://github.com/daltskin/sysml-v2-grammar) —
  ANTLR4 grammar files for SysML v2 + KerML. Fallback substrate if
  `sysml-v2-parser` turns out abandoned/inadequate and we need to generate a
  parser ourselves — last resort, not a starting point.

### Tier 2 — Python, third look

- [`sysml-style`](https://github.com/mycr0ft/sysml-style) — the only linter/
  formatter on the whole list, built on `sysmlpy`. If we need a `.sysml`
  linter for CI, this is it or nothing (no Rust/TS equivalent exists yet).
- [`PySysML2`](https://github.com/nakane1chome/PySysML2) — ANTLR4 Python
  parser, exports to pandas/Graphviz/NumPy. Apache-2.0.
- [`Windseeker`](https://github.com/Westfall-io/windseeker) — dependency
  analysis + Jupyter notebook generation + SVG/PNG extraction.
- [SYSMOD SysML v2 API + MCP](https://github.com/Open-MBEE/sysmod-sysmlv2-api) —
  Flask REST API + a 17-tool MCP server for SYSMOD models (requirements, use
  cases, quality checks, AI wizard). Good MCP-surface prior art even though
  it's Python/Flask, not something we'd run as-is.

### Tier 3 — TypeScript, fourth look

- [VSCode SysML Extension](https://github.com/daltskin/VSCode_SysML_Extension) —
  10 diagram views, model explorer, 29 snippets. Editor-support reference.
- [`sysmlv2-language-server`](https://github.com/vpathai-git/sysmlv2-language-server) —
  Langium-based, 210+ validation rules — the other LSP candidate, would need
  a bake-off against `daltskin/sysml-v2-lsp` if we go the "wrap a TS LSP"
  route instead of building our own.
- [`sysml-reactflow`](https://github.com/Hollando78/SysML-reactflow) — React
  Flow components for SysML v2. Not needed — `holon-viz` already committed
  to Cytoscape.js for rendering (see `AGENTS.md` type-architecture ruling).

### Tier 4 — JVM, container-only per constraint #2, evaluate last

- [SysML v2 Pilot Implementation](https://github.com/Systems-Modeling/SysML-v2-Pilot-Implementation) —
  the OMG reference implementation (Java/Xtext). Documented 16 GB RAM
  guidance as a plain JVM process. Likely still worth having as a
  **conformance oracle** (validate our parser's output against the reference
  parser) — but only as a GraalVM native-image in a container, never a host
  install.
- [SysML v2 API Services](https://github.com/Systems-Modeling/SysML-v2-API-Services) —
  Spring Boot reference REST API. Same GraalVM-container-only rule; only
  worth standing up if we need live branch/commit/element API semantics
  rather than pure text parsing.
- Eclipse SysON, Open-MBEE `sysmlv2-web-modeler`, Refinery Validation
  Pipeline — graphical/validation tooling, all JVM-heavy. Deprioritized;
  same container-only rule applies if any of these are ever adopted.

### Sample corpora (free test data, use instead of hand-writing our own)

- [SysML v2 Release examples](https://github.com/Systems-Modeling/SysML-v2-Release/tree/main/sysml) —
  canonical OMG examples (`kerml/`, `sysml/`).
- [GfSE SysML v2 Models](https://github.com/GfSE/SysML-v2-Models) — CI-validated
  against the Pilot Implementation parser.
- [Airbus Apollo 11 SysML v2](https://github.com/airbus/apollo-11-sysml-v2) —
  large, realistic 5-layer model with full requirements traceability.
- [Don't Panic Batmobile](https://github.com/MBSE4U/dont-panic-batmobile) —
  small, approachable teaching example.

### .NET tooling

- **`SysML2.NET`** — decided in scope under the **same rule as JVM**: usable
  only when containerized/isolated the same way, never a direct host
  dependency. Not otherwise prioritized — no candidate here has been
  evaluated yet.

### Commercial tools — still flagging, not deciding

- CATIA/Cameo, Siemens, Ansys, PTC, Syside, etc. — not evaluated; OSS-first
  per the rest of this survey.

## Decisions (resolved 2026-08-21/22)

1. **Home for this work: confirmed `ledgrrr`.** The existing infrastructure
   (`holon-viz`, `ufo-types`, `feat/holonic-viz-sysml-owl2-cytoscape`,
   `ledgrrr#99`) is the substrate — no new repo/system.
2. **Wrap vs. build for LSP/MCP: wrap `daltskin/sysml-v2-lsp` now.** Accept
   the TypeScript runtime dependency inside b00t as a pragmatic exception to
   the Rust-first rule, in exchange for shipping in days/weeks instead of
   building a Rust LSP from scratch on an unverified parser crate. Revisit
   only if the TS dependency becomes an actual operational problem.
3. **.NET scope: same rule as JVM** — see above.
4. **Conformance oracle: stand up the OMG Pilot Implementation now, in
   parallel** with the Tier 0 spikes below — as a GraalVM native-image in a
   container, never a host install.

## Still open

- **DVC integration** for data-source tracking — flagged in the earlier
  planning round, untouched by this survey; separate tool line from
  SysML-v2 parsing/LSP/MCP.

## Next steps

1. Spike-test `sysml-v2-parser` (Rust crate) against the OMG sample corpora
   above — does it round-trip with `holon-viz`'s `SysmlV2Emitter` output?
2. Read `mycr0ft/sysmlpy#3` in full before depending on `sysmlpy` for
   anything — the "silent data loss" note is a red flag worth resolving
   first.
3. Vendor `sysml-v2-docs` as a b00t skill/reference corpus — no decision
   blocking this, do it regardless of the rest.
4. Vendor/wrap `daltskin/sysml-v2-lsp` (LSP + MCP server) into the b00t MCP
   surface via the existing `DatumType` pattern (`src/datum_mcp.rs`,
   `src/commands/mcp.rs`) — decided, ready to scope as its own task.
5. Stand up the OMG Pilot Implementation as a GraalVM native-image in a
   container, in parallel with (1) and (2), as the conformance oracle.
6. DVC integration remains open — needs its own scoping pass, separate from
   this epic's parser/LSP/MCP thread.
