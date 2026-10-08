---
phase: quick-261007-ftn
plan: "01"
subsystem: revision-index-owner
status: incomplete-live-verification
requires: [quick-261006-c9d, ufo-types-5f8d018]
provides: [durable-sealed-index-jobs, deterministic-rdf-projection, bounded-revision-query-owner]
completed: false
---

# Quick 261007-ftn: durable revision-indexed discovery

The owner implementation is committed on `feature/sysml-discovery-revisions` and passes local storage, projection, query, route and compatibility checks. The required full private live probe has not passed. This quick, full P4 and the original P0–P5/all-seven-gate objective remain incomplete.

## Implementation commits

| Task | Commits | Evidence |
|---|---|---|
| Durable accepted manifests and index jobs | `39640d1` RED; `1a02d39` implementation | Accepted candidate plus actual provider revision; preserving v1→v2→v3 migrations; monotonic worker fences, bounded retries, immutable sealing, atomic checkpoint/Indexed receipt/eligible pointer publication, authorization and rebuild. Actual subprocess migration/publication interruption tests pass. |
| Complete projection and bounded evaluator | `9cbe428` | Typed semantic triples plus a complete typed canonical field tree; deterministic versioned NQuads, original provenance/anchors/context and empty metadata; SELECT/ASK parser preflight and supervised query workers. |
| Runtime, owner routes and integration | `b9ce3ed`, `0ce30a1`, `6d9dc6b`, `344070b`, `bdad06a`, `daaa19b`, `c3b3739` | Authenticated exact/minimum/current routes; native head observation, background worker and trusted recovery CLI; real live probe and documentation. Final probe budgets semantic queries at 2000 ms instead of an insufficient 250 ms. Live GREEN remains missing. |

The parent owns Cargo/storage/server integration, the query child owns projection/query files, and the runtime child owns scripts/recipes/operational documentation. Prior mandatory Explore and graph discovery preceded implementation. The only workspace canonical pin is published `5f8d0186947da376cfa17ad8b59cd280f2f85338`; no public contract forks or production path patches were added.

Oxigraph 0.5.11 uses isolated memory stores with default features disabled. A reproduced spareval 0.2.7/md-5 0.11 Digest incompatibility required the bounded lockfile pin md-5 0.10.6 ([issue 251](https://github.com/PromptExecution/ledgrrr/issues/251)). Real expensive VALUES-only Cartesian COUNT/filter tests showed cooperative cancellation cannot interrupt all pre-first-row work ([issue 252](https://github.com/PromptExecution/ledgrrr/issues/252)). The approved implementation uses a supervised same-binary worker: safe mature rlimit sets 1 GiB address-space and 6-second CPU ceilings before input/artifact loading; bounded IPC, total deadline, cancellation, forced termination and awaited reaping retain permits until worker exit. Original expensive cases remain covered, alongside actual cooperative engine cancellation and disconnect/output/resource limits. No sentinel triples or query rewrites manufacture cancellation checkpoints.

## Local verification

All Cargo commands use `CARGO_TARGET_DIR=/tmp/sysml-implementation/ledgrrr-target CARGO_BUILD_JOBS=2`.

| Command | Terminal result |
|---|---|
| `just sysml-revision-test` | Exit 0: 60 tests and 1 doctest. Log `/tmp/sysml-implementation/ftn-revision-final.log`, SHA256 `dc2d99409d81d6a6b1e090a978def012aa5ef2d1b91079857f4988edb2d9af92`. The standalone live test has no configured provider; ordinary suite success is not native live evidence. |
| `just sysml-index-test` | Exit 0: 6 durable-index tests, 3 projection tests, 7 query tests and 1 real loopback route test. Log `/tmp/sysml-implementation/ftn-index-tests-final.log`. The initial sandbox-only listener PermissionDenied was rerun successfully with local listener permission. |
| `just sysml-index-check` | Exit 0, all-target Clippy `-D warnings`. Log `/tmp/sysml-implementation/ftn-index-clippy.log`, SHA256 `8de17a605d04230b1a98560701710249693a59ea0da496a537b5f88a2ea076db`. |
| `just sysml-revision-check` | Exit 0, same required all-target checks. Log `/tmp/sysml-implementation/ftn-revision-clippy-final.log`, SHA256 `a3d35cf86ceab5e14fdf07a1a92436fd80899fcb42553381991a69f16b14dc6e`. |
| `cargo check -p ledger-core -p ledgerr-mcp -p holon-viz --locked` | Exit 0, existing warnings. Log `/tmp/sysml-implementation/ftn-consumer-compat.log`, SHA256 `dfc8e181b79fffd72ffc88e57faf648971334c424495ce98abef5568ddf657df`. |

Child final projection/query evidence: `/tmp/sysml-implementation/p4-query-process-final2-tests.log` SHA256 `41f5fd6f7d6e04a19e3177e007ac3c4bc0783588b7e7fd561fc401f9118ba5c3`; Clippy log `p4-query-process-clippy3.log` SHA256 `1e57fc2fceac5511b600e3348d4d7efa48c0624bd61d024f4a8193dc4eea8632`. These include actual worker limit inspection, expensive evaluation termination/reaping and capacity recovery. Official plugin documentation was checked after implementation commits; MCP surface is unchanged. The final exact-selector head-observation behavior matches the verified committed implementation; an unattributed temporary skip patch was preserved outside Git and corrected because it would report stale model-head freshness. The existing loopback query-boundary test passed again (exit 0; `/tmp/sysml-implementation/ftn-exact-observation-route.log`), and scoped rustfmt/diff checks passed.

## Actual live checkpoint: failed, not GREEN

Report `/tmp/sysml-index-c9d-live-report.json` is terminal exit 1, status **Violated**, SHA256 `206bfd2b3c8e61d263f970633dd3940b33fa4e6b250919f6188f9c016db3c220`. It binds implementation `daaa19b4dcbdf74db923ff544f2ffdbed0adac98`, binary SHA256 `b0cf0e2bebb1b519a84b48ff1fd8eb87b7506ca02813cb9f1e4875e5938d566f`, and native fixture SHA256 `068e1770817dd46d11962d8196910e9a3573d73703c02a3937ca27b4733b842c`.

The real owner accepted operation `index-1791452380571827157-base` as native revision `b0074eba-8344-4719-9b5d-3fd32dd6d6f6`, parent `addd34ae-2929-4115-93a2-8c13cc009443`, dispatch fence 45. Prepared accepted candidate digest is `sha256:b7654f5defc13318a69125c054e768d09dbe5b42256e260adcf19b56ae4623cd`. This is actual accepted evidence, not a synthetic receipt.

The prior readable historical checkpoint was revision `ce387bfd-63ed-4804-8aff-196c560ae143`, graph digest `sha256:f7b4e95eb40b3002439f531dd3e78c4ffac933dad82b7777887bfe73c009c203`, candidate `sha256:7c90d6f3e1c9b90b2422194462c7d99ce93c36774a3488a1779060742be63fe1`, 1036 quads, projection schema `urn:ledgrrr:revision-projection:1`. It is an observed historical checkpoint, not proof the new accepted revision was published.

Report gates head-readable, intake and accepted were Satisfied. Gate `receipt-before-index-is-pending` was Violated: its 250 ms exact query returned Unavailable/DeadlineExceeded with unknown model head. All subsequent live query, crash, out-of-order, rebuild and authorization gates were not executed and remain **Unknown**. Later read-only exact queries of that accepted revision completed in 333 ms/297 ms after the cleanup worker had resumed; that isolates the insufficient probe budget but does not establish pending-before-publication or replace the missing full run.

Revalidated private handles: owner `a54c9f432f9a`, native `82d3dc1df337`, PostgreSQL `a7a6428dc300`, all running. Native/database handles persisted across the failed run; retained create-drop reference runtime was not restarted. Credentials, generated DB/token files and reports remain outside Git.

Automatic approval review rejected both scoped live rerun attempts before execution because it recognized the trusted user scope as commit-to-feature and rejected the requested persistent-runtime publishing/restart/kill actions. The second attempt included the active parent goal-resumption and exact root-plan runtime scope; it was also rejected. No bypass or further runtime mutation was attempted. A direct user authorization is needed for the private runtime probe before its live claims can be verified.

## Remaining original gates

| Integrated gate | Status for this quick |
|---|---|
| 1: typed import/native requirement and trace model | Unknown as a full gate; prior supported native evidence does not prove all native/library/behavior semantics. |
| 2: conditional acceptance and actual commit | Actual acceptance observed above; full integrated gate still requires complete typed/native slice evidence. |
| 3: exact unverified requirement/code path query | Local typed projection/evaluator tests pass; required new full live query receipts Unknown. |
| 4: fresh-project portable round trip and original bytes | Unknown for the full integrated slice. |
| 5: disjoint merge and typed conflict race | Existing P3 evidence remains; expanded P4 rebase/live sequence not executed past failure. |
| 6: index crash/replay and same digest | Actual local subprocess interruption/fencing/rebuild tests pass; full private live crash/rebuild proof Unknown. |
| 7: UI conflict/fidelity/unknown/pending states | Unknown; no integrated UI evidence supplied by this quick. |

Full P4 also requires the kr0ki owner-read interface. NATS context/authorization remained unavailable on Oct8; no kr0ki edits were authorized. P0/P1/P2/P3/P5 obligations, native derivation/library/behavior fidelity, source codec completeness, governed proposals and UI remain governed by the authoritative root plan. No completed-goal or full-P4 claim is made.
