# ledgrrr Integration Roadmap

**Status:** living document. Written 2026-07-25 as the output of an architecture-integration audit
requested against the vision: *ledgrrr as a smart, auditable MCP gateway that records agent tool
use and enforces business-process constraints via constraint generation; a swiss-army-knife
toolkit with a tray, a Claude `.mcpb` extension, and a shared background server that the tray and
MCP surfaces both connect through; a CLIF-based diagram/process engine with isometric
visualization and symbolic iconography; deterministic, composable pipeline execution instead of
LLM-improvised command sequences.*

This document records what the audit found (with exact file:line evidence), and breaks the gap
between "current state" and "vision" into independent subsystems, each with its own plan file so
each can ship and be tested on its own. Do not treat this as one monolithic project — the five
subsystems below have almost no shared code and can be built/reviewed in parallel once the first
is done.

## Audit findings (verified against code, not docs)

1. **The MCP gateway does not audit tool calls today.** `crates/ledgerr-mcp/src/bin/ledgerr-mcp-server.rs`
   builds *two* independent `TurboLedgerService` instances in `build_service()` (line ~269): one is
   wrapped by `service.spawn_actor()` (the "new" actor/gate dispatch system, per
   `mcp_adapter.rs:1-10`'s own header comment: *"New code should route through `actor::ServiceHandle`
   instead... replaced by the actor/gate channel system in PRD-7 Phase 0-4"*), the other is boxed,
   leaked, and used as `global_raw_service()`. The `"tools/call"` match in `handle_request` (line
   ~86) dispatches **exclusively** through `mcp_adapter::handle_*_tool(global_raw_service(), ...)`
   — the legacy direct-call path. The actor/gate instance and its spawned thread are constructed
   and then never used again. `legacy` is in `ledgerr-mcp`'s `default` feature set
   (`Cargo.toml:38`), so this isn't a config mistake — it is the only path that currently compiles
   into the shipped binary. Net effect: there is no single choke point today through which every
   tool call passes, which is the prerequisite for "records agent tool use."
2. **No constraint/policy enforcement exists at the gateway layer.** The Kasuari/Z3 "legal
   intelligence" layer (PRD-7 Phase 0) is a domain-specific tax-rule solver (AU GST/FBT, US
   Schedule C/FBAR/FEIE) invoked from within specific `ledgerr_tax` operations — it does not gate
   *which tools an agent may call, in what order, or with what arguments* at the transport layer.
   "Constraint generation enforcement of business processes" as a cross-cutting gateway concern is
   0% built.
3. **Tool-contract documentation has already drifted once**, which is exactly the kind of
   inconsistency a consolidated control plane is supposed to prevent: `AGENTS.md:44` and
   `AGENTS.md:292` say the default catalog is 8 top-level tools; `crates/ledgerr-mcp/src/contract.rs:58`
   publishes `PUBLISHED_TOOLS: [ToolContractSpec; 9]`, i.e. `ledgerr_evidence` shipped without the
   docs being updated.
4. **CLIF does not exist anywhere in this repository.** `grep -ril clif` across `*.rs *.md *.toml`
   returns zero hits. No parser, AST, serializer, or logical-form-to-diagram lowering exists. The
   diagram system that does exist (`crates/ledgerr-desktop-agent/src/render.rs`, the workflow-TOML
   compiler referenced in `AGENTS.md`, `mdbook-rhai-mermaid`) produces Mermaid from Rhai-FSM/TOML
   workflow definitions — a different, narrower input format than CLIF. Isometric rendering exists
   as a docs-editor feature (`book/` live-sync tooling) but is not connected to a general symbolic
   process-diagram authoring pipeline.
5. **Three separate desktop/control surfaces exist and do not share one background server:**
   `ledgerr-tauri` (current primary shell, native win32 tray via `crates/ledgerr-host/src/tray/native.rs`
   wired in through `crates/ledgerr-tauri/src/tray.rs`), `ledgerr-host`'s legacy Slint
   `host-tray`/`host-window` binaries (still built via the `wsl2-pwsh-*` Justfile recipes), and
   `ledgerr-desktop-agent`'s `ledgrrr-service`/`ledgrrr-mcp` controller (PRD-10 Phase 1, its own
   stdio MCP server). None of the three currently proxy through a shared background server process
   with its own MCP client/server fan-out, as the vision describes.
6. **`LedgerOperation` dispatcher (`crates/ledger-core/src/ledger_ops.rs`) is a real trait with a
   real dispatcher, but the operations that matter most — `IngestStatementOp`, `ClassifyTransactionsOp`,
   and others — return `Err(LedgerOpError::NotImplemented(...))` today** (confirmed at
   `ledger_ops.rs:342,372,499,572,608`, and asserted as *current, expected* behavior by
   `integration_tests.rs:105-110,178-182`). "Deterministic execution as a composable pipeline of
   steps" has the right interface shape already; most steps behind it are stubs.

## Subsystems (independent plans)

| # | Subsystem | Plan file | Why this order |
|---|-----------|-----------|-----------------|
| 1 | **Gateway call audit log** — single choke point in `ledgerr-mcp-server.rs` records every `tools/call`, persisted locally, queryable via `ledgerr_audit`. | `2026-07-25-mcp-gateway-audit-log.md` — **shipped** (`gateway_audit.rs`, wired into dispatch, e2e-tested) | Smallest, most tractable, and it is the literal foundation the user asked for first ("records agent tool use"). Every later constraint-enforcement gate hooks into this same choke point, so building it first avoids rework. |
| 2 | **Gateway policy gate** — a `GatewayPolicy` trait evaluated pre-dispatch at the same choke point (allow/deny), seeded with a real sequencing constraint (`ledgerr_reconciliation/commit` requires a prior successful `validate`). | **shipped** (`gateway_policy.rs`, e2e-tested) | Note (2026-07-25, post-ship): the operator clarified "constraint generation" in the original ask was actually **"constrained generation"** — the xgrammar/grammar-constrained-decoding sense (forcing LLM token generation to conform to a formal grammar), not this after-the-fact policy gate. This subsystem remains legitimate as defense-in-depth (catches anything that reaches the gateway regardless of how it was generated) but is a different mechanism than what "constrained generation" now refers to — see issue #116. |
| 3 | **Actor/legacy dispatch consolidation** — retire the orphaned second `TurboLedgerService` instance; route `handle_request` through `ServiceHandle` (the actor) so there is exactly one live service instance. | **partially shipped**: dead actor-spawn removed, misleading module doc corrected. Remainder (routing all 28 `mcp_adapter` handlers through the actor, or retiring `actor`/`gate` instead) filed as issue #119 — real regression risk, needs its own TDD-per-handler plan. | Housekeeping that makes subsystems 1 and 2 architecturally honest instead of a second parallel wrapper. The waste (idle thread, unused service instance) was safe to fix immediately; the larger dispatch migration is not. |
| 4 | **Shared background server + tray/MCP proxy unification** — pick one of the three existing surfaces as the long-lived background process and make `ledgerr-tauri`'s tray and any Claude-facing `.mcpb` controller talk to it instead of each owning independent state. | `2026-08-06-unify-desktop-background-server.md` — **Phase A complete** (issue #118): new `ledgrrr-settings` crate is the shared settings model, an HTTP settings server (`ledgrrr-service`, `crates/ledgerr-desktop-agent/src/bin/ledgrrr-service.rs`) is the long-lived background process, and `host-tauri`/`host-tray` were switched to HTTP clients of it instead of each owning local state. Verified with `cargo check`/`test`/`clippy` on the non-Windows-only surfaces plus a live `/settings` smoke test. **Caveat: the Windows-only surfaces** (`host-tauri`'s `state.rs`/`commands.rs`, `host-tray`'s `tray/runtime.rs`) **are implemented but compiler-unverified** — this Linux/WSL environment cfg-strips them before typecheck, so they've only had manual/static code review, never a compiler or test pass; a real Windows build is required before merge for compiler-backed confidence in that portion. | Large, cross-crate, UI-visible change; a genuine architecture decision (which of the 3 surfaces is the survivor), not a mechanical fix. Operator direction 2026-07-25: defer, not a "next task" priority right now. |
| 5 | **CLIF diagram/process engine** — CLIF is confirmed to mean the literal ISO/IEC 24707 standard (not an informal DSL). Net-new: CLIF parser/AST in Rust, lowering to a diagram IR and to an executable pipeline, isometric renderer hookup, symbolic icon set per operation family. | **backlogged** — issues #114 (CLIF AST/interpreter), #115 (Rhai vs Monty, deferred as options not a decision), #116 (constrained generation / xgrammar), #117 (RDF/triple-store + semantic vector search cognitive-assistance layer, the longer-range destination this is heading toward) | Biggest, most novel, longest-lead subsystem — explicitly deferred by the operator 2026-07-25 ("we don't need to solve the CLIF today; put all this on the backlog in gh issues"). Do not resume without the operator opening one of these issues as the next task. |
| 6 | **`LedgerOperation` NotImplemented burn-down** — originally scoped as "wire the stubs to already-working implementations." | **backlogged, and rescoped** — issue #119. Investigation found `OperationDispatcher`/`LedgerOperation` have **zero production callers** anywhere in the workspace (only self-referential in `ledger_ops.rs`'s own tests). `ClassifyTransactionsOp`'s stub also references a "ledger store" that doesn't exist as a type in `ledger-core` — filling the stub body would mean inventing an unvalidated storage abstraction, not plumbing to existing tested code as originally assumed. | Corrected finding, not just deferred: this needs a decision (does the calendar-scheduled-operation abstraction become a live trigger path, or stay a documented-intent skeleton) before any stub-filling, not just engineering time. |

## Status as of 2026-07-25 (session end)

Subsystems 1–2 shipped and tested (7 commits, all `cargo test -p ledgerr-mcp --all-targets --all-features` green throughout). Subsystem 3's safe half shipped. Subsystems 3's remainder, 4, 5, and 6 are filed as GitHub issues #114–120 per explicit operator direction to stop implementation work and backlog the rest — this was a deliberate scope narrowing mid-session, not an assessment that the work is unimportant. Resume from the relevant issue, not from this roadmap's original "not yet written" framing.

## Working agreement for this effort

- Each plan file is independently executable and independently testable — per `writing-plans`'
  scope check, do not merge these into one giant plan.
- Every task closes with the project's evidence-line convention: run the declared test/verification
  command and paste the `PASS`/`FAIL` output verbatim before marking a task done (see root
  `CLAUDE.md` "Trace-or-filler").
- No `unwrap`/unchecked indexing in any new financial-path code (root `CLAUDE.md` Safety Bar).
- Subsystem 4 (control-plane unification) and subsystem 5 (CLIF) are architecture decisions, not
  mechanical fixes — brainstorm with the user before writing their task plans.
