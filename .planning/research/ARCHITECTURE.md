# Architecture Research

**Domain:** CI/CD release automation + winget package-manager distribution for a Tauri desktop app vendored inside a monorepo
**Researched:** 2026-08-08
**Confidence:** HIGH (based on direct inspection of the actual workflow files, `cog.toml`s, `tauri.conf.json`, crate layout, git remotes/visibility, and tag history in both repos — not generic winget docs)

> Note: this replaces the prior contents of this file (2026-03-28 core-domain architecture research for v1.0). That research is superseded/archived by the v1.0/v1.1 milestone artifacts under `.planning/milestones/`; this file now tracks the **current** v1.3 "Windows Distribution & Winget Packaging" milestone's research, per this project's `.planning/research/` convention of reflecting the active milestone.

## Repo Topology (load-bearing fact)

Two separate git repos, two separate tag namespaces, one GitHub Actions surface:

```
elasticdotventures/_b00t_          (outer monorepo, PUBLIC, remote: git@github.com:elasticdotventures/_b00t_.git)
├── .github/workflows/              <- ALL CI, including build-tauri-windows.yml, lives here
├── cog.toml                        <- tag_prefix "v", plain v* tags already used (see release.yml)
└── vendor/ledgrrr/  (git submodule)
    └── → PromptExecution/ledgrrr    (PUBLIC, remote: https://github.com/PromptExecution/ledgrrr.git)
        ├── cog.toml                 <- tag_prefix "v", generate_mono_repository_package_tags = true
        ├── .planning/PROJECT.md     <- v1.3 "Windows Distribution & Winget Packaging" milestone JUST kicked off
        └── crates/ledgerr-host/     <- the actual Tauri app now lives here (see below)
```

`cog bump --auto` (per `vendor/ledgrrr/CLAUDE.md`) is run **inside the ledgrrr submodule** and pushes a `vX.Y.Z` tag **into `PromptExecution/ledgrrr`**, not into the outer `_b00t_` repo. Pushing a tag in a submodule does **not** trigger workflows in the superproject — GitHub Actions only watches the repo it's literally hosted in. Since `.github/workflows/build-tauri-windows.yml` lives in `_b00t_`, a tag pushed to `ledgrrr` alone will never fire it. This is the single most important integration fact for this milestone: **the tag that triggers release CI must be pushed to `_b00t_` (the outer repo), separately from the `cog bump` tag pushed inside `ledgrrr`.**

## Stale-path finding (verified, must be fixed regardless of new work)

`vendor/ledgrrr` was checked out on branch `rebase/165-unify-desktop-server` (commit `f5ff4ac`) during this research, while the outer repo's `main` branch currently pins submodule commit `a233aa5`. Diffing crate layout across these points:

| Commit | `crates/ledgerr-tauri` exists? | Tauri app location |
|---|---|---|
| `a233aa5` (currently pinned by `_b00t_` main) | Yes | `crates/ledgerr-tauri` |
| `origin/main` of `ledgrrr` (and the `f5ff4ac` branch checked out locally) | **No** | Merged into `crates/ledgerr-host`, binary `host-tauri` (`crates/ledgerr-host/src/bin/tauri/main.rs`, `default-run = "host-tauri"`), config at `crates/ledgerr-host/tauri.conf.json` |

`.github/workflows/build-tauri-windows.yml` currently does:
```yaml
- name: Build MSI and NSIS installers
  run: |
    cd vendor/ledgrrr/crates/ledgerr-tauri
    cargo tauri build --bundles msi,nsis
```
This still works **today** only because the outer repo's submodule pointer hasn't been advanced past the crate rename. The `ledgrrr`-side commit history (`fd1d058 feat(tray): native Windows tray wired into ledgerr-tauri (#136)` → later merged into `ledgerr-host`) shows the rename already happened upstream. **Any submodule pointer bump done as part of this milestone will break the existing build step** unless `cd vendor/ledgrrr/crates/ledgerr-tauri` is changed to `cd vendor/ledgrrr/crates/ledgerr-host` (and the PR path-filters in the `on:` block updated to match, since they still reference `crates/ledgerr-tauri/**`). Treat this as a required "modify" item, not optional cleanup — it's on the critical path, not a nice-to-have.

`tauri.conf.json` (`crates/ledgerr-host/tauri.conf.json`): `"productName": "ledgrrr"`, `"identifier": "ventures.elastic.ledgrrr"`. Workspace version is `1.9.0` (`vendor/ledgrrr/Cargo.toml [workspace.package]`).

## Existing Precedent: `browser-ext-release.yml`

`.github/workflows/browser-ext-release.yml` is the only tag-triggered public-release pattern already in the monorepo. Relevant structure to mirror:

```yaml
on:
  push:
    tags: [ 'b00t-browser-ext-v*' ]   # product-scoped tag prefix — avoids colliding with plain v*
  release:
    types: [created]
...
- name: Create GitHub Release (on tag)
  if: startsWith(github.ref, 'refs/tags/b00t-browser-ext-v')
  uses: softprops/action-gh-release@v1
  with:
    files: |
      artifacts/*.zip
    name: b00t Browser Extension v${{ steps.version.outputs.version }}
    body: |
      ...
  env:
    GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

Note it does **not** declare an explicit `permissions:` block — relies on repo-default `GITHUB_TOKEN` write access. The new job should declare an explicit `permissions: contents: write` (least-privilege, and required if the org/repo default is ever locked down to read-only).

Also present, `.github/workflows/release.yml` (top-level workspace release) proves **plain `v*` tags are already in active use** in `_b00t_` (it creates `git tag -a "v${WORKSPACE_VERSION}"`). This confirms the collision risk named in the question is real, not hypothetical — a bare `v*` tag trigger on `build-tauri-windows.yml` would either double-fire alongside or create ambiguity with the top-level workspace release flow.

## Tag Naming Decision

Following the `b00t-browser-ext-v*` precedent exactly:

**`ledgrrr-desktop-v*`** (e.g. `ledgrrr-desktop-v1.9.0`)

Rationale:
- `ledgrrr-v*` alone risks ambiguity with `PromptExecution/ledgrrr`'s own internal `v*` tags if anyone ever cross-references tags between the two repos (different repos, so no literal collision, but confusing in shared search/branch-protection tooling and human comms).
- `-desktop-` scopes it explicitly to the Tauri/Windows artifact, leaving room for a future `ledgrrr-cli-v*` or `ledgrrr-mcp-v*` tag if other sub-artifacts of ledgrrr ever get their own release lane from the same monorepo.
- Matches the existing `<product>-<subcomponent>-v*` shape the repo already established with `b00t-browser-ext-v*`.

This tag is pushed to `_b00t_` (the outer repo), **not** `ledgrrr`. It is a distinct, manually-created (or automation-created, see Data Flow) tag from the `cog bump --auto` tag inside the submodule.

## Component Responsibilities — New vs Modified

| Component | New or Modified | What it owns |
|---|---|---|
| `.github/workflows/build-tauri-windows.yml` | **Modified** | Fix stale `crates/ledgerr-tauri` → `crates/ledgerr-host` path (see above, required regardless). Add `push: tags: ['ledgrrr-desktop-v*']` trigger alongside existing branch triggers. Add SHA256 + GitHub Release steps gated on the tag ref (mirroring `browser-ext-release.yml`'s `if: startsWith(github.ref, ...)` pattern) — this file stays a single file (see decision below). |
| A tag-triggered release job | **New** (as steps appended to the existing workflow file) | Compute SHA256 of `.msi` and `.exe` (NSIS) bundle outputs; create/attach a GitHub Release via `softprops/action-gh-release@v1` with `contents: write` permission. |
| `vendor/ledgrrr/xtask` (`xtask/src/publisher.rs`) | **Existing pattern to extend, not invent fresh** | Already implements a `release_tag` + `artifact_url` + `sha256` publish flow (`McpbPublisher`, used today for MCP-registry publishing via `gh release upload`). This is the established in-repo idiom for "hash + attach artifact to a release" — a new `xtask winget` subcommand computing SHA256 and rendering the winget manifest would be consistent with existing conventions rather than ad-hoc `shasum` lines pasted into YAML (contrast: `build-release.yml` in the outer repo does inline `shasum -a 256 ... > file.sha256`, which is the other acceptable precedent if xtask extension is judged out of scope for phase 1). |
| winget manifest directory (`manifests/<first-letter>/<Publisher>/<Package>/<version>/*.yaml`) | **New** | Three YAML files per winget-pkgs convention: `<PackageIdentifier>.yaml` (version manifest), `<PackageIdentifier>.installer.yaml`, `<PackageIdentifier>.locale.en-US.yaml`. Lives in a **fork of `microsoft/winget-pkgs`**, not in this repo — see Data Flow. |
| PackageIdentifier decision | **New (decision, not code)** | `tauri.conf.json`'s reverse-DNS identifier is `ventures.elastic.ledgrrr`; git ownership is split between `elasticdotventures` (monorepo host) and `PromptExecution` (ledgrrr's actual project home). winget convention is `Publisher.PackageName` matching the *project's* public identity, not the CI host repo. Needs an explicit decision before first submission — candidates: `PromptExecution.ledgrrr` (matches the project's real GitHub org, per `vendor/ledgrrr/CLAUDE.md`'s framing of ledgrrr as an independently reusable component) vs `ElasticVentures.ledgrrr`. This blocks manifest generation, not release automation — sequence it into phase 2, not phase 1. |
| winget-pkgs submission mechanism | **New** | Either manual (`wingetcreate submit` or hand-authored PR) for the first submission, or automated via `vedantmgoyal2009/winget-releaser` GitHub Action on subsequent releases. No existing reference to either tool was found anywhere in this monorepo or in `ledgrrr` (repo-wide grep came back empty) — this is genuinely new territory for the codebase, not an extension of an existing pattern. |

## Data Flow

```
[developer/agent, inside vendor/ledgrrr]
    cog bump --auto                       (bumps Cargo.toml versions, writes CHANGELOG.md,
                                            commits, tags vX.Y.Z INSIDE the ledgrrr repo)
    git push --follow-tags                (pushes to PromptExecution/ledgrrr — this tag
                                            does NOT reach _b00t_ and triggers nothing there)
        ↓
[maintainer, in the OUTER _b00t_ repo]
    1. bump the vendor/ledgrrr submodule pointer to the new commit
    2. git tag ledgrrr-desktop-v<version>  (NEW, distinct tag, in _b00t_)
    3. git push origin main --tags
        ↓  (tag push triggers workflow)
[.github/workflows/build-tauri-windows.yml, windows-latest runner]
    checkout _b00t_ @ tag → init vendor/ledgrrr submodule
    cargo tauri build --bundles msi,nsis   (from crates/ledgerr-host after path fix)
        ↓
    installer artifacts: *.msi, *.exe (NSIS)
        ↓
    [NEW STEP] sha256sum on both files
        ↓
    [NEW STEP] softprops/action-gh-release@v1
        → creates a GitHub Release on _b00t_
        → attaches .msi, .exe, and .sha256 sidecar files
        → this Release's asset URLs are the "real signed URL+hash" winget needs
        ↓
[MANUAL, gated — phase 2 only]
    author winget manifest (PackageIdentifier, InstallerUrl = the Release asset URL,
    InstallerSha256 = computed hash) → PR to microsoft/winget-pkgs
        ↓
[AUTOMATED, phase 3 only, once phase-2 submission is accepted]
    vedantmgoyal2009/winget-releaser (or equivalent) watches subsequent
    ledgrrr-desktop-v* GitHub Releases and auto-opens the update PR to
    microsoft/winget-pkgs on every new release.
```

**Natural automation boundary:** everything up through "GitHub Release with hashed, publicly-downloadable installer assets" can and should be fully automated (it's entirely within `_b00t_`'s own CI trust boundary). Everything from "open a PR against a third-party repo owned by Microsoft" onward is **not** something to automate blindly on day one — `microsoft/winget-pkgs` has its own validation bots, manual moderator review, and community trust ramp-up (new publishers get more scrutiny). The first submission should be manual (`wingetcreate submit` run locally, or a manually-triggered `workflow_dispatch` job) so a human can respond to reviewer feedback / fix manifest schema issues without fighting CI feedback latency. Auto-PR-on-every-release (winget-releaser) is only safe to turn on **after** the first manifest has been accepted upstream and the manifest schema/shape is proven correct.

## Suggested Build Order (dependency-ordered, for roadmap phases)

**Phase 1 — Release automation (foundational, monorepo-only, no external dependency)**
- Fix the stale `crates/ledgerr-tauri` → `crates/ledgerr-host` path in `build-tauri-windows.yml` (blocks everything downstream the moment the submodule pointer is next bumped — do this first, independently of the tag work, since it's a live landmine).
- Add `ledgrrr-desktop-v*` tag trigger.
- Add SHA256 computation step.
- Add `softprops/action-gh-release@v1` step (gated on tag ref, `contents: write` permission, mirroring `browser-ext-release.yml`).
- Extend the existing `build-tauri-windows.yml` rather than forking a new file: the build step (`cargo tauri build`) and the release step share the same runner/artifacts, and splitting them means either a second full Windows build or artifact hand-off between workflows (extra complexity for no isolation benefit — `browser-ext-release.yml` gets away with one file because it's build+release together too, same shape).
- Success is verifiable entirely within `_b00t_` — no winget-pkgs interaction, no external gatekeeper. This is why it's phase 1: everything later depends on a real, public, hash-stable installer URL existing, and this phase is what produces one.

**Phase 2 — First winget submission (depends on Phase 1's Release existing and being stable)**
- Resolve the PackageIdentifier decision (`PromptExecution.ledgrrr` vs alternatives) — needs a real decision, not a placeholder, since winget-pkgs PRs get rejected/require re-submission if the identifier changes later.
- Hand-author (or `wingetcreate new`) the three manifest YAMLs against a real Phase-1-produced Release asset URL + hash.
- Manual PR to `microsoft/winget-pkgs`; iterate on reviewer feedback.
- Gate: do not proceed to Phase 3 until this PR merges — winget-releaser has nothing to update against otherwise.

**Phase 3 — Ongoing auto-update via winget-releaser (depends on Phase 2's manifest being accepted upstream)**
- Add `vedantmgoyal2009/winget-releaser` (or current equivalent) as a step triggered on `release: types: [published]` for `ledgrrr-desktop-v*` releases, using a PAT with `public_repo` scope stored as a repo secret.
- This closes the loop: future `cog bump --auto` → submodule bump → tag → Phase-1 Release → Phase-3 auto-PR, with no manual winget-pkgs step in steady state.

## Anti-Patterns

### Anti-Pattern 1: Automating the winget-pkgs PR before the first manifest has ever been accepted
**What people do:** Wire up winget-releaser in the same change that adds release automation, on the theory that "it's all one feature."
**Why it's wrong:** The first submission to a Microsoft-owned repo is a trust-building, iteration-heavy process (schema validation bots + human moderators). Automating it means every release-automation bug also becomes a spam risk against a third-party repo, and failures are debugged against someone else's CI feedback loop instead of your own.
**Do this instead:** Manual first submission (Phase 2), automate only once that manifest shape is proven (Phase 3).

### Anti-Pattern 2: Using a bare `v*` tag for the desktop release trigger
**What people do:** Reuse the same tag scheme the crate/workspace already uses (`vX.Y.Z`) for the new release workflow because it's already there.
**Why it's wrong:** `_b00t_` already uses bare `v*` tags for its own top-level workspace release (`release.yml`), and `ledgrrr`'s own repo uses `v*` internally for `cog bump`. A bare `v*` push to `_b00t_` would either double-fire unrelated workflows or create an ambiguous tag whose meaning depends on which repo you're looking at.
**Do this instead:** Product-scoped tag prefix (`ledgrrr-desktop-v*`), matching the `b00t-browser-ext-v*` precedent already established in this exact repo.

### Anti-Pattern 3: Assuming submodule tag pushes trigger superproject workflows
**What people do:** Expect `cog bump --auto && git push --follow-tags` run inside `vendor/ledgrrr` to be sufficient to kick off the Windows build/release in `_b00t_`.
**Why it's wrong:** GitHub Actions triggers are scoped to the repo the workflow file lives in. A tag pushed to `PromptExecution/ledgrrr` is invisible to `elasticdotventures/_b00t_`'s Actions.
**Do this instead:** Treat the submodule-pointer bump + a separate tag push in `_b00t_` as an explicit, distinct step in the release process (manual today; a candidate for its own small automation later, e.g. a workflow_dispatch or a bot that watches the submodule's upstream tags and opens a bump PR — out of scope for this milestone).

## Integration Points

### External Services

| Service | Integration Pattern | Notes |
|---|---|---|
| `microsoft/winget-pkgs` (GitHub repo) | Fork + PR, either via `wingetcreate submit` CLI (manual) or `vedantmgoyal2009/winget-releaser` GH Action (automated, later phase) | Requires a PAT with `public_repo` scope for the automated path; manifest schema validated by upstream bots before human review |
| GitHub Releases API (on `_b00t_`) | `softprops/action-gh-release@v1`, same as `browser-ext-release.yml` | Needs `contents: write`; asset URLs must be stable long-term since winget manifests pin exact `InstallerUrl` + `InstallerSha256` per version |

### Internal Boundaries

| Boundary | Communication | Notes |
|---|---|---|
| `PromptExecution/ledgrrr` (submodule, its own `cog.toml`/tags) ↔ `elasticdotventures/_b00t_` (outer repo, hosts all CI) | Submodule pointer bump (git commit in `_b00t_`), **not** a shared tag namespace | This is the crux integration seam for this whole milestone — see Data Flow |
| `build-tauri-windows.yml`'s existing PR/branch-push triggers ↔ the new tag-triggered release path | Same workflow file, different `on:` conditions, release steps gated with `if: startsWith(github.ref, 'refs/tags/ledgrrr-desktop-v')` | Exactly the pattern `browser-ext-release.yml` already uses to share one file between "build on every PR" and "release on tag" |
| `xtask/src/publisher.rs` (`ledgrrr`'s existing hash+release-attach pattern for MCP registry publishing) ↔ a potential new winget-manifest-generation step | Could be a new `xtask` subcommand reusing the same `release_tag`/`sha256`/`gh release upload` idiom | Optional for phase 1; worth considering before writing fresh bash/YAML for hash computation, per this repo's DRY convention |

## Sources

- Direct inspection: `.github/workflows/build-tauri-windows.yml`, `.github/workflows/browser-ext-release.yml`, `.github/workflows/release.yml`, `.github/workflows/b00t-mcp-npm-release.yml`, `.github/workflows/build-release.yml` (all in `elasticdotventures/_b00t_`)
- `vendor/ledgrrr/cog.toml`, `vendor/ledgrrr/Cargo.toml`, `vendor/ledgrrr/crates/ledgerr-host/tauri.conf.json`, `vendor/ledgrrr/crates/ledgerr-host/Cargo.toml`, `vendor/ledgrrr/xtask/src/publisher.rs`
- `vendor/ledgrrr/.planning/PROJECT.md`, `vendor/ledgrrr/.planning/STATE.md` (v1.3 milestone kickoff, commit `f5ff4ac`)
- `git log`, `git ls-tree`, `git submodule status`, `gh repo view` (visibility/remote checks) run against both repos during this research
- No existing reference to `winget-releaser`, `wingetcreate`, or a winget manifest anywhere in either repo — confirmed via repo-wide grep

---
*Architecture research for: winget distribution of the ledgrrr desktop app*
*Researched: 2026-08-08*
