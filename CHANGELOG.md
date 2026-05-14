# Changelog
All notable changes to this project will be documented in this file. See [conventional commits](https://www.conventionalcommits.org/) for commit guidelines.

- - -
## v1.9.0 - 2026-05-13
#### Bug Fixes
- **(ci)** add GTK deps to Dockerfile, remove --all-features - (02fc48b) - Claude Sonnet (coordinator)
- **(ci)** add feat/** to push branch trigger - (9bcf48a) - Claude Sonnet (coordinator)
- **(ci)** remove --all-features, explict feature list, drop real_datums from CI - (3517129) - Claude Sonnet (coordinator)
- **(ci)** replace --all-features with explicit feature list, skip real_datums in CI - (4917f80) - Claude Sonnet (coordinator)
- **(ci)** checkout _b00t_ as sibling dir for include_str! path resolution - (f960b31) - Claude Sonnet (coordinator)
- **(ci)** checkout _b00t_ repo for real_datums feature tests - (c92f32d) - Claude Sonnet (coordinator)
- **(clippy)** zero warnings workspace-wide, remove handle_external_tool vestigial param - (5e634e4) - Claude Sonnet (coordinator)
- **(clippy)** resolve all PR #71 review issues — unused imports, operator precedence, and_then→map - (dca1773) - Claude Sonnet (coordinator)
- **(datum)** apply clippy autofix for vec_init_then_push and unused mut - (5b12fa6) - Claude Sonnet (coordinator)
- **(docgen)** resolve all 8 documentation generation pipeline gaps - (106b60c) - Claude Sonnet (coordinator)
- **(evidence)** address MECE review findings - (3e1cd28) - copilot-swe-agent[bot]
- **(gitignore)** .b00t/scratch/ and .b00t/datums/*.txt ignored, loop writes to scratch - (d1134d7) - Claude Sonnet (coordinator)
- **(ledger-core)** remove conflicting From impl in ingest.rs (#93) - (73d5250) - PromptExecution.com
- **(mcp)** flatten schemars oneOf for Claude API input_schema compatibility (#83) - (898613e) - PromptExecution.com
- **(mcp/contract)** bouncer follow-ups — atomic JSONL writes + conditional additionalProperties removal (#87) - (75fd8a4) - PromptExecution.com
- **(prd9)** address code review feedback on cfg gates and dead code - (162affc) - copilot-swe-agent[bot]
- **(prd9)** address compilation errors, shutdown gap, and visual narrative readiness gaps - (9770321) - copilot-swe-agent[bot]
- **(review)** hardcoded SCCACHE_DIR path, indentation and SVG tspan in to_animated_svg - (be0523d) - copilot-swe-agent[bot]
- **(rotel-visual)** resolve compilation errors — unclosed delimiters, shadowed identifiers - (96ecb94) - Claude Sonnet (coordinator)
- **(rotel-visual)** address all review feedback from PR #65 - (e4baa7b) - copilot-swe-agent[bot]
- **(sort)** add deterministic tx-id tie-break to apply_transaction_sort; add PRD handover doc - (af973b6) - Claude Sonnet (coordinator)
- **(tauri)** functional panel UI with working navigation - (1655a66) - Claude Sonnet (coordinator)
- **(tauri)** autorun with telemetry dump, 5s countdown then exit - (24067d4) - Claude Sonnet (coordinator)
- **(tauri/evidence)** MECE review — 6 correctness, security and UX fixes - (719b165) - copilot-swe-agent[bot]
- **(tauri/mece)** restore CSS, wire EvidenceState, fix dashboard command + XSS + auto-refresh - (efad326) - copilot-swe-agent[bot]
- **(tauri/ui)** guard DASH_PANEL_INDEX != -1, use template literal for provider display - (3874f9f) - copilot-swe-agent[bot]
- **(tests)** eliminate flaky sort test by using unique workbook paths per test - (46cbaa5) - Claude Sonnet (coordinator)
- close 9 of 10 identified pipeline gaps (ops stubs, semantic matching) (#95) - (994b34c) - PromptExecution.com
- address PR review feedback on ledgerr-focus and ledgerr-mcp - (b9a358c) - copilot-swe-agent[bot]
- apply review feedback - doc comment, workbook headers, verify test - (5a44339) - copilot-swe-agent[bot]
- apply reviewer feedback for lfmf-counter and ontology-extractor - (275a567) - copilot-swe-agent[bot]
- RhaiDsl type errors in iso/iso_objects, merge conflict markers in iso.rs - (6554f0d) - Claude Sonnet (coordinator)
#### Documentation
- **(agents)** context exhaustion post-mortem + delegation rules - (3e509cc) - Claude Sonnet (coordinator)
- **(release)** odd/even minor version policy + Justfile enforcement (#49) - (49c9ff7) - PromptExecution.com
- session learning — force-push guard, generated panel pattern, evidence graph - (1800f14) - Claude Sonnet (coordinator)
#### Features
- **(cdp)** CDP automation harness, clean-build, test scripts - (9d095c6) - Claude Sonnet (coordinator)
- **(cdp)** enable WebView2 remote debugging via env var - (9c74c1a) - Claude Sonnet (coordinator)
- **(dashboard)** generated panels, b00t handshake, UI hardening (#81) - (1d8588f) - PromptExecution.com
- **(evidence)** wire arc-kit-au evidence graph into MCP query tools - (b5adf44) - Claude Sonnet (coordinator)
- **(holon-viz)** scaffold holonic viz engine + model server stub - (e34d982) - Claude Sonnet (coordinator)
- **(ledgerr-mcp)** PRD-10 financial pipeline + MCP gaps #24 #25 #26 (#89) - (d5eb2e7) - PromptExecution.com
- **(mdbook-rhai-mermaid)** expose parser and emitter as library - (74bf80a) - brianh
- **(prd-6-future)** ledger-attest proc-macro crate — #[attested] lint skeleton (#59) (#94) - (5da2f0a) - PromptExecution.com
- **(prd-7)** materialize AUDIT.log sheet — AuditRow, 9 columns, MetaFlag Display (#57) (#92) - (0539143) - PromptExecution.com
- **(prd-7)** populate TransactionFacts from PipelineState doc_fields (#55) (#90) - (d624be4) - PromptExecution.com
- **(prd-8)** Kani harness crate + CI — InvoiceConstraintSolver, VendorConstraintSet, CommitGate (#56) (#91) - (53debf5) - PromptExecution.com
- **(prd9)** add docs UI scaffolding with isometric canvas and vite config - (27cf983) - Claude Sonnet (coordinator)
- **(prd9)** add VizManifest/VizSpecOwned types and xtask export command - (41f251f) - Claude Sonnet (coordinator)
- **(rotel)** OTel journal surface — embedded collector, log-shape classifier, visual dashboard - (bc1a8e1) - Brian H
- **(rotel-visual)** end-to-end OTLP ingestion → classification → visualization pipeline - (be90a67) - Brian H
- **(tauri)** baked build counter, release build + MSI install - (ecf928d) - Claude Sonnet (coordinator)
- **(tauri)** test loop with monotonic build counter, DOM versioning - (ac86b41) - Claude Sonnet (coordinator)
- **(tauri)** version titlebar with build counter from harness - (9b773e0) - Claude Sonnet (coordinator)
- **(tauri)** local vision analysis with Florence-2-base - (853a193) - Claude Sonnet (coordinator)
- **(tauri)** countdown footer, screenshot capture, full SLO trace - (1c7b8a2) - Claude Sonnet (coordinator)
- **(tauri)** test harness with UUID signal path, 3x kill redundancy, SLO trace - (30b67fd) - Claude Sonnet (coordinator)
- **(tauri)** merge host-tauri into ledgerr-host, replace Slint as desktop host - (e582b91) - Claude Sonnet (coordinator)
- **(tauri)** rename binary to ledgrrr, install via MSI with admin elevation - (d28c0d2) - Claude Sonnet (coordinator)
- **(tauri)** build script with pre-flight check, hash signing, datum TOML - (a8c23ae) - Claude Sonnet (coordinator)
- **(tauri)** generated panels + EvidenceState dashboard - (13dd370) - Claude Sonnet (coordinator)
- **(tauri)** surface EvidenceState/TodayQueue in Dashboard panel - (240d6b0) - Claude Sonnet (coordinator)
- **(wrkflw)** add local docgen visualization pipeline test workflow - (e0652a1) - Claude Sonnet (coordinator)
- ledgerr-focus FOCUS v1.3 crate, ledgerr_focus MCP tool, Dockerfile, workspace registration - (1573711) - brianh
- checkpoint workspace core updates - (f82a938) - Brian H
- add lfmf and ontology tooling - (54ca266) - Brian H
#### Miscellaneous Chores
- **(lockfile)** sync rotel-visual version to 1.8.1 - (ac40309) - brianh
- **(rustfmt)** apply rustfmt across workspace - (a56ec94) - Claude Sonnet (coordinator)
#### Performance Improvements
- use sort_unstable_by in extract_rust_idioms - (63e3200) - copilot-swe-agent[bot]
#### Refactoring
- **(otel)** idiomatic abstractions, self-telemetry, SARIF SLO wiring - (76ea89d) - Brian H
#### Tests
- avoid persisting lfmf counters in doctests - (8f00edc) - Brian H

- - -

## v1.8.1 - 2026-05-02
#### Features
- (**b00t-iface**) SARIF module enhancements and ralph stub - (27cabfa) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*
- (**datum**) AST linter, logic gates, protocol constraint system, and tomllmd compiler - (d90660e) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*
- (**ledgerr-mcp**) generic McpProvider trait and stdio providers for b00t, just, ir0ntology - (ccbf9e6) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*
- (**prd7**) sunset legacy dispatch behind cfg feature flag - (496bdcf) - Claude Sonnet (coordinator)
- (**prd7**) McpProvider trait, actor/gate modules, ledgerr-mcp-core crate - (2ec1ae8) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*
- (**prd7**) Phase 0 — wire constraint + legal solvers into pipeline; add type attestation concept - (44ac9a5) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*
- (**prd9**) Phase 0 — isometric pipeline visualization types and lint suite - (613127d) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*
- add b00t-iface interface library with datum lifecycle, autoresearch, SARIF lint, isometric viz, and b00t↔l3dg3rr handshake - (89bfbaa) - Claude Sonnet (coordinator)
#### Bug Fixes
- (**b00t-iface**) use expect() instead of unwrap() on tempdir creation in tests - (bc89f33) - copilot-swe-agent[bot], *elasticdotventures*
- (**b00t-iface**) fix clippy approx_constant and gate external-dir tests with tempdir/real_datums - (f495e7e) - copilot-swe-agent[bot], *elasticdotventures*
- (**datum**) correct symbolic_gate_test! macro doc — comma-separated args, no recursion - (47e91bb) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*
- address PR review feedback - path traversal validation, mkdir, and clippy lints - (6f11460) - copilot-swe-agent[bot], *elasticdotventures*
#### Documentation
- (**agents**) operational notes for tomllmd format, McpProvider invariant, datum AST linter - (379220a) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*
#### Tests
- (**ledger-core**) add legal-z3 native integration test; wire libz3-dev and datum CI steps - (4dfd2a6) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*
#### Miscellaneous Chores
- ignore .codebase-memory local MCP index - (606f553) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*
- ignore app4dog artifacts; update Cargo.lock - (cb9c8ed) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*

- - -

## v1.8.0 - 2026-05-01
#### Miscellaneous Chores
- (**version**) v1.7.0 - (9162007) - Claude Sonnet (coordinator)

- - -

## v1.7.0 - 2026-05-01
#### Features
- add selectable Windows AI provider - (319882f) - Claude Sonnet (coordinator)
- redesign chat panel with model selector and scalable layout - (fa3f6d9) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*
#### Bug Fixes
- cloud_readiness rejects internal endpoints and placeholder keys - (4565fa0) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*
- remove spurious .unwrap() on unit-returning build_full_chain in test - (51af3ef) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*
- C1-C5 + G5.1 — foundry failure not silent, discovery timeout, CSP, unsafe_code, review_log_text Option, doc placement - (1a6705d) - Claude Sonnet (coordinator)
- H3 emit_ingest_evidence warn, H6 work_queue_summary wiring + validation field, H7 From<NodeType> bridge + re-exports - (777d71a) - Claude Sonnet (coordinator)
- call cloud_readiness without Some wrapper - (3dc7555) - Claude Sonnet (coordinator)
- builder test corruption from sed; final H1-H8 clean compile - (9038d01) - Claude Sonnet (coordinator)
- H1-H8 review gaps — provider_status requires settings, idempotent EvidenceBuilder, ValidationIssue emission, work_queue_summary, bridge, resolve_chat - (12721e9) - Claude Sonnet (coordinator)
- reviewer-verified P0/P1 gaps incl G5.1 (tests in mod) and G4.3 (ValidationIssue node) - (5abf7e5) - Claude Sonnet (coordinator)
- address P0/P1 review gaps — tests structure, silent-zero amounts, cloud readiness, provider fallback, evidence emission for export/validation - (7946fea) - Claude Sonnet (coordinator)
- resolve Core Functional Shape nodes to canonical isometric visual types - (850bae0) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*
- make window dynamically resizable and prevent screen overflow - (d89d942) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*
#### Documentation
- add operator simplification PRD - (b1878b1) - Claude Sonnet (coordinator)
- align Claude plugin MCP runtime - (b20a2e5) - brianh
- consolidate Rhai workflow structure - (5a89eb4) - brianh
#### Continuous Integration
- install linux desktop dependencies - (5230e67) - Claude Sonnet (coordinator)
#### Miscellaneous Chores
- add scheduled_tasks.lock to gitignore; add PRD-6 draft - (b5b68cd) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*

- - -

## v1.5.0 - 2026-04-19
#### Features
- ship the Windows desktop host further toward an operator control plane with richer tray state, persisted tray settings, and a basic Slint chat window backed by `rig-core`
- expose more host operational state and notification settings directly through the tray menu and persisted settings surface
#### Refactoring
- remove the mistaken `mistralrs` dependency from the host path and standardize the tray chat client on `rig-core` for OpenAI-compatible and local API backends
#### Tooling
- harden `Justfile` Cocogitto recipes with an `ensure-cog` guard so `just v`, `just validate`, `just changelog`, and `just release` self-check the binary before use

- - -
## v1.4.0 - 2026-04-17
#### Features
- (**plugin-info**) add l3dg3rr_plugin_info MCP tool with Windows self-update - (16d070c) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*
- reconcile document inventory queue onto 7-tool contract architecture - (59dd2cf) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*
- add document inventory queue - (0dcd8f2) - Claude Sonnet (coordinator)
- persist mcp operational state across restart - (e2fe9c7) - Claude Sonnet (coordinator)
- honor workbook export contract - (d130226) - Claude Sonnet (coordinator)
- generate mcp contract artifacts from rust - (764b3c9) - Claude Sonnet (coordinator)
#### Bug Fixes
- (**ci**) resolve clippy errors and warnings blocking CI - (1595d6e) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*
- (**docker**) copy docs and scripts into builder so contract tests can read generated artifacts - (5b6f7d7) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*
- (**docker**) copy docs and scripts into builder so contract tests can read generated artifacts - (cb1e080) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*
- (**docker**) copy docs and scripts into builder so contract tests can read generated artifacts - (0d3beeb) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*
- (**docker**) copy docs and scripts into builder so contract tests can read generated artifacts - (9165d81) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*
- (**mcp**) BUG-003 — replace invalid "type":"json" content blocks with "type":"text" - (7ca1a73) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*
- atomic persist write, valid bash/JSON in demo script, repo-relative doc links - (cc1c26b) - copilot-swe-agent[bot], *elasticdotventures*
- address PR review feedback - schema enum, mutex scope, and doc path - (290dcb1) - copilot-swe-agent[bot], *elasticdotventures*
- resolve generated contract ci drift - (b23efe5) - Claude Sonnet (coordinator)
#### Documentation
- (**claude**) document required dev tools and release workflow - (9915584) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*
#### Tests
- (**plugin-info**) add MCP e2e tests for l3dg3rr_plugin_info tool - (d4c8e59) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*
#### Refactoring
- collapse mcp surface to ledgerr tools - (5472ead) - Claude Sonnet (coordinator)
#### Miscellaneous Chores
- (**cog**) add pre_bump_hook to keep Cargo.toml version in sync - (8b74749) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*
- (**version**) bump workspace version to 1.3.7 to match release tags - (6fd8115) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*

- - -

## v1.3.8 - 2026-04-17
#### Bug Fixes
- (**mcp**) fix BUG-003 (P0): all 27 tool handlers returned `"type": "json"` content blocks, which is not a valid MCP 2025-11-25 content type and causes Zod validation failures in spec-compliant clients; converted every content block to `"type": "text"` with payload serialized as a JSON string; added `text_content()` private helper to centralise the pattern; added `handle_pipeline_status()` handler to eliminate the one remaining hand-rolled response in the server binary - (cbde62d) - Claude Sonnet (coordinator)

- - -

## v1.3.7 - 2026-04-16
#### Bug Fixes
- (**mcp**) fix `proxy_docling_ingest_pdf` schema: required fields were `source_ref` in schema vs `pdf_path`/`journal_path`/`workbook_path` in impl; schema updated to match; `extracted_rows` made truly optional (BUG-001) - (cbde62d) - Claude Sonnet (coordinator)
- (**mcp**) fix `l3dg3rr_ontology_export_snapshot` schema: advertised no params but impl required `ontology_path`; schema corrected and handler routed through `TurboLedgerService` (BUG-002) - (cbde62d) - Claude Sonnet (coordinator)
#### Refactor
- (**ledgerr-mcp**) extracted `error_envelope` private helper — 29 duplicated parse-error/service-error JSON blocks collapsed to a single call site - (cbde62d) - Claude Sonnet (coordinator)
- (**ledgerr-mcp**) renamed 23 handler functions: `*_tool_result` → `handle_*`; multi-tool dispatchers → `dispatch_reconciliation`/`dispatch_hsm` - (cbde62d) - Claude Sonnet (coordinator)
- (**ledgerr-mcp**) renamed `map_tool_error` → `error_payload`, `normalize_rows_with_provenance` → `rows_to_json_with_provenance` - (cbde62d) - Claude Sonnet (coordinator)
- (**ledgerr-mcp**) renamed catalog/descriptor trio: `tool_catalog` → `tool_names`, `tool_catalog_with_features` → `tool_names_for`, `tool_list_entries` → `tool_descriptors` - (cbde62d) - Claude Sonnet (coordinator)
- (**ledgerr-mcp**) removed `pub` from two parse functions with no external callers; removed vestigial `McpAdapter` struct - (cbde62d) - Claude Sonnet (coordinator)

- - -
## v1.3.6 - 2026-04-16
#### Bug Fixes
- (**mcpb**) use server/ subdir and ${__dirname} in command per mcpb spec - (faba56d) - Claude Sonnet (coordinator)

- - -

## v1.3.5 - 2026-04-16
#### Bug Fixes
- (**mcpb**) drop ./ from command; set author to Prompt Execution Pty Ltd. - (a421da0) - Claude Sonnet (coordinator)

- - -

## v1.3.4 - 2026-04-16
#### Bug Fixes
- (**mcpb**) derive entry_point and command from binary filename - (3ce9e03) - Claude Sonnet (coordinator)

- - -

## v1.3.3 - 2026-04-16
#### Bug Fixes
- (**justfile**) use gh release list for tag fallback in publish-mcpb (not cog) - (3531fe0) - Claude Sonnet (coordinator)

- - -

## v1.3.2 - 2026-04-16
#### Bug Fixes
- (**ci**) chain mcpb-publish via workflow_run instead of workflow_dispatch - (346a822) - Claude Sonnet (coordinator)

- - -

## v1.3.1 - 2026-04-16
#### Bug Fixes
- (**mcpb**) remove configuration field from manifest (not in Claude Code spec) - (e914af4) - Claude Sonnet (coordinator)
- (**release**) manual-only trigger; use latest/download URLs in README - (1a7ce13) - Claude Sonnet (coordinator)
#### Documentation
- simplify Windows install to single-line PowerShell (no backtick continuation) - (99a0199) - Claude Sonnet (coordinator)

- - -

## v1.3.0 - 2026-04-16
#### Features
- (**skills**) add agent skills scaffold — 5 SKILL.md runbooks - (7f74db7) - Claude Sonnet (coordinator)

- - -

## v1.2.2 - 2026-04-16
#### Bug Fixes
- (**ci**) use macos-14 for Intel cross-compile; publish even if one target fails - (1138121) - Claude Sonnet (coordinator)
- (**release**) trigger mcpb-publish explicitly after gh release create - (3e9d52a) - Claude Sonnet (coordinator)
#### Documentation
- update install examples to v1.2.1 - (2bbabd2) - Claude Sonnet (coordinator)

- - -

## v1.2.1 - 2026-04-16
#### Bug Fixes
- (**mcpb**) store binary under entry_point name, not filename - (777271d) - Claude Sonnet (coordinator)

- - -

## v1.2.0 - 2026-04-16
#### Features
- (**ci**) add Windows binary to mcpb-publish matrix + claude mcp add install docs - (0417ab5) - Claude Sonnet (coordinator)
- (**mcpb**) add xtask-mcpb library + deterministic bundle pipeline - (d2490cf) - Claude Sonnet (coordinator)
- (**release**) verify release workflow operational with cocogitto versioning - (52e3702) - Claude Sonnet (coordinator)
- add workflow_dispatch for manual release trigger - (6f1ab50) - Claude Sonnet (coordinator)
#### Bug Fixes
- (**ci**) align podman-publish trigger to current CI workflow name; stamp server.json v0.1.0 - (33f838c) - Claude Sonnet (coordinator)
- (**devops**) clear QA backlog — MCP spec, CI wiring, Dockerfile, deps, docs - (8cafec1) - Claude Sonnet (coordinator)
- (**docker**) add xtask/ to Dockerfile COPY so workspace resolves in container - (38ac8d9) - Claude Sonnet (coordinator)
- (**release**) fetch-tags, concurrency guard, explicit tag push - (e0b9302) - Claude Sonnet (coordinator)
- (**release**) set tag_prefix=v in cog.toml; guard against duplicate releases - (00a5f66) - Claude Sonnet (coordinator)
- (**release**) use --patch flag for cog bump (cog v7 syntax) - (50061d0) - Claude Sonnet (coordinator)
- (**test**) address Copilot review comments on phase6 exposure-gap suite - (9bb99b4) - Claude Sonnet (coordinator)
- simplify release workflow to use cocogitto-action v4 with bundled cog - (b3b927e) - Claude Sonnet (coordinator)
- fetch all tags in release workflow and fix cog.toml config - (942d8f7) - Claude Sonnet (coordinator)
- remove deprecated pre field from cog.toml - (bcfbdd1) - Claude Sonnet (coordinator)
- allow workflow_dispatch trigger in release condition - (54243dc) - Claude Sonnet (coordinator)
- simplify release workflow trigger condition - (8349560) - Claude Sonnet (coordinator)
- use proper mcpb manifest v0.3 schema with server config - (d923aa5) - Claude Sonnet (coordinator)
- use positional args for mcpb pack (directory output) - (1000c18) - Claude Sonnet (coordinator)
- use -o flag for mcpb pack output path - (0ab1788) - Claude Sonnet (coordinator)
- fix mcpb pack output path to stay in bundle dir - (ac8b823) - Claude Sonnet (coordinator)
#### Documentation
- update versioning section to reference justfile release recipe - (95c0652) - Claude Sonnet (coordinator)
#### Miscellaneous Chores
- (**version**) v1.2.0 - (61daa0a) - github-actions[bot]
- (**version**) v1.2.0 - (99f6664) - github-actions[bot]
- (**version**) v1.2.0 - (5548c9a) - github-actions[bot]
- (**version**) 0.0.1 - (9949944) - Claude Sonnet (coordinator)
- (**version**) 0.1.0 - (c588d71) - github-actions[bot]
- (**version**) 0.1.0 - (e890d6c) - github-actions[bot]
- (**version**) 0.1.0 - (3f6bbac) - github-actions[bot]
- (**version**) 0.1.0 - (b64a975) - github-actions[bot]
- (**version**) 0.1.0 - (dcd8f0f) - github-actions[bot]
- (**version**) 0.1.0 - (8e28f71) - github-actions[bot]
- verify release badge - (44dcc54) - Claude Sonnet (coordinator)
- test release workflow trigger - (c22303f) - Claude Sonnet (coordinator)

- - -

## v0.1.0 - 2026-04-16
#### Features
- (**mcp**) expose P0/P1/P2 tool gap handlers as wired MCP tools - (0482f67) - Claude Sonnet (coordinator)
- (**mcp**) expose account listing and raw-context tools - (79b0e5f) - brianh
- (**test**) add outcome-driven mcp flow runner behind just test - (f50d916) - brianh
#### Bug Fixes
- (**ci**) use valid rust image and pin ledger-core publish version - (e9b1140) - brianh
- (**mcp**) replace absolute paths in docs, fix service-only list, add path traversal guard to get_raw_context - (eac38fb) - copilot-swe-agent[bot]
- add contents:write permission for release creation - (00bbf78) - Claude Sonnet (coordinator)
- update package name from turbo-mcp to ledgerr-mcp in e2e script - (d800497) - Claude Sonnet (coordinator)
- align marketplace and plugin manifests with Cowork validation - (13d10d5) - brianh
#### Tests
- (**turbo-mcp**) add phase6 failing tests for MCP exposure gaps (P0/P1/P2) - (d2b7354) - Claude Sonnet (coordinator)
#### Continuous Integration
- disable MCP Registry publish (requires direct write access) - (abf6b60) - Claude Sonnet (coordinator)
- add MCPB publish gate after tests - (676c58b) - Claude Sonnet (coordinator)
- add clippy sarif upload and podman publish-on-main - (588a231) - brianh
#### Refactoring
- rename turbo-mcp to ledgerr-mcp - (4b484cd) - Claude Sonnet (coordinator)
#### Miscellaneous Chores
- (**dev**) add dotenv ignore and secret setup recipe fixes - (7a029d2) - brianh
- (**docs**) add plugin usage validation flow and mcp cli demos - (d522338) - brianh

- - -

## v1.2.0 - 2026-04-16
#### Features
- (**ci**) add Windows binary to mcpb-publish matrix + claude mcp add install docs - (0417ab5) - Claude Sonnet (coordinator)
- (**mcpb**) add xtask-mcpb library + deterministic bundle pipeline - (d2490cf) - Claude Sonnet (coordinator)
- (**release**) verify release workflow operational with cocogitto versioning - (52e3702) - Claude Sonnet (coordinator)
- add workflow_dispatch for manual release trigger - (6f1ab50) - Claude Sonnet (coordinator)
#### Bug Fixes
- (**ci**) align podman-publish trigger to current CI workflow name; stamp server.json v0.1.0 - (33f838c) - Claude Sonnet (coordinator)
- (**devops**) clear QA backlog — MCP spec, CI wiring, Dockerfile, deps, docs - (8cafec1) - Claude Sonnet (coordinator)
- (**docker**) add xtask/ to Dockerfile COPY so workspace resolves in container - (38ac8d9) - Claude Sonnet (coordinator)
- (**release**) set tag_prefix=v in cog.toml; guard against duplicate releases - (00a5f66) - Claude Sonnet (coordinator)
- (**release**) use --patch flag for cog bump (cog v7 syntax) - (50061d0) - Claude Sonnet (coordinator)
- (**test**) address Copilot review comments on phase6 exposure-gap suite - (9bb99b4) - Claude Sonnet (coordinator)
- simplify release workflow to use cocogitto-action v4 with bundled cog - (b3b927e) - Claude Sonnet (coordinator)
- fetch all tags in release workflow and fix cog.toml config - (942d8f7) - Claude Sonnet (coordinator)
- remove deprecated pre field from cog.toml - (bcfbdd1) - Claude Sonnet (coordinator)
- allow workflow_dispatch trigger in release condition - (54243dc) - Claude Sonnet (coordinator)
- simplify release workflow trigger condition - (8349560) - Claude Sonnet (coordinator)
- use proper mcpb manifest v0.3 schema with server config - (d923aa5) - Claude Sonnet (coordinator)
- use positional args for mcpb pack (directory output) - (1000c18) - Claude Sonnet (coordinator)
- use -o flag for mcpb pack output path - (0ab1788) - Claude Sonnet (coordinator)
- fix mcpb pack output path to stay in bundle dir - (ac8b823) - Claude Sonnet (coordinator)
#### Documentation
- update versioning section to reference justfile release recipe - (95c0652) - Claude Sonnet (coordinator)
#### Miscellaneous Chores
- (**version**) v1.2.0 - (99f6664) - github-actions[bot]
- (**version**) v1.2.0 - (5548c9a) - github-actions[bot]
- (**version**) 0.0.1 - (9949944) - Claude Sonnet (coordinator)
- (**version**) 0.1.0 - (c588d71) - github-actions[bot]
- (**version**) 0.1.0 - (e890d6c) - github-actions[bot]
- (**version**) 0.1.0 - (3f6bbac) - github-actions[bot]
- (**version**) 0.1.0 - (b64a975) - github-actions[bot]
- (**version**) 0.1.0 - (dcd8f0f) - github-actions[bot]
- (**version**) 0.1.0 - (8e28f71) - github-actions[bot]
- verify release badge - (44dcc54) - Claude Sonnet (coordinator)
- test release workflow trigger - (c22303f) - Claude Sonnet (coordinator)

- - -

## v0.1.0 - 2026-04-16
#### Features
- (**mcp**) expose P0/P1/P2 tool gap handlers as wired MCP tools - (0482f67) - Claude Sonnet (coordinator)
- (**mcp**) expose account listing and raw-context tools - (79b0e5f) - brianh
- (**test**) add outcome-driven mcp flow runner behind just test - (f50d916) - brianh
#### Bug Fixes
- (**ci**) use valid rust image and pin ledger-core publish version - (e9b1140) - brianh
- (**mcp**) replace absolute paths in docs, fix service-only list, add path traversal guard to get_raw_context - (eac38fb) - copilot-swe-agent[bot]
- add contents:write permission for release creation - (00bbf78) - Claude Sonnet (coordinator)
- update package name from turbo-mcp to ledgerr-mcp in e2e script - (d800497) - Claude Sonnet (coordinator)
- align marketplace and plugin manifests with Cowork validation - (13d10d5) - brianh
#### Tests
- (**turbo-mcp**) add phase6 failing tests for MCP exposure gaps (P0/P1/P2) - (d2b7354) - Claude Sonnet (coordinator)
#### Continuous Integration
- disable MCP Registry publish (requires direct write access) - (abf6b60) - Claude Sonnet (coordinator)
- add MCPB publish gate after tests - (676c58b) - Claude Sonnet (coordinator)
- add clippy sarif upload and podman publish-on-main - (588a231) - brianh
#### Refactoring
- rename turbo-mcp to ledgerr-mcp - (4b484cd) - Claude Sonnet (coordinator)
#### Miscellaneous Chores
- (**dev**) add dotenv ignore and secret setup recipe fixes - (7a029d2) - brianh
- (**docs**) add plugin usage validation flow and mcp cli demos - (d522338) - brianh

- - -

## v1.2.0 - 2026-04-16
#### Features
- (**ci**) add Windows binary to mcpb-publish matrix + claude mcp add install docs - (0417ab5) - Claude Sonnet (coordinator)
- (**mcpb**) add xtask-mcpb library + deterministic bundle pipeline - (d2490cf) - Claude Sonnet (coordinator)
- (**release**) verify release workflow operational with cocogitto versioning - (52e3702) - Claude Sonnet (coordinator)
- add workflow_dispatch for manual release trigger - (6f1ab50) - Claude Sonnet (coordinator)
#### Bug Fixes
- (**ci**) align podman-publish trigger to current CI workflow name; stamp server.json v0.1.0 - (33f838c) - Claude Sonnet (coordinator)
- (**devops**) clear QA backlog — MCP spec, CI wiring, Dockerfile, deps, docs - (8cafec1) - Claude Sonnet (coordinator)
- (**docker**) add xtask/ to Dockerfile COPY so workspace resolves in container - (38ac8d9) - Claude Sonnet (coordinator)
- (**release**) set tag_prefix=v in cog.toml; guard against duplicate releases - (00a5f66) - Claude Sonnet (coordinator)
- (**release**) use --patch flag for cog bump (cog v7 syntax) - (50061d0) - Claude Sonnet (coordinator)
- (**test**) address Copilot review comments on phase6 exposure-gap suite - (9bb99b4) - Claude Sonnet (coordinator)
- simplify release workflow to use cocogitto-action v4 with bundled cog - (b3b927e) - Claude Sonnet (coordinator)
- fetch all tags in release workflow and fix cog.toml config - (942d8f7) - Claude Sonnet (coordinator)
- remove deprecated pre field from cog.toml - (bcfbdd1) - Claude Sonnet (coordinator)
- allow workflow_dispatch trigger in release condition - (54243dc) - Claude Sonnet (coordinator)
- simplify release workflow trigger condition - (8349560) - Claude Sonnet (coordinator)
- use proper mcpb manifest v0.3 schema with server config - (d923aa5) - Claude Sonnet (coordinator)
- use positional args for mcpb pack (directory output) - (1000c18) - Claude Sonnet (coordinator)
- use -o flag for mcpb pack output path - (0ab1788) - Claude Sonnet (coordinator)
- fix mcpb pack output path to stay in bundle dir - (ac8b823) - Claude Sonnet (coordinator)
#### Documentation
- update versioning section to reference justfile release recipe - (95c0652) - Claude Sonnet (coordinator)
#### Miscellaneous Chores
- (**version**) v1.2.0 - (5548c9a) - github-actions[bot]
- (**version**) 0.0.1 - (9949944) - Claude Sonnet (coordinator)
- (**version**) 0.1.0 - (c588d71) - github-actions[bot]
- (**version**) 0.1.0 - (e890d6c) - github-actions[bot]
- (**version**) 0.1.0 - (3f6bbac) - github-actions[bot]
- (**version**) 0.1.0 - (b64a975) - github-actions[bot]
- (**version**) 0.1.0 - (dcd8f0f) - github-actions[bot]
- (**version**) 0.1.0 - (8e28f71) - github-actions[bot]
- verify release badge - (44dcc54) - Claude Sonnet (coordinator)
- test release workflow trigger - (c22303f) - Claude Sonnet (coordinator)

- - -

## v0.1.0 - 2026-04-16
#### Features
- (**mcp**) expose P0/P1/P2 tool gap handlers as wired MCP tools - (0482f67) - Claude Sonnet (coordinator)
- (**mcp**) expose account listing and raw-context tools - (79b0e5f) - brianh
- (**test**) add outcome-driven mcp flow runner behind just test - (f50d916) - brianh
#### Bug Fixes
- (**ci**) use valid rust image and pin ledger-core publish version - (e9b1140) - brianh
- (**mcp**) replace absolute paths in docs, fix service-only list, add path traversal guard to get_raw_context - (eac38fb) - copilot-swe-agent[bot]
- add contents:write permission for release creation - (00bbf78) - Claude Sonnet (coordinator)
- update package name from turbo-mcp to ledgerr-mcp in e2e script - (d800497) - Claude Sonnet (coordinator)
- align marketplace and plugin manifests with Cowork validation - (13d10d5) - brianh
#### Tests
- (**turbo-mcp**) add phase6 failing tests for MCP exposure gaps (P0/P1/P2) - (d2b7354) - Claude Sonnet (coordinator)
#### Continuous Integration
- disable MCP Registry publish (requires direct write access) - (abf6b60) - Claude Sonnet (coordinator)
- add MCPB publish gate after tests - (676c58b) - Claude Sonnet (coordinator)
- add clippy sarif upload and podman publish-on-main - (588a231) - brianh
#### Refactoring
- rename turbo-mcp to ledgerr-mcp - (4b484cd) - Claude Sonnet (coordinator)
#### Miscellaneous Chores
- (**dev**) add dotenv ignore and secret setup recipe fixes - (7a029d2) - brianh
- (**docs**) add plugin usage validation flow and mcp cli demos - (d522338) - brianh

- - -

## v1.2.0 - 2026-04-16
#### Features
- (**ci**) add Windows binary to mcpb-publish matrix + claude mcp add install docs - (0417ab5) - Claude Sonnet (coordinator)
- (**mcpb**) add xtask-mcpb library + deterministic bundle pipeline - (d2490cf) - Claude Sonnet (coordinator)
- (**release**) verify release workflow operational with cocogitto versioning - (52e3702) - Claude Sonnet (coordinator)
- add workflow_dispatch for manual release trigger - (6f1ab50) - Claude Sonnet (coordinator)
#### Bug Fixes
- (**ci**) align podman-publish trigger to current CI workflow name; stamp server.json v0.1.0 - (33f838c) - Claude Sonnet (coordinator)
- (**devops**) clear QA backlog — MCP spec, CI wiring, Dockerfile, deps, docs - (8cafec1) - Claude Sonnet (coordinator)
- (**docker**) add xtask/ to Dockerfile COPY so workspace resolves in container - (38ac8d9) - Claude Sonnet (coordinator)
- (**release**) set tag_prefix=v in cog.toml; guard against duplicate releases - (00a5f66) - Claude Sonnet (coordinator)
- (**release**) use --patch flag for cog bump (cog v7 syntax) - (50061d0) - Claude Sonnet (coordinator)
- (**test**) address Copilot review comments on phase6 exposure-gap suite - (9bb99b4) - Claude Sonnet (coordinator)
- simplify release workflow to use cocogitto-action v4 with bundled cog - (b3b927e) - Claude Sonnet (coordinator)
- fetch all tags in release workflow and fix cog.toml config - (942d8f7) - Claude Sonnet (coordinator)
- remove deprecated pre field from cog.toml - (bcfbdd1) - Claude Sonnet (coordinator)
- allow workflow_dispatch trigger in release condition - (54243dc) - Claude Sonnet (coordinator)
- simplify release workflow trigger condition - (8349560) - Claude Sonnet (coordinator)
- use proper mcpb manifest v0.3 schema with server config - (d923aa5) - Claude Sonnet (coordinator)
- use positional args for mcpb pack (directory output) - (1000c18) - Claude Sonnet (coordinator)
- use -o flag for mcpb pack output path - (0ab1788) - Claude Sonnet (coordinator)
- fix mcpb pack output path to stay in bundle dir - (ac8b823) - Claude Sonnet (coordinator)
#### Documentation
- update versioning section to reference justfile release recipe - (95c0652) - Claude Sonnet (coordinator)
#### Miscellaneous Chores
- (**version**) 0.0.1 - (9949944) - Claude Sonnet (coordinator)
- (**version**) 0.1.0 - (c588d71) - github-actions[bot]
- (**version**) 0.1.0 - (e890d6c) - github-actions[bot]
- (**version**) 0.1.0 - (3f6bbac) - github-actions[bot]
- (**version**) 0.1.0 - (b64a975) - github-actions[bot]
- (**version**) 0.1.0 - (dcd8f0f) - github-actions[bot]
- (**version**) 0.1.0 - (8e28f71) - github-actions[bot]
- verify release badge - (44dcc54) - Claude Sonnet (coordinator)
- test release workflow trigger - (c22303f) - Claude Sonnet (coordinator)

- - -

## v0.1.0 - 2026-04-16
#### Features
- (**mcp**) expose P0/P1/P2 tool gap handlers as wired MCP tools - (0482f67) - Claude Sonnet (coordinator)
- (**mcp**) expose account listing and raw-context tools - (79b0e5f) - brianh
- (**test**) add outcome-driven mcp flow runner behind just test - (f50d916) - brianh
#### Bug Fixes
- (**ci**) use valid rust image and pin ledger-core publish version - (e9b1140) - brianh
- (**mcp**) replace absolute paths in docs, fix service-only list, add path traversal guard to get_raw_context - (eac38fb) - copilot-swe-agent[bot]
- add contents:write permission for release creation - (00bbf78) - Claude Sonnet (coordinator)
- update package name from turbo-mcp to ledgerr-mcp in e2e script - (d800497) - Claude Sonnet (coordinator)
- align marketplace and plugin manifests with Cowork validation - (13d10d5) - brianh
#### Tests
- (**turbo-mcp**) add phase6 failing tests for MCP exposure gaps (P0/P1/P2) - (d2b7354) - Claude Sonnet (coordinator)
#### Continuous Integration
- disable MCP Registry publish (requires direct write access) - (abf6b60) - Claude Sonnet (coordinator)
- add MCPB publish gate after tests - (676c58b) - Claude Sonnet (coordinator)
- add clippy sarif upload and podman publish-on-main - (588a231) - brianh
#### Refactoring
- rename turbo-mcp to ledgerr-mcp - (4b484cd) - Claude Sonnet (coordinator)
#### Miscellaneous Chores
- (**dev**) add dotenv ignore and secret setup recipe fixes - (7a029d2) - brianh
- (**docs**) add plugin usage validation flow and mcp cli demos - (d522338) - brianh

- - -

## 0.0.1 - 2026-04-16
#### Features
- (**13-01**) add stdio MCP adapter and proxy boundary - (c95c675) - brianh
- (**13-02**) implement MCP stdio ingest e2e harness and replay checks - (cf28f37) - brianh
- (**13-03**) wire rustledger proxy ingest rows over MCP tools/call - (fd96420) - brianh
- (**14-01**) add service-owned ontology query tool wrappers - (587cbb4) - brianh
- (**14-01**) implement ontology store with deterministic persistence - (c8a8dd1) - brianh
- (**14-02**) add ontology MCP query/export transport handlers - (d5f6ca0) - brianh
- (**15-01**) enforce deterministic reconciliation commit guardrails - (1f7b0ef) - brianh
- (**15-01**) add reconciliation stage contracts and service APIs - (3518079) - brianh
- (**15-02**) expose reconciliation stage tools over MCP transport - (e63959a) - brianh
- (**16-01**) implement deterministic hsm transition and status APIs - (832e1f7) - brianh
- (**16-01**) add hsm domain contracts and service stubs - (b87ded0) - brianh
- (**16-02**) wire deterministic checkpoint persistence and resume - (35d8609) - brianh
- (**16-02**) add hsm checkpoint and resume contracts - (db30cc4) - brianh
- (**16-03**) expose hsm transition status resume over mcp - (674c433) - brianh
- (**17-01**) append deterministic lifecycle events from service actions - (8e56aa5) - brianh
- (**17-01**) add append-only lifecycle event store contracts - (9c2b6fd) - brianh
- (**17-02**) wire lifecycle replay service API - (93d1918) - brianh
- (**17-02**) add deterministic replay projector contracts - (17bf7c2) - brianh
- (**17-03**) wire MCP event replay and history tools - (b02a324) - brianh
- (**18-01**) implement deterministic tax-assist and ambiguity composition - (75b6b97) - brianh
- (**18-01**) add tax-assist service contracts and tool stubs - (749adf0) - brianh
- (**18-02**) implement deterministic evidence-chain retrieval - (8bc24e1) - brianh
- (**18-03**) expose tax-assist interfaces over MCP transport - (621bbea) - brianh
- (**ci**) add Windows binary to mcpb-publish matrix + claude mcp add install docs - (0417ab5) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*
- (**mcp**) expose P0/P1/P2 tool gap handlers as wired MCP tools - (0482f67) - Claude Sonnet (coordinator)
- (**mcp**) expose account listing and raw-context tools - (79b0e5f) - brianh
- (**mcpb**) add xtask-mcpb library + deterministic bundle pipeline - (d2490cf) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*
- (**phase-03**) implement runtime rhai classification and review queue - (accd407) - brianh
- (**phase-04**) add audited classification mutations with invariants - (d833758) - brianh
- (**phase-05**) add cpa workbook export and schedule summaries - (1da735e) - brianh
- (**phase-06**) add ci release automation and mvp e2e flow - (49b5a18) - brianh
- (**phase-1**) scaffold contracts, bootstrap, and turbo MCP interface - (1699ce8) - brianh
- (**phase-2**) complete deterministic ingest pipeline and verification - (ebf0fd5) - brianh
- (**phase-2**) add ingest_pdf and get_raw_context MCP contracts - (3b757a8) - brianh
- (**phase-2**) pivot ingest to rustledger-compatible beancount journals - (f69d7bd) - brianh
- (**phase-2**) add deterministic ingest primitives with idempotency tests - (11b4b9f) - brianh
- (**release**) verify release workflow operational with cocogitto versioning - (52e3702) - Claude Sonnet (coordinator)
- (**test**) add outcome-driven mcp flow runner behind just test - (f50d916) - brianh
- add workflow_dispatch for manual release trigger - (6f1ab50) - Claude Sonnet (coordinator)
- expand Cowork marketplace runtime and packaging guidance - (16fc219) - brianh
- add Claude Cowork plugin marketplace distribution artifacts - (628bd9d) - brianh
#### Bug Fixes
- (**13-01**) harden deterministic status and MCP error mapping - (3f307c7) - brianh
- (**ci**) align podman-publish trigger to current CI workflow name; stamp server.json v0.1.0 - (33f838c) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*
- (**ci**) use valid rust image and pin ledger-core publish version - (e9b1140) - brianh
- (**devops**) clear QA backlog — MCP spec, CI wiring, Dockerfile, deps, docs - (8cafec1) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*
- (**docker**) add xtask/ to Dockerfile COPY so workspace resolves in container - (38ac8d9) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*
- (**mcp**) replace absolute paths in docs, fix service-only list, add path traversal guard to get_raw_context - (eac38fb) - copilot-swe-agent[bot], *elasticdotventures*
- (**release**) use --patch flag for cog bump (cog v7 syntax) - (50061d0) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*
- (**test**) address Copilot review comments on phase6 exposure-gap suite - (9bb99b4) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*
- simplify release workflow to use cocogitto-action v4 with bundled cog - (b3b927e) - Claude Sonnet (coordinator)
- fetch all tags in release workflow and fix cog.toml config - (942d8f7) - Claude Sonnet (coordinator)
- remove deprecated pre field from cog.toml - (bcfbdd1) - Claude Sonnet (coordinator)
- allow workflow_dispatch trigger in release condition - (54243dc) - Claude Sonnet (coordinator)
- simplify release workflow trigger condition - (8349560) - Claude Sonnet (coordinator)
- use proper mcpb manifest v0.3 schema with server config - (d923aa5) - Claude Sonnet (coordinator)
- use positional args for mcpb pack (directory output) - (1000c18) - Claude Sonnet (coordinator)
- use -o flag for mcpb pack output path - (0ab1788) - Claude Sonnet (coordinator)
- fix mcpb pack output path to stay in bundle dir - (ac8b823) - Claude Sonnet (coordinator)
- add contents:write permission for release creation - (00bbf78) - Claude Sonnet (coordinator)
- update package name from turbo-mcp to ledgerr-mcp in e2e script - (d800497) - Claude Sonnet (coordinator)
- align marketplace and plugin manifests with Cowork validation - (13d10d5) - brianh
- remove Zone.Identifier files, add gitignore rule, reconcile STATE.md - (6efa654) - copilot-swe-agent[bot], *elasticdotventures*
- address all PR review feedback (version alignment, path safety, classify_ingested, sheets count, active year, release loop guard) - (47ea635) - copilot-swe-agent[bot], *elasticdotventures*
#### Documentation
- (**02**) research phase domain - (81b1b74) - brianh
- (**02**) add phase plan - (7bd1a7d) - brianh
- (**02**) discuss context - (ae7b8c7) - brianh
- (**03**) discuss context - (3592081) - brianh
- (**13**) add gap-closure plan for rustledger proxy MCP callable surface - (ea8424e) - brianh
- (**13**) add verification report with gap findings - (89f6e11) - brianh
- (**13**) add context research and validation artifacts - (aa12c52) - brianh
- (**13**) create phase plans - (3cd7bb6) - brianh
- (**13-01**) complete mcp boundary proxy surface plan - (a8a709f) - brianh
- (**13-02**) complete mcp-only doc verification plan - (dfa4b63) - brianh
- (**13-02**) publish MCP-only runbook and validation mapping - (787c501) - brianh
- (**13-03**) complete rustledger proxy callable-surface plan - (69b4dfe) - brianh
- (**13-03**) align runbook and validation to rustledger proxy transport - (ebbe75c) - brianh
- (**14**) create ontology persistence phase plans - (eca1808) - brianh
- (**14-01**) complete ontology persistence and query surface plan - (ee80513) - brianh
- (**14-02**) complete ontology mcp transport plan - (0c816f4) - brianh
- (**14-02**) align ontology MCP runbook and validation map - (9dad54e) - brianh
- (**15**) add reconciliation and commit guardrail plans - (55e3bfe) - brianh
- (**15-02**) complete reconciliation-and-commit-guardrails plan - (2468594) - brianh
- (**15-02**) align reconciliation transport runbook and validation map - (8eba986) - brianh
- (**16**) create autonomous HSM phase context and execution plans - (9106bb5) - brianh
- (**16-03**) complete moku-hsm-deterministic-status-and-resume plan - (66ad731) - brianh
- (**17**) create phase plan - (f6a86a5) - brianh
- (**17-01**) complete event domain foundation plan - (b4b410e) - brianh
- (**17-02**) complete deterministic replay plan - (8df1735) - brianh
- (**17-03**) complete MCP event query plan - (2e051d2) - brianh
- (**18**) plan tax assist evidence-chain interfaces - (f80d1f6) - brianh
- (**18-03**) complete tax-assist evidence-chain interfaces plans - (115b5d1) - brianh
- (**agents**) require session lesson capture for posterity - (ad47077) - brianh
- (**milestone**) archive v1.0 roadmap and requirements - (482983d) - brianh
- (**milestone**) add v1.0 audit report - (01ee29d) - brianh
- (**phase-13**) evolve PROJECT.md after phase completion - (4eab3d8) - brianh
- (**phase-13**) complete phase execution - (8939d74) - brianh
- (**roadmap**) add gap closure phases 13-18 - (b347a67) - brianh
- (**state**) align handoff to phase 14 after phase 13 completion - (bd72945) - brianh
- update versioning section to reference justfile release recipe - (95c0652) - Claude Sonnet (coordinator)
- create milestone v1.2 roadmap - (82c024e) - brianh
- define milestone v1.2 requirements - (2e65b2e) - brianh
- start milestone v1.2 Claude Connector Interop - (5960e4f) - brianh
- capture todo - Add Claude Cowork MCP install matrix and CI gate - (c33cb11) - brianh
- add concrete mcp usage examples to agents guide - (9831bc4) - brianh
- add agent purpose/capability guide and README reference - (a1b1bb9) - brianh
- start milestone v1.1 fdkms integrity - (9064ba6) - brianh
- add backlog item 999.1 — CI + release automation hardening - (84de370) - brianh
- create roadmap (6 phases) - (6e9bd7e) - brianh
- define v1 requirements - (4ff43fb) - brianh
- add project research - (c4619ba) - brianh
- initialize project - (4677e99) - brianh
#### Tests
- (**13-01**) add failing MCP adapter contract tests - (546c56e) - brianh
- (**13-02**) add failing MCP stdio DOC requirement tests - (d95d617) - brianh
- (**13-03**) add failing rustledger proxy transport coverage - (1aa7970) - brianh
- (**14-01**) add failing ONTO-01/02 ontology contract tests - (ce24318) - brianh
- (**14-02**) add failing ontology MCP transport tests - (ae0098a) - brianh
- (**15-01**) add failing reconciliation guardrail contracts - (163ebd9) - brianh
- (**15-02**) add failing reconciliation MCP transport contracts - (7f75625) - brianh
- (**16-01**) add failing hsm lifecycle and guard contracts - (fa2178b) - brianh
- (**16-02**) add failing checkpoint and resume contracts - (181b02c) - brianh
- (**16-03**) add failing hsm mcp transport e2e contracts - (8001964) - brianh
- (**17-01**) add failing contracts for append-only lifecycle events - (c6ff2ef) - brianh
- (**17-02**) add failing deterministic replay contracts - (30f3f20) - brianh
- (**17-03**) add failing MCP event history e2e contracts - (ec4a80e) - brianh
- (**18-01**) add failing tax-assist contracts for TAXA-01 and TAXA-03 - (ca1f8b6) - brianh
- (**18-02**) add failing evidence-chain contract for TAXA-02 - (bde256f) - brianh
- (**18-03**) add failing MCP e2e contracts for tax-assist tools - (baac788) - brianh
- (**e2e**) expand bdd coverage for ingest tool behaviors - (7efd6ac) - brianh
- (**turbo-mcp**) add phase6 failing tests for MCP exposure gaps (P0/P1/P2) - (d2b7354) - Claude Sonnet (coordinator), *Claude Sonnet 4.6*
- add sample statement fixtures for e2e regression - (5f40bf6) - brianh
#### Continuous Integration
- disable MCP Registry publish (requires direct write access) - (abf6b60) - Claude Sonnet (coordinator)
- add MCPB publish gate after tests - (676c58b) - Claude Sonnet (coordinator)
- add clippy sarif upload and podman publish-on-main - (588a231) - brianh
- add publish workflow for ghcr crates and pypi - (e6bb524) - brianh
#### Refactoring
- rename turbo-mcp to ledgerr-mcp - (4b484cd) - Claude Sonnet (coordinator)
#### Miscellaneous Chores
- (**16-03**) publish hsm validation map and operator verification docs - (eebdcda) - brianh
- (**17**) capture MCP row-key normalization guidance in AGENTS - (1323e94) - brianh
- (**17-03**) publish event MCP validation and verification docs - (2a7f381) - brianh
- (**18-03**) publish tax-assist runbook and phase validation map - (54f8012) - brianh
- (**dev**) add dotenv ignore and secret setup recipe fixes - (7a029d2) - brianh
- (**docs**) add plugin usage validation flow and mcp cli demos - (d522338) - brianh
- (**planning**) persist autonomous closeout state - (b9ba437) - brianh
- (**release**) add changelog for v1.1.0 - (acb4d27) - brianh
- (**v1.1**) archive completed milestone phases - (cf9b3c2) - brianh
- (**version**) 0.1.0 - (c588d71) - github-actions[bot]
- (**version**) 0.1.0 - (e890d6c) - github-actions[bot]
- (**version**) 0.1.0 - (3f6bbac) - github-actions[bot]
- (**version**) 0.1.0 - (b64a975) - github-actions[bot]
- (**version**) 0.1.0 - (dcd8f0f) - github-actions[bot]
- (**version**) 0.1.0 - (8e28f71) - github-actions[bot]
- verify release badge - (44dcc54) - Claude Sonnet (coordinator)
- test release workflow trigger - (c22303f) - Claude Sonnet (coordinator)
- archive phase directories from completed milestones - (230159f) - brianh
- archive v1.0 milestone - (571ab76) - brianh
- ignore local PRD and refine phase 2 plan - (023da6b) - brianh
- add project config - (f498561) - brianh

- - -

## 0.1.0 - 2026-04-16
#### Features
- (**13-01**) add stdio MCP adapter and proxy boundary - (c95c675) - brianh
- (**13-02**) implement MCP stdio ingest e2e harness and replay checks - (cf28f37) - brianh
- (**13-03**) wire rustledger proxy ingest rows over MCP tools/call - (fd96420) - brianh
- (**14-01**) add service-owned ontology query tool wrappers - (587cbb4) - brianh
- (**14-01**) implement ontology store with deterministic persistence - (c8a8dd1) - brianh
- (**14-02**) add ontology MCP query/export transport handlers - (d5f6ca0) - brianh
- (**15-01**) enforce deterministic reconciliation commit guardrails - (1f7b0ef) - brianh
- (**15-01**) add reconciliation stage contracts and service APIs - (3518079) - brianh
- (**15-02**) expose reconciliation stage tools over MCP transport - (e63959a) - brianh
- (**16-01**) implement deterministic hsm transition and status APIs - (832e1f7) - brianh
- (**16-01**) add hsm domain contracts and service stubs - (b87ded0) - brianh
- (**16-02**) wire deterministic checkpoint persistence and resume - (35d8609) - brianh
- (**16-02**) add hsm checkpoint and resume contracts - (db30cc4) - brianh
- (**16-03**) expose hsm transition status resume over mcp - (674c433) - brianh
- (**17-01**) append deterministic lifecycle events from service actions - (8e56aa5) - brianh
- (**17-01**) add append-only lifecycle event store contracts - (9c2b6fd) - brianh
- (**17-02**) wire lifecycle replay service API - (93d1918) - brianh
- (**17-02**) add deterministic replay projector contracts - (17bf7c2) - brianh
- (**17-03**) wire MCP event replay and history tools - (b02a324) - brianh
- (**18-01**) implement deterministic tax-assist and ambiguity composition - (75b6b97) - brianh
- (**18-01**) add tax-assist service contracts and tool stubs - (749adf0) - brianh
- (**18-02**) implement deterministic evidence-chain retrieval - (8bc24e1) - brianh
- (**18-03**) expose tax-assist interfaces over MCP transport - (621bbea) - brianh
- (**ci**) add Windows binary to mcpb-publish matrix + claude mcp add install docs - (0417ab5) - Claude Sonnet (coordinator)
- (**mcp**) expose P0/P1/P2 tool gap handlers as wired MCP tools - (0482f67) - Claude Sonnet (coordinator)
- (**mcp**) expose account listing and raw-context tools - (79b0e5f) - brianh
- (**mcpb**) add xtask-mcpb library + deterministic bundle pipeline - (d2490cf) - Claude Sonnet (coordinator)
- (**phase-03**) implement runtime rhai classification and review queue - (accd407) - brianh
- (**phase-04**) add audited classification mutations with invariants - (d833758) - brianh
- (**phase-05**) add cpa workbook export and schedule summaries - (1da735e) - brianh
- (**phase-06**) add ci release automation and mvp e2e flow - (49b5a18) - brianh
- (**phase-1**) scaffold contracts, bootstrap, and turbo MCP interface - (1699ce8) - brianh
- (**phase-2**) complete deterministic ingest pipeline and verification - (ebf0fd5) - brianh
- (**phase-2**) add ingest_pdf and get_raw_context MCP contracts - (3b757a8) - brianh
- (**phase-2**) pivot ingest to rustledger-compatible beancount journals - (f69d7bd) - brianh
- (**phase-2**) add deterministic ingest primitives with idempotency tests - (11b4b9f) - brianh
- (**release**) verify release workflow operational with cocogitto versioning - (52e3702) - Claude Sonnet (coordinator)
- (**test**) add outcome-driven mcp flow runner behind just test - (f50d916) - brianh
- add workflow_dispatch for manual release trigger - (6f1ab50) - Claude Sonnet (coordinator)
- expand Cowork marketplace runtime and packaging guidance - (16fc219) - brianh
- add Claude Cowork plugin marketplace distribution artifacts - (628bd9d) - brianh
#### Bug Fixes
- (**13-01**) harden deterministic status and MCP error mapping - (3f307c7) - brianh
- (**ci**) align podman-publish trigger to current CI workflow name; stamp server.json v0.1.0 - (33f838c) - Claude Sonnet (coordinator)
- (**ci**) use valid rust image and pin ledger-core publish version - (e9b1140) - brianh
- (**devops**) clear QA backlog — MCP spec, CI wiring, Dockerfile, deps, docs - (8cafec1) - Claude Sonnet (coordinator)
- (**docker**) add xtask/ to Dockerfile COPY so workspace resolves in container - (38ac8d9) - Claude Sonnet (coordinator)
- (**mcp**) replace absolute paths in docs, fix service-only list, add path traversal guard to get_raw_context - (eac38fb) - copilot-swe-agent[bot]
- (**test**) address Copilot review comments on phase6 exposure-gap suite - (9bb99b4) - Claude Sonnet (coordinator)
- simplify release workflow to use cocogitto-action v4 with bundled cog - (b3b927e) - Claude Sonnet (coordinator)
- fetch all tags in release workflow and fix cog.toml config - (942d8f7) - Claude Sonnet (coordinator)
- remove deprecated pre field from cog.toml - (bcfbdd1) - Claude Sonnet (coordinator)
- allow workflow_dispatch trigger in release condition - (54243dc) - Claude Sonnet (coordinator)
- simplify release workflow trigger condition - (8349560) - Claude Sonnet (coordinator)
- use proper mcpb manifest v0.3 schema with server config - (d923aa5) - Claude Sonnet (coordinator)
- use positional args for mcpb pack (directory output) - (1000c18) - Claude Sonnet (coordinator)
- use -o flag for mcpb pack output path - (0ab1788) - Claude Sonnet (coordinator)
- fix mcpb pack output path to stay in bundle dir - (ac8b823) - Claude Sonnet (coordinator)
- add contents:write permission for release creation - (00bbf78) - Claude Sonnet (coordinator)
- update package name from turbo-mcp to ledgerr-mcp in e2e script - (d800497) - Claude Sonnet (coordinator)
- align marketplace and plugin manifests with Cowork validation - (13d10d5) - brianh
- remove Zone.Identifier files, add gitignore rule, reconcile STATE.md - (6efa654) - copilot-swe-agent[bot]
- address all PR review feedback (version alignment, path safety, classify_ingested, sheets count, active year, release loop guard) - (47ea635) - copilot-swe-agent[bot]
#### Documentation
- (**02**) research phase domain - (81b1b74) - brianh
- (**02**) add phase plan - (7bd1a7d) - brianh
- (**02**) discuss context - (ae7b8c7) - brianh
- (**03**) discuss context - (3592081) - brianh
- (**13**) add gap-closure plan for rustledger proxy MCP callable surface - (ea8424e) - brianh
- (**13**) add verification report with gap findings - (89f6e11) - brianh
- (**13**) add context research and validation artifacts - (aa12c52) - brianh
- (**13**) create phase plans - (3cd7bb6) - brianh
- (**13-01**) complete mcp boundary proxy surface plan - (a8a709f) - brianh
- (**13-02**) complete mcp-only doc verification plan - (dfa4b63) - brianh
- (**13-02**) publish MCP-only runbook and validation mapping - (787c501) - brianh
- (**13-03**) complete rustledger proxy callable-surface plan - (69b4dfe) - brianh
- (**13-03**) align runbook and validation to rustledger proxy transport - (ebbe75c) - brianh
- (**14**) create ontology persistence phase plans - (eca1808) - brianh
- (**14-01**) complete ontology persistence and query surface plan - (ee80513) - brianh
- (**14-02**) complete ontology mcp transport plan - (0c816f4) - brianh
- (**14-02**) align ontology MCP runbook and validation map - (9dad54e) - brianh
- (**15**) add reconciliation and commit guardrail plans - (55e3bfe) - brianh
- (**15-02**) complete reconciliation-and-commit-guardrails plan - (2468594) - brianh
- (**15-02**) align reconciliation transport runbook and validation map - (8eba986) - brianh
- (**16**) create autonomous HSM phase context and execution plans - (9106bb5) - brianh
- (**16-03**) complete moku-hsm-deterministic-status-and-resume plan - (66ad731) - brianh
- (**17**) create phase plan - (f6a86a5) - brianh
- (**17-01**) complete event domain foundation plan - (b4b410e) - brianh
- (**17-02**) complete deterministic replay plan - (8df1735) - brianh
- (**17-03**) complete MCP event query plan - (2e051d2) - brianh
- (**18**) plan tax assist evidence-chain interfaces - (f80d1f6) - brianh
- (**18-03**) complete tax-assist evidence-chain interfaces plans - (115b5d1) - brianh
- (**agents**) require session lesson capture for posterity - (ad47077) - brianh
- (**milestone**) archive v1.0 roadmap and requirements - (482983d) - brianh
- (**milestone**) add v1.0 audit report - (01ee29d) - brianh
- (**phase-13**) evolve PROJECT.md after phase completion - (4eab3d8) - brianh
- (**phase-13**) complete phase execution - (8939d74) - brianh
- (**roadmap**) add gap closure phases 13-18 - (b347a67) - brianh
- (**state**) align handoff to phase 14 after phase 13 completion - (bd72945) - brianh
- update versioning section to reference justfile release recipe - (95c0652) - Claude Sonnet (coordinator)
- create milestone v1.2 roadmap - (82c024e) - brianh
- define milestone v1.2 requirements - (2e65b2e) - brianh
- start milestone v1.2 Claude Connector Interop - (5960e4f) - brianh
- capture todo - Add Claude Cowork MCP install matrix and CI gate - (c33cb11) - brianh
- add concrete mcp usage examples to agents guide - (9831bc4) - brianh
- add agent purpose/capability guide and README reference - (a1b1bb9) - brianh
- start milestone v1.1 fdkms integrity - (9064ba6) - brianh
- add backlog item 999.1 — CI + release automation hardening - (84de370) - brianh
- create roadmap (6 phases) - (6e9bd7e) - brianh
- define v1 requirements - (4ff43fb) - brianh
- add project research - (c4619ba) - brianh
- initialize project - (4677e99) - brianh
#### Tests
- (**13-01**) add failing MCP adapter contract tests - (546c56e) - brianh
- (**13-02**) add failing MCP stdio DOC requirement tests - (d95d617) - brianh
- (**13-03**) add failing rustledger proxy transport coverage - (1aa7970) - brianh
- (**14-01**) add failing ONTO-01/02 ontology contract tests - (ce24318) - brianh
- (**14-02**) add failing ontology MCP transport tests - (ae0098a) - brianh
- (**15-01**) add failing reconciliation guardrail contracts - (163ebd9) - brianh
- (**15-02**) add failing reconciliation MCP transport contracts - (7f75625) - brianh
- (**16-01**) add failing hsm lifecycle and guard contracts - (fa2178b) - brianh
- (**16-02**) add failing checkpoint and resume contracts - (181b02c) - brianh
- (**16-03**) add failing hsm mcp transport e2e contracts - (8001964) - brianh
- (**17-01**) add failing contracts for append-only lifecycle events - (c6ff2ef) - brianh
- (**17-02**) add failing deterministic replay contracts - (30f3f20) - brianh
- (**17-03**) add failing MCP event history e2e contracts - (ec4a80e) - brianh
- (**18-01**) add failing tax-assist contracts for TAXA-01 and TAXA-03 - (ca1f8b6) - brianh
- (**18-02**) add failing evidence-chain contract for TAXA-02 - (bde256f) - brianh
- (**18-03**) add failing MCP e2e contracts for tax-assist tools - (baac788) - brianh
- (**e2e**) expand bdd coverage for ingest tool behaviors - (7efd6ac) - brianh
- (**turbo-mcp**) add phase6 failing tests for MCP exposure gaps (P0/P1/P2) - (d2b7354) - Claude Sonnet (coordinator)
- add sample statement fixtures for e2e regression - (5f40bf6) - brianh
#### Continuous Integration
- disable MCP Registry publish (requires direct write access) - (abf6b60) - Claude Sonnet (coordinator)
- add MCPB publish gate after tests - (676c58b) - Claude Sonnet (coordinator)
- add clippy sarif upload and podman publish-on-main - (588a231) - brianh
- add publish workflow for ghcr crates and pypi - (e6bb524) - brianh
#### Refactoring
- rename turbo-mcp to ledgerr-mcp - (4b484cd) - Claude Sonnet (coordinator)
#### Miscellaneous Chores
- (**16-03**) publish hsm validation map and operator verification docs - (eebdcda) - brianh
- (**17**) capture MCP row-key normalization guidance in AGENTS - (1323e94) - brianh
- (**17-03**) publish event MCP validation and verification docs - (2a7f381) - brianh
- (**18-03**) publish tax-assist runbook and phase validation map - (54f8012) - brianh
- (**dev**) add dotenv ignore and secret setup recipe fixes - (7a029d2) - brianh
- (**docs**) add plugin usage validation flow and mcp cli demos - (d522338) - brianh
- (**planning**) persist autonomous closeout state - (b9ba437) - brianh
- (**release**) add changelog for v1.1.0 - (acb4d27) - brianh
- (**v1.1**) archive completed milestone phases - (cf9b3c2) - brianh
- (**version**) 0.1.0 - (e890d6c) - github-actions[bot]
- (**version**) 0.1.0 - (3f6bbac) - github-actions[bot]
- (**version**) 0.1.0 - (b64a975) - github-actions[bot]
- (**version**) 0.1.0 - (dcd8f0f) - github-actions[bot]
- (**version**) 0.1.0 - (8e28f71) - github-actions[bot]
- verify release badge - (44dcc54) - Claude Sonnet (coordinator)
- test release workflow trigger - (c22303f) - Claude Sonnet (coordinator)
- archive phase directories from completed milestones - (230159f) - brianh
- archive v1.0 milestone - (571ab76) - brianh
- ignore local PRD and refine phase 2 plan - (023da6b) - brianh
- add project config - (f498561) - brianh

- - -

## 0.1.0 - 2026-04-15
#### Features
- (**13-01**) add stdio MCP adapter and proxy boundary - (c95c675) - brianh
- (**13-02**) implement MCP stdio ingest e2e harness and replay checks - (cf28f37) - brianh
- (**13-03**) wire rustledger proxy ingest rows over MCP tools/call - (fd96420) - brianh
- (**14-01**) add service-owned ontology query tool wrappers - (587cbb4) - brianh
- (**14-01**) implement ontology store with deterministic persistence - (c8a8dd1) - brianh
- (**14-02**) add ontology MCP query/export transport handlers - (d5f6ca0) - brianh
- (**15-01**) enforce deterministic reconciliation commit guardrails - (1f7b0ef) - brianh
- (**15-01**) add reconciliation stage contracts and service APIs - (3518079) - brianh
- (**15-02**) expose reconciliation stage tools over MCP transport - (e63959a) - brianh
- (**16-01**) implement deterministic hsm transition and status APIs - (832e1f7) - brianh
- (**16-01**) add hsm domain contracts and service stubs - (b87ded0) - brianh
- (**16-02**) wire deterministic checkpoint persistence and resume - (35d8609) - brianh
- (**16-02**) add hsm checkpoint and resume contracts - (db30cc4) - brianh
- (**16-03**) expose hsm transition status resume over mcp - (674c433) - brianh
- (**17-01**) append deterministic lifecycle events from service actions - (8e56aa5) - brianh
- (**17-01**) add append-only lifecycle event store contracts - (9c2b6fd) - brianh
- (**17-02**) wire lifecycle replay service API - (93d1918) - brianh
- (**17-02**) add deterministic replay projector contracts - (17bf7c2) - brianh
- (**17-03**) wire MCP event replay and history tools - (b02a324) - brianh
- (**18-01**) implement deterministic tax-assist and ambiguity composition - (75b6b97) - brianh
- (**18-01**) add tax-assist service contracts and tool stubs - (749adf0) - brianh
- (**18-02**) implement deterministic evidence-chain retrieval - (8bc24e1) - brianh
- (**18-03**) expose tax-assist interfaces over MCP transport - (621bbea) - brianh
- (**mcp**) expose P0/P1/P2 tool gap handlers as wired MCP tools - (0482f67) - Claude Sonnet (coordinator)
- (**mcp**) expose account listing and raw-context tools - (79b0e5f) - brianh
- (**mcpb**) add xtask-mcpb library + deterministic bundle pipeline - (d2490cf) - Claude Sonnet (coordinator)
- (**phase-03**) implement runtime rhai classification and review queue - (accd407) - brianh
- (**phase-04**) add audited classification mutations with invariants - (d833758) - brianh
- (**phase-05**) add cpa workbook export and schedule summaries - (1da735e) - brianh
- (**phase-06**) add ci release automation and mvp e2e flow - (49b5a18) - brianh
- (**phase-1**) scaffold contracts, bootstrap, and turbo MCP interface - (1699ce8) - brianh
- (**phase-2**) complete deterministic ingest pipeline and verification - (ebf0fd5) - brianh
- (**phase-2**) add ingest_pdf and get_raw_context MCP contracts - (3b757a8) - brianh
- (**phase-2**) pivot ingest to rustledger-compatible beancount journals - (f69d7bd) - brianh
- (**phase-2**) add deterministic ingest primitives with idempotency tests - (11b4b9f) - brianh
- (**release**) verify release workflow operational with cocogitto versioning - (52e3702) - Claude Sonnet (coordinator)
- (**test**) add outcome-driven mcp flow runner behind just test - (f50d916) - brianh
- add workflow_dispatch for manual release trigger - (6f1ab50) - Claude Sonnet (coordinator)
- expand Cowork marketplace runtime and packaging guidance - (16fc219) - brianh
- add Claude Cowork plugin marketplace distribution artifacts - (628bd9d) - brianh
#### Bug Fixes
- (**13-01**) harden deterministic status and MCP error mapping - (3f307c7) - brianh
- (**ci**) align podman-publish trigger to current CI workflow name; stamp server.json v0.1.0 - (33f838c) - Claude Sonnet (coordinator)
- (**ci**) use valid rust image and pin ledger-core publish version - (e9b1140) - brianh
- (**devops**) clear QA backlog — MCP spec, CI wiring, Dockerfile, deps, docs - (8cafec1) - Claude Sonnet (coordinator)
- (**docker**) add xtask/ to Dockerfile COPY so workspace resolves in container - (38ac8d9) - Claude Sonnet (coordinator)
- (**mcp**) replace absolute paths in docs, fix service-only list, add path traversal guard to get_raw_context - (eac38fb) - copilot-swe-agent[bot]
- (**test**) address Copilot review comments on phase6 exposure-gap suite - (9bb99b4) - Claude Sonnet (coordinator)
- simplify release workflow to use cocogitto-action v4 with bundled cog - (b3b927e) - Claude Sonnet (coordinator)
- fetch all tags in release workflow and fix cog.toml config - (942d8f7) - Claude Sonnet (coordinator)
- remove deprecated pre field from cog.toml - (bcfbdd1) - Claude Sonnet (coordinator)
- allow workflow_dispatch trigger in release condition - (54243dc) - Claude Sonnet (coordinator)
- simplify release workflow trigger condition - (8349560) - Claude Sonnet (coordinator)
- use proper mcpb manifest v0.3 schema with server config - (d923aa5) - Claude Sonnet (coordinator)
- use positional args for mcpb pack (directory output) - (1000c18) - Claude Sonnet (coordinator)
- use -o flag for mcpb pack output path - (0ab1788) - Claude Sonnet (coordinator)
- fix mcpb pack output path to stay in bundle dir - (ac8b823) - Claude Sonnet (coordinator)
- add contents:write permission for release creation - (00bbf78) - Claude Sonnet (coordinator)
- update package name from turbo-mcp to ledgerr-mcp in e2e script - (d800497) - Claude Sonnet (coordinator)
- align marketplace and plugin manifests with Cowork validation - (13d10d5) - brianh
- remove Zone.Identifier files, add gitignore rule, reconcile STATE.md - (6efa654) - copilot-swe-agent[bot]
- address all PR review feedback (version alignment, path safety, classify_ingested, sheets count, active year, release loop guard) - (47ea635) - copilot-swe-agent[bot]
#### Documentation
- (**02**) research phase domain - (81b1b74) - brianh
- (**02**) add phase plan - (7bd1a7d) - brianh
- (**02**) discuss context - (ae7b8c7) - brianh
- (**03**) discuss context - (3592081) - brianh
- (**13**) add gap-closure plan for rustledger proxy MCP callable surface - (ea8424e) - brianh
- (**13**) add verification report with gap findings - (89f6e11) - brianh
- (**13**) add context research and validation artifacts - (aa12c52) - brianh
- (**13**) create phase plans - (3cd7bb6) - brianh
- (**13-01**) complete mcp boundary proxy surface plan - (a8a709f) - brianh
- (**13-02**) complete mcp-only doc verification plan - (dfa4b63) - brianh
- (**13-02**) publish MCP-only runbook and validation mapping - (787c501) - brianh
- (**13-03**) complete rustledger proxy callable-surface plan - (69b4dfe) - brianh
- (**13-03**) align runbook and validation to rustledger proxy transport - (ebbe75c) - brianh
- (**14**) create ontology persistence phase plans - (eca1808) - brianh
- (**14-01**) complete ontology persistence and query surface plan - (ee80513) - brianh
- (**14-02**) complete ontology mcp transport plan - (0c816f4) - brianh
- (**14-02**) align ontology MCP runbook and validation map - (9dad54e) - brianh
- (**15**) add reconciliation and commit guardrail plans - (55e3bfe) - brianh
- (**15-02**) complete reconciliation-and-commit-guardrails plan - (2468594) - brianh
- (**15-02**) align reconciliation transport runbook and validation map - (8eba986) - brianh
- (**16**) create autonomous HSM phase context and execution plans - (9106bb5) - brianh
- (**16-03**) complete moku-hsm-deterministic-status-and-resume plan - (66ad731) - brianh
- (**17**) create phase plan - (f6a86a5) - brianh
- (**17-01**) complete event domain foundation plan - (b4b410e) - brianh
- (**17-02**) complete deterministic replay plan - (8df1735) - brianh
- (**17-03**) complete MCP event query plan - (2e051d2) - brianh
- (**18**) plan tax assist evidence-chain interfaces - (f80d1f6) - brianh
- (**18-03**) complete tax-assist evidence-chain interfaces plans - (115b5d1) - brianh
- (**agents**) require session lesson capture for posterity - (ad47077) - brianh
- (**milestone**) archive v1.0 roadmap and requirements - (482983d) - brianh
- (**milestone**) add v1.0 audit report - (01ee29d) - brianh
- (**phase-13**) evolve PROJECT.md after phase completion - (4eab3d8) - brianh
- (**phase-13**) complete phase execution - (8939d74) - brianh
- (**roadmap**) add gap closure phases 13-18 - (b347a67) - brianh
- (**state**) align handoff to phase 14 after phase 13 completion - (bd72945) - brianh
- update versioning section to reference justfile release recipe - (95c0652) - Claude Sonnet (coordinator)
- create milestone v1.2 roadmap - (82c024e) - brianh
- define milestone v1.2 requirements - (2e65b2e) - brianh
- start milestone v1.2 Claude Connector Interop - (5960e4f) - brianh
- capture todo - Add Claude Cowork MCP install matrix and CI gate - (c33cb11) - brianh
- add concrete mcp usage examples to agents guide - (9831bc4) - brianh
- add agent purpose/capability guide and README reference - (a1b1bb9) - brianh
- start milestone v1.1 fdkms integrity - (9064ba6) - brianh
- add backlog item 999.1 — CI + release automation hardening - (84de370) - brianh
- create roadmap (6 phases) - (6e9bd7e) - brianh
- define v1 requirements - (4ff43fb) - brianh
- add project research - (c4619ba) - brianh
- initialize project - (4677e99) - brianh
#### Tests
- (**13-01**) add failing MCP adapter contract tests - (546c56e) - brianh
- (**13-02**) add failing MCP stdio DOC requirement tests - (d95d617) - brianh
- (**13-03**) add failing rustledger proxy transport coverage - (1aa7970) - brianh
- (**14-01**) add failing ONTO-01/02 ontology contract tests - (ce24318) - brianh
- (**14-02**) add failing ontology MCP transport tests - (ae0098a) - brianh
- (**15-01**) add failing reconciliation guardrail contracts - (163ebd9) - brianh
- (**15-02**) add failing reconciliation MCP transport contracts - (7f75625) - brianh
- (**16-01**) add failing hsm lifecycle and guard contracts - (fa2178b) - brianh
- (**16-02**) add failing checkpoint and resume contracts - (181b02c) - brianh
- (**16-03**) add failing hsm mcp transport e2e contracts - (8001964) - brianh
- (**17-01**) add failing contracts for append-only lifecycle events - (c6ff2ef) - brianh
- (**17-02**) add failing deterministic replay contracts - (30f3f20) - brianh
- (**17-03**) add failing MCP event history e2e contracts - (ec4a80e) - brianh
- (**18-01**) add failing tax-assist contracts for TAXA-01 and TAXA-03 - (ca1f8b6) - brianh
- (**18-02**) add failing evidence-chain contract for TAXA-02 - (bde256f) - brianh
- (**18-03**) add failing MCP e2e contracts for tax-assist tools - (baac788) - brianh
- (**e2e**) expand bdd coverage for ingest tool behaviors - (7efd6ac) - brianh
- (**turbo-mcp**) add phase6 failing tests for MCP exposure gaps (P0/P1/P2) - (d2b7354) - Claude Sonnet (coordinator)
- add sample statement fixtures for e2e regression - (5f40bf6) - brianh
#### Continuous Integration
- disable MCP Registry publish (requires direct write access) - (abf6b60) - Claude Sonnet (coordinator)
- add MCPB publish gate after tests - (676c58b) - Claude Sonnet (coordinator)
- add clippy sarif upload and podman publish-on-main - (588a231) - brianh
- add publish workflow for ghcr crates and pypi - (e6bb524) - brianh
#### Refactoring
- rename turbo-mcp to ledgerr-mcp - (4b484cd) - Claude Sonnet (coordinator)
#### Miscellaneous Chores
- (**16-03**) publish hsm validation map and operator verification docs - (eebdcda) - brianh
- (**17**) capture MCP row-key normalization guidance in AGENTS - (1323e94) - brianh
- (**17-03**) publish event MCP validation and verification docs - (2a7f381) - brianh
- (**18-03**) publish tax-assist runbook and phase validation map - (54f8012) - brianh
- (**dev**) add dotenv ignore and secret setup recipe fixes - (7a029d2) - brianh
- (**docs**) add plugin usage validation flow and mcp cli demos - (d522338) - brianh
- (**planning**) persist autonomous closeout state - (b9ba437) - brianh
- (**release**) add changelog for v1.1.0 - (acb4d27) - brianh
- (**v1.1**) archive completed milestone phases - (cf9b3c2) - brianh
- (**version**) 0.1.0 - (3f6bbac) - github-actions[bot]
- (**version**) 0.1.0 - (b64a975) - github-actions[bot]
- (**version**) 0.1.0 - (dcd8f0f) - github-actions[bot]
- (**version**) 0.1.0 - (8e28f71) - github-actions[bot]
- verify release badge - (44dcc54) - Claude Sonnet (coordinator)
- test release workflow trigger - (c22303f) - Claude Sonnet (coordinator)
- archive phase directories from completed milestones - (230159f) - brianh
- archive v1.0 milestone - (571ab76) - brianh
- ignore local PRD and refine phase 2 plan - (023da6b) - brianh
- add project config - (f498561) - brianh

- - -

## 0.1.0 - 2026-04-15
#### Features
- (**13-01**) add stdio MCP adapter and proxy boundary - (c95c675) - brianh
- (**13-02**) implement MCP stdio ingest e2e harness and replay checks - (cf28f37) - brianh
- (**13-03**) wire rustledger proxy ingest rows over MCP tools/call - (fd96420) - brianh
- (**14-01**) add service-owned ontology query tool wrappers - (587cbb4) - brianh
- (**14-01**) implement ontology store with deterministic persistence - (c8a8dd1) - brianh
- (**14-02**) add ontology MCP query/export transport handlers - (d5f6ca0) - brianh
- (**15-01**) enforce deterministic reconciliation commit guardrails - (1f7b0ef) - brianh
- (**15-01**) add reconciliation stage contracts and service APIs - (3518079) - brianh
- (**15-02**) expose reconciliation stage tools over MCP transport - (e63959a) - brianh
- (**16-01**) implement deterministic hsm transition and status APIs - (832e1f7) - brianh
- (**16-01**) add hsm domain contracts and service stubs - (b87ded0) - brianh
- (**16-02**) wire deterministic checkpoint persistence and resume - (35d8609) - brianh
- (**16-02**) add hsm checkpoint and resume contracts - (db30cc4) - brianh
- (**16-03**) expose hsm transition status resume over mcp - (674c433) - brianh
- (**17-01**) append deterministic lifecycle events from service actions - (8e56aa5) - brianh
- (**17-01**) add append-only lifecycle event store contracts - (9c2b6fd) - brianh
- (**17-02**) wire lifecycle replay service API - (93d1918) - brianh
- (**17-02**) add deterministic replay projector contracts - (17bf7c2) - brianh
- (**17-03**) wire MCP event replay and history tools - (b02a324) - brianh
- (**18-01**) implement deterministic tax-assist and ambiguity composition - (75b6b97) - brianh
- (**18-01**) add tax-assist service contracts and tool stubs - (749adf0) - brianh
- (**18-02**) implement deterministic evidence-chain retrieval - (8bc24e1) - brianh
- (**18-03**) expose tax-assist interfaces over MCP transport - (621bbea) - brianh
- (**mcp**) expose P0/P1/P2 tool gap handlers as wired MCP tools - (0482f67) - Claude Sonnet (coordinator)
- (**mcp**) expose account listing and raw-context tools - (79b0e5f) - brianh
- (**mcpb**) add xtask-mcpb library + deterministic bundle pipeline - (d2490cf) - Claude Sonnet (coordinator)
- (**phase-03**) implement runtime rhai classification and review queue - (accd407) - brianh
- (**phase-04**) add audited classification mutations with invariants - (d833758) - brianh
- (**phase-05**) add cpa workbook export and schedule summaries - (1da735e) - brianh
- (**phase-06**) add ci release automation and mvp e2e flow - (49b5a18) - brianh
- (**phase-1**) scaffold contracts, bootstrap, and turbo MCP interface - (1699ce8) - brianh
- (**phase-2**) complete deterministic ingest pipeline and verification - (ebf0fd5) - brianh
- (**phase-2**) add ingest_pdf and get_raw_context MCP contracts - (3b757a8) - brianh
- (**phase-2**) pivot ingest to rustledger-compatible beancount journals - (f69d7bd) - brianh
- (**phase-2**) add deterministic ingest primitives with idempotency tests - (11b4b9f) - brianh
- (**release**) verify release workflow operational with cocogitto versioning - (52e3702) - Claude Sonnet (coordinator)
- (**test**) add outcome-driven mcp flow runner behind just test - (f50d916) - brianh
- add workflow_dispatch for manual release trigger - (6f1ab50) - Claude Sonnet (coordinator)
- expand Cowork marketplace runtime and packaging guidance - (16fc219) - brianh
- add Claude Cowork plugin marketplace distribution artifacts - (628bd9d) - brianh
#### Bug Fixes
- (**13-01**) harden deterministic status and MCP error mapping - (3f307c7) - brianh
- (**ci**) align podman-publish trigger to current CI workflow name; stamp server.json v0.1.0 - (33f838c) - Claude Sonnet (coordinator)
- (**ci**) use valid rust image and pin ledger-core publish version - (e9b1140) - brianh
- (**devops**) clear QA backlog — MCP spec, CI wiring, Dockerfile, deps, docs - (8cafec1) - Claude Sonnet (coordinator)
- (**docker**) add xtask/ to Dockerfile COPY so workspace resolves in container - (38ac8d9) - Claude Sonnet (coordinator)
- (**mcp**) replace absolute paths in docs, fix service-only list, add path traversal guard to get_raw_context - (eac38fb) - copilot-swe-agent[bot]
- simplify release workflow to use cocogitto-action v4 with bundled cog - (b3b927e) - Claude Sonnet (coordinator)
- fetch all tags in release workflow and fix cog.toml config - (942d8f7) - Claude Sonnet (coordinator)
- remove deprecated pre field from cog.toml - (bcfbdd1) - Claude Sonnet (coordinator)
- allow workflow_dispatch trigger in release condition - (54243dc) - Claude Sonnet (coordinator)
- simplify release workflow trigger condition - (8349560) - Claude Sonnet (coordinator)
- use proper mcpb manifest v0.3 schema with server config - (d923aa5) - Claude Sonnet (coordinator)
- use positional args for mcpb pack (directory output) - (1000c18) - Claude Sonnet (coordinator)
- use -o flag for mcpb pack output path - (0ab1788) - Claude Sonnet (coordinator)
- fix mcpb pack output path to stay in bundle dir - (ac8b823) - Claude Sonnet (coordinator)
- add contents:write permission for release creation - (00bbf78) - Claude Sonnet (coordinator)
- update package name from turbo-mcp to ledgerr-mcp in e2e script - (d800497) - Claude Sonnet (coordinator)
- align marketplace and plugin manifests with Cowork validation - (13d10d5) - brianh
- remove Zone.Identifier files, add gitignore rule, reconcile STATE.md - (6efa654) - copilot-swe-agent[bot]
- address all PR review feedback (version alignment, path safety, classify_ingested, sheets count, active year, release loop guard) - (47ea635) - copilot-swe-agent[bot]
#### Documentation
- (**02**) research phase domain - (81b1b74) - brianh
- (**02**) add phase plan - (7bd1a7d) - brianh
- (**02**) discuss context - (ae7b8c7) - brianh
- (**03**) discuss context - (3592081) - brianh
- (**13**) add gap-closure plan for rustledger proxy MCP callable surface - (ea8424e) - brianh
- (**13**) add verification report with gap findings - (89f6e11) - brianh
- (**13**) add context research and validation artifacts - (aa12c52) - brianh
- (**13**) create phase plans - (3cd7bb6) - brianh
- (**13-01**) complete mcp boundary proxy surface plan - (a8a709f) - brianh
- (**13-02**) complete mcp-only doc verification plan - (dfa4b63) - brianh
- (**13-02**) publish MCP-only runbook and validation mapping - (787c501) - brianh
- (**13-03**) complete rustledger proxy callable-surface plan - (69b4dfe) - brianh
- (**13-03**) align runbook and validation to rustledger proxy transport - (ebbe75c) - brianh
- (**14**) create ontology persistence phase plans - (eca1808) - brianh
- (**14-01**) complete ontology persistence and query surface plan - (ee80513) - brianh
- (**14-02**) complete ontology mcp transport plan - (0c816f4) - brianh
- (**14-02**) align ontology MCP runbook and validation map - (9dad54e) - brianh
- (**15**) add reconciliation and commit guardrail plans - (55e3bfe) - brianh
- (**15-02**) complete reconciliation-and-commit-guardrails plan - (2468594) - brianh
- (**15-02**) align reconciliation transport runbook and validation map - (8eba986) - brianh
- (**16**) create autonomous HSM phase context and execution plans - (9106bb5) - brianh
- (**16-03**) complete moku-hsm-deterministic-status-and-resume plan - (66ad731) - brianh
- (**17**) create phase plan - (f6a86a5) - brianh
- (**17-01**) complete event domain foundation plan - (b4b410e) - brianh
- (**17-02**) complete deterministic replay plan - (8df1735) - brianh
- (**17-03**) complete MCP event query plan - (2e051d2) - brianh
- (**18**) plan tax assist evidence-chain interfaces - (f80d1f6) - brianh
- (**18-03**) complete tax-assist evidence-chain interfaces plans - (115b5d1) - brianh
- (**agents**) require session lesson capture for posterity - (ad47077) - brianh
- (**milestone**) archive v1.0 roadmap and requirements - (482983d) - brianh
- (**milestone**) add v1.0 audit report - (01ee29d) - brianh
- (**phase-13**) evolve PROJECT.md after phase completion - (4eab3d8) - brianh
- (**phase-13**) complete phase execution - (8939d74) - brianh
- (**roadmap**) add gap closure phases 13-18 - (b347a67) - brianh
- (**state**) align handoff to phase 14 after phase 13 completion - (bd72945) - brianh
- update versioning section to reference justfile release recipe - (95c0652) - Claude Sonnet (coordinator)
- create milestone v1.2 roadmap - (82c024e) - brianh
- define milestone v1.2 requirements - (2e65b2e) - brianh
- start milestone v1.2 Claude Connector Interop - (5960e4f) - brianh
- capture todo - Add Claude Cowork MCP install matrix and CI gate - (c33cb11) - brianh
- add concrete mcp usage examples to agents guide - (9831bc4) - brianh
- add agent purpose/capability guide and README reference - (a1b1bb9) - brianh
- start milestone v1.1 fdkms integrity - (9064ba6) - brianh
- add backlog item 999.1 — CI + release automation hardening - (84de370) - brianh
- create roadmap (6 phases) - (6e9bd7e) - brianh
- define v1 requirements - (4ff43fb) - brianh
- add project research - (c4619ba) - brianh
- initialize project - (4677e99) - brianh
#### Tests
- (**13-01**) add failing MCP adapter contract tests - (546c56e) - brianh
- (**13-02**) add failing MCP stdio DOC requirement tests - (d95d617) - brianh
- (**13-03**) add failing rustledger proxy transport coverage - (1aa7970) - brianh
- (**14-01**) add failing ONTO-01/02 ontology contract tests - (ce24318) - brianh
- (**14-02**) add failing ontology MCP transport tests - (ae0098a) - brianh
- (**15-01**) add failing reconciliation guardrail contracts - (163ebd9) - brianh
- (**15-02**) add failing reconciliation MCP transport contracts - (7f75625) - brianh
- (**16-01**) add failing hsm lifecycle and guard contracts - (fa2178b) - brianh
- (**16-02**) add failing checkpoint and resume contracts - (181b02c) - brianh
- (**16-03**) add failing hsm mcp transport e2e contracts - (8001964) - brianh
- (**17-01**) add failing contracts for append-only lifecycle events - (c6ff2ef) - brianh
- (**17-02**) add failing deterministic replay contracts - (30f3f20) - brianh
- (**17-03**) add failing MCP event history e2e contracts - (ec4a80e) - brianh
- (**18-01**) add failing tax-assist contracts for TAXA-01 and TAXA-03 - (ca1f8b6) - brianh
- (**18-02**) add failing evidence-chain contract for TAXA-02 - (bde256f) - brianh
- (**18-03**) add failing MCP e2e contracts for tax-assist tools - (baac788) - brianh
- (**e2e**) expand bdd coverage for ingest tool behaviors - (7efd6ac) - brianh
- (**turbo-mcp**) add phase6 failing tests for MCP exposure gaps (P0/P1/P2) - (d2b7354) - Claude Sonnet (coordinator)
- add sample statement fixtures for e2e regression - (5f40bf6) - brianh
#### Continuous Integration
- disable MCP Registry publish (requires direct write access) - (abf6b60) - Claude Sonnet (coordinator)
- add MCPB publish gate after tests - (676c58b) - Claude Sonnet (coordinator)
- add clippy sarif upload and podman publish-on-main - (588a231) - brianh
- add publish workflow for ghcr crates and pypi - (e6bb524) - brianh
#### Refactoring
- rename turbo-mcp to ledgerr-mcp - (4b484cd) - Claude Sonnet (coordinator)
#### Miscellaneous Chores
- (**16-03**) publish hsm validation map and operator verification docs - (eebdcda) - brianh
- (**17**) capture MCP row-key normalization guidance in AGENTS - (1323e94) - brianh
- (**17-03**) publish event MCP validation and verification docs - (2a7f381) - brianh
- (**18-03**) publish tax-assist runbook and phase validation map - (54f8012) - brianh
- (**dev**) add dotenv ignore and secret setup recipe fixes - (7a029d2) - brianh
- (**docs**) add plugin usage validation flow and mcp cli demos - (d522338) - brianh
- (**planning**) persist autonomous closeout state - (b9ba437) - brianh
- (**release**) add changelog for v1.1.0 - (acb4d27) - brianh
- (**v1.1**) archive completed milestone phases - (cf9b3c2) - brianh
- (**version**) 0.1.0 - (b64a975) - github-actions[bot]
- (**version**) 0.1.0 - (dcd8f0f) - github-actions[bot]
- (**version**) 0.1.0 - (8e28f71) - github-actions[bot]
- verify release badge - (44dcc54) - Claude Sonnet (coordinator)
- test release workflow trigger - (c22303f) - Claude Sonnet (coordinator)
- archive phase directories from completed milestones - (230159f) - brianh
- archive v1.0 milestone - (571ab76) - brianh
- ignore local PRD and refine phase 2 plan - (023da6b) - brianh
- add project config - (f498561) - brianh

- - -

## 0.1.0 - 2026-04-12
#### Features
- (**13-01**) add stdio MCP adapter and proxy boundary - (c95c675) - brianh
- (**13-02**) implement MCP stdio ingest e2e harness and replay checks - (cf28f37) - brianh
- (**13-03**) wire rustledger proxy ingest rows over MCP tools/call - (fd96420) - brianh
- (**14-01**) add service-owned ontology query tool wrappers - (587cbb4) - brianh
- (**14-01**) implement ontology store with deterministic persistence - (c8a8dd1) - brianh
- (**14-02**) add ontology MCP query/export transport handlers - (d5f6ca0) - brianh
- (**15-01**) enforce deterministic reconciliation commit guardrails - (1f7b0ef) - brianh
- (**15-01**) add reconciliation stage contracts and service APIs - (3518079) - brianh
- (**15-02**) expose reconciliation stage tools over MCP transport - (e63959a) - brianh
- (**16-01**) implement deterministic hsm transition and status APIs - (832e1f7) - brianh
- (**16-01**) add hsm domain contracts and service stubs - (b87ded0) - brianh
- (**16-02**) wire deterministic checkpoint persistence and resume - (35d8609) - brianh
- (**16-02**) add hsm checkpoint and resume contracts - (db30cc4) - brianh
- (**16-03**) expose hsm transition status resume over mcp - (674c433) - brianh
- (**17-01**) append deterministic lifecycle events from service actions - (8e56aa5) - brianh
- (**17-01**) add append-only lifecycle event store contracts - (9c2b6fd) - brianh
- (**17-02**) wire lifecycle replay service API - (93d1918) - brianh
- (**17-02**) add deterministic replay projector contracts - (17bf7c2) - brianh
- (**17-03**) wire MCP event replay and history tools - (b02a324) - brianh
- (**18-01**) implement deterministic tax-assist and ambiguity composition - (75b6b97) - brianh
- (**18-01**) add tax-assist service contracts and tool stubs - (749adf0) - brianh
- (**18-02**) implement deterministic evidence-chain retrieval - (8bc24e1) - brianh
- (**18-03**) expose tax-assist interfaces over MCP transport - (621bbea) - brianh
- (**mcp**) expose P0/P1/P2 tool gap handlers as wired MCP tools - (0482f67) - Claude Sonnet (coordinator)
- (**mcp**) expose account listing and raw-context tools - (79b0e5f) - brianh
- (**phase-03**) implement runtime rhai classification and review queue - (accd407) - brianh
- (**phase-04**) add audited classification mutations with invariants - (d833758) - brianh
- (**phase-05**) add cpa workbook export and schedule summaries - (1da735e) - brianh
- (**phase-06**) add ci release automation and mvp e2e flow - (49b5a18) - brianh
- (**phase-1**) scaffold contracts, bootstrap, and turbo MCP interface - (1699ce8) - brianh
- (**phase-2**) complete deterministic ingest pipeline and verification - (ebf0fd5) - brianh
- (**phase-2**) add ingest_pdf and get_raw_context MCP contracts - (3b757a8) - brianh
- (**phase-2**) pivot ingest to rustledger-compatible beancount journals - (f69d7bd) - brianh
- (**phase-2**) add deterministic ingest primitives with idempotency tests - (11b4b9f) - brianh
- (**test**) add outcome-driven mcp flow runner behind just test - (f50d916) - brianh
- add workflow_dispatch for manual release trigger - (6f1ab50) - Claude Sonnet (coordinator)
- expand Cowork marketplace runtime and packaging guidance - (16fc219) - brianh
- add Claude Cowork plugin marketplace distribution artifacts - (628bd9d) - brianh
#### Bug Fixes
- (**13-01**) harden deterministic status and MCP error mapping - (3f307c7) - brianh
- (**ci**) use valid rust image and pin ledger-core publish version - (e9b1140) - brianh
- (**mcp**) replace absolute paths in docs, fix service-only list, add path traversal guard to get_raw_context - (eac38fb) - copilot-swe-agent[bot]
- simplify release workflow to use cocogitto-action v4 with bundled cog - (b3b927e) - Claude Sonnet (coordinator)
- fetch all tags in release workflow and fix cog.toml config - (942d8f7) - Claude Sonnet (coordinator)
- remove deprecated pre field from cog.toml - (bcfbdd1) - Claude Sonnet (coordinator)
- allow workflow_dispatch trigger in release condition - (54243dc) - Claude Sonnet (coordinator)
- simplify release workflow trigger condition - (8349560) - Claude Sonnet (coordinator)
- use proper mcpb manifest v0.3 schema with server config - (d923aa5) - Claude Sonnet (coordinator)
- use positional args for mcpb pack (directory output) - (1000c18) - Claude Sonnet (coordinator)
- use -o flag for mcpb pack output path - (0ab1788) - Claude Sonnet (coordinator)
- fix mcpb pack output path to stay in bundle dir - (ac8b823) - Claude Sonnet (coordinator)
- add contents:write permission for release creation - (00bbf78) - Claude Sonnet (coordinator)
- update package name from turbo-mcp to ledgerr-mcp in e2e script - (d800497) - Claude Sonnet (coordinator)
- align marketplace and plugin manifests with Cowork validation - (13d10d5) - brianh
- remove Zone.Identifier files, add gitignore rule, reconcile STATE.md - (6efa654) - copilot-swe-agent[bot]
- address all PR review feedback (version alignment, path safety, classify_ingested, sheets count, active year, release loop guard) - (47ea635) - copilot-swe-agent[bot]
#### Documentation
- (**02**) research phase domain - (81b1b74) - brianh
- (**02**) add phase plan - (7bd1a7d) - brianh
- (**02**) discuss context - (ae7b8c7) - brianh
- (**03**) discuss context - (3592081) - brianh
- (**13**) add gap-closure plan for rustledger proxy MCP callable surface - (ea8424e) - brianh
- (**13**) add verification report with gap findings - (89f6e11) - brianh
- (**13**) add context research and validation artifacts - (aa12c52) - brianh
- (**13**) create phase plans - (3cd7bb6) - brianh
- (**13-01**) complete mcp boundary proxy surface plan - (a8a709f) - brianh
- (**13-02**) complete mcp-only doc verification plan - (dfa4b63) - brianh
- (**13-02**) publish MCP-only runbook and validation mapping - (787c501) - brianh
- (**13-03**) complete rustledger proxy callable-surface plan - (69b4dfe) - brianh
- (**13-03**) align runbook and validation to rustledger proxy transport - (ebbe75c) - brianh
- (**14**) create ontology persistence phase plans - (eca1808) - brianh
- (**14-01**) complete ontology persistence and query surface plan - (ee80513) - brianh
- (**14-02**) complete ontology mcp transport plan - (0c816f4) - brianh
- (**14-02**) align ontology MCP runbook and validation map - (9dad54e) - brianh
- (**15**) add reconciliation and commit guardrail plans - (55e3bfe) - brianh
- (**15-02**) complete reconciliation-and-commit-guardrails plan - (2468594) - brianh
- (**15-02**) align reconciliation transport runbook and validation map - (8eba986) - brianh
- (**16**) create autonomous HSM phase context and execution plans - (9106bb5) - brianh
- (**16-03**) complete moku-hsm-deterministic-status-and-resume plan - (66ad731) - brianh
- (**17**) create phase plan - (f6a86a5) - brianh
- (**17-01**) complete event domain foundation plan - (b4b410e) - brianh
- (**17-02**) complete deterministic replay plan - (8df1735) - brianh
- (**17-03**) complete MCP event query plan - (2e051d2) - brianh
- (**18**) plan tax assist evidence-chain interfaces - (f80d1f6) - brianh
- (**18-03**) complete tax-assist evidence-chain interfaces plans - (115b5d1) - brianh
- (**agents**) require session lesson capture for posterity - (ad47077) - brianh
- (**milestone**) archive v1.0 roadmap and requirements - (482983d) - brianh
- (**milestone**) add v1.0 audit report - (01ee29d) - brianh
- (**phase-13**) evolve PROJECT.md after phase completion - (4eab3d8) - brianh
- (**phase-13**) complete phase execution - (8939d74) - brianh
- (**roadmap**) add gap closure phases 13-18 - (b347a67) - brianh
- (**state**) align handoff to phase 14 after phase 13 completion - (bd72945) - brianh
- create milestone v1.2 roadmap - (82c024e) - brianh
- define milestone v1.2 requirements - (2e65b2e) - brianh
- start milestone v1.2 Claude Connector Interop - (5960e4f) - brianh
- capture todo - Add Claude Cowork MCP install matrix and CI gate - (c33cb11) - brianh
- add concrete mcp usage examples to agents guide - (9831bc4) - brianh
- add agent purpose/capability guide and README reference - (a1b1bb9) - brianh
- start milestone v1.1 fdkms integrity - (9064ba6) - brianh
- add backlog item 999.1 — CI + release automation hardening - (84de370) - brianh
- create roadmap (6 phases) - (6e9bd7e) - brianh
- define v1 requirements - (4ff43fb) - brianh
- add project research - (c4619ba) - brianh
- initialize project - (4677e99) - brianh
#### Tests
- (**13-01**) add failing MCP adapter contract tests - (546c56e) - brianh
- (**13-02**) add failing MCP stdio DOC requirement tests - (d95d617) - brianh
- (**13-03**) add failing rustledger proxy transport coverage - (1aa7970) - brianh
- (**14-01**) add failing ONTO-01/02 ontology contract tests - (ce24318) - brianh
- (**14-02**) add failing ontology MCP transport tests - (ae0098a) - brianh
- (**15-01**) add failing reconciliation guardrail contracts - (163ebd9) - brianh
- (**15-02**) add failing reconciliation MCP transport contracts - (7f75625) - brianh
- (**16-01**) add failing hsm lifecycle and guard contracts - (fa2178b) - brianh
- (**16-02**) add failing checkpoint and resume contracts - (181b02c) - brianh
- (**16-03**) add failing hsm mcp transport e2e contracts - (8001964) - brianh
- (**17-01**) add failing contracts for append-only lifecycle events - (c6ff2ef) - brianh
- (**17-02**) add failing deterministic replay contracts - (30f3f20) - brianh
- (**17-03**) add failing MCP event history e2e contracts - (ec4a80e) - brianh
- (**18-01**) add failing tax-assist contracts for TAXA-01 and TAXA-03 - (ca1f8b6) - brianh
- (**18-02**) add failing evidence-chain contract for TAXA-02 - (bde256f) - brianh
- (**18-03**) add failing MCP e2e contracts for tax-assist tools - (baac788) - brianh
- (**e2e**) expand bdd coverage for ingest tool behaviors - (7efd6ac) - brianh
- (**turbo-mcp**) add phase6 failing tests for MCP exposure gaps (P0/P1/P2) - (d2b7354) - Claude Sonnet (coordinator)
- add sample statement fixtures for e2e regression - (5f40bf6) - brianh
#### Continuous Integration
- disable MCP Registry publish (requires direct write access) - (abf6b60) - Claude Sonnet (coordinator)
- add MCPB publish gate after tests - (676c58b) - Claude Sonnet (coordinator)
- add clippy sarif upload and podman publish-on-main - (588a231) - brianh
- add publish workflow for ghcr crates and pypi - (e6bb524) - brianh
#### Refactoring
- rename turbo-mcp to ledgerr-mcp - (4b484cd) - Claude Sonnet (coordinator)
#### Miscellaneous Chores
- (**16-03**) publish hsm validation map and operator verification docs - (eebdcda) - brianh
- (**17**) capture MCP row-key normalization guidance in AGENTS - (1323e94) - brianh
- (**17-03**) publish event MCP validation and verification docs - (2a7f381) - brianh
- (**18-03**) publish tax-assist runbook and phase validation map - (54f8012) - brianh
- (**dev**) add dotenv ignore and secret setup recipe fixes - (7a029d2) - brianh
- (**docs**) add plugin usage validation flow and mcp cli demos - (d522338) - brianh
- (**planning**) persist autonomous closeout state - (b9ba437) - brianh
- (**release**) add changelog for v1.1.0 - (acb4d27) - brianh
- (**v1.1**) archive completed milestone phases - (cf9b3c2) - brianh
- (**version**) 0.1.0 - (dcd8f0f) - github-actions[bot]
- (**version**) 0.1.0 - (8e28f71) - github-actions[bot]
- verify release badge - (44dcc54) - Claude Sonnet (coordinator)
- test release workflow trigger - (c22303f) - Claude Sonnet (coordinator)
- archive phase directories from completed milestones - (230159f) - brianh
- archive v1.0 milestone - (571ab76) - brianh
- ignore local PRD and refine phase 2 plan - (023da6b) - brianh
- add project config - (f498561) - brianh

- - -

## 0.1.0 - 2026-04-12
#### Features
- (**13-01**) add stdio MCP adapter and proxy boundary - (c95c675) - brianh
- (**13-02**) implement MCP stdio ingest e2e harness and replay checks - (cf28f37) - brianh
- (**13-03**) wire rustledger proxy ingest rows over MCP tools/call - (fd96420) - brianh
- (**14-01**) add service-owned ontology query tool wrappers - (587cbb4) - brianh
- (**14-01**) implement ontology store with deterministic persistence - (c8a8dd1) - brianh
- (**14-02**) add ontology MCP query/export transport handlers - (d5f6ca0) - brianh
- (**15-01**) enforce deterministic reconciliation commit guardrails - (1f7b0ef) - brianh
- (**15-01**) add reconciliation stage contracts and service APIs - (3518079) - brianh
- (**15-02**) expose reconciliation stage tools over MCP transport - (e63959a) - brianh
- (**16-01**) implement deterministic hsm transition and status APIs - (832e1f7) - brianh
- (**16-01**) add hsm domain contracts and service stubs - (b87ded0) - brianh
- (**16-02**) wire deterministic checkpoint persistence and resume - (35d8609) - brianh
- (**16-02**) add hsm checkpoint and resume contracts - (db30cc4) - brianh
- (**16-03**) expose hsm transition status resume over mcp - (674c433) - brianh
- (**17-01**) append deterministic lifecycle events from service actions - (8e56aa5) - brianh
- (**17-01**) add append-only lifecycle event store contracts - (9c2b6fd) - brianh
- (**17-02**) wire lifecycle replay service API - (93d1918) - brianh
- (**17-02**) add deterministic replay projector contracts - (17bf7c2) - brianh
- (**17-03**) wire MCP event replay and history tools - (b02a324) - brianh
- (**18-01**) implement deterministic tax-assist and ambiguity composition - (75b6b97) - brianh
- (**18-01**) add tax-assist service contracts and tool stubs - (749adf0) - brianh
- (**18-02**) implement deterministic evidence-chain retrieval - (8bc24e1) - brianh
- (**18-03**) expose tax-assist interfaces over MCP transport - (621bbea) - brianh
- (**mcp**) expose P0/P1/P2 tool gap handlers as wired MCP tools - (0482f67) - Claude Sonnet (coordinator)
- (**mcp**) expose account listing and raw-context tools - (79b0e5f) - brianh
- (**phase-03**) implement runtime rhai classification and review queue - (accd407) - brianh
- (**phase-04**) add audited classification mutations with invariants - (d833758) - brianh
- (**phase-05**) add cpa workbook export and schedule summaries - (1da735e) - brianh
- (**phase-06**) add ci release automation and mvp e2e flow - (49b5a18) - brianh
- (**phase-1**) scaffold contracts, bootstrap, and turbo MCP interface - (1699ce8) - brianh
- (**phase-2**) complete deterministic ingest pipeline and verification - (ebf0fd5) - brianh
- (**phase-2**) add ingest_pdf and get_raw_context MCP contracts - (3b757a8) - brianh
- (**phase-2**) pivot ingest to rustledger-compatible beancount journals - (f69d7bd) - brianh
- (**phase-2**) add deterministic ingest primitives with idempotency tests - (11b4b9f) - brianh
- (**test**) add outcome-driven mcp flow runner behind just test - (f50d916) - brianh
- add workflow_dispatch for manual release trigger - (6f1ab50) - Claude Sonnet (coordinator)
- expand Cowork marketplace runtime and packaging guidance - (16fc219) - brianh
- add Claude Cowork plugin marketplace distribution artifacts - (628bd9d) - brianh
#### Bug Fixes
- (**13-01**) harden deterministic status and MCP error mapping - (3f307c7) - brianh
- (**ci**) use valid rust image and pin ledger-core publish version - (e9b1140) - brianh
- (**mcp**) replace absolute paths in docs, fix service-only list, add path traversal guard to get_raw_context - (eac38fb) - copilot-swe-agent[bot]
- simplify release workflow to use cocogitto-action v4 with bundled cog - (b3b927e) - Claude Sonnet (coordinator)
- fetch all tags in release workflow and fix cog.toml config - (942d8f7) - Claude Sonnet (coordinator)
- remove deprecated pre field from cog.toml - (bcfbdd1) - Claude Sonnet (coordinator)
- allow workflow_dispatch trigger in release condition - (54243dc) - Claude Sonnet (coordinator)
- simplify release workflow trigger condition - (8349560) - Claude Sonnet (coordinator)
- use proper mcpb manifest v0.3 schema with server config - (d923aa5) - Claude Sonnet (coordinator)
- use positional args for mcpb pack (directory output) - (1000c18) - Claude Sonnet (coordinator)
- use -o flag for mcpb pack output path - (0ab1788) - Claude Sonnet (coordinator)
- fix mcpb pack output path to stay in bundle dir - (ac8b823) - Claude Sonnet (coordinator)
- add contents:write permission for release creation - (00bbf78) - Claude Sonnet (coordinator)
- update package name from turbo-mcp to ledgerr-mcp in e2e script - (d800497) - Claude Sonnet (coordinator)
- align marketplace and plugin manifests with Cowork validation - (13d10d5) - brianh
- remove Zone.Identifier files, add gitignore rule, reconcile STATE.md - (6efa654) - copilot-swe-agent[bot]
- address all PR review feedback (version alignment, path safety, classify_ingested, sheets count, active year, release loop guard) - (47ea635) - copilot-swe-agent[bot]
#### Documentation
- (**02**) research phase domain - (81b1b74) - brianh
- (**02**) add phase plan - (7bd1a7d) - brianh
- (**02**) discuss context - (ae7b8c7) - brianh
- (**03**) discuss context - (3592081) - brianh
- (**13**) add gap-closure plan for rustledger proxy MCP callable surface - (ea8424e) - brianh
- (**13**) add verification report with gap findings - (89f6e11) - brianh
- (**13**) add context research and validation artifacts - (aa12c52) - brianh
- (**13**) create phase plans - (3cd7bb6) - brianh
- (**13-01**) complete mcp boundary proxy surface plan - (a8a709f) - brianh
- (**13-02**) complete mcp-only doc verification plan - (dfa4b63) - brianh
- (**13-02**) publish MCP-only runbook and validation mapping - (787c501) - brianh
- (**13-03**) complete rustledger proxy callable-surface plan - (69b4dfe) - brianh
- (**13-03**) align runbook and validation to rustledger proxy transport - (ebbe75c) - brianh
- (**14**) create ontology persistence phase plans - (eca1808) - brianh
- (**14-01**) complete ontology persistence and query surface plan - (ee80513) - brianh
- (**14-02**) complete ontology mcp transport plan - (0c816f4) - brianh
- (**14-02**) align ontology MCP runbook and validation map - (9dad54e) - brianh
- (**15**) add reconciliation and commit guardrail plans - (55e3bfe) - brianh
- (**15-02**) complete reconciliation-and-commit-guardrails plan - (2468594) - brianh
- (**15-02**) align reconciliation transport runbook and validation map - (8eba986) - brianh
- (**16**) create autonomous HSM phase context and execution plans - (9106bb5) - brianh
- (**16-03**) complete moku-hsm-deterministic-status-and-resume plan - (66ad731) - brianh
- (**17**) create phase plan - (f6a86a5) - brianh
- (**17-01**) complete event domain foundation plan - (b4b410e) - brianh
- (**17-02**) complete deterministic replay plan - (8df1735) - brianh
- (**17-03**) complete MCP event query plan - (2e051d2) - brianh
- (**18**) plan tax assist evidence-chain interfaces - (f80d1f6) - brianh
- (**18-03**) complete tax-assist evidence-chain interfaces plans - (115b5d1) - brianh
- (**agents**) require session lesson capture for posterity - (ad47077) - brianh
- (**milestone**) archive v1.0 roadmap and requirements - (482983d) - brianh
- (**milestone**) add v1.0 audit report - (01ee29d) - brianh
- (**phase-13**) evolve PROJECT.md after phase completion - (4eab3d8) - brianh
- (**phase-13**) complete phase execution - (8939d74) - brianh
- (**roadmap**) add gap closure phases 13-18 - (b347a67) - brianh
- (**state**) align handoff to phase 14 after phase 13 completion - (bd72945) - brianh
- create milestone v1.2 roadmap - (82c024e) - brianh
- define milestone v1.2 requirements - (2e65b2e) - brianh
- start milestone v1.2 Claude Connector Interop - (5960e4f) - brianh
- capture todo - Add Claude Cowork MCP install matrix and CI gate - (c33cb11) - brianh
- add concrete mcp usage examples to agents guide - (9831bc4) - brianh
- add agent purpose/capability guide and README reference - (a1b1bb9) - brianh
- start milestone v1.1 fdkms integrity - (9064ba6) - brianh
- add backlog item 999.1 — CI + release automation hardening - (84de370) - brianh
- create roadmap (6 phases) - (6e9bd7e) - brianh
- define v1 requirements - (4ff43fb) - brianh
- add project research - (c4619ba) - brianh
- initialize project - (4677e99) - brianh
#### Tests
- (**13-01**) add failing MCP adapter contract tests - (546c56e) - brianh
- (**13-02**) add failing MCP stdio DOC requirement tests - (d95d617) - brianh
- (**13-03**) add failing rustledger proxy transport coverage - (1aa7970) - brianh
- (**14-01**) add failing ONTO-01/02 ontology contract tests - (ce24318) - brianh
- (**14-02**) add failing ontology MCP transport tests - (ae0098a) - brianh
- (**15-01**) add failing reconciliation guardrail contracts - (163ebd9) - brianh
- (**15-02**) add failing reconciliation MCP transport contracts - (7f75625) - brianh
- (**16-01**) add failing hsm lifecycle and guard contracts - (fa2178b) - brianh
- (**16-02**) add failing checkpoint and resume contracts - (181b02c) - brianh
- (**16-03**) add failing hsm mcp transport e2e contracts - (8001964) - brianh
- (**17-01**) add failing contracts for append-only lifecycle events - (c6ff2ef) - brianh
- (**17-02**) add failing deterministic replay contracts - (30f3f20) - brianh
- (**17-03**) add failing MCP event history e2e contracts - (ec4a80e) - brianh
- (**18-01**) add failing tax-assist contracts for TAXA-01 and TAXA-03 - (ca1f8b6) - brianh
- (**18-02**) add failing evidence-chain contract for TAXA-02 - (bde256f) - brianh
- (**18-03**) add failing MCP e2e contracts for tax-assist tools - (baac788) - brianh
- (**e2e**) expand bdd coverage for ingest tool behaviors - (7efd6ac) - brianh
- (**turbo-mcp**) add phase6 failing tests for MCP exposure gaps (P0/P1/P2) - (d2b7354) - Claude Sonnet (coordinator)
- add sample statement fixtures for e2e regression - (5f40bf6) - brianh
#### Continuous Integration
- disable MCP Registry publish (requires direct write access) - (abf6b60) - Claude Sonnet (coordinator)
- add MCPB publish gate after tests - (676c58b) - Claude Sonnet (coordinator)
- add clippy sarif upload and podman publish-on-main - (588a231) - brianh
- add publish workflow for ghcr crates and pypi - (e6bb524) - brianh
#### Refactoring
- rename turbo-mcp to ledgerr-mcp - (4b484cd) - Claude Sonnet (coordinator)
#### Miscellaneous Chores
- (**16-03**) publish hsm validation map and operator verification docs - (eebdcda) - brianh
- (**17**) capture MCP row-key normalization guidance in AGENTS - (1323e94) - brianh
- (**17-03**) publish event MCP validation and verification docs - (2a7f381) - brianh
- (**18-03**) publish tax-assist runbook and phase validation map - (54f8012) - brianh
- (**dev**) add dotenv ignore and secret setup recipe fixes - (7a029d2) - brianh
- (**docs**) add plugin usage validation flow and mcp cli demos - (d522338) - brianh
- (**planning**) persist autonomous closeout state - (b9ba437) - brianh
- (**release**) add changelog for v1.1.0 - (acb4d27) - brianh
- (**v1.1**) archive completed milestone phases - (cf9b3c2) - brianh
- (**version**) 0.1.0 - (8e28f71) - github-actions[bot]
- test release workflow trigger - (c22303f) - Claude Sonnet (coordinator)
- archive phase directories from completed milestones - (230159f) - brianh
- archive v1.0 milestone - (571ab76) - brianh
- ignore local PRD and refine phase 2 plan - (023da6b) - brianh
- add project config - (f498561) - brianh

- - -

## 0.1.0 - 2026-04-12
#### Features
- (**13-01**) add stdio MCP adapter and proxy boundary - (c95c675) - brianh
- (**13-02**) implement MCP stdio ingest e2e harness and replay checks - (cf28f37) - brianh
- (**13-03**) wire rustledger proxy ingest rows over MCP tools/call - (fd96420) - brianh
- (**14-01**) add service-owned ontology query tool wrappers - (587cbb4) - brianh
- (**14-01**) implement ontology store with deterministic persistence - (c8a8dd1) - brianh
- (**14-02**) add ontology MCP query/export transport handlers - (d5f6ca0) - brianh
- (**15-01**) enforce deterministic reconciliation commit guardrails - (1f7b0ef) - brianh
- (**15-01**) add reconciliation stage contracts and service APIs - (3518079) - brianh
- (**15-02**) expose reconciliation stage tools over MCP transport - (e63959a) - brianh
- (**16-01**) implement deterministic hsm transition and status APIs - (832e1f7) - brianh
- (**16-01**) add hsm domain contracts and service stubs - (b87ded0) - brianh
- (**16-02**) wire deterministic checkpoint persistence and resume - (35d8609) - brianh
- (**16-02**) add hsm checkpoint and resume contracts - (db30cc4) - brianh
- (**16-03**) expose hsm transition status resume over mcp - (674c433) - brianh
- (**17-01**) append deterministic lifecycle events from service actions - (8e56aa5) - brianh
- (**17-01**) add append-only lifecycle event store contracts - (9c2b6fd) - brianh
- (**17-02**) wire lifecycle replay service API - (93d1918) - brianh
- (**17-02**) add deterministic replay projector contracts - (17bf7c2) - brianh
- (**17-03**) wire MCP event replay and history tools - (b02a324) - brianh
- (**18-01**) implement deterministic tax-assist and ambiguity composition - (75b6b97) - brianh
- (**18-01**) add tax-assist service contracts and tool stubs - (749adf0) - brianh
- (**18-02**) implement deterministic evidence-chain retrieval - (8bc24e1) - brianh
- (**18-03**) expose tax-assist interfaces over MCP transport - (621bbea) - brianh
- (**mcp**) expose P0/P1/P2 tool gap handlers as wired MCP tools - (0482f67) - Claude Sonnet (coordinator)
- (**mcp**) expose account listing and raw-context tools - (79b0e5f) - brianh
- (**phase-03**) implement runtime rhai classification and review queue - (accd407) - brianh
- (**phase-04**) add audited classification mutations with invariants - (d833758) - brianh
- (**phase-05**) add cpa workbook export and schedule summaries - (1da735e) - brianh
- (**phase-06**) add ci release automation and mvp e2e flow - (49b5a18) - brianh
- (**phase-1**) scaffold contracts, bootstrap, and turbo MCP interface - (1699ce8) - brianh
- (**phase-2**) complete deterministic ingest pipeline and verification - (ebf0fd5) - brianh
- (**phase-2**) add ingest_pdf and get_raw_context MCP contracts - (3b757a8) - brianh
- (**phase-2**) pivot ingest to rustledger-compatible beancount journals - (f69d7bd) - brianh
- (**phase-2**) add deterministic ingest primitives with idempotency tests - (11b4b9f) - brianh
- (**test**) add outcome-driven mcp flow runner behind just test - (f50d916) - brianh
- add workflow_dispatch for manual release trigger - (6f1ab50) - Claude Sonnet (coordinator)
- expand Cowork marketplace runtime and packaging guidance - (16fc219) - brianh
- add Claude Cowork plugin marketplace distribution artifacts - (628bd9d) - brianh
#### Bug Fixes
- (**13-01**) harden deterministic status and MCP error mapping - (3f307c7) - brianh
- (**ci**) use valid rust image and pin ledger-core publish version - (e9b1140) - brianh
- (**mcp**) replace absolute paths in docs, fix service-only list, add path traversal guard to get_raw_context - (eac38fb) - copilot-swe-agent[bot]
- simplify release workflow to use cocogitto-action v4 with bundled cog - (b3b927e) - Claude Sonnet (coordinator)
- fetch all tags in release workflow and fix cog.toml config - (942d8f7) - Claude Sonnet (coordinator)
- remove deprecated pre field from cog.toml - (bcfbdd1) - Claude Sonnet (coordinator)
- allow workflow_dispatch trigger in release condition - (54243dc) - Claude Sonnet (coordinator)
- simplify release workflow trigger condition - (8349560) - Claude Sonnet (coordinator)
- use proper mcpb manifest v0.3 schema with server config - (d923aa5) - Claude Sonnet (coordinator)
- use positional args for mcpb pack (directory output) - (1000c18) - Claude Sonnet (coordinator)
- use -o flag for mcpb pack output path - (0ab1788) - Claude Sonnet (coordinator)
- fix mcpb pack output path to stay in bundle dir - (ac8b823) - Claude Sonnet (coordinator)
- add contents:write permission for release creation - (00bbf78) - Claude Sonnet (coordinator)
- update package name from turbo-mcp to ledgerr-mcp in e2e script - (d800497) - Claude Sonnet (coordinator)
- align marketplace and plugin manifests with Cowork validation - (13d10d5) - brianh
- remove Zone.Identifier files, add gitignore rule, reconcile STATE.md - (6efa654) - copilot-swe-agent[bot]
- address all PR review feedback (version alignment, path safety, classify_ingested, sheets count, active year, release loop guard) - (47ea635) - copilot-swe-agent[bot]
#### Documentation
- (**02**) research phase domain - (81b1b74) - brianh
- (**02**) add phase plan - (7bd1a7d) - brianh
- (**02**) discuss context - (ae7b8c7) - brianh
- (**03**) discuss context - (3592081) - brianh
- (**13**) add gap-closure plan for rustledger proxy MCP callable surface - (ea8424e) - brianh
- (**13**) add verification report with gap findings - (89f6e11) - brianh
- (**13**) add context research and validation artifacts - (aa12c52) - brianh
- (**13**) create phase plans - (3cd7bb6) - brianh
- (**13-01**) complete mcp boundary proxy surface plan - (a8a709f) - brianh
- (**13-02**) complete mcp-only doc verification plan - (dfa4b63) - brianh
- (**13-02**) publish MCP-only runbook and validation mapping - (787c501) - brianh
- (**13-03**) complete rustledger proxy callable-surface plan - (69b4dfe) - brianh
- (**13-03**) align runbook and validation to rustledger proxy transport - (ebbe75c) - brianh
- (**14**) create ontology persistence phase plans - (eca1808) - brianh
- (**14-01**) complete ontology persistence and query surface plan - (ee80513) - brianh
- (**14-02**) complete ontology mcp transport plan - (0c816f4) - brianh
- (**14-02**) align ontology MCP runbook and validation map - (9dad54e) - brianh
- (**15**) add reconciliation and commit guardrail plans - (55e3bfe) - brianh
- (**15-02**) complete reconciliation-and-commit-guardrails plan - (2468594) - brianh
- (**15-02**) align reconciliation transport runbook and validation map - (8eba986) - brianh
- (**16**) create autonomous HSM phase context and execution plans - (9106bb5) - brianh
- (**16-03**) complete moku-hsm-deterministic-status-and-resume plan - (66ad731) - brianh
- (**17**) create phase plan - (f6a86a5) - brianh
- (**17-01**) complete event domain foundation plan - (b4b410e) - brianh
- (**17-02**) complete deterministic replay plan - (8df1735) - brianh
- (**17-03**) complete MCP event query plan - (2e051d2) - brianh
- (**18**) plan tax assist evidence-chain interfaces - (f80d1f6) - brianh
- (**18-03**) complete tax-assist evidence-chain interfaces plans - (115b5d1) - brianh
- (**agents**) require session lesson capture for posterity - (ad47077) - brianh
- (**milestone**) archive v1.0 roadmap and requirements - (482983d) - brianh
- (**milestone**) add v1.0 audit report - (01ee29d) - brianh
- (**phase-13**) evolve PROJECT.md after phase completion - (4eab3d8) - brianh
- (**phase-13**) complete phase execution - (8939d74) - brianh
- (**roadmap**) add gap closure phases 13-18 - (b347a67) - brianh
- (**state**) align handoff to phase 14 after phase 13 completion - (bd72945) - brianh
- create milestone v1.2 roadmap - (82c024e) - brianh
- define milestone v1.2 requirements - (2e65b2e) - brianh
- start milestone v1.2 Claude Connector Interop - (5960e4f) - brianh
- capture todo - Add Claude Cowork MCP install matrix and CI gate - (c33cb11) - brianh
- add concrete mcp usage examples to agents guide - (9831bc4) - brianh
- add agent purpose/capability guide and README reference - (a1b1bb9) - brianh
- start milestone v1.1 fdkms integrity - (9064ba6) - brianh
- add backlog item 999.1 — CI + release automation hardening - (84de370) - brianh
- create roadmap (6 phases) - (6e9bd7e) - brianh
- define v1 requirements - (4ff43fb) - brianh
- add project research - (c4619ba) - brianh
- initialize project - (4677e99) - brianh
#### Tests
- (**13-01**) add failing MCP adapter contract tests - (546c56e) - brianh
- (**13-02**) add failing MCP stdio DOC requirement tests - (d95d617) - brianh
- (**13-03**) add failing rustledger proxy transport coverage - (1aa7970) - brianh
- (**14-01**) add failing ONTO-01/02 ontology contract tests - (ce24318) - brianh
- (**14-02**) add failing ontology MCP transport tests - (ae0098a) - brianh
- (**15-01**) add failing reconciliation guardrail contracts - (163ebd9) - brianh
- (**15-02**) add failing reconciliation MCP transport contracts - (7f75625) - brianh
- (**16-01**) add failing hsm lifecycle and guard contracts - (fa2178b) - brianh
- (**16-02**) add failing checkpoint and resume contracts - (181b02c) - brianh
- (**16-03**) add failing hsm mcp transport e2e contracts - (8001964) - brianh
- (**17-01**) add failing contracts for append-only lifecycle events - (c6ff2ef) - brianh
- (**17-02**) add failing deterministic replay contracts - (30f3f20) - brianh
- (**17-03**) add failing MCP event history e2e contracts - (ec4a80e) - brianh
- (**18-01**) add failing tax-assist contracts for TAXA-01 and TAXA-03 - (ca1f8b6) - brianh
- (**18-02**) add failing evidence-chain contract for TAXA-02 - (bde256f) - brianh
- (**18-03**) add failing MCP e2e contracts for tax-assist tools - (baac788) - brianh
- (**e2e**) expand bdd coverage for ingest tool behaviors - (7efd6ac) - brianh
- (**turbo-mcp**) add phase6 failing tests for MCP exposure gaps (P0/P1/P2) - (d2b7354) - Claude Sonnet (coordinator)
- add sample statement fixtures for e2e regression - (5f40bf6) - brianh
#### Continuous Integration
- disable MCP Registry publish (requires direct write access) - (abf6b60) - Claude Sonnet (coordinator)
- add MCPB publish gate after tests - (676c58b) - Claude Sonnet (coordinator)
- add clippy sarif upload and podman publish-on-main - (588a231) - brianh
- add publish workflow for ghcr crates and pypi - (e6bb524) - brianh
#### Refactoring
- rename turbo-mcp to ledgerr-mcp - (4b484cd) - Claude Sonnet (coordinator)
#### Miscellaneous Chores
- (**16-03**) publish hsm validation map and operator verification docs - (eebdcda) - brianh
- (**17**) capture MCP row-key normalization guidance in AGENTS - (1323e94) - brianh
- (**17-03**) publish event MCP validation and verification docs - (2a7f381) - brianh
- (**18-03**) publish tax-assist runbook and phase validation map - (54f8012) - brianh
- (**dev**) add dotenv ignore and secret setup recipe fixes - (7a029d2) - brianh
- (**docs**) add plugin usage validation flow and mcp cli demos - (d522338) - brianh
- (**planning**) persist autonomous closeout state - (b9ba437) - brianh
- (**release**) add changelog for v1.1.0 - (acb4d27) - brianh
- (**v1.1**) archive completed milestone phases - (cf9b3c2) - brianh
- test release workflow trigger - (c22303f) - Claude Sonnet (coordinator)
- archive phase directories from completed milestones - (230159f) - brianh
- archive v1.0 milestone - (571ab76) - brianh
- ignore local PRD and refine phase 2 plan - (023da6b) - brianh
- add project config - (f498561) - brianh

- - -

## 0.1.0 - 2026-03-30
#### Features
- (**13-01**) add stdio MCP adapter and proxy boundary - (c95c675) - brianh
- (**13-02**) implement MCP stdio ingest e2e harness and replay checks - (cf28f37) - brianh
- (**13-03**) wire rustledger proxy ingest rows over MCP tools/call - (fd96420) - brianh
- (**14-01**) add service-owned ontology query tool wrappers - (587cbb4) - brianh
- (**14-01**) implement ontology store with deterministic persistence - (c8a8dd1) - brianh
- (**14-02**) add ontology MCP query/export transport handlers - (d5f6ca0) - brianh
- (**15-01**) enforce deterministic reconciliation commit guardrails - (1f7b0ef) - brianh
- (**15-01**) add reconciliation stage contracts and service APIs - (3518079) - brianh
- (**15-02**) expose reconciliation stage tools over MCP transport - (e63959a) - brianh
- (**16-01**) implement deterministic hsm transition and status APIs - (832e1f7) - brianh
- (**16-01**) add hsm domain contracts and service stubs - (b87ded0) - brianh
- (**16-02**) wire deterministic checkpoint persistence and resume - (35d8609) - brianh
- (**16-02**) add hsm checkpoint and resume contracts - (db30cc4) - brianh
- (**16-03**) expose hsm transition status resume over mcp - (674c433) - brianh
- (**17-01**) append deterministic lifecycle events from service actions - (8e56aa5) - brianh
- (**17-01**) add append-only lifecycle event store contracts - (9c2b6fd) - brianh
- (**17-02**) wire lifecycle replay service API - (93d1918) - brianh
- (**17-02**) add deterministic replay projector contracts - (17bf7c2) - brianh
- (**17-03**) wire MCP event replay and history tools - (b02a324) - brianh
- (**18-01**) implement deterministic tax-assist and ambiguity composition - (75b6b97) - brianh
- (**18-01**) add tax-assist service contracts and tool stubs - (749adf0) - brianh
- (**18-02**) implement deterministic evidence-chain retrieval - (8bc24e1) - brianh
- (**18-03**) expose tax-assist interfaces over MCP transport - (621bbea) - brianh
- (**phase-03**) implement runtime rhai classification and review queue - (accd407) - brianh
- (**phase-04**) add audited classification mutations with invariants - (d833758) - brianh
- (**phase-05**) add cpa workbook export and schedule summaries - (1da735e) - brianh
- (**phase-06**) add ci release automation and mvp e2e flow - (49b5a18) - brianh
- (**phase-1**) scaffold contracts, bootstrap, and turbo MCP interface - (1699ce8) - brianh
- (**phase-2**) complete deterministic ingest pipeline and verification - (ebf0fd5) - brianh
- (**phase-2**) add ingest_pdf and get_raw_context MCP contracts - (3b757a8) - brianh
- (**phase-2**) pivot ingest to rustledger-compatible beancount journals - (f69d7bd) - brianh
- (**phase-2**) add deterministic ingest primitives with idempotency tests - (11b4b9f) - brianh
- expand Cowork marketplace runtime and packaging guidance - (16fc219) - brianh
- add Claude Cowork plugin marketplace distribution artifacts - (628bd9d) - brianh
#### Bug Fixes
- (**13-01**) harden deterministic status and MCP error mapping - (3f307c7) - brianh
- remove Zone.Identifier files, add gitignore rule, reconcile STATE.md - (6efa654) - copilot-swe-agent[bot], *elasticdotventures*
- address all PR review feedback (version alignment, path safety, classify_ingested, sheets count, active year, release loop guard) - (47ea635) - copilot-swe-agent[bot], *elasticdotventures*
#### Documentation
- (**02**) research phase domain - (81b1b74) - brianh
- (**02**) add phase plan - (7bd1a7d) - brianh
- (**02**) discuss context - (ae7b8c7) - brianh
- (**03**) discuss context - (3592081) - brianh
- (**13**) add gap-closure plan for rustledger proxy MCP callable surface - (ea8424e) - brianh
- (**13**) add verification report with gap findings - (89f6e11) - brianh
- (**13**) add context research and validation artifacts - (aa12c52) - brianh
- (**13**) create phase plans - (3cd7bb6) - brianh
- (**13-01**) complete mcp boundary proxy surface plan - (a8a709f) - brianh
- (**13-02**) complete mcp-only doc verification plan - (dfa4b63) - brianh
- (**13-02**) publish MCP-only runbook and validation mapping - (787c501) - brianh
- (**13-03**) complete rustledger proxy callable-surface plan - (69b4dfe) - brianh
- (**13-03**) align runbook and validation to rustledger proxy transport - (ebbe75c) - brianh
- (**14**) create ontology persistence phase plans - (eca1808) - brianh
- (**14-01**) complete ontology persistence and query surface plan - (ee80513) - brianh
- (**14-02**) complete ontology mcp transport plan - (0c816f4) - brianh
- (**14-02**) align ontology MCP runbook and validation map - (9dad54e) - brianh
- (**15**) add reconciliation and commit guardrail plans - (55e3bfe) - brianh
- (**15-02**) complete reconciliation-and-commit-guardrails plan - (2468594) - brianh
- (**15-02**) align reconciliation transport runbook and validation map - (8eba986) - brianh
- (**16**) create autonomous HSM phase context and execution plans - (9106bb5) - brianh
- (**16-03**) complete moku-hsm-deterministic-status-and-resume plan - (66ad731) - brianh
- (**17**) create phase plan - (f6a86a5) - brianh
- (**17-01**) complete event domain foundation plan - (b4b410e) - brianh
- (**17-02**) complete deterministic replay plan - (8df1735) - brianh
- (**17-03**) complete MCP event query plan - (2e051d2) - brianh
- (**18**) plan tax assist evidence-chain interfaces - (f80d1f6) - brianh
- (**18-03**) complete tax-assist evidence-chain interfaces plans - (115b5d1) - brianh
- (**agents**) require session lesson capture for posterity - (ad47077) - brianh
- (**milestone**) archive v1.0 roadmap and requirements - (482983d) - brianh
- (**milestone**) add v1.0 audit report - (01ee29d) - brianh
- (**phase-13**) evolve PROJECT.md after phase completion - (4eab3d8) - brianh
- (**phase-13**) complete phase execution - (8939d74) - brianh
- (**roadmap**) add gap closure phases 13-18 - (b347a67) - brianh
- (**state**) align handoff to phase 14 after phase 13 completion - (bd72945) - brianh
- create milestone v1.2 roadmap - (82c024e) - brianh
- define milestone v1.2 requirements - (2e65b2e) - brianh
- start milestone v1.2 Claude Connector Interop - (5960e4f) - brianh
- capture todo - Add Claude Cowork MCP install matrix and CI gate - (c33cb11) - brianh
- add concrete mcp usage examples to agents guide - (9831bc4) - brianh
- add agent purpose/capability guide and README reference - (a1b1bb9) - brianh
- start milestone v1.1 fdkms integrity - (9064ba6) - brianh
- add backlog item 999.1 — CI + release automation hardening - (84de370) - brianh
- create roadmap (6 phases) - (6e9bd7e) - brianh
- define v1 requirements - (4ff43fb) - brianh
- add project research - (c4619ba) - brianh
- initialize project - (4677e99) - brianh
#### Tests
- (**13-01**) add failing MCP adapter contract tests - (546c56e) - brianh
- (**13-02**) add failing MCP stdio DOC requirement tests - (d95d617) - brianh
- (**13-03**) add failing rustledger proxy transport coverage - (1aa7970) - brianh
- (**14-01**) add failing ONTO-01/02 ontology contract tests - (ce24318) - brianh
- (**14-02**) add failing ontology MCP transport tests - (ae0098a) - brianh
- (**15-01**) add failing reconciliation guardrail contracts - (163ebd9) - brianh
- (**15-02**) add failing reconciliation MCP transport contracts - (7f75625) - brianh
- (**16-01**) add failing hsm lifecycle and guard contracts - (fa2178b) - brianh
- (**16-02**) add failing checkpoint and resume contracts - (181b02c) - brianh
- (**16-03**) add failing hsm mcp transport e2e contracts - (8001964) - brianh
- (**17-01**) add failing contracts for append-only lifecycle events - (c6ff2ef) - brianh
- (**17-02**) add failing deterministic replay contracts - (30f3f20) - brianh
- (**17-03**) add failing MCP event history e2e contracts - (ec4a80e) - brianh
- (**18-01**) add failing tax-assist contracts for TAXA-01 and TAXA-03 - (ca1f8b6) - brianh
- (**18-02**) add failing evidence-chain contract for TAXA-02 - (bde256f) - brianh
- (**18-03**) add failing MCP e2e contracts for tax-assist tools - (baac788) - brianh
- (**e2e**) expand bdd coverage for ingest tool behaviors - (7efd6ac) - brianh
- add sample statement fixtures for e2e regression - (5f40bf6) - brianh
#### Continuous Integration
- add publish workflow for ghcr crates and pypi - (e6bb524) - brianh
#### Miscellaneous Chores
- (**16-03**) publish hsm validation map and operator verification docs - (eebdcda) - brianh
- (**17**) capture MCP row-key normalization guidance in AGENTS - (1323e94) - brianh
- (**17-03**) publish event MCP validation and verification docs - (2a7f381) - brianh
- (**18-03**) publish tax-assist runbook and phase validation map - (54f8012) - brianh
- (**planning**) persist autonomous closeout state - (b9ba437) - brianh
- (**v1.1**) archive completed milestone phases - (cf9b3c2) - brianh
- archive phase directories from completed milestones - (230159f) - brianh
- archive v1.0 milestone - (571ab76) - brianh
- ignore local PRD and refine phase 2 plan - (023da6b) - brianh
- add project config - (f498561) - brianh

- - -

Changelog generated by [cocogitto](https://github.com/cocogitto/cocogitto).
