# Tray/Tauri Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give `host-tauri.exe` (the main app) the full tray feature set that only the standalone `host-tray.exe` currently has, fix the "toast doesn't work" bug (never wired on host-tauri's Windows tray), retire the two now-redundant standalone binaries, and merge the one PR that's ready.

**Architecture:** Make `tray::runtime::run()`'s "show window" action an injected closure instead of a hardcoded spawn of the legacy Slint `host-window.exe`. Wire `host-tauri.exe`'s Windows tray setup to call the shared `run()` on a background thread, injecting "show the Tauri webview" as that closure — giving it the exact same tested toggle/notification/toast menu as `host-tray.exe`, with zero duplicated logic. Only after that lands, retire `host-window.exe`/`host-tray.exe` as build targets.

**Tech Stack:** Rust, `windows-rs` (Win32 tray), Tauri 2.x, existing `tempfile`-based test harness. Native Windows build via `pwsh.exe` invoked from WSL (`cargo` on Linux WSL cannot link this crate — no `cc` installed — and cross-compiling to `windows-gnu` isn't the project's toolchain; always build/test through the native Windows checkout).

**Spec:** `docs/superpowers/specs/2026-08-29-tray-tauri-integration-design.md`

## Global Constraints

- Build/test only through the native Windows checkout at `D:\promptjects\ledgrrr` via `pwsh.exe -NoProfile -Command '...'` invoked from WSL, **never** WSL's own `cargo` (no linker) and never against a `\\wsl.localhost\...` path as `RepoRoot` (this crate's own `AGENTS.md` documents native Windows Cargo cannot build with a WSL UNC working directory).
- Always run with `-j 2` (`cargo test ... -j 2`, `cargo build ... -j 2`): this machine has 24G RAM with ~8-9G typically free, and the default `-j 8` parallelism has reproducibly OOM-crashed rustc mid-build (`alloc::alloc::handle_alloc_error`), corrupting `.rmeta` files and cascading into unrelated compile errors across the whole crate. If that ever recurs anyway, the fix is `cargo clean -p ledgerr-host` (or a full `cargo clean` if errors span multiple unrelated crates) followed by a retry at `-j 2` — don't chase individual "invalid metadata" errors as if they were real code bugs.
- Set `$env:TMP` / `$env:TEMP` to a `D:`-rooted path before any cargo invocation that might shell out to `cargo install` (e.g. `windows-package.ps1`'s mdbook step) — the Windows temp dir defaults to `C:`, which has almost no free space on this machine (~4G) and will hard-fail with "not enough space on the disk" otherwise. Not needed for the `cargo build`/`cargo test` commands in this plan, which don't invoke `cargo install`, but keep it in any shell session touching this checkout as a standing habit.
- **Every edit made in the WSL-side checkout (`/home/brianh/promptexecution/_b00t_/vendor/ledgrrr`) must be copied to the native Windows checkout (`D:\promptjects\ledgrrr`, i.e. `/mnt/d/promptjects/ledgrrr` from WSL) before running any Windows-side build/test.** These are two independent clones, not a shared filesystem. Use `cp <wsl-path> <matching /mnt/d/promptjects/ledgrrr path>` per changed file; verify with `diff` before building.
- All commits happen in the WSL-side checkout (that's the one with real git history/remote tracking on `feat/windows-desktop-dogfood`); the `D:` copy is a disposable build/test mirror only — never commit from there.
- TDD: every behavior change gets a failing test first, per-task, before implementation.

---

### Task 1: Make `TrayCommand::ShowWindow`'s action injectable

**Files:**
- Modify: `crates/ledgerr-host/src/tray/runtime.rs` (`run()` at line 21, `handle_command` at line 130, its `ShowWindow` arm at lines 190-196, `show_window_process` at line 275, the call site at line 89)
- Modify: `crates/ledgerr-host/src/bin/host-tray.rs` (its `main()` call to `run()`)
- Test: `crates/ledgerr-host/src/tray/runtime.rs`'s existing `#[cfg(test)] mod tests` (in-file, starting ~line 260)

**Interfaces:**
- Consumes: nothing new from other tasks.
- Produces: `pub fn run(store: SettingsStore, show_window: impl Fn() -> Result<(), Box<dyn std::error::Error>>) -> Result<(), Box<dyn std::error::Error>>` — Task 2 and Task 3's callers depend on this exact signature. `handle_command`'s new 5th parameter type: `show_window: &dyn Fn() -> Result<(), Box<dyn std::error::Error>>`.

- [ ] **Step 1: Write the failing test for the injected closure**

Add to the `#[cfg(test)] mod tests` block in `runtime.rs` (near `show_window_marks_state_visible`, which this replaces):

```rust
    #[test]
    fn show_window_invokes_the_injected_closure_and_marks_state_visible() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(dir.path().join("settings.json"));
        let initial = store.load().unwrap();
        let mut state = TrayState::from_settings(&initial);
        state.window_visible = false;
        let state = Arc::new(Mutex::new(state));
        let (control_tx, _control_rx) = mpsc::channel();

        let call_count = Arc::new(Mutex::new(0));
        let call_count_clone = Arc::clone(&call_count);
        let show_window = move || -> Result<(), Box<dyn std::error::Error>> {
            *call_count_clone.lock().unwrap() += 1;
            Ok(())
        };

        let _ = handle_command(
            TrayCommand::ShowWindow,
            &store,
            &state,
            &control_tx,
            &show_window,
        );

        assert_eq!(*call_count.lock().unwrap(), 1);
        assert!(state.lock().unwrap().window_visible);
    }
```

Remove the old `show_window_marks_state_visible` test entirely — this replaces it (same behavior asserted, plus the new injection-point assertion).

- [ ] **Step 2: Run the test to verify it fails to compile**

From WSL:
```bash
cp /home/brianh/promptexecution/_b00t_/vendor/ledgrrr/crates/ledgerr-host/src/tray/runtime.rs /mnt/d/promptjects/ledgrrr/crates/ledgerr-host/src/tray/runtime.rs
```
Then from a `pwsh.exe -NoProfile -Command` session (from WSL):
```powershell
$env:PATH = "C:\Users\gru3h\.cargo\bin;" + $env:PATH
Set-Location "D:\promptjects\ledgrrr"
cargo test -p ledgerr-host --lib tray::runtime::tests -j 2
```
Expected: **compile error** — `handle_command` takes 4 arguments, test passes 5.

- [ ] **Step 3: Add the `show_window` parameter to `handle_command` and thread it through**

In `crates/ledgerr-host/src/tray/runtime.rs`, change the `handle_command` signature (currently line 130):

```rust
fn handle_command(
    command: TrayCommand,
    store: &SettingsStore,
    state: &Arc<Mutex<TrayState>>,
    control_tx: &mpsc::Sender<TrayControl>,
    show_window: &dyn Fn() -> Result<(), Box<dyn std::error::Error>>,
) -> Result<bool, Box<dyn std::error::Error>> {
```

Change the `TrayCommand::ShowWindow` arm (currently lines 190-196):

```rust
        TrayCommand::ShowWindow => {
            if let Ok(mut state) = state.lock() {
                state.window_visible = true;
            }
            show_window()?;
            Ok(false)
        }
```

Delete the `show_window_process` function entirely (currently at line 275):

```rust
fn show_window_process() -> Result<(), Box<dyn std::error::Error>> {
    let current_exe = std::env::current_exe()?;
    let host_window = current_exe.with_file_name("host-window.exe");
    std::process::Command::new(host_window).spawn()?;
    Ok(())
}
```

Change `run()`'s signature (currently line 21) and its call to `handle_command` (currently line 89):

```rust
pub fn run(
    store: SettingsStore,
    show_window: impl Fn() -> Result<(), Box<dyn std::error::Error>>,
) -> Result<(), Box<dyn std::error::Error>> {
```

```rust
            let should_quit =
                handle_command(command, &store, &state, &tray.control_tx, &show_window)?;
```

- [ ] **Step 4: Fix every other existing test call site to pass a `show_window` closure**

Add a small named no-op helper near the top of the `#[cfg(test)] mod tests` block (used by every test that doesn't care about window-showing):

```rust
    fn noop_show_window() -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }
```

Update `assert_toggle_roundtrips` (currently ~line 294) to pass it — find this line inside the function body:

```rust
        let should_quit =
            handle_command(make_command(target), &store, &state, &control_tx).unwrap();
```

and change it to:

```rust
        let should_quit = handle_command(
            make_command(target),
            &store,
            &state,
            &control_tx,
            &noop_show_window,
        )
        .unwrap();
```

Update the three remaining direct `handle_command(...)` call sites in the test module the same way — `cycle_backend_persists_and_updates_control`, and `quit_requests_shutdown_without_persisting_changes` — appending `, &noop_show_window` as the 5th argument to each `handle_command(...)` call.

- [ ] **Step 5: Update `host-tray.rs`'s call to `run()` to preserve its current behavior**

`crates/ledgerr-host/src/bin/host-tray.rs` currently calls `ledgerr_host::tray::runtime::run(store)`. This binary is retired in Task 4, but must keep compiling and behaving identically until then. Change its Windows `main()` to:

```rust
#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let store =
        ledgerr_host::settings::SettingsStore::new(ledgerr_host::settings::default_settings_path());
    ledgerr_host::tray::runtime::run(store, || {
        let current_exe = std::env::current_exe()?;
        let host_window = current_exe.with_file_name("host-window.exe");
        std::process::Command::new(host_window).spawn()?;
        Ok(())
    })
}
```

(This moves the exact body of the old `show_window_process` here — the only place it's still needed.)

- [ ] **Step 6: Sync and run the full test suite**

```bash
cp /home/brianh/promptexecution/_b00t_/vendor/ledgrrr/crates/ledgerr-host/src/tray/runtime.rs /mnt/d/promptjects/ledgrrr/crates/ledgerr-host/src/tray/runtime.rs
cp /home/brianh/promptexecution/_b00t_/vendor/ledgrrr/crates/ledgerr-host/src/bin/host-tray.rs /mnt/d/promptjects/ledgrrr/crates/ledgerr-host/src/bin/host-tray.rs
```
```powershell
$env:PATH = "C:\Users\gru3h\.cargo\bin;" + $env:PATH
$env:TMP="D:\promptjects\tmp"; $env:TEMP="D:\promptjects\tmp"
Set-Location "D:\promptjects\ledgrrr"
cargo test -p ledgerr-host --lib --tests --bins -j 2
```
Expected: `test result: ok. 83 passed` for the lib tests (same count as before — one test replaced, none added net-new in this task beyond the replacement), and every other test binary still green. No compile errors.

- [ ] **Step 7: Commit**

```bash
cd /home/brianh/promptexecution/_b00t_/vendor/ledgrrr
git add crates/ledgerr-host/src/tray/runtime.rs crates/ledgerr-host/src/bin/host-tray.rs
git commit -m "refactor(tray): make ShowWindow's action injectable

Replaces the hardcoded spawn of host-window.exe with a caller-supplied
closure, so run()/handle_command can be reused by host-tauri (which
will show its own webview) without any dependency on the legacy Slint
binary. host-tray.exe's own behavior is unchanged (its main() now
supplies the old spawn-host-window.exe closure explicitly)."
```

---

### Task 2: Wire `host-tauri.exe`'s Windows tray through the shared runtime

**Files:**
- Modify: `crates/ledgerr-host/src/bin/tauri/tray.rs` (the `#[cfg(windows)] pub fn setup_tray` function)

**Interfaces:**
- Consumes: `ledgerr_host::tray::runtime::run(store: SettingsStore, show_window: impl Fn() -> Result<(), Box<dyn std::error::Error>>) -> Result<(), Box<dyn std::error::Error>>` from Task 1.
- Produces: nothing new consumed by later tasks (this is the integration point itself).

There's no new unit-testable logic here — `run()`'s dispatch is already covered by Task 1's tests, and Tauri's `AppHandle`/`WebviewWindow` aren't practically unit-testable outside a running app. The test for this task is the live launch in Step 3.

- [ ] **Step 1: Replace the Windows `setup_tray` body**

In `crates/ledgerr-host/src/bin/tauri/tray.rs`, replace the entire `#[cfg(windows)] pub fn setup_tray` function (currently lines 16-81) with:

```rust
#[cfg(windows)]
pub fn setup_tray(app: &tauri::App) {
    use ledgerr_host::settings::{default_settings_path, SettingsStore};

    let app_handle = app.handle().clone();

    std::thread::spawn(move || {
        let show_app_handle = app_handle.clone();
        let store = SettingsStore::new(default_settings_path());
        let result = ledgerr_host::tray::runtime::run(store, move || {
            if let Some(window) = show_app_handle.get_webview_window("main") {
                window
                    .show()
                    .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;
                window
                    .set_focus()
                    .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;
            }
            Ok(())
        });
        if let Err(e) = result {
            eprintln!("[tray] tray runtime exited with error: {e}");
        }
        app_handle.exit(0);
    });
}
```

This deletes the old hand-built minimal `TrayMenuLabels` construction, the direct `NativeTrayPlatform::spawn` call, and the 2-command match — all of that now lives once, inside `tray::runtime::run()`, reused as-is.

Remove now-unused imports at the top of the file if `cargo build` flags them (the old code imported `make_icon_data`, `NativeTrayPlatform`, `TrayEvent`, `CMD_EXIT`, `CMD_SHOW_WINDOW`, `TrayMenuLabels`, `Duration` — none of these are referenced by the new body).

- [ ] **Step 2: Sync and build**

```bash
cp /home/brianh/promptexecution/_b00t_/vendor/ledgrrr/crates/ledgerr-host/src/bin/tauri/tray.rs /mnt/d/promptjects/ledgrrr/crates/ledgerr-host/src/bin/tauri/tray.rs
```
```powershell
$env:PATH = "C:\Users\gru3h\.cargo\bin;" + $env:PATH
Set-Location "D:\promptjects\ledgrrr"
cargo build -p ledgerr-host --bin host-tauri -j 2
```
Expected: clean build, no warnings about unused imports (remove any that appear).

- [ ] **Step 3: Live-launch and verify the tray**

```powershell
Get-Process host-tauri -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 300
Start-Process -FilePath "D:\promptjects\ledgrrr\target\debug\host-tauri.exe" -WorkingDirectory "D:\promptjects\ledgrrr"
Start-Sleep -Seconds 3
Get-Process host-tauri -ErrorAction SilentlyContinue | Select-Object Id,Responding,WorkingSet
```
Expected: process alive, `Responding: True`. This is an automated proxy for "didn't crash on startup" — full menu-content and toast verification happens in Task 5's checklist, since it requires actually right-clicking the tray icon, which needs a human or a UI-automation harness this plan doesn't set up.

- [ ] **Step 4: Commit**

```bash
cd /home/brianh/promptexecution/_b00t_/vendor/ledgrrr
git add crates/ledgerr-host/src/bin/tauri/tray.rs
git commit -m "feat(tray): give host-tauri's Windows tray full parity with host-tray.exe

Replaces the minimal Show/Exit-only stub with the shared
tray::runtime::run(), injecting 'show the Tauri webview' as the
window-show action. host-tauri now has the full toggle/notification/
toast menu that was previously only in the standalone host-tray.exe —
this is the fix for toast notifications never having been wired up on
the main app's tray."
```

---

### Task 3: Retire the standalone `host-window` and `host-tray` binaries

**Files:**
- Modify: `crates/ledgerr-host/Cargo.toml` (remove two `[[bin]]` entries)
- Move: `crates/ledgerr-host/src/bin/host-window.rs` → `crates/ledgerr-host/src/bin/legacy/host-window.rs`
- Move: `crates/ledgerr-host/src/bin/host-tray.rs` → `crates/ledgerr-host/src/bin/legacy/host-tray.rs`

**Interfaces:**
- Consumes: nothing (Task 1 and 2 are already complete and don't depend on these files remaining in place).
- Produces: nothing consumed by later tasks.

No new tests — this task's verification is "the crate still builds, and the two binaries are gone."

- [ ] **Step 1: Remove the `[[bin]]` entries**

In `crates/ledgerr-host/Cargo.toml`, delete these two blocks (confirmed present at lines 8-10 and 12-14 as of this plan's writing — grep to confirm current line numbers before editing, since earlier tasks may have shifted nothing in this file but re-check anyway):

```toml
[[bin]]
name = "host-window"
path = "src/bin/host-window.rs"
```

```toml
[[bin]]
name = "host-tray"
path = "src/bin/host-tray.rs"
```

Leave `default-run = "host-tauri"` and the `host-tauri` `[[bin]]` entry untouched.

- [ ] **Step 2: Move the source files out of Cargo's autodiscovery path**

```bash
cd /home/brianh/promptexecution/_b00t_/vendor/ledgrrr
mkdir -p crates/ledgerr-host/src/bin/legacy
git mv crates/ledgerr-host/src/bin/host-window.rs crates/ledgerr-host/src/bin/legacy/host-window.rs
git mv crates/ledgerr-host/src/bin/host-tray.rs crates/ledgerr-host/src/bin/legacy/host-tray.rs
```

Cargo only auto-discovers `.rs` files directly inside `src/bin/`, not subdirectories (this is exactly why `host-tauri`'s source already lives in `src/bin/tauri/main.rs` rather than `src/bin/host-tauri.rs`) — moving these two files one level deeper, with no matching `[[bin]]` entry, removes them from the build entirely while keeping them in the repo as reference.

Add a short header comment to the top of each moved file marking it clearly:

In `crates/ledgerr-host/src/bin/legacy/host-window.rs`, prepend:
```rust
// DEPRECATED, REFERENCE ONLY — not part of the build (no [[bin]] entry,
// and this directory is outside Cargo's src/bin/*.rs auto-discovery).
// Superseded by host-tauri.exe's integrated webview UI. Kept for
// reference only; may bit-rot as the rest of the crate changes.
//
```

In `crates/ledgerr-host/src/bin/legacy/host-tray.rs`, prepend:
```rust
// DEPRECATED, REFERENCE ONLY — not part of the build (no [[bin]] entry,
// and this directory is outside Cargo's src/bin/*.rs auto-discovery).
// Superseded by host-tauri.exe, whose Windows tray now uses the same
// tray::runtime::run() this binary used. Kept for reference only; may
// bit-rot as the rest of the crate changes.
//
```

- [ ] **Step 3: Sync and verify the build**

```bash
cp /home/brianh/promptexecution/_b00t_/vendor/ledgrrr/crates/ledgerr-host/Cargo.toml /mnt/d/promptjects/ledgrrr/crates/ledgerr-host/Cargo.toml
rm -f /mnt/d/promptjects/ledgrrr/crates/ledgerr-host/src/bin/host-window.rs /mnt/d/promptjects/ledgrrr/crates/ledgerr-host/src/bin/host-tray.rs
mkdir -p /mnt/d/promptjects/ledgrrr/crates/ledgerr-host/src/bin/legacy
cp /home/brianh/promptexecution/_b00t_/vendor/ledgrrr/crates/ledgerr-host/src/bin/legacy/host-window.rs /mnt/d/promptjects/ledgrrr/crates/ledgerr-host/src/bin/legacy/host-window.rs
cp /home/brianh/promptexecution/_b00t_/vendor/ledgrrr/crates/ledgerr-host/src/bin/legacy/host-tray.rs /mnt/d/promptjects/ledgrrr/crates/ledgerr-host/src/bin/legacy/host-tray.rs
```
```powershell
$env:PATH = "C:\Users\gru3h\.cargo\bin;" + $env:PATH
$env:TMP="D:\promptjects\tmp"; $env:TEMP="D:\promptjects\tmp"
Set-Location "D:\promptjects\ledgrrr"
cargo clean -p ledgerr-host
cargo test -p ledgerr-host --lib --tests --bins -j 2
Get-ChildItem target\debug\host-window.exe,target\debug\host-tray.exe -ErrorAction SilentlyContinue
```
Expected: full test suite still green (same counts as Task 1's Step 6 — nothing in this task touches tested logic). The `Get-ChildItem` for the two `.exe` files must report nothing found (or an error) — proving they're no longer built. `cargo clean -p ledgerr-host` first ensures we're not looking at stale `.exe` files left over from before this task.

- [ ] **Step 4: Commit**

```bash
cd /home/brianh/promptexecution/_b00t_/vendor/ledgrrr
git add crates/ledgerr-host/Cargo.toml crates/ledgerr-host/src/bin/legacy/
git commit -m "chore(tray): retire host-window.exe and host-tray.exe as build targets

Both are superseded by host-tauri.exe (Task 1-2). Source moved to
src/bin/legacy/ for reference — outside Cargo's src/bin/*.rs
auto-discovery, so neither compiles as part of any normal build."
```

---

### Task 4: Update `Justfile` recipes that reference the removed binaries

**Files:**
- Modify: `Justfile` (repo root)

**Interfaces:** none — this is tooling/docs, no code interface changes.

- [ ] **Step 1: Find and update the affected recipes**

```bash
grep -n "host-window\|host-tray" /home/brianh/promptexecution/_b00t_/vendor/ledgrrr/Justfile
```

As of this plan's writing, five recipes reference the removed binaries by name: `wsl2-pwsh-build`, `wsl2-pwsh-install`, `wsl2-pwsh-run-tray`, `wsl2-pwsh-run-window`, `wsl2-pwsh-run-window-phi4` (all already hardcoded to a different developer's machine paths — `C:\Users\wendy\...`, `D:\Projects\l3dg3rr` — and not directly runnable on this machine as-is regardless of this task).

Change `wsl2-pwsh-build`'s recipe body from building `--bin host-tray --bin host-window` to `--bin host-tauri`.

Change `wsl2-pwsh-install`'s recipe body's `cargo build -p ledgerr-host --bin host-tray --bin host-window` to `cargo build -p ledgerr-host --bin host-tauri`, and its final `Get-Item` listing from `"target\debug\host-tray.exe","target\debug\host-window.exe","target\debug\ledgerr-mcp-server.exe"` to `"target\debug\host-tauri.exe","target\debug\ledgerr-mcp-server.exe"`.

Change `wsl2-pwsh-run-tray`'s recipe body to build and launch `host-tauri` instead of `host-tray` (same `Start-Process`/`Stop-Process` shape, target renamed).

Delete `wsl2-pwsh-run-window` and `wsl2-pwsh-run-window-phi4` entirely — there is no replacement target; the Slint window they launched no longer builds.

Update the two doc comments directly above `wsl2-pwsh-build` (currently "Build the Windows host binaries from WSL via PowerShell. This is the canonical path for `host-tray.exe` and `host-window.exe`.") and above `wsl2-pwsh-run-tray`/`wsl2-pwsh-run-window` to reflect that `host-tauri.exe` is now the canonical target, and that `host-window`/`host-tray` are retired (point at `docs/superpowers/specs/2026-08-29-tray-tauri-integration-design.md` for why).

Check `host-playbook-window`, `host-playbook-window-phi4`, and `host-playbook-window-windows-ai` (which call `just wsl2-pwsh-run-window`) — since that recipe is being deleted, these three recipes now reference a missing recipe and must also be removed or repointed at `wsl2-pwsh-run-tray` (now aliased to `host-tauri`), whichever preserves their stated intent ("launch the host window" → now means "launch host-tauri"). Repoint them at `wsl2-pwsh-run-tray`.

- [ ] **Step 2: Verify no other recipe references a deleted recipe name**

```bash
grep -n "wsl2-pwsh-run-window\b" /home/brianh/promptexecution/_b00t_/vendor/ledgrrr/Justfile
```
Expected: no output (all references either updated or the recipe itself deleted).

- [ ] **Step 3: Commit**

```bash
cd /home/brianh/promptexecution/_b00t_/vendor/ledgrrr
git add Justfile
git commit -m "chore(justfile): repoint host-window/host-tray recipes at host-tauri

Those binaries no longer exist as build targets (see prior commit).
Recipes that built/launched them now target host-tauri instead;
recipes with no host-tauri equivalent (the Slint-window-specific ones)
are removed."
```

---

### Task 5: Merge PR #209, final live verification checklist

**Files:** none — this task is process, not code.

- [ ] **Step 1: Merge PR #209**

```bash
gh pr merge 209 --repo PromptExecution/ledgrrr --merge
```
(Use `--merge`, not `--squash`/`--rebase`, unless the repo's branch protection requires a specific method — check `gh pr view 209 --repo PromptExecution/ledgrrr --json mergeStateStatus` first if the plain merge is rejected.)

Expected: PR #209 shows as merged. Leave PR #187 (`spike/sysml-v2-parser-roundtrip`, draft) untouched — do not merge it.

- [ ] **Step 2: Run the final verification checklist**

This is the checklist the validation subagent (Task 6, below, or a separate agent per the execution handoff) runs independently against. Every item must be checked by actually doing it, not by inspecting code:

```powershell
$env:PATH = "C:\Users\gru3h\.cargo\bin;" + $env:PATH
$env:TMP="D:\promptjects\tmp"; $env:TEMP="D:\promptjects\tmp"
Set-Location "D:\promptjects\ledgrrr"
cargo test -p ledgerr-host --lib --tests --bins -j 2
```
- [ ] All tests pass, zero failures, zero compile errors.

```powershell
Get-ChildItem target\debug\host-window.exe,target\debug\host-tray.exe -ErrorAction SilentlyContinue
```
- [ ] Neither file exists.

```powershell
Get-Process host-tauri -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 300
cargo build -p ledgerr-host --bin host-tauri -j 2
Start-Process -FilePath "D:\promptjects\ledgrrr\target\debug\host-tauri.exe" -WorkingDirectory "D:\promptjects\ledgrrr"
Start-Sleep -Seconds 3
Get-Process host-tauri | Select-Object Id,Responding
```
- [ ] Process alive and `Responding: True`.
- [ ] **Manually right-click the tray icon.** Menu shows the full set (version/backend/last-test info rows, toast toggle, cycle backend, start-minimized, window-visible, the "Notify me for" submenu with its 4 items, test toast, status, show window, exit) — not just "Show Window"/"Exit".
- [ ] **Manually click "Test Toast."** A real Windows toast notification appears. (This is the specific thing reported broken — it must visibly work here.)
- [ ] **Manually click "Show Window."** The Tauri app's own webview window appears and gets focus — no separate `host-window.exe` process is spawned (confirm via `Get-Process host-window -ErrorAction SilentlyContinue` returning nothing).
- [ ] `gh pr view 209 --repo PromptExecution/ledgrrr --json state` reports `"MERGED"`.
- [ ] `gh pr view 187 --repo PromptExecution/ledgrrr --json state,isDraft` still reports `"OPEN"` / `true` — untouched.

- [ ] **Step 3: Report results**

Summarize which checklist items passed/failed, with the actual command output for each, not just a checkmark claim.

---

## Self-Review Notes

- **Spec coverage:** injectable show-window action (Task 1), host-tauri wiring (Task 2), binary retirement (Task 3), Justfile fallout (Task 4, identified during design exploration, included per spec's "not leave them silently broken"), PR merge (Task 5). All spec sections covered.
- **Type consistency:** `run()`'s `show_window: impl Fn() -> Result<(), Box<dyn std::error::Error>>` (Task 1) matches the closure Task 2 passes into it and the `&dyn Fn(...)` `handle_command` receives — verified against the exact `Result`/`Box<dyn Error>` return type used throughout the existing file's other functions (`handle_command`, `run_notification_test`, etc.), so no new error-type mismatches are introduced.
- **No placeholders:** every step above shows literal code, not a description of code.
