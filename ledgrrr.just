# Ledgrrr just recipes — Phase 1: Runtime Schema Extensibility
#
# Usage:
#   just schema-check     # Run cargo check on the ledgerr-mcp crate
#   just schema-test      # Run only schema-related unit tests
#   just schema-e2e       # Run schema integration tests
#   just schema-all       # Run all schema-related checks and tests
#   just test-all         # Run all ledgerr-mcp tests

# ── Schema Phase 1 recipes ────────────────────────────────────────────────────

# Check ledgerr-mcp compiles cleanly
schema-check:
    cargo +nightly check -p ledgerr-mcp

# Run schema unit tests (lib tests matching 'schema')
schema-test:
    cargo +nightly test -p ledgerr-mcp -- schema_ --test-threads=4

# Run schema integration tests (e2e via MCP protocol)
schema-e2e:
    cargo +nightly test -p ledgerr-mcp -- schema_e2e --test-threads=4 --nocapture

# Run all schema tests including tools registry check
schema-all: schema-check schema-test schema-e2e

# Run the full ledgerr-mcp test suite
test-all:
    cargo +nightly test -p ledgerr-mcp -- --test-threads=4

# Just a quick build check
check:
    cargo +nightly check -p ledgerr-mcp

# Run the Phase 3 training pipeline test suite (18 tests, <30s, no GPU/network)
train-test:
    uv run python3 -m pytest scripts/training/test_pipeline.py -v --tb=short

# Check that training data generation is deterministic against committed snapshot
check-determinism:
    uv run python3 scripts/training/check-determinism.py

# Validate export-gguf.py parses correctly
test-gguf:
    uv run python3 scripts/training/test_gguf_conversion.py

# Rebuild viz-manifest.json from source types
pipeline-check:
    cargo +nightly test -p ledgerr-mcp -- pipeline_ --test-threads=4 --nocapture

# Rebuild viz-manifest.json from source types
rebuild-viz:
    cargo +nightly run --package xtask -- export-viz-manifest

# Single command: run all tests + check training data + rebuild viz-manifest
develop-basic: schema-all train-test test-gguf check-determinism rebuild-viz
    @echo "=== develop-basic complete ==="

# Start the MCP server (stdio mode, for testing)
mcp-start manifest="ledgrr.toml":
    cargo +nightly run --package ledgerr-mcp --bin ledgerr-mcp-server -- --manifest-path {{manifest || "ledgrr.toml"}}

# Clean build artifacts
clean:
    cargo +nightly clean -p ledgerr-mcp

# Run Criterion benchmarks for ledger-core and ledgerr-mcp
bench:
    cargo +nightly bench -p ledger-core --bench viz_bench
    cargo +nightly bench -p ledgerr-mcp --bench schema_bench

# E2E MCP endpoint tests via pytest
test-e2e-mcp:
    uv run python3 -m pytest scripts/tests/test_mcp_endpoints.py -v --tb=short

# ── Epoch 5: Rhai DSL formatting ───────────────────────────────────────────────

format-rhai-dsl:
    uv run python3 scripts/format-rhai-dsl.py

format-rhai-dsl-check:
    uv run python3 scripts/format-rhai-dsl.py --check

# ── Epoch 5: Generate tool documentation ───────────────────────────────────────

docgen:
    uv run python3 scripts/docs/generate-tool-docs.py

# ── Epoch 5: Enhanced develop recipe ────────────────────────────────────────────

develop: schema-all train-test test-gguf check-determinism rebuild-viz bench format-rhai-dsl-check docgen
    @echo "=== develop complete ==="

# ── Unsloth Fine-Tuning Pipeline ───────────────────────────────────────────────
# 🍰 Full pipeline: train-data → fine-tune → fine-tune-export → llm-rebuild
#
# Quick start:
#   just ledgrrr train-data               # Generate JSONL from canonical types
#   just ledgrrr train-stats              # Show token count report
#   just ledgrrr fine-tune-dry-run        # 3-step sanity check (~30s, no GPU)
#   just ledgrrr fine-tune                # Run Unsloth LoRA (requires GPU)
#   just ledgrrr fine-tune-export         # Merge LoRA → GGUF Q3_K_M
#   just ledgrrr llm-rebuild              # Replace model, restart endpoint
#
# Dependencies:
#   unsloth        — pip install unsloth (GPU required for actual training)
#   tiktoken       — pip install tiktoken (token counting)
#   llama.cpp      — for convert_hf_to_gguf.py (GGUF conversion)

# Generate JSONL training data from viz-manifest + ontology types
train-data:
    uv run python3 scripts/training/generate-training-data.py

# Show token count report (tiktoken cl100k_base — not character counting)
train-stats:
    @echo "=== Token Count Report ==="
    uv run python3 scripts/training/show-stats.py

# 3-step dry-run sanity check (no GPU, no network, <30s)
fine-tune-dry-run:
    @echo "=== Unsloth fine-tune dry-run ==="
    @echo ""
    @echo "[1/3] Checking training scripts parse correctly..."
    uv run python3 scripts/training/check-scripts-parse.py
    @echo ""
    @echo "[2/3] Checking training data exists..."
    @if [ -f scripts/training/train/train.jsonl ]; then \
        echo "  OK train.jsonl: $$(wc -l < scripts/training/train/train.jsonl) pairs"; \
    else \
        echo "  WARN train.jsonl missing — run 'just ledgrrr train-data' first"; \
    fi
    @if [ -f scripts/training/train/val.jsonl ]; then \
        echo "  OK val.jsonl: $$(wc -l < scripts/training/train/val.jsonl) pairs"; \
    else \
        echo "  WARN val.jsonl missing — run 'just ledgrrr train-data' first"; \
    fi
    @echo ""
    @echo "[3/3] Running pipeline test suite..."
    uv run python3 -m pytest scripts/training/test_pipeline.py -v --tb=short -q 2>&1 | tail -8
    @echo ""
    @echo "=== Dry-run complete ==="

# Run Unsloth LoRA fine-tuning (requires GPU/torch/unsloth)
fine-tune model="microsoft/Phi-3-mini-4k-instruct" epochs="3" lr="2e-4" batch="2" grad-accum="4":
    @echo "=== Unsloth LoRA fine-tuning ==="
    @echo "Model:     {{model}}"
    @echo "Epochs:    {{epochs}}"
    @echo "LR:        {{lr}}"
    @echo "Batch:     {{batch}}"
    @echo "Grad accum: {{grad-accum}}"
    @echo ""
    uv run python3 scripts/training/fine-tune.py \
        --model {{model}} \
        --epochs {{epochs}} \
        --lr {{lr}} \
        --batch-size {{batch}} \
        --grad-accum {{grad-accum}}

# Export merged GGUF from trained adapter
fine-tune-export:
    uv run python3 scripts/training/export-gguf.py

# ── b00t Self-Training Pipeline ─────────────────────────────────────────────────
# Wire b00t command/agent usage data into the unsloth pipeline
#
# Usage:
#   just ledgrrr b00t-train-data           # Collect b00t FSL examples
#   just ledgrrr b00t-train-unsloth        # Fine-tune on b00t command data

# Collect b00t command→output pairs from session history + task queue
b00t-train-data output=".b00t/fsl/b00t-commands.jsonl":
    uv run python3 scripts/training/collect-b00t-training-data.py \
        --output {{output}}

# Fine-tune qwen36 GGUF using b00t-specific training data (requires GPU)
b00t-train-unsloth:
    @echo "=== b00t self-training: unsloth Qwen3.6-27B ==="
    uv run python3 scripts/training/fine-tune.py \
        --train .b00t/fsl/b00t-commands.jsonl \
        --model unsloth/Qwen3.6-27B-GGUF \
        --epochs 3 \
        --lr 2e-4 \
        --batch-size 1 \
        --grad-accum 8 \
        --output .b00t/models/adapters/b00t-qwen36-v1/
