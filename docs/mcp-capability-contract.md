# MCP Capability Contract (Generated)

This file is generated from `crates/ledgerr-mcp/src/contract.rs`.

Rust code is the only source of truth for the published MCP surface. If this file drifts from the contract module, tests should fail.

The default catalog is intentionally small: 9 top-level `ledgerr_*` tools. Each tool uses a required `action` field so the major capability families stay visible while related operations are grouped under one top-level command.

## Published MCP Tools

| Tool | Purpose | Common actions |
|---|---|---|
| `ledgerr_documents` | document intake (PDF, image, CSV), tagging, filesystem metadata sync | `list_accounts`, `pipeline_status`, `validate_filename`, `ingest_pdf`, `ingest_image`, `ingest_rows`, `get_raw_context`, `document_inventory`, `apply_tags`, `remove_tags`, `list_tagged`, `sync_fs_metadata`, `normalize_filename` |
| `ledgerr_review` | classification and human-review workflows | `run_rule`, `classify_ingested`, `query_flags`, `classify_transaction`, `reconcile_excel_classification`, `query_transactions`, `batch_classify`, `bulk_resolve_flags`, `apply_mapping_bulk`, `fetch_work_queue` |
| `ledgerr_reconciliation` | staged totals/postings guardrails | `validate`, `reconcile`, `commit` |
| `ledgerr_workflow` | lifecycle/HSM orchestration plus relocated plugin ops | `status`, `transition`, `resume`, `plugin_info` |
| `ledgerr_audit` | append-only event and audit-log views | `event_history`, `event_replay`, `query_audit_log` |
| `ledgerr_tax` | tax summaries, evidence, ambiguity review, workbook export | `assist`, `evidence_chain`, `ambiguity_review`, `schedule_summary`, `export_workbook` |
| `ledgerr_ontology` | ontology query/export/write operations | `query_path`, `export_snapshot`, `upsert_entities`, `upsert_edges` |
| `ledgerr_xero` | Xero accounting integration: contacts, accounts, bank accounts, entity linking | `get_auth_url`, `exchange_code`, `fetch_contacts`, `search_contacts`, `fetch_accounts`, `fetch_bank_accounts`, `fetch_invoices`, `link_entity`, `sync_catalog` |
| `ledgerr_focus` | FOCUS (FinOps Cost Usage Spec) v1.3 cost/usage records, FocusDelta comparison, experiment scoring | `append_focus_record`, `query_focus_summary`, `compute_focus_delta`, `experiment_score` |
| `ledgerr_evidence` | evidence traceability: provenance gaps, transaction lineage, review badges, graph summary and node queries | `provenance_gaps`, `trace_tx`, `summary`, `list_nodes`, `node_detail` |

The concrete parser, action enums, field aliases, and JSON Schemas all live in [crates/ledgerr-mcp/src/contract.rs](../crates/ledgerr-mcp/src/contract.rs).

The transport adapter in [crates/ledgerr-mcp/src/mcp_adapter.rs](../crates/ledgerr-mcp/src/mcp_adapter.rs) consumes that contract rather than re-describing it by hand.

## Compatibility

The server still accepts older `l3dg3rr_*` and proxy-style call names as hidden compatibility aliases, but they are no longer advertised in `tools/list`. Agents should use the `ledgerr_*` tools above by default.

## Internal Service API

Canonical trait:
[TurboLedgerTools in crates/ledgerr-mcp/src/lib.rs](../crates/ledgerr-mcp/src/lib.rs#L289)

Important distinction:
- The MCP surface is the 8-tool catalog defined in Rust.
- The internal service trait remains more granular and implementation-oriented.

API layering:
1. `ledgerr-mcp-server` (stdio transport)
2. `contract` (published tool families, action enums, generated schema/doc artifacts)
3. `mcp_adapter` (dispatch + envelope shaping)
4. `TurboLedgerService` (domain logic, guardrails, state/event/HSM ops)
5. `ledger-core` (ingest, filename validation, classification primitives)

## Example Flow

### Step A: initialize and list tools

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"demo","version":"0.1.0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
```

### Step B: ingest a PDF through `ledgerr_documents`

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "tools/call",
  "params": {
    "name": "ledgerr_documents",
    "arguments": {
      "action": "ingest_pdf",
      "pdf_path": "WF--BH-CHK--2023-01--statement.pdf",
      "journal_path": "/tmp/demo.beancount",
      "workbook_path": "/tmp/demo.xlsx",
      "raw_context_bytes": [
        99,
        116,
        120
      ],
      "extracted_rows": [
        {
          "account_id": "WF-BH-CHK",
          "date": "2023-01-05",
          "amount": "-42.50",
          "description": "Coffee Beans",
          "source_ref": "wf-2023-01.rkyv"
        }
      ]
    }
  }
}
```

### Step C: run reconciliation commit gate

```json
{
  "jsonrpc": "2.0",
  "id": 4,
  "method": "tools/call",
  "params": {
    "name": "ledgerr_reconciliation",
    "arguments": {
      "action": "commit",
      "source_total": "42.50",
      "extracted_total": "42.50",
      "posting_amounts": [
        "-42.50",
        "42.50"
      ]
    }
  }
}
```

### Step D: inspect workflow status and audit replay

```json
{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"ledgerr_workflow","arguments":{"action":"status"}}}
{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"ledgerr_audit","arguments":{"action":"event_replay","document_ref":"wf-2023-01.rkyv"}}}
```

### Step E: ask for tax evidence

```json
{
  "jsonrpc": "2.0",
  "id": 7,
  "method": "tools/call",
  "params": {
    "name": "ledgerr_tax",
    "arguments": {
      "action": "evidence_chain",
      "ontology_path": "/tmp/ontology.json",
      "from_entity_id": "WF-BH-CHK",
      "document_ref": "wf-2023-01.rkyv"
    }
  }
}
```

## Current Gaps

Open design/roadmap gaps are tracked in:
- `#20` persistent state across restart
- `#21` workbook export completeness
- `#22` schema/doc drift elimination
- `#23` document inventory/queue
- `#24` unified work queue
- `#25` batch review actions
- `#26` transaction query + preflight/rule preview
