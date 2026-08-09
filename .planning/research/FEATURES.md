# Feature Research

**Domain:** Windows package-manager distribution (winget-pkgs) for a Tauri v2 desktop app
**Researched:** 2026-08-08
**Confidence:** HIGH (primary sources: Microsoft Learn docs, live winget-pkgs repo docs, GitHub issues; MEDIUM on exact current moderator-review SLA, which Microsoft docs leave vague)

> Scope note: this supersedes the previous FEATURES.md content (product-level tax-ledger
> feature landscape) for this milestone. This pass is scoped ONLY to the winget-packaging
> milestone: shipping the already-built, unsigned Tauri MSI+NSIS installers to
> `microsoft/winget-pkgs`. It does not re-litigate core product features.

## Feature Landscape

### Table Stakes (Users Expect These)

Minimum a maintainer must do to get `<Publisher>.ledgrrr` listed and installable via `winget install`.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Valid multi-file manifest (version + defaultLocale + installer, YAML, schema 1.12.x) | winget-pkgs requires PascalCase YAML conforming to JSON Schema; singleton format only works for 1 installer/1 locale — ledgrrr ships MSI+NSIS so multi-file is required | LOW | 3 files minimum: `<id>.yaml` (version), `<id>.locale.en-US.yaml` (defaultLocale), `<id>.installer.yaml`. Path: `manifests/<letter>/<Publisher>/ledgrrr/<version>/` |
| `InstallerSha256` matching actual published asset hash | PR bot fails hard (`Error-Hash-Mismatch` / `Validation-Hash-Verification-Failed`) if hash doesn't match download | LOW | Must recompute per release; automatable — see winget-releaser below |
| `InstallerUrl` is direct HTTPS link from GitHub Releases (own repo), not a redirector/vanity URL | Policy requires installer come directly from ISV's release location; `Validation-HTTP-Error` / `Validation-Indirect-URL` / `Validation-Domain` bot checks enforce this | LOW | GitHub Releases asset URLs satisfy this natively — already true of existing CI output |
| Correct `InstallerType` per artifact (`nullsoft` for the `.exe` NSIS installer, `msi`/`wix` for the `.msi`) | Bot maps type to expected silent-install behavior; wrong type causes `Validation-Unattended-Failed` (install hangs waiting for UI) | LOW | Tauri's NSIS output is Nullsoft-based; declaring `InstallerType: nullsoft` makes winget auto-apply `/S` — no manual `InstallerSwitches` needed. MSI needs no switches either (winget knows `msiexec /quiet`) unless custom UI properties are used |
| Silent/unattended install support (both installers) | Winget policy: "All tools must support a silent install" — hard requirement, not optional; bot dynamically test-installs with no UI | LOW | Tauri NSIS/MSI bundlers support this by default (`/S` and `/quiet` respectively) — no app code changes needed, just correct manifest declaration |
| Clean uninstall (both installers register uninstall correctly, remove files) | `Validation-Uninstall-Error` bot check — tests silent uninstall too | LOW-MEDIUM | Verify via `Tools/SandboxTest.ps1` locally before submitting; Tauri bundlers generally handle this out of the box |
| Correct `Architecture` field per installer entry (x64 today) | Required manifest field; wrong/missing value causes `Internal-Error-NoArchitectures` | LOW | Existing CI only builds x64 per milestone context — declare `x64` only for v1 |
| Unique `PackageIdentifier` in `Publisher.Package` form, no existing manifest/open PR collision | Winget rejects duplicate identifiers outright; only one manifest per PR allowed | LOW | Candidate: `PromptExecution.ledgrrr` (matches org) — verify identifier is unused in winget-pkgs and has no open competing PR before drafting |
| Fork winget-pkgs, sparse-checkout, branch-per-submission, open PR to `microsoft/winget-pkgs` | This *is* the distribution mechanism — no alternate submission path exists | LOW-MEDIUM | One-time repo/tooling setup; the `wingetcreate` CLI (`winget install wingetcreate`) can automate manifest authoring + PR creation instead of hand-editing YAML |
| Pass automated Azure Pipelines validation (URL reputation, hash check, dynamic install/uninstall test, AV scan) | Gate before any human ever looks at the PR; failing labels (`Binary-Validation-Error`, `Validation-Defender-Error`, etc.) block merge | MEDIUM | Largely out of maintainer's direct control once submitted, but avoidable by getting the manifest right first time and by the installer being clean under Defender |
| **Manual moderator review and approval** | Microsoft stopped auto-merging all PRs after abuse/quality problems; every submission — including passing ones — now gets a human moderator look before merge | LOW effort / **HIGH wait-time variance** | Not skippable for a first submission; no fixed SLA is published. This is a *process* dependency, not a technical one — factor into release-cadence expectations, not CI design |

### Differentiators (Competitive Advantage)

Not required for listing, but materially improve maintainer ergonomics and end-user experience once listed.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| `winget-releaser` GitHub Action (vedantmgoyal9/winget-releaser, built on Komac) on every GitHub Release | Fully automates post-v1 manifest updates: bumps `PackageVersion`, recomputes hashes, opens the update PR — zero manual YAML editing per release | LOW-MEDIUM | Depends on the existing `release.yml` publishing GitHub Releases with predictable asset names — **already satisfied** by current CI (`gh release create` in `release.yml`). Needs a classic PAT (`public_repo` scope) stored as a repo secret; add as a new job/step triggered on `release: published` |
| Product-identity stability across releases | Winget's upgrade-matching can misfire if package identity drifts between builds (see Anti-Features) | LOW (verification only) | ledgrrr already uses a non-default `productName: "ledgrrr"` and `identifier: "ventures.elastic.ledgrrr"` in `tauri.conf.json` — avoids the known Tauri-default-name collision bug (winget-cli#6040). No action needed beyond *not* reverting to `tauri-app` defaults |
| Additional locale manifest(s) | Broader discoverability/search relevance in non-en-US locales | LOW | Optional; only add if real localized strings exist — don't fabricate translations |
| `AppMoniker` field | Lets users `winget install ledgrrr` by short name instead of full `Publisher.Package` id, if moniker is unclaimed | LOW | Free win, no dependency; add during manifest authoring |
| ARM64 installer + manifest architecture entry | Growing ARM64 Windows (Snapdragon X, Surface) install base | MEDIUM-HIGH | Requires a new CI build target (`aarch64-pc-windows-msvc`, extra Visual Studio components) — genuinely new CI work, not just packaging; defer past v1 |
| Code-signing certificate (EV or standard) | Removes SmartScreen "Unrecognized App" friction, builds reputation faster, satisfies stricter enterprise winget-source policies | HIGH (cost + process, not code) | Explicitly out of scope per milestone context ("no signing") — listed only as the natural next differentiator once budget/process exists. Unsigned apps ARE fully submittable to winget-pkgs; signing is not a submission requirement, only a UX one |
| `ReleaseNotesUrl` pointing at Cocogitto changelog output | Shows changelog in `winget show`/upgrade UX | LOW | `release.yml` already generates `RELEASE_NOTES.md` via Cocogitto — trivial to surface as a URL in the locale manifest |
| Verified/known publisher status | Microsoft's evolving "verified publishers" fast-track is said to reduce manual-review friction on future submissions | N/A (not maintainer-initiated) | Emerging Microsoft-side policy, not something to design for directly; mention as a future tailwind only |

### Anti-Features (Commonly Requested, Often Problematic)

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|------------------|-------------|
| Shipping with Tauri's default `productName`/identifier (`tauri-app`) | Seems like a non-issue — "it's just a name" | Confirmed live bug (microsoft/winget-cli#6040): a default-named Tauri MSI installed to the default path gets misidentified by `winget list`/upgrade matching as an unrelated third-party package, corrupting upgrade detection | Already avoided — `tauri.conf.json` sets a distinct `productName: "ledgrrr"` and reverse-DNS `identifier`. Treat any regression back to Tauri defaults as a release blocker |
| Custom/self-hosted "app store" or redirector URL for the installer download | Feels more "branded" than a raw GitHub Releases link | Trips `Validation-Indirect-URL` / `Validation-Domain` bot checks — policy requires the installer URL come directly from the ISV's actual release location, not a redirector or vanity domain | Point `InstallerUrl` straight at the GitHub Release asset URL (already the natural output of existing CI) |
| Trying to get the first submission auto-merged / skip review | Faster time-to-listing | Not possible under current policy — manual moderator review is mandatory for all submissions since Microsoft disabled auto-merge; treating it as skippable leads to false "why hasn't this merged" escalations | Plan announcement timing around unpredictable review latency (days-to-weeks); don't gate marketing on a specific merge date |
| Requesting a signing certificate as a submission blocker | Assumption that winget requires signed installers like some enterprise app stores | False — winget-pkgs explicitly accepts unsigned installers (SmartScreen friction is a UX cost, not a submission gate); conflating the two adds unnecessary scope/cost to this milestone | Submit unsigned now (matches milestone scope); revisit signing as a separate, later differentiator once reputation/UX friction is actually felt |
| Hand-editing manifests for every release long-term | Feels "simpler" than setting up automation for a one-time task | Doesn't scale — every future release becomes a manual YAML/hash/PR chore, and hash mismatches (`Error-Hash-Mismatch`) are the most common self-inflicted validation failure | `winget-releaser` Action wired to the existing `release.yml` GitHub Release step (already a natural fit — see Differentiators) |
| Submitting MSI and NSIS as two separate `PackageIdentifier`s | Simpler mental model per-installer | Fragments the package across two winget entries, confuses `winget upgrade`, violates the "one manifest per unique app" expectation | One manifest, two `Installers:` entries differentiated by `InstallerType` (`msi` and `nullsoft`) under the same `PackageIdentifier`/version, as shown in Microsoft's own multi-installer examples |

## Feature Dependencies

```
Multi-file manifest (version+defaultLocale+installer)
    └──requires──> Stable PackageIdentifier decision (Publisher.Package)
                       └──requires──> Verify identifier unused in winget-pkgs

winget-pkgs PR submission
    └──requires──> Multi-file manifest passing `winget validate`
    └──requires──> Correct InstallerType + InstallerSha256 per artifact
    └──requires──> Existing CI producing MSI+NSIS release assets (ALREADY BUILT)

winget-releaser automation (Differentiator)
    └──requires──> First manual manifest merged into winget-pkgs (establishes the folder/id to update)
    └──requires──> Existing release.yml GitHub Release step (ALREADY BUILT)
    └──enhances──> Manual manifest submission (removes it as a recurring task after v1)

ARM64 manifest entry (Differentiator, deferred)
    └──requires──> New ARM64 CI build target (NOT YET BUILT — separate CI scope, not packaging scope)

Code signing (Differentiator, deferred)
    └──enhances──> SmartScreen/reputation UX
    └──conflicts with──> "no signing" milestone scope — explicitly out of this milestone
```

### Dependency Notes

- **winget-pkgs PR submission requires existing CI producing MSI+NSIS assets:** satisfied per milestone context — no new build-artifact work needed, only manifest authoring and the fork/PR workflow.
- **winget-releaser requires the first manifest to already exist in winget-pkgs:** the Action updates an existing package entry; it cannot create the initial submission. The first PR must be done manually (via `wingetcreate` or hand-authored YAML) before automation is wired in.
- **winget-releaser enhances the manual submission path:** once live, it removes hash/version-bump toil from every subsequent release, turning winget distribution from a per-release chore into a zero-touch side effect of the existing `release.yml` GitHub Release step.
- **ARM64 and code-signing both conflict with current milestone scope:** each is a real, separately-scoped body of work (new CI target; certificate acquisition + signing pipeline) and should not be pulled into "get unsigned x64 installer onto winget" — flagged here only so the roadmap doesn't accidentally conflate them.
- **Manual moderator review is a process dependency, not a technical one:** no manifest quality improvement removes the mandatory human-review step for a first submission; roadmap timing should treat "PR opened" and "PR merged" as separated by an unpredictable (days-to-weeks) gap.

## MVP Definition

### Launch With (v1)

Minimum to get `ledgrrr` installable via `winget install` for the first time.

- [ ] Choose and verify-available `PackageIdentifier` (e.g. `PromptExecution.ledgrrr`) — blocks everything else
- [ ] Author 3-file manifest (version, defaultLocale en-US, installer) covering both MSI (`msi`/`wix`) and NSIS (`nullsoft`) installers, x64 only — required for a valid submission
- [ ] Compute and embed correct `InstallerSha256` for both release assets — hash mismatch is the #1 avoidable bot failure
- [ ] Run `winget validate` and `Tools/SandboxTest.ps1` locally before opening PR — catches unattended-install/uninstall failures before the bot does
- [ ] Fork `microsoft/winget-pkgs`, submit PR, respond to bot/moderator feedback — the actual delivery mechanism

### Add After Validation (v1.x)

Add once the first manifest is merged and listed.

- [ ] Wire `winget-releaser` Action into `release.yml` — trigger: first manual merge succeeds, so there's an existing package entry to target
- [ ] Add `ReleaseNotesUrl` pointing at Cocogitto-generated changelog — trigger: manifest already exists, cheap follow-up PR
- [ ] Add `AppMoniker` if `ledgrrr` moniker is unclaimed — trigger: cheap, do opportunistically alongside releaser wiring

### Future Consideration (v2+)

Defer until there's real signal (user demand, ARM64 hardware feedback, or budget for signing).

- [ ] ARM64 build + manifest architecture entry — defer until ARM64 Windows demand is observed; requires a new CI target, out of packaging scope
- [ ] Code signing (EV/standard cert) — defer until SmartScreen friction is an actual reported user complaint; explicitly out of current milestone
- [ ] Additional locale manifests — defer until real (non-machine-translated) localized strings exist for the app itself

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| Multi-file manifest + PR submission (x64, MSI+NSIS) | HIGH | LOW | P1 |
| Correct InstallerType/switches per installer | HIGH | LOW | P1 |
| Local `winget validate` / SandboxTest before submitting | MEDIUM | LOW | P1 |
| winget-releaser automation | MEDIUM | LOW | P2 |
| ReleaseNotesUrl / AppMoniker | LOW | LOW | P2 |
| ARM64 support | LOW-MEDIUM | HIGH | P3 |
| Code signing | MEDIUM | HIGH | P3 |
| Additional locales | LOW | MEDIUM | P3 |

**Priority key:**
- P1: Must have for launch
- P2: Should have, add when possible
- P3: Nice to have, future consideration

## Competitor Feature Analysis

Not a competitive-product domain in the usual sense — this is a distribution-channel integration. "Competitors" here are comparable open-source desktop apps' winget packaging maturity, used as a bar for what "well-maintained" looks like.

| Feature | Reference examples | Our Approach |
|---------|--------------------|--------------|
| Auto-update manifests on release | Many Tauri/Electron OSS apps wire `winget-releaser` directly into their existing release workflow (pattern documented in the Action's own README/marketplace listing) | Adopt the same pattern once the v1 manifest is merged — reuses existing `release.yml` |
| Non-default Tauri product identity | Apps that keep Tauri's default `tauri-app` name have hit the winget-cli#6040 misidentification bug; well-packaged Tauri apps always set a distinct `productName`/`identifier` | Already correct in `tauri.conf.json` (`productName: "ledgrrr"`, `identifier: "ventures.elastic.ledgrrr"`) — just don't regress it |
| Unsigned-first, sign-later | Common bootstrap path for small OSS teams — submit unsigned, accept SmartScreen friction short-term, add signing once budget/adoption justifies it | Matches this milestone's explicit scope exactly |

## Sources

- [Submit your manifest to the repository | Microsoft Learn](https://learn.microsoft.com/en-us/windows/package-manager/package/repository)
- [Create your package manifest | Microsoft Learn](https://learn.microsoft.com/en-us/windows/package-manager/package/manifest)
- [winget-pkgs repository (microsoft/winget-pkgs)](https://github.com/microsoft/winget-pkgs)
- [winget-pkgs doc/README.md](https://github.com/microsoft/winget-pkgs/blob/master/doc/README.md)
- [WinGet Releaser action (vedantmgoyal9/winget-releaser)](https://github.com/vedantmgoyal9/winget-releaser)
- [WinGet Releaser — GitHub Marketplace listing](https://github.com/marketplace/actions/winget-releaser)
- [microsoft/winget-cli issue #6040 — Tauri default-name package misidentification](https://github.com/microsoft/winget-cli/issues/6040)
- [SmartScreen reputation for Windows app developers | Microsoft Learn](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation)
- ["Microsoft will manually review Winget repo submissions" — Winaero](https://winaero.com/microsoft-will-manually-review-winget-repo-submissions/)
- [Windows Installer | Tauri v2 docs](https://v2.tauri.app/distribute/windows-installer/)
- Repo inspection: `/home/brianh/.dotfiles/vendor/ledgrrr/.github/workflows/release.yml`, `/home/brianh/.dotfiles/vendor/ledgrrr/crates/ledgerr-host/tauri.conf.json`

---
*Feature research for: winget distribution of the ledgrrr Tauri desktop app*
*Researched: 2026-08-08*
