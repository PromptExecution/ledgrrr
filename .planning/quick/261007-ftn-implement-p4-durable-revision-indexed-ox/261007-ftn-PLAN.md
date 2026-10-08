---
phase: quick-261007-ftn
plan: "01"
type: execute
wave: 1
depends_on: [quick-261006-c9d, upstream-revision-query-contract]
files_modified:
  - Cargo.toml
  - Cargo.lock
  - Justfile
  - crates/ledgrrr-revision-io/src/lib.rs
  - crates/ledgrrr-revision-io/src/store.rs
  - crates/ledgrrr-revision-io/src/schema.sql
  - crates/ledgrrr-revision-io/src/schema-v2.sql
  - crates/ledgrrr-revision-io/tests/durable_index.rs
  - crates/ledgrrr-revision-io/tests/fixtures/index_jobs.json
  - crates/ledgrrr-sysml-adapter/src/projection.rs
  - crates/ledgrrr-sysml-adapter/src/query.rs
  - crates/ledgrrr-sysml-adapter/src/indexing.rs
  - crates/ledgrrr-sysml-adapter/src/lib.rs
  - crates/ledgrrr-sysml-adapter/src/promotion.rs
  - crates/ledgrrr-sysml-adapter/src/server.rs
  - crates/ledgrrr-sysml-adapter/src/bin/revision-owner.rs
  - crates/ledgrrr-sysml-adapter/Cargo.toml
  - crates/ledgrrr-sysml-adapter/tests/revision_projection.rs
  - crates/ledgrrr-sysml-adapter/tests/revision_queries.rs
  - crates/ledgrrr-sysml-adapter/tests/index_owner_routes.rs
  - crates/ledgrrr-sysml-adapter/tests/fixtures/revision_projection.json
  - scripts/sysml-index-live-probe.py
  - scripts/sysml-owner-runtime.sh
  - book/src/sysml-revision-adapter.md
  - AGENTS.md
autonomous: true
requirements: [P4, SLICE-3, SLICE-6]
must_haves:
  truths:
    - Accepted candidate and actual provider commit durably identify complete revision indexing work, including rebased and no-op acceptances.
    - Only a validated immutable sealed graph can publish a checkpoint, Indexed receipt and eligible branch pointer atomically under a current worker fence.
    - Authorized exact and minimum-checkpoint SELECT/ASK answers identify actual model/index revisions, digest and freshness, with joined evaluator cancellation.
    - Crash, replay, stale workers, out-of-order completion and projection loss cannot expose mixed revisions or change deterministic rebuild digests.
    - Real accepted native owner receipts establish unverified-requirement and code-to-requirement query evidence.
  artifacts:
    - path: crates/ledgrrr-revision-io/tests/durable_index.rs
      provides: Actual migration/publication interruption, fencing, replay and rebuild evidence
    - path: crates/ledgrrr-sysml-adapter/src/projection.rs
      provides: Complete versioned deterministic accepted-bundle RDF projection
    - path: crates/ledgrrr-sysml-adapter/src/query.rs
      provides: Parser preflight and bounded cancellable SELECT/ASK execution
    - path: scripts/sysml-index-live-probe.py
      provides: Real owner acceptance, revision query, restart and digest-equivalent rebuild report
  key_links:
    - from: Store.record_commit_inner
      to: durable accepted manifest and index job
      via: Same immediate transaction as actual commit evidence
    - from: prepared.envelope
      to: sealed canonical NQuads
      via: Accepted candidate digest plus actual provider revision and projection schema
    - from: fenced sealed graph
      to: checkpoint, Indexed receipt and branch index pointer
      via: Atomic publication transaction
    - from: authenticated query route
      to: verified immutable authorized revision artifact
      via: Current transactional project grant and authoritative selector resolution
---

<objective>
Implement durable revision-indexed discovery in the existing private revision owner. Build complete immutable graphs from accepted manifests, publish under fenced durable work, and serve authorized bounded revision-specific queries with explicit freshness. Prove the owner P4 boundary using real accepted native commits and actual interruption/rebuild evidence.

The authoritative root plan remains `/home/brianh/promptexecution/docs/PLAN-2026-10-05-sysml-discovery-sync.md`: all P0–P5 packages and all seven integrated gates remain required. This quick cannot establish full P4 until its kr0ki read-interface dependency is also implemented and verified; it cannot establish missing native behavior/library semantics by projecting envelope facts.
</objective>

<context>
@AGENTS.md
@.planning/STATE.md
@.planning/quick/261006-c9d-integrate-the-durable-revision-owner-wit/261006-c9d-SUMMARY.md
@/home/brianh/promptexecution/docs/PLAN-2026-10-05-sysml-discovery-sync.md
@/home/brianh/promptexecution/docs/SYSML-INDEX-PUBLICATION-NOTES-2026-10-06.md

Mandatory read-only Explore completed: index_owner_explore refreshed graph to 14,162 nodes / 47,524 edges; index_query_explore verified Oxigraph/spargebra APIs and canonical projection gaps. Existing owner HEAD c2da4ea, feature/sysml-discovery-revisions. Existing IndexPublisher.record_index accepts caller-created checkpoint identity without graph evidence and must cease being a path to fabricated Indexed status.

Use graph-first discovery; stale Rust source offsets warrant narrow source fallback. Preserve unrelated work. Do not create/reset nested worktrees: workflow.use_worktrees=false intentionally retains this isolated feature branch. Do not change Phase 19–21 ROADMAP or progression. Final quick metadata updates belong only to this quick's PLAN/SUMMARY and STATE quick row/activity.
</context>

<decisions_and_dependencies>
Task 0, outside this consumer repo: root implements, validates and publishes shared ufo-types query contracts, then supplies an exact Git pin. Require selectors Exact{revision}, Minimum{checkpoint,allow_older}, Current{allow_older}; closed Fresh/Stale/Pending/Unavailable results with project/branch identities, actual model/index revisions, graph digest, typed SELECT bindings/ASK value and typed absence/failure. A public graph descriptor binds projection schema, accepted candidate digest, artifact digest and actual checkpoint. Keep IndexCheckpoint compatibility. Wire requests carry no trusted actor/grant or caller-controlled service caps. Do not invent consumer-local public equivalents or production path patches. Parent alone updates the workspace pin/lock coherently after publication.

Choose owner-persisted immutable canonical NQuads artifacts, loaded into isolated Oxigraph Store::new() instances. Pin Oxigraph 0.5.11 with default features disabled; compatible spargebra 0.4.7 / spareval 0.2.7. No RocksDB APIs, no federated service handler. Projection schema is explicit and versioned. Hash sorted/deduplicated serializer-valid graph bytes with fixed trailing-newline policy and no blank nodes; graph digest cannot contain itself.

Executor owns durable storage, Cargo/lock/lib exports, owner integration and final commits. Resume the two existing implementation agents in parallel after agreeing interfaces: index_query_explore becomes projection/query child and exclusively owns projection.rs, query.rs and their tests/JSON fixture; index_owner_explore becomes live/runtime child and exclusively owns scripts, Justfile, book and operational AGENTS notes after its independent native review is terminal. Existing thread limit requires reuse rather than new spawns. Parent alone integrates child exports/dependencies and avoids editing their files. Children may start independent RED fixtures before upstream pin arrives; consumer compile integration waits for the published contract. Parent obtains terminal results before final verification. Existing Explore satisfies discovery for these modules; new unfamiliar kr0ki modules need their own Explore.

Service default caps: query text 64 KiB, AST 10,000 nodes/depth 128, response 4 MiB/10,000 rows, graph artifact 64 MiB/1,000,000 quads, nested typed values depth 128, four concurrent evaluator workers, maximum total request deadline 5 seconds, default 2 seconds; enforce lower configured values and reject invalid/zero/over-cap deadlines. Index work leases and retries are separately bounded/configured; persisted fence monotonicity survives restart. Tests override caps downward to deterministically exercise every boundary. Account for graph load, parsing, execution and lazy result consumption in the same total deadline. Keep permits until actual worker termination.
</decisions_and_dependencies>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: Durable accepted-revision manifests, fenced jobs and atomic sealed publication</name>
  <files>crates/ledgrrr-revision-io/src/lib.rs, crates/ledgrrr-revision-io/src/store.rs, crates/ledgrrr-revision-io/src/schema.sql, crates/ledgrrr-revision-io/src/schema-v2.sql, crates/ledgrrr-revision-io/tests/durable_index.rs, crates/ledgrrr-revision-io/tests/fixtures/index_jobs.json</files>
  <behavior>
    - Actual accepted merge indexes prepared candidate, never stale intake; no-op operations share one consistent revision job and all obtain its checkpoint.
    - Expired/reclaimed worker cannot seal/publish; older completion remains historically queryable without replacing newer branch discovery.
    - Interrupted migration/publication rolls back; restart/replay yields one complete checkpoint; deleted-all graph is complete and preserves historical reads.
    - Missing/corrupt projection is unavailable, then deterministic rebuild from retained authoritative accepted manifest reproduces the original digest.
  </behavior>
  <action>
Parent owns this task. Add RED tests using independent connections and real child-process interruption. Preserve strict current v2 schema as schema-v2.sql; implement atomic v2→v3 and retained v1→v2→v3 migration preserving every artifact, receipt, prepared candidate and ambiguous dispatch. Retain compiled-test-only interruption hooks; never ship process-kill environment triggers in production. Add accepted revision records, independently observed model branch head/ancestry, deduplicated durable jobs keyed project/revision/schema/dialect, leases/fences/attempt bounds, unpublished graph seals, checkpoints and branch index pointer. Enqueue accepted manifest and work in record_commit_inner transaction using prepared.envelope/candidate_digest plus actual_revision; same actual revision with inconsistent accepted identity fails closed. Legacy committed records lacking prepared candidates require independently validated native reconstruction or explicit Unavailable, never original-proposal substitution.

Expose internal claim/seal/publish/read/rebuild operations with transactional current project grants. Claim bounds work and persists a monotonic fencing token; seal checks exact accepted identity, graph bytes/limits/digest/schema and validated complete projection evidence; publication rechecks live lease/fence/seal and atomically records checkpoint, matching receipt transitions and eligible branch pointer. Existing record_index must require this sealed evidence or refuse; receipt-generation checks alone cannot bypass graph publication. Exact historical completion is allowed, branch pointer eligibility derives from accepted lineage/observed head, never UUID comparison or completion order. Retain authoritative accepted manifests/blobs separately from removable graph projection artifacts so loss/rebuild is real. Graph/artifact reads remain project-linked, including cached/revoked-grant paths.
  </action>
  <verify><automated>CARGO_TARGET_DIR=/tmp/sysml-implementation/ledgrrr-target CARGO_BUILD_JOBS=2 cargo test -p ledgrrr-revision-io --test durable_index --locked</automated></verify>
  <done>Actual subprocess migration and publication interruptions, expired fences, no-op dedup, stale merged candidate, out-of-order jobs, corruption, projection loss/rebuild and transactional project isolation pass; no API can create Indexed without a validated sealed graph.</done>
</task>

<task type="auto" tdd="true">
  <name>Task 2: Complete canonical RDF projection and bounded cancellable query core</name>
  <files>crates/ledgrrr-sysml-adapter/src/projection.rs, crates/ledgrrr-sysml-adapter/src/query.rs, crates/ledgrrr-sysml-adapter/tests/revision_projection.rs, crates/ledgrrr-sysml-adapter/tests/revision_queries.rs, crates/ledgrrr-sysml-adapter/tests/fixtures/revision_projection.json</files>
  <behavior>
    - Deterministic projection covers every canonical element/relation/type/value/evidence variant, including Unicode/lexemes/absent-versus-empty/ordered collections and empty models.
    - Source-to-requirement and unverified-requirement traversal preserves authored/inferred/compiler authority, rule and actual input revision.
    - Unsupported query kinds, recursive SERVICE and unauthorized graph/dataset selection fail before execution.
    - Expensive pre-first-row and lazy-result evaluation terminates through actual evaluator cancellation; no detached work retains capacity after deadline.
  </behavior>
  <action>
Delegate exclusively to projection/query child. Implement pure validated accepted-bundle→NQuads projection and descriptor, with stable collision-resistant helper IRIs, exact original IDs as data and metadata even when empty. Include all typed elements/properties, TypeRef, multiplicities, assignment presence, ordered/nested values, extensions, relation identities/variants/named endpoint roles/ordered connection ends, FactAuthority, inference rule plus actual accepted input revision, all SourceAnchor variants/optional fields, artifact paths/digests, source/library/toolchain/parent/fidelity context. Preserve exact decimal/timestamp lexemes alongside any query-friendly typed values. Use first-class relation/evidence nodes; derive only documented convenience edges with authority retained. Code paths start from actual SymbolPath/RustSpan evidence; invent no source-symbol ElementKind, verification assertion or native action semantics. Provide reusable documented exact typed-filter queries for missing authored Verify/Satisfy, trait implementation evidence when available, transition source evidence and revision-affected artifacts. JSON matrix covers every supported canonical variant, bounds, tombstones and cyclic relationships.

Parse SELECT/ASK with mature spargebra before evaluator execution. Walk the entire recursive AST, including Exists inside expressions, LeftJoin conditions, ordering and aggregate expressions; reject SERVICE/SILENT/variable federation, updates, CONSTRUCT/DESCRIBE, FROM clauses and caller graph selection outside the selected immutable revision. Bound AST nodes/depth and input bytes. Use SparqlEvaluator::for_query(parsed).with_cancellation_token(token).on_store(&store).execute(); cancellation remains attached while consuming lazy solutions. Hold bounded worker permits through join; cancel and await termination on deadline/disconnect/row or byte overflow, including work before first row. Fresh isolated stores load only verified selected graph artifacts; caches key full project/revision/dialect/schema/digest and never bypass owner authorization. Return upstream typed bindings preserving RDF terms and lexical values, with no arbitrary JSON domain shortcuts. Validate every graph with a fresh parse/load before producing seal evidence.
  </action>
  <verify><automated>CARGO_TARGET_DIR=/tmp/sysml-implementation/ledgrrr-target CARGO_BUILD_JOBS=2 cargo test -p ledgrrr-sysml-adapter --test revision_projection --test revision_queries --locked</automated></verify>
  <done>Complete variant matrix and deterministic digest pass; real expensive query tests prove token cancellation and joined worker exit; recursive forbidden AST and all configured bounds fail before unauthorized work/results escape.</done>
</task>

<task type="auto" tdd="true">
  <name>Task 3: Authenticated owner indexing/query integration and real restart/rebuild proof</name>
  <files>Cargo.toml, Cargo.lock, crates/ledgrrr-sysml-adapter/Cargo.toml, crates/ledgrrr-sysml-adapter/src/lib.rs, crates/ledgrrr-sysml-adapter/src/indexing.rs, crates/ledgrrr-sysml-adapter/src/promotion.rs, crates/ledgrrr-sysml-adapter/src/server.rs, crates/ledgrrr-sysml-adapter/src/bin/revision-owner.rs, crates/ledgrrr-sysml-adapter/tests/index_owner_routes.rs, scripts/sysml-index-live-probe.py, scripts/sysml-owner-runtime.sh, Justfile, book/src/sysml-revision-adapter.md, AGENTS.md</files>
  <behavior>
    - Real owner acceptance and reconciliation enqueue graph work, explicit exact/min/current queries return accurate freshness and actual candidate/commit identity.
    - Pending reads wait within deadline, stale reads require permission, unknown checkpoint lineage/digest/project fails closed; revoked grants invalidate cached access.
    - Kill/restart around accepted receipt, staging, sealing and publication plus graph loss/rebuild yields equivalent complete queries and one stable digest without duplicate native commits.
  </behavior>
  <action>
Parent owns Rust/Cargo integration; live/runtime child owns scripts/recipes/docs and can build RED probe cases concurrently. Integrate published canonical pin once, expose reachable background index worker with bounded polling/leases/retries/shutdown, enqueue through promote/reconcile/no-op paths, add authenticated checkpoint/query routes and truthful head freshness. Resolve Exact by actual revision, Minimum by proven accepted ancestry/sequence and authoritative checkpoint digest/project/dialect, Current by independently observed model head; pending/older/unavailable outcomes remain typed. Poll native head for external edits: independently fetch/validate accepted identity before indexing; unknown/missing provenance never produces invented trusted facts. Enforce project authorization inside owner storage reads and again on query completion where revocation could occur; never accept actor spoofing or caller artifact bytes. Every answer/export binds the same accepted revision manifest. Add recipe-first repeatable test/check/live commands and operational docs/actual query examples; preserve MCP catalog unless intentionally changed and checked.

Live child revalidates actual c9d process/container handles before restart and preserves retained create-drop reference pod. Use real private native owner acceptances from full slice fixture facts, not a mocked accept or invented checkpoint. Record unverified requirement and code-to-requirement results, source/attachment original bytes, rebased accepted versus submitted digest, empty/deleted-all results and historical exact queries. Kill actual worker/owner before staging, after seal and around atomic publication; replay and rebuild after removing only projection artifacts must reproduce digest with unchanged accepted model/native commit count. Test receipt-before-index restart, older-after-newer completion, unauthorized/cross-project/revoked grants, minimum deadline/explicit stale responses, bounded cancellation and external head observation. Reports stay outside Git with source/tool/library/fixture/binary SHAs, actual commit IDs, checkpoints, query receipts and terminal exit states. A failed/missing live infrastructure check is Unknown, never passing.

Required dependent kr0ki read integration remains part of full P4: after NATS registration or explicit authorized exception, use a separate mandatory Explore and delegated implementation in its existing isolated worktree to add an owner read client using the same canonical selectors/results/project grant, wire its read route to owner rather than request-supplied replacement graphs, and test accepted exact/min revision propagation and freshness. Known entry files are crates/kr0ki-core/src/rdf_store.rs and crates/kr0ki-server/src/app.rs; exact new client files follow that Explore. Do not edit kr0ki before its prerequisite is satisfied, do not silently delete this requirement, and report owner milestone versus full P4 truth separately if dependency remains unresolved.
  </action>
  <verify><automated>CARGO_TARGET_DIR=/tmp/sysml-implementation/ledgrrr-target CARGO_BUILD_JOBS=2 just sysml-index-test; CARGO_TARGET_DIR=/tmp/sysml-implementation/ledgrrr-target CARGO_BUILD_JOBS=2 just sysml-index-check; just sysml-index-live</automated></verify>
  <done>Terminal scoped Rust/test/check/live evidence proves owner publication/query/crash/rebuild gates with real provider receipts. Full P4 additionally requires terminal kr0ki owner-read integration evidence; unresolved NATS leaves that gate explicitly incomplete. SUMMARY records exact evidence and remaining root P0–P5/all-seven requirements.</done>
</task>

</tasks>

<source_coverage_audit>
GOAL: root P4 accepted-revision immutable staging/publication and bounded discovery → Tasks 1–3. REQ: SLICE-3 exact unverified/code-path query → Tasks 2/3; SLICE-6 actual index crash/replay/same digest → Tasks 1/3. RESEARCH: strict migration, accepted candidate identity, no-op dedup, fencing/ordering, project grants, graph rebuild → Task 1; complete typed deterministic mapping and parser/cancellation → Task 2; polling, real owner receipts, freshness and kr0ki read interface → Task 3. CONTEXT: existing feature isolation, canonical upstream types, immutable artifact storage, parallel ownership, no fake native semantics and preservation of all remaining gates → decisions and all tasks. P0/P1/P2/P3/P5 requirements remain outside this quick's implementation scope and remain required by the original objective.
</source_coverage_audit>

<verification>
Run RED→GREEN meaningful tests for each behavior group; log exact commands and terminal outcomes outside Git. Use CARGO_TARGET_DIR=/tmp/sysml-implementation/ledgrrr-target and CARGO_BUILD_JOBS=2 for all Cargo work. New Justfile recipes precede repeatable verification. Run existing just sysml-revision-test/check after integration, scoped formatting and git diff --check. Check existing canonical consumers only when public pin/API changes warrant it. Per standing repository requirement check official Claude plugin documentation after implementation commits and verify MCP catalog/docs if touched. No broad unrelated workspace cleanup or dependency upgrades.

For every done claim inspect actual committed implementation and report, not test names/counts or caller-created receipts. A process observation timeout is not terminal; re-poll confirmed handles rather than restart. Root performs targeted independent review after executor terminal completion. Preserve source/library-resolution and UI gates as unproven where missing.
</verification>

<success_criteria>
Owner P4 milestone is reviewable when all three tasks have terminal evidence and real exact-revision queries plus sealed atomic publication/rebuild are proven. Full P4 is achieved only with required kr0ki read integration as well. Full root objective remains active until all P0–P5 and seven slice gates have requirement-matched evidence.
</success_criteria>

<output>
Create .planning/quick/261007-ftn-implement-p4-durable-revision-indexed-ox/261007-ftn-SUMMARY.md with per-task commits, exact tests/live report digests, checkpoint/query evidence and explicit unresolved dependencies. Update only this quick's STATE completed row/activity once its verified scope is truthfully recorded; preserve Phase 19–21 progression. Commit coherent implementation milestones on the existing feature branch; final metadata commit stages exact PLAN/SUMMARY/STATE paths, never unrelated files.
</output>
