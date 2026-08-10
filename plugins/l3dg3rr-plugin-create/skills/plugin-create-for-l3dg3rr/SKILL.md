---
name: plugin-create-for-l3dg3rr
description: Bootstrap and validate l3dg3rr MCP usage in Claude Cowork Plugin Create workflows
---

Use this skill when setting up or troubleshooting `l3dg3rr` in Claude Cowork Plugin Create.

## Install and activate

1. Add marketplace:
   - `/plugin marketplace add https://github.com/PromptExecution/ledgrrr`
2. Install plugin:
   - `/plugin install l3dg3rr-plugin-create@promptexecution-fdkms`
3. Validate install:
   - `/plugin list`
   - `/plugin show l3dg3rr-plugin-create`

## MCP runtime choices

- Cargo (default in plugin manifest):
  - `cargo run -p ledgerr-mcp --bin ledgerr-mcp-server`
- Prebuilt binary:
  - `./target/release/ledgerr-mcp-server`
- Docker:
  - `docker run -i --rm -v "$PWD:/workspace" -w /workspace tax-ledger:dev cargo run -p ledgerr-mcp --bin ledgerr-mcp-server`
- Python launcher (local package in plugin):
  - `python -m l3dg3rr_mcp_launcher --mode cargo`

## First-call validation

1. `tools/list` and confirm the 8 top-level `ledgerr_*` capability tools appear.
2. `tools/call ledgerr_documents {"action":"pipeline_status"}`.
3. `tools/call ledgerr_documents {"action":"list_accounts"}`.
4. If available, run one domain call such as `tools/call ledgerr_ontology {"action":"export_snapshot"}`.

For local plugin development and reload workflow (recommended):

- Run Claude with local plugin directory during iteration:
  - `claude --plugin-dir ./plugins/l3dg3rr-plugin-create`
- After edits, run:
  - `/reload-plugins`
- Invoke skill with namespace:
  - `/l3dg3rr-plugin-create:plugin-create-for-l3dg3rr`

## Start/stop behavior

- In Cowork, MCP process lifecycle is managed by Claude.
- Manual start is foreground stdio (`cargo run ...`); stop with `Ctrl+C`.
- Optional helper commands (repo root):
  - `just mcp-start`
  - `just mcp-stop`
  - `just mcp-cli-basic`
  - `just mcp-cli-spinning-wheels`

## Troubleshooting checklist

- Confirm `cargo --version` or selected runtime binary exists.
- Confirm working directory is repository root.
- Confirm manifest path: `plugins/l3dg3rr-plugin-create/.claude-plugin/plugin.json`.
- Run MCP regression: `./scripts/mcp_e2e.sh`.

## Quality bar

- Keep outputs deterministic, concise, and machine-readable.
- Do not bypass l3dg3rr capability boundaries with ad-hoc file parsing.
- Preserve explicit blocked diagnostics for permission or invariant failures.
