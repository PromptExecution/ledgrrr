# Desktop Agent and Office Playbook Surface

Ledgrrr's desktop direction is a local-first Windows control plane for process modelling, visualization, simulation, and approval. The desktop stack is not only a bookkeeping UI. It is the durable host for a local service, tray/taskbar controls, Claude Desktop MCPB integration, Microsoft 365 diagram surfaces, and b00t-controlled orchestration playbooks.

See [PRD-11](../../PRD-11.md) for the full requirements and definition of done.

## Present State

| Area | Current state |
|---|---|
| MCP server | `ledgerr-mcp-server` exposes ledgrrr capability families over stdio. |
| Claude plugin | The repo has a Claude plugin marketplace entry for Cowork/plugin workflows. |
| `ledgrrr-mcp` controller | Implemented (`crates/ledgerr-desktop-agent`). Real stdio JSON-RPC server exposing exactly 11 `ledgrrr_*` desktop-control tools; it is distinct from the 12 published `ledgerr_*` ledger capability families. |
| Claude Desktop MCPB bundle | Implemented. `scripts/package-desktop-agent.sh` / `just package-desktop-mcpb` builds `dist/ledgrrr-claude.mcpb/`. It includes only the unprivileged controller and visible package helper—not a signing key, service, or silent installer. |
| `ledgrrr-service` | Durable local runtime with an authenticated loopback health/shutdown endpoint, shared per-user config/audit schema, and an explicit per-user fallback. The package may later register it with SCM for machine scope. |
| Windows desktop host | Tauri host is the tray/taskbar executable in the external payload; `ledgrrr_open_tray` discovers the installed `ledgrrr-tray.exe` before compatibility names. |
| Native Windows dogfood package | Implemented: a test-signed sparse MSIX/external-location identity package, external Win32 payload, install/repair/uninstall helper, checksum, provenance, and Windows release-CI smoke. Public certificate procurement is deferred. |
| Diagram rendering | Mermaid, JSON, and a minimal deterministic SVG layout are implemented and golden-tested in `ledgerr-desktop-agent` for the `ledgrrr_render_diagram` tool. PNG remains unsupported pending a rasterizer dependency. Mermaid/isometric documentation rendering for the Rhai diagram DSL is separately implemented in the mdBook tooling. |
| Deterministic simulation | Implemented: `ledgrrr_simulate_pipeline` walks a playbook's nodes/edges/gates with no LLM and no wall-clock dependency, producing a reproducible run id, step trace, and gate decisions (golden-tested). |
| Office/SharePoint | Not packaged as OneNote/Office add-in or SPFx web part — requires a Microsoft 365/SharePoint tenant. `ledgrrr_export_office_artifact` writes a local, version-numbered bundle (Mermaid + SVG + playbook JSON + provenance) that a future Office/SPFx bridge can consume. |
| b00t linkage | Source-controlled package and Tauri datums live in `.b00t/datums/`; installed desktop/runtime state is reported by `ledgrrr_status`. Office, SharePoint, and model integrations remain status-visible but unconfigured. |

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

The authoritative Windows delivery is a sparse MSIX/external-location identity package. Its package identity enables Windows integration while its external payload owns the Win32 stack:

- `ledgrrr-service.exe`
- `ledgrrr-tray.exe`
- `ledgrrr-mcp.exe`
- `support-manifest.json` (binary names, per-user state contract, prerequisites)
- local model/runtime assets
- WebView2/Tauri prerequisites
- Start Menu entries
- repair/uninstall registration
- update metadata

Per-user install is the dogfood default and requires no UAC: it installs the test certificate into Current User Trusted People, copies the external payload under `%LOCALAPPDATA%\Programs\ledgrrr`, and registers the MSIX identity with `Add-AppxPackage -ExternalLocation`. It writes `%LOCALAPPDATA%\ledgrrr\package-install.json` so an MCPB controller installed elsewhere can locate that payload, and caches the public MSIX/certificate under `%LOCALAPPDATA%\ledgrrr\package-cache` so repair can re-register the identity without a new download. Machine scope surfaces UAC and stages/provisions the package. Uninstall removes the external payload, install record, and package cache; runtime audit/config data remain under `%LOCALAPPDATA%\ledgrrr` unless the operator chooses to remove their personal data separately.

The repeatable Windows commands are:

```text
just windows-package <Build|TestInstall> <windows-repo-root> <version> [output-dir] [certificate-store-path]
just wsl2-pwsh-msix-build <windows-repo-root> <version>
just wsl2-pwsh-msix-smoke <windows-repo-root> <version>
```

Use the first recipe when composing an automation flow. `TestInstall` is the
single mutating dogfood command: it keeps package changes behind one visible
Windows/UAC boundary instead of asking an operator to approve each lifecycle
step individually. Controller MCP tools remain plan-first and require explicit
approval for a user-initiated install, repair, or uninstall.

The smoke path builds, test-signs, installs, discovers, and uninstalls the identity package. Per-user dogfood registration uses the explicit `Add-AppxPackage -AllowUnsigned` test mode so a self-signed root CA is never installed; this is not a public-signing substitute and is not used for machine-wide installation. Release CI uploads the `.msix`, public `.cer`, `external-payload.zip`, `INSTALL.json`, checksum, and provenance next to (not instead of) the domain MCPB artifacts. Extract `external-payload.zip` to a sibling `payload` directory before running the command in `INSTALL.json`.

### Windows Toolchain Prerequisites

The package command uses PowerShell 7 (`pwsh.exe`; 7.2+), including when invoked
from WSL, not only a manually opened Developer PowerShell. Install it with
`winget install --id Microsoft.PowerShell --source winget` if it is absent.
When `P:` is available, the test signing PFX and matching public certificate
persist under `P:\ledgrrr\test-signing`; release outputs contain only the public
`.cer`. Use `-CertificateStorePath` to select another private signer location.
Prefer a WSLC Windows container for repeatable Windows toolchain work when the
installed `b00t` exposes that capability; otherwise stage a local Windows copy.
It activates the Visual Studio environment through `VsDevCmd.bat` when needed.
Before a local build or smoke test, install:

- Windows 10 version 2004 / build 19041 or newer;
- Visual Studio 2022 Build Tools with **Desktop development with C++** (the
  `Microsoft.VisualStudio.Workload.VCTools` workload) and its recommended
  Windows SDK components, which provide `link.exe`, `makeappx.exe`, and
  `signtool.exe`;
- the Microsoft Edge WebView2 Evergreen Runtime (required when the tray is
  installed); and
- the built-in Windows Appx PowerShell module.

The package build detects a missing or stale mdBook toolchain and repairs the
`mdbook-admonish` asset/binary mismatch automatically. It then embeds the
generated playbook in the external payload, where the tray's `/docs/` route
discovers it beside `ledgrrr-tray.exe`. Offline builds should preinstall
`mdbook 0.5.x`, `mdbook-admonish 1.20.x`, and the repo-local
`mdbook-rhai-mermaid` binary. The legacy `mdbook-mermaid 0.16` preprocessor is
not compatible with mdBook 0.5 and is intentionally not used.

On a clean Windows machine, install the compiler workload with:

```powershell
winget install --id Microsoft.VisualStudio.2022.BuildTools --exact --source winget --accept-package-agreements --accept-source-agreements --silent --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

Then verify the C++ workload and run the acceptance path (substitute the
checkout's Windows path when launching from WSL):

```powershell
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
& $vswhere -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
.\scripts\windows-package.ps1 -Action TestInstall -Version 1.9.0
```

If a repaired/offline Build Tools instance has `VsDevCmd.bat` but is not yet
reported by `vswhere`, set `LEDGRRR_VSDEVCMD` to that file before invoking the
package script. The script also probes the normal Build Tools location and the
documented `C:\BuildTools` recovery location.

Native Windows Cargo does not support a `\\wsl.localhost\...` checkout as its
working directory. Keep a normal Windows clone under a drive such as `C:\src`
or `D:\src` for desktop packaging. If the source only exists in the WSL Linux
filesystem, stage a temporary local copy (excluding `.git`, `target`, `dist`,
`.claude`, and optional `models`) before running the command; copy artifacts
back only after the smoke succeeds. This is a Windows/Cargo constraint, not an
installer permission requirement.

`TestInstall` builds the package, generates a test certificate, creates the
MSIX/checksum/provenance/payload archive, installs per-user, calls the
controller's status/start/render/stop actions, repairs, and uninstalls. It is
safe for dogfood only: the certificate is public/test-only and must be replaced
by public signing before distribution outside the test channel.

## Required MCPB Tools

### Governed Process State Machines

`PlaybookModel` is also the deterministic process-state-machine contract. A
task state may declare its executing role, the b00t capability it invokes, and
the stable outcome it emits. Validation fails closed when a state invokes a
capability absent from `capability_refs`, or when its role lacks that capability
in `role_authorizations`. This is the executable form of the invariant that a
process cannot exceed its declared tools.

The checked-in `b00t learn ooda` fixture represents Observe → Orient → Decide
→ Act as states with outcome-labelled transitions. Render it through
`ledgrrr_render_diagram` with `format: "state-machine"`; simulate it through
`ledgrrr_simulate_pipeline` with the deterministic profile. The resulting trace
records each state, role, capability, evidence id, gate decision, and outcome
without a model call or wall-clock dependency.

All 11 tools below are implemented in `ledgrrr-mcp` (`crates/ledgerr-desktop-agent`) and covered by contract tests. Package mutations are plan-first: `ledgrrr_install_plan` exposes affected paths, scope, unattended command, and UAC boundary; `install_desktop`/`repair`/`uninstall` require `approved: true` and launch the actual visible PowerShell package workflow.

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

The source-controlled b00t package/Tauri datums are in `.b00t/datums/`; the
other integration names below are status vocabulary, not falsely advertised
installed packages:

- `ledgrrr.cli` — pre-existing FOCUS/transport datum for the vendored checkout.
- `ledgrrr.mcp` — the `ledgrrr-mcp` controller surface (this document), `tool_prefix = "ledgrrr_"`.
- `ledgrrr.service` — the authenticated per-user runtime boundary, `type = "runtime"`; a machine service registration remains an elevated package option.
- `ledgrrr.desktop` — the test-signed sparse-MSIX package contract (`.b00t/datums/ledgrrr.toml`), with installed state reported by `ledgrrr_status.desktop_package`.
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
