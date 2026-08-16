---
name: ledgerr-qa
description: Use this skill for adversarial quality inspection of the l3dg3rr repo. Covers MCP protocol compliance, CI/CD correctness, Containerfile, dependency versions, README completeness, and test coverage. Run this before any release or when reviewing a contractor's work.
---

# ledgerr-qa

## Inspection Checklist

Run these checks in order. Each has caused a real bug in this repo.

### 1. Compile + Tests

```bash
cargo test --workspace --all-targets --all-features
# Expect: zero errors, zero FAILED
cargo clippy --workspace --all-targets --all-features --message-format=json | grep '"level":"error"'
# Expect: no output
```

### 2. MCP Protocol Compliance

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"qa","version":"1"}}}
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  | ./target/debug/ledgerr-mcp-server
```

Check the tools/list response:
- No `tools/list` or `tools/call` in the tool names (those are JSON-RPC methods, not tools)
- Every tool has `"inputSchema"` field
- No duplicate tool names (`l3dg3rr_query_audit_log` appeared twice — was in both CLASSIFICATION and AUDIT groups)
- Tool count should be 27

### 3. CI Workflow Trigger Names

`release.yml` must reference the EXACT `name:` field of the CI workflow:

```bash
grep "^name:" .github/workflows/ci.yml
grep "workflows:" .github/workflows/release.yml
# These must match exactly
```

Current correct value: `"CI with MCP Registry Publish"`

### 4. server.json Artifact URL

```bash
grep "identifier" server.json
grep "target:" .github/workflows/mcpb-publish.yml
# Must use same target suffix (linux-musl, not linux-gnu)
```

### 5. publish.yml Crate Names

```bash
grep "cargo publish -p" .github/workflows/publish.yml
# Must match actual workspace members: ledger-core, ledgerr-mcp
# NOT turbo-mcp (old name, doesn't exist)
```

### 6. Containerfile

```bash
grep "CMD\|cargo test\|cargo-chef\|COPY --from" Containerfile
```

- `CMD` must run the binary (`/usr/local/bin/ledgerr-mcp-server`), not `cargo test`
- Must use `cargo-chef` for dependency layer caching
- Runtime stage must copy only the binary, not the full builder filesystem

### 7. Dependency Versions (CLAUDE.md spec)

```bash
grep "calamine\|thiserror" crates/*/Cargo.toml
# calamine must be 0.34.x (not 0.26)
# thiserror must be 2.x (not 1.x)
grep -A2 "name = \"calamine\"\|name = \"thiserror\"" Cargo.lock | grep version
```

### 8. Justfile Portability

```bash
grep "/home/" justfile
# Must be empty — no hardcoded absolute user paths
# cog, just, cargo must be bare commands, not full paths
```

### 9. README Prerequisites

```bash
grep -i "rust\|just\|cog\|cross\|prerequisites" README.md
# Must have a prerequisites section listing: Rust 1.88+, just, cocogitto, cross, mcp-publisher
```

### 10. Phase 6 Exposure Gaps

```bash
RUSTFLAGS="--cfg phase6_gap_tests" cargo test -p ledgerr-mcp --test phase6_mcp_exposure_gaps
# All 20 tests must pass — these are the MCP tool acceptance criteria
```

## Scoring

Count FAILs. 0 = shippable. Any FAIL on items 1–6 = block release.
Items 7–10 = should fix before merge.
