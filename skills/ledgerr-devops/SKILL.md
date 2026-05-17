---
name: ledgerr-devops
description: Use this skill for release management, CI/CD operations, Docker/Podman image publishing, branch/PR workflow, and justfile task execution on the l3dg3rr repo. Covers the full devops loop from branch creation through GitHub release to GHCR image verification.
---

# ledgerr-devops

## Branch + PR Workflow

Always work on a named branch — never commit directly to main.

```bash
git checkout -b fix/<topic>     # or feat/<topic>, chore/<topic>
# ... make changes ...
git add <specific files>        # never git add -A
git commit -m "type(scope): message"
git push -u origin <branch>
gh pr create --title "..." --body "..."
gh pr checks <number> --watch   # wait for CI
gh pr merge <number> --squash --delete-branch
git checkout main && git pull
```

## Release

```bash
just release patch    # runs cargo test + e2e, bumps version via cog, pushes tag
just release minor
just release major
```

Release auto-triggers CI → `podman-publish` → GHCR image push → `mcpb-publish` multi-platform bundles.

## CI Workflow Names (exact — must match workflow_run triggers)

| File | name: field |
|------|-------------|
| `ci.yml` | `CI with MCP Registry Publish` |
| `release.yml` | `Release` |
| `publish.yml` | `Publish Artifacts` |
| `mcpb-publish.yml` | `MCPB Multi-Platform Publish` |
| `podman-publish.yml` | `Publish Podman Image` |

## Justfile Quick Reference

```bash
just test                  # full workspace test suite + mcp-outcome-test
just mcp-start             # run MCP server (stdio, dev build)
just mcp-start-release     # run release binary
just mcp-podman-run        # pull + run latest GHCR image via podman
just mcp-podman-run v0.2.0 # specific release tag
just mcp-podman-verify     # inspect manifest without full pull
just bundle                # build .mcpb for x86_64-unknown-linux-musl
just bundle-all            # all tier-1 targets (requires cross toolchains)
just v                     # print current version
just validate              # check conventional commits via cog
just changelog             # show cog changelog
```

## Verifying a Release Image

```bash
just mcp-podman-verify main            # manifest inspect (no pull)
just mcp-podman-run main               # pull + run, mount $PWD/data:/data
```

The container runs `/usr/local/bin/ledgerr-mcp-server` (stdio MCP transport).
Mount `-v $PWD/data:/data` for workbook and PDF inbox.
Env vars: `LEDGERR_WORKBOOK_PATH`, `LEDGER_PDF_INBOX`.

## Secrets Required (CI)

| Secret | Purpose |
|--------|---------|
| `GITHUB_TOKEN` | Auto-provided; GHCR push, release creation |
| `CRATES_IO_TOKEN` | crates.io publish (optional) |
| `PYPI_API_TOKEN` | PyPI publish (optional) |

Set via: `just gh-secrets-set-repo` (reads from `.env`).

## Workspace Crates

| Crate | Purpose |
|-------|---------|
| `ledger-core` | Domain types, ingest, classify, workbook |
| `ledgerr-mcp` | MCP adapter + stdio server binary |
| `xtask-mcpb` | Bundle/publish automation |
