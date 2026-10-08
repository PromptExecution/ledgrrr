---
phase: quick-261006-c9d
plan: "01"
subsystem: revision-owner
tags: [sysml, sqlite, native-codec, concurrency, recovery]
requires: [quick-261005-j1m, ufo-types-ba12e5e]
provides: [physical-authority-fencing, native-supported-roundtrip, private-owner-promotion]
affects: [sysml-discovery-P1, sysml-discovery-P2, sysml-discovery-P3, sysml-discovery-P4]
tech-stack:
  added: [reqwest-0.12.28, axum-0.7.9]
  patterns: [immutable-preparation, single-send-authorization, reachable-history-reconciliation]
key-files:
  created: [crates/ledgrrr-sysml-adapter, scripts/sysml-owner-runtime.sh, scripts/sysml-owner-live-probe.py, book/src/sysml-revision-adapter.md]
  modified: [crates/ledgrrr-revision-io, Cargo.toml, Cargo.lock, Justfile]
key-decisions:
  - Physical provider/project uniqueness excludes dialect.
  - Accepted candidate and actual provider revision remain separate from immutable intake.
  - Native FeatureTyping uses published canonical types rather than generic specialization coercion.
  - Ambiguous dispatched writes never expire or authorize resend.
completed: 2026-10-07
duration: interrupted execution across 2026-10-06 and 2026-10-07
---

# Quick 261006-c9d: native supported codec and private revision owner

Durable physical authority, immutable rebased preparation and bounded authenticated promotion now integrate with the pinned native provider. This completes the quick milestone's supported slice, not the full P0–P5 objective.

## Commits and work

| Task | Commits | Result |
|---|---|---|
| 1: physical authority and preparation | `373feb8` RED; `8527d56` GREEN | Issue [#250](https://github.com/PromptExecution/ledgrrr/issues/250): dialect/project aliases cannot obtain independent physical writers; explicit v1→v2 migration preserves receipts/artifacts and unresolved dispatches. |
| 2: codec and bounded transport | `5edb079` RED; `a195518` GREEN; `d4d4a8a`; `5d424f5` | Deterministic UUID/opaque identity mapping; strict envelopes/native fields, duplicate-key rejection, bounded complete snapshot/history reads and typed fidelity refusal. |
| 3: real owner integration | `14a19bc` | Reservation before head read, upstream merge/conflicts, accepted candidate/index identity, one-time send authorization, observed no-op, authenticated owner API and separate private runtime. |
| Final verification | `11fd22e` | Actual Store::open migration subprocess killed after row writes and before commit; rollback/reopen preserves original artifacts, ambiguous intent and branch blocking. Hook is compiled only in the unit-test binary. Scoped formatting corrected. |

The native codec and runtime/scripts were delegated with separate file ownership. Their interrupted work was recovered from committed source and terminal evidence; no completed implementation was restarted. Task 1/2 have RED commits. Task 3 has failing live integration observations and GREEN evidence, but no separate RED-only commit.

The single published canonical pin is `ba12e5e2baa79dbfaf330a6b634ac6e3ae1e5ea1` (ufo-types 0.16.0). Upstream issue #33 preceded its FeatureTyping extension. No consumer semantic enum fork, local production path patch or unrelated dependency upgrade is present.

## Final verification

All Rust commands use `CARGO_TARGET_DIR=/tmp/sysml-implementation/ledgrrr-target CARGO_BUILD_JOBS=2`.

| Command | Terminal result / evidence |
|---|---|
| `just sysml-revision-test` | Exit 0; 2 storage unit tests, 17 storage integration tests, 17 codec/client tests, 1 real loopback owner-route test and 1 storage doctest. Log: `/tmp/sysml-implementation/c9d-final-test.log`. |
| `cargo test -p ledgrrr-revision-io --lib killed_actual_v1_upgrade --locked` | Exit 0; actual interrupted migration rollback/reopen. Log: `c9d-migration-crash.log` in the same directory. |
| `just sysml-revision-check` | Exit 0, all-target Clippy `-D warnings`; `c9d-final-clippy.log`. |
| `cargo check -p ledger-core -p ledgerr-mcp -p holon-viz --locked` | Exit 0; existing warnings remain; `c9d-final-consumers.log`. |
| Scoped `rustfmt --edition 2021 --check` over both crates' changed Rust files | Exit 0; `c9d-final-fmt.log`. |
| `git diff --check` | Exit 0. |

The two standalone `live_roundtrip` tests return early when SYSML_API_URL is absent. Their ordinary test-run success is **not live evidence**. They were not rerun against the retained baseline. Actual supported fetch→emit→fetch evidence comes from the owner report below.

Standing official plugin documentation check: `https://code.claude.com/docs/en/plugins` checked after implementation commits and again after `11fd22e`; these changes introduce no MCP catalog/argument or plugin/skill surface change. No generated MCP contract update was needed.

## Authoritative real HTTP evidence

The preserved post-pin terminal report is `/tmp/sysml-p3-probe-result.json`: **Satisfied, all 57 recorded gates Satisfied**, produced by `scripts/sysml-owner-live-probe.py` (recipe: `just sysml-owner-live`; explicit report output can be selected with `--output /tmp/sysml-p3-probe-result.json`). SHA-256: `b03c1d233f7b60a457f429d9025e606ab46790afc929bee5d14f69bb57fab254`.

On resumption, running containers were inspected before any restart decision. The preserved report's owner binary SHA-256 `ca4a59af5f1056588b26bd5b0b63475f295f97593db4455b25745e0b1fd3f766` matches `/tmp/sysml-owner-c9d/revision-owner`. Final source changes are test-only migration instrumentation and formatting; no native runtime restart or repeated write probe was necessary. Earlier reports `sysml-owner-c9d-live-report.json` and `sysml-owner-c9d-live-before-empty-report.json` are older evidence and must not replace the post-pin report.

Measured results include:

- Actual fetched RequirementDefinition/Usage, VerificationCaseDefinition/Usage, PartDefinition/Usage, SatisfyRequirementUsage, AllocationUsage and three FeatureTyping rows. Requirement/verification definition references and FeatureTyping endpoints are singletons; PartUsage.partDefinition is an array. Verification's actual verifiedRequirement field is checked rather than fabricating an independent Verify row.
- Original submitted envelope formatting, opaque Unicode IDs, CRLF/BOM source and binary attachment bytes remain retrievable with project authorization. Full-envelope no-op binds the existing revision with native commit count **25→25**. Equal model semantics with changed context create a new commit.
- Two authenticated editors retain both independent edits. The rebased operation retains submitted digest `sha256:f8d3ae7cb5283f4813df1e8a265e43bd63c2af7a7fdf724d1bc77bc9655e3298`, while accepted candidate digest is `sha256:4ffc971e5c57d45979b5fc452eb2ad5bcb2c7f97598055e5aebfefc2e919d875`; actual promotion parent is `6b78c10b-8335-4b4a-9102-60281867a2f2`. Conflicting edits return an upstream typed Conflict artifact without a commit.
- Unsupported ActionDefinition publication returns fidelity failure before remote creation. Fresh-project hydration preserves the semantic model and original bytes. Deleting all managed elements retains exactly one Package envelope anchor and hydrates an empty model.
- Native and owner persistence restarts preserve exact revision/receipt identity. Process exit 86 after native acceptance, and separately an explicit ambiguous response, both reconcile after restart through complete bound branch history with one remote commit and no duplicate dispatch. Repeated ambiguous promotion stays blocked.
- Authentication, project/artifact authorization and client actor spoofing checks pass. All 36 native mutation-route/prefix probes return 404. Startup refuses a different owner database in the fixed task topology.

| Observation | Actual provider revision |
|---|---|
| First supported acceptance / observed no-op | `a546541c-9e8a-4a65-9d9c-8bf74aeb154f` |
| Disjoint edit B / accepted parent of rebased A | `6b78c10b-8335-4b4a-9102-60281867a2f2` |
| Rebased edit A | `c54d70b2-5227-412f-82b0-78b76ba1a11f` |
| Crash reconciliation | `16408b8b-6ec7-4e89-8045-653344d916fa` |
| Ambiguous reconciliation | `addd34ae-2929-4115-93a2-8c13cc009443` |
| Fresh-project hydration | `0f6d3305-c61d-494a-8458-a4f421a2b38b` |
| Deleted-all anchor | `ec2ea27b-4b74-487f-a8ca-2090bc64eb9c` |

Native provider projects: `fc540735-660f-4ab3-a310-834aeaf19ead` (live) and `07215f58-dcc7-42ad-ab3d-4157dc0b940c` (fresh). The report contains complete receipts, candidate/projection/identity/index digests, actual parent chain, typed conflicts, native snapshots and artifact bytes.

## Runtime and pinned inputs

Separate rootless pod `pex-sysml-owner-c9d` publishes only owner `127.0.0.1:19001`. Native HTTP and PostgreSQL bind pod-local loopback. Kernel socket evidence confirms the native listener; host and unrelated-container connections to observed IPv4/IPv6 addresses fail with curl exit 7. This is actual namespace isolation, not a configuration boolean. Host/Podman/database administrators remain trusted. Independent owner databases/topologies are unsupported; SQLite is not distributed authority.

Official source: `0af711b14bbcea7b240bb0a3a65817ae68302092`.
Java image: `sha256:8db2bcf62ae171d1247c331db0410d8bc6347e7493bdafe104bb9a33d59c1291`.
PostgreSQL image: `sha256:1a6ab3f5345eb6dbe04a1349529caabdb0ab09293a09590fad07b2246bfa4b54`.
Task copy stage SHA-256: `e5283d852bf8cabbf25aab50b6ed6123749b71da2821e90ff8b13885422c0208`.
Copied persistence configuration SHA-256: `e2a60b8a28576e37787d4947e444226ac054212346d2e854bf46c2534fe48e3e`; explicit `hibernate.hbm2ddl.auto=update`.
Native fixture SHA-256: `068e1770817dd46d11962d8196910e9a3573d73703c02a3937ca27b4733b842c`.
Operation fixture SHA-256: `b1f2e78ce2b4a544203fbe071ae4466095f4560e4de3ff82ca77c067a8de4a9c`.

The retained `pex-sysml-reference` pod remains untouched; its create-drop setting makes restart destructive. Credentials/configuration/database/generated reports remain outside Git.

## Deviations, limits and remaining full-plan gates

Missing canonical FeatureTyping was handled issue-first upstream, followed by one exact published pin. Generic Specialization is refused rather than coerced. Native properties, compiler/source evidence and verified-machine evidence retained in an envelope do not establish native property evaluation, action/state behavior or derivation semantics. Fixture library/compiler/parser labels are retained evidence only: `ScalarValues=git:library-1`, `rustc-1.98.0` and `sysml-v2-parser-0.54.0` are not resolved native library/parser proof.

The backend still ignores stale previousCommit; no unrestricted backend CAS or multi-writer claim is made. Dispatched ambiguity does not expire. Commit titles mentioning “P2” or “P3” and the 57-gate count describe this supported local slice, not completion of those full work packages.

P0 dependent health/registration/lockfile release checks remain. P1 complete source/ReqIF adapters and browser integration remain. P2 pinned resolved derivation library, broader parts/ports/connections, declared action/state behavior and full semantic resolution remain. P3 complete client/deployment integration remains beyond this isolated topology. P4 complete immutable staged graph publication, authorized exact/min-revision queries, freshness, crash/replay/rebuild and graph digest equivalence remain. P5 interactive authoring/discovery/proposals/navigation and UI conflict/fidelity/unknown/pending inspection remain.

All seven root slice gates remain individually required: gates 1/2/4/5 advance only for the supported native/owner scope demonstrated here; gate 3 revision queries, gate 6 index publication/rebuild and gate 7 UI inspection remain unproven. ModelCommitted and accepted index-work identity do not establish Indexed. No unfinished behavior is represented by a successful fabricated native class or graph checkpoint.

## Self-Check: PASSED

Implementation commits and named files exist; final Rust tests/Clippy/consumer checks/format/diff checks passed. The authoritative report has 57 Satisfied gates and the exact published canonical pin/runtime binary digest. Only PLAN/SUMMARY/STATE metadata is finalized here; connector Phase 19–21 progress and ROADMAP are preserved.

