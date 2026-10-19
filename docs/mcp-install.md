# ledgerr-mcp Install Guide

> How to build, bundle, install, and verify the ledgerr MCP server.

## Architecture overview

```
  agent / Claude Desktop
        │  JSON-RPC 2.0 over stdio
        ▼
  ledgerr-mcp-server  (single binary)
        │
        ├── reads LEDGERR_MCP_MANIFEST (optional) for workspace config
        └── writes all state to the local filesystem — no cloud dependency
```

The server exposes the `ledgerr_*` tool catalog over the MCP stdio transport.
Every tool receives an `action` argument; the published catalog is in
[`docs/mcp-capability-contract.md`](mcp-capability-contract.md).

## Build from source

```bash
# 1. Compile the release binary
cargo build -p ledgerr-mcp --bin ledgerr-mcp-server --release

# The binary is now at:
#   target/release/ledgerr-mcp-server
```

## Bundle into a .mcpb installer

The `.mcpb` format is a ZIP that includes the binary and a `manifest.json`.
Claude Desktop and compatible MCP clients can install directly from the bundle.

```bash
cargo xtask bundle \
  --binary target/release/ledgerr-mcp-server \
  --output dist/ledgerr-mcp.mcpb \
  --version v1.10.0

# Output:
#   bundled: dist/ledgerr-mcp.mcpb
#   sha256:  <hex>
#   size:    <bytes>
```

Verify the bundle is well-formed:

```bash
cargo xtask verify dist/ledgerr-mcp.mcpb
# ok: dist/ledgerr-mcp.mcpb (ledgerr-mcp v1.10.0)
```

## Install for Claude Desktop

1. Build and bundle as above (or download a release artifact from GitHub).
2. In Claude Desktop → Settings → Developer → MCP Servers → "Install from file",
   select `dist/ledgerr-mcp.mcpb`.
3. Claude Desktop extracts the bundle and registers the server. The server
   command in the installed `mcp.json` will be:
   ```json
   { "command": "${__dirname}/server/ledgerr-mcp-server", "args": [] }
   ```
4. Restart Claude Desktop. The `ledgerr_*` tools appear in the tool list.

### Optional: point the server at a workspace

```bash
export LEDGERR_MCP_MANIFEST=/path/to/ledgrrr-manifest.toml
```

Without this, the server starts in single-user mode and creates a default
manifest on first use.

## Client interface: JSON-RPC 2.0 over stdio

The server speaks MCP (JSON-RPC 2.0) on stdin/stdout.

**Required startup sequence:**

```jsonc
// → initialize
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0.0.1"}}}

// ← server replies with capabilities

// → notifications/initialized  (no reply expected)
{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}

// → list available tools
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}

// ← server replies with the ledgerr_* catalog

// → call a tool
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ledgerr_documents","arguments":{"action":"pipeline_status"}}}
```

Each JSON message is one line terminated by `\n`.

## Verify installation end-to-end (CI / smoke test)

The repo includes a binary-spawning integration test that exercises the
exact install path — same binary, same stdio protocol — without an IDE:

```bash
cargo test -p ledgerr-mcp --test install_e2e -- --nocapture
```

Expected output:

```
initialize ok
tools/list ok: 17 tools published
ledgerr_documents/pipeline_status ok
PASS: ledgerr-mcp-server install path verified
```

For the full MCP stdio test suite:

```bash
cargo test -p ledgerr-mcp --test mcp_stdio_e2e
```

## Functional test demo (manual)

The generated script exercises the full tool catalog via the compiled binary:

```bash
bash scripts/mcp_cli_demo.sh basic
```

This spawns `target/debug/ledgerr-mcp-server`, sends the initialize handshake,
lists tools, and calls several tools in sequence — all via stdio.
