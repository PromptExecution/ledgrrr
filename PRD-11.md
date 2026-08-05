# PRD-11: Desktop Agent, MCPB Bootstrapper, Office Diagram Playbook, and Local Simulation Runtime

**Status:** Future-state requirements | **Priority:** P0 platform packaging / P1 Office surface | **Date:** 2026-07-20

---

## 1. Situation

`l3dg3rr` is evolving from a local-first bookkeeping and evidence graph into a Windows-first control plane for modelling, visualizing, simulating, and approving orchestration pipelines. The system must remain local-first: sensitive documents, workbook state, model prompts, and simulation traces should be able to run without leaving the user's machine.

The current project already has several relevant pieces:

| Area | Present state |
|---|---|
| Core bookkeeping graph | Rust crates model documents, transactions, journal projections, evidence, workflow, validation, legal checks, and workbook export. |
| MCP surface | `ledgerr-mcp-server` exposes typed `ledgerr_*` capability families over stdio. |
| Visualization | mdBook and the live Rhai editor render Mermaid and isometric pipeline diagrams from a narrow DSL. |
| Desktop host | Tauri/host crates exist for Windows tray/window control and local operator state. |
| b00t integration | `_b00t_` consumes ledgrrr as a submodule and exposes b00t/agent/MCP context around it. |
| Claude plugin marketplace | A Claude plugin marketplace entry exists for Cowork/plugin workflows, but it is not a Claude Desktop MCPB binary bundle. |
| Release packaging | CI builds release binaries and a Windows tray artifact in adjacent workflows, but there is not yet one coherent Windows install/update story. |

The missing product shape is a unified installation and runtime architecture:

1. Claude Desktop should install a small MCPB bundle that starts a local ledgrrr controller MCP server.
2. That controller should be able to inspect, install, repair, update, and control the native Windows ledgrrr stack with explicit user approval.
3. The native stack should install the tray/taskbar UI, background service, model/runtime assets, and Office/SharePoint integration helpers.
4. OneNote, Office, and SharePoint should display and persist generated diagrams/playbooks from ledgrrr, not just static screenshots.
5. The full path should support local CPU execution of a fine-tuned model for generative process modelling, visualization, simulation, and orchestration control.

---

## 2. Product Thesis

Ledgrrr is the local control mechanism for financial and operational pipelines. b00t supplies the typed agent and orchestration vocabulary; ledgrrr supplies the durable desktop runtime, evidence graph, visual playbooks, and Office-facing user surface.

The target user should be able to install ledgrrr once, open Claude Desktop or Microsoft 365, and ask for a model of a pipeline. The system should:

1. discover the relevant b00t/ledgrrr capabilities,
2. generate a typed pipeline/playbook model,
3. render it as Mermaid/isometric/Office-friendly artifacts,
4. simulate the state transitions locally,
5. expose approval and rollback controls,
6. write the resulting evidence into ledgrrr and, where requested, OneNote or SharePoint.

---

## 3. Chosen Distribution Architecture

### 3.1 Claude Desktop MCPB

The Claude Desktop artifact is a MCPB bundle. It installs only the Claude-facing controller:

```text
ledgrrr-claude.mcpb
├── manifest.json
├── server/
│   ├── ledgrrr-mcp.exe
│   └── ledgrrr-mcp-support.json
└── assets/
    └── icon.png
```

The MCPB server is `ledgrrr-mcp.exe`, a small binary MCP server using stdio. It must not silently install services, write HKLM registry keys, or mutate system state during bundle installation. It exposes explicit tools that the user or agent can call.

Required MCPB tools:

| Tool | Purpose |
|---|---|
| `ledgrrr_status` | Report installed desktop version, service status, tray status, model runtime status, Office add-in status, and b00t linkage. |
| `ledgrrr_install_plan` | Return a dry-run install/repair plan, including privilege requirements and affected paths. |
| `ledgrrr_install_desktop` | Launch the signed native installer with visible UI or documented silent flags. |
| `ledgrrr_install_service` | Request service installation through the native installer/elevated helper. |
| `ledgrrr_start_service` | Start the ledgrrr background service if installed. |
| `ledgrrr_stop_service` | Stop the background service. |
| `ledgrrr_open_tray` | Launch or focus the tray/taskbar application. |
| `ledgrrr_open_playbook` | Open the local playbook UI for a pipeline/run id. |
| `ledgrrr_render_diagram` | Render a typed pipeline to Mermaid/SVG/PNG/HTML payloads. |
| `ledgrrr_simulate_pipeline` | Run local simulation over a pipeline model and return state/evidence summary. |
| `ledgrrr_export_office_artifact` | Produce an Office-safe artifact for OneNote/SharePoint insertion. |
| `ledgrrr_repair` | Re-run repair checks for service, tray, model runtime, and Office manifests. |
| `ledgrrr_uninstall` | Launch the native uninstaller or return exact removal instructions. |

### 3.2 Native Windows Installer

The native Windows installer is the authoritative installer for the desktop stack. It should use MSIX with external location or an equivalent Microsoft Store-compatible package identity strategy, while preserving unrestricted Win32 behavior where required.

The installer owns:

- `ledgrrr-service.exe` background service.
- `ledgrrr-tray.exe` tray/taskbar widget.
- `ledgrrr-mcp.exe` local controller binary.
- Local model/runtime assets and configuration directories.
- WebView2 / Tauri prerequisites where required.
- Start Menu entries, repair/uninstall registration, logging locations, and update channel metadata.
- Optional Office add-in and SharePoint helper manifests/documentation.

The installer must support:

- interactive install,
- unattended enterprise install,
- repair,
- uninstall,
- versioned upgrades,
- code signing,
- explicit UAC/elevation for service/HKLM operations,
- non-admin per-user mode where service installation is skipped.

### 3.3 Microsoft 365 Surfaces

Office and SharePoint integration are separate from the Windows installer. They are deployed with Microsoft 365 mechanisms:

| Surface | Deployment path | Responsibility |
|---|---|---|
| OneNote/Office Add-in | Office add-in manifest / centralized deployment | Task pane for generating, previewing, inserting, and refreshing diagrams/playbooks. |
| SharePoint diagram display | SPFx web part | Render ledgrrr playbook artifacts in SharePoint pages/libraries. |
| Office artifact bridge | ledgrrr local service + Graph/user export | Convert ledgrrr models into Office-safe SVG/PNG/HTML/JSON artifacts. |

The Office surfaces should not depend on Claude Desktop. Claude Desktop is one client of the ledgrrr local controller; Microsoft 365 is another.

---

## 4. Runtime Component Model

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

The local service is the durable runtime. It owns state, audit, simulation jobs, model lifecycle, and artifact persistence. The tray app owns user-visible status and approvals. The MCPB server is a controller and bridge, not the service itself.

---

## 5. b00t Contract

Ledgrrr must integrate with b00t as a typed local capability, not as an opaque desktop app.

Required b00t-facing artifacts:

| Artifact | Requirement |
|---|---|
| Datum(s) | Publish datums for `ledgrrr.cli`, `ledgrrr.mcp`, `ledgrrr.desktop`, `ledgrrr.service`, `ledgrrr.office-addin`, `ledgrrr.sharepoint-webpart`, and `ledgrrr.model-runtime`. |
| Capability map | Map `ledgerr_*` MCP tools to b00t capability families and installation state. |
| Installer recipe | b00t must be able to ask for install status, dry-run install, repair, and uninstall without guessing commands. |
| Simulation model | b00t pipeline/orchestration datums must be serializable into ledgrrr playbook models. |
| Evidence | Every install, repair, simulation, diagram export, and approval must emit structured evidence with deterministic ids. |
| Local model | b00t must be able to select local CPU fine-tuned inference for diagram/playbook generation when the model runtime is installed. |

Minimum b00t command contracts:

```text
b00t learn ledgrrr
b00t ledgrrr status --json
b00t ledgrrr install --dry-run
b00t ledgrrr repair --json
b00t ledgrrr render --input pipeline.json --format mermaid
b00t ledgrrr simulate --input pipeline.json --profile local-cpu
b00t ledgrrr export-office --input playbook.json --target onenote
```

These commands may be implemented by b00t delegating to `ledgrrr-mcp.exe` or `ledgrrr-service.exe`, but the user-facing contract should stay stable.

---

## 6. Diagram and Playbook Requirements

The diagram generator is not a decorative feature. It is the control surface for process modelling and orchestration.

### 6.1 Supported diagram formats

Initial:

- Mermaid flowchart/state diagrams.
- SVG export.
- PNG export for Office clients that cannot preserve live SVG behavior.
- JSON playbook model containing typed nodes, edges, gates, evidence refs, and simulation state.

Future:

- PlantUML import/export where useful.
- UFO/ISO stereotype overlay.
- Isometric HTML export for interactive SharePoint pages.
- Signed artifact bundles containing diagram, playbook JSON, provenance, and render metadata.

### 6.2 Playbook model

A playbook must contain:

- `playbook_id`
- `title`
- `version`
- `source`
- typed nodes
- typed edges
- b00t capability references
- ledgrrr evidence references
- simulation profile
- approval gates
- rollback/repair actions
- Office artifact pointers

### 6.3 Simulation

Simulation must support:

- deterministic run id,
- local CPU inference profile,
- pure deterministic mode with no LLM,
- step-by-step state transition trace,
- resource/cost/time estimates,
- evidence output,
- comparison of planned vs actual execution,
- export into the workbook/audit model where relevant.

---

## 7. Security and Governance

This project has privileged local execution risk. The following constraints are mandatory:

- MCPB install must not silently perform privileged mutation.
- Service installation must require native installer/elevation.
- Claude-facing tools that mutate host state must return a plan before execution.
- The tray app must surface pending privileged actions and their reason.
- Credentials remain outside model prompts and are stored through OS/user-approved secure storage.
- Office/SharePoint artifacts must distinguish generated content, source evidence, and user-approved publication.
- Every tool call that installs, repairs, starts/stops services, exports artifacts, or runs simulation must be audit logged.
- Local model use must be visible in status output, including model id, quantization/profile, and whether cloud fallback is enabled.

---

## 8. CI and Release Requirements

CI must produce and validate these artifacts:

| Artifact | CI requirement |
|---|---|
| `ledgrrr-claude-<platform>.mcpb` | Pack and validate MCPB manifest; upload as PR artifact and release asset. |
| Native Windows installer | Build signed or test-signed installer; validate silent install flags in CI where possible. |
| `ledgrrr-service.exe` | Build and run smoke tests for status/start/stop protocol. |
| `ledgrrr-tray.exe` | Build and run non-interactive smoke test for startup/config detection. |
| Office add-in manifest | Validate manifest schema and task-pane URL/assets. |
| SPFx web part package | Build/package and validate static assets. |
| Diagram renderer | Golden tests for Mermaid/SVG/PNG/playbook JSON. |
| b00t contract | JSON status/dry-run/render/simulate commands must have stable schema tests. |

Release assets must include checksums and provenance metadata. The Windows installer and binaries must be prepared for code signing before public distribution.

---

## 9. Definition of Done

The future-state project is done when all of the following are true.

### Installation

- A user can install `ledgrrr-claude.mcpb` in Claude Desktop and call `ledgrrr_status`.
- A user can install the native Windows package from a release asset.
- The native installer supports install, repair, upgrade, uninstall, and documented unattended mode.
- The installer can install a Windows service with UAC and can install a per-user tray/taskbar app without admin rights.
- The tray app shows service status, model runtime status, Claude MCPB status, Office add-in status, and b00t status.

### Runtime

- `ledgrrr-service.exe` runs as the long-lived local runtime and exposes a local IPC/API consumed by the MCPB controller, tray app, and Office bridge.
- `ledgrrr-mcp.exe` runs as a stdio MCP server and exposes the required tool set listed in this PRD.
- Service, tray, and MCPB controller share one config schema and one audit log schema.
- All privileged operations require an explicit plan and user/elevation boundary.

### b00t

- b00t can discover ledgrrr installation state.
- b00t can invoke dry-run install, repair, render, simulate, and Office export contracts without ad hoc shell knowledge.
- Ledgrrr publishes typed datums for its desktop, service, MCP, Office, SharePoint, and model-runtime surfaces.
- b00t orchestration pipeline datums can be transformed into ledgrrr playbook models.
- Every b00t-triggered ledgrrr action returns structured evidence.

### Diagram and Office

- OneNote/Office add-in can insert a generated diagram artifact into a notebook/document.
- SharePoint SPFx web part can render the same playbook artifact.
- Mermaid/SVG/PNG exports are deterministic for a fixed playbook input.
- A playbook artifact includes provenance, source model/profile, evidence refs, render metadata, and refresh instructions.
- Diagram refresh never silently mutates a published artifact without creating a new version/evidence node.

### Simulation and Local AI

- A local CPU inference profile can generate or mutate playbook models without cloud access.
- A deterministic non-LLM simulation mode exists for CI and audit replay.
- Simulation output includes state trace, gate decisions, timing/cost estimates, and evidence ids.
- The user can compare planned, simulated, and actual execution paths.

### CI/Release

- PR CI validates MCPB, Windows build, manifest schemas, diagram golden outputs, and b00t JSON contracts.
- Release CI uploads MCPB, Windows installer, checksums, and provenance metadata.
- Docs include install, repair, uninstall, Office deployment, SPFx deployment, and b00t integration instructions.
- A clean Windows test machine can install, run `ledgrrr_status`, render a sample Mermaid diagram, and uninstall without manual cleanup.

---

## 10. Non-Goals

- MCPB is not the privileged installer.
- Claude Desktop is not the only client; Office, SharePoint, b00t, and the tray app must remain first-class clients.
- Cloud inference is not required for the target state.
- Office/SharePoint integration does not replace the local evidence graph; it publishes views over ledgrrr-owned artifacts.
