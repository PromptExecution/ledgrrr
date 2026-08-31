# Foundry Local Epic — Phase 4: Status Detection + Install-Assist

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Real Foundry Local status detection (installed? running?) surfaced through the existing status contract, plus a plan-before-mutation winget install-assist flow — the backend half of "the installer should recommend and attempt to help install Windows Foundry Local... and tell what is installed." UI wiring (an actual button) is a deliberate follow-up, matching this epic's own Phase 2/2b split precedent.

**Architecture:** Two new pieces in `ledgerr-desktop-agent` (Tasks 1-2) plus Tauri command wiring in `ledgerr-host`'s Tauri binary (Task 3) — all within the `ledgrrr` repo, stacked on top of the still-open `feat/tray-client-server-port` branch (PR #216), continuing this session's established stacked-PR pattern. `ledgerr-desktop-agent` cannot depend on `ledgerr-host` (dependency direction is the reverse — `ledgerr-host` already depends on `ledgerr-desktop-agent` for `get_desktop_status`), so Task 1's detection is a deliberately minimal, self-contained CLI/service-presence check — NOT a duplicate of `ledgerr-host::internal_openai`'s full REST-endpoint-resolution logic, which stays there and keeps owning actual chat connectivity.

**Tech Stack:** Rust, existing `install_plan.rs`'s plan-before-mutation pattern as the template for Task 2, existing `commands.rs`/`main.rs` Tauri/specta wiring pattern for Task 3.

**Spec:** https://github.com/PromptExecution/ledgrrr/issues/219

## Global Constraints

- `ledgerr-desktop-agent` does not and must not gain a dependency on `ledgerr-host` — verify this before writing any code (`grep -n "ledgerr-host" crates/ledgerr-desktop-agent/Cargo.toml` must stay empty). The dependency direction in this workspace is `ledgerr-host → ledgerr-desktop-agent`, and reversing any part of it would create a cycle.
- Task 1's `detect_foundry_local()` checks CLI presence (`where.exe`/`which foundry`) and whether `foundry service status` exits successfully — nothing more. It does NOT resolve or return the actual REST chat endpoint; that stays `ledgerr_host::internal_openai::discover_foundry_local_endpoint`'s job, unchanged and untouched by this plan.
- Every new public field on `LedgrrrStatus` (i.e. the new `foundry_local: FoundryLocalStatus` field) must not break `LedgrrrStatus`'s existing JSON shape for already-present fields — this is purely additive.
- Task 2's `foundry_install_plan.rs` follows `install_plan.rs`'s exact plan-before-mutation contract: a plan struct with `executable_now`/`blocked_reason`, an `approved: bool`-gated `invoke` that refuses to run anything unless explicitly approved, and an audit-log call (`state::audit(...)`) on the actual mutating action — matching this crate's established safety pattern for anything that shells out to install software.
- Use the exact winget command already proven in this repo's own `Justfile` (`windows-ai-install` recipe, verified this session): `winget install --id Microsoft.FoundryLocal --source winget --accept-package-agreements --accept-source-agreements`. Do not invent a different flag set.
- Work happens in the existing worktree at `/home/brianh/promptexecution/_b00t_/vendor/ledgrrr` (WSL path) — this worktree is already on branch `feat/tray-client-server-port` (PR #216, open, unmerged), and this plan's commits land ON TOP of it as new commits on the same branch (stacked), not a new branch. **Also mirror every edit to `/mnt/d/promptjects/ledgrrr` for building/testing** — this session's established WSL↔Windows-native workflow: the WSL worktree is the source of truth for edits (via normal Read/Edit/Write), but `cargo build`/`cargo test` only work through `pwsh.exe` against the native Windows clone at `D:\promptjects\ledgrrr`, since WSL has no C linker at all and this crate additionally needs the full Windows/MSVC toolchain (unlike `ufo-types`, this is Windows GUI-adjacent code). After editing a file in WSL, copy it to the mirror before testing: `cp crates/ledgerr-desktop-agent/src/status.rs /mnt/d/promptjects/ledgrrr/crates/ledgerr-desktop-agent/src/status.rs` (adjust path per file). **Before starting Task 1, verify the D: mirror is reasonably in sync** — run `diff -q crates/ledgerr-desktop-agent/src/status.rs /mnt/d/promptjects/ledgrrr/crates/ledgerr-desktop-agent/src/status.rs` (and the same for `install_plan.rs`, `commands.rs`, `main.rs`) from the WSL worktree; if any differ, sync the D: copy FROM the WSL worktree first (`cp` WSL → D:, the WSL worktree is authoritative) before making any edits, and note this in your report.
- Always pass `-j 2` to `cargo build`/`cargo test` on the D: side (`pwsh.exe -NoProfile -Command "cd D:\promptjects\ledgrrr; cargo test -p ledgerr-desktop-agent -j 2"` / `-p ledgerr-host` for Task 3) — this machine has previously OOM-crashed rustc at the default parallelism.
- `ledgerr-desktop-agent` is cross-platform code with runtime `cfg!(windows)` branches (not compile-time `#[cfg(windows)]` gating) — Task 1/2's new code follows this same convention, not a Windows-only compile gate.

---

### Task 1: `FoundryLocalStatus` detection

**Files:**
- Modify: `crates/ledgerr-desktop-agent/src/status.rs` (add struct, detection fn, wire into `LedgrrrStatus`/`collect()`, add a test)

**Interfaces:**
- Produces: `pub struct FoundryLocalStatus { pub cli_found: bool, pub service_running: bool }`, added as a new field `pub foundry_local: FoundryLocalStatus` on `LedgrrrStatus`. Automatically flows to the UI via the already-existing `get_desktop_status` Tauri command (`ledgerr-host/src/bin/tauri/commands.rs:35`, calls `ledgerr_desktop_agent::status::collect()`) — no changes needed there for this task.

- [ ] **Step 1: Write the failing test**

In `crates/ledgerr-desktop-agent/src/status.rs`, inside the existing `#[cfg(test)] mod tests { ... }` block (after `tray_candidates_matches_the_real_host_tauri_binary_name`), add:

```rust
    #[test]
    fn foundry_local_status_roundtrips_through_json() {
        let status = FoundryLocalStatus {
            cli_found: true,
            service_running: false,
        };
        let json = serde_json::to_string(&status).unwrap();
        let back: FoundryLocalStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(back.cli_found, true);
        assert_eq!(back.service_running, false);
    }
```

This requires adding `use serde_json;` — check the top of the file: it already imports `serde::{Deserialize, Serialize}` but likely not `serde_json` directly (the struct derives handle serialization; the test needs the crate). Add `serde_json = "1"` as a dev-dependency in `crates/ledgerr-desktop-agent/Cargo.toml` if it's not already a dependency (check `grep serde_json crates/ledgerr-desktop-agent/Cargo.toml` first — it may already be present as a regular dependency, in which case no `Cargo.toml` change is needed).

- [ ] **Step 2: Run test to verify it fails**

First sync the file to the D: mirror (`cp crates/ledgerr-desktop-agent/src/status.rs /mnt/d/promptjects/ledgrrr/crates/ledgerr-desktop-agent/src/status.rs`), then:
Run: `pwsh.exe -NoProfile -Command "cd D:\promptjects\ledgrrr; cargo test -p ledgerr-desktop-agent -j 2 foundry_local_status_roundtrips -- --nocapture"`
Expected: FAIL to compile — `FoundryLocalStatus` not found.

- [ ] **Step 3: Write the minimal implementation**

Add to `crates/ledgerr-desktop-agent/src/status.rs`, near the other status struct definitions (after `ModelRuntimeStatus`, before `PackageStatus`):

```rust
/// Windows Foundry Local presence/liveness — deliberately minimal. This
/// crate cannot depend on `ledgerr-host` (the dependency direction in this
/// workspace runs the other way), so this does NOT resolve the actual REST
/// chat endpoint the way `ledgerr_host::internal_openai::
/// discover_foundry_local_endpoint` does — it only answers "is Foundry
/// Local present and alive" for status reporting. Actually connecting to
/// it for chat stays ledgerr-host's job, unchanged.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FoundryLocalStatus {
    pub cli_found: bool,
    pub service_running: bool,
}
```

Add the detection function near `detect_model_runtime()`:

```rust
fn detect_foundry_local() -> FoundryLocalStatus {
    let locator = if cfg!(windows) { "where.exe" } else { "which" };
    let cli_found = Command::new(locator)
        .arg("foundry")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);
    if !cli_found {
        return FoundryLocalStatus {
            cli_found: false,
            service_running: false,
        };
    }
    let service_running = Command::new("foundry")
        .args(["service", "status"])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);
    FoundryLocalStatus {
        cli_found,
        service_running,
    }
}
```

Wire into `LedgrrrStatus` — add the field:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LedgrrrStatus {
    pub controller_version: String,
    pub b00t: B00tStatus,
    pub service: ServiceStatus,
    pub tray: TrayStatus,
    pub model_runtime: ModelRuntimeStatus,
    pub foundry_local: FoundryLocalStatus,
    pub desktop_package: PackageStatus,
    pub claude_controller: ClaudeControllerStatus,
    pub office_addin: OfficeSurfaceStatus,
    pub sharepoint_webpart: OfficeSurfaceStatus,
}
```

And in `collect()`:

```rust
pub fn collect() -> LedgrrrStatus {
    LedgrrrStatus {
        controller_version: env!("CARGO_PKG_VERSION").to_string(),
        b00t: detect_b00t(),
        service: detect_service(),
        tray: detect_tray(),
        model_runtime: detect_model_runtime(),
        foundry_local: detect_foundry_local(),
        desktop_package: detect_package(),
        claude_controller: ClaudeControllerStatus {
            state: "installed_with_mcpb_or_direct_stdio".to_string(),
            expected_tools: 11,
        },
        office_addin: OfficeSurfaceStatus {
            state: "not_configured".to_string(),
        },
        sharepoint_webpart: OfficeSurfaceStatus {
            state: "not_configured".to_string(),
        },
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Sync the file again (`cp crates/ledgerr-desktop-agent/src/status.rs /mnt/d/promptjects/ledgrrr/crates/ledgerr-desktop-agent/src/status.rs`), then:
Run: `pwsh.exe -NoProfile -Command "cd D:\promptjects\ledgrrr; cargo test -p ledgerr-desktop-agent -j 2 foundry_local_status_roundtrips -- --nocapture"`
Expected: PASS

- [ ] **Step 5: Run the crate's full test suite**

Run: `pwsh.exe -NoProfile -Command "cd D:\promptjects\ledgrrr; cargo test -p ledgerr-desktop-agent -j 2"`
Expected: PASS, all existing tests plus the new one green.

- [ ] **Step 6: Commit**

Commit from the WSL worktree (the source of truth — the D: mirror is a build/test copy only):

```bash
cd /home/brianh/promptexecution/_b00t_/vendor/ledgrrr
git add crates/ledgerr-desktop-agent/src/status.rs crates/ledgerr-desktop-agent/Cargo.toml
git commit -m "feat(status): add FoundryLocalStatus detection to LedgrrrStatus"
```

(Only include `Cargo.toml` in the `git add` if Step 1 actually required a change there — omit it if `serde_json` was already a dependency.)

---

### Task 2: Winget install-assist plan

**Files:**
- Create: `crates/ledgerr-desktop-agent/src/foundry_install_plan.rs`
- Modify: `crates/ledgerr-desktop-agent/src/lib.rs` (add module declaration)

**Interfaces:**
- Consumes: `crate::state` (for `state::audit`, matching `install_plan.rs`'s exact usage).
- Produces: `pub struct FoundryInstallPlan { pub action: String, pub executable_now: bool, pub blocked_reason: Option<String>, pub unattended_command: String }`, `pub struct FoundryInstallActionArgs { pub approved: bool }` (with `#[serde(default)]` on `approved` and a `Default` impl), `pub struct FoundryInstallResult { pub ok: bool, pub launched: bool, pub message: String, pub plan: FoundryInstallPlan }`, `pub fn install_plan() -> FoundryInstallPlan`, `pub fn invoke(args: FoundryInstallActionArgs) -> FoundryInstallResult`. Consumed by Task 3's Tauri commands.

- [ ] **Step 1: Confirm the module wiring convention**

Run: `grep -n "pub mod install_plan" crates/ledgerr-desktop-agent/src/lib.rs`
Note the exact line so Step 4 below can place the new module declaration correctly (alongside it, alphabetically: `foundry_install_plan` sorts before `install_plan` — f < i).

- [ ] **Step 2: Write the failing test**

Create `crates/ledgerr-desktop-agent/src/foundry_install_plan.rs`:

```rust
//! Plan-before-mutation winget install-assist for Windows Foundry Local.
//! Mirrors `install_plan.rs`'s exact safety contract for this repo's own
//! MSIX package (approve-then-invoke, never runs anything unapproved).

use std::process::Command;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::state;

/// Exact command already proven in this repo's own `Justfile`
/// (`windows-ai-install` recipe) — do not invent a different flag set.
const WINGET_INSTALL_COMMAND: &str = "winget install --id Microsoft.FoundryLocal --source winget --accept-package-agreements --accept-source-agreements";

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FoundryInstallPlan {
    pub action: String,
    pub executable_now: bool,
    pub blocked_reason: Option<String>,
    pub unattended_command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FoundryInstallActionArgs {
    #[serde(default)]
    pub approved: bool,
}

impl Default for FoundryInstallActionArgs {
    fn default() -> Self {
        Self { approved: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FoundryInstallResult {
    pub ok: bool,
    pub launched: bool,
    pub message: String,
    pub plan: FoundryInstallPlan,
}

pub fn install_plan() -> FoundryInstallPlan {
    let executable_now = cfg!(windows);
    let blocked_reason = if !cfg!(windows) {
        Some("Windows Foundry Local can only be installed via winget on Windows.".to_string())
    } else {
        None
    };
    FoundryInstallPlan {
        action: "install_foundry_local".to_string(),
        executable_now,
        blocked_reason,
        unattended_command: WINGET_INSTALL_COMMAND.to_string(),
    }
}

pub fn invoke(args: FoundryInstallActionArgs) -> FoundryInstallResult {
    let plan = install_plan();
    if !args.approved {
        return FoundryInstallResult {
            ok: false,
            launched: false,
            message: "approval required: call the install plan and retry with approved=true"
                .to_string(),
            plan,
        };
    }
    if !plan.executable_now {
        return FoundryInstallResult {
            ok: false,
            launched: false,
            message: plan
                .blocked_reason
                .clone()
                .unwrap_or_else(|| "Foundry Local install is unavailable".to_string()),
            plan,
        };
    }
    let launched = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            WINGET_INSTALL_COMMAND,
        ])
        .spawn()
        .is_ok();
    state::audit(
        "controller",
        "install_foundry_local",
        if launched { "launched" } else { "failed" },
        "winget install --id Microsoft.FoundryLocal",
    );
    FoundryInstallResult {
        ok: launched,
        launched,
        message: if launched {
            "launched Foundry Local install via winget".to_string()
        } else {
            "failed to launch the Foundry Local winget install".to_string()
        },
        plan,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invoke_without_approval_never_launches() {
        let result = invoke(FoundryInstallActionArgs { approved: false });
        assert!(!result.ok);
        assert!(!result.launched);
        assert!(result.message.contains("approval required"));
    }

    #[test]
    fn install_plan_reports_the_exact_proven_winget_command() {
        let plan = install_plan();
        assert_eq!(
            plan.unattended_command,
            "winget install --id Microsoft.FoundryLocal --source winget --accept-package-agreements --accept-source-agreements"
        );
    }

    #[test]
    fn default_action_args_are_not_approved() {
        let args = FoundryInstallActionArgs::default();
        assert!(!args.approved);
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Sync the new file (`cp crates/ledgerr-desktop-agent/src/foundry_install_plan.rs /mnt/d/promptjects/ledgrrr/crates/ledgerr-desktop-agent/src/foundry_install_plan.rs`) — it won't compile yet since the module isn't wired into `lib.rs`, then:
Run: `pwsh.exe -NoProfile -Command "cd D:\promptjects\ledgrrr; cargo test -p ledgerr-desktop-agent -j 2 foundry_install_plan:: -- --nocapture"`
Expected: FAIL — module not found (no test output at all, a "no tests ran matching" or compile-scope message, since `foundry_install_plan` isn't declared as a module yet).

- [ ] **Step 4: Wire the module into lib.rs**

In `crates/ledgerr-desktop-agent/src/lib.rs`, add (alphabetically, before `pub mod install_plan;`):

```rust
pub mod foundry_install_plan;
```

- [ ] **Step 5: Run test to verify it passes**

Sync `lib.rs` too, then:
Run: `pwsh.exe -NoProfile -Command "cd D:\promptjects\ledgrrr; cargo test -p ledgerr-desktop-agent -j 2 foundry_install_plan:: -- --nocapture"`
Expected: PASS (3 tests)

- [ ] **Step 6: Run the crate's full test suite**

Run: `pwsh.exe -NoProfile -Command "cd D:\promptjects\ledgrrr; cargo test -p ledgerr-desktop-agent -j 2"`
Expected: PASS, all tests green (existing + Task 1's + these 3).

- [ ] **Step 7: Commit**

```bash
cd /home/brianh/promptexecution/_b00t_/vendor/ledgrrr
git add crates/ledgerr-desktop-agent/src/foundry_install_plan.rs crates/ledgerr-desktop-agent/src/lib.rs
git commit -m "feat(foundry_install_plan): add winget install-assist plan-before-mutation flow"
```

---

### Task 3: Tauri command wiring

**Files:**
- Modify: `crates/ledgerr-host/src/bin/tauri/commands.rs` (add two new `#[tauri::command]` functions)
- Modify: `crates/ledgerr-host/src/bin/tauri/main.rs` (register the two new commands in `collect_commands!`)

**Interfaces:**
- Consumes: `ledgerr_desktop_agent::foundry_install_plan::{install_plan, invoke, FoundryInstallActionArgs}` (Task 2).
- Produces: two new Tauri commands, `get_foundry_local_install_plan` and `foundry_local_install_action`, following `get_desktop_repair_plan`'s exact JSON-string-return convention (`desktop_json` helper, already defined in `commands.rs`).

- [ ] **Step 1: Run the crate's existing tests once, to record a clean baseline**

Run: `pwsh.exe -NoProfile -Command "cd D:\promptjects\ledgrrr; cargo test -p ledgerr-host -j 2 2>&1 | Select-Object -Last 40"`
Note the pass/fail counts — you'll compare against this after your changes. `ledgerr-host` is a larger crate; expect this to take longer than Tasks 1-2's crate.

- [ ] **Step 2: Add the two Tauri commands**

In `crates/ledgerr-host/src/bin/tauri/commands.rs`, add after `get_desktop_repair_plan` (around line 77):

```rust
#[tauri::command]
#[specta::specta]
pub fn get_foundry_local_install_plan() -> Result<String, String> {
    desktop_json(&ledgerr_desktop_agent::foundry_install_plan::install_plan())
}

#[tauri::command]
#[specta::specta]
pub fn foundry_local_install_action(approved: bool) -> Result<String, String> {
    desktop_json(&ledgerr_desktop_agent::foundry_install_plan::invoke(
        ledgerr_desktop_agent::foundry_install_plan::FoundryInstallActionArgs { approved },
    ))
}
```

- [ ] **Step 3: Register the commands in main.rs**

In `crates/ledgerr-host/src/bin/tauri/main.rs`, inside the `collect_commands![...]` macro invocation, add after `commands::get_desktop_repair_plan,` (around line 146):

```rust
        commands::get_foundry_local_install_plan,
        commands::foundry_local_install_action,
```

- [ ] **Step 4: Build to confirm it compiles**

Sync all three touched files to the D: mirror (`commands.rs`, `main.rs`, plus Task 1/2's files if not already synced), then:
Run: `pwsh.exe -NoProfile -Command "cd D:\promptjects\ledgrrr; cargo build -p ledgerr-host --bin host-tauri -j 2 2>&1 | Select-Object -Last 40"`
Expected: no errors. (This binary is `#[cfg(target_os = "windows")]`-gated for its real logic — building on the native Windows toolchain via `pwsh.exe` is what actually exercises the Windows-specific code path; this is the same build this session's earlier tray work already relied on.)

- [ ] **Step 5: Run the crate's full test suite, compare against baseline**

Run: `pwsh.exe -NoProfile -Command "cd D:\promptjects\ledgrrr; cargo test -p ledgerr-host -j 2 2>&1 | Select-Object -Last 40"`
Expected: same pass count as Step 1's baseline, or higher (no regressions) — this task doesn't add new `ledgerr-host`-side tests (the logic being wired is already tested in Task 2's crate), so the count should match Step 1 exactly.

- [ ] **Step 6: Commit**

```bash
cd /home/brianh/promptexecution/_b00t_/vendor/ledgrrr
git add crates/ledgerr-host/src/bin/tauri/commands.rs crates/ledgerr-host/src/bin/tauri/main.rs
git commit -m "feat(tauri): wire Foundry Local install-plan/action commands"
```

---

## Final Verification Checklist (for the validating/committing agent)

Run these from the WSL worktree `/home/brianh/promptexecution/_b00t_/vendor/ledgrrr` for git operations, and via `pwsh.exe` against `D:\promptjects\ledgrrr` for all `cargo` commands (sync the D: mirror first if anything looks out of date — `diff -rq crates/ledgerr-desktop-agent /mnt/d/promptjects/ledgrrr/crates/ledgerr-desktop-agent` and the same for the touched `ledgerr-host` files), after all 3 tasks are complete, before pushing:

- [ ] `pwsh.exe -NoProfile -Command "cd D:\promptjects\ledgrrr; cargo test -p ledgerr-desktop-agent -j 2"` — all tests pass, including the 4 new ones (1 in Task 1, 3 in Task 2).
- [ ] `pwsh.exe -NoProfile -Command "cd D:\promptjects\ledgrrr; cargo build -p ledgerr-host --bin host-tauri -j 2"` — no errors.
- [ ] `pwsh.exe -NoProfile -Command "cd D:\promptjects\ledgrrr; cargo test -p ledgerr-host -j 2"` — pass count matches or exceeds Task 3's recorded baseline; no regressions.
- [ ] Confirm `crates/ledgerr-desktop-agent/Cargo.toml` still has NO dependency on `ledgerr-host` (`grep -n "ledgerr-host" crates/ledgerr-desktop-agent/Cargo.toml` returns nothing).
- [ ] Confirm the WSL worktree and the D: mirror are back in sync for every file this plan touched (`diff` each one) — if the validator made any last-minute fix, make sure it landed in BOTH copies before committing, since only the WSL copy is what git tracks and pushes.
- [ ] This plan's commits land on the EXISTING branch `feat/tray-client-server-port` (PR #216) — do NOT create a new branch, do NOT open a new PR. Push with `git push` (already tracking `origin/feat/tray-client-server-port`) and the existing PR #216 picks up the new commits automatically. Report the updated PR URL (still #216) and confirm the push succeeded (`git log --oneline -5` should show all 3 new commits on top of what was already there).
