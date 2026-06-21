# ── ledgrrr — ledgerr-mcp lifecycle + viz dashboard + test harness ────────
# Just module — shared across vendor/l3dg3rr (symlink: vendor/ledgrrr).
#
# Module pattern (canonical):
#   In parent justfile:  mod ledgrrr 'vendor/ledgrrr/ledgrrr.just'
#   Invocation:          just ledgrrr <recipe>
#   List recipes:        just --list ledgrrr
#
# When to use a module vs inline:
#   - VENDOR-SCOPED recipes → module in vendor/<name>/<name>.just
#     e.g. vendor/ledgrrr/ledgrrr.just, vendor/irontology-mcp/irontology.just
#   - ROOT-SCOPED recipes with cross-cutting concerns → inline in justfile
#     e.g. publish-dry-run, ansible-k0s
#   - SUBSYSTEM lifecycle (build/test/start/stop/viz) → module
#
# Rationale:
#   - Prevents justfile bloat (b00t justfile was 1169 lines)
#   - Recipes run in module's source directory automatically
#   - Module recipes are namespaced: `just ledgrrr viz` not `just ledgrrr-viz`
#   - Vendors own their justfiles; root justfile only adds `mod <name>` line
#
# Reference: https://just.systems/man/en/modules.html

# ─── Build ─────────────────────────────────────────────────────────────────────

# Build ledgerr-mcp binary from vendor workspace
build:
    cargo build --release -p ledgerr-mcp

# Build Docker image for ledgerr-mcp
docker-build:
    docker build -f Dockerfile.ledgerr-mcp -t ledgerr-mcp:latest .

# ─── Start/Stop ────────────────────────────────────────────────────────────────

# Run ledgerr-mcp container on port 8001
docker-run:
    docker rm -f ledgerr-mcp 2>/dev/null || true
    docker run -d --name ledgerr-mcp -p 8001:8001 ledgerr-mcp:latest
    echo "started on :8001"

# Stop and remove ledgerr-mcp container
docker-stop:
    docker rm -f ledgerr-mcp 2>/dev/null || echo "no container"

# Rebuild and restart
docker-restart: docker-build docker-stop docker-run

# ─── Status ────────────────────────────────────────────────────────────────────

# Check if ledgerr-mcp container is running
docker-status:
    docker inspect ledgerr-mcp --format '{{ "{{" }}.State.Status{{ "}}" }}' 2>/dev/null || echo "not running"

# Follow container logs
docker-logs:
    docker logs -f ledgerr-mcp

# ─── Install ───────────────────────────────────────────────────────────────────

# Build native binary and copy to ~/.cargo/bin
install: build
    cp target/release/ledgerr-mcp-server {{env_var_or_default("HOME", "~")}}/.local/bin/ledgerr-mcp-server
    echo "installed to ~/.local/bin/ledgerr-mcp-server"

# ─── Viz Dashboard Server ──────────────────────────────────────────────────────

# Start HTTP + CDP-compatible viz dashboard (default :8080)
viz PORT="8080":
    python3 scripts/ledgrrr-viz-serve.py start --port={{PORT}}

# Stop the viz dashboard server
viz-stop:
    python3 scripts/ledgrrr-viz-serve.py stop

# Show viz dashboard server status
viz-status:
    python3 scripts/ledgrrr-viz-serve.py status

# Regenerate dashboard HTML from the holon-viz demo binary
viz-build:
    cargo run -p holon-viz --bin holon-viz-demo

# ─── Test Harness ──────────────────────────────────────────────────────────────

# Run comprehensive MECE test suite (42 tests)
test:
    cargo test --test holon_viz_comprehensive -p holon-viz

# Run ALL holon-viz tests (unit + integration)
test-all:
    cargo test -p holon-viz

# ─── Lifecycle ─────────────────────────────────────────────────────────────────

# Show subsystem status (repo, binary, running)
status:
    @printf "Ledgrrr subsystem status:\n"
    @if [ -d .git ]; then printf "  ✅  Repo: %s\n" "$$(pwd)"; else printf "  ❌  Repo missing\n"; fi
    @if [ -f target/release/ledgerr-mcp-server ]; then \
        ver=$$(strings target/release/ledgerr-mcp-server 2>/dev/null | grep -E '^\d+\.\d+\.\d+' | head -1 || echo "built"); \
        printf "  ✅  Binary: %s (%s)\n" "target/release/ledgerr-mcp-server" "$$ver"; \
    else printf "  ❌  Binary not built (run: just ledgrrr build)\n"; fi
    @if pidof ledgerr-mcp-server >/dev/null 2>&1; then \
        printf "  ✅  Running (pid %s)\n" "$$(pidof ledgerr-mcp-server)"; \
    else printf "  ○   Not running\n"; fi
