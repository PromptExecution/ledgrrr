# Desktop Agent and Office Playbook Surface

Ledgrrr's desktop direction is a local-first Windows control plane for process modelling, visualization, simulation, and approval. The desktop stack is not only a bookkeeping UI. It is the durable host for a local service, tray/taskbar controls, Claude Desktop MCPB integration, Microsoft 365 diagram surfaces, and b00t-controlled orchestration playbooks.

See [PRD-10](../../PRD-10.md) for the full requirements and definition of done.

## Present State

| Area | Current state |
|---|---|
| MCP server | `ledgerr-mcp-server` exposes ledgrrr capability families over stdio. |
| Claude plugin | The repo has a Claude plugin marketplace entry for Cowork/plugin workflows. |
| Claude Desktop bundle | Missing. There is no installable MCPB package yet. |
| Windows desktop host | Tauri/host crates exist for tray/window/local operator work. |
| Windows service | Not yet a productized install target. |
| Diagram rendering | Mermaid and isometric documentation rendering are implemented for the Rhai diagram DSL. |
| Office/SharePoint | Not yet packaged as OneNote/Office add-in or SPFx web part. |
| b00t linkage | Conceptually strong, but install/render/simulate/status contracts still need stable schemas. |

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

b00t must be able to treat ledgrrr as a typed local capability:

```text
b00t learn ledgrrr
b00t ledgrrr status --json
b00t ledgrrr install --dry-run
b00t ledgrrr repair --json
b00t ledgrrr render --input pipeline.json --format mermaid
b00t ledgrrr simulate --input pipeline.json --profile local-cpu
b00t ledgrrr export-office --input playbook.json --target onenote
```

Required datums:

- `ledgrrr.cli`
- `ledgrrr.mcp`
- `ledgrrr.desktop`
- `ledgrrr.service`
- `ledgrrr.office-addin`
- `ledgrrr.sharepoint-webpart`
- `ledgrrr.model-runtime`

Each b00t-triggered action must return structured evidence with deterministic ids.

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
