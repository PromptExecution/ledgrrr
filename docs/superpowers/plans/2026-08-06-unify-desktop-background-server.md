# Unify Desktop Background Server (gh#118) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `ledgrrr-service` (crate `ledgerr-desktop-agent`, binary `ledgrrr-service`) the one long-lived local background process that owns settings/chat/review-log/evidence state, and make `host-tauri` and `host-tray` (crate `ledgerr-host`) HTTP clients of it instead of each owning independent local state.

**Architecture:** `ledgrrr-service` grows a hand-rolled HTTP server (same pattern as the existing `internal_openai.rs` endpoint — nonblocking `TcpListener`, no async runtime) bound to `127.0.0.1:15116` by default. State types currently defined in `ledgerr-host` (settings, notification sub-types, model-provider label) move into a new dependency-light crate, `ledgrrr-settings`, so `ledgerr-desktop-agent` can construct/serve them without pulling in `ledgerr-host`'s heavy Windows/Slint/LLM dependency graph. `ledgerr-host`'s `settings`/`notify`/`internal_openai` modules become thin re-export shims over the new crate so every existing call site keeps compiling unchanged.

**Tech Stack:** Rust, `std::net::TcpListener` (no async runtime added), `reqwest::blocking` (already a workspace dependency with `blocking`+`json` features enabled) for the client side, `serde_json` for the wire format.

## Global Constraints

- No `unsafe_code` (workspace lint: `deny`).
- `ledgrrr-settings` must not depend on `windows`, `slint`, `tauri`, `mistralrs`, `candle-*`, `tokio`, or any other heavy/platform-specific crate — `serde`, `serde_json`, `thiserror`, `chrono`, and (Windows-only, already lightweight) `windows-registry` are the ceiling.
- Every existing `ledgerr_host::settings::*`, `ledgerr_host::notify::*`, and root-level `ledgerr_host::ModelProviderLabel` re-export must keep resolving after this plan — this is a refactor, not a breaking API change, and `crates/ledgerr-host/src/bin/tauri/commands.rs` / `main.rs` / `crates/ledgerr-host/src/settings/schema.rs` (`internal_openai::ModelProviderLabel` import) must not need edits beyond what Task 8/9 explicitly call out.
- New HTTP surface binds to `127.0.0.1` only (never `0.0.0.0`) — this is a local-only control surface, matching the existing `internal_openai.rs` endpoint's own binding discipline.
- `cargo test --workspace` and `cargo clippy --workspace --all-targets --all-features` must stay green after every task's commit.

---

## Roadmap (this plan covers Phase A only)

| Phase | Scope | Status |
|---|---|---|
| **A** | IPC scaffold on `ledgrrr-service` + settings migration (this plan) | Detailed below |
| B | Migrate chat history + review log (`ledgerr_host::chat` — `send_chat_message`, `ChatTurn`, `ReviewLog`) to be served by `ledgrrr-service` | Not yet planned |
| C | Migrate evidence state (`ledgerr_host::evidence::EvidenceState`) to be served by `ledgrrr-service` | Not yet planned |
| D | Retire `host-tauri`'s local `AppState` entirely; tray icon (both the Windows-native path in `ledgerr-host/src/tray/native.rs` and the non-Windows Tauri `TrayIconBuilder` path) displays live `ledgrrr_status`-derived state instead of static menu items | Not yet planned |

`ledgerr-mcp-server` (the domain MCP server) is explicitly **out of scope** for all phases — it stays stdio-only. `host-window` (the legacy Slint binary) is not touched by Phase A; its own migration is deferred to a later phase, noted but not scheduled above.

---

## File Structure (Phase A)

New:
- `crates/ledgrrr-settings/Cargo.toml`, `crates/ledgrrr-settings/src/lib.rs` — new crate.
- `crates/ledgrrr-settings/src/backend/{mod.rs, json_file.rs, windows_registry.rs}` — moved from `crates/ledgerr-host/src/settings_backend.rs` + its `json_file`/`windows_registry` submodules.
- `crates/ledgrrr-settings/src/schema.rs` — moved from `crates/ledgerr-host/src/settings/schema.rs`, minus `AppSettings::resolve_chat`.
- `crates/ledgrrr-settings/src/store.rs` — moved from `crates/ledgerr-host/src/settings/store.rs`.
- `crates/ledgrrr-settings/src/path.rs` — moved from `crates/ledgerr-host/src/settings/path.rs`.
- `crates/ledgrrr-settings/src/model_provider.rs` — `ModelProviderLabel` moved from `crates/ledgerr-host/src/internal_openai.rs`.
- `crates/ledgrrr-settings/src/notification.rs` — `NotificationBackend`, `NotificationStatus`, `NotificationTestResult` moved from `crates/ledgerr-host/src/notify/types.rs`.
- `crates/ledgerr-desktop-agent/src/settings_server.rs` — the new HTTP server module.

Modified:
- `Cargo.toml` (workspace root) — add `crates/ledgrrr-settings` to `members`.
- `crates/ledgerr-desktop-agent/Cargo.toml` — add `ledgrrr-settings` and `reqwest` is NOT needed here (server side only writes responses, doesn't make outbound calls).
- `crates/ledgerr-host/Cargo.toml` — add `ledgrrr-settings` path dependency.
- `crates/ledgerr-host/src/settings/mod.rs`, `crates/ledgerr-host/src/settings_backend.rs`, `crates/ledgerr-host/src/notify/types.rs`, `crates/ledgerr-host/src/internal_openai.rs` — become re-export shims (see Task 4).
- `crates/ledgerr-desktop-agent/src/status.rs` — fix `TRAY_CANDIDATES` bug.
- `crates/ledgerr-desktop-agent/src/bin/ledgrrr-service.rs` — wire in the settings server alongside the heartbeat loop.
- `crates/ledgerr-host/src/bin/tauri/state.rs`, `main.rs`, `commands.rs` — `AppState.store` becomes an HTTP client wrapper instead of a local `SettingsStore`.
- `crates/ledgerr-host/src/bin/host-tray.rs` — same client swap.

---

### Task 1: Create `ledgrrr-settings` crate + move the storage backend

**Files:**
- Create: `crates/ledgrrr-settings/Cargo.toml`
- Create: `crates/ledgrrr-settings/src/lib.rs`
- Create: `crates/ledgrrr-settings/src/backend/mod.rs` (from `crates/ledgerr-host/src/settings_backend.rs`)
- Create: `crates/ledgrrr-settings/src/backend/json_file.rs` (from `crates/ledgerr-host/src/settings_backend/json_file.rs`)
- Create: `crates/ledgrrr-settings/src/backend/windows_registry.rs` (from `crates/ledgerr-host/src/settings_backend/windows_registry.rs`, Windows-only)
- Modify: `Cargo.toml` (workspace root, `members` array)
- Delete: `crates/ledgerr-host/src/settings_backend.rs` and its submodule files (replaced by Task 4's shim)

**Interfaces:**
- Produces: `ledgrrr_settings::backend::{SettingsBackend, SettingsBackendError, create_backend, JsonFileBackend}` (and `WindowsRegistryBackend` on Windows) — same names/signatures as today's `ledgerr_host::settings_backend`, just under the new crate path.

- [ ] **Step 1: Read the exact current file contents to copy verbatim**

Run: `cat crates/ledgerr-host/src/settings_backend.rs crates/ledgerr-host/src/settings_backend/json_file.rs`
(and `crates/ledgerr-host/src/settings_backend/windows_registry.rs` if it exists as a separate file — confirm with `ls crates/ledgerr-host/src/settings_backend/`)

Do not paraphrase — copy the file bodies exactly into the new locations in Step 2. This plan does not reproduce their full text here because they must be copied byte-for-byte, not retyped from a description.

- [ ] **Step 2: Create the new crate**

`crates/ledgrrr-settings/Cargo.toml`:
```toml
[package]
name = "ledgrrr-settings"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
chrono = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }

[target.'cfg(windows)'.dependencies]
windows-registry = "0.6"

[dev-dependencies]
tempfile = { workspace = true }

[lints]
workspace = true
```

`crates/ledgrrr-settings/src/lib.rs`:
```rust
pub mod backend;
pub mod model_provider;
pub mod notification;
pub mod path;
pub mod schema;
pub mod store;

pub use model_provider::ModelProviderLabel;
pub use notification::{NotificationBackend, NotificationStatus, NotificationTestResult};
pub use path::default_settings_path;
pub use schema::{AppSettings, ChatSettings, SettingsSchemaVersion, ShowNotificationsFor};
pub use store::{SettingsError, SettingsStore};
```

Copy `crates/ledgerr-host/src/settings_backend.rs` verbatim to `crates/ledgrrr-settings/src/backend/mod.rs`, and its `json_file`/`windows_registry` submodule files verbatim to `crates/ledgrrr-settings/src/backend/json_file.rs` / `windows_registry.rs`. No content changes in this step — this is a pure file move.

Register in workspace root `Cargo.toml`, in the `members` array, alongside the other `crates/*` entries (exact insertion point: after the `"crates/ledgerr-focus",` line, matching the existing alphabetical-ish grouping):
```toml
  "crates/ledgerr-focus",
  "crates/ledgrrr-settings",
  "crates/ledgerr-host",
```

- [ ] **Step 3: Build to confirm the new crate compiles standalone**

Run: `cargo check -p ledgrrr-settings`
Expected: compiles clean (the crate has no consumers yet, so no downstream breakage possible at this step).

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml crates/ledgrrr-settings
git commit -m "feat(ledgrrr-settings): scaffold new crate, move settings_backend"
```

(`crates/ledgerr-host/src/settings_backend.rs` deletion happens in Task 4, once the shim is in place — do not delete it yet, or `ledgerr-host` stops compiling.)

---

### Task 2: Move notification sub-types

**Files:**
- Modify: `crates/ledgrrr-settings/src/notification.rs` (create, content below)
- Modify: `crates/ledgerr-host/src/notify/types.rs` (remove the three moved items, keep the rest, import the new crate for them)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `ledgrrr_settings::notification::{NotificationBackend, NotificationStatus, NotificationTestResult}`.

- [ ] **Step 1: Write the moved file**

`crates/ledgrrr-settings/src/notification.rs`:
```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationBackend {
    Auto,
    PowerShell,
    Noop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationStatus {
    Disabled,
    Unknown,
    Ready,
    Degraded,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationTestResult {
    pub status: NotificationStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}
```

- [ ] **Step 2: Update `crates/ledgerr-host/src/notify/types.rs`**

Remove the `NotificationBackend`, `NotificationStatus`, `NotificationTestResult` definitions (lines 5-21 and 34-41 in the current file). Add at the top of the file, alongside the existing `use` block:
```rust
pub use ledgrrr_settings::{NotificationBackend, NotificationStatus, NotificationTestResult};
```
Leave `NotificationEvent`, `NotificationSettings`, `NotifyError`, and the `Notifier` trait exactly as they are — they reference the now-imported types by name, which still resolves.

`ledgerr-host`'s `Cargo.toml` needs the new dependency for this to compile — add now (this crate will need it for every remaining task in this plan, so add it once here):
```toml
ledgrrr-settings = { path = "../ledgrrr-settings" }
```

- [ ] **Step 3: Build**

Run: `cargo check -p ledgerr-host --all-features`
Expected: compiles clean. If it doesn't, the error will name every call site that referenced `crate::notify::types::NotificationBackend` (etc.) directly instead of via `crate::notify::NotificationBackend` — fix by adding a matching `pub use` at `crates/ledgerr-host/src/notify/mod.rs` if one doesn't already re-export `types::*`.

- [ ] **Step 4: Run existing tests**

Run: `cargo test -p ledgerr-host --all-features`
Expected: same pass/fail set as before this task (no new failures introduced by a pure type-relocation).

- [ ] **Step 5: Commit**

```bash
git add crates/ledgrrr-settings/src/notification.rs crates/ledgrrr-settings/src/lib.rs \
        crates/ledgerr-host/src/notify/types.rs crates/ledgerr-host/Cargo.toml
git commit -m "refactor(ledgerr-host): move NotificationBackend/Status/TestResult to ledgrrr-settings"
```

---

### Task 3: Move `ModelProviderLabel`

**Files:**
- Create: `crates/ledgrrr-settings/src/model_provider.rs`
- Modify: `crates/ledgerr-host/src/internal_openai.rs` (remove the definition, re-export instead)

**Interfaces:**
- Produces: `ledgrrr_settings::model_provider::ModelProviderLabel` (with its `display_name()` inherent method).

- [ ] **Step 1: Write the moved file**

`crates/ledgrrr-settings/src/model_provider.rs`:
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelProviderLabel {
    /// Private local inference. Works immediately. May use a deterministic stub if no GGUF is configured.
    LocalDemo,
    /// Private local inference via Windows AI / Foundry Local. Requires setup first.
    WindowsAi,
    /// Explicit external API call. Requires operator-supplied endpoint and key.
    Cloud,
}

impl ModelProviderLabel {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::LocalDemo => "Local Demo",
            Self::WindowsAi => "Windows AI",
            Self::Cloud => "Cloud",
        }
    }
}
```

(Read `crates/ledgerr-host/src/internal_openai.rs` lines 46-62 first to confirm the derive list and match arms above are byte-identical to the source before deleting the original — copy exactly, don't retype from memory if the real file differs from what's shown here.)

- [ ] **Step 2: Update `crates/ledgerr-host/src/internal_openai.rs`**

Delete the `ModelProviderLabel` enum + impl block. Add near the top of the file:
```rust
pub use ledgrrr_settings::ModelProviderLabel;
```

- [ ] **Step 3: Build**

Run: `cargo check -p ledgerr-host --all-features`
Expected: clean. `crates/ledgerr-host/src/lib.rs` already does `pub use internal_openai::{..., ModelProviderLabel, ...};` at crate root — this keeps working unchanged since the name still resolves from `internal_openai`, just via re-export now instead of definition.

- [ ] **Step 4: Run tests, commit**

```bash
cargo test -p ledgerr-host --all-features
git add crates/ledgrrr-settings/src/model_provider.rs crates/ledgrrr-settings/src/lib.rs \
        crates/ledgerr-host/src/internal_openai.rs
git commit -m "refactor(ledgerr-host): move ModelProviderLabel to ledgrrr-settings"
```

---

### Task 4: Move `AppSettings`/`SettingsStore`/`default_settings_path`, delete old `settings_backend.rs`

**Files:**
- Create: `crates/ledgrrr-settings/src/schema.rs` (from `crates/ledgerr-host/src/settings/schema.rs`, minus `resolve_chat`)
- Create: `crates/ledgrrr-settings/src/store.rs` (from `crates/ledgerr-host/src/settings/store.rs`)
- Create: `crates/ledgrrr-settings/src/path.rs` (from `crates/ledgerr-host/src/settings/path.rs`)
- Modify: `crates/ledgerr-host/src/settings/mod.rs` → becomes a pure re-export shim
- Modify: `crates/ledgerr-host/src/internal_openai.rs` → add `resolve_chat` as a free function
- Delete: `crates/ledgerr-host/src/settings/schema.rs`, `store.rs`, `path.rs`
- Delete: `crates/ledgerr-host/src/settings_backend.rs` and its submodule files (superseded by Task 1's copy — this is the point where the original is finally removed)

**Interfaces:**
- Consumes: `ledgrrr_settings::backend::{SettingsBackend, SettingsBackendError, create_backend}` (Task 1), `ledgrrr_settings::{ModelProviderLabel, NotificationBackend, NotificationTestResult}` (Tasks 2-3).
- Produces: `ledgrrr_settings::{AppSettings, ChatSettings, SettingsSchemaVersion, ShowNotificationsFor, SettingsStore, SettingsError, default_settings_path}`.

- [ ] **Step 1: Write `crates/ledgrrr-settings/src/schema.rs`**

Copy `crates/ledgerr-host/src/settings/schema.rs` verbatim, with two changes:
1. Replace `use crate::internal_openai::ModelProviderLabel;` with `use crate::model_provider::ModelProviderLabel;`
2. Replace `use crate::notify::{NotificationBackend, NotificationTestResult};` with `use crate::notification::{NotificationBackend, NotificationTestResult};`
3. Delete the `impl AppSettings { pub fn resolve_chat(&self) -> ... }` block entirely (it moves to `ledgerr-host` in Step 4 below, since it needs `internal_openai::resolve_chat_settings` which must not become a dependency of this crate).

- [ ] **Step 2: Write `crates/ledgrrr-settings/src/store.rs` and `src/path.rs`**

Copy `crates/ledgerr-host/src/settings/store.rs` verbatim to `crates/ledgrrr-settings/src/store.rs`, with one change: `use super::schema::{AppSettings, SettingsSchemaVersion};` becomes `use crate::schema::{AppSettings, SettingsSchemaVersion};`, and `use crate::settings_backend::{create_backend, SettingsBackend, SettingsBackendError};` becomes `use crate::backend::{create_backend, SettingsBackend, SettingsBackendError};`. Its existing `#[cfg(test)] mod tests` block (the `load_returns_defaults_when_backend_is_empty` / `save_and_load_roundtrip` tests) copies unchanged — these become the new crate's regression tests.

Copy `crates/ledgerr-host/src/settings/path.rs` verbatim to `crates/ledgrrr-settings/src/path.rs` — no changes needed, it has no internal crate references.

- [ ] **Step 3: Turn `crates/ledgerr-host/src/settings/mod.rs` into a shim**

Replace its entire content with:
```rust
pub use ledgrrr_settings::{
    default_settings_path, AppSettings, ChatSettings, SettingsError, SettingsSchemaVersion,
    SettingsStore, ShowNotificationsFor,
};
```
Delete `crates/ledgerr-host/src/settings/schema.rs`, `store.rs`, `path.rs` (now dead — content lives in `ledgrrr-settings`).

- [ ] **Step 4: Add `resolve_chat` as a free function in `ledgerr-host`**

Confirmed via `grep -rn "\.resolve_chat()" crates/ledgerr-host/` that this method has zero current call sites — it's dead code today, but keep its behavior available under a new name so nothing is silently dropped. In `crates/ledgerr-host/src/internal_openai.rs`, near `resolve_chat_settings`, add:
```rust
/// Resolve ChatSettings from the operator's model_provider choice.
///
/// Returns (resolved_settings, Option<ProviderReadiness>) where the second
/// element is Some when a fallback occurred (e.g., WindowsAi selected but
/// Foundry not installed). The caller decides whether to surface the warning.
pub fn resolve_chat(settings: &ledgrrr_settings::AppSettings) -> (ChatSettings, Option<ProviderReadiness>) {
    resolve_chat_settings(settings)
}
```
(`ChatSettings` here is `crate::settings::ChatSettings`, which after Step 3 resolves via the shim to `ledgrrr_settings::ChatSettings` — no import change needed if `internal_openai.rs` already imports `ChatSettings` from `crate::settings`; if it doesn't, add `use crate::settings::ChatSettings;`.)

- [ ] **Step 5: Delete the old backend files, update `ledgerr-host/src/lib.rs`**

Delete `crates/ledgerr-host/src/settings_backend.rs` and its submodule directory. Remove `pub mod settings_backend;` from `crates/ledgerr-host/src/lib.rs`. If anything outside `settings/` referenced `crate::settings_backend::*` directly, change it to `ledgrrr_settings::backend::*` (search with `grep -rn "settings_backend" crates/ledgerr-host/src/` after the deletion — the build error will also catch any miss).

- [ ] **Step 6: Build**

Run: `cargo check -p ledgerr-host --all-features`
Expected: clean. This is the step most likely to surface a missed call site — fix forward from whatever the compiler names.

- [ ] **Step 7: Run tests**

Run: `cargo test -p ledgrrr-settings && cargo test -p ledgerr-host --all-features`
Expected: `ledgrrr-settings`'s two moved tests pass; `ledgerr-host`'s suite has the same pass/fail set as before Task 1.

- [ ] **Step 8: Commit**

```bash
git add crates/ledgrrr-settings/src crates/ledgerr-host/src/settings crates/ledgerr-host/src/internal_openai.rs \
        crates/ledgerr-host/src/lib.rs
git rm -r crates/ledgerr-host/src/settings_backend.rs crates/ledgerr-host/src/settings_backend/ 2>/dev/null || true
git commit -m "refactor(ledgerr-host): move AppSettings/SettingsStore/default_settings_path to ledgrrr-settings"
```

---

### Task 5: Fix the `TRAY_CANDIDATES` bug

**Files:**
- Modify: `crates/ledgerr-desktop-agent/src/status.rs:99`

**Interfaces:** none (self-contained bugfix, no new interface).

- [ ] **Step 1: Write the failing test**

Add to `crates/ledgerr-desktop-agent/src/status.rs`, inside a `#[cfg(test)] mod tests` block (create one if none exists yet in this file):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_candidates_matches_the_real_host_tauri_binary_name() {
        assert!(
            TRAY_CANDIDATES.contains(&"host-tauri"),
            "TRAY_CANDIDATES must list the real host-tauri bin target, not a nonexistent ledgerr-tauri binary: {TRAY_CANDIDATES:?}"
        );
    }
}
```

- [ ] **Step 2: Run it to confirm it fails**

Run: `cargo test -p ledgerr-desktop-agent tray_candidates_matches_the_real_host_tauri_binary_name`
Expected: FAIL — `TRAY_CANDIDATES` is currently `["host-tray", "ledgerr-tauri"]`.

- [ ] **Step 3: Fix it**

```rust
const TRAY_CANDIDATES: &[&str] = &["host-tray", "host-tauri"];
```

- [ ] **Step 4: Run it to confirm it passes**

Run: `cargo test -p ledgerr-desktop-agent tray_candidates_matches_the_real_host_tauri_binary_name`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/ledgerr-desktop-agent/src/status.rs
git commit -m "fix(ledgerr-desktop-agent): TRAY_CANDIDATES named a binary (ledgerr-tauri) that doesn't exist"
```

---

### Task 6: Add the settings HTTP server to `ledgerr-desktop-agent`

**Files:**
- Create: `crates/ledgerr-desktop-agent/src/settings_server.rs`
- Modify: `crates/ledgerr-desktop-agent/src/lib.rs` (add `pub mod settings_server;`)
- Modify: `crates/ledgerr-desktop-agent/Cargo.toml` (add `ledgrrr-settings` dependency)

**Interfaces:**
- Consumes: `ledgrrr_settings::{SettingsStore, AppSettings, default_settings_path}` (Task 4).
- Produces: `settings_server::{SETTINGS_SERVER_ADDR, route_request, spawn}` — `route_request` is the pure, directly-testable request handler; `spawn` wraps it in the actual `TcpListener` accept loop for `ledgrrr-service.rs` to call in Task 7.

- [ ] **Step 1: Add the dependency**

`crates/ledgerr-desktop-agent/Cargo.toml`, in `[dependencies]`:
```toml
ledgrrr-settings = { path = "../ledgrrr-settings" }
```

- [ ] **Step 2: Write the failing tests for `route_request`**

`crates/ledgerr-desktop-agent/src/settings_server.rs` (test module first):
```rust
//! HTTP settings server for `ledgrrr-service` — same hand-rolled,
//! nonblocking-`TcpListener` style as `ledgerr-host`'s `internal_openai.rs`
//! endpoint. GET /settings returns the current `AppSettings` as JSON;
//! POST /settings replaces them. No async runtime.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use ledgrrr_settings::{AppSettings, SettingsStore};

pub const SETTINGS_SERVER_ADDR: &str = "127.0.0.1:15116";

#[cfg(test)]
mod tests {
    use super::*;

    fn store_with_defaults() -> SettingsStore {
        let dir = tempfile::tempdir().unwrap();
        SettingsStore::new(dir.path().join("settings.json"))
    }

    #[test]
    fn get_settings_returns_defaults_as_json() {
        let store = store_with_defaults();
        let response = route_request(b"GET /settings HTTP/1.1\r\n\r\n", &store);
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        let body_start = response.find("\r\n\r\n").unwrap() + 4;
        let parsed: AppSettings = serde_json::from_str(&response[body_start..]).unwrap();
        assert!(parsed.toast_enabled);
    }

    #[test]
    fn post_settings_persists_and_get_reflects_it() {
        let store = store_with_defaults();
        let mut updated = store.load().unwrap();
        updated.toast_enabled = false;
        let body = serde_json::to_string(&updated).unwrap();
        let request = format!(
            "POST /settings HTTP/1.1\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );

        let post_response = route_request(request.as_bytes(), &store);
        assert!(post_response.starts_with("HTTP/1.1 200 OK"));

        let get_response = route_request(b"GET /settings HTTP/1.1\r\n\r\n", &store);
        let body_start = get_response.find("\r\n\r\n").unwrap() + 4;
        let parsed: AppSettings = serde_json::from_str(&get_response[body_start..]).unwrap();
        assert!(!parsed.toast_enabled);
    }

    #[test]
    fn post_settings_rejects_malformed_json() {
        let store = store_with_defaults();
        let body = "{not json";
        let request = format!(
            "POST /settings HTTP/1.1\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let response = route_request(request.as_bytes(), &store);
        assert!(response.starts_with("HTTP/1.1 400 Bad Request"));
    }

    #[test]
    fn unknown_route_returns_404() {
        let store = store_with_defaults();
        let response = route_request(b"GET /nope HTTP/1.1\r\n\r\n", &store);
        assert!(response.starts_with("HTTP/1.1 404 Not Found"));
    }
}
```

Add `tempfile = { workspace = true }` to `crates/ledgerr-desktop-agent/Cargo.toml`'s `[dev-dependencies]` (create that section if it doesn't exist) — needed for the tests above.

- [ ] **Step 3: Run tests to confirm they fail**

Run: `cargo test -p ledgerr-desktop-agent settings_server`
Expected: FAIL to compile — `route_request` doesn't exist yet.

- [ ] **Step 4: Implement `route_request` and the accept-loop `spawn`**

Append to `crates/ledgerr-desktop-agent/src/settings_server.rs` (above the `#[cfg(test)]` module):
```rust
fn json_response(status: u16, payload: &impl serde::Serialize) -> String {
    let body = serde_json::to_string(payload)
        .unwrap_or_else(|_| "{\"error\":\"serialization failure\"}".to_string());
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "OK",
    };
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn parse_content_length(headers: &str) -> Option<usize> {
    headers.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("content-length")
            .then(|| value.trim().parse().ok())
            .flatten()
    })
}

fn route_request(raw: &[u8], store: &SettingsStore) -> String {
    let Some(header_end) = find_header_end(raw) else {
        return json_response(400, &serde_json::json!({ "error": "invalid request" }));
    };
    let headers = String::from_utf8_lossy(&raw[..header_end]);
    let request_line = headers.lines().next().unwrap_or_default();
    let body = &raw[header_end + 4..];

    if request_line.starts_with("GET /settings ") || request_line.starts_with("GET /settings HTTP") {
        return match store.load() {
            Ok(settings) => json_response(200, &settings),
            Err(error) => json_response(500, &serde_json::json!({ "error": error.to_string() })),
        };
    }

    if request_line.starts_with("POST /settings ") || request_line.starts_with("POST /settings HTTP") {
        let settings: AppSettings = match serde_json::from_slice(body) {
            Ok(settings) => settings,
            Err(error) => {
                return json_response(
                    400,
                    &serde_json::json!({ "error": format!("invalid settings body: {error}") }),
                );
            }
        };
        return match store.save(&settings) {
            Ok(()) => json_response(200, &settings),
            Err(error) => json_response(500, &serde_json::json!({ "error": error.to_string() })),
        };
    }

    json_response(404, &serde_json::json!({ "error": "not found" }))
}

fn request_complete(buffer: &[u8]) -> bool {
    let Some(header_end) = find_header_end(buffer) else {
        return false;
    };
    let headers = String::from_utf8_lossy(&buffer[..header_end]);
    let content_length = parse_content_length(&headers).unwrap_or_default();
    buffer.len() >= header_end + 4 + content_length
}

fn handle_stream(mut stream: TcpStream, store: &SettingsStore) {
    let mut buffer = Vec::with_capacity(4096);
    let mut chunk = [0_u8; 2048];
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buffer.extend_from_slice(&chunk[..n]);
                if request_complete(&buffer) {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let response = route_request(&buffer, store);
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

/// Bind the settings server and return the live listener, already set to
/// nonblocking so the caller can interleave `accept()` polling with other
/// periodic work (the heartbeat write in `ledgrrr-service`'s main loop).
pub fn bind() -> std::io::Result<TcpListener> {
    let listener = TcpListener::bind(SETTINGS_SERVER_ADDR)?;
    listener.set_nonblocking(true)?;
    Ok(listener)
}

/// Poll the listener once. Call this in a loop; returns immediately if no
/// connection is pending (`WouldBlock`) rather than blocking, so the caller
/// stays free to also run heartbeat writes on the same thread.
pub fn accept_once(listener: &TcpListener, store: &SettingsStore) {
    match listener.accept() {
        Ok((stream, _)) => handle_stream(stream, store),
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
        Err(_) => {}
    }
}
```

Add `pub mod settings_server;` to `crates/ledgerr-desktop-agent/src/lib.rs`.

- [ ] **Step 5: Run tests to confirm they pass**

Run: `cargo test -p ledgerr-desktop-agent settings_server`
Expected: PASS — all 4 tests (`get_settings_returns_defaults_as_json`, `post_settings_persists_and_get_reflects_it`, `post_settings_rejects_malformed_json`, `unknown_route_returns_404`).

- [ ] **Step 6: Commit**

```bash
git add crates/ledgerr-desktop-agent/src/settings_server.rs crates/ledgerr-desktop-agent/src/lib.rs \
        crates/ledgerr-desktop-agent/Cargo.toml
git commit -m "feat(ledgerr-desktop-agent): add settings HTTP server (GET/POST /settings)"
```

---

### Task 7: Wire the settings server into `ledgrrr-service`'s main loop

**Files:**
- Modify: `crates/ledgerr-desktop-agent/src/bin/ledgrrr-service.rs`

**Interfaces:**
- Consumes: `settings_server::{bind, accept_once}` (Task 6), `ledgrrr_settings::{SettingsStore, default_settings_path}` (Task 4).

- [ ] **Step 1: Read the current file to confirm nothing has drifted since this plan's research**

Run: `cat crates/ledgerr-desktop-agent/src/bin/ledgrrr-service.rs`
Expected content (as of this plan's research — confirm it still matches before editing):
```rust
use ledgerr_desktop_agent::state;
use std::time::Duration;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);

fn main() {
    let pid = std::process::id();
    let started_at = state::now();
    loop {
        let _ = state::write_heartbeat(pid, started_at);
        std::thread::sleep(HEARTBEAT_INTERVAL);
    }
}
```
(No signal handler by design, per the file's own header comment — Phase A does not change that.)

- [ ] **Step 2: Replace the loop body**

```rust
use ledgerr_desktop_agent::{settings_server, state};
use ledgrrr_settings::{default_settings_path, SettingsStore};
use std::time::{Duration, Instant};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(25);

fn main() {
    let pid = std::process::id();
    let started_at = state::now();
    let store = SettingsStore::new(default_settings_path());

    let listener = match settings_server::bind() {
        Ok(listener) => Some(listener),
        Err(error) => {
            eprintln!(
                "ledgrrr-service: failed to bind settings server on {}: {error} — heartbeat only, no settings HTTP surface this run",
                settings_server::SETTINGS_SERVER_ADDR
            );
            None
        }
    };

    let mut last_heartbeat = Instant::now() - HEARTBEAT_INTERVAL;
    loop {
        if last_heartbeat.elapsed() >= HEARTBEAT_INTERVAL {
            let _ = state::write_heartbeat(pid, started_at);
            last_heartbeat = Instant::now();
        }
        if let Some(listener) = &listener {
            settings_server::accept_once(listener, &store);
        }
        std::thread::sleep(ACCEPT_POLL_INTERVAL);
    }
}
```

Bind failure (e.g., port already in use by another `ledgrrr-service` instance) degrades to heartbeat-only rather than crashing the process — matches PRD-11 §7's "never a silent no-op or fabricated success" only in the sense that the failure is logged to stderr, not swallowed; the service still provides its one previously-guaranteed function (liveness heartbeat) rather than dying entirely over a secondary feature.

- [ ] **Step 3: Build**

Run: `cargo build -p ledgerr-desktop-agent --bin ledgrrr-service`
Expected: compiles clean.

- [ ] **Step 4: Manual smoke test**

Run: `LEDGRRR_STATE_DIR=$(mktemp -d) ./target/debug/ledgrrr-service &`
Then: `sleep 1 && curl -s http://127.0.0.1:15116/settings | head -c 200`
Expected: JSON starting with `{"schema_version":"v2",...}` (or similar — the actual `AppSettings` default JSON). Kill the background process afterward: `kill %1`.

- [ ] **Step 5: Commit**

```bash
git add crates/ledgerr-desktop-agent/src/bin/ledgrrr-service.rs
git commit -m "feat(ledgerr-desktop-agent): serve settings HTTP endpoint from ledgrrr-service's main loop"
```

---

### Task 8: Switch `host-tauri` to be an HTTP client of the settings endpoint

**Files:**
- Modify: `crates/ledgerr-host/src/bin/tauri/state.rs`
- Modify: `crates/ledgerr-host/src/bin/tauri/main.rs`
- Modify: `crates/ledgerr-host/src/bin/tauri/commands.rs`
- Create: `crates/ledgerr-host/src/bin/tauri/settings_client.rs`

**Interfaces:**
- Consumes: `ledgrrr_settings::AppSettings` (Task 4), `settings_server::SETTINGS_SERVER_ADDR` (Task 6, for the default URL — reused as a constant, not a runtime dependency on the crate itself, to avoid `ledgerr-host` depending on `ledgerr-desktop-agent`; the literal `127.0.0.1:15116` is duplicated as a `const` here, matching how `INTERNAL_OPENAI_ADDR` is already a freestanding const in `internal_openai.rs` rather than shared across crates).
- Produces: `settings_client::{SettingsClient, SettingsClientError}` with `load() -> Result<AppSettings, SettingsClientError>` and `save(&AppSettings) -> Result<(), SettingsClientError>` — same two operations `AppState.store` exposed before, so `commands.rs` call sites change their receiver but not their call shape.

- [ ] **Step 1: Write the failing test**

`crates/ledgerr-host/src/bin/tauri/settings_client.rs`:
```rust
//! HTTP client for `ledgrrr-service`'s settings endpoint (see
//! `ledgerr_desktop_agent::settings_server` for the server side). Replaces
//! the local `SettingsStore` this binary used to own directly — settings
//! are now `ledgrrr-service`'s state, not `host-tauri`'s.

use ledgrrr_settings::AppSettings;

const DEFAULT_SETTINGS_SERVER_URL: &str = "http://127.0.0.1:15116";

#[derive(Debug, thiserror::Error)]
pub enum SettingsClientError {
    #[error("request to ledgrrr-service failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("ledgrrr-service returned an error: {0}")]
    Server(String),
}

pub struct SettingsClient {
    base_url: String,
    client: reqwest::blocking::Client,
}

impl SettingsClient {
    pub fn new() -> Self {
        Self::with_base_url(DEFAULT_SETTINGS_SERVER_URL.to_string())
    }

    pub fn with_base_url(base_url: String) -> Self {
        Self {
            base_url,
            client: reqwest::blocking::Client::new(),
        }
    }

    pub fn load(&self) -> Result<AppSettings, SettingsClientError> {
        let response = self
            .client
            .get(format!("{}/settings", self.base_url))
            .send()?;
        if !response.status().is_success() {
            return Err(SettingsClientError::Server(response.status().to_string()));
        }
        Ok(response.json()?)
    }

    pub fn save(&self, settings: &AppSettings) -> Result<(), SettingsClientError> {
        let response = self
            .client
            .post(format!("{}/settings", self.base_url))
            .json(settings)
            .send()?;
        if !response.status().is_success() {
            return Err(SettingsClientError::Server(response.status().to_string()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// Minimal fake server: accepts exactly one connection, replies with a
    /// fixed body, then stops. Enough to test the client without depending
    /// on ledgerr-desktop-agent (which would create a dependency cycle risk
    /// — ledgerr-host must not depend on ledgerr-desktop-agent).
    fn fake_server_returning(body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0_u8; 1024];
            let _ = stream.read(&mut buf);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
        });
        format!("http://{addr}")
    }

    #[test]
    fn load_parses_settings_from_server_response() {
        let defaults = AppSettings::default();
        let body = serde_json::to_string(&defaults).unwrap();
        let base_url = fake_server_returning(Box::leak(body.into_boxed_str()));
        let client = SettingsClient::with_base_url(base_url);
        let loaded = client.load().unwrap();
        assert_eq!(loaded, defaults);
    }
}
```

- [ ] **Step 2: Run to confirm it fails**

Run: `cargo test -p ledgerr-host --bin host-tauri load_parses_settings_from_server_response`
Expected: FAIL to compile — module not wired into `main.rs` yet, and `AppSettings` needs `PartialEq` derived (confirm via `grep -n "derive" crates/ledgrrr-settings/src/schema.rs` after Task 4 — the source `schema.rs` already derives `PartialEq, Eq` on `AppSettings` per the version read during this plan's research, so no change needed there; if Task 4's copy dropped it, add it back).

- [ ] **Step 3: Wire the module in and update `AppState`**

Add `mod settings_client;` to `crates/ledgerr-host/src/bin/tauri/main.rs` (near its other `mod` declarations).

`crates/ledgerr-host/src/bin/tauri/state.rs` — replace the `store` field's type:
```rust
use std::sync::{Arc, Mutex};

use ledgerr_host::chat::{ChatTurn, ReviewLog};
use ledgerr_host::evidence::EvidenceState;
use ledgerr_host::internal_openai::InternalOpenAiHandle;

use crate::settings_client::SettingsClient;

pub struct AppState {
    pub store: Arc<SettingsClient>,
    pub history: Arc<Mutex<Vec<ChatTurn>>>,
    pub review_log: Arc<Mutex<ReviewLog>>,
    pub internal_endpoint: Arc<Mutex<Option<InternalOpenAiHandle>>>,
    pub evidence: Arc<Mutex<EvidenceState>>,
}
```

`crates/ledgerr-host/src/bin/tauri/main.rs:58` — the construction site:
```rust
let store = Arc::new(crate::settings_client::SettingsClient::new());
```
(Removes the `default_settings_path()` import/call if nothing else in `main.rs` uses it — check with `grep -n "default_settings_path" crates/ledgerr-host/src/bin/tauri/main.rs` after this edit.)

`crates/ledgerr-host/src/bin/tauri/commands.rs` — every `state.store.load()` / `state.store.save(&settings)` / `state.store.path().display()` call site changes shape slightly: `load()`/`save()` keep the same names and now return `Result<_, SettingsClientError>` instead of `Result<_, SettingsError>` (the `.map_err(|e| e.to_string())` pattern already used at every call site absorbs this transparently — no call-site signature change needed there). `state.store.path().display()` has no equivalent on `SettingsClient` (there's no longer a local file path to display, since state lives in `ledgrrr-service` now) — at `commands.rs:151` and `:192`, replace:
```rust
let status_text = format!("Editing {}", state.store.path().display());
```
with:
```rust
let status_text = "Editing settings via ledgrrr-service".to_string();
```
and similarly at line 192's `state.store.path().display()` usage.

- [ ] **Step 4: Build and run the test**

Run: `cargo test -p ledgerr-host --bin host-tauri load_parses_settings_from_server_response`
Expected: PASS.

Run: `cargo check -p ledgerr-host --all-features`
Expected: clean — this will surface any remaining `state.store.path()` call site this plan's research didn't catch; fix forward.

- [ ] **Step 5: Manual end-to-end smoke test**

```bash
LEDGRRR_STATE_DIR=$(mktemp -d) ./target/debug/ledgrrr-service &
sleep 1
cargo run -p ledgerr-host --bin host-tauri &
# Confirm in the Tauri window that settings load without error (no "Editing ..." panic,
# no connection-refused toast). Then:
kill %1 %2
```

- [ ] **Step 6: Commit**

```bash
git add crates/ledgerr-host/src/bin/tauri/settings_client.rs crates/ledgerr-host/src/bin/tauri/state.rs \
        crates/ledgerr-host/src/bin/tauri/main.rs crates/ledgerr-host/src/bin/tauri/commands.rs
git commit -m "feat(host-tauri): settings now served by ledgrrr-service over HTTP, not a local SettingsStore"
```

---

### Task 9: Switch `host-tray` to the same HTTP client

**Files:**
- Modify: `crates/ledgerr-host/src/bin/host-tray.rs`
- Modify: `crates/ledgerr-host/src/tray/runtime.rs`

**Interfaces:**
- Consumes: same `SettingsClient` shape as Task 8 — but `host-tray.rs` is a separate binary target from `host-tauri`'s `bin/tauri/*.rs` module tree, so it needs its own copy of `settings_client.rs` (binary targets in this crate don't share modules with each other, only with the library crate root `ledgerr_host::*`). Given that duplication concern, this task instead promotes `settings_client.rs` from a `host-tauri`-local module to a `ledgerr-host` library module, shared by both binaries.

Confirmed by reading `crates/ledgerr-host/src/tray/runtime.rs` in full (342 lines) during this plan's writing: `SettingsStore` is threaded through as a type annotation in exactly two places — `pub fn run(store: SettingsStore) -> Result<(), Box<dyn std::error::Error>>` (line 21) and its helper `fn handle_command(command: TrayCommand, store: &SettingsStore, state: &Arc<Mutex<TrayState>>, control_tx: &mpsc::Sender<TrayControl>) -> Result<bool, Box<dyn std::error::Error>>` (line 120-125), called as `handle_command(command, &store, &state, &tray.control_tx)?` (line 109). Every other call site in the file (~9 `store.load()?` / `store.save(&settings)?` pairs, one per `TrayCommand` variant, e.g. lines 128-130, 136-138, 144-154, 160-202, 215) calls `.load()`/`.save()` by method name only. Since `SettingsClient::load(&self) -> Result<AppSettings, SettingsClientError>` and `SettingsClient::save(&self, settings: &AppSettings) -> Result<(), SettingsClientError>` (Task 8) share identical method names and argument shapes with `SettingsStore::load`/`save`, and `SettingsClientError` derives `thiserror::Error` (so `?` still coerces into `Box<dyn std::error::Error>` at every one of those call sites) — **no trait, no adapter, and no changes to any of the ~9 command-handler bodies are needed.** This is a pure type-annotation swap in exactly two places.

- [ ] **Step 1: Promote `settings_client` to a library module**

Move `crates/ledgerr-host/src/bin/tauri/settings_client.rs` to `crates/ledgerr-host/src/settings_client.rs` (no content changes to the struct/impl — only its module path moves). Add `pub mod settings_client;` to `crates/ledgerr-host/src/lib.rs`. In `crates/ledgerr-host/src/bin/tauri/main.rs`, remove `mod settings_client;` and change every `crate::settings_client::SettingsClient` reference (in `state.rs`) to `ledgerr_host::settings_client::SettingsClient`.

`reqwest = { workspace = true }` is already present in `crates/ledgerr-host/Cargo.toml`'s `[dependencies]` (confirmed during this plan's research) — no new dependency needed for the promoted module.

- [ ] **Step 2: Run Task 8's test again from its new location**

Run: `cargo test -p ledgerr-host load_parses_settings_from_server_response`
Expected: PASS (same test, now compiled as part of the library crate instead of the `host-tauri` binary — confirms the promotion didn't change behavior).

- [ ] **Step 3: Swap the type in `tray/runtime.rs`**

Line 10, change:
```rust
use crate::settings::{AppSettings, SettingsStore};
```
to:
```rust
use crate::settings::AppSettings;
use crate::settings_client::SettingsClient;
```

Line 21, change:
```rust
pub fn run(store: SettingsStore) -> Result<(), Box<dyn std::error::Error>> {
```
to:
```rust
pub fn run(store: SettingsClient) -> Result<(), Box<dyn std::error::Error>> {
```

Line 122, inside `fn handle_command(...)`, change:
```rust
    store: &SettingsStore,
```
to:
```rust
    store: &SettingsClient,
```

No other line in this file changes — every `store.load()?` / `store.save(&settings)?` call site keeps compiling unchanged because the method names and `?`-compatible error types match.

- [ ] **Step 4: Update `host-tray.rs`**

Current content (confirmed by reading the file during this plan's writing):
```rust
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let store =
        ledgerr_host::settings::SettingsStore::new(ledgerr_host::settings::default_settings_path());
    ledgerr_host::tray::runtime::run(store)
}

#[cfg(not(windows))]
fn main() {
    eprintln!("host-tray is currently supported on Windows builds only");
    std::process::exit(1);
}
```

Replace the `#[cfg(windows)]` block's body:
```rust
#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = ledgerr_host::settings_client::SettingsClient::new();
    ledgerr_host::tray::runtime::run(client)
}
```
The `#[cfg(not(windows))]` block is unchanged. The whole binary only does real work behind `#[cfg(windows)]` — `cargo build -p ledgerr-host --bin host-tray` on non-Windows still compiles both arms (proves the code is syntactically/type-correct) but only the Windows arm exercises `tray::runtime::run`.

- [ ] **Step 5: Build**

Run: `cargo build -p ledgerr-host --bin host-tray`
Expected: compiles clean on any platform (per Step 4's note, only the `#[cfg(windows)]` arm touches `runtime::run`, but both arms must still type-check).

- [ ] **Step 6: Commit**

```bash
git add crates/ledgerr-host/src/settings_client.rs crates/ledgerr-host/src/lib.rs \
        crates/ledgerr-host/src/bin/tauri/state.rs crates/ledgerr-host/src/bin/tauri/main.rs \
        crates/ledgerr-host/src/bin/host-tray.rs crates/ledgerr-host/src/tray/runtime.rs
git commit -m "feat(host-tray): settings now served by ledgrrr-service over HTTP, shared SettingsClient with host-tauri"
```

---

### Task 10: Full workspace verification

**Files:** none (verification-only task).

- [ ] **Step 1: Full workspace build**

Run: `cargo check --workspace --all-targets --all-features`
Expected: clean. (Known pre-existing unrelated failure: none, assuming gh#162 has merged by the time this plan executes — if it hasn't, `ledger-core` won't compile at all and this whole plan is blocked on that landing first, per this plan's own research findings from the gh#162 work earlier this session.)

- [ ] **Step 2: Full workspace test**

Run: `cargo test --workspace --all-features`
Expected: same pass/fail set as `main` had before this plan started, plus the new tests from Tasks 5, 6, and 8 passing. No regressions.

- [ ] **Step 3: Clippy**

Run: `cargo clippy --workspace --all-targets --all-features`
Expected: clean (workspace lints deny `unsafe_code`; nothing in this plan introduces any).

- [ ] **Step 4: Manual three-process smoke test**

```bash
LEDGRRR_STATE_DIR=$(mktemp -d) ./target/debug/ledgrrr-service &
sleep 1
curl -s http://127.0.0.1:15116/settings | python3 -m json.tool  # confirm valid JSON
cargo run -p ledgerr-host --bin host-tauri &
sleep 2
# In the Tauri window: change a setting, save it, close and reopen the settings panel,
# confirm the change persisted (proves the round-trip through ledgrrr-service, not
# just that the UI didn't crash).
kill %1 %2
```

- [ ] **Step 5: Update `docs/superpowers/plans/2026-07-25-ledgrrr-integration-roadmap.md`'s subsystem 4 row**

This file exists in git history (commits `c829368`, `2e8d3ad`) but not in the current working tree. Restore it and mark subsystem 4 as Phase A complete:
```bash
git show 2e8d3ad:docs/superpowers/plans/2026-07-25-ledgrrr-integration-roadmap.md > docs/superpowers/plans/2026-07-25-ledgrrr-integration-roadmap.md
```
Then edit the subsystem 4 row's "Plan file" column from `backlogged — issue #118` to reference this plan's filename, and update its status. Commit alongside.

- [ ] **Step 6: Final commit**

```bash
git add docs/superpowers/plans/2026-07-25-ledgrrr-integration-roadmap.md
git commit -m "docs: mark gh#118 Phase A (settings unification) complete in the integration roadmap"
```
