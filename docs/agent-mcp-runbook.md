# Agent MCP Runbook (Generated)

This file is generated from `crates/ledgerr-mcp/src/contract.rs`.

Agent workflows must use `initialize`, `notifications/initialized`, `tools/list`, and `tools/call` over stdio.

## Runtime Model

The default published surface is the `ledgerr_*` catalog generated from `PUBLISHED_TOOLS`:

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
- `ledgerr_schema`
- `ledgerr_manifest`
- `ledgerr_budget`
- `ledgerr_gcp_billing`

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

## `ledgerr_gcp_billing` — GCP BigQuery Billing Export (FOCUS) ingestion

Environment (all optional; sane defaults):

| Var | Default | Purpose |
|---|---|---|
| `GCP_BILLING_PROJECT_ID` / `GCP_BILLING_DATASET` / `GCP_BILLING_TABLE` | — (required for `ingest_since`) | BigQuery billing export table location |
| `GCP_BILLING_SIDECAR_PATH` | `~/.local/share/b00t/focus/gcp_billing_focus_rows.jsonl` | JSONL sink for mapped `CostAndUsageRow`s (content-hash deduped; atomic tmp+rename writes) |
| `GCP_BILLING_MAX_ROWS` | `50000` | Explicit `--max_rows` for `bq query`. Never rely on bq's own default (100, silently truncated). Hitting the cap sets `truncated: true` in the response and fails the scheduled op — narrow `since` or raise the cap. |
| `GCP_BILLING_QUERY_TIMEOUT_SECS` | `120` | Wall-clock bound on the `bq` subprocess (mirrors `PdfIngestOp`). A hang fails the call instead of stalling the MCP server. |

First-run validation against a real export (the row-shape mapping in
`ledgerr-gcp-billing` was written against Google's published FOCUS column
names, not a live export — see its crate docs):

```bash
bq query --use_legacy_sql=false --max_rows=5 --format=json \
  'SELECT * FROM `PROJECT.DATASET.TABLE` ORDER BY ChargePeriodStart DESC' \
  > /tmp/real_rows.json
# then for each row object, call:
#   {"name":"ledgerr_gcp_billing","arguments":{"action":"dry_run_map_row","raw_row_json":{...}}}
# a clean map confirms the field-name guesses; a MissingField/InvalidValue
# error names exactly which column guess needs correction.
```
