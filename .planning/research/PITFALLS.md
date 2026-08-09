# Pitfalls Research

**Domain:** Windows package distribution (winget) for an unsigned, first-release Tauri v2 desktop app
**Researched:** 2026-08-08
**Confidence:** MEDIUM — winget-pkgs manifest mechanics and Tauri bundler behavior are well documented in official docs/issues; SmartScreen reputation behavior and moderation timelines are corroborated across multiple independent sources but Microsoft does not publish exact thresholds/SLAs.

## Critical Pitfalls

### Pitfall 1: Release-channel leakage — dev/pre-release builds reach winget users

**What goes wrong:**
ledgrrr's `release.yml` already encodes a stable/dev convention (even minor = stable `--latest`, odd minor = `--prerelease`). `winget-releaser` (vedantmgoyal9/winget-releaser) has **no built-in prerelease/draft filter** — its `action.yml` only exposes `release-tag`, `version`, `installers-regex`, `identifier`, `token`. If the winget-publish workflow triggers on `release: types: [published]` without re-checking the same parity logic, an odd-minor "dev/experimental" pre-release gets pushed into winget-pkgs and becomes installable/upgradable by every `winget install`/`winget upgrade` user — there is no winget-side concept of "pre-release channel" to protect them.

**Why it happens:**
Two independently-written automations (the existing `release.yml` parity check, and a new winget-publish workflow) drift out of sync. It's easy to wire the winget workflow to the generic `release: published` event and forget it fires for both stable and dev releases.

**How to avoid:**
Gate the winget-publish job on the same parity computation already in `release.yml` (`MINOR % 2 == 0`), or — simpler — only trigger winget publish from the "Create GitHub Release (stable even minor)" step's output, never from the pre-release step. Do not let `winget-releaser` run unconditionally on every published release.

**Warning signs:**
A winget-publish workflow step exists with no reference to `parity`/`is_stable` from the release job; `winget-releaser` invoked with `release-tag: ${{ github.event.release.tag_name }}` and no upstream conditional.

**Phase to address:**
Release-automation phase (wiring the winget-publish job into existing CI, before any manifest work).

---

### Pitfall 2: Generic/default productName causes winget package-matching collisions

**What goes wrong:**
`microsoft/winget-cli` issue #6040 documents a Tauri app installed with the **default scaffold name "tauri-app"** being misidentified by `winget list`/`winget upgrade` as a completely unrelated package (`maqibin.MDXNotes`) already in the winget catalog, because winget's Add/Remove-Programs matching is fuzzy on DisplayName/Publisher. A generic or collision-prone `productName` risks the same class of bug even without submitting to winget yourself — any local user's ARP entry can get fuzzy-matched against a catalog entry.

**Why it happens:**
Winget correlates installed-app registry entries (ARP) to catalog manifests by name/publisher heuristics when a package hasn't been explicitly installed via winget. Common/generic names collide.

**How to avoid:**
ledgrrr already uses a distinctive `productName: "ledgrrr"` and `identifier: "ventures.elastic.ledgrrr"` in `tauri.conf.json` — confirm this stays stable (see Pitfall 9) and is distinctive enough that ARP fuzzy-matching won't false-positive. Don't rename the app casually after this milestone ships.

**Warning signs:**
`winget list` on a clean test VM after installing ledgrrr shows any Source other than blank/"winget-pkgs (yours)"; DisplayName in ARP is generic ("app", "desktop", a truncated/templated string).

**Phase to address:**
Manifest-authoring phase (verify identity fields before generating the first manifest); re-check in Submission phase via clean-VM install test.

---

### Pitfall 3: Missing/incorrect InstallerSwitches.Silent breaks silent install and CI validation

**What goes wrong:**
winget-pkgs automated validation flags manifests where `Silent`/`SilentWithProgress` switches aren't specified for `InstallerType: exe` (NSIS) — this is one of the most common first-PR validation failures. Without a correct silent switch, `winget install` still "succeeds" from winget's perspective but pops an interactive installer UI, or worse, `winget upgrade --all` (used by many automation scripts) hangs waiting for user input.

**Why it happens:**
Tauri's NSIS installer supports `/S` for silent mode by default, but this is **not automatically reflected in a hand-authored or wingetcreate-generated manifest** — `InstallerSwitches.Silent` must be set explicitly (`"/S"` for NSIS; MSI/WiX installers get silent behavior largely for free via `msiexec` but still benefit from explicit `/quiet` in `InstallerSwitches.Silent` for clarity and validator satisfaction).

**How to avoid:**
Explicitly set `InstallerSwitches.Silent: "/S"` (NSIS) and `InstallerSwitches.Silent: "/quiet"` (MSI, if MSI is submitted) in the manifest. Run `winget validate <manifest-path>` locally before opening the PR — it catches this class of error pre-submission.

**Warning signs:**
`winget validate` or the winget-pkgs PR bot posts an `InstallerType` / "does not support unattended install" warning; manual `winget install ledgrrr` pops a visible installer window instead of running silently.

**Phase to address:**
Manifest-authoring phase.

---

### Pitfall 4: AppsAndFeaturesEntries + NSIS breaks silent uninstall

**What goes wrong:**
There is a known winget-cli limitation: once a manifest includes `AppsAndFeaturesEntries` (needed for correct Add/Remove-Programs version matching), **winget will not run the NSIS uninstaller silently** — because NSIS-generated uninstallers don't support a true `SilentWithProgress` mode the way MSI/msiexec does. Users running `winget uninstall ledgrrr` (or automation doing the same) can get an unexpected UI prompt or a hung/failed silent uninstall.

**Why it happens:**
This is a documented winget-cli/NSIS interaction gap, not something the packager can fully fix from the manifest side — it's inherent to how NSIS uninstallers report exit/silence status versus MSI's builtin `msiexec /x /quiet`.

**How to avoid:**
If a fully-silent uninstall matters (e.g., for fleet/automation use), prefer submitting the **MSI** as the primary winget installer rather than NSIS, since MSI uninstall-silence is reliable via `msiexec`. If NSIS is submitted, document in release notes / README that `winget uninstall` may require interactive confirmation, and test the actual uninstall UX on a clean VM rather than assuming.

**Warning signs:**
`winget uninstall ledgrrr` from a non-interactive/CI/remote session hangs or leaves the app partially removed.

**Phase to address:**
Manifest-authoring phase (installer-type decision); verify in Submission phase's clean-VM pass.

---

### Pitfall 5: Shipping both MSI and NSIS for the same architecture creates ambiguity and a one-way upgrade trap

**What goes wrong:**
CI already builds both `msi,nsis` bundles. Submitting both installer types for the same architecture in one winget manifest means: (a) winget has no defined precedence between them beyond enumeration order in the manifest (users can force a choice with `--installer-type`, but default behavior is not something you control cleanly), and (b) Tauri/WiX upgrades are **one-directional**: a user who installed via MSI can be upgraded to NSIS, but a user who installed via NSIS **cannot** be upgraded to MSI in a later version. Flip-flopping which type winget serves as "the" installer across versions silently strands NSIS-original users.

**Why it happens:**
It seems natural to submit everything CI already produces, but winget manifests aren't "pick the best installer at install time" the way an OS package repo with dependency resolution is — each `InstallerType` node is closer to a distinct distribution channel that must stay consistent release-over-release.

**How to avoid:**
Pick **one** primary `InstallerType` for the winget submission (NSIS is the more common winget-pkgs convention for Tauri apps and matches the "no admin required, per-user" default; MSI if perMachine/enterprise silent-uninstall matters more — see Pitfall 4). Keep both artifacts on the GitHub Release for users who download directly, but don't put both into the winget manifest for the same architecture unless you've explicitly tested and accepted the upgrade-path asymmetry.

**Warning signs:**
Manifest PR review comment "Number of InstallerUrls are not equal" (a real recurring winget-releaser failure mode) after a build only produces one type for one arch; users reporting `winget upgrade` fails after previously installing via the "other" type.

**Phase to address:**
Manifest-authoring phase (installer-type decision), enforced again at Release-automation phase (CI must consistently produce the chosen type/arch set every release).

---

### Pitfall 6: Installer hash mismatch from mutable/re-uploaded release assets

**What goes wrong:**
`SHA256InstallerHash` in the manifest is pinned to one specific file. If a GitHub Release asset is ever re-uploaded/replaced after the tag is published (re-run a failed CI job, "fix" a corrupted artifact in place, manually swap a file), the byte content changes but the URL doesn't — winget then downloads a file whose hash doesn't match the manifest and refuses to install ("Installer hash does not match"). This is one of the most common recurring winget-pkgs issue reports.

**Why it happens:**
GitHub Releases don't enforce asset immutability, and a "hotfix re-upload to the same tag" feels harmless from the release-author's side but breaks every downstream consumer pinned to that hash (winget, but also any third-party mirrors).

**How to avoid:**
Treat published release assets as immutable once the SHA256 is computed and embedded in a winget manifest. If a build artifact is wrong, ship a **new version/tag**, never overwrite an existing release asset in place. Compute the SHA256 in the same CI job that uploads the asset (not a later re-download step) so the recorded hash always matches exactly what's public.

**Warning signs:**
Any manual "delete and re-upload asset" step in a runbook; CI re-run of a release job after partial failure without bumping the version.

**Phase to address:**
Release-automation phase.

---

### Pitfall 7: winget-releaser cannot bootstrap the very first submission

**What goes wrong:**
`winget-releaser` explicitly requires that **at least one version of the package already exists in winget-pkgs** — it uses the prior manifest as a template/diff base for new versions. Since this is ledgrrr's first-ever public release, wiring `winget-releaser` into CI from day one and expecting it to "just work" on the first tag will fail. The first submission must be authored and PR'd manually (or via `wingetcreate`/`komac`), merged, and only then does the automated releaser have something to build on.

**Why it happens:**
The action is an "update an existing entry" tool, not a "create new package" tool — this is undocumented as a hard blocker in most quickstart guides people copy from.

**How to avoid:**
Sequence the roadmap explicitly: (1) manually author + submit the v1.3 initial manifest with `wingetcreate` or `komac`, get it merged by a moderator; (2) only after merge, add `winget-releaser` to the CI pipeline for all subsequent versions. Don't build the CI automation step before the manual bootstrap PR is merged — it has nothing to act on.

**Warning signs:**
`winget-releaser` job fails immediately on first run with an error implying it can't find the package/prior manifest, or with "package doesn't exist in winget-pkgs."

**Phase to address:**
Submission phase (manual bootstrap) precedes Release-automation phase (ongoing `winget-releaser` wiring) — this pitfall is really about **phase ordering** in the roadmap itself.

---

### Pitfall 8: SmartScreen friction is inherent, resets every release, and has no guaranteed fix

**What goes wrong:**
For unsigned executables, SmartScreen application reputation accrues **per file hash**, not per publisher/product. Microsoft's own documentation acknowledges "an unsigned application that is updated regularly will appear as multiple distinct programs that will have to build reputation individually." Practically: every single new version's installer is a brand-new unknown file to SmartScreen and can trigger the full "Windows protected your PC" blocking dialog again, even if a prior version built up trust. This is expected steady-state behavior, not a bug to "fix" once.

**Why it happens:**
SmartScreen has no persistent unsigned-publisher identity to attach reputation to — only a code-signing certificate provides that continuity (and even EV certs no longer grant *immediate* reputation as of recent Microsoft policy changes).

**How to avoid (mitigations short of a cert):**
- Submit each release asset to Microsoft Security Intelligence (https://www.microsoft.com/en-us/wdsi/filesubmission) for a false-positive/reputation review immediately after publishing — best-effort, not guaranteed, but costs nothing.
- Set user expectations explicitly: README/release notes should pre-empt the SmartScreen dialog ("Windows may show 'Windows protected your PC' — click **More info → Run anyway**") rather than let first-time users assume the app is broken/malicious and abandon install.
- Budget for a standard (OV) code-signing certificate as a follow-up milestone if adoption/trust friction becomes a real blocker — do not treat "submit to MS and wait" as a permanent substitute for signing if the app grows beyond a small technical audience.
- Do not self-sign as a workaround — an untrusted self-signed cert is often *worse* UX than unsigned (still blocked, now with a "the publisher couldn't be verified" flavor) with no reputation benefit.

**Warning signs:**
Support requests/issues along the lines of "is this a virus?" or "SmartScreen won't let me install" after every release, not just the first one.

**Phase to address:**
Post-release/maintenance phase — this should be a documented, expected, recurring operational step (submit-for-review after each release), not a one-time launch task. Set expectations in the Manifest-authoring/Submission phase docs so it isn't a surprise later.

---

### Pitfall 9: Tauri's UpgradeCode is derived from productName — renaming breaks all future upgrades silently

**What goes wrong:**
Tauri's WiX/MSI bundler generates the `UpgradeCode` deterministically as a UUID v5 of `<productName>.exe.app.x64` (unless explicitly overridden via `WixSettings.upgrade_code`/`tauri.conf.json` `bundle.windows.wix.upgradeCode`). This is good — it's why upgrades work across versions without extra config — but it also means the code is a hidden function of `productName`. If `productName` in `tauri.conf.json` is ever changed after the first public/winget release (rebrand, typo fix, casing change), the UpgradeCode silently changes too, and every subsequent MSI is treated by Windows as an unrelated product: old and new versions install side-by-side instead of upgrading, and winget's version-tracking breaks with no error message pointing at the real cause.

**Why it happens:**
The productName→UpgradeCode derivation is convenient default behavior that most teams never need to think about — until a rename happens for unrelated (branding/UX) reasons and nobody connects it to the installer identity layer.

**How to avoid:**
Treat `productName` (currently `"ledgrrr"` in `crates/ledgerr-host/tauri.conf.json`) as a **frozen identity field** the moment the first winget manifest ships. If a rename is ever needed, explicitly pin `upgradeCode` in `tauri.conf.json` to the UUID that was already derived from the original name, rather than letting it silently regenerate. Document this constraint next to the existing `identifier` field in the config.

**Warning signs:**
Any PR touching `productName` in `tauri.conf.json` after v1.3 ships without a corresponding explicit `upgradeCode` pin; users reporting two copies of ledgrrr in Add/Remove Programs after an update.

**Phase to address:**
Manifest-authoring phase (freeze the identity fields as an explicit, documented decision before first submission).

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|-----------------|------------------|
| Submit only NSIS to winget, skip MSI entirely | Simpler manifest, avoids Pitfall 5/4 ambiguity | No clean perMachine/enterprise silent-uninstall story if ever needed | v1.3 MVP; revisit only if an enterprise/fleet-deploy request appears |
| Keep default `webviewInstallMode: downloadBootstrapper` | Smallest installer, least CI/storage cost | Silent `winget install` can fail on machines without internet access to Microsoft's WebView2 CDN at install time (locked-down corporate/offline hosts) | Acceptable given WebView2 ships in-box on Win10/11; revisit if offline-install support is ever a stated requirement |
| Manual first winget-pkgs submission via `wingetcreate`/`komac` instead of building full submission automation up front | Unblocks the very first release (Pitfall 7 makes this mandatory anyway) | One-time manual step must not become the permanent process — future releases need `winget-releaser` wired in immediately after | Always acceptable for the bootstrap PR; never acceptable as the ongoing per-release process |
| No code-signing certificate | $0 cost, ships now | Recurring SmartScreen friction on every release (Pitfall 8), erodes user trust, some corporate SmartScreen/App Control policies may block install outright | Acceptable for v1.3 first-release/small-audience; explicitly revisit cert budget once adoption grows |

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|-----------------|-------------------|
| `winget-releaser` action | Wiring it to fire on every `release: published`, including odd-minor "dev" pre-releases | Gate the workflow on the same stable/dev parity check already in `release.yml` before invoking the action |
| `winget-releaser` action | Assuming it can create a brand-new package entry on first use | Manually bootstrap the first manifest via `wingetcreate`/`komac`, merge it, *then* add the action |
| `winget-releaser` `installers-regex` | Leaving the default `.(exe\|msi\|msix\|appx)(bundle){0,1}$`, which matches both the NSIS `.exe` and the `.msi` from the same release, generating an unintended dual-installer manifest | Explicitly scope `installers-regex` to only the chosen installer type/filename pattern (see Pitfall 5) |
| GitHub Releases (asset hosting) | Re-uploading/replacing a release asset in place after the tag is published | Treat published assets as immutable; ship a new version if a build was wrong |
| winget-pkgs PR review | Assuming CI-green (`winget validate`) means the PR will be approved quickly | `winget validate` only catches schema-level issues; a human moderator still reviews before merge — build in review-latency buffer, don't block a release announcement on same-day merge |

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|-----------------|
| Unbounded `max-versions-to-keep` (default 0 = keep all) in `winget-releaser` | winget-pkgs manifest history grows every release forever, slower PR diffs over time | Set a sane `max-versions-to-keep` (e.g. 5-10) once release cadence stabilizes | Noticeable after dozens of releases; not a v1.3 launch concern |
| Full `offlineInstaller`/`fixedVersion` WebView2 mode (+127-180MB) chosen "just to be safe" | Installer size balloons, CI artifact upload/download time and GitHub Release storage grow | Default (`downloadBootstrapper`) is fine given Win10/11 ship WebView2 in-box; only switch modes if offline-install becomes a real requirement | Immediately, if chosen without a specific offline-install need |

## Security Mistakes

| Mistake | Risk | Prevention |
|---------|------|------------|
| Computing the SHA256 hash from a locally re-downloaded copy of the release asset instead of the exact bytes uploaded to GitHub | A tampered-in-transit or accidentally-different file could pass hash validation against the wrong reference, or a legitimate mismatch masks a real supply-chain issue | Compute and record the hash in the same CI job/step that produces and uploads the artifact, before any network round-trip |
| Training users to routinely click through SmartScreen "Run anyway" without any other verification | Normalizes bypassing a real phishing/malware defense; users can't distinguish ledgrrr's benign unsigned warning from an actually malicious unsigned file | Give users a way to verify authenticity out-of-band (publish the SHA256 hash in release notes / README so a technical user can `Get-FileHash` and compare) rather than "just click through" as the only guidance |
| Passing unsanitized CI-derived values (tag names, version strings) into `InstallerSwitches.Custom` or shell-invoked `msiexec`/NSIS command lines in release-automation scripts | Command-injection risk if any of those values are ever influenced by an external actor (e.g., a PR title, a forked-repo release) | Keep installer switches static/hardcoded in the manifest; never interpolate untrusted CI context into command-line arguments |

## UX Pitfalls

| Pitfall | User Impact | Better Approach |
|---------|-------------|-------------------|
| No pre-emptive warning about SmartScreen before first install | Users see "Windows protected your PC," assume the app is malware, and abandon install | Document the expected SmartScreen dialog and exact click-through steps in the README/release notes, right next to the install command |
| `perMachine`/`both` NSIS install mode triggering a UAC elevation prompt during a scripted or remote (SSH/RDP-less) `winget install` | Silent/automated installs hang indefinitely waiting for an elevation dialog no one can see | Default to `currentUser` install mode (Tauri's NSIS default) unless perMachine is a deliberate, documented choice, and declare `Scope: User` explicitly in the manifest so winget doesn't assume machine-wide |
| Users who already manually downloaded/installed the MSI/NSIS from GitHub before it's on winget | `winget upgrade`/`winget list` doesn't recognize the pre-existing install as "the winget package" (ARP entry not linked to a winget source), so they see no updates or get confusing dual-entry state | Document in release notes that early adopters who installed pre-winget should uninstall and reinstall via `winget install` once available, to get proper update tracking |

## "Looks Done But Isn't" Checklist

- [ ] **GitHub Release artifact hashing:** Often computed from a local build copy — verify the SHA256 in the manifest matches `Invoke-WebRequest`/`curl` downloading the *actual public URL*, not the CI runner's local file.
- [ ] **`winget validate` passing:** Often treated as "ready to submit" — verify with an actual `winget install <id>` **and** `winget uninstall <id>` on a clean Windows VM/sandbox before opening the PR; validate only checks schema, not real install/uninstall behavior.
- [ ] **Silent install switches:** Often assumed correct because Tauri "supports" `/S` — verify by literally running the silent command and confirming zero UI appears and the exit code is 0.
- [ ] **Version-channel gating:** Often missing until the first accidental dev-release leaks — verify the winget-publish trigger condition explicitly mirrors `release.yml`'s stable/pre-release parity logic before turning on automation.
- [ ] **Uninstall path:** Often untested — verify `winget uninstall` actually completes cleanly given `AppsAndFeaturesEntries` + the chosen installer type (see Pitfall 4), especially if MSI/NSIS choice changes later.
- [ ] **Identity-field freeze:** Often undocumented — verify `productName`/`identifier`/(and, if MSI is used) `upgradeCode` are explicitly called out as "do not change without a migration plan" somewhere a future contributor will actually read.

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|----------------|------------------|
| Hash mismatch from a swapped release asset | LOW | Ship a new patch version/tag with a fresh build; submit a manifest PR for the new version with the correct hash — never try to "patch" an existing merged manifest's hash in place |
| Dev/pre-release accidentally published to winget-pkgs | MEDIUM | Submit a follow-up PR to winget-pkgs removing that version's manifest folder (or superseding it with the next stable version quickly) and fix the CI gating (Pitfall 1) before any further releases |
| ProductName/UpgradeCode drift breaks the upgrade chain (Pitfall 9) | HIGH | No clean automatic fix — requires either restoring the original `productName` or explicitly pinning `upgradeCode` to the historical value, plus release notes telling any already-diverged users to manually uninstall the orphaned old copy |
| Wrong/ambiguous installer type discovered after the manifest is merged (Pitfall 5) | MEDIUM | Submit a manifest PR correcting the `InstallerType`/removing the unintended node for the *next* version; the already-merged version stays as-is (manifests for already-released versions are rarely edited) |
| SmartScreen blocking a large fraction of installs | ONGOING/MEDIUM | Resubmit the current release hash to Microsoft Security Intelligence for review; if friction persists across several releases, escalate to budgeting an OV/EV signing certificate |

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|-------------------|----------------|
| 1. Dev/pre-release leakage into winget | Release-automation phase | winget-publish workflow's trigger condition references the same `is_stable`/parity output as `release.yml`; a test odd-minor tag does *not* invoke `winget-releaser` |
| 2. Generic productName collision | Manifest-authoring phase | Clean-VM `winget list` after install shows no cross-match to an unrelated catalog package |
| 3. Missing silent-install switches | Manifest-authoring phase | `winget validate` clean; manual silent install shows zero UI, exit code 0 |
| 4. AppsAndFeaturesEntries breaks NSIS silent uninstall | Manifest-authoring phase | Clean-VM `winget uninstall ledgrrr` completes without a stuck/interactive prompt |
| 5. Dual MSI+NSIS ambiguity / one-way upgrade | Manifest-authoring phase | Manifest contains exactly one `InstallerType` per architecture; `installers-regex` scoped accordingly in Release-automation phase |
| 6. Hash mismatch from mutable assets | Release-automation phase | CI job computes/uploads hash and asset atomically; runbook explicitly forbids in-place asset replacement |
| 7. winget-releaser can't bootstrap first submission | Submission phase (ordering) | Roadmap sequences manual `wingetcreate`/`komac` PR + merge *before* `winget-releaser` is added to CI |
| 8. SmartScreen friction on unsigned installer | Post-release/maintenance phase | README/release notes contain the SmartScreen click-through guidance; a per-release "submit to MS for review" step exists in the runbook |
| 9. UpgradeCode drift from productName changes | Manifest-authoring phase | `productName`/`identifier`(/`upgradeCode` if MSI) documented as frozen identity fields; any future rename PR includes an explicit `upgradeCode` pin |

## Sources

- [Manifest-Validation-Error · Issue #984 · microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs/issues/984)
- [winget-pkgs installer.md schema (1.12.0)](https://github.com/microsoft/winget-pkgs/blob/master/doc/manifest/schema/1.12.0/installer.md)
- [winget-pkgs-submission-test/Troubleshoot.md](https://github.com/microsoft/winget-pkgs-submission-test/blob/master/Troubleshoot.md)
- [Validation of ProductCode · Issue #140 · microsoft/winget-create](https://github.com/microsoft/winget-create/issues/140)
- [vedantmgoyal9/winget-releaser (README + action.yml)](https://github.com/vedantmgoyal9/winget-releaser)
- [[Question]: "Number of InstallerUrls are not equal" · Issue #19 · vedantmgoyal2009/winget-releaser](https://github.com/vedantmgoyal2009/winget-releaser/issues/19)
- [Installed tauri-app with default settings is detected as different Winget application · Issue #6040 · microsoft/winget-cli](https://github.com/microsoft/winget-cli/issues/6040)
- [Blur-AutoClicker windows-release-trust.md (unsigned Tauri SmartScreen playbook)](https://github.com/Blur009/Blur-AutoClicker/blob/main/docs/windows-release-trust.md)
- [Windows Code Signing | Tauri v2 docs](https://v2.tauri.app/distribute/sign/windows/)
- [Windows Installer | Tauri v2 docs](https://v2.tauri.app/distribute/windows-installer/)
- [Release and update .exe (nsis) & .msi packages simultaneously · Discussion #8963 · tauri-apps/tauri](https://github.com/tauri-apps/tauri/discussions/8963)
- [How does winget choose an MSI over an EXE when the manifest has both? · Discussion #3497 · microsoft/winget-cli](https://github.com/microsoft/winget-cli/discussions/3497)
- [Silent uninstallation not working for packages with AppsAndFeaturesEntries · Issue #4068 · microsoft/winget-cli](https://github.com/microsoft/winget-cli/issues/4068)
- [How does WinGet identify packages for applications installed on a system · Discussion #3033 · microsoft/winget-cli](https://github.com/microsoft/winget-cli/discussions/3033)
- [winget-pkgs/doc/Moderation.md](https://github.com/microsoft/winget-pkgs/blob/master/doc/Moderation.md)
- [Winget Explained – Installer hash does not match](https://blog.intunepckgr.com/2026/01/21/winget-explained-installer-hash-does-not-match/)
- [SmartScreen reputation for Windows app developers | Microsoft Learn](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation)
- [Windows Apps PSA: EV Certs do not grant immediate reputation anymore | ToDesktop Blog](https://www.todesktop.com/blog/posts/windows-apps-psa-ev-certs-do-not-grant-immediate-reputation-anymore)
- [WixSettings in tauri_bundler::bundle — docs.rs](https://docs.rs/tauri-bundler/latest/tauri_bundler/bundle/struct.WixSettings.html)
- Repo context reviewed directly: `.github/workflows/release.yml` (stable/dev parity logic), `crates/ledgerr-host/tauri.conf.json` (productName/identifier/bundle config), `.github/workflows/ci.yml` (no existing Windows build job at time of research)

---
*Pitfalls research for: Windows winget distribution of the ledgrrr Tauri desktop app (v1.3 milestone)*
*Researched: 2026-08-08*
