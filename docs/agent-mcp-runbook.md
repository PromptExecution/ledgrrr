# Agent MCP Runbook (Generated)

This file is generated from `crates/ledgerr-mcp/src/contract.rs`.

Agent workflows must use `initialize`, `notifications/initialized`, `tools/list`, and `tools/call` over stdio.

## Runtime Model

The default published surface is the 8-tool catalog:

- `ledgerr_documents`
- `ledgerr_review`
- `ledgerr_reconciliation`
- `ledgerr_workflow`
- `ledgerr_audit`
- `ledgerr_tax`
- `ledgerr_ontology`
- `ledgerr_xero`
- `ledgerr_focus`
- `ledgerr_evidence`

Each tool requires an `action` argument.

## Bootstrap

From repo root:

```bash
cargo build -p ledgerr-mcp --bin ledgerr-mcp-server
```

## Lifecycle

Required order:

1. `initialize`
2. `notifications/initialized`
3. `tools/list`
4. `tools/call`

## Basic Happy Path

```json
{"name":"ledgerr_documents","arguments":{"action":"pipeline_status"}}
{"name":"ledgerr_documents","arguments":{"action":"list_accounts"}}
{"name":"ledgerr_documents","arguments":{"action":"ingest_pdf","pdf_path":"WF--BH-CHK--2023-01--statement.pdf","journal_path":"/tmp/demo.beancount","workbook_path":"/tmp/demo.xlsx","raw_context_bytes":[99,116,120],"extracted_rows":[{"account_id":"WF-BH-CHK","date":"2023-01-15","amount":"-42.11","description":"Coffee Shop","source_ref":"wf-2023-01.rkyv"}]}}
{"name":"ledgerr_documents","arguments":{"action":"get_raw_context","rkyv_ref":"wf-2023-01.rkyv"}}
```

## Troubleshooting / Spinning Wheels

```json
{"name":"ledgerr_workflow","arguments":{"action":"resume","state_marker":"invalid-checkpoint"}}
{"name":"ledgerr_reconciliation","arguments":{"action":"commit","source_total":"100.00","extracted_total":"95.00","posting_amounts":["-95.00","95.00"]}}
{"name":"ledgerr_audit","arguments":{"action":"event_history","time_start":"2026-12-31","time_end":"2026-01-01"}}
```

Expected blocked outcomes:

- invalid workflow resume returns `HsmResumeBlocked`
- imbalanced reconciliation commit returns `ReconciliationBlocked`
- invalid audit time range returns `EventHistoryBlocked`

## Suggested Test Commands

```bash
cargo test -p ledgerr-mcp --test mcp_stdio_e2e -- --nocapture
cargo test -p ledgerr-mcp --test plugin_info_mcp_e2e -- --nocapture
bash scripts/mcp_cli_demo.sh
bash scripts/mcp_e2e.sh
```

## Notes

- Hidden compatibility aliases still exist for older `l3dg3rr_*` and proxy-style calls, but agents should not depend on them.
- Use `docs/mcp-capability-contract.md` as the concise surface map.
