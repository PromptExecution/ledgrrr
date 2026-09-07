import type {
  BudgetSummary,
  CloudProvider,
  DailySpend,
  ProviderSpend,
  ServiceSpend,
  TagAllocationRow,
} from './sampleData.js';

// ---- Shared helpers ---------------------------------------------------------

// Categorical colors, fixed order (blue / orange / green / violet) — chosen so
// adjacent pairs stay distinguishable under color-vision deficiency. Every
// provider is always paired with a visible text label, never color alone.
const PROVIDER_COLORS: Record<CloudProvider, string> = {
  GCP: '#3b82f6',
  AWS: '#ea580c',
  Alicloud: '#16a34a',
  HuggingFace: '#7c3aed',
};

const ACCENT = '#3b82f6';
const STATUS_GOOD = '#22c55e';
const STATUS_CRITICAL = '#ef4444';

const currencyFormatter = new Intl.NumberFormat('en-US', {
  style: 'currency',
  currency: 'USD',
  maximumFractionDigits: 0,
});

const currencyFormatterPrecise = new Intl.NumberFormat('en-US', {
  style: 'currency',
  currency: 'USD',
  maximumFractionDigits: 2,
});

function fmt(amountUsd: number): string {
  return currencyFormatter.format(amountUsd);
}

function fmtPrecise(amountUsd: number): string {
  return currencyFormatterPrecise.format(amountUsd);
}

function escapeHtml(str: string): string {
  return str
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function checkIcon(color: string): string {
  return `<svg class="bill-status-icon" width="12" height="12" viewBox="0 0 16 16" fill="none" aria-hidden="true">
    <circle cx="8" cy="8" r="7" stroke="${color}" stroke-width="1.5" />
    <path d="M4.5 8.3l2.2 2.2 4.8-4.8" stroke="${color}" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" />
  </svg>`;
}

function crossIcon(color: string): string {
  return `<svg class="bill-status-icon" width="12" height="12" viewBox="0 0 16 16" fill="none" aria-hidden="true">
    <circle cx="8" cy="8" r="7" stroke="${color}" stroke-width="1.5" />
    <path d="M5.5 5.5l5 5M10.5 5.5l-5 5" stroke="${color}" stroke-width="1.5" stroke-linecap="round" />
  </svg>`;
}

// ---- 1. Budget summary meter -------------------------------------------------

export function renderSummary(container: HTMLElement, budget: BudgetSummary): void {
  const pct = budget.capUsd > 0 ? budget.spentUsd / budget.capUsd : 0;
  const overBudget = budget.spentUsd > budget.capUsd;
  const fillPct = Math.min(pct, 1) * 100;
  const fillColor = overBudget ? STATUS_CRITICAL : ACCENT;

  const overBudgetBadge = overBudget
    ? `<span class="bill-summary__badge bill-summary__badge--over">${crossIcon(STATUS_CRITICAL)}Over budget</span>`
    : '';

  container.innerHTML = `
    <div class="bill-card bill-summary">
      <div class="bill-summary__head">
        <span class="bill-summary__label">${escapeHtml(budget.monthLabel)} spend</span>
        ${overBudgetBadge}
      </div>
      <div class="bill-summary__figure">${fmtPrecise(budget.spentUsd)}
        <span class="bill-summary__of">of ${fmt(budget.capUsd)} cap</span>
      </div>
      <div class="bill-meter" role="progressbar" aria-valuenow="${Math.round(pct * 100)}" aria-valuemin="0" aria-valuemax="100">
        <div class="bill-meter__track">
          <div class="bill-meter__fill" style="width: ${fillPct}%; background: ${fillColor};"></div>
        </div>
        <span class="bill-meter__pct">${Math.round(pct * 100)}%</span>
      </div>
    </div>
  `.trim();
}

// ---- 2. Spend by provider -----------------------------------------------------

export function renderProviders(container: HTMLElement, data: ProviderSpend[]): void {
  const max = Math.max(...data.map(d => d.amountUsd), 1);

  const rows = data
    .slice()
    .sort((a, b) => b.amountUsd - a.amountUsd)
    .map(d => {
      const color = PROVIDER_COLORS[d.provider];
      const widthPct = (d.amountUsd / max) * 100;
      return `
        <li class="bill-hbar-row">
          <span class="bill-hbar-row__label">
            <span class="bill-hbar-row__swatch" style="background: ${color};"></span>
            ${escapeHtml(d.provider)}
          </span>
          <span class="bill-hbar-row__track">
            <span class="bill-hbar-row__fill" style="width: ${widthPct}%; background: ${color};" title="${escapeHtml(d.provider)}: ${fmtPrecise(d.amountUsd)}"></span>
          </span>
          <span class="bill-hbar-row__value">${fmt(d.amountUsd)}</span>
        </li>
      `;
    })
    .join('');

  container.innerHTML = `
    <div class="bill-card">
      <h2 class="bill-card__title">Spend by provider</h2>
      <ul class="bill-hbar-list">${rows}</ul>
    </div>
  `.trim();
}

// ---- 3. Spend by service (top N) ----------------------------------------------

export function renderServices(container: HTMLElement, data: ServiceSpend[], topN = 6): void {
  const sorted = data.slice().sort((a, b) => b.amountUsd - a.amountUsd);
  const top = sorted.slice(0, topN);
  const max = Math.max(...top.map(d => d.amountUsd), 1);

  const rows = top
    .map(d => {
      const widthPct = (d.amountUsd / max) * 100;
      return `
        <li class="bill-hbar-row">
          <span class="bill-hbar-row__label" title="${escapeHtml(d.service)} (${escapeHtml(d.provider)})">
            ${escapeHtml(d.service)}
          </span>
          <span class="bill-hbar-row__track">
            <span class="bill-hbar-row__fill" style="width: ${widthPct}%; background: ${ACCENT};" title="${escapeHtml(d.service)}: ${fmtPrecise(d.amountUsd)}"></span>
          </span>
          <span class="bill-hbar-row__value">${fmt(d.amountUsd)}</span>
        </li>
      `;
    })
    .join('');

  container.innerHTML = `
    <div class="bill-card">
      <h2 class="bill-card__title">Top ${top.length} services</h2>
      <ul class="bill-hbar-list bill-hbar-list--services">${rows}</ul>
    </div>
  `.trim();
}

// ---- 4. Daily spend trend (30 days), plain SVG path ----------------------------

export function renderTrend(container: HTMLElement, data: DailySpend[]): void {
  const width = 720;
  const height = 200;
  const padL = 8;
  const padR = 60; // room for the end-value direct label
  const padT = 16;
  const padB = 24;
  const plotW = width - padL - padR;
  const plotH = height - padT - padB;

  const values = data.map(d => d.amountUsd);
  const max = Math.max(...values);
  const min = 0; // spend charts anchor to a zero baseline

  const x = (i: number): number => padL + (i / (data.length - 1)) * plotW;
  const y = (v: number): number => padT + plotH - ((v - min) / (max - min || 1)) * plotH;

  const linePoints = data.map((d, i) => `${x(i)},${y(d.amountUsd)}`).join(' ');
  const areaPoints = `${padL},${padT + plotH} ${linePoints} ${padL + plotW},${padT + plotH}`;

  const last = data[data.length - 1];
  const first = data[0];
  const mid = data[Math.floor(data.length / 2)];

  // Sparse hover targets: one small circle per point, native <title> tooltip.
  const dots = data
    .map((d, i) => `<circle cx="${x(i)}" cy="${y(d.amountUsd)}" r="7" fill="transparent" class="bill-trend__hit"><title>${escapeHtml(d.date)}: ${fmtPrecise(d.amountUsd)}</title></circle>`)
    .join('');

  container.innerHTML = `
    <div class="bill-card">
      <h2 class="bill-card__title">Daily spend — last 30 days</h2>
      <svg class="bill-trend" viewBox="0 0 ${width} ${height}" preserveAspectRatio="none" role="img" aria-label="Daily spend over the last 30 days">
        <line x1="${padL}" y1="${padT + plotH}" x2="${padL + plotW}" y2="${padT + plotH}" class="bill-trend__baseline" />
        <polygon points="${areaPoints}" class="bill-trend__area" />
        <polyline points="${linePoints}" class="bill-trend__line" />
        <circle cx="${x(data.length - 1)}" cy="${y(last.amountUsd)}" r="4" class="bill-trend__end-dot" />
        ${dots}
        <text x="${padL}" y="${height - 6}" class="bill-trend__axis-label">${escapeHtml(first.date.slice(5))}</text>
        <text x="${x(Math.floor(data.length / 2))}" y="${height - 6}" text-anchor="middle" class="bill-trend__axis-label">${escapeHtml(mid.date.slice(5))}</text>
        <text x="${padL + plotW}" y="${height - 6}" text-anchor="end" class="bill-trend__axis-label">${escapeHtml(last.date.slice(5))}</text>
        <text x="${x(data.length - 1) + 10}" y="${y(last.amountUsd) + 4}" class="bill-trend__end-label">${fmt(last.amountUsd)}</text>
      </svg>
    </div>
  `.trim();
}

// ---- 5. Tag-based cost allocation table -----------------------------------------

export function renderTagTable(container: HTMLElement, rows: TagAllocationRow[]): void {
  const sorted = rows.slice().sort((a, b) => b.amountUsd - a.amountUsd);
  const matchedCount = sorted.filter(r => r.matched).length;

  const body = sorted
    .map(r => {
      const statusHtml = r.matched
        ? `${checkIcon(STATUS_GOOD)}<span class="bill-status-text bill-status-text--good">Matched</span>`
        : `${crossIcon(STATUS_CRITICAL)}<span class="bill-status-text bill-status-text--bad">Unmatched</span>`;
      return `
        <tr>
          <td class="bill-table__tag">${escapeHtml(r.tag)}</td>
          <td>
            <span class="bill-hbar-row__swatch" style="background: ${PROVIDER_COLORS[r.provider]};"></span>
            ${escapeHtml(r.provider)}
          </td>
          <td class="bill-table__amount">${fmtPrecise(r.amountUsd)}</td>
          <td class="bill-table__status">${statusHtml}</td>
        </tr>
      `;
    })
    .join('');

  container.innerHTML = `
    <div class="bill-card">
      <div class="bill-card__head">
        <h2 class="bill-card__title">Tag-based cost allocation</h2>
        <span class="bill-card__subtitle">${matchedCount} / ${sorted.length} matched to receipts</span>
      </div>
      <div class="bill-table-wrap">
        <table class="bill-table">
          <thead>
            <tr>
              <th>Tag</th>
              <th>Provider</th>
              <th>Amount</th>
              <th>Reconciliation</th>
            </tr>
          </thead>
          <tbody>${body}</tbody>
        </table>
      </div>
    </div>
  `.trim();
}
