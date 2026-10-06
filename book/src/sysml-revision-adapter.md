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
Resolved derivation/library and broader native behavior, immutable graph publication,
revision-scoped queries, browser integration and interactive UI inspection remain
required full-plan work.
