// These interfaces mirror Rust structs that cross the Tauri IPC boundary as
// plain JSON-encoded strings (see `desktop_json` in
// crates/ledgerr-host/src/bin/tauri/commands.rs), so tauri-specta does not
// generate typed bindings for them in crates/ui/bindings.ts — the commands'
// return type is just `Result<String, String>`. Keep these in sync by hand
// with their Rust sources:
//   - FoundryLocalStatus   <- crates/ledgerr-desktop-agent/src/status.rs
//   - FoundryInstallPlan   <- crates/ledgerr-desktop-agent/src/foundry_install_plan.rs
//   - FoundryInstallResult <- crates/ledgerr-desktop-agent/src/foundry_install_plan.rs

/** Windows Foundry Local presence/liveness, as reported by `get_desktop_status`. */
export interface FoundryLocalStatus {
  cli_found: boolean;
  service_running: boolean;
}

/**
 * The slice of `LedgrrrStatus` (crates/ledgerr-desktop-agent/src/status.rs)
 * this module reads. `get_desktop_status` returns the full struct; every
 * other field is ignored here.
 */
export interface DesktopStatusSlice {
  foundry_local: FoundryLocalStatus;
}

/** Plan-before-mutation install plan from `get_foundry_local_install_plan`. */
export interface FoundryInstallPlan {
  action: string;
  executable_now: boolean;
  blocked_reason: string | null;
  unattended_command: string;
}

/** Result of `foundry_local_install_action`. */
export interface FoundryInstallResult {
  ok: boolean;
  launched: boolean;
  message: string;
  plan: FoundryInstallPlan;
}

/** Status-panel half of the module: reflects `get_desktop_status`. */
export type StatusPhase =
  | { kind: 'loading' }
  | { kind: 'loaded'; status: FoundryLocalStatus }
  | { kind: 'error'; message: string };

/**
 * Install-assist half of the module: plan-before-mutation flow driven by
 * `get_foundry_local_install_plan` and `foundry_local_install_action`.
 */
export type InstallPhase =
  | { kind: 'idle' }
  | { kind: 'plan-loading' }
  | { kind: 'plan'; plan: FoundryInstallPlan }
  | { kind: 'plan-error'; message: string }
  | { kind: 'installing' }
  | { kind: 'result'; result: FoundryInstallResult }
  | { kind: 'install-error'; message: string };

export interface FoundryLocalState {
  status: StatusPhase;
  install: InstallPhase;
}

export type FoundryLocalEvent = { type: 'state'; state: FoundryLocalState };
