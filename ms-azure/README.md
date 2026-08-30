# ms-azure — ledgrrr's own Azure/Entra tenant structure

## Scope and status

This directory is `ledgrrr`'s own infrastructure-as-code, for the specific
Azure/Entra tenant and capability set `ledgrrr` needs to run as a
tenant-deployable, MCP-based service — distinct from
`PromptExecution/infrastructure`'s `msft-corp/`, which is PromptExecution
Pty Ltd's own corporate tenant mapping (Entra SPs for LAN boxes, click-ops
import, etc.). This directory consumes patterns from there but is not the
same tenant or the same concern.

**This is a scaffold, not a working deployment.** Directory structure and
intent are laid out below; the actual Bicep/Cedar content is unwritten
pending the design decisions each subdirectory's README calls out.
Positioning `ledgrrr` as an official Microsoft Copilot ecosystem plugin is
the long-term aspiration driving this — that's Microsoft's call to make,
not something this repo can build its way into. What *is* buildable now is
the infrastructure a credible submission would need to already have:
a real, runnable, multi-tenant-capable Azure deployment story.

## The shape being built toward

`ledgrrr` today (see `server.json`, `crates/ledgerr-mcp`) ships as a
**local-first** MCP server bundle (`.mcpb`) plus a Windows client
(`windows/package/` — AppX/MSIX packaging already exists). The direction
this scaffold is for: a **distributed MCP-based ERP delivery model** — the
Windows client talks to `ledgerr-mcp`/`ledgerr-cloud` capabilities exposed
through Azure, gated by a ServiceNow-style **agent-service-desk**: RBAC
roles (Entra-backed) decide which agent/user can reach which MCP tool
surface, enforced via Cedar policy through `crates/msft-agent-gov-ledgrrr`'s
`LedgrrAgtGateway` (already exists, Phase 1 complete, `cedar-policy`
feature flag already scaffolded there for exactly this).

## Directory layout

```
ms-azure/
  README.md     - this file
  bicep/         - tenant-scoped Bicep: app registrations, RBAC role
                   assignments, agent-service-desk resources. Should
                   consume the reusable module library published from
                   PromptExecution/infrastructure's msft-corp/modules/
                   (e.g. modules/agent-identity/) rather than
                   reimplementing the same Entra-identity pattern here -
                   see that repo's #131/#133/#139/#140 for the identity
                   and Cedar-governance groundwork already in progress.
  policies/      - Cedar policy bundles for agent-service-desk RBAC
                   gating. Wire-up target: msft-agent-gov-ledgrrr's
                   `cedar-policy` feature (currently off by default,
                   see that crate's SPEC.md Gap 5 - "Do not migrate to
                   Cedar/OPA yet" was the Phase-1-era call; this
                   directory existing is the trigger to revisit that).
  examples/      - runnable, single-tenant quickstart deployments -
                   the "runnable examples" half of the goal. Each
                   example should be `az deployment group create`-able
                   end to end, not just a code sample.
```

## Open design questions (not answered here)

- What Entra tenant does this actually target — PromptExecution's own
  (`1fd87b50-f47c-4023-aad1-50c18cad799d`), a dedicated `ledgrrr`-only
  tenant, or a template deployable into a *customer's* tenant (the shape
  a real Copilot-ecosystem plugin submission would need)? These have very
  different Bicep/RBAC designs.
- Does the agent-service-desk's RBAC model reuse Entra app roles directly,
  or is Entra only the identity provider with all authorization logic
  living in Cedar (via `msft-agent-gov-ledgrrr`)?
- SysML-v2 digestion (`PromptExecution/infrastructure#133`): the Bicep
  modules here should be structured so "b00t bicep" can eventually walk
  their resource-type metadata into SysML-v2/KerML types. #133 is itself
  still `[future]` — don't block on it, but keep resource/module
  boundaries clean (one clear capability per module) since that's the
  same shape SysML digestion would need regardless of whether the tool
  exists yet.

## Related work

- `PromptExecution/infrastructure` — `msft-corp/` (Bicep tenant mapping +
  extension library), #131 (Entra SP identity), #133 (SysML v2 digestion,
  future), #139 (agent governance / Cedar, active), #140 (session
  lifecycle / billing, future).
- `crates/msft-agent-gov-ledgrrr` (this repo) — the governance engine this
  scaffold's `policies/` directory targets.
