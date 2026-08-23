# DVC Integration Scoping (SysML v2 Epic, follow-on)

Status: investigation complete, no implementation recommended yet — see
"Recommendation" below.

## Purpose

Both epic parts left DVC (Data Version Control, the `iterative/dvc` open-source
tool) integration as an explicitly open item, separate from the SysML v2
parser/LSP/MCP thread:

- Part 1 ([`docs/sysml-v2-tooling-survey.md`](sysml-v2-tooling-survey.md),
  [`ledgrrr#180`](https://github.com/PromptExecution/ledgrrr/pull/180)):
  "DVC integration remains open — needs its own scoping pass, separate from
  this epic's parser/LSP/MCP thread."
- Part 2 ([`docs/systems-modeling-registry-rescope.md`](systems-modeling-registry-rescope.md),
  [`ledgrrr#181`](https://github.com/PromptExecution/ledgrrr/pull/181)):
  "DVC integration remains untouched and out of scope for this doc — same
  status as Part 1 left it: a separate tool line (data-source tracking)
  needing its own scoping pass."

This doc is that scoping pass. It does not implement anything — it inventories
what exists, what doesn't, and the concrete options if/when the need becomes
real.

## What was actually investigated

Per an explicit instruction to extend b00t's own MCP server registry (already
vetted/"blessed") rather than depend on a third-party discovery tool
(`vinkius-labs/discover-mcp`, "unblessed" — its live catalog API is
token-gated, see below) directly, the search for an existing DVC MCP server
covered every source available without a paid subscription:

1. **b00t's own blessed registry** (`b00t mcp registry search dvc` /
   `--tag dvc`, 52 servers indexed): **0 matches**, by keyword or tag.
2. **The official MCP registry** (`registry.modelcontextprotocol.io`, synced
   fresh via `b00t mcp registry sync-official`, 27 servers): **0 matches**.
3. **Direct GitHub code/repo search**, three separate query angles: two false
   positives only —
   - `wisrovi/wDVC-mcp` — an unrelated personal "wDVC" data-pipeline-management
     project; name collision only, nothing to do with `iterative/dvc`.
   - `jiangmuran/dvcode_mcp` — usage-tracking for `dvlab.com`, unrelated to
     data version control despite the "dvc" substring.
4. **`vinkius-labs/mcp-database`** (the "Vinkius Open Data Initiative" — a
   fully open-source, no-auth GitHub dataset of MCP server metadata, 6533
   entries under `mcps/`, distinct from Vinkius's *other*, paid, token-gated
   product — the `discover-mcp` npm package / `api.vinkius.com` live catalog,
   confirmed to 401 on any unauthenticated request): contains exactly one
   matching entry, **`mcps/dvc.md`**. Read in full. It documents a **hosted
   proxy to DVC Studio** (`iterative.ai`'s paid cloud product for ML
   experiment tracking) — not a self-hostable, plain-`dvc`-CLI MCP server.
   Its 6 tools (`get_project`, `list_experiments`, `list_views`, `get_view`,
   `list_projects`, `get_user`) all operate against a DVC Studio *account*,
   requiring a **DVC Studio Client Access Token** (obtained from DVC Studio's
   own settings page) *and* routing through Vinkius Edge
   (`https://edge.vinkius.com/[TOKEN]/mcp`), a second token-gated hop.

## Conclusion

**No genuine, self-hostable, open-source DVC (`iterative/dvc`) MCP server
exists today**, in the official registry, in b00t's own registry, on GitHub,
or in the open Vinkius metadata dataset. The one hit that superficially
matches (`mcps/dvc.md`) is a hosted proxy into DVC Studio's paid cloud
product — a different thing from "wrap the `dvc` CLI as an MCP tool" — and
would require the user to have (or create) a DVC Studio account regardless of
whether it's adopted.

This is a negative result with reasonably high confidence: four independent,
non-overlapping sources (official registry, b00t's blessed registry, GitHub
search, and a 6533-entry open community dataset) all agree.

## Options, if/when a concrete need arises

1. **Build a thin first-party DVC MCP wrapper** (Rust, subprocess-shelling out
   to the `dvc` CLI — `dvc list`, `dvc exp show --json`, `dvc metrics show
   --json`, `dvc dag`, etc.). Feasible without any token: plain `dvc` operates
   against a local `.dvc`/`dvc.yaml` + a configured remote (S3/GCS/local/etc.)
   — DVC Studio's cloud dashboard (and its token) is only needed for the
   hosted UI, not for `dvc`'s core version-control/experiment-tracking
   functionality. This is the only path that avoids depending on a third-party
   paid product, and would follow the same "wrap over MCP, don't port to Rust"
   shape decision already made for `reqif-opa-mcp` in Part 2 (§5, decision 6)
   — except here there would be no existing first-party wrapper to reuse; it
   would need to be written from scratch.
2. **Register the Vinkius-hosted `dvc.md` entry into b00t's registry as
   metadata only** (searchable/discoverable, tagged `requires-token`), without
   attempting to invoke it — this is already covered by the broader
   `vinkius-labs/mcp-database` sync work tracked separately (b00t-cli/
   b00t-c0re-lib, not this repo), and costs nothing beyond that sync existing.
   It does not give ledgrrr a working DVC integration; it just makes the
   option visible next time someone searches.
3. **Do nothing further** — no concrete ledgrrr use case for DVC has been
   identified in this scoping pass itself (the investigation was triggered by
   a general "what data-version-control tooling exists" exploration, not a
   demonstrated requirement inside ledgrrr's own pipeline). Revisit only when
   an actual need appears (e.g., versioning the large sample corpora used by
   the SysML v2 parser spikes, or ML-experiment tracking if/when a model-
   training component is added to ledgrrr).

## Recommendation

**Defer.** Do not build option 1 speculatively — there is no identified
consumer for it yet, and it would be new subprocess-integration surface
(shelling out to a Python CLI) for a capability nothing in ledgrrr currently
calls. Option 2 (metadata-only registry visibility) is low-cost and already
in flight as part of unrelated registry-extension work. Revisit this doc and
promote to an implementation task only when a concrete ledgrrr workflow
actually needs data/experiment versioning — at that point, re-check whether
the MCP server landscape has changed (an official or community DVC-CLI
wrapper may exist by then) before defaulting to option 1.
