# Requirements: tax-ledger

**Defined:** 2026-03-30
**Core Value:** Convert raw historical financial PDFs into accountant-usable, auditable Excel tax records without sending private data to third-party SaaS.

## v1 Requirements

### Document Ingestion (Docling)

- [x] **DOC-01**: User can ingest statement PDFs through Docling/docling-mcp and produce normalized transaction candidates with per-field provenance.
- [x] **DOC-02**: User can map extracted fields to canonical transaction schema (`account`, `date`, `amount`, `description`, `currency`, `source_ref`) deterministically.
- [x] **DOC-03**: User can replay ingestion for the same source and receive stable candidate IDs with no duplicate candidates.

### Ontology & Knowledge Model

- [x] **ONTO-01**: User can persist ontology entities for document, account, institution, transaction, tax-category, and evidence reference.
- [x] **ONTO-02**: User can query ontology relationships (document -> extracted tx -> reconciliation state -> tax treatment).
- [x] **ONTO-03**: User can serialize ontology data in structured machine-readable form for AI agent consumption.

### Reconciliation & Verification

- [x] **RECON-01**: User can enforce double-entry balancing constraints before transactions become committed truth.
- [x] **RECON-02**: User can run automated reconciliation checks between source totals, extracted rows, and ledger postings.
- [x] **RECON-03**: User receives explicit blocking errors for invariant failures (imbalance, duplicate, schema mismatch).

### Hierarchical State Orchestration (Moku HSM)

- [x] **HSM-01**: User can run pipeline lifecycle as hierarchical states (ingest -> normalize -> validate -> reconcile -> commit -> summarize).
- [x] **HSM-02**: User can transition states only through validated guards and collect transition evidence.
- [x] **HSM-03**: User can resume interrupted pipelines from last valid state without violating invariants.

### Event-Sourced Audit Log (Disintegrate)

- [x] **EVT-01**: User can persist append-only domain events for ingestion, classification, reconciliation, and adjustment actions.
- [x] **EVT-02**: User can reconstruct entity state from disintegrate event streams.
- [x] **EVT-03**: User can query event history by transaction/document/time window for audit and agent explanation.

### US Expat Tax Agent Assist

- [x] **TAXA-01**: User can derive US expat tax-relevant structured outputs (Schedule C/D/E and FBAR evidence views) from reconciled ontology truth.
- [x] **TAXA-02**: AI agents can retrieve explainable evidence chains for tax decisions (source doc -> event log -> current state).
- [x] **TAXA-03**: User can flag scenarios with elevated tax ambiguity for human review with linked provenance.

## v1.2 Requirements

### Claude Connector Interoperability

- [ ] **CCONN-01**: Operator can configure l3dg3rr as a Claude-compatible connector endpoint with explicit capability metadata and deterministic tool descriptions.
- [ ] **CCONN-02**: Operator can install and activate l3dg3rr connector flows for Claude/Cowork/Desktop using concise runbook steps with deterministic verification commands.
- [ ] **CCONN-03**: Agent can run connector-scoped tool discovery and tool invocation with deterministic responses under connector session constraints.

### Connector Safety and Governance

- [ ] **CCONN-04**: Operator can enforce explicit connector permission scope (read/write/action classes) and see deterministic denial diagnostics when scope is insufficient.
- [ ] **CCONN-05**: Operator can audit connector interaction outcomes (success, blocked, error class) with deterministic reason keys for troubleshooting.
- [ ] **CCONN-06**: Team/enterprise deployment can declare organization-level connector readiness checks and compatibility notes without changing core ledger invariants.

## v1.3 Requirements

### Release Automation

- [ ] **REL-01**: Maintainer's build pipeline no longer references the stale `crates/ledgerr-tauri` path — fixed to `crates/ledgerr-host` in `build-tauri-windows.yml` before any submodule pointer bump.
- [ ] **REL-02**: Maintainer can trigger a Windows desktop release by pushing a `ledgrrr-desktop-v*` tag to the outer `_b00t_` repo.
- [ ] **REL-03**: Release pipeline computes SHA256 hashes for both the MSI and NSIS installer artifacts on tagged builds.
- [ ] **REL-04**: Release pipeline creates an unsigned public GitHub Release with the installer and hash artifacts attached on tag push.

### Winget Manifest & Submission

- [ ] **WGT-01**: Maintainer has a decided, verified-available `PackageIdentifier` (`PromptExecution.ledgrrr`) before manifest authoring begins.
- [ ] **WGT-02**: Maintainer can author a 3-file winget manifest (version, `locale.en-US`, installer) covering both MSI and NSIS installers at x64, including `ReleaseNotesUrl` and `AppMoniker` fields.
- [ ] **WGT-03**: Maintainer validates the manifest locally (`winget validate` / `Tools/SandboxTest.ps1`) before opening a submission PR.
- [ ] **WGT-04**: Maintainer submits a manual first-listing PR to `microsoft/winget-pkgs` (fork + PR) and responds to bot/moderator feedback until merged.
- [ ] **WGT-05**: Maintainer has `winget-releaser` wired into the release pipeline for automated future-release manifest updates, gated on WGT-04 having merged.

## v2 Requirements

### Extended Intelligence

- **INTEL-01**: Probabilistic anomaly detection over historical ingestion/reconciliation behavior.
- **INTEL-02**: Automated agent-generated remediation proposals for detected reconciliation anomalies.

## Out of Scope

| Feature | Reason |
|---------|--------|
| Replacing CPA review with full autonomous filing | Human-in-the-loop accountant signoff remains mandatory |
| Multi-tenant cloud SaaS deployment | Current milestone remains local-first and operator-controlled |
| Non-tax personal finance dashboards as primary objective | Milestone scope is integrity and tax-assist knowledge workflows |
| ARM64 installer + manifest architecture entry | Requires a new CI build target (`aarch64-pc-windows-msvc`); separate scope from packaging the existing x64 build |
| Code-signing certificate (EV or standard) | None available; winget-pkgs accepts unsigned installers today — signing is a UX/reputation improvement, not a submission requirement. Revisit as a later differentiator |
| Additional locale manifests beyond en-US | No real (non-machine-translated) localized strings exist for the app yet |
| Automating the winget-pkgs PR before WGT-04 has merged | `winget-releaser` cannot bootstrap a brand-new package identifier — confirmed by two independent research passes; the first submission must be manual |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| DOC-01 | Phase 13 | Complete |
| DOC-02 | Phase 13 | Complete |
| DOC-03 | Phase 13 | Complete |
| ONTO-01 | Phase 14 | Complete |
| ONTO-02 | Phase 14 | Complete |
| ONTO-03 | Phase 14 | Complete |
| RECON-01 | Phase 15 | Complete |
| RECON-02 | Phase 15 | Complete |
| RECON-03 | Phase 15 | Complete |
| HSM-01 | Phase 16 | Complete |
| HSM-02 | Phase 16 | Complete |
| HSM-03 | Phase 16 | Complete |
| EVT-01 | Phase 17 | Complete |
| EVT-02 | Phase 17 | Complete |
| EVT-03 | Phase 17 | Complete |
| TAXA-01 | Phase 18 | Complete |
| TAXA-02 | Phase 18 | Complete |
| TAXA-03 | Phase 18 | Complete |
| CCONN-01 | Phase 19 | Pending |
| CCONN-02 | Phase 20 | Pending |
| CCONN-03 | Phase 21 | Pending |
| CCONN-04 | Phase 19 | Pending |
| CCONN-05 | Phase 21 | Pending |
| CCONN-06 | Phase 20 | Pending |

**Coverage:**
- v1 requirements: 18 total
- v1.2 additional requirements: 6 total (parked, not yet phase-planned this cycle)
- v1.3 additional requirements: 9 total (REL-01..04, WGT-01..05) — not yet mapped to phases, pending roadmap
- All requirements (v1 + v1.2 + v1.3): 33 total
- Mapped to phases: 24
- Unmapped: 9 (v1.3, pending `/gsd-plan-phase` roadmap)

---
*Requirements defined: 2026-03-30*
*Last updated: 2026-08-08 for v1.3 milestone requirements*
