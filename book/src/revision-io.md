# Durable Revision I/O

`ledgrrr-revision-io` owns portable proposal artifacts, immutable operation identity,
project grants and restart-visible receipts in a local SQLite database. It consumes
the canonical upstream `ufo-types` revision contract. It has no bookkeeping, model
runtime, provider HTTP or graph-store dependency.

The embedding host authenticates each actor and controls project bootstrap and
database file permissions. Never construct the authenticated `ActorId` from an
untrusted request field. `ProjectBinding` contains configured provider handles,
not credentials. An explicit bootstrap creates an absent project; existing
projects cannot be taken over or rebound through bootstrap. All later project
methods check the relevant grant in the same transaction as their work. A digest
is an identity, not a read capability: artifact retrieval also requires a project
reference and read grant. Grant administration cannot remove the last admin.

This executable example proves durable local intake and original-envelope retention.
Its empty model keeps the example short; the JSON integration fixture exercises
requirements, source symbols, typed values, relations and binary attachments.

```rust
use ledgrrr_revision_io::*;
use std::collections::BTreeMap;
use ufo_types::revision::{PortableModel, RevisionContext};

let dir = tempfile::tempdir()?;
let path = dir.path().join("revisions.sqlite");
let owner = ActorId::new("authenticated-owner")?;
let project = ProjectId::new("example-project")?;
let branch = BranchId::new("proposal-branch")?;
let binding = ProjectBinding {
    provider: "configured-provider".into(),
    remote_project: "configured-project".into(),
    model_dialect: "SysML-v2".into(),
};
let bundle = PortableBundle::dehydrate(
    PortableModel { elements: BTreeMap::new(), relations: BTreeMap::new() },
    RevisionContext {
        project: project.clone(), revision: RevisionId::new("proposal-1")?,
        parent: None, model_dialect: binding.model_dialect.clone(),
        library_revisions: BTreeMap::new(), source_revisions: BTreeMap::new(),
        toolchain: BTreeMap::new(),
    }, BTreeMap::new(),
)?;
let raw = serde_json::to_vec_pretty(&bundle)?;
let mut store = Store::open(&path)?;
store.bootstrap_project(&owner, &project, &binding)?;
store.register_branch(&owner, &project, &branch, "configured-remote-branch")?;
let request = Intake {
    project: project.clone(), branch, operation: OperationId::new("operation-1")?,
    expected_head: ExpectedHead::Empty, binding,
};
let accepted = store.intake(&owner, &request, &raw)?;
assert_eq!(accepted.receipt.status, SyncStatus::Pending);
drop(store);
let mut reopened = Store::open(&path)?;
assert_eq!(reopened.operation(&owner, &project, &request.operation)?, accepted);
assert_eq!(reopened.artifact(&owner, &project, &accepted.raw_envelope)?, raw);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Intake strictly validates a `PortableBundle`, its complete digest, source bytes,
project, dialect and proposal parent. `ExpectedHead::Empty` is explicitly empty,
never an unchecked write. The branch must have an immutable configured remote
binding. The operation key is `(project, operation)`; its actor, branch, expected
head and full proposal digest are immutable. Identical retries return the current
receipt without replacing the first original envelope. Differently formatted JSON
is acceptable only when its complete canonical bundle identity agrees. Original
envelope bytes, canonical envelope bytes and every referenced source/attachment
are stored separately as immutable SHA-256-addressed BLOBs.

`start_dispatch` requires an authenticated admin worker, reserves a branch and
persists its monotonic fence and request evidence **before** the provider request.
A repeated call never authorizes resending, including after process restart.
`mark_ambiguous` preserves that reservation. There is no timeout, lease-expiry,
force-success or automatic resend/release API. A worker resolves a matching
dispatch with `record_commit` using independently obtained provider evidence bound
to its project/provider, operation, fence, expected head and complete proposal
digest. The actual observed provider revision is preserved; a deterministic
index-work identity is persisted with `ModelCommitted` in the same transaction.
Predispatch `Conflict` and `Unavailable` can be recorded through
`reject_before_dispatch`; after dispatch they cannot release an uncertain request.

This store cannot prove that evidence reflects a real remote observation. Keep
provider workers inside the authenticated trusted host, with credentials outside
proposal clients. In particular, a local fence cannot stop an in-flight request
on a provider that lacks conditional writes. Every provider mutation route must
be constrained separately before advertising safe remote serialization.

`publisher` returns a project-scoped capability only for a trusted admin. Keep it
inside the actual graph publisher, not a generic client receipt-update endpoint.
Its `record_index` checks receipt generation and exact project/revision/dialect.
It rejects Pending-to-Indexed and contradictory completion. It records supplied
publication evidence only; it cannot prove graph completeness, publish a graph or
advance a branch checkpoint pointer. Completion of an older operation never
changes a newer branch dispatch reservation.

`recovery` returns bounded deterministic pages (1–256 records), including exact
intent identity, original/canonical artifact references, dispatch evidence,
actual revision and indexing-work identity. Reads revalidate canonical records,
byte hashes and receipt identities under a transactional project grant check.

SQLite connections verify foreign keys, WAL and FULL synchronization and use a
bounded busy timeout. Writes use immediate transactions and parameterized SQL.
The application ID, schema version, schema definitions and schema digest are
checked. Initialization is allowed only for an empty database; future, unrelated
or corrupt schemas fail closed. No migration or reset is inferred. Storage
guarantees apply to local filesystems and the SQLite/filesystem durability
contract, not distributed authority or an untrusted process with raw database
write access. See [SQLite transactions](https://www.sqlite.org/lang_transaction.html)
and [PRAGMA settings](https://www.sqlite.org/pragma.html).

Run `just revision-io-test` and `just revision-io-check`. Integration tests cover
restart, independent connection races, conflicting identity, denied/revoked access,
corrupt records/artifacts, SQL rollback and actual child-process interruption.
The interruption test kills a child after uncommitted artifact insertion and after
a completed public intake; it does not simulate hardware power loss or interrupt
every individual public intake statement.

## Remaining live gates

Native/ReqIF adapters, browser schema, envelope projection and fetch–emit–fetch
fidelity are pending. Private provider mutation routes, actual remote promotion,
outcome reconciliation, external head polling and live two-client merge/conflict
are pending. Complete immutable staged Oxigraph graphs, fenced atomic publication,
exact/min-revision queries, freshness and replay/rebuild are pending. Integrated
proposal/conflict/fidelity/unknown/pending UI inspection is pending. This storage
milestone does not pass the complete SysML plan's seven end-to-end gates.

## Related Chapters

- [Ontology & Type Mesh](./ontology-type-mesh.md)
- [MCP Surface](./mcp-surface.md)

## Native promotion preparation

Schema version 2 explicitly upgrades the known version 1 schema in one SQLite
transaction. Artifacts, receipts and identity columns stay intact; legacy
unresolved dispatches remain authorized and blocking. Duplicate physical
`(provider, remote_project)` mappings reject the upgrade without resetting data
(issue [#250](https://github.com/PromptExecution/ledgrrr/issues/250)). Dialect is
excluded from physical identity. Provider handles are canonical lowercase host
configuration identities; the host must map each physical origin to one handle.
One owner database owns a physical provider topology; SQLite is not distributed
authority across independently configured databases.

New adapters call `reserve` before remote head reads, `prepare` to persist the
immutable accepted candidate, then `authorize_send` exactly once. Preparation
stores full candidate/artifact bytes, actual parent, native projection and identity
map digests separately from original intake. A repeat authorization never grants
a second send. Never-authorized reservations can use `cancel_reserved`; authorized
or ambiguous work never expires. `record_observed` atomically binds a fully
verified existing revision without authorizing a request. Index work binds the
accepted candidate digest and actual server revision. `start_dispatch` remains
a compatibility operation for trusted older adapters and grants one send.
