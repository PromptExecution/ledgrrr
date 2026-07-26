#!/usr/bin/env bash
set -euo pipefail

# ─── package-desktop-agent.sh ──────────────────────────────────────────────────
# Builds ledgrrr-desktop-agent and assembles a .mcpb directory for Claude Desktop.
#
# The desktop-agent MCPB is distinct from the ledgerr-mcp-server (domain server)
# MCPB.  Use this package when you want Claude Desktop to control the l3dg3rr
# desktop host, not the full ledger dataplane.
#
# Output: dist/ledgerr-desktop-agent.mcpb/
# ────────────────────────────────────────────────────────────────────────────────

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIST_DIR="$REPO_ROOT/dist"
OUT_DIR="$DIST_DIR/ledgerr-desktop-agent.mcpb"
BINARY_NAME="ledgerr-desktop-agent"

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

# 4. Write manifest (clearly identifies as desktop-agent, NOT domain server)
echo "[3/3] Writing manifest.json..."
VERSION="${1:-$(cd "$REPO_ROOT" && (cog get-version 2>/dev/null || echo "0.0.0"))}"
cat > "$OUT_DIR/manifest.json" <<MANIFEST_EOF
{
  "manifest_version": "0.3",
  "name": "ledgerr-desktop-agent",
  "version": "${VERSION}",
  "description": "Claude Desktop controller for l3dg3rr — lightweight stdio agent for desktop host control.  NOT the ledgerr-mcp domain server (that is a separate .mcpb for the full tax ledger dataplane).",
  "author": {
    "name": "Prompt Execution Pty Ltd.",
    "url": "https://github.com/PromptExecution/l3dg3rr"
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
