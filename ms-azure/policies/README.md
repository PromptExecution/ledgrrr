# ms-azure/policies

Cedar policy bundles for the agent-service-desk's RBAC gating.

**Not started.** Wire-up target is `crates/msft-agent-gov-ledgrrr`'s
`cedar-policy` feature flag (`LedgrrAgtGateway`, currently YAML-only by
default — see that crate's `SPEC.md`, Gap 5). That gap analysis
deliberately deferred the Cedar swap ("Phase 3 concern... upstream action:
none, AGT already supports it") — this directory existing is the signal to
pick that back up, not a reason to bypass `msft-agent-gov-ledgrrr` and call
`cedar-policy` directly from somewhere else. Keep the policy engine
integration in one place.

Before writing real policy here: decide the rule set. The existing YAML
policy (`SPEC.md` §1.4 — `block-shell`, `commit-approval-gate`,
`ingest-rate-limit`, `xero-rate-limit`, `allow-all-ledgerr-ops`) is the
thing a Cedar equivalent needs to at least match, extended with whatever
the agent-service-desk's RBAC-role-to-MCP-tool-surface mapping requires
once the tenant-scope question in `ms-azure/README.md` is resolved.
