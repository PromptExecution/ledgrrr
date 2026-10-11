# Private SysML revision adapter

`ledgrrr-sysml-adapter` projects the upstream portable revision contract into the
pinned official SysML API and promotes it through the durable revision owner.
Run `just sysml-revision-test` and `just sysml-revision-check` for the local
contract checks. Run `just sysml-owner-up`, then `just sysml-owner-live` for the
separate real provider gate. The live report is
`/tmp/sysml-owner-c9d-live-report.json`; unavailable infrastructure exits nonzero
with `Unknown`, and incomplete evidence cannot pass acceptance.

The task-owned rootless Podman pod `pex-sysml-owner-c9d` publishes only the owner
at `127.0.0.1:19001`. Native HTTP binds **inside the pod** to `127.0.0.1:9000`;
PostgreSQL also binds pod-local loopback. The owner shares that network namespace.
An unrelated network namespace cannot reach those loopback services. This is a
network isolation boundary, independently probed from the host and an unrelated
container. A loopback publication alone would not establish private backend
access. Host/Podman administrators who can attach processes to the pod remain
trusted, as do administrators who can write the owner database/configuration.
The runtime refuses extra published ports and never publishes the native API.

The official source is pinned to
`0af711b14bbcea7b240bb0a3a65817ae68302092`; Java and PostgreSQL images use the
baseline's immutable digests. `sysml-owner-up` requires its already-built official
stage, copies it into task-owned state, and edits only the copied JPA configuration
to explicit `hibernate.hbm2ddl.auto=update`. PostgreSQL files and the owner's SQLite
database persist in `/tmp/sysml-owner-c9d`. There is no destructive reset recipe.
The retained **`pex-sysml-reference` must never be restarted**: its `create-drop`
configuration would destroy the earlier probe data. New provider project/branch
provisioning is a privileged host action before owner startup.

Credentials are generated into a mode-0600 host configuration outside Git. Tokens
map to host-established actors/grants; requests cannot choose their actor, provider,
bootstrap or publisher. Only bounded `/v1` proposal/promotion, receipt/recovery,
exact bundle/artifact and head routes are exposed. Native project, branch, tag,
ordinary/named commit and query mutations have no passthrough route. Editors may
submit proposals; the configured worker holds promotion administration. Artifact
reads require the same project authorization, even when a digest is known.

The provider ignores stale `previousCommit`, so independent native writers are
unsupported. One physical provider branch has one durable owner authority;
project aliases must not bypass it ([issue 250](https://github.com/PromptExecution/ledgrrr/issues/250)).
Reservation precedes head fetch. Accepted merged content, actual parent and native
projection are persisted separately from immutable submitted bytes. Dispatch is
single-use. Unknown outcomes retain their reservation across restart; reconciliation
requires complete reachable history and exact operation/content/parent evidence.
Missing evidence never authorizes resending. A full-envelope/native no-op records
an observed existing revision and sends no commit.

Native requirement, verification, satisfy and allocation claims require comparison
of fetched native fields, including their actual singleton/array endpoint shapes.
Envelope preservation separately proves original source/attachment byte fidelity.
The capability table refuses required unsupported native semantics before publication.
Source symbols/compiler evidence retained in an envelope do not prove resolved
SysML behavior or a derivation library. Exact accepted revisions enqueue indexing;
`ModelCommitted` does not establish `Indexed`.

Live acceptance records actual remote revisions and parents, fetched bundles/native
fields, original blob bytes, full-envelope no-op commit counts, disjoint merge,
conflict, authorization, route isolation, and restart reconciliation. Trusted crash
hooks belong exclusively to the host test harness; production requests cannot
activate them. Review every report gate rather than treating an HTTP 200 as proof.
Resolved derivation/library and broader native behavior, browser integration and
interactive UI inspection remain required full-plan work. Owner graph publication
and revision queries have their own verification gates below.

## Revision index verification

Use `just sysml-index-test` and `just sysml-index-check` for storage publication,
projection and query checks. `just sysml-index-live` runs the real accepted-model
probe and writes `/tmp/sysml-index-c9d-live-report.json`. A successful test process
does not replace the live report: inspect every gate and its actual revision,
candidate digest, graph descriptor and query response. Missing infrastructure is
`Unknown` with exit 2; a contradicted invariant is `Violated` with exit 1.

The index probe uses the preserving `pex-sysml-owner-c9d` runtime. It inspects
existing container handles before restarting the owner, disables automatic
indexing through trusted host configuration, and drives separate claim, seal and
publication stages through the host-only `revision-owner index` command. It kills
the actual owner after claiming work and after sealing an unpublished graph,
then reopens the same database. Compiled-test-only storage tests cover interruption
inside the atomic publication transaction. No HTTP request can select these
administrative actions or supply graph artifacts/checkpoints for publication.

`SYSML_INDEXING_ENABLED=false` on an owner restart updates only the task's private
host configuration. The probe restores the reader grant and automatic indexing
when it exits, including after partial failure. Provider data is never reset.
The final report records terminal container handles and restoration failures;
a successful query cannot conceal a stopped owner or an unrestored grant.
Removing a projection for rebuild proof retains its accepted manifest and source
blobs; rebuild must preserve the graph digest and native commit count.

Authenticated revision queries use
`POST /v1/projects/{project}/branches/{branch}/query`. The body includes the same
project and branch, a canonical selector, SPARQL text and relative deadline:

```json
{
  "project": "live-project",
  "branch": "main",
  "selector": {"kind": "current", "allow_older": false},
  "query": "ASK {}",
  "deadline_ms": 2000
}
```

Exact selectors identify an actual accepted provider revision. Minimum selectors
carry a previously returned checkpoint and require proven accepted ancestry;
revision UUID ordering has no freshness meaning. A completed response includes
actual model/index revisions, a graph descriptor and `fresh` or `stale` freshness.
Pending and unavailable results contain no fabricated query answer. Historical
exact answers can be stale relative to the branch while fully satisfying their
requested revision. Project grants apply to graph/checkpoint access even when the
caller already knows their digests.

Query execution uses a bounded pool of supervised `revision-owner query-worker`
processes. The request deadline covers graph loading, parsing, evaluation and lazy
result consumption. The supervisor cancels the evaluator, kills a worker that
outlives its deadline, and reaps it before returning capacity. Each child also has
a 1 GiB address-space limit and a six-second CPU limit; the total request deadline
is at most five seconds. At most four query workers run concurrently. Worker input
uses a bounded 1 MiB header plus at most 64 MiB of graph bytes; output and error
pipes are bounded and joined before capacity is released. This process boundary handles engine operations
that fail to observe cooperative cancellation before producing their first row
([tracked evaluator defect #252](https://github.com/PromptExecution/ledgrrr/issues/252)).
Embedded hosts must configure the `revision-owner` executable as their worker path;
the default executable is the current owner process.
The live probe observes an actual expensive-query child PID, then checks its exit
and a successful subsequent query; an HTTP timeout alone does not prove this gate.

The versioned `urn:ledgrrr:revision:1:` projection preserves canonical model and
evidence fields. Its convenience predicates include `element_kind`, `id`,
`relation_kind`, named endpoint roles and `authority`; source coordinates hang
from `evidence` nodes. For example, this query finds requirement definitions or
usages without an authored Verify relation:

```sparql
PREFIX rev: <urn:ledgrrr:revision:1:>
SELECT ?id WHERE {
  ?requirement rev:element_kind ?kind ; rev:id ?id .
  FILTER (?kind IN ("requirement_definition", "requirement_usage"))
  FILTER NOT EXISTS {
    ?relation rev:relation_kind "verify" ;
      rev:requirement ?requirement ; rev:authority "authored" .
  }
}
ORDER BY ?id
```

These graph facts retain their authority and accepted input identity. Projecting
compiler or inferred evidence does not promote it to an authored native assertion.
Owner graph publication remains distinct from full P4: kr0ki must consume the
same authorized revision query contract before its discovery integration is proven.
