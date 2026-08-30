# ms-azure/bicep

Tenant-scoped Bicep for `ledgrrr`'s Azure/Entra footprint: app
registrations, service principals, RBAC role assignments, and any
agent-service-desk-specific resources.

**Not started.** Blocked on the tenant-scope question in the parent
README (own tenant vs. PromptExecution's vs. customer-deployable
template) — that decision determines whether this consumes
`PromptExecution/infrastructure`'s `msft-corp/modules/agent-identity/`
module directly (single-tenant case) or needs its own
parameterized-per-tenant variant (multi-tenant/customer-deployable case).

When this starts: prefer composing the reusable module library from
`msft-corp/modules/` over reimplementing Entra-identity Bicep here -
that's the whole point of that library existing as a separate,
`msft-corp/examples/`-demonstrated module rather than inline in one repo's
`entra/` mapping files.
