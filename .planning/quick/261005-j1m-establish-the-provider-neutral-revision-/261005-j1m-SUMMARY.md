---
phase: quick-261005-j1m
plan: "01"
subsystem: revision-io
tags: [rust, sqlite, revision, authorization, recovery]
dependency-graph:
  requires: [canonical-ufo-revision-contract, revision-authority-adr]
  provides: [durable-artifacts, immutable-intake, project-grants, guarded-receipts, recoverable-index-identity]
  affects: [native-provider-adapter, graph-index-publisher]
tech-stack:
  added: [rusqlite-0.33.0-bundled]
  patterns: [immediate-transactions, immutable-content-addressed-blobs, authenticated-host-boundary]
key-files:
  created: [crates/ledgrrr-revision-io/src/lib.rs, crates/ledgrrr-revision-io/src/store.rs, crates/ledgrrr-revision-io/src/schema.sql, crates/ledgrrr-revision-io/tests/durable_store.rs, book/src/revision-io.md]
  modified: [Cargo.toml, Cargo.lock, Justfile, AGENTS.md, book/src/SUMMARY.md]
decisions:
  - All canonical ufo-types consumers share one published exact Git pin.
  - Ambiguous dispatch is never released by expiry or automatically resent.
  - Index completion is a trusted publisher capability and does not publish graphs or move branch pointers.
metrics:
  completed: 2026-10-05
  tasks-completed: 3
---

# Quick 261005-j1m: Durable revision I/O owner

SQLite now durably stores authorized portable proposal bytes, immutable operation identities, project grants and generation/fence-guarded recovery receipts across restart and independent connections.

## Task commits

| Task | Commit | Result |
|---|---|---|
| 1 | `83e5617` | New provider-neutral library and public owner contracts; one canonical upstream pin across four consumers; delegated thin Justfile recipes |
| 2 RED | `c510de1` | Committed JSON-fixture storage tests fail because Store does not exist |
| 2 GREEN | `1134dfb` | Immediate transactional SQLite schema, explicit project bootstrap/grants, immutable original/canonical artifacts and intake, bounded authorized recovery |
| 3 RED | `cadcdc2` | Recovery tests fail because dispatch/transition/publisher APIs do not exist |
| 3 GREEN | `50a9560` | Durable dispatch reservation, actual commit evidence, deterministic index identity, trusted publisher checks, corruption/interruption tests and executable operational documentation |

All implementation commits are on `feature/sysml-discovery-revisions`, based on `c83464b`. No protected reference, nested worktree, submodule path, ROADMAP or unrelated checkout was changed. The final task metadata commit is separate from the implementation commits above.

## Verification

- `cargo metadata --format-version 1 --no-deps`: passed; output `/tmp/sysml-implementation/ledgrrr-task1-metadata.json`.
- `cargo check -p ledgrrr-revision-io`: passed (`ledgrrr-task1-check.log`).
- `cargo check -p ledger-core -p ledgerr-mcp -p holon-viz --locked`: passed (`ledgrrr-final-consumer-check.log`). Two pre-existing MCP warnings and a dependency future-incompatibility notice remain; no consumer migration error.
- `just revision-io-test`: passed 12 integration tests (including the subprocess helper) and 1 executable book doctest (`ledgrrr-task3-final-test.log`). The fixture contains two requirements, typed source/controller facts, state relationships, Unicode identifiers, source anchors, BOM/CRLF source bytes, binary attachment bytes, actors and mutation cases.
- `just revision-io-check`: passed all-target crate Clippy with `-D warnings` (`ledgrrr-task3-final-clippy.log`).
- Scoped `rustfmt --edition 2021 --check`: passed for the new library, store and integration tests. `git diff --check`: passed.
- Manifest/lock inspection: exactly one ufo-types package and exact published revision `e900190e4be8b3380c9f4a44be0c85dda4f14b47`; all four consumers inherit it. Only the new consumer enables revision; holon-viz retains sysml as a dev dependency. No local substitute pin or unrelated existing-package upgrade.
- SQLite dependency is exact `rusqlite =0.33.0` with bundled `libsqlite3-sys 0.31.0`. Writes use immediate transactions, parameterized SQL, checked foreign keys/WAL/FULL settings and a 3-second busy timeout.
- Before/after process interruption: a killed child leaves no committed partial artifact transaction; another killed child after completed public intake leaves its exact durable envelope retrievable. SQL-trigger failure independently proves public intake rollback after artifact insertion. This does not claim every individual statement was interrupted or hardware power-loss testing.
- Independent connection races prove one immutable intake identity and one active branch dispatch. Restart preserves Ambiguous and blocks subsequent dispatch. Tests reject stale generations/fences, contradictory checkpoint identities, Pending-to-Indexed skips, wrong provider/project/head/proposal evidence, unauthorized or revoked grants, corrupt bytes/records and future/unrelated schemas. Older index completion does not release newer dispatch.
- Standing post-commit reference check used `https://code.claude.com/docs/en/plugins` after each commit. No MCP implementation/catalog/generated-doc file changed. Graph inspection confirms `PUBLISHED_TOOLS` has 14 capability families; existing canonical MCP documentation advertises 14 and required action fields. No new MCP surface or documentation drift was introduced. Historical AGENTS references to older counts are pre-existing.

## Operational APIs for dependent adapters

The host supplies authenticated `ActorId`; credentials never enter proposal data. Bootstrap is a privileged host provisioning path, not a client-selected actor endpoint.

- Host provisioning: `Store::open`, `bootstrap_project`, `register_branch`, `set_grant`.
- Proposal/read adapter: `intake`, `operation`, `artifact`, `project_binding`, `remote_branch`, `recovery` (bounded pages, cursor is last operation ID).
- Trusted provider worker: `start_dispatch` before sending a request, `mark_ambiguous` for uncertain outcomes, `record_commit` with independently obtained exact `CommitEvidence`, `reject_before_dispatch` for known Conflict/Unavailable before any request.
- Trusted index worker: read the exact canonical artifact and actual accepted revision from recovery, build/validate/publish the complete external graph, then obtain `publisher` and call `IndexPublisher::record_index` with exact checkpoint evidence.

`StoredOperation` includes canonical upstream receipt, generation, original/canonical artifact digests, dispatch token/evidence, actual provider revision, complete commit evidence and deterministic indexing-work identity. Source/attachment BLOBs are retrievable only through project grants and references. Intake retry preserves the first original envelope and returns the current receipt.

There is no generic receipt setter, graph publication, branch checkpoint pointer, lease expiry, force-success or automatic resend API. A supplied checkpoint proves only store identity/preconditions; graph completeness remains the publisher's independently verified obligation. Matching provider evidence can resolve ambiguous dispatch, but this store cannot itself prove a remote observation. It does not fence a remote request on a provider accepting unconditional writes.

## Deviations and limitations

- The required architecture-first delegation assigned independent Justfile work to a child agent; its list/dry-run checks passed. Rust/Cargo/tests/docs stayed with this executor.
- Rust 2021 temporary query lifetimes were corrected during GREEN compilation. Reload additionally checks complete commit/index work identity and proposal-parent consistency. These are direct correctness safeguards, not new architecture.
- The existing isolated feature worktree was retained with `workflow.use_worktrees=false` as explicitly directed. No GSD worktree-agent namespace or cleanup was introduced.
- No authentication gate occurred. No stub blocks this storage milestone. No network endpoint was introduced. Raw database file writers remain trusted host processes; local SQLite is not distributed authority.
- The schema is versioned and fail-closed; no migration is supplied or inferred. Schema/model dialect expansion requires explicit future migrations and tests.

## Remaining full-plan gates

P0 kr0ki registration, lockfile reconciliation and live adapter checks remain. P1 native/ReqIF codecs, browser contract generation/integration, native persisted envelope mapping and full byte fidelity remain. P2 native derivation mapping, semantic/library resolution and fetch–emit–fetch equivalence remain. P3 private provider endpoint/all mutation routes, actual serialized remote promotion, outcome reconciliation, external head polling, live two-client disjoint merge/conflict and end-to-end grants remain. P4 complete immutable staged Oxigraph projection, fenced atomic checkpoint publication, separate branch pointer, exact/min-revision bounded authorized queries, freshness, index crash/replay/rebuild and graph digest evidence remain. P5 integrated source/SysML/verification navigation and proposal/conflict/fidelity/unknown/pending UI inspection remain.

All seven end-to-end slice gates from the root plan remain unverified by this storage milestone. The native reference server's stale `previousCommit` acceptance is retained as a capability failure; this milestone does not establish server CAS, accepted owner-backed model commits, live graph checkpoints or completed UX.

## Self-Check: PASSED

All five implementation/test commits exist, all named source/fixture/documentation artifacts exist, final crate tests and Clippy passed, and the dependency/lock check matches the published canonical pin. Planning metadata completion preserves existing Phase 19–21 roadmap and progress.
