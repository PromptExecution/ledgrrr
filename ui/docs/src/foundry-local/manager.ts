// `generated/bindings.ts` is a synced copy of crates/ui/bindings.ts, made by
// `npm run sync-bindings` (runs automatically before dev/build — see
// package.json and scripts/sync-bindings.mjs for why this isn't imported
// directly from crates/ui/).
import { commands } from '../generated/bindings.js';
import { taskbarBus } from '../taskbar/bus.js';
import { foundryLocalBus } from './bus.js';
import { renderFoundryLocal } from './render.js';
import type {
  DesktopStatusSlice,
  FoundryInstallPlan,
  FoundryInstallResult,
  FoundryLocalState,
} from './types.js';

/**
 * Foundry Local status display + plan-before-mutation install-assist.
 *
 * Backed by three Tauri commands (crates/ledgerr-host/src/bin/tauri/commands.rs):
 *   - get_desktop_status              -> LedgrrrStatus (this module reads .foundry_local)
 *   - get_foundry_local_install_plan  -> FoundryInstallPlan
 *   - foundry_local_install_action    -> FoundryInstallResult (only after explicit user confirm)
 *
 * All three cross the IPC boundary as JSON-encoded strings (see `desktop_json`
 * in commands.rs), so this module parses them by hand — see ./types.ts.
 */
export class FoundryLocalManager {
  private state: FoundryLocalState = {
    status: { kind: 'loading' },
    install: { kind: 'idle' },
  };

  private containerEl: HTMLElement;

  constructor(containerEl: HTMLElement) {
    this.containerEl = containerEl;
    foundryLocalBus.subscribe(event => {
      if (event.type === 'state') {
        this.state = event.state;
        this.render();
      }
    });
    this.render();
  }

  getState(): FoundryLocalState {
    return this.state;
  }

  render(): void {
    renderFoundryLocal(this.containerEl, this.state, {
      onRefreshStatus: () => this.refreshStatus(),
      onRequestPlan: () => this.requestPlan(),
      onConfirmInstall: () => this.confirmInstall(),
      onDismissInstall: () => this.dismissInstall(),
    });
  }

  private publish(state: FoundryLocalState): void {
    foundryLocalBus.publish({ type: 'state', state });
  }

  async refreshStatus(): Promise<void> {
    this.publish({ ...this.state, status: { kind: 'loading' } });
    const result = await commands.getDesktopStatus();
    if (result.status === 'error') {
      this.publish({ ...this.state, status: { kind: 'error', message: result.error } });
      return;
    }
    try {
      const parsed = JSON.parse(result.data) as DesktopStatusSlice;
      this.publish({ ...this.state, status: { kind: 'loaded', status: parsed.foundry_local } });
    } catch (err) {
      this.publish({
        ...this.state,
        status: { kind: 'error', message: `failed to parse desktop status: ${describeError(err)}` },
      });
    }
  }

  async requestPlan(): Promise<void> {
    this.publish({ ...this.state, install: { kind: 'plan-loading' } });
    const result = await commands.getFoundryLocalInstallPlan();
    if (result.status === 'error') {
      this.publish({ ...this.state, install: { kind: 'plan-error', message: result.error } });
      return;
    }
    try {
      const plan = JSON.parse(result.data) as FoundryInstallPlan;
      this.publish({ ...this.state, install: { kind: 'plan', plan } });
    } catch (err) {
      this.publish({
        ...this.state,
        install: { kind: 'plan-error', message: `failed to parse install plan: ${describeError(err)}` },
      });
    }
  }

  /** Only called from the render layer's explicit "Install" confirmation click. */
  async confirmInstall(): Promise<void> {
    this.publish({ ...this.state, install: { kind: 'installing' } });
    const result = await commands.foundryLocalInstallAction(true);
    if (result.status === 'error') {
      this.publish({ ...this.state, install: { kind: 'install-error', message: result.error } });
      taskbarBus.publish({
        type: 'toast',
        toast: {
          id: crypto.randomUUID(),
          message: `Foundry Local install failed: ${result.error}`,
          level: 'error',
          ttl: 6000,
        },
      });
      return;
    }
    try {
      const parsed = JSON.parse(result.data) as FoundryInstallResult;
      this.publish({ ...this.state, install: { kind: 'result', result: parsed } });
      taskbarBus.publish({
        type: 'toast',
        toast: {
          id: crypto.randomUUID(),
          message: parsed.message,
          level: parsed.ok ? 'success' : 'error',
          ttl: 6000,
        },
      });
      // Re-probe status shortly after a launched install so cli_found/service_running
      // pick up the new state once winget finishes.
      if (parsed.launched) {
        setTimeout(() => {
          this.refreshStatus();
        }, 5000);
      }
    } catch (err) {
      this.publish({
        ...this.state,
        install: { kind: 'install-error', message: `failed to parse install result: ${describeError(err)}` },
      });
    }
  }

  dismissInstall(): void {
    this.publish({ ...this.state, install: { kind: 'idle' } });
  }
}

function describeError(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}
