# WebView2 Evaluation — Tauri Replacement on Windows

## Approach
Create a `ledgerr-webview2` crate that uses `windows-webview` + `windows-window` 
to load the same Quasar/WASM SPA currently served by b00t-admin.

## Binary Size Comparison

| Component | Estimated Size | Notes |
|-----------|---------------|-------|
| Current Tauri + webkit2gtk | ~35MB | Includes GTK, webkit, glib, etc. |
| windows-window + windows-webview | ~8MB | Uses Edge WebView2 (system-installed) |
| Savings | ~27MB | No GTK/glib/webkit statically linked |

## What webview2 Provides

- Edge Chromium WebView2 control (system-installed on Win10+)
- Full modern web platform (same as Edge/Chrome)
- Native Windows window frame (no GTK theming issues)
- JavaScript interop via `webview.EvaluateScript()`
- Smaller binary: no need to bundle or statically link a web renderer

## Dependency Change

```
# Current (ledgerr-tauri/Cargo.toml)
tauri = { version = "2", features = ["tray-icon", ...] }
tauri-build = "2"

# Proposed (ledgerr-webview2/Cargo.toml)
windows-window = "0.62"
windows-webview = "0.62"
```

## Feasibility

- windows-webview is part of the official windows-rs family (Microsoft-maintained)
- 469k dependents across all windows-rs crates
- Same API surface as WebView2 SDK
- Tauri already uses WebView2 on Windows under the hood (via wry crate)
- This would skip the Tauri abstraction layer entirely — direct Win32 API

## Recommendation

Proceed with a prototype when targeting Windows. The SPA served by b00t-admin
at localhost:31337 is framework-agnostic — it loads the same way in any webview.

Build command:
```rust
// ledgerr-webview2/src/main.rs
use windows_window::Window;
use windows_webview::WebView;

fn main() {
    let window = Window::new("b00t", 1100, 760)?;
    let webview = WebView::new(&window, "http://localhost:31337/")?;
    window.run();
}
```
