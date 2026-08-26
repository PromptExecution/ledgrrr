#!/usr/bin/env bash
set -euo pipefail

git config core.hooksPath .githooks
chmod +x .githooks/commit-msg .githooks/pre-commit
echo "Git hooks installed: core.hooksPath=.githooks"
echo "Conventional commit enforcement is active."
echo "Generated-artifact drift check (docs/mcp-capability-contract.md, viz-manifest.json) runs on every commit."

