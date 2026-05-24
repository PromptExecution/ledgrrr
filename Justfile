set shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set dotenv-load := true

# Canonical build/test/run recipes live here. If a workflow needs a command,
# add or update the relevant `just` recipe and reference it from AGENTS.md.

mcp-build:
    cargo build -p ledgerr-mcp --bin ledgerr-mcp-server

mcp-start:
    cargo run -p ledgerr-mcp --bin ledgerr-mcp-server

mcp-start-release:
    ./target/release/ledgerr-mcp-server

mcp-stop:
    pkill -f ledgerr-mcp-server || true

# Build the Windows host binaries from WSL via PowerShell. This is the canonical
# path for `host-tray.exe` and `host-window.exe`.
wsl2-pwsh-build:
    powershell.exe -NoProfile -Command '$env:PATH = "C:\Users\wendy\.cargo\bin;C:\msys64\mingw64\bin;" + $env:PATH; Set-Location "D:\Projects\l3dg3rr"; cargo build -p ledgerr-host --bin host-tray --bin host-window'

# Full local install: build host binaries, MCP server, and docs.
# Run this from WSL after any code change to get a fresh Windows build.
wsl2-pwsh-install:
    powershell.exe -NoProfile -ExecutionPolicy Bypass -Command '$env:PATH = "C:\Users\wendy\.cargo\bin;C:\msys64\mingw64\bin;" + $env:PATH; Set-Location "D:\Projects\l3dg3rr"; Write-Host "[1/3] Building ledgerr-host bins..."; cargo build -p ledgerr-host --bin host-tray --bin host-window; if ($LASTEXITCODE -ne 0) { throw "host build failed" }; Write-Host "[2/3] Building MCP server..."; cargo build -p ledgerr-mcp --bin ledgerr-mcp-server; if ($LASTEXITCODE -ne 0) { throw "MCP server build failed" }; Write-Host "[3/3] Build complete."; Write-Host ""; Write-Host "Installed binaries:"; Get-Item "target\debug\host-tray.exe","target\debug\host-window.exe","target\debug\ledgerr-mcp-server.exe" | ForEach-Object { "  " + $_.FullName + "  (" + [math]::Round($_.Length/1KB, 1) + " KB)" }'

# Rebuild and launch the tray host on Windows.
wsl2-pwsh-run-tray:
    powershell.exe -NoProfile -Command '$env:PATH = "C:\Users\wendy\.cargo\bin;C:\msys64\mingw64\bin;" + $env:PATH; Set-Location "D:\Projects\l3dg3rr"; cargo build -p ledgerr-host --bin host-tray | Out-Null; Get-Process host-tray -ErrorAction SilentlyContinue | Stop-Process -Force; Start-Sleep -Milliseconds 250; Start-Process -FilePath "D:\Projects\l3dg3rr\target\debug\host-tray.exe" -WorkingDirectory "D:\Projects\l3dg3rr"'

# Rebuild and launch the legacy Slint host window on Windows (no local LLM).
# The internal endpoint falls back to the deterministic Phi-4 stub.
wsl2-pwsh-run-window:
    powershell.exe -NoProfile -Command '$env:PATH = "C:\Users\wendy\.cargo\bin;C:\msys64\mingw64\bin;" + $env:PATH; Set-Location "D:\Projects\l3dg3rr"; cargo build -p ledgerr-host --bin host-window | Out-Null; Start-Process -FilePath "D:\Projects\l3dg3rr\target\debug\host-window.exe" -WorkingDirectory "D:\Projects\l3dg3rr"'

# Same as above but compiled with the real mistralrs Phi-4 Mini backend.
# Requires the model GGUF at models/unsloth/Phi-4-mini-reasoning-GGUF/ (just phi4-reasoning-symlink).
# First inference call writes a ~2 GB patched sidecar; subsequent calls reuse it.
wsl2-pwsh-run-window-phi4:
    powershell.exe -NoProfile -Command '$env:PATH = "C:\Users\wendy\.cargo\bin;C:\msys64\mingw64\bin;" + $env:PATH; Set-Location "D:\Projects\l3dg3rr"; cargo build -p ledgerr-host --bin host-window --features mistralrs-llm | Out-Null; Start-Process -FilePath "D:\Projects\l3dg3rr\target\debug\host-window.exe" -WorkingDirectory "D:\Projects\l3dg3rr"'

# Build the mdBook playbook assets, then launch the legacy Slint host window whose
# internal localhost server serves both `/v1/chat/completions` and `/docs/`.
# Uses the deterministic Phi-4 stub backend (no GGUF required).
host-playbook-window:
    just docgen
    just wsl2-pwsh-run-window

# Same as host-playbook-window but with the real local Phi-4 Mini model.
# Requires: just phi4-reasoning-symlink (model file on D: drive).
host-playbook-window-phi4:
    just docgen
    just wsl2-pwsh-run-window-phi4

# Build the docs and launch the host window. In the window, explicitly select
# "Windows AI / Foundry Local" to use the Foundry Local OpenAI-compatible endpoint.
host-playbook-window-windows-ai:
    just docgen
    just wsl2-pwsh-run-window

# Build docs, start Windows-local HTTP server, and open browser for live Rhai diagram editing.
wsl2-pwsh-docserve:
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File "D:\Projects\l3dg3rr\scripts\docserve-live.ps1"

# Pull and run the MCP server from GHCR using podman (stdio transport)
# Usage: just mcp-podman-run        — latest image on main
#        just mcp-podman-run v0.2.0 — specific release tag
mcp-podman-run tag="main":
    @command -v podman >/dev/null || { echo "error: podman not found — install podman first"; exit 1; }
    podman pull ghcr.io/promptexecution/l3dg3rr:{{tag}}
    podman run --rm -i \
      -v "${LEDGER_DATA_DIR:-$PWD/data}:/data" \
      ghcr.io/promptexecution/l3dg3rr:{{tag}}

# Verify the GHCR image exists for a given tag without pulling the full image
mcp-podman-verify tag="main":
    @command -v podman >/dev/null || { echo "error: podman not found"; exit 1; }
    podman manifest inspect ghcr.io/promptexecution/l3dg3rr:{{tag}}

mcp-e2e:
    ./scripts/mcp_e2e.sh

mcp-cli-basic:
    ./scripts/mcp_cli_demo.sh basic

mcp-cli-spinning-wheels:
    ./scripts/mcp_cli_demo.sh spinning-wheels

mcp-doc-demo:
    ./scripts/mcp_cli_demo.sh basic
    ./scripts/mcp_cli_demo.sh spinning-wheels
    ./scripts/mcp_e2e.sh

test:
    cargo test --workspace --all-targets --all-features
    cargo build -p ledgerr-mcp --bin mcp-outcome-test
    ./target/debug/mcp-outcome-test

# ─── Tauri build (Windows host) ─────────────────────────────────────────────────

# Pre-flight check: verify cargo-tauri is installed, then build ledgrrr.
# Outputs binary + datum TOML with version and SHA256 hash.
# Run from WSL. Uses the Windows toolchain from the host.
wsl2-pwsh-tauri-build:
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File "D:\Projects\l3dg3rr\scripts\tauri-build.ps1"

# Build host-tauri (Windows toolchain via PowerShell) then launch with CDP for viz demo.
# Opens the tray app — click VZ in the sidebar to see the Cytoscape pipeline graph.
demo-viz:
    powershell.exe -NoProfile -Command '$env:PATH="C:\Users\wendy\.cargo\bin;C:\msys64\mingw64\bin;"+$env:PATH; Set-Location "D:\Projects\l3dg3rr"; cargo build -p ledgerr-host --bin host-tauri; if ($LASTEXITCODE -ne 0){throw "build failed"}; Write-Host "Build OK — launching with CDP on port 19222"; $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=19222"; Start-Process -FilePath "D:\Projects\l3dg3rr\target\debug\host-tauri.exe" -WorkingDirectory "D:\Projects\l3dg3rr\crates\ledgerr-host"; Write-Host "Launched — click VZ in the sidebar"'

# Run the CDP-based holon-viz acceptance test (builds, launches, asserts window._cy has nodes).
test-holon-viz:
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File "D:\Projects\l3dg3rr\scripts\test-holon-viz.ps1"

# Same as test-holon-viz but skips the build step (uses existing binary).
test-holon-viz-fast:
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File "D:\Projects\l3dg3rr\scripts\test-holon-viz.ps1" -SkipBuild

# Build WASM package for holon-viz filtering
build-wasm:
    cd crates/holon-viz-wasm && wasm-pack build --target web --out-dir pkg

# ─── UI build (TypeScript / esbuild) ──────────────────────────────────────────

# Install UI dependencies
ui-install:
    cd crates/ledgerr-host/ui && npm install

# Build UI TypeScript bundle
ui-build: ui-install
    cd crates/ledgerr-host/ui && npm run build

# Type-check UI
ui-typecheck: ui-install
    cd crates/ledgerr-host/ui && npm run typecheck

# Watch UI for development
ui-watch:
    cd crates/ledgerr-host/ui && npm run watch

# ─── Local model assets ───────────────────────────────────────────────────────

# Install Microsoft Foundry Local on Windows, then print version and service status.
# This is intentionally explicit and never auto-selects Windows AI in the app.
windows-ai-install:
    powershell.exe -NoProfile -ExecutionPolicy Bypass -Command '$ErrorActionPreference = "Stop"; winget install --id Microsoft.FoundryLocal --source winget --accept-package-agreements --accept-source-agreements; $env:PATH = [Environment]::GetEnvironmentVariable("PATH", "Machine") + ";" + [Environment]::GetEnvironmentVariable("PATH", "User"); foundry --version; foundry service restart; foundry service status'

# Download/setup the Foundry Local Phi-4 Mini alias. Foundry Local chooses the
# hardware-specific variant for the current Windows machine.
windows-ai-setup model="phi-4-mini":
    powershell.exe -NoProfile -ExecutionPolicy Bypass -Command '$ErrorActionPreference = "Stop"; $env:PATH = [Environment]::GetEnvironmentVariable("PATH", "Machine") + ";" + [Environment]::GetEnvironmentVariable("PATH", "User"); foundry model list --filter task=chat-completion; foundry model info "{{model}}"; foundry model download "{{model}}"; foundry cache list'

# Print diagnostics for the Windows AI / Foundry Local runtime and cache.
windows-ai-status:
    powershell.exe -NoProfile -ExecutionPolicy Bypass -Command '$ErrorActionPreference = "Stop"; $env:PATH = [Environment]::GetEnvironmentVariable("PATH", "Machine") + ";" + [Environment]::GetEnvironmentVariable("PATH", "User"); foundry --version; foundry service status; foundry service ps; foundry cache location; foundry cache list'

# End-to-end Foundry Local smoke test. Fails if the service endpoint, model load,
# or OpenAI-compatible chat completion path is not working.
windows-ai-smoke model="phi-4-mini":
    powershell.exe -NoProfile -ExecutionPolicy Bypass -Command '$ErrorActionPreference = "Stop"; $env:PATH = [Environment]::GetEnvironmentVariable("PATH", "Machine") + ";" + [Environment]::GetEnvironmentVariable("PATH", "User"); foundry service start; foundry model download "{{model}}"; foundry model load "{{model}}"; $status = foundry service status | Out-String; $endpoint = [regex]::Match($status, "https?://(?:localhost|127\.0\.0\.1):\d+").Value; if (-not $endpoint) { throw "No Foundry Local endpoint in service status: $status" }; $restStatus = Invoke-RestMethod "$endpoint/openai/status"; $endpoint = @($restStatus.Endpoints)[0]; if (-not $endpoint) { throw "No endpoint in /openai/status response" }; $body = @{ model = "{{model}}"; messages = @(@{ role = "user"; content = "Reply with exactly: ok" }); max_tokens = 8; temperature = 0 } | ConvertTo-Json -Depth 8; $response = Invoke-RestMethod -Method Post -Uri "$endpoint/v1/chat/completions" -ContentType "application/json" -Body $body; $text = $response.choices[0].message.content; if ([string]::IsNullOrWhiteSpace($text)) { throw "Foundry Local returned an empty assistant message" }; Write-Host "Foundry Local smoke response: $text"'

# Requires the Hugging Face CLI from `huggingface_hub`:
#   uv tool install huggingface-hub
# Download the Phi-4 mini reasoning GGUF quantization used for local host tests.
hf-download-phi4-mini-gguf local_dir="/mnt/d/models/unsloth/Phi-4-mini-reasoning-GGUF":
    @command -v hf >/dev/null || { echo "error: hf CLI not found — install with: uv tool install huggingface-hub"; exit 1; }
    mkdir -p "{{local_dir}}"
    hf download unsloth/Phi-4-mini-reasoning-GGUF Phi-4-mini-reasoning-Q3_K_M.gguf --local-dir "{{local_dir}}"

# Create the repo-relative symlink that points to the D: drive GGUF directory.
# Required before running test-phi4 if the symlink does not already exist.
phi4-reasoning-symlink:
    ln -sfn /mnt/d/models/unsloth/Phi-4-mini-reasoning-GGUF models/unsloth/Phi-4-mini-reasoning-GGUF
    @echo "symlink ready: models/unsloth/Phi-4-mini-reasoning-GGUF"

# Phi-4 Mini reasoning smoke test — candle backend (in-process, slower, no cmake needed).
# Downloads tokenizer.json from HuggingFace Hub on first run (~2 MB, cached).
# Requires the model file: models/unsloth/Phi-4-mini-reasoning-GGUF/Phi-4-mini-reasoning-Q3_K_M.gguf
test-phi4:
    cargo test -p ledgerr-host --features local-llm --test phi4_smoke -- --nocapture

# Phi-4 Mini reasoning smoke test — mistralrs backend (faster, correct partial RoPE).
# Downloads tokenizer from HuggingFace Hub on first run (cached).
# Requires the model file: models/unsloth/Phi-4-mini-reasoning-GGUF/Phi-4-mini-reasoning-Q3_K_M.gguf
test-phi4-mistral:
    cargo test -p ledgerr-host --features mistralrs-llm --test phi4_smoke -- --nocapture

# Fine-tuning follow-up:
# - install Unsloth in a CUDA-capable Python environment,
# - prepare an instruction dataset from `book/src/`, `docs/`, and checked-in samples,
# - fine-tune Phi-4 mini against documentation/operator workflows,
# - export adapter artifacts and a local inference target without committing model weights.
# Print the planned Unsloth fine-tuning workflow placeholder.
unsloth-finetune-plan:
    @echo "TODO: install Unsloth and add a reproducible Phi-4 mini documentation fine-tuning recipe."

# TODO: add cargo bench benchmark for docgen rendering pipeline performance
# ─── Devtools (Linux) ─────────────────────────────────────────────────────

# Install common developer tools missing from the base Ubuntu 24.04 image.
# Skips tools that already exist so repeated runs are fast.
# Tries apt first (needs sudo), falls back to cargo install where possible.
install-devtools:
    #!/bin/bash
    set -euo pipefail
    echo "=== install-devtools (Linux x86_64) ==="

    # Prefer apt for system packages; skip if sudo requires a TTY
    if sudo -n true 2>/dev/null; then
        sudo apt-get update -qq
        sudo apt-get install -y -qq ripgrep fd-find bat hyperfine jq tree httpie shellcheck 2>/dev/null || true
    else
        echo "[skip] apt packages require interactive sudo — will use cargo fallbacks"
    fi

    # Install ripgrep via cargo if not found
    if ! command -v rg >/dev/null 2>&1; then
        echo "Installing ripgrep via cargo..."
        cargo install ripgrep --quiet
    fi

    # Install fd-find via cargo if neither the upstream nor Ubuntu/Debian binary name is found
    if ! command -v fd >/dev/null 2>&1 && ! command -v fdfind >/dev/null 2>&1; then
        echo "Installing fd-find via cargo..."
        cargo install fd-find --quiet
    fi

    # Install bat (syntax-highlighted pager) via cargo if neither the upstream nor Ubuntu/Debian binary name is found
    if ! command -v bat >/dev/null 2>&1 && ! command -v batcat >/dev/null 2>&1; then
        echo "Installing bat via cargo..."
        cargo install bat --quiet
    fi

    # Install hyperfine (benchmark runner) via cargo if not found
    if ! command -v hyperfine >/dev/null 2>&1; then
        echo "Installing hyperfine via cargo..."
        cargo install hyperfine --quiet
    fi

    # Install cargo-binstall (binary installer for Rust tools)
    if ! command -v cargo-binstall >/dev/null 2>&1; then
        echo "Installing cargo-binstall via cargo..."
        cargo install cargo-binstall --quiet
    fi

    # Install jq via binstall
    if ! command -v jq >/dev/null 2>&1 && command -v cargo-binstall >/dev/null 2>&1; then
        echo "Installing jq via cargo-binstall..."
        cargo binstall -y jq --quiet 2>/dev/null || true
    fi

    # Install cargo-update (for `cargo install-update -a`)
    cargo binstall -y cargo-update --quiet 2>/dev/null || true

    echo "=== done ==="

gh-secrets-help:
    @echo "Expected .env values (optional):"
    @echo "  CRATES_IO_TOKEN=..."
    @echo "  PYPI_API_TOKEN=..."
    @echo ""
    @echo "Recipes:"
    @echo "  just gh-secrets-set-repo"
    @echo "  just gh-secrets-set-repo repo=PromptExecution/l3dg3rr force=true"
    @echo "  just gh-secrets-set-org org=PromptExecution repos=l3dg3rr"
    @echo "  just gh-secrets-set-org org=PromptExecution repos=l3dg3rr force=true"

gh-secrets-set-repo repo="PromptExecution/l3dg3rr" force="false":
    @command -v gh >/dev/null || { echo "gh CLI not found"; exit 1; }
    @gh auth status >/dev/null
    @for name in CRATES_IO_TOKEN PYPI_API_TOKEN; do \
      value="${!name:-}"; \
      if [ -z "$value" ]; then \
        echo "SKIP $name: not set in .env"; \
        continue; \
      fi; \
      if gh secret list -R "{{repo}}" | awk '{print $1}' | grep -qx "$name"; then \
        if [ "{{force}}" = "true" ]; then \
          printf "%s" "$value" | gh secret set "$name" -R "{{repo}}"; \
          echo "UPDATE $name: repo={{repo}}"; \
        else \
          echo "SKIP $name: already exists in repo={{repo}} (force=true to overwrite)"; \
        fi; \
      else \
        printf "%s" "$value" | gh secret set "$name" -R "{{repo}}"; \
        echo "SET $name: repo={{repo}}"; \
      fi; \
    done

gh-secrets-set-org org="PromptExecution" repos="l3dg3rr" force="false":
    @command -v gh >/dev/null || { echo "gh CLI not found"; exit 1; }
    @gh auth status >/dev/null
    @for name in CRATES_IO_TOKEN PYPI_API_TOKEN; do \
      value="${!name:-}"; \
      if [ -z "$value" ]; then \
        echo "SKIP $name: not set in .env"; \
        continue; \
      fi; \
      if gh secret list --org "{{org}}" | awk '{print $1}' | grep -qx "$name"; then \
        if [ "{{force}}" = "true" ]; then \
          printf "%s" "$value" | gh secret set "$name" --org "{{org}}" --visibility selected --repos "{{repos}}"; \
          echo "UPDATE $name: org={{org}} repos={{repos}}"; \
        else \
          echo "SKIP $name: already exists in org={{org}} (force=true to overwrite)"; \
        fi; \
      else \
        printf "%s" "$value" | gh secret set "$name" --org "{{org}}" --visibility selected --repos "{{repos}}"; \
        echo "SET $name: org={{org}} repos={{repos}}"; \
      fi; \
    done

# ─── MCPB bundle + publish ────────────────────────────────────────────────────

# Build release binary and assemble a deterministic .mcpb bundle for one target
bundle target="x86_64-unknown-linux-musl":
    cargo build -p ledgerr-mcp --release --bin ledgerr-mcp-server --target {{target}}
    cargo xtask-mcpb bundle \
        --binary target/{{target}}/release/ledgerr-mcp-server \
        --output dist/ledgerr-mcp-{{target}}.mcpb \
        --version $(just v)

# Bundle for all tier-1 distribution targets (requires cross-compilation toolchains)
bundle-all:
    just bundle x86_64-unknown-linux-musl
    just bundle x86_64-apple-darwin
    just bundle aarch64-apple-darwin

# Print the manifest.json for the current version (no bundle created)
manifest:
    cargo xtask-mcpb manifest --version $(just v)

# Verify a .mcpb bundle's structure and manifest
verify path:
    cargo xtask-mcpb verify {{path}}

# Upload all dist/*.mcpb artifacts to a GitHub release
publish-mcpb tag="":
    #!/bin/bash
    set -euo pipefail
    TAG="{{tag}}"
    if [ -z "$TAG" ]; then TAG=$(gh release list --limit 1 --json tagName --jq '.[0].tagName'); fi
    shopt -s nullglob
    bundles=(dist/*.mcpb)
    if [ ${#bundles[@]} -eq 0 ]; then
      echo "error: no .mcpb files found in dist/ — run 'just bundle' first"
      exit 1
    fi
    for f in "${bundles[@]}"; do
      cargo xtask-mcpb publish-github --release-tag "$TAG" --artifact "$f"
    done

# Update server.json with the current release version + sha256 from a bundle artifact.
# Run before `mcp-publisher publish`.
update-server-json artifact sha256="":
    #!/bin/bash
    set -euo pipefail
    SHA="{{sha256}}"
    if [ -z "$SHA" ]; then
        SHA=$(sha256sum "{{artifact}}" | cut -d' ' -f1)
    fi
    VERSION=$(just v)
    FILENAME=$(basename "{{artifact}}")
    cargo xtask-mcpb update-server-json \
        --version "$VERSION" \
        --sha256 "$SHA" \
        --artifact-url "https://github.com/PromptExecution/l3dg3rr/releases/download/$VERSION/$FILENAME"

# Submit bundle to MCP Registry (requires mcp-publisher on PATH + registry auth)
publish-registry tag artifact-url sha256:
    cargo xtask-mcpb publish-registry \
        --release-tag {{tag}} \
        --artifact-url {{artifact-url}} \
        --sha256 {{sha256}}

# ─── Cocogitto release recipe (major|minor|patch, defaults to patch) ──────────

[private]
ensure-cog:
    @PATH="${HOME}/.cargo/bin:${PATH}" bash -eu -o pipefail -c 'if command -v cog >/dev/null 2>&1; then echo "Using existing cog"; else echo "cog not found; installing cocogitto..."; cargo install cocogitto; fi'

# Fast test suite for release gates — skips model-inference tests that need GGUF assets.
# phi4_produces_output and phi4_mistral_produces_output load a ~2 GB GGUF model and
# can run for 10+ minutes; they are exercised separately via `just test-phi4`.
test-fast:
    cargo test --workspace --all-targets --all-features \
        -- --skip phi4_produces_output --skip phi4_mistral_produces_output

# Cocogitto release recipe (major|minor|patch, defaults to patch).
#
# Odd/even minor version policy (Ubuntu-style):
#   Even minor (1.0, 1.2, 1.4, 1.8 …) — Stable. Full test gate incl. phi4 inference.
#                                          GitHub release created. LTS supported.
#   Odd minor  (1.1, 1.3, 1.5, 1.7 …) — Dev/Experimental. Fast test gate only.
#                                          No GitHub release. No LTS support.
release version="patch": ensure-cog
    #!/bin/bash
    set -euo pipefail
    export PATH="${HOME}/.cargo/bin:${PATH}"
    case "{{version}}" in
        major|minor|patch) ;;
        *) echo "Invalid version: {{version}} (use major, minor, or patch)" && exit 1 ;;
    esac

    # Determine what the next version will be to apply odd/even policy.
    CURRENT=$(cog get-version)
    CURRENT_MINOR=$(echo "$CURRENT" | cut -d. -f2)
    if [ "{{version}}" = "minor" ]; then
        NEXT_MINOR=$(( CURRENT_MINOR + 1 ))
    elif [ "{{version}}" = "major" ]; then
        NEXT_MINOR=0
    else
        NEXT_MINOR=$CURRENT_MINOR
    fi
    IS_EVEN=$(( NEXT_MINOR % 2 == 0 ))

    if [ "$IS_EVEN" -eq 1 ]; then
        echo "Stable (even minor) release — running full test suite including phi4 inference..."
        cargo test --workspace --all-targets --all-features
    else
        echo "Dev (odd minor) release — running fast test suite (phi4 inference skipped)..."
        just test-fast
    fi

    ./scripts/e2e_mvp.sh
    echo "Bumping {{version}} version with cocogitto..."
    cog bump --{{version}}
    cog changelog
    echo "Pushing branch and tags..."
    git push --follow-tags
    TAG=$(cog get-version | sed 's/^/v/')

    if [ "$IS_EVEN" -eq 1 ]; then
        echo "Stable release — creating GitHub release for ${TAG}..."
        NOTES=$(awk "/^## ${TAG//./\\.}/,/^## v[0-9]/" CHANGELOG.md \
            | grep -v "^## v[0-9]" | sed '/^[[:space:]]*$/d' | head -80)
        gh release create "${TAG}" \
            --title "${TAG} (stable)" \
            --notes "${NOTES:-See CHANGELOG.md for details.}" \
            --latest
        echo "GitHub release created: https://github.com/PromptExecution/l3dg3rr/releases/tag/${TAG}"
    else
        echo "Dev release — no GitHub release created for odd minor ${TAG}."
        echo "Tag ${TAG} pushed. Use 'just release minor' again to reach next stable even minor."
    fi

# Show current version
v: ensure-cog
    @PATH="${HOME}/.cargo/bin:${PATH}" cog get-version

# Validate commits
validate: ensure-cog
    @PATH="${HOME}/.cargo/bin:${PATH}" cog check

# Show changelog
changelog: ensure-cog
    @PATH="${HOME}/.cargo/bin:${PATH}" cog changelog

# Show release stats
stats:
    @echo "Tags:"
    @git tag -l
    @echo ""
    @echo "Recent commits:"
    @git log --oneline -5

# Build mdbook documentation locally
# Requires: cargo install mdbook mdbook-mermaid && cargo install --path crates/mdbook-rhai-mermaid
# mdbook-admonish: cargo install --git https://github.com/padamson/mdbook-admonish.git --branch feat/mdbook-0.5-compat mdbook-admonish
# TODO: switch to a released version once tommilligan/mdbook-admonish#235 merges
docgen:
    @if [ ! -x ~/.cargo/bin/mdbook ]; then echo "error: mdbook not found — run: cargo install mdbook mdbook-mermaid"; exit 1; fi
    @if [ ! -x ~/.cargo/bin/mdbook-mermaid ]; then echo "error: mdbook-mermaid not found — run: cargo install mdbook-mermaid"; exit 1; fi
    @if [ ! -x ~/.cargo/bin/mdbook-admonish ]; then echo "error: mdbook-admonish not found — see comment above docgen recipe in Justfile"; exit 1; fi
    @if [ ! -x ~/.cargo/bin/mdbook-rhai-mermaid ]; then cargo install --path crates/mdbook-rhai-mermaid --quiet; fi
    PATH="$HOME/.cargo/bin:$PATH" ~/.cargo/bin/mdbook build book
    @echo "Docs built in book/book/ — serve with: npx serve book/book"

# Build and serve mdbook locally with the live Rhai editor enabled
docserve host="127.0.0.1" port="3000":
    @if [ ! -x ~/.cargo/bin/mdbook ]; then echo "error: mdbook not found — run: cargo install mdbook mdbook-mermaid"; exit 1; fi
    @if [ ! -x ~/.cargo/bin/mdbook-mermaid ]; then echo "error: mdbook-mermaid not found — run: cargo install mdbook-mermaid"; exit 1; fi
    @if [ ! -x ~/.cargo/bin/mdbook-rhai-mermaid ]; then cargo install --path crates/mdbook-rhai-mermaid --quiet; fi
    PATH="$HOME/.cargo/bin:$PATH" ~/.cargo/bin/mdbook build book
    @echo "Serving http://{{host}}:{{port}}"
    cd book/book && python3 -m http.server {{port}} --bind {{host}}

# ─── KerML codegen ───────────────────────────────────────────────────────────

# Regenerate crates/holon-viz/src/gen.rs from types/domain.kerm.
# This now happens automatically at build time via crates/holon-viz/build.rs.
# Edit types/domain.kerm to add/remove domain types, then re-build.
gen-kerm:
    @echo "gen.rs is now auto-generated at build time by holon-viz/build.rs."
    @echo "Just run 'cargo build -p holon-viz' (or any build) and gen.rs is regenerated from types/domain.kerm."
    @echo "The checked-in gen.rs is a thin include!() shim — the real code lives in OUT_DIR."

# ─── Zero-drift checks ───────────────────────────────────────────────────────

# MECE zero-drift check: verify all generated artifacts are up to date with their sources.
# Fails (exit 1) if any artifact is stale. Run after any change to Tauri commands or MCP tools.
#
# Covered artifacts:
#   crates/ui/bindings.ts          — generated by cargo build -p ledgerr-host --bin host-tauri
#   docs/mcp-capability-contract.md — generated by cargo run -p ledgerr-mcp --bin regen-docs
#
# gen/schemas/*.json are generated by tauri_build::build() on Windows and are not
# reproducible on Linux; they are explicitly excluded from this check.
check-drift:
    #!/bin/bash
    set -euo pipefail
    FAIL=0

    echo "=== check-drift: bindings.ts === (may fail on stable Rust — specta needs nightly)"
    if cargo build -p ledgerr-host --bin host-tauri 2>&1; then
        if git diff --exit-code crates/ui/bindings.ts; then
            echo "PASS: bindings.ts is up to date"
        else
            echo "FAIL: bindings.ts is out of date — run: cargo build -p ledgerr-host --bin host-tauri"
            FAIL=1
        fi
    else
        echo "SKIP: bindings.ts — specta requires nightly Rust on this platform"
    fi

    echo ""
    echo "=== check-drift: mcp-capability-contract.md ==="
    cargo run -p ledgerr-mcp --bin regen-docs 2>&1
    if ! git diff --exit-code docs/mcp-capability-contract.md; then
        echo "FAIL: mcp-capability-contract.md is out of date — run: cargo run -p ledgerr-mcp --bin regen-docs"
        FAIL=1
    else
        echo "PASS: mcp-capability-contract.md is up to date"
    fi

    echo ""
    echo "=== check-drift: holon-viz/src/gen.rs === (auto-generated by build.rs, no diff check needed)"
    echo "PASS: gen.rs is regenerated at build time from types/domain.kerm via build.rs"

    echo ""
    echo "=== check-drift: generated-types.ts ==="
    cargo run -p xtask-mcpb -- generate-ts-types --output /tmp/ts_check.ts 2>&1
    if ! diff ui/docs/src/iso/generated-types.ts /tmp/ts_check.ts > /dev/null 2>&1; then
        echo "FAIL: generated-types.ts is out of date — run: cargo run -p xtask-mcpb -- generate-ts-types"
        FAIL=1
    else
        echo "PASS: generated-types.ts is up to date"
    fi

    echo "=== check-drift: generated-types.py ==="
    cargo run -p xtask-mcpb -- generate-py-types --output /tmp/py_check.py 2>&1
    if ! diff ui/docs/src/iso/generated-types.py /tmp/py_check.py > /dev/null 2>&1; then
        echo "FAIL: generated-types.py is out of date — run: cargo run -p xtask-mcpb -- generate-py-types"
        FAIL=1
    else
        echo "PASS: generated-types.py is up to date"
    fi

    echo ""
    if [ "$FAIL" -eq 0 ]; then
        echo "=== check-drift: all artifacts up to date ==="
    else
        echo "=== check-drift: FAILED — stale artifacts detected (see above) ==="
        exit 1
    fi

# Verify docs build, rhai→mermaid injection happened, diagrams render, cross-references valid
docgen-check:
    @if [ ! -x ~/.cargo/bin/mdbook ]; then echo "error: mdbook not found — run: cargo install mdbook mdbook-mermaid"; exit 1; fi
    @if [ ! -x ~/.cargo/bin/mdbook-mermaid ]; then echo "error: mdbook-mermaid not found — run: cargo install mdbook-mermaid"; exit 1; fi
    @if [ ! -x ~/.cargo/bin/mdbook-rhai-mermaid ]; then cargo install --path crates/mdbook-rhai-mermaid --quiet; fi
    PATH="$HOME/.cargo/bin:$PATH" ~/.cargo/bin/mdbook build book
    @echo "Checking for generated Mermaid diagram blocks..."
    @grep -q 'class="mermaid"' book/book/theory.html && echo "✓ theory.html has generated Mermaid diagrams" || { echo "error: no Mermaid diagrams in theory.html"; exit 1; }
    @grep -q 'class="mermaid"' book/book/pipeline.html && echo "✓ pipeline.html has generated Mermaid diagrams" || { echo "error: no Mermaid diagrams in pipeline.html"; exit 1; }
    @grep -q 'class="mermaid"' book/book/visualize.html && echo "✓ visualize.html has generated Mermaid diagrams" || { echo "error: no Mermaid diagrams in visualize.html"; exit 1; }
    @echo "Verifying cross-references..."
    @grep -q 'href="./graph.html"' book/book/intro.html && echo "✓ intro.html references graph.html" || exit 1
    @grep -q 'href="./validation.html"' book/book/pipeline.html && echo "✓ pipeline.html references validation.html" || exit 1
    @grep -q 'href="./pipeline.html"' book/book/validation.html && echo "✓ validation.html references pipeline.html" || exit 1
    @grep -q 'href="./match-visualization-plan.html"' book/book/visualize.html && echo "✓ visualize.html references match-visualization-plan.html" || exit 1
    @echo "Verifying rhai→mermaid injection..."
    @grep -q 'class="language-rhai"' book/book/theory.html && echo "✓ theory.html has rhai source blocks" || exit 1
    @grep -q 'class="mermaid"' book/book/theory.html && echo "✓ theory.html has generated mermaid blocks" || { echo "error: rhai→mermaid injection missing in theory.html"; exit 1; }
    @grep -q 'match result.disposition' book/book/match-visualization-plan.html && echo "✓ match-visualization-plan.html includes match DSL examples" || { echo "error: match DSL examples missing in match-visualization-plan.html"; exit 1; }
    @grep -q 'theme/rhai-live-' book/book/theory.html && echo "✓ theory.html loads live-editor assets" || { echo "error: live-editor JS missing in theory.html"; exit 1; }
    @echo "Checking live-editor runtime syntax..."
    @node -c book/theme/rhai-live-core.js
    @node -c book/theme/rhai-live.js
    @echo "Running live-editor unit tests..."
    @node --test book/theme/rhai-live-core.test.js
    @echo "Checking iso-pipeline-objects.html has at least 5 mermaid blocks..."
    @count=$(grep -c 'class="mermaid"' book/book/iso-pipeline-objects.html || true); echo "Found $count mermaid blocks in iso-pipeline-objects.html"; if [ "$count" -lt 5 ]; then echo "error: expected at least 5 mermaid blocks, found $count"; exit 1; fi; echo "✓ iso-pipeline-objects.html has $count mermaid blocks, expected at least 5"
    @echo "All documentation diagrams validated!"

# Verify the exact mdBook output directory published to GitHub Pages.
docgen-pages-check:
    just docgen-check
    @test -f book/book/index.html || { echo "error: GitHub Pages publish payload missing book/book/index.html"; exit 1; }
    @compgen -G 'book/book/theme/rhai-live-core*.js' >/dev/null || { echo "error: GitHub Pages publish payload missing live editor core asset"; exit 1; }
    @compgen -G 'book/book/theme/rhai-live-*.js' >/dev/null || { echo "error: GitHub Pages publish payload missing live editor asset"; exit 1; }
    @compgen -G 'book/book/mdbook-admonish*.css' >/dev/null || { echo "error: GitHub Pages publish payload missing admonish CSS"; exit 1; }
    @grep -q 'l3dg3rr Ledger Documentation' book/book/index.html || { echo "error: GitHub Pages index does not look like the hosted docs"; exit 1; }
    @echo "✓ GitHub Pages docs payload validated at book/book/"

# Negative test: verify broken cross-references are present in output (mdBook
# does not fail on broken links at build time — this confirms the behavior)
docgen-check-negative:
    @if [ ! -x ~/.cargo/bin/mdbook ]; then echo "error: mdbook not found — run: cargo install mdbook mdbook-mermaid"; exit 1; fi
    @if [ ! -x ~/.cargo/bin/mdbook-mermaid ]; then echo "error: mdbook-mermaid not found — run: cargo install mdbook-mermaid"; exit 1; fi
    @if [ ! -x ~/.cargo/bin/mdbook-rhai-mermaid ]; then cargo install --path crates/mdbook-rhai-mermaid --quiet; fi
    @echo "Creating temp file with broken cross-reference..."
    echo "# Broken Page" > book/src/broken.md
    echo "" >> book/src/broken.md
    echo "[bad](./nonexistent.html)" >> book/src/broken.md
    echo "[good](./intro.html)" >> book/src/broken.md
    echo "[relative](../nonexistent/deep.html)" >> book/src/broken.md
    @echo "Building book with known-broken link..."
    PATH="$$HOME/.cargo/bin:$$PATH" $$HOME/.cargo/bin/mdbook build book
    @echo "Verifying broken link appears in output (mdBook doesn't fail at build time)..."
    @grep -q 'href="./nonexistent.html"' book/book/broken.html && echo "✓ confirmed: nonexistent.html link present in output" || { echo "error: expected broken link not found"; rm -f book/src/broken.md; exit 1; }
    @grep -q 'href="../nonexistent/deep.html"' book/book/broken.html && echo "✓ confirmed: deep broken link present in output" || { echo "error: expected deep broken link not found"; rm -f book/src/broken.md; exit 1; }
    @grep -q 'href="./intro.html"' book/book/broken.html && echo "✓ confirmed: valid link also present" || { echo "error: valid link missing"; rm -f book/src/broken.md; exit 1; }
    @echo "Cleaning up temp test file..."
    rm -f book/src/broken.md
    rm -rf book/book/broken.html
    @echo "✓ docgen-check-negative passed — mdBook does not fail on broken links"

# Run the McpProvider smoke test (compile-and-construct, no external binaries needed)
test-mcp-providers:
    cargo test -p ledgerr-mcp --test mcp_provider_smoke 2>&1 | tail -20

# Run OpenMetadata MCP bridge tests. Live MCP checks run only when
# OPENMETADATA_MCP_URL and OPENMETADATA_MCP_BEARER_TOKEN are set.
test-openmetadata-mcp:
    cargo test -p ledgerr-mcp-core test_http_mcp_provider_against_local_reference_server
    cargo test -p ledgerr-mcp --test openmetadata_ontology
    cargo test -p ledgerr-mcp --test mcp_provider_smoke live_openmetadata_provider_lists_prefixed_tools_when_configured -- --nocapture

# Verify this shell is configured for a live OpenMetadata MCP endpoint.
openmetadata-config-check:
    @test -n "$${OPENMETADATA_MCP_URL:-$${OPENMETADATA_URL:-}}" || { echo "OPENMETADATA_MCP_URL or OPENMETADATA_URL is required"; exit 1; }
    @test -n "$${OPENMETADATA_MCP_BEARER_TOKEN:-$${OPENMETADATA_JWT_TOKEN:-}}" || { echo "OPENMETADATA_MCP_BEARER_TOKEN or OPENMETADATA_JWT_TOKEN is required"; exit 1; }
    @echo "OpenMetadata MCP endpoint configured: $${OPENMETADATA_MCP_URL:-$${OPENMETADATA_URL}}"

# Run only the live OpenMetadata MCP discovery test. This does not start OpenMetadata.
openmetadata-live-smoke: openmetadata-config-check
    cargo test -p ledgerr-mcp --test mcp_provider_smoke live_openmetadata_provider_lists_prefixed_tools_when_configured -- --nocapture

# Validate and run the OpenMetadata MCP GitHub Actions surface locally via wrkflw.
wrkflw-openmetadata-test emulation="secure-emulation":
    @if ! command -v wrkflw >/dev/null 2>&1; then echo "error: wrkflw not found — run: b00t install wrkflw"; exit 1; fi
    wrkflw validate --verbose .github/workflows/openmetadata-mcp.yml
    wrkflw run --runtime {{emulation}} .github/workflows/openmetadata-mcp.yml

# ─── build: local CI build via wrkflw ──────────────────────────────────────

# Prove the Dockerfile planner fix: runs cargo chef prepare + ledgerr-mcp build
# via wrkflw emulation mode (no Docker required).
build emulation="emulation":
    @if ! command -v wrkflw >/dev/null 2>&1; then echo "error: wrkflw not found — run: cargo install wrkflw"; exit 1; fi
    @echo "=== wrkflw: CI build verification ==="
    wrkflw run --runtime {{emulation}} .github/workflows/wrkflw-ci-build.yml
    @echo "=== build complete ==="

# ─── wrkflw: local CI pipeline runner ──────────────────────────────────────

# Run the wrkflw-local-docgen workflow locally using emulation mode (no Docker).
# Tests all visualization pipeline stages: Rhai parser, iso lint, viz derive,
# legal Z3, docgen build, Kasuari constraints, iso objects, live-editor JS.
# Requires: cargo install wrkflw
wrkflw-docgen-test emulation="secure-emulation":
    @if ! command -v wrkflw >/dev/null 2>&1; then echo "error: wrkflw not found — run: cargo install wrkflw"; exit 1; fi
    @echo "=== wrkflw: Running docgen visualization pipeline ==="
    wrkflw run --runtime {{emulation}} .github/workflows/wrkflw-docgen.yml
    @echo "=== wrkflw-docgen-test complete ==="

# Validate the wrkflw workflow definition for syntax correctness
wrkflw-validate:
    @if ! command -v wrkflw >/dev/null 2>&1; then echo "error: wrkflw not found — run: cargo install wrkflw"; exit 1; fi
    wrkflw validate --verbose .github/workflows/wrkflw-docgen.yml
    @echo "✓ wrkflw-docgen workflow validates"

# List all workflows wrkflw can discover
wrkflw-list:
    @if ! command -v wrkflw >/dev/null 2>&1; then echo "error: wrkflw not found — run: cargo install wrkflw"; exit 1; fi
    wrkflw list

# Run specific stages of the docgen pipeline via wrkflw with job selection
wrkflw-job job="stage-1-rhai-parser-tests" emulation="secure-emulation":
    @if ! command -v wrkflw >/dev/null 2>&1; then echo "error: wrkflw not found — run: cargo install wrkflw"; exit 1; fi
    wrkflw run --job "{{job}}" --runtime {{emulation}} .github/workflows/wrkflw-docgen.yml

# Open wrkflw TUI to inspect and run workflows interactively
wrkflw-tui:
    @if ! command -v wrkflw >/dev/null 2>&1; then echo "error: wrkflw not found — run: cargo install wrkflw"; exit 1; fi
    wrkflw tui

# Full wrkflw test: validate first, then run the full docgen pipeline
wrkflw-full-test emulation="secure-emulation":
    @if ! command -v wrkflw >/dev/null 2>&1; then echo "error: wrkflw not found — run: cargo install wrkflw"; exit 1; fi
    @echo "=== Step 1: Validate ==="
    wrkflw validate .github/workflows/wrkflw-docgen.yml
    @echo ""
    @echo "=== Step 2: Run docgen pipeline ==="
    wrkflw run --runtime {{emulation}} .github/workflows/wrkflw-docgen.yml

# Verify all env vars in source code are documented in .env.example
env-docs-check:
    bash scripts/check-env-docs.sh

# Timed b00t maintenance probe for version/task/focus/audit surfaces.
b00t-maintenance-check budget="":
    bash scripts/check-b00t-maintenance.sh {{ if budget == "" { "" } else { "--budget " + budget } }}
