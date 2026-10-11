# ms-azure/examples

Runnable, single-tenant example deployments — each one should be
`az deployment group create`-able end to end (or `az bicep what-if`-able
at minimum before that's safe to run against a real tenant), not just a
code sample that doesn't actually deploy.

**Not started.** First candidate once `ms-azure/bicep/` has real content:
a minimal quickstart that stands up one agent identity (via
`msft-corp/modules/agent-identity/`) and one Cedar-gated MCP tool-surface
grant, end to end, against a disposable resource group — small enough to
tear down/rerun freely while the tenant-scope and RBAC-model questions in
the parent README are still being worked out.
