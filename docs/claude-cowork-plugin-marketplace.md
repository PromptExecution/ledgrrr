# Claude Cowork Plugin Marketplace for l3dg3rr

## What this delivers

This repo now includes a Claude plugin marketplace and a plugin entry intended for Cowork + Plugin Create workflows:

- Marketplace: `promptexecution-fdkms`
- Plugin: `l3dg3rr-plugin-create`

## Key files

- Marketplace catalog: [marketplace.json](.claude-plugin/marketplace.json)
- Plugin manifest: [plugin.json](plugins/l3dg3rr-plugin-create/.claude-plugin/plugin.json)
- Plugin skill: [SKILL.md](plugins/l3dg3rr-plugin-create/skills/plugin-create-for-l3dg3rr/SKILL.md)
- MCP server entrypoint: [ledgerr-mcp-server.rs](crates/ledgerr-mcp/src/bin/ledgerr-mcp-server.rs)
- Runtime helper commands: [Justfile](Justfile)
- MCP regression script: [mcp_e2e.sh](scripts/mcp_e2e.sh)
- Container build: [Containerfile](../Containerfile)
- Python launcher package: [pyproject.toml](plugins/l3dg3rr-plugin-create/python/pyproject.toml)

## Install in Cowork

```text
/plugin marketplace add https://github.com/PromptExecution/ledgrrr
/plugin install l3dg3rr-plugin-create@promptexecution-fdkms
```

Validate:

```text
/plugin list
/plugin show l3dg3rr-plugin-create
```

## MCP runtime profiles

The plugin manifest ships multiple MCP server profiles:

- `l3dg3rr-cargo` (default development path)
- `l3dg3rr-binary` (compiled release binary)
- `l3dg3rr-docker` (container runtime)
- `l3dg3rr-python` (python launcher wrapper)

### 1) Cargo

```bash
just mcp-build
just mcp-start
```

### 2) Compiled binary

```bash
cargo build --release -p ledgerr-mcp --bin ledgerr-mcp-server
just mcp-start-release
```

### 3) Docker

```bash
docker build -f Containerfile -t tax-ledger:dev .
docker run -i --rm -v "$PWD:/workspace" -w /workspace tax-ledger:dev \
  cargo run -p ledgerr-mcp --bin ledgerr-mcp-server
```

### 4) Python packaging / launcher

Install local launcher package:

```bash
uv pip install -e plugins/l3dg3rr-plugin-create/python
```

Run:

```bash
l3dg3rr-mcp --mode cargo
l3dg3rr-mcp --mode binary --binary ./target/release/ledgerr-mcp-server
l3dg3rr-mcp --mode docker --image tax-ledger:dev
```

## Start/stop MCP interface

- Start (foreground): one of the runtime commands above.
- Stop (foreground): `Ctrl+C`.
- Stop (best-effort process kill): `just mcp-stop`.

In Cowork, process lifecycle is typically managed by Claude once the plugin is installed and selected.

## Validation in Cowork

After install, in a Cowork task:

```text
tools/list
tools/call ledgerr_documents {"action":"pipeline_status"}
tools/call ledgerr_documents {"action":"list_accounts"}
```

Optional raw-context retrieval check (after an ingest tool call writes a `.rkyv` path within your workbook directory):

```text
tools/call ledgerr_documents {"action":"get_raw_context","rkyv_ref":"relative/path/to/context.rkyv"}
```

Then run deeper checks from shell:

```bash
just test
```

## References

- Plugin marketplaces docs: https://code.claude.com/docs/en/plugin-marketplaces
- Cowork plugin usage: https://support.claude.com/en/articles/13837440-use-plugins-in-cowork
- Claude Code GitHub Action: https://github.com/marketplace/actions/claude-code-action-official
