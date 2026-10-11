import type { FoundryLocalState, FoundryInstallPlan } from './types.js';

export interface FoundryLocalHandlers {
  onRefreshStatus: () => void;
  onRequestPlan: () => void;
  onConfirmInstall: () => void;
  onDismissInstall: () => void;
}

function escapeHtml(str: string): string {
  return str
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function renderStatusLine(state: FoundryLocalState): string {
  const { status } = state;
  if (status.kind === 'loading') {
    return `<div class="fl-status fl-status--loading">Checking Foundry Local…</div>`;
  }
  if (status.kind === 'error') {
    return `<div class="fl-status fl-status--error">Status unavailable: ${escapeHtml(status.message)}</div>`;
  }
  const { cli_found, service_running } = status.status;
  const cliDot = cli_found ? 'fl-dot--ok' : 'fl-dot--off';
  const svcDot = service_running ? 'fl-dot--ok' : 'fl-dot--off';
  return `
    <div class="fl-status">
      <span class="fl-dot ${cliDot}"></span>
      <span>CLI ${cli_found ? 'found' : 'not found'}</span>
      <span class="fl-status__sep"></span>
      <span class="fl-dot ${svcDot}"></span>
      <span>Service ${service_running ? 'running' : 'stopped'}</span>
    </div>
  `.trim();
}

function renderPlan(plan: FoundryInstallPlan): string {
  const blocked = plan.blocked_reason
    ? `<div class="fl-plan__blocked">${escapeHtml(plan.blocked_reason)}</div>`
    : '';
  return `
    <div class="fl-plan">
      <div class="fl-plan__action">${escapeHtml(plan.action)}</div>
      ${blocked}
      <code class="fl-plan__command">${escapeHtml(plan.unattended_command)}</code>
      <div class="fl-plan__buttons">
        <button class="fl-btn fl-btn--primary" data-action="confirm" ${plan.executable_now ? '' : 'disabled'}>
          Install
        </button>
        <button class="fl-btn" data-action="cancel">Cancel</button>
      </div>
    </div>
  `.trim();
}

export function renderFoundryLocal(
  container: HTMLElement,
  state: FoundryLocalState,
  handlers: FoundryLocalHandlers,
): void {
  const { install } = state;

  let installSection = '';
  switch (install.kind) {
    case 'idle':
      installSection = `<button class="fl-btn fl-btn--primary" data-action="request-plan">Install-assist…</button>`;
      break;
    case 'plan-loading':
      installSection = `<div class="fl-status fl-status--loading">Building install plan…</div>`;
      break;
    case 'plan':
      installSection = renderPlan(install.plan);
      break;
    case 'plan-error':
      installSection = `
        <div class="fl-status fl-status--error">Plan failed: ${escapeHtml(install.message)}</div>
        <button class="fl-btn" data-action="request-plan">Retry</button>
      `.trim();
      break;
    case 'installing':
      installSection = `<div class="fl-status fl-status--loading">Launching winget install…</div>`;
      break;
    case 'result':
      installSection = `
        <div class="fl-status ${install.result.ok ? 'fl-status--ok' : 'fl-status--error'}">
          ${escapeHtml(install.result.message)}
        </div>
        <button class="fl-btn" data-action="dismiss">Close</button>
      `.trim();
      break;
    case 'install-error':
      installSection = `
        <div class="fl-status fl-status--error">Install failed: ${escapeHtml(install.message)}</div>
        <button class="fl-btn" data-action="dismiss">Close</button>
      `.trim();
      break;
  }

  container.innerHTML = `
    <div class="fl-panel">
      <div class="fl-panel__header">
        <span class="fl-panel__title">Foundry Local</span>
        <button class="fl-btn fl-btn--icon" data-action="refresh-status" title="Refresh status">⟳</button>
      </div>
      ${renderStatusLine(state)}
      <div class="fl-install">${installSection}</div>
    </div>
  `.trim();

  container.querySelector('[data-action="refresh-status"]')
    ?.addEventListener('click', () => handlers.onRefreshStatus());
  container.querySelector('[data-action="request-plan"]')
    ?.addEventListener('click', () => handlers.onRequestPlan());
  container.querySelector('[data-action="confirm"]')
    ?.addEventListener('click', () => handlers.onConfirmInstall());
  container.querySelector('[data-action="cancel"]')
    ?.addEventListener('click', () => handlers.onDismissInstall());
  container.querySelector('[data-action="dismiss"]')
    ?.addEventListener('click', () => handlers.onDismissInstall());
}
