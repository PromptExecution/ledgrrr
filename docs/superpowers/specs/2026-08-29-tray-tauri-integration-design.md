# Tray / Tauri Integration, Slint Deprecation, Toast Fix

Status: approved (in-chat design), pending spec review
Branch: `feat/windows-desktop-dogfood`
Crate: `crates/ledgerr-host`

## Problem

`ledgerr-host` currently ships three separate, divergent tray/window
implementations instead of one:

1. `tray::runtime.rs` + `tray::native.rs` — the standalone `host-tray.exe`
   binary. Full-featured: all notification toggles, backend cycling, test
   toast, status display. Its "Show Window" command spawns `host-window.exe`
   (the legacy Slint UI) as a child process.
2. `bin/tauri/tray.rs`, Windows path (`#[cfg(windows)]`) — used by
   `host-tauri.exe`, the actual main/default-run app. A **minimal stub**:
   only "Show Window" and "Exit" are wired; every notification/toast/toggle
   label is left blank (`String::new()` / `""`). Comment in the source
   admits this: "Most notification-specific items are left empty since this
   is a minimal tray surface."
3. `bin/tauri/tray.rs`, non-Windows path (`#[cfg(not(windows))]`) — a third,
   different implementation using Tauri's own cross-platform
   `TrayIconBuilder`, with a completely different menu built from
   `ledgerr_desktop_agent::status::collect()` (service/package/model/
   controller/b00t status rather than notification toggles).

This directly explains the reported "desktop tray toast does not work":
whoever tests `host-tauri.exe` (the real app) is hitting implementation #2,
which never wired toast/notifications at all — not a bug in the toast code,
a gap in the integration. `host-tray.exe` (implementation #1, full toast
support) is a legacy standalone binary predating the Tauri app, now
redundant, and its "Show Window" still points at the deprecated Slint UI.

## Goal

One canonical tray for `host-tauri.exe` on Windows, with full parity to
implementation #1's feature set, but showing the Tauri webview instead of
spawning the legacy Slint `host-window.exe`. Retire the standalone
`host-tray.exe` and `host-window.exe` binaries as build targets (source
kept for reference, not compiled).

Out of scope: the non-Windows tray (implementation #3) — already different,
already working, not part of this problem.

## Design

### Make the window-show action injectable

`runtime.rs::run()` currently owns the whole process (standalone binary's
`main()`) and its `TrayCommand::ShowWindow` handler hardcodes
`show_window_process()`, which spawns `host-window.exe` next to the current
executable.

Change `run()`'s signature to accept how to show a window:

```rust
pub fn run(
    store: SettingsStore,
    show_window: impl Fn() -> Result<(), Box<dyn std::error::Error>> + Send + 'static,
) -> Result<(), Box<dyn std::error::Error>>
```

Thread `show_window` down into `handle_command`'s `TrayCommand::ShowWindow`
arm in place of the `show_window_process()` call. Delete
`show_window_process()` and the `current_exe()`/`with_file_name` logic
entirely — it has no valid target once `host-window.exe` stops being built.

`handle_command`, `negate`, `apply_toggle`, and every `TrayCommand` variant
are otherwise unchanged — this is purely swapping one hardcoded side effect
for an injected one.

### Wire host-tauri's Windows tray through the shared runtime

Replace `bin/tauri/tray.rs`'s `#[cfg(windows)] setup_tray` — currently a
hand-rolled thread loop matching only `CMD_SHOW_WINDOW`/`CMD_EXIT` — with:

```rust
let store = SettingsStore::new(default_settings_path());
std::thread::spawn(move || {
    let app_handle_for_show = app_handle.clone();
    let result = tray::runtime::run(store, move || {
        if let Some(window) = app_handle_for_show.get_webview_window("main") {
            window.show()?;
            window.set_focus()?;
        }
        Ok(())
    });
    if let Err(e) = result {
        eprintln!("[tray] tray runtime exited with error: {e}");
    }
    app_handle.exit(0);
});
```

This gives `host-tauri.exe` the exact same tested toggle/notification/toast
menu as the old `host-tray.exe`, for free, with zero duplicated logic.
`TrayMenuLabels` construction moves from the hand-built minimal struct in
`tray.rs` into whatever `runtime::run()` already builds internally from
`TrayState`/`tray_menu_labels()` — `tray.rs` no longer constructs labels at
all.

### Retire the standalone binaries

- Remove the `host-window` and `host-tray` `[[bin]]` entries from
  `crates/ledgerr-host/Cargo.toml`.
- Move `src/bin/host-window.rs` and `src/bin/host-tray.rs` into
  `src/bin/legacy/` (mirroring how `host-tauri` already lives in
  `src/bin/tauri/` specifically to avoid Cargo's flat `src/bin/*.rs`
  auto-discovery). Files are kept as reference; they will not compile as
  part of any normal `cargo build`/`cargo test` invocation.
- Update the `Justfile`: `wsl2-pwsh-build`, `wsl2-pwsh-install`,
  `wsl2-pwsh-run-tray`, `wsl2-pwsh-run-window`,
  `wsl2-pwsh-run-window-phi4` currently build/run `host-tray`/`host-window`
  by name and would break outright (no such bin target) once removed.
  Repoint the tray-flavored recipes (`wsl2-pwsh-run-tray`,
  `wsl2-pwsh-build`, `wsl2-pwsh-install`) at `host-tauri`; remove the
  window-flavored recipes (`wsl2-pwsh-run-window*`) since there is no
  replacement target for the standalone Slint window.

### Error handling

Unchanged patterns: `run()` still returns `Result`, `handle_command` still
propagates via `?`, tray-creation failure still logs via `eprintln!` rather
than panicking (matches the existing non-Windows tray's `.expect()` on
unrecoverable setup failures being the outlier, not the norm).

### Testing

The 112 existing tests (`tray::runtime::tests::*`, `settings_backend::*`,
etc.) test `handle_command`/`negate`/`apply_toggle` as pure logic already
decoupled from *how* a window gets shown (tests already pass a bare
`mpsc::Sender` and inspect `TrayControl` messages, never touching window
display) — they need no changes and must keep passing unmodified as the
regression gate for this refactor.

Add: a test that `TrayCommand::ShowWindow` invokes exactly the injected
`show_window` closure (spy closure incrementing a counter / setting a flag),
proving the injection point works independent of any real window.

Final acceptance is a live build + launch of `host-tauri.exe`:
- Tray menu shows the full toggle/notification set (not just Show/Exit).
- Right-click → menu renders correctly (including the existing "Notify me
  for" submenu from this session's earlier work).
- Test Toast produces a real toast (the thing that was reported broken).
- Show Window shows the Tauri webview, not a spawned Slint process.
- `host-window.exe`/`host-tray.exe` are absent from `target/debug/` after a
  clean build (proving they're no longer built).

## PR housekeeping (independent of the above)

- Merge #209 (`feat/chat-mcp-tool-loop` → `main`): all CI green
  (test-and-build, Kani, clippy, Windows desktop TestInstall, docs build),
  not a draft. No review approval recorded, but per operator direction this
  gets merged now rather than blocked on that.
- Leave #187 (`spike/sysml-v2-parser-roundtrip` → `main`) open: explicitly a
  draft spike; merging drafts violates the convention the author signaled
  with that title/state.

## Execution strategy

A build+test subagent implements the design above and runs the full test
suite + a live `host-tauri.exe` launch. A second, independent subagent
validates the result against this spec's acceptance criteria before it's
considered done — it does not implement, only checks.
