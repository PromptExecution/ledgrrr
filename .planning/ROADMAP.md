# Roadmap: tax-ledger

## Milestones

- ✅ **v1.0 MVP** — Phases 1-6 shipped 2026-03-29 ([archive](./milestones/v1.0-ROADMAP.md))
- ✅ **v1.1 FDKMS Integrity** — Phases 7-18 shipped 2026-03-29 ([archive](./milestones/v1.1-ROADMAP.md))
- ⏸️ **v1.2 Claude Connector Interop** — Phases 19-21 (parked, not shipped, not abandoned — resume via `/gsd-plan-phase 19`)
- 🚧 **v1.3 Windows Distribution & Winget Packaging** — Phases 22-24 (in progress)

## Overview

Milestone v1.2 focuses on Claude connector interoperability so l3dg3rr can be installed, activated, and operated through connector-style MCP workflows with deterministic capability metadata, scoped permissions, and auditable session behavior.

## Phases

- [ ] **Phase 19: Connector Capability Profile and Scope Contracts** - Define connector-facing capability metadata, deterministic tool descriptors, and permission scope contracts.
- [ ] **Phase 20: Connector Installation and Activation Workflows** - Implement and validate operator install/activation flows for Claude/Cowork/Desktop connector contexts.
- [ ] **Phase 21: Connector Session Execution and Governance Diagnostics** - Prove connector-session tool execution, deterministic denial/error semantics, and auditable interaction outcomes.

## Phase Details

### Phase 19: Connector Capability Profile and Scope Contracts
**Goal**: Expose deterministic connector capability profiles and permission scope contracts for l3dg3rr MCP tools.
**Depends on**: Phase 18
**Requirements**: CCONN-01, CCONN-04
**Success Criteria**:
  1. Connector-facing tool metadata is deterministic and concise across runs.
  2. Permission scope policy is explicit by capability class and action type.
  3. Scope-denied operations return deterministic machine-readable denial diagnostics.

### Phase 20: Connector Installation and Activation Workflows
**Goal**: Deliver clear connector install and activation pathways with deterministic verification for operator environments.
**Depends on**: Phase 19
**Requirements**: CCONN-02, CCONN-06
**Success Criteria**:
  1. Claude/Cowork/Desktop connector install paths are documented and executable.
  2. Activation checks verify connector readiness deterministically.
  3. Organization-level compatibility/readiness notes are captured without changing ledger invariants.

### Phase 21: Connector Session Execution and Governance Diagnostics
**Goal**: Validate connector-scoped tool discovery/invocation and expose governance-grade diagnostics for operations.
**Depends on**: Phase 20
**Requirements**: CCONN-03, CCONN-05
**Success Criteria**:
  1. Connector sessions can run tools/list and tools/call for supported capabilities.
  2. Session-constrained failures map to deterministic reason keys and error classes.
  3. Connector interaction outcomes are auditable by success/blocked/error categories.

## Progress

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 19. Connector Capability Profile and Scope Contracts | 0/TBD | Not started | - |
| 20. Connector Installation and Activation Workflows | 0/TBD | Not started | - |
| 21. Connector Session Execution and Governance Diagnostics | 0/TBD | Not started | - |

## Backlog

- Phase 999.1: CI + Release Automation Hardening (deferred from prior cycle; can be promoted if needed)

## v1.3 Windows Distribution & Winget Packaging

### Overview

Milestone v1.3 ships an installable, unsigned ledgrrr desktop package via winget. Phase numbering starts at **Phase 22**, not 20, because v1.2 (Phases 19-21) is parked — not shipped, not abandoned — and its number range stays reserved for its eventual resumption via `/gsd-plan-phase 19`. Research (`.planning/research/SUMMARY.md`) lays out a strict dependency-ordered 3-phase shape: fix a live build landmine and wire fully-automated Release publishing within this repo's own trust boundary first; then perform the mandatory manual first-time `microsoft/winget-pkgs` submission (external moderator gate, cannot be automated); only then wire up `winget-releaser` for ongoing automation, since it cannot bootstrap a brand-new package.

### Phases

- [ ] **Phase 22: Release Automation Foundation** - Fix the stale `crates/ledgerr-tauri` -> `crates/ledgerr-host` path landmine and wire tag-triggered, hash-stable GitHub Release publishing for MSI/NSIS installers, entirely within `_b00t_`'s own trust boundary.
- [ ] **Phase 23: First Winget Submission** - Manually bootstrap the first-ever `microsoft/winget-pkgs` manifest and PR for ledgrrr, gated on external Microsoft moderator review before this phase can be marked complete.
- [ ] **Phase 24: Ongoing Winget Automation** - Wire `winget-releaser` into the release pipeline so every subsequent release auto-updates the manifest, hard-gated on Phase 23's PR having actually merged upstream.

### Phase Details

#### Phase 22: Release Automation Foundation
**Goal**: Fix the stale crate-path landmine and produce a real, publicly-downloadable, hash-stable GitHub Release for the Windows installers on every `ledgrrr-desktop-v*` tag push — fully within this repo's own CI trust boundary, no external gatekeeper.
**Depends on**: Phase 18 (last completed phase; v1.2 Phases 19-21 remain parked and untouched)
**Requirements**: REL-01, REL-02, REL-03, REL-04
**Success Criteria**:
  1. `build-tauri-windows.yml` references `crates/ledgerr-host` (not the stale `crates/ledgerr-tauri`) in both its build steps and PR path-filters.
  2. Pushing a `ledgrrr-desktop-v*` tag to the outer `_b00t_` repo triggers the Windows build+release job.
  3. The release job computes SHA256 hashes for both the MSI and NSIS installer artifacts in the same job that uploads them (no mutable-asset hash mismatch).
  4. Tag push produces an unsigned, public GitHub Release on `_b00t_` with the MSI, NSIS, and `.sha256` files attached.

#### Phase 23: First Winget Submission
**Goal**: Bootstrap the first-ever `microsoft/winget-pkgs` manifest for ledgrrr by hand and get it merged — `winget-releaser`/`komac` cannot create a new package, so this step cannot be replaced by automation.
**Depends on**: Phase 22 (requires a real, hash-stable Release URL to reference); gated on external Microsoft moderator review before this phase can be marked complete.
**Requirements**: WGT-01, WGT-02, WGT-03, WGT-04
**Success Criteria**:
  1. `PackageIdentifier` candidate `PromptExecution.ledgrrr` is confirmed available (no existing manifest or open competing PR) in `microsoft/winget-pkgs`.
  2. A 3-file manifest (version, `locale.en-US`, installer) is authored covering both MSI and NSIS installers at x64, with `ReleaseNotesUrl` and `AppMoniker` populated and referencing Phase 22's exact `InstallerUrl`/`InstallerSha256`.
  3. `winget validate` (or `Tools/SandboxTest.ps1`) passes locally against the manifest before the PR is opened.
  4. A PR is opened against `microsoft/winget-pkgs` and merged after moderator review — the phase is not complete until the merge lands.

#### Phase 24: Ongoing Winget Automation
**Goal**: Wire `winget-releaser` into the release pipeline so every future `_b00t_` release automatically PRs a manifest update, closing the loop with no manual winget-pkgs step in steady state.
**Depends on**: Phase 23 (hard gate — its PR must have actually merged upstream; `winget-releaser` updates an existing entry, it does not create one)
**Requirements**: WGT-05
**Success Criteria**:
  1. A `vedantmgoyal9/winget-releaser@v2` workflow step triggers on `release: types: [published]`, scoped to `ledgrrr-desktop-v*` releases.
  2. `installers-regex` is scoped to the single canonical installer type only (not the default pattern that would ambiguously match both MSI and NSIS).
  3. A subsequent tagged release produces an automated PR to `microsoft/winget-pkgs` updating the existing merged manifest to the new version, with no manual manifest-authoring step required.

### Progress

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 22. Release Automation Foundation | 0/TBD | Not started | - |
| 23. First Winget Submission | 0/TBD | Not started | - |
| 24. Ongoing Winget Automation | 0/TBD | Not started | - |
