# Systems Modeling & Action Registry — Re-scope (Epic, Part 2)

Status: research + design complete, all 6 open questions resolved. Still no
implementation — this doc scopes the next concrete tasks, doesn't do them.
Follows on from [`docs/sysml-v2-tooling-survey.md`](sysml-v2-tooling-survey.md)
(Part 1, `PromptExecution/ledgrrr#180`) — read that first, this doc does not
repeat its SysML v2/KerML tooling findings.

## 1. Framing — why this doc exists

`ledgrrr` is currently described, in its own primary artifact, as a tax
accounting system. `README.md`'s System Thesis says so directly:

> `l3dg3rr` is a local-first bookkeeping application for turning financial
> documents into an accountant-usable, CPA-auditable source of truth.
>
> **Primary bookkeeping outcome:** ingest raw historical statements, classify
> and reconcile transactions, preserve evidence and mutation history, then
> export a CPA-reviewable Excel workbook...

That framing is now explicitly corrected by the user: **ledgrrr is a systems
modeling process and action registry**, not a tax-accounting tool — tax/
bookkeeping is one dogfood vertical sitting on top of a more general
substrate, not the substrate's purpose.

This correction is not a pivot invented from nothing — `AGENTS.md` already
carries the internal tension. It opens by calling ledgrrr "a local agentic
governance proxy... not a CRM, not a cloud SaaS, not a plain ETL pipeline,"
and its own Roadmap section says, in so many words, that bookkeeping is a
test case rather than the point:

> Wire `McpProviderRegistry` into `ledgerr-mcp-server.rs`... **Target:
> ledgrrr's own tax-ledger dataset is the first dogfood corpus.**

And separately schedules "MBSE / SysML-v2 isometric expansion" as a named
next phase with KerML as the canonical metamodel. So the README is the
lagging artifact — the AGENTS.md roadmap and the live
`feat/holonic-viz-sysml-owl2-cytoscape` branch already point the direction
the user is now making explicit. `docs/ontological-implementation-spec.md`
is the clearest evidence of the tension living inside one document: its
architecture is genuinely general (OWL2 open-world domain layer + SysML v2
closed-world process layer + Rhai runtime validation + Z3/Kasuari formal
proof), but it is titled and framed entirely in tax-ledger terms ("hybrid
ontological approach for **tax-ledger** holon-viz system", `TaxCategory`/
`Jurisdiction`/`GSTFreeSupply` used as the only worked example throughout).
README correction itself is out of scope for this doc — see §7.

**Corrected positioning for this epic:** ledgrrr is a general-purpose,
git-versioned systems modeling and action/decision registry. Requirements
(ReqIF), schema/data modeling (LinkML), and a decision+cost ledger are three
new domain verticals sitting on the same substrate the tax-ledger vertical
already uses — the ontology graph, the evidence chain, Rhai policy, and
KerML/OWL2 metamodel work already surveyed in Part 1. None of them should be
built as a parallel system.

## 2. Mapping onto existing mechanisms vs. genuine gaps

Verified against the current repo (25 crates under `crates/`, detached HEAD
at `2d546ed`; PRD-6's "9 crates" figure is stale — the workspace has grown
substantially since).

| Need | Existing mechanism | Fit | Gap |
|---|---|---|---|
| Deterministic identity for requirements/decisions/costs, dedup-safe re-ingest | `crates/arc-kit-au/src/node.rs` — `NodeId::new(node_type, content_hash)`, Blake3 content hashing, format `{type_prefix}:{blake3_hex}` | Direct reuse — this is exactly the "version-controlled log of decisions and costs" primitive; a decision/cost is just content that gets the same treatment a transaction gets today | `NodeType` enum (`arc-kit-au`) only knows `SourceDoc/ExtractedRow/Transaction/Classification/ModelProposal/OperatorApproval/WorkbookRow/ValidationIssue` — no `Requirement`/`Decision`/`Cost`/`SchemaEntity` variants yet |
| Typed graph linking requirements ↔ decisions ↔ costs ↔ evidence ↔ artifacts | `crates/ledger-core/src/ontology.rs` `ArtifactKind` enum + `crates/ledgerr-mcp/src/ontology.rs` + `crates/ledger-core/src/graph.rs` | Direct reuse — same pattern already used for `XeroContact`/`WorkbookRow`/etc. | `ArtifactKind` is closed and 100% bookkeeping-shaped. **Decided (§5, Q1): widen the core enum** — `Requirement`/`Decision`/`Cost` become first-class `ArtifactKind`/`NodeType` variants, not custom kinds through `ledgerr_ontology`. Rationale: unlike OpenMetadata's objects (one integration), these three are used across *every* domain vertical this epic touches, so first-class status pays for itself immediately rather than being speculative. |
| Version-controlled history of decisions/costs (not just current state) | Git itself (the repo already is one) + `crates/ledger-core/src/ledger_ops.rs`'s `LedgerOperation` trait + `arc-kit-au`'s append-only evidence chain (source docs → rows → tx → classification → proposal → approval → workbook row, each step a new node, nothing mutated in place) | Strong conceptual fit — the evidence chain is already "append, never mutate," which is the core discipline a decision/cost ledger needs | `ledger_ops.rs`'s `OperationKind` enum is tax-specific (`IngestStatement`, `ClassifyTransactions`, `ReconcileAccount`) — a `RecordDecision`/`RecordCost`/`ImportRequirement` operation kind doesn't exist yet. **Decided (§5, Q6): `arc-kit-au` stays the canonical decision+cost ledger mechanism** — `reqif-opa-mcp`'s evidence-store only supplies parsed requirements/evidence *into* this chain, it does not replace it. |
| 3D/2D visualization of requirements/decisions alongside existing pipeline/legal/attestation layers | `crates/ledger-core/src/iso.rs` `ZLayer` enum (`Document, Pipeline, Constraint, Legal, FormalProof, Attestation` — 6 variants, matching `EvidenceChain<S>`'s 6 state markers) + `HasVisualization` trait | Direct extension point, already anticipated — `docs/ontological-implementation-spec.md` §6.1 already proposes widening `ZLayer` to 8 variants by adding `Domain` (z=6, "ontological concepts," e.g. `TaxCategory`/`Jurisdiction`) and `Meta` (z=7, the metamodel machinery itself) | **Decided (§5, Q2): Requirement/Decision/Cost get their own new, dedicated `ZLayer` variant** (a 9th), rather than folding into the already-proposed `Domain` layer alongside `TaxCategory`/`Jurisdiction` — chosen despite `Domain`'s structural fit, to keep the systems-modeling vertical's content independently toggleable/colorable in the isometric renderer from tax-domain content. Naming and exact z-depth/color TBD when this is implemented. |
| Requirements interchange (import/export with other tools) | Nothing today — no `ArtifactKind::Requirement`, no ReqIF reader/writer anywhere in the workspace | — | Full gap. See §3. Now partially closed by the `reqif-opa-mcp` finding. |
| Schema/data modeling layer for defining what a "Requirement", "Decision", "Cost" node actually looks like | `crates/ufo-types` (UFO stereotypes: Kind/SubKind/Role/Relator/Mode) partially covers the ontological-type side; nothing covers the data-shape/validation side (JSON Schema/SHACL-equivalent) | Partial — UFO gives the philosophical category, not the concrete field-level schema | This is exactly LinkML's job — see §3. **Decided (§5, Q4): spike LinkML before committing** — not accepted or rejected outright yet. |
| Requirements ↔ decision ↔ cost traceability, review workflow, git branching for model changes ("sysgit.io-comparable capabilities") | Git (already the storage substrate) + `ledger_ops.rs`'s `LedgerOperation` composable trait + arc-kit-au evidence chain + Rhai policy for gating | See §4 — sysgit.io is a real, close-fit product | **Decided (§5, Q5): reference vibe only, not a feature-parity backlog.** §4's gap table stays context, not scoped tasks. |

## 2a. Addendum: Rust-as-source-of-truth via a derive macro (added mid-review)

Refinement raised by the user while this doc was being finalized, and folded
in before publishing rather than left as a follow-up surprise:

> If anything, the existing previous ledger types should be concrete
> implementations of the more generic SysML v2 types, ideally, we should be
> able to walk the Rust AST and use macros to generate a lot of the SysML
> content.

Concretely, today `arc-kit-au/src/node.rs` has real Rust structs with real
fields — e.g. `Transaction { tx_id, account_id, date, amount, description,
source_rows }` — that are hand-kept in sync with a separate flat `NodeType`
tag enum, and with whatever hand-written logic `holon-viz`'s `SysmlV2Emitter`
would eventually need to describe that type's shape as a SysML-v2 block
definition. The refinement inverts this: instead of `Transaction`/
`TaxCategory` (existing) and `Requirement`/`Decision`/`Cost` (new, decision 1
above) being flat, independent tag variants with per-type emitter logic
hand-written on the side, **each concrete Rust struct becomes a specialization
of a generic SysML v2 block type, and a proc-macro derive walks the struct's
AST (field names + types) to auto-generate its SysML-v2/KerML block
definition** — attributes from scalar fields, parts/references from
`NodeId`/foreign-key-shaped fields — rather than that mapping being
hand-maintained per type in the emitter.

This is not a new pattern for this codebase — it's the same technique
already accepted here for a different target language. `AGENTS.md` (PM-3,
approved 2026-05-13) already rules `#[derive(specta::Type)]` as the sanctioned
way to walk a Rust struct's AST and emit TypeScript bindings for `holon-viz`'s
Cytoscape types. A `#[derive(SysmlBlock)]`-shaped macro (name TBD) doing the
same AST walk to emit SysML-v2/KerML block text instead of TypeScript is a
direct extension of an already-approved approach, not a new architectural
risk to evaluate from scratch.

**Where this lands relative to decisions already made:**

- **Decision 1 (widen `ArtifactKind`/`NodeType`) stands, but its
  implementation shifts.** The new `Requirement`/`Decision`/`Cost` variants —
  and ideally the existing `Transaction`/`TaxCategory`/etc. variants too,
  as a follow-on cleanup — should be backed by structs annotated with the new
  derive macro, with the `ArtifactKind`/`NodeType` tag itself becoming
  machine-derivable from the annotated struct set rather than a separately
  hand-maintained enum. Retrofitting the *existing* variants is not required
  to unblock decision 1's next steps (§6 task 1 can proceed against the
  existing hand-written enum), but should be tracked as a deliberate
  follow-on so the codebase doesn't end up with two conventions
  side-by-side indefinitely.
- **Decision 4 (LinkML: spike first) gets a genuine Rust-native alternative
  to compare against.** LinkML's appeal was "compiles to Rust as one of many
  targets, but authoring is Python." A Rust derive macro authors *and*
  targets Rust directly — no Python step at all — which is a stronger fit
  for the Rust > Python > TypeScript priority from Part 1 for this specific
  need. The LinkML spike (§6 task 3) should now explicitly produce a
  side-by-side comparison against this macro approach, not just judge
  LinkML's Rust-codegen output in isolation. LinkML may still earn its place
  for cross-language schema portability elsewhere in the epic — that's a
  separate question from "what authors Requirement/Decision/Cost's Rust
  shape."
- **New crate implied.** Per Part 1's rule ("new libraries we author get
  their own crate under the PromptExecution org"), the derive macro is its
  own crate (proc-macro crates conventionally must be separate from the
  types they annotate anyway) — working name `sysml-derive` pending a real
  naming pass, built on `syn`+`quote` (the standard Rust macro-authoring
  crates, already implicitly trusted via `specta`'s own use of the same
  foundation).

This addendum does not reopen decisions 1–6 as voted — it refines *how*
decision 1 gets implemented and gives decision 4's spike a concrete
Rust-native alternative to measure against, both reflected in the task list
in §6.

## 3. Tooling survey — ReqIF and LinkML

Same Tier 0–4 convention as `sysml-v2-tooling-survey.md`.

### ReqIF

**`PromptExecution/reqif-opa-mcp`** ("ReqIF → OPA → SARIF Compliance Gate")
is the chosen first-party option, superseding the external candidates below.
Written by us, ours to modify however we want, **not currently used in
production anywhere** (so no compatibility debt to protect). Verified
directly against the repo (languages: Python 323k/OPA-Rego 28k/Just/
Dockerfile/Bicep/Shell — no Rust):

- `reqif_mcp/reqif_parser.py` — a real ReqIF 1.2 XML parser (`SpecObject`,
  `SpecType`, `AttributeDefinition`, `AttributeValue`, header), returning a
  `Result`-wrapped (`returns` library) typed structure, not a toy. Exposed as
  a FastMCP server (`reqif_mcp/server.py`) alongside `validation.py`,
  `normalization.py`, `compliance_gate.py`, `opa_evaluator.py`,
  `sarif_producer.py`/`sarif_validator.py`.
- `reqif_ingest_cli/` — a **second, standalone pipeline** that derives ReqIF
  baselines from source artifacts (XLSX via `xlsx_extractor.py`, offline
  PDF text via `docling_adapter.py`, optional Azure Foundry review hooks via
  `foundry_adapter.py`) — i.e. it already does the "heterogeneous document →
  normalized requirement" ingest shape §2 flags as ledgrrr's own
  `ingest.rs`-shaped gap, demonstrated end-to-end against real standards
  (`samples/standards/upstream/{nist-ssdf,owasp-asvs}/*.pdf` →
  `samples/standards/derived/*.reqif`).
- `docs/evidence-store.md` + `reqif_mcp/decision_logger.py` implement an
  immutable, append-only, ULID-identified store with three linked artifact
  kinds — verification events (JSONL), SARIF reports (JSON), OPA decision
  logs (JSONL, full requirement+facts+decision+policy-hash per entry) —
  chained by a single `evaluation_id`
  (`Requirement → Agent Facts → OPA Evaluation → Decision Log → SARIF →
  Verification Event`). Structurally the same "append-only, chained,
  content-addressed" discipline as ledgrrr's own `arc-kit-au` evidence chain
  (§2), just built independently in Python with ULIDs instead of Blake3 and
  JSONL files instead of a petgraph. **Per §5 Q6, this store is not becoming
  the canonical decision+cost ledger** — `arc-kit-au` keeps that role; this
  store is a source `reqif-opa-mcp` supplies evidence/requirements from.
- README states current scope plainly: "What exists today" = ReqIF
  parse/normalize/query/verification via MCP, deterministic source→ReqIF
  derivation (XLSX/PDF/DOCX/Markdown), OWASP ASVS + NIST SSDF sample
  baselines, typed test coverage (18 test files). "Still future work" =
  ingest not yet exposed as MCP tools, no baseline diffing, no persistent
  baseline storage beyond in-memory handles. Domain content today is
  security/compliance-flavored (cyber/SSDF/ASVS OPA bundles) — the parser,
  evidence store, and decision logger underneath are domain-agnostic and not
  hard-wired to that content.

| Candidate | Lang | Status | Notes |
|---|---|---|---|
| [`reqif-opa-mcp`](https://github.com/PromptExecution/reqif-opa-mcp) | Python + OPA/Rego | **Chosen** | Wrapped over MCP (§5 Q6, decided), not ported to Rust. |
| [`strictdoc-project/reqif`](https://github.com/strictdoc-project/reqif) | Python | Tier 1 — fallback only | Apache-2.0, actively maintained, real two-stage parser→tree design. Only worth reaching for if `reqif-opa-mcp`'s parser proves too narrow. |
| [`reqif-rs`](https://crates.io/crates/reqif-rs) | **Rust** | Tier 2 | Write-only (no read/parse path), stale since 2024-04. Not a round-trip solution. |
| [`doorstop-reqif`](https://pypi.org/project/reqif/0.0.1/) | Python | Tier 3 | One-way doorstop→reqif conversion only, relevant only if Doorstop is adopted upstream. |
| [`lutaml/reqif`](https://github.com/lutaml/reqif) | **Ruby**, not Rust | Tier 4, discard | GitHub confirms `language: Ruby`. Flagged so a future search doesn't re-chase this thinking it's a Rust hit. |

**Integration shape (decided, §5 Q6):** `reqif-opa-mcp` stays a separate
Python/FastMCP process, wrapped over MCP through the same `DatumType`
pattern used for other externally-owned tools (loose coupling, two
processes) — not ported/rewritten into ledgrrr's Rust workspace. A Rust
second-stage converter reads `reqif-opa-mcp`'s parsed requirement records
(via MCP call) and writes them into ledgrrr's own `ArtifactKind`/ontology
graph as the new `Requirement` variant — matching `holon-viz`'s "foreign
format in, Rust internal representation out" shape (opposite direction:
parse-in, not emit-out).

### LinkML

| Aspect | Finding |
|---|---|
| What it is | Python-based (Monarch Initiative project, `linkml.io`), YAML-authored schema language: classes + slots + enums, with inheritance and mixins. Positioned *between* schema languages (JSON Schema, Avro) and ontology languages (RDF/OWL, SHACL) — the "concrete data-shape" layer §2's table identifies as the actual gap next to UFO-types' more philosophical stereotypes. |
| Output formats | One schema compiles to JSON Schema, SHACL, ShEx, OWL, JSON-LD context, SQL DDL, Protobuf, GraphQL, OpenAPI, Pydantic, Java, TypeScript, **and Rust** — plus Excel/CSV/Pandera/TypeDB. |
| Rust angle | LinkML **does ship a Rust code generator** — authoring/compilation is Python-only, output can be Rust structs. Same shape of exception already accepted for KerML→Rust+TS codegen and for `sysml-v2-lsp` (TS runtime, Rust-first only at the boundary that matters). |
| Decision (§5 Q4) | **Spike before committing**: author one small schema (e.g. a `Requirement` class), run the Rust generator, judge the output. Not yet done — see §6 task 2. |
| Relationship to existing OWL2/UFO/KerML stack | Complementary, not competing. Proposed division of labor: **UFO-types** = philosophical/ontological category (Kind/Role/Relator/Mode); **KerML/SysML v2** = process/behavior metamodel (state machines, blocks, ports); **LinkML** = concrete field-level schema + validation for the domain nodes that live inside all of the above. |

## 4. sysgit.io — reference vibe, not a backlog (decided, §5 Q5)

sysgit.io is a real, close-fit product — a "Git-native platform for complex
hardware development," explicitly SysML v2-native, with git-as-substrate
model storage, a SysML v2 IDE, requirements management, PR-based model
review, CI/CD checks on model changes, DOORS/Jama document ingest, and
model-reuse libraries. It does **not** appear to cover decision/cost
tracking as first-class linked artifacts — that gap is the actual
differentiator ledgrrr is going for, not feature parity.

**Per §5 Q5, this section stays positioning context, not scoped backlog.**
The table below is kept for reference; none of its rows are tasks:

| sysgit.io feature | ledgrrr equivalent today | Gap (context only, not backlog) |
|---|---|---|
| Git-native model storage | The whole workspace already is a git repo; `ledger_ops.rs`'s composable `LedgerOperation` trait already models pipeline operations | No requirement/decision/cost operation kinds yet (§2) |
| SysML v2 IDE | `holon-viz`'s `SysmlV2Emitter` (emit-only) + Part 1's `sysml-v2-parser`/`sysml-v2-lsp` candidates (not yet spiked) | Full IDE experience depends on Part 1's parser/LSP work landing first |
| Reviews/PRs, CI/CD checks on model changes | Standard GitHub Actions on this repo already (`badges/ci.yml`) | Nothing ledgrrr-specific validates a *model* change today |
| Document Ingest (DOORS/Jama/spreadsheets) | `ledger-core`'s existing statement-ingest path (`ingest.rs`), tax-specific today | ReqIF import (§3) is the natural first non-tax ingest source |
| Model Reuse / MOSA | Nothing today | Not evidenced as a real need — flagged only |

## 5. Decisions (resolved 2026-08-22)

1. **`ArtifactKind`/`NodeType` widening: widen the core enums.** Requirement/
   Decision/Cost become first-class variants (not custom kinds via
   `ledgerr_ontology`) — chosen over the repo's own OpenMetadata precedent
   because these three are cross-vertical primitives, not a single
   integration's private vocabulary.
2. **`ZLayer`: new dedicated variant, not the existing proposed `Domain`
   layer.** Despite `Domain` (z=6, "ontological concepts": `TaxCategory`/
   `Jurisdiction`) being a structural match, Requirement/Decision/Cost get
   their own 9th variant so the systems-modeling vertical's content is
   independently toggleable/colorable from tax-domain content in the
   isometric renderer.
3. ~~ReqIF: accept the Python-wrap exception, or hold out for Rust?~~
   **Resolved 2026-08-22**: use `PromptExecution/reqif-opa-mcp` (first-party,
   ours to modify) rather than a third-party wrap.
4. **LinkML: spike first.** Author one small schema, run the Rust generator,
   judge the output — before deciding whether LinkML becomes the canonical
   schema layer or hand-written Rust structs stay the norm.
5. **sysgit.io: reference vibe only.** Not a feature-parity roadmap — §4's
   gap table is context, not backlog.
6. **`reqif-opa-mcp` integration, three parts, all resolved:**
   - **Shape**: wrap over MCP (loose coupling, two processes) — not ported
     to Rust.
   - **Canonical ledger**: `arc-kit-au`'s Blake3/petgraph chain stays
     canonical for decisions+costs. `reqif-opa-mcp`'s evidence-store/
     decision-logger supplies parsed requirements/evidence *into* that
     chain; it does not replace it.
   - **Policy engine**: OPA/Rego and Rhai coexist, one per domain — OPA
     stays scoped to compliance-gate/SARIF decisions where it already works,
     Rhai stays scoped to ledgrrr's classification/workflow rules. No
     consolidation.

## 6. Next concrete tasks

1. **Spike the `sysml-derive` proc-macro approach (§2a) before anything
   else** — scaffold a new crate, get a minimal `#[derive(SysmlBlock)]`
   walking one existing struct's AST (e.g. `arc-kit-au::Transaction`) to
   emit a SysML-v2 block definition text fragment. This is now the gating
   spike for tasks 2–3 below, not an independent nice-to-have.
2. Spike LinkML (decision 4) **in parallel with task 1, as a comparison, not
   in isolation**: author the same small `Requirement` schema, run LinkML's
   Rust generator, and judge its output against task 1's macro output —
   usability, maintenance burden, whether Python-authoring buys anything the
   macro doesn't.
3. Once 1–2 give a real comparison, widen `ArtifactKind`
   (`ledger-core/src/ontology.rs`) and `NodeType` (`arc-kit-au/src/node.rs`)
   with `Requirement`/`Decision`/`Cost` variants (decision 1), backed by
   structs annotated with whichever approach won task 1 vs. 2. Add the
   matching `OperationKind` variants (`RecordDecision`/`RecordCost`/
   `ImportRequirement`) in `ledger_ops.rs`. Retrofitting existing variants
   (`Transaction`/`TaxCategory`/etc.) onto the same macro is a tracked
   follow-on, not required to unblock this task.
4. Add the new dedicated `ZLayer` variant in `ledger-core/src/iso.rs`
   (decision 2) — naming, z-depth, and color TBD at implementation time;
   follow the existing `IsometricProjection`/`HasVisualization` pattern used
   by the other 6 (soon 8) variants.
5. Spike `reqif-opa-mcp` wrapped over MCP (decision 6): point it at its own
   sample derived baselines (`samples/standards/derived/
   {nist_ssdf_dogfood,owasp_asvs_cwe}.reqif`), call its parse/query tools
   from a throwaway Rust client via the `DatumType`/MCP pattern, and write
   the second-stage Rust converter that maps its requirement records into
   the new `ArtifactKind::Requirement` nodes from task 3.
6. Wire the new `Requirement`/`Decision`/`Cost` capability family into
   `crates/ledgerr-mcp/src/contract.rs` (the single source of truth the
   docs/runbook auto-generate from) once tasks 1–5 land.
7. **DVC integration remains untouched and out of scope for this doc** —
   same status as Part 1 left it: a separate tool line (data-source
   tracking) needing its own scoping pass.

## 7. Explicitly not done in this pass

- **`README.md`'s tax-accounting framing is not corrected in this doc.** §1
  documents that it's wrong and why, but rewriting the System Thesis/primary
  description is a separate, standalone edit — not bundled into this
  research-and-design pass. Worth doing as its own small follow-up once
  ready to touch the README (note: the README's own "Humble Beginnings"
  section already articulates something close to the corrected framing
  further down the page — the fix is largely promoting that framing to the
  top, not inventing new language).
