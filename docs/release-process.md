# Release Process

## Versioning Policy

l3dg3rr uses an **odd/even minor version** convention (Ubuntu-style):

| Minor | Series | Test gate | GitHub release | LTS |
|-------|--------|-----------|----------------|-----|
| Even (1.0, 1.2, 1.8…) | **Stable** | Full suite incl. phi4 GGUF inference | Yes (`--latest`) | Yes |
| Odd (1.1, 1.3, 1.7…) | **Dev/Experimental** | Fast suite only (phi4 skipped) | Pre-release only | No |

Breaking changes within a major series are permitted on odd minors. Even minors are the supported upgrade targets.

## Preconditions

- CI is green on `main`.
- Commits follow [Conventional Commits](https://www.conventionalcommits.org/).
- `cocogitto` and `gh` CLI are installed.
- For even minor releases: phi4 GGUF model assets are present (`just phi4-reasoning-symlink`).

## Creating a Release (local)

```bash
# Bump minor version (most common — advances to next even or odd minor)
just release minor

# Bump patch (preserves current minor parity)
just release patch

# Bump major (resets minor to 0 — even, so stable)
just release major
```

`just release` automatically:
1. Detects the next minor number and applies odd/even policy
2. **Even minor** — runs the full test suite including phi4 inference (~10+ min with model assets)
3. **Odd minor** — runs the fast test suite, skipping phi4 inference (~seconds)
4. Runs `./scripts/e2e_mvp.sh` end-to-end smoke path
5. Calls `cog bump --<version>` — bumps all `Cargo.toml` versions, creates a conventional-commit bump commit and a semver git tag
6. Pushes branch and tags with `git push --follow-tags`
7. **Even minor** — creates a stable GitHub release (`gh release create --latest`)
8. **Odd minor** — creates a GitHub pre-release (`gh release create --prerelease`)

## Test Gates

```bash
# Fast gate (always safe; phi4 skipped) — used automatically for odd minors
just test-fast

# Full gate including phi4 GGUF inference — required before any even minor release
just test-phi4
cargo test --workspace --all-targets --all-features

# E2E smoke path (required before any release)
./scripts/e2e_mvp.sh
```

## GitHub Pages Deployment

Docs deploy automatically via `.github/workflows/docs.yml` on:

- Any push to `main`
- Any `v*` tag push (covers both local `just release` and CI release paths)
- Manual `workflow_dispatch`

No manual step is needed — pushing the release tag triggers the docs deployment.

## CI Release Path (workflow_dispatch)

The `release.yml` workflow (trigger: Actions → Release → Run workflow) runs `cog bump --auto`, pushes to `main`, and creates the GitHub release. It applies the same odd/even policy:

- Even minor → `gh release create --latest`
- Odd minor → `gh release create --prerelease`

The subsequent `main` push and tag push both trigger `docs.yml`, so GitHub Pages is updated regardless of which path created the release.

## Manual Recovery

If a release tag exists but no GitHub release was created:

```bash
TAG=v1.8.0
cog changelog --at "$TAG" > /tmp/notes.md
gh release create "$TAG" --title "$TAG (stable)" --notes-file /tmp/notes.md --latest
```

If GitHub Pages is stale after a release:

```bash
# Trigger a manual docs redeploy from GitHub Actions UI, or:
gh workflow run docs.yml --repo PromptExecution/ledgrrr
```
