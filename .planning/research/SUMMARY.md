# Project Research Summary

**Project:** ledgrrr desktop app — v1.3 "Windows Distribution & Winget Packaging" milestone
**Domain:** CI/CD release automation + Windows package-manager (winget) distribution for a Tauri v2 desktop app vendored inside a monorepo
**Researched:** 2026-08-08
**Confidence:** HIGH

## Executive Summary

This milestone takes ledgrrr from "CI builds unsigned MSI/NSIS installers on `windows-latest`" to "installable via `winget install`." The four research passes agree on a clean three-phase shape: (1) fix a live landmine in the existing Windows build workflow and wire up hash-stable GitHub Release publishing inside the monorepo's own trust boundary, (2) manually bootstrap the very first `microsoft/winget-pkgs` submission by hand (`wingetcreate`/`komac`, PR, human moderator review), and only then (3) wire `vedantmgoyal9/winget-releaser` into CI so every subsequent release auto-updates the manifest. No code-signing, no ARM64, and no cross-compiled build path are in scope — all four documents independently confirm these are out-of-scope differentiators, not blockers.

The single most important cross-cutting risk is topological, not technical: `vendor/ledgrrr` is a git submodule of `elasticdotventures/_b00t_`, with its own separate tag namespace and repo (`PromptExecution/ledgrrr`). A `cog bump --auto` tag pushed inside the submodule never reaches the outer repo's Actions — the release-triggering tag must be a distinct, manually-pushed tag (`ledgrrr-desktop-v*`) in `_b00t_` itself. Layered on top of that, the existing `build-tauri-windows.yml` workflow currently references a crate path (`vendor/ledgrrr/crates/ledgerr-tauri`) that no longer exists on ledgrrr's `main` (renamed/merged into `crates/ledgerr-host`) — this is a pre-existing landmine, independent of any new winget work, that will silently break the build the moment the submodule pointer is next advanced. It must be fixed as a standalone first step, before any of the winget-specific work begins.

The second major risk is process, not code: `winget-releaser` (and the `komac` engine it wraps) categorically cannot create a brand-new winget-pkgs package — this was found independently by both the Stack and Pitfalls research passes, making it the most corroborated finding in this research set. The roadmap must sequence a manual bootstrap submission strictly before any release-automation is pointed at winget-pkgs, or the automation step will fail immediately with no useful recovery path other than doing the manual step anyway.

## Key Findings

### Recommended Stack

The new layer needed is entirely GitHub Actions + one-time CLI tooling — no new Cargo/npm dependencies. `softprops/action-gh-release@v3.0.2` publishes the existing MSI/NSIS build output as a GitHub Release with computed SHA256 hashes. `wingetcreate v1.12.13.0` (Microsoft's official CLI) performs the one-time manual bootstrap submission. `vedantmgoyal9/winget-releaser@v2` (renamed from `vedantmgoyal2009`, wraps `komac v2.16.0`) automates every subsequent manifest update once the package exists upstream. Manifests target winget-pkgs schema `1.10.0`–`1.12.0`.

**Core technologies:**
- `softprops/action-gh-release@v3.0.2` — publishes MSI+NSIS build artifacts as a GitHub Release with attached hashes — already fits the pattern this repo's `browser-ext-release.yml` uses (`action-gh-release@v1`, same idiom, newer major)
- `wingetcreate v1.12.13.0` — generates and submits the first-ever winget-pkgs manifest for `ventures.elastic.ledgrrr` — required because `winget-releaser` cannot bootstrap a new package (see Critical Pitfalls)
- `vedantmgoyal9/winget-releaser@v2` (wraps `komac v2.16.0`) — auto-generates and PRs manifest updates on every subsequent GitHub Release, once a package already exists upstream
- Classic GitHub PAT (`public_repo` scope) — required by both tools; fine-grained PATs are explicitly unsupported (tracked upstream in vedantmgoyal9/winget-releaser#172)

Notably, NSIS is the recommended canonical winget `InstallerType` (`nullsoft`, not `nsis`) over MSI — it's Tauri's default bundle target, smaller, and matches the one concrete Tauri-on-winget precedent found (WSL-UI). CI can keep building both bundle types for other purposes (e.g. MSI for enterprise/GPO deployment); only one type should be the winget-submitted "installer of record" to avoid the dual-installer ambiguity covered under Critical Pitfalls.

### Expected Features

**Must have (table stakes) — required to get `winget install` working at all:**
- Valid multi-file manifest (version + defaultLocale + installer YAML, schema ~1.10–1.12) at `manifests/<letter>/<Publisher>/ledgrrr/<version>/`
- `InstallerSha256` computed from the exact published release asset, `InstallerUrl` a direct GitHub Releases HTTPS link (no redirector/vanity domain)
- Correct `InstallerType` per artifact (`nullsoft` for NSIS `.exe`, `msi`/`wix` for `.msi`) with explicit silent-install switches
- Unique, verified-available `PackageIdentifier` in `Publisher.Package` form — a decision that blocks manifest authoring (see Architecture Approach)
- Manual first-time moderator review and approval — not skippable, no published SLA; a process dependency, not a technical one

**Should have (differentiators, add after the first manifest is merged):**
- `winget-releaser` wired into the existing release path for zero-touch future updates
- `ReleaseNotesUrl` pointing at the Cocogitto-generated changelog (already produced by existing CI)
- `AppMoniker` for short-name `winget install ledgrrr`

**Defer (v2+):**
- ARM64 installer + manifest entry — requires a genuinely new CI build target, out of packaging scope
- Code-signing certificate — winget-pkgs accepts unsigned installers; signing only affects SmartScreen UX, not submission eligibility
- Additional locale manifests — only once real (non-machine-translated) localized strings exist

### Architecture Approach

Two separate git repos share one GitHub Actions surface: `elasticdotventures/_b00t_` (outer monorepo, hosts all CI including `build-tauri-windows.yml`) vendors `PromptExecution/ledgrrr` as a submodule, each with its own `cog.toml` and tag namespace. This split drives nearly every architectural decision in this milestone.

**Major components:**
1. **`.github/workflows/build-tauri-windows.yml` (modified)** — first, fix the stale `crates/ledgerr-tauri` → `crates/ledgerr-host` path (the app was renamed/merged upstream in ledgrrr but the outer repo's pinned submodule commit predates the rename, masking the break until the next pointer bump); then add a `push: tags: ['ledgrrr-desktop-v*']` trigger and SHA256 + `action-gh-release` steps gated on that tag ref, mirroring the existing `browser-ext-release.yml` build+release-in-one-file pattern
2. **Tag-triggered release job (new steps in the same workflow file)** — computes SHA256 of both bundle outputs, publishes/attaches them to a GitHub Release on `_b00t_`
3. **winget manifest + submission (new, external)** — three YAML files authored against a Phase-1-produced Release asset URL+hash, living in a fork of `microsoft/winget-pkgs`, not in this repo
4. **`PackageIdentifier` decision (new, decision-only)** — `tauri.conf.json`'s `identifier` is `ventures.elastic.ledgrrr`, but winget convention favors the project's public GitHub identity; candidate `PromptExecution.ledgrrr` needs an explicit decision before manifest generation, since winget-pkgs PRs require re-submission if the identifier changes later

The critical, load-bearing integration fact: **`cog bump --auto` inside the submodule tags and pushes to `PromptExecution/ledgrrr` only — that tag is invisible to `elasticdotventures/_b00t_` Actions.** The release-triggering tag (`ledgrrr-desktop-v*`, chosen to avoid colliding with `_b00t_`'s own already-active bare `v*` top-level release tags) must be a distinct, separately-pushed tag in the outer monorepo, applied after the submodule pointer is bumped to the post-rename commit.

### Critical Pitfalls

1. **Stale crate path breaks the build on the next submodule bump** — `build-tauri-windows.yml` still `cd`s into `vendor/ledgrrr/crates/ledgerr-tauri`, which no longer exists on ledgrrr's current `main` (renamed/merged into `crates/ledgerr-host`). This currently "works" only because the outer repo's pinned submodule commit predates the rename. Fix the path (and the workflow's PR path-filters, which still reference `crates/ledgerr-tauri/**`) as a standalone first step, independent of and before any winget-specific work — it is a pre-existing landmine, not new scope.
2. **`winget-releaser` cannot bootstrap a brand-new package** — confirmed independently by both the Stack and Pitfalls research passes (the action hard-fails with "package does not exist in the winget-pkgs repository"). The first submission must be manual (`wingetcreate new`/`komac`), merged by a moderator, before `winget-releaser` is added to CI for subsequent releases. Building the automation before the manual bootstrap merges gives it nothing to act on.
3. **Submodule tag pushes never trigger superproject workflows** — a tag pushed to `PromptExecution/ledgrrr` is invisible to `elasticdotventures/_b00t_` Actions, since GitHub Actions triggers are scoped to the repo the workflow file lives in. Release tagging must happen as an explicit, separate step in the outer monorepo (`ledgrrr-desktop-v*`), distinct from the `cog bump` tag inside the submodule.
4. **Dev/pre-release leakage into winget** — `winget-releaser` has no built-in prerelease filter; if the winget-publish job triggers on generic `release: published` without re-checking `release.yml`'s existing even/odd-minor stable/dev parity logic, an experimental dev build becomes installable/upgradable by every winget user with no winget-side concept of a "pre-release channel" to protect them.
5. **`productName`-derived `UpgradeCode` drift breaks future upgrades silently** — Tauri's WiX bundler derives `UpgradeCode` as a UUID v5 of `productName`. ledgrrr already avoids the known collision bug (tauri-apps/tauri#14968 / winget-cli#6040, both tied to the Tauri default `"tauri-app"` name) by using the distinctive `"ledgrrr"` — but `productName`/`identifier` must be treated as frozen identity fields from the moment the first manifest ships; any future rename needs an explicit `upgradeCode` pin, not a silent regeneration.

## Implications for Roadmap

Based on research, suggested phase structure:

### Phase 1: Fix the build + release-automation foundation (monorepo-only, no external dependency)
**Rationale:** The stale `crates/ledgerr-tauri` path is a pre-existing landmine independent of this milestone's new scope — it must be fixed regardless, and fixing it first de-risks every later step that depends on a working Windows build. Everything else in this phase stays entirely within `_b00t_`'s own trust boundary (no third-party gatekeeper), which is why it's safe to fully automate immediately.
**Delivers:** Fixed `build-tauri-windows.yml` (`crates/ledgerr-host` path + updated PR path-filters), `ledgrrr-desktop-v*` tag trigger, SHA256 computation, `action-gh-release` publishing MSI+NSIS to a GitHub Release on `_b00t_`.
**Addresses:** FEATURES.md's table-stakes `InstallerSha256`/`InstallerUrl` requirements (a real, hash-stable, direct GitHub Releases URL must exist before any manifest can be authored).
**Avoids:** Pitfall 1 (stale path breaking the very next submodule bump), Pitfall 3 (submodule-vs-superproject tag topology), Pitfall 6 (hash mismatch from mutable assets — compute the hash in the same job that uploads the asset), Pitfall 1/dev-leakage (gate the tag/release step on the same stable/dev parity `release.yml` already computes).

### Phase 2: First manual winget-pkgs submission
**Rationale:** `winget-releaser`/`komac` cannot create a new package — confirmed independently by two research passes (Stack, Pitfalls) — so this phase is mandatory and cannot be replaced by automation. It also depends on Phase 1's Release existing and being stable, since the manifest pins an exact `InstallerUrl`+`InstallerSha256`.
**Delivers:** Resolved `PackageIdentifier` decision (candidate `PromptExecution.ledgrrr`), hand-authored/`wingetcreate`-generated 3-file manifest (single canonical `InstallerType`, explicit silent-install switches, frozen `productName`/`identifier` documented), PR to `microsoft/winget-pkgs`, merged after moderator review.
**Addresses:** FEATURES.md's full table-stakes list (manifest validity, unique identifier, silent install, manual moderator review) plus PITFALLS.md's manifest-authoring-phase items (Pitfalls 2, 3, 4, 5, 9).
**Avoids:** Pitfall 5 (dual MSI+NSIS ambiguity/one-way upgrade trap — pick one canonical `InstallerType`), Pitfall 7 (winget-releaser bootstrap failure, by not attempting automation here at all).

### Phase 3: Ongoing automation via winget-releaser
**Rationale:** Only safe once Phase 2's manifest shape is proven correct and merged upstream — `winget-releaser` updates an existing entry, it does not create one. Gate is explicit and hard: no Phase 3 work should start before Phase 2's PR is merged.
**Delivers:** `vedantmgoyal9/winget-releaser@v2` wired into a workflow triggered on `release: types: [published]`, scoped to `ledgrrr-desktop-v*` releases and to the same stable/dev parity check as Phase 1, using a classic PAT (`WINGET_TOKEN` secret) and an `installers-regex` scoped to only the chosen installer type (not the default pattern that would match both MSI and NSIS).
**Uses:** `vedantmgoyal9/winget-releaser@v2` / `komac v2.16.0` from STACK.md.
**Implements:** The "winget-pkgs submission mechanism" component from ARCHITECTURE.md's Data Flow, closing the loop from `cog bump --auto` → submodule bump → tag → Release → auto-PR with no manual winget-pkgs step in steady state.

### Phase Ordering Rationale

- Phase 1 before Phase 2 because a real, publicly-downloadable, hash-stable installer URL must exist before any manifest can reference it — and because the stale-path fix is a live landmine that should not be deferred behind winget-specific work.
- Phase 2 before Phase 3 because `winget-releaser`/`komac` have a hard, undocumented-in-quickstarts requirement that at least one manifest version already exist upstream — attempting Phase 3 first fails immediately with no recovery except doing Phase 2 anyway. This is the single most corroborated finding across the four research documents (found independently in both STACK.md and PITFALLS.md).
- The `PackageIdentifier` decision is sequenced into Phase 2, not Phase 1, because it only blocks manifest generation, not release automation — but it must be resolved as a real decision (not a placeholder) since winget-pkgs PRs require re-submission if the identifier changes after the fact.
- Code signing, ARM64, and additional locales are explicitly excluded from all three phases — each is a separately-scoped body of work (certificate acquisition, new CI build target, real translations) that would conflate unrelated scope into "get an unsigned x64 installer onto winget," per FEATURES.md's Anti-Features and Feature Dependencies sections.

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 2:** The exact moderator-review process and PR iteration pattern for a first-time publisher is only MEDIUM-confidence (Microsoft doesn't publish SLAs or exact review criteria) — expect to learn real turnaround/iteration cadence only by actually submitting.
- **Phase 3:** `installers-regex` scoping and `max-versions-to-keep` tuning are well-documented but worth a config-level sanity check against ledgrrr's actual release-asset naming once Phase 1's release step is live, to avoid the documented "Number of InstallerUrls are not equal" failure mode.

Phases with standard patterns (skip research-phase):
- **Phase 1:** Directly extends this repo's own existing `browser-ext-release.yml` pattern (tag-triggered, single-file build+release, `action-gh-release`) — no new pattern to research, just apply the established idiom plus the required path fix.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | Versions verified live via `gh api`/raw GitHub fetches on 2026-08-08, not training memory |
| Features | HIGH (MEDIUM on exact moderator-review SLA) | Primary sources: Microsoft Learn docs, live winget-pkgs repo docs, GitHub issues; Microsoft does not publish a fixed review timeline |
| Architecture | HIGH | Based on direct inspection of this repo's actual workflow files, `cog.toml`s, `tauri.conf.json`, crate layout, and tag history in both repos |
| Pitfalls | MEDIUM | Manifest mechanics and Tauri bundler behavior are well documented officially; SmartScreen reputation behavior and moderation timelines are corroborated across independent sources but Microsoft does not publish exact thresholds/SLAs |

**Overall confidence:** HIGH

### Gaps to Address

- **Moderator review latency:** No published SLA exists; treat "PR opened" and "PR merged" as separated by an unpredictable days-to-weeks gap when planning any announcement/marketing timing — do not gate release comms on a specific merge date.
- **`PackageIdentifier` final decision:** `PromptExecution.ledgrrr` is the leading candidate (matches the project's real GitHub org identity per ledgrrr's own `CLAUDE.md`) but is not yet finalized — confirm availability in winget-pkgs (no existing manifest or open competing PR) before drafting the Phase 2 manifest.
- **Canonical `InstallerType` choice (NSIS vs MSI):** STACK.md leans NSIS (Tauri default, smaller, has a concrete precedent); PITFALLS.md notes MSI has more reliable silent-uninstall behavior via `msiexec`. This is a real trade-off (Pitfall 4) that should be an explicit decision recorded in Phase 2 planning, not left implicit.
- **SmartScreen mitigation as ongoing process:** Not a one-time launch task — PITFALLS.md frames per-release Microsoft Security Intelligence submission as a recurring operational step; this should be captured in the roadmap's post-release/maintenance phase, not just this milestone's scope.

## Sources

### Primary (HIGH confidence)
- Direct repo inspection: `.github/workflows/build-tauri-windows.yml`, `.github/workflows/browser-ext-release.yml`, `.github/workflows/release.yml` (all in `elasticdotventures/_b00t_`)
- `vendor/ledgrrr/cog.toml`, `vendor/ledgrrr/Cargo.toml`, `crates/ledgerr-host/tauri.conf.json`, `crates/ledgerr-host/Cargo.toml`, `xtask/src/publisher.rs`
- `gh api repos/microsoft/winget-create/releases/latest`, `gh api repos/vedantmgoyal9/winget-releaser`, `gh api repos/softprops/action-gh-release/releases/latest`, `gh api repos/microsoft/winget-cli/releases/latest`, `gh api repos/russellbanks/Komac/releases/latest`
- https://learn.microsoft.com/en-us/windows/package-manager/package/repository and /manifest — official manifest schema and submission docs
- https://github.com/microsoft/winget-cli/issues/6040 — confirmed Tauri default-name package misidentification bug
- https://github.com/tauri-apps/tauri/issues/14968 — confirmed real `UpgradeCode` collision case

### Secondary (MEDIUM confidence)
- https://wsl-ui.octasoft.co.uk/blog/building-wsl-ui-winget — concrete Tauri-app-on-winget precedent
- https://github.com/microsoft/winget-pkgs/blob/master/doc/Moderation.md and Troubleshoot.md — bot error classes and review process description
- https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation — per-file-hash reputation behavior, no published thresholds
- https://www.todesktop.com/blog/posts/windows-apps-psa-ev-certs-do-not-grant-immediate-reputation-anymore — EV cert reputation policy change

### Tertiary (LOW confidence)
- Winaero article on manual moderator review policy change — single-source community reporting, directionally consistent with official Moderation.md but not itself an official source

---
*Research completed: 2026-08-08*
*Ready for roadmap: yes*
