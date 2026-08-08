# Desktop Agent and Office Playbook Surface

Ledgrrr's desktop direction is a local-first Windows control plane for process modelling, visualization, simulation, and approval. The desktop stack is not only a bookkeeping UI. It is the durable host for a local service, tray/taskbar controls, Claude Desktop MCPB integration, Microsoft 365 diagram surfaces, and b00t-controlled orchestration playbooks.

See [PRD-11](../../PRD-11.md) for the full requirements and definition of done.

## Present State

| Area | Current state |
|---|---|
| MCP server | `ledgerr-mcp-server` exposes ledgrrr capability families over stdio. |
| Claude plugin | The repo has a Claude plugin marketplace entry for Cowork/plugin workflows. |
| `ledgrrr-mcp` controller | Implemented (`crates/ledgerr-desktop-agent`). Real stdio JSON-RPC server exposing all 11 `ledgrrr_*` tools; `ledgrrr_status` measures actual local state (b00t CLI, service heartbeat/pid liveness, tray binary presence), no mocked fields. |
| Claude Desktop MCPB bundle | Implemented (Phase 1). `scripts/package-desktop-agent.sh` / `just package-desktop-mcpb` builds `dist/ledgrrr-claude.mcpb/` (manifest + binary); not yet wired into a released Claude Desktop install flow. |
| `ledgrrr-service` | Implemented as a Phase 1 user-level heartbeat process (`crates/ledgerr-desktop-agent/src/bin/ledgrrr-service.rs`), spawned/killed by `ledgrrr_start_service`/`ledgrrr_stop_service`. Not an OS service registration — that is native-installer scope. |
| Windows desktop host | Tauri/host crates exist for tray/window/local operator work; `ledgrrr_open_tray` can launch the existing `host-tray`/`ledgerr-tauri` binary if built. |
| Windows service (OS-registered) | Not yet a productized install target — requires the native installer. |
| Diagram rendering | Mermaid, JSON, and a minimal deterministic SVG layout are implemented and golden-tested in `ledgerr-desktop-agent` for the `ledgrrr_render_diagram` tool. PNG remains unsupported pending a rasterizer dependency. Mermaid/isometric documentation rendering for the Rhai diagram DSL is separately implemented in the mdBook tooling. |
| Deterministic simulation | Implemented: `ledgrrr_simulate_pipeline` walks a playbook's nodes/edges/gates with no LLM and no wall-clock dependency, producing a reproducible run id, step trace, and gate decisions (golden-tested). |
| Office/SharePoint | Not packaged as OneNote/Office add-in or SPFx web part — requires a Microsoft 365/SharePoint tenant. `ledgrrr_export_office_artifact` writes a local, version-numbered bundle (Mermaid + SVG + playbook JSON + provenance) that a future Office/SPFx bridge can consume. |
| b00t linkage | Datums published for all seven required capabilities (`ledgrrr.cli`, `ledgrrr.mcp`, `ledgrrr.desktop`, `ledgrrr.service`, `ledgrrr.office-addin`, `ledgrrr.sharepoint-webpart`, `ledgrrr.model-runtime`) in `_b00t_/`. The desktop/office-addin/sharepoint-webpart/model-runtime datums are honestly marked `missing`/not configured — see [b00t Contract](#b00t-contract) below. |

## Target Architecture

```text
Claude Desktop
  -> ledgrrr-claude.mcpb
    -> ledgrrr-mcp.exe --stdio
      -> ledgrrr-service.exe
      -> ledgrrr-tray.exe
      -> local model runtime
      -> b00t capability index
      -> ledger-core / ledgerr-mcp / visualization crates

OneNote / Office Add-in
  -> local service or exported artifact bridge
    -> diagram renderer
    -> evidence graph
    -> workbook/playbook store

SharePoint SPFx Web Part
  -> stored diagram artifact
  -> optional signed refresh link / local handoff
```

## Installation Boundary

MCPB installs the Claude-facing controller only. It must not silently install a service, write machine-wide registry keys, or mutate host state during bundle install.

The native Windows installer owns the privileged stack:

- `ledgrrr-service.exe`
- `ledgrrr-tray.exe`
- `ledgrrr-mcp.exe`
- local model/runtime assets
- WebView2/Tauri prerequisites
- Start Menu entries
- repair/uninstall registration
- update metadata

Privileged service installation requires the normal Windows elevation path. Per-user mode must remain possible when the service is not installed.

## Required MCPB Tools

All 11 tools below are implemented in `ledgrrr-mcp` (`crates/ledgerr-desktop-agent`) and covered by contract tests (`tests/contract.rs`). `install_desktop`/`repair`/`uninstall` return a structured plan explaining that they require the native installer, which does not exist yet — they never fake a mutation.

| Tool | Purpose |
|---|---|
| `ledgrrr_status` | Report installed desktop, service, tray, model, Office, and b00t state. |
| `ledgrrr_install_plan` | Return a dry-run install/repair plan and privilege requirements. |
| `ledgrrr_install_desktop` | Launch the signed native installer. |
| `ledgrrr_start_service` | Start the service if installed. |
| `ledgrrr_stop_service` | Stop the service. |
| `ledgrrr_open_tray` | Launch or focus tray/taskbar UI. |
| `ledgrrr_render_diagram` | Render Mermaid/SVG/PNG/HTML from a typed playbook. |
| `ledgrrr_simulate_pipeline` | Run local deterministic or model-assisted simulation. |
| `ledgrrr_export_office_artifact` | Produce OneNote/SharePoint-safe diagram artifacts. |
| `ledgrrr_repair` | Repair service/tray/model/Office integration. |
| `ledgrrr_uninstall` | Launch the native uninstaller or return exact removal steps. |

## Office and SharePoint Surface

The diagram generator is the control surface for AI-generated process models.

| Surface | Role |
|---|---|
| OneNote/Office Add-in | Task pane for generating, previewing, inserting, and refreshing diagrams/playbooks. |
| SPFx web part | SharePoint rendering surface for published playbook artifacts. |
| Local service bridge | Converts playbook JSON into Mermaid, SVG, PNG, HTML, and provenance metadata. |

Office artifacts must be versioned. Refreshing a generated diagram creates a new artifact version and evidence node; it must not silently replace a previously published diagram.

## b00t Contract

Required datums are published in `_b00t_/` and discoverable today:

- `ledgrrr.cli` — pre-existing FOCUS/transport datum for the vendored checkout.
- `ledgrrr.mcp` — the `ledgrrr-mcp` controller surface (this document), `tool_prefix = "ledgrrr_"`.
- `ledgrrr.service` — the Phase 1 heartbeat process, `type = "runtime"`.
- `ledgrrr.desktop` — the native Windows installer target, `type = "vendor"`, honestly marked `status = "missing"`.
- `ledgrrr.office-addin` / `ledgrrr.sharepoint-webpart` — Office/SharePoint overlays, `type = "overlay"`, marked `status = "missing"` pending a Microsoft 365 tenant.
- `ledgrrr.model-runtime` — local CPU inference profile, `type = "ai"`, marked missing; `ledgrrr_status.model_runtime.configured` stays `false` until `LEDGRRR_MODEL_RUNTIME_PROFILE` is set to something real.

There is no dedicated `b00t ledgrrr <verb>` subcommand in `b00t-cli` — that would require changes to the separate `b00t-cli` crate, out of scope here. What works today is the generic guard-enforced execution path plus the datum registry:

```text
b00t learn ledgrrr-desktop-agent
b00t capabilities              # lists the published ledgrrr.* datums
b00t exec ledgrrr_status       # invoke the controller tool through the guard/audit path
```

Every `ledgrrr_*` tool call returns structured JSON (see `tests/contract.rs`); `ledgrrr_simulate_pipeline` additionally returns deterministic evidence ids per step.

## Future Definition of Done

The desktop/Office future state is done when:

- Claude Desktop installs `ledgrrr-claude.mcpb` and `ledgrrr_status` works.
- A signed/test-signed native Windows package installs service, tray, controller, model config, repair, and uninstall.
- The tray UI shows service, model, MCPB, Office, and b00t status.
- OneNote/Office can insert a generated diagram artifact.
- SharePoint can render the same playbook artifact via SPFx.
- Local CPU inference can generate or mutate playbooks without cloud access.
- Deterministic non-LLM simulation exists for CI and audit replay.
- CI validates MCPB, Windows build, Office/SPFx manifests, diagram golden outputs, and b00t JSON contracts.
- Release CI uploads MCPB, Windows installer, checksums, and provenance metadata.
