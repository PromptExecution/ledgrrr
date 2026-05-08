//! Ledgrrr-specific AGT policy — maps the 10 published MCP tools onto the
//! AGT policy engine's YAML rule format.
//!
//! Action dot-notation: `{tool_name}.{action}` — e.g. `ledgerr_documents.ingest_pdf`.
//! Patterns use the same underscore-joined tool names so namespace scoping matches.

/// All 10 top-level capability families published by ledgrrr.
pub const PUBLISHED_TOOL_NAMES: &[&str] = &[
    "ledgerr_documents",
    "ledgerr_review",
    "ledgerr_reconciliation",
    "ledgerr_workflow",
    "ledgerr_audit",
    "ledgerr_tax",
    "ledgerr_ontology",
    "ledgerr_xero",
    "ledgerr_evidence",
    "ledgerr_focus",
];

/// Capabilities advertised in the DID document for this agent.
pub const LEDGERR_CAPABILITIES: &[&str] = &[
    "ledgerr_documents.*",
    "ledgerr_review.*",
    "ledgerr_reconciliation.read",
    "ledgerr_workflow.read",
    "ledgerr_audit.read",
    "ledgerr_tax.read",
    "ledgerr_ontology.*",
    "ledgerr_evidence.*",
    "ledgerr_focus.*",
];

/// YAML policy applied to every `AgentMeshClient` created by this crate.
///
/// Action format: `{tool_name}.{action}` where tool_name uses underscores,
/// matching the `format!("{}.{}", tool, action)` in `check_tool_call`.
///
/// Rules evaluated in priority order (highest first):
/// 1. Block shell/system access (priority 100)
/// 2. Require approval for commit/reverse operations (priority 90)
/// 3. Rate-limit ingest operations to 120/min (priority 80)
/// 4. Rate-limit Xero to 30/min (priority 75)
/// 5. Allow all read + safe write operations (priority 50)
pub const LEDGERR_POLICY_YAML: &str = r#"
version: "1.0"
agent: ledgerr-mcp-server
policies:
  - name: block-shell
    type: capability
    priority: 100
    denied_actions:
      - "shell:*"
      - "system:*"
      - "os:*"

  - name: commit-approval-gate
    type: approval
    priority: 90
    actions:
      - "ledgerr_reconciliation.commit*"
      - "ledgerr_reconciliation.reverse*"
      - "ledgerr_workflow.commit*"
      - "ledgerr_workflow.approve*"
    min_approvals: 1

  - name: ingest-rate-limit
    type: rate_limit
    priority: 80
    actions:
      - "ledgerr_documents.ingest_pdf"
      - "ledgerr_documents.ingest_rows"
    max_calls: 120
    window: "60s"

  - name: xero-rate-limit
    type: rate_limit
    priority: 75
    actions:
      - "ledgerr_xero.*"
    max_calls: 30
    window: "60s"

  - name: allow-all-ledgerr-ops
    type: capability
    priority: 50
    allowed_actions:
      - "ledgerr_documents.*"
      - "ledgerr_review.*"
      - "ledgerr_audit.*"
      - "ledgerr_tax.*"
      - "ledgerr_ontology.*"
      - "ledgerr_evidence.*"
      - "ledgerr_focus.*"
      - "ledgerr_xero.*"
      - "ledgerr_workflow.*"
      - "ledgerr_reconciliation.*"
"#;
