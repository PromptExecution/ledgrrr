#!/usr/bin/env bash
set -euo pipefail

# ─── package-desktop-agent.sh ──────────────────────────────────────────────────
# Builds ledgrrr-mcp and assembles the ledgrrr-claude.mcpb bundle for Claude
# Desktop, per PRD-10 §3.1.
#
# The desktop controller MCPB is distinct from the ledgerr-mcp-server (domain
# server) MCPB built by `just bundle`: this one exposes the eleven ledgrrr_*
# desktop-control tools (status/install/service/tray/render/simulate/office),
# not the full ledger dataplane.
#
# Output: dist/ledgrrr-claude.mcpb/
# ────────────────────────────────────────────────────────────────────────────────

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIST_DIR="$REPO_ROOT/dist"
OUT_DIR="$DIST_DIR/ledgrrr-claude.mcpb"
BINARY_NAME="ledgrrr-mcp"

echo "=== package-desktop-agent ==="

# 1. Build release binary
echo "[1/3] Building $BINARY_NAME (release)..."
cargo build --release -p ledgerr-desktop-agent --bin "$BINARY_NAME" 2>&1

# 2. Create output directory
echo "[2/3] Creating $OUT_DIR..."
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR/server"

# 3. Copy binary
cp "$REPO_ROOT/target/release/$BINARY_NAME" "$OUT_DIR/server/$BINARY_NAME"
chmod 755 "$OUT_DIR/server/$BINARY_NAME"

# 4. Write manifest (PRD-10 §3.1 shape)
echo "[3/3] Writing manifest.json..."
VERSION="${1:-$(cd "$REPO_ROOT" && (cog get-version 2>/dev/null || echo "0.0.0"))}"
cat > "$OUT_DIR/manifest.json" <<MANIFEST_EOF
{
  "manifest_version": "0.3",
  "name": "${BINARY_NAME}",
  "version": "${VERSION}",
  "description": "Claude Desktop controller for l3dg3rr — stdio MCP server exposing the eleven ledgrrr_* desktop-control tools (PRD-10 §3.1). NOT the ledgerr-mcp-server domain server (that is a separate .mcpb for the full tax ledger dataplane, built by 'just bundle').",
  "author": {
    "name": "Prompt Execution Pty Ltd.",
    "url": "https://github.com/PromptExecution/ledgrrr"
  },
  "server": {
    "type": "binary",
    "entry_point": "server/${BINARY_NAME}",
    "mcp_config": {
      "command": "\${__dirname}/server/${BINARY_NAME}",
      "args": []
    }
  }
}
MANIFEST_EOF

echo ""
echo "=== done ==="
echo "  bundle: $OUT_DIR/"
echo "  binary: $OUT_DIR/server/$BINARY_NAME"
echo "  manifest: $OUT_DIR/manifest.json"
ls -lh "$OUT_DIR/server/$BINARY_NAME"
