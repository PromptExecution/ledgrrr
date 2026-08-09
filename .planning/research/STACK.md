# Stack Research

**Domain:** Windows package-manager distribution (winget) for an existing Tauri v2 desktop app
**Researched:** 2026-08-08
**Confidence:** HIGH (versions verified via GitHub API / raw file fetch on 2026-08-08, not training memory)

**Scope note:** This milestone (v1.3) starts from an already-working `windows-latest` CI job that
runs `cargo tauri build --bundles msi,nsis` on the native MSVC target and produces unsigned MSI +
NSIS bundles. Nothing below touches that build step. This file covers only the NEW layer: turning
those artifacts into a GitHub Release with a computed hash, and turning that release into a
winget-pkgs submission.

## Recommended Stack

### Core Technologies

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| winget-pkgs manifest schema | `1.10.0` (stable baseline; repo now also has up to `1.12.0`/`1.28.0` dirs, but `1.10.0` is still what current-generation tooling emits and what most active manifests use, e.g. Microsoft.PowerToys 0.99.1 uses `1.12.0`, LocalSend historically `1.4.0`–`1.9.x`) | Defines the 3-file manifest trio (`version.yaml`, `<locale>.locale.yaml`, `installer.yaml`) submitted to `microsoft/winget-pkgs` | This is the wire format winget itself validates against. You do not hand-author `ManifestVersion` in practice — `wingetcreate`/`komac` stamp it — but the roadmap needs to know the shape it will generate. |
| `wingetcreate` (winget-create) | `v1.12.13.0` (released 2026-07-23, verified via `gh api repos/microsoft/winget-create/releases/latest`) | Microsoft's official CLI for generating/updating/submitting winget-pkgs manifests | Needed for the **first-ever submission** of `ventures.elastic.ledgrrr` — see gotcha below: the community automation action refuses to bootstrap a brand-new package. |
| `vedantmgoyal9/winget-releaser@v2` (renamed from `vedantmgoyal2009`; tag `v2`, action.yml current on `main`, verified via `gh api repos/vedantmgoyal9/winget-releaser`) | `v2` | GitHub Action that auto-generates + submits a manifest-update PR to winget-pkgs on every subsequent GitHub Release | Handles the ongoing/repeat-release case with zero manual `wingetcreate` runs after the initial bootstrap. Wraps `komac` internally. |

### Supporting Libraries / Actions

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `softprops/action-gh-release` | `v3.0.2` (released 2026-07-13, verified via `gh api`) | Creates/updates the GitHub Release and attaches the MSI + NSIS build artifacts | Add as a step in the existing Windows CI job (or a new release-publish job) right after `cargo tauri build` — it needs `files:` pointing at the `*.msi` and `*-setup.exe` bundle output paths. |
| `komac` | `v2.16.0` (latest, verified via `gh api repos/russellbanks/Komac`) | Manifest generation/update + winget-pkgs PR creation engine | Not called directly — it's what `winget-releaser` invokes under the hood. Useful to know for debugging action failures (error messages come from `komac`), and it's a valid manual fallback CLI if `wingetcreate` output needs equivalent handling on non-Windows CI. |
| PowerShell `Get-FileHash -Algorithm SHA256` or `winget hash <installer>` | built-in / winget-cli `v1.29.280` | Computes `InstallerSha256` | Only needed if hand-authoring/verifying a manifest locally; `wingetcreate`/`winget-releaser` compute this automatically from the downloaded release asset. |

### Development Tools

| Tool | Purpose | Notes |
|------|---------|-------|
| `winget validate <manifest>` / `winget install --manifest <manifest>` | Local pre-submission check | Run before opening a PR by hand (bootstrap submission) — catches `Manifest-Validation-Error` and SHA256 mismatches that the winget-pkgs bot would otherwise reject on. |
| Classic GitHub PAT (`public_repo` scope) | Required token for both `wingetcreate submit` and `winget-releaser` | **Fine-grained PATs are explicitly unsupported** by `winget-releaser` (tracked in vedantmgoyal9/winget-releaser#172). Store as a repo secret, e.g. `WINGET_TOKEN`. |
| Fork of `microsoft/winget-pkgs` under the `PromptExecution` org (or a bot account) | Required staging ground for both tools | `winget-releaser`'s `fork-user` input defaults to `github.repository_owner`; must exist before the action runs. |

## Installation

```yaml
# .github/workflows — additions to the existing Windows CI/release path.
# No new Cargo/npm dependencies. These are GitHub Actions + one-time CLI usage.

# 1) In the existing windows-latest job, after `cargo tauri build --bundles msi,nsis`:
- name: Publish GitHub Release with installers
  uses: softprops/action-gh-release@v3.0.2
  with:
    files: |
      target/**/release/bundle/msi/*.msi
      target/**/release/bundle/nsis/*.exe
    fail_on_unmatched_files: true

# 2) One-time, LOCAL/manual bootstrap (Windows machine, first release only):
#    wingetcreate new ventures.elastic.ledgrrr `
#      -u https://github.com/<org>/ledgrrr/releases/download/vX.Y.Z/ledgrrr_X.Y.Z_x64-setup.exe `
#      -t <PAT>
#    (installs via: winget install --id wingetcreate -e   OR   download aka.ms/wingetcreate/latest)

# 3) After the package exists in winget-pkgs, add a repo-level workflow for all FUTURE releases:
name: Publish to WinGet
on:
  release:
    types: [released]
jobs:
  publish:
    runs-on: windows-latest   # winget-releaser's own examples use ubuntu-slim/windows runners interchangeably; pwsh steps require a runner with PowerShell (windows-latest is safest, ubuntu-latest also has pwsh preinstalled)
    steps:
      - uses: vedantmgoyal9/winget-releaser@v2
        with:
          identifier: ventures.elastic.ledgrrr
          installers-regex: '\.(exe|msi)$'
          token: ${{ secrets.WINGET_TOKEN }}
```

## Alternatives Considered

| Recommended | Alternative | When to Use Alternative |
|-------------|-------------|--------------------------|
| `softprops/action-gh-release@v3` | `gh release create` raw CLI in a bash step | If you want zero third-party Action dependency; more boilerplate for multi-file glob uploads and idempotent re-runs. `action-gh-release` already handles "release exists, add assets" cleanly, which matters since this repo's `release.yml` already creates the release via Cocogitto/`gh` separately — `action-gh-release` can also update an existing release rather than only create one. |
| `wingetcreate` for bootstrap, `winget-releaser` for ongoing | `wingetcreate update` on every release via CI (skip winget-releaser entirely) | Valid, and arguably simpler (one tool, no action dependency) — but `winget-releaser`/`komac` add version pruning (`max-versions-to-keep`) and are purpose-built for the "on release published" trigger with less workflow code. Either is defensible; `winget-releaser` is the more common community pattern in 2026 (used by e.g. WSL-UI's Tauri app). |
| MSI + NSIS both submitted as separate `Installers` architecture/type entries in one manifest | NSIS-only submission | winget-pkgs manifests support multiple installer entries per architecture with different `InstallerType`; submitting both isn't required. Simpler to pick ONE canonical type for the winget listing (see recommendation below) rather than dual-maintain both in the manifest. |

## What NOT to Use

| Avoid | Why | Use Instead |
|-------|-----|--------------|
| mingw/cross-compiled Windows build from Linux/WSL CI | Explicitly out of scope per this project's convention — CI already uses a native `windows-latest` MSVC runner; introducing a cross-compile path adds a second, divergent build surface with no benefit since native CI already works | Keep building only on `windows-latest` with `x86_64-pc-windows-msvc`, as today |
| Fine-grained GitHub PAT for `winget-releaser`/`wingetcreate` | Explicitly unsupported (microsoft/winget-create and vedantmgoyal9/winget-releaser both require classic PAT with `public_repo` scope) | Classic PAT stored as `WINGET_TOKEN` secret |
| Trying to auto-bootstrap the very first winget-pkgs submission via `winget-releaser` | The action hard-fails with `::error::Package <id> does not exist in the winget-pkgs repository` — it only works once at least one version is already merged | `wingetcreate new` (or a manual PR) for the first version only |
| Code-signing as a blocking prerequisite for this milestone | No cert is available yet, and winget-pkgs does accept unsigned installers (SmartScreen/Defender reputation issues are a real but separate risk, not a submission blocker) | Submit unsigned now; track code-signing as a future milestone that will improve first-run SmartScreen UX and reduce false-positive `Binary-Validation-Error` risk, not as a gate on shipping to winget |
| Default Tauri `product_name`-derived WiX `UpgradeCode` left unexamined | Tauri's default upgrade-code derivation (`Uuid::new_v5` over `"{product_name}.exe.app.x64"`) has a confirmed real-world collision case (tauri-apps/tauri#14968: default `tauri-app` name collided with an unrelated winget package, `maqibin.MDXNotes`, confusing `winget list`/upgrade matching) | ledgrrr's `productName` is already the distinctive string `"ledgrrr"` (not the Tauri default), which avoids the specific reported collision — but confirm the resulting `UpgradeCode`/`ProductCode` emitted by `cargo tauri build` is stable across builds before wiring it into `ProductCode`/`AppsAndFeaturesEntries` in the manifest, since winget uses it for upgrade detection |

## Stack Patterns by Variant

**If choosing which installer type to register as the winget-listed installer (MSI vs NSIS):**
- Prefer **NSIS** (`InstallerType: nullsoft` in the manifest — note winget's enum name for NSIS is `nullsoft`, *not* `nsis`)
- Because: NSIS is Tauri's default/primary Windows bundle target in v2, is smaller, and is the type used in the one concrete Tauri-app-on-winget precedent found (WSL-UI, a Tauri app, submits its NSIS `-setup.exe` installers to winget via `wingetcreate`). Both MSI and NSIS are equally valid `InstallerType` values in the schema and both are well-supported by the winget-pkgs validation bot; there is no hard technical blocker to using MSI instead. Silent-install switches differ (NSIS: `/S` uppercase; MSI: standard `msiexec /quiet`) and only matter if you hand-specify `InstallerSwitches` — `wingetcreate`/`komac` infer sensible defaults for both types automatically.
- If both bundles are useful to keep building for other reasons (e.g. enterprise MSI/GPO deployment), that's fine — just pick one as the canonical winget submission to avoid double-maintaining manifest entries; NSIS is the simpler pick since it's already Tauri's default in `tauri.conf.json` (`bundle.targets: "all"` currently builds both, so no config change needed regardless of which one CI later selects for the release-asset regex).

**If this is the very first winget submission for `ventures.elastic.ledgrrr`:**
- Use `wingetcreate new` run manually/locally (or as a one-off `workflow_dispatch` job) against the first tagged release's installer URL
- Because `winget-releaser` (and by extension `komac`) categorically refuse to create a package that doesn't already exist upstream — this is a hard gate in the action's first script step, not a configuration option

**If a future milestone adds code signing:**
- No stack change needed here — same manifest fields (`InstallerSha256`, `SignatureSha256` only applies to MSIX) — signing only affects SmartScreen reputation warming, not the winget-pkgs submission mechanics
- Because winget-pkgs already accepts unsigned installers today; signing is a UX/trust improvement layered on top, not a schema or tooling change

## Version Compatibility

| Package A | Compatible With | Notes |
|-----------|------------------|-------|
| `softprops/action-gh-release@v3.0.2` | `windows-latest` / `ubuntu-latest` runners | Runner-agnostic; run it as the final step of the existing Windows build job, or in a separate `ubuntu-latest` job that downloads the Windows job's `actions/upload-artifact` output — either works, but keeping it in the same job avoids an extra artifact round-trip. |
| `vedantmgoyal9/winget-releaser@v2` | GitHub PowerShell (`pwsh`) runners | Composite action's steps are all `shell: pwsh` — works on `windows-latest`, `ubuntu-latest`, and `macos-latest` runners since GitHub-hosted runners all ship PowerShell Core; does not require an actual Windows runner to execute (it never runs the installer, only manipulates manifests/PRs via `komac`). |
| `wingetcreate v1.12.13.0` | .NET Runtime 6.0+ | Needs a Windows machine (or `windows-latest` runner) to run interactively for `new`/`update` — this is the one piece of this workflow that genuinely wants a Windows execution environment, matching the existing `windows-latest` CI convention already used for the Tauri build. |
| winget-pkgs manifest schema `1.10.0`–`1.12.0` | winget-cli `v1.29.280` (latest client, verified via `gh api repos/microsoft/winget-cli`) | Current client supports schema versions well above what `wingetcreate`/`komac` currently emit; no version-skew risk for this milestone. |

## Sources

- https://github.com/microsoft/winget-pkgs/tree/master/doc/manifest/schema — enumerated schema version directories (1.0.0 → 1.28.0); confirmed via `gh api repos/microsoft/winget-pkgs/contents/doc/manifest/schema`
- https://raw.githubusercontent.com/microsoft/winget-pkgs/master/doc/manifest/schema/1.10.0/installer.md — full installer.yaml field list, minimal + complex examples
- https://raw.githubusercontent.com/microsoft/winget-pkgs/master/doc/manifest/schema/1.10.0/version.md — version.yaml required fields (`PackageIdentifier`, `PackageVersion`, `DefaultLocale`)
- https://raw.githubusercontent.com/microsoft/winget-pkgs/master/doc/manifest/schema/1.10.0/defaultLocale.md — locale.yaml required fields (`Publisher`, `PackageName`, `License`, `ShortDescription`, etc.)
- https://raw.githubusercontent.com/microsoft/winget-cli/master/schemas/JSON/manifests/v1.10.0/manifest.installer.1.10.0.json — confirmed `InstallerType` enum: `msix, msi, appx, exe, zip, inno, nullsoft, wix, burn, pwa, portable` (NSIS = `nullsoft`)
- https://raw.githubusercontent.com/microsoft/winget-pkgs/master/manifests/m/Microsoft/PowerToys/0.99.1/Microsoft.PowerToys.installer.yaml — real-world current manifest example, `ManifestVersion: 1.12.0`, `ReleaseDate: 2026-04-29`
- `gh api repos/microsoft/winget-create/releases/latest` — wingetcreate `v1.12.13.0`, published 2026-07-23
- `gh api repos/vedantmgoyal9/winget-releaser/tags` and `.../releases` — confirmed repo renamed from `vedantmgoyal2009` to `vedantmgoyal9`; current usable tag `v2`
- https://raw.githubusercontent.com/vedantmgoyal9/winget-releaser/main/action.yml — full input list (`identifier`, `version`, `installers-regex`, `max-versions-to-keep`, `release-repository`, `release-tag`, `token`, `fork-user`), confirmed uses `komac` internally, confirmed default `installers-regex` matches `exe|msi|msix|appx(bundle)?`
- https://raw.githubusercontent.com/vedantmgoyal9/winget-releaser/main/README.md — confirmed **no Partner Center identity required**; confirmed classic PAT with `public_repo` scope required (fine-grained unsupported, tracked in issue #172); confirmed hard requirement that ≥1 version already exists in winget-pkgs before the action can run
- `gh api repos/softprops/action-gh-release/releases/latest` — `v3.0.2`, published 2026-07-13
- `gh api repos/microsoft/winget-cli/releases/latest` — client `v1.29.280`, published 2026-06-24
- `gh api repos/russellbanks/Komac/releases/latest` — `v2.16.0`, published 2026-03-29 (the engine `winget-releaser` wraps)
- https://raw.githubusercontent.com/microsoft/winget-pkgs-submission-test/master/Troubleshoot.md — documented bot error classes: `Manifest-Validation-Error`, `Binary-Validation-Error` (SHA256 mismatch, bad URL, or malware false-positive), `SmartScreen-Validation-Error` (download URL reputation), `Internal-Error`
- https://github.com/tauri-apps/tauri/issues/14968 — confirmed real collision bug: default Tauri `product_name` ("tauri-app") produces a WiX `UpgradeCode` that collided with an unrelated winget package (`maqibin.MDXNotes`), breaking `winget list`/upgrade detection for both
- https://wsl-ui.octasoft.co.uk/blog/building-wsl-ui-winget — concrete precedent: a Tauri app publishing NSIS (`nullsoft`) installers to winget via `wingetcreate`, confirms predictable-URL-per-version and uppercase-SHA256 requirements in practice
- `crates/ledgerr-host/tauri.conf.json` (this repo) — confirmed `productName: "ledgrrr"`, `identifier: "ventures.elastic.ledgrrr"`, `bundle.targets: "all"` (already builds both MSI and NSIS, no config change needed for either choice)

---
*Stack research for: Windows Distribution & Winget Packaging (milestone v1.3)*
*Researched: 2026-08-08*
