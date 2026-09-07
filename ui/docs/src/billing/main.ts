import '../styles/billing.css';

import {
  sampleBudget,
  sampleProviderSpend,
  sampleServiceSpend,
  sampleDailySpend,
  sampleTagAllocation,
  // TODO: replace with real data fetch once the `ledgerr_gcp_billing` MCP
  // tool ships — see src/billing/sampleData.ts for the shapes consumed
  // below and the exact boundary to swap out.
} from './sampleData.js';
import { renderSummary, renderProviders, renderServices, renderTrend, renderTagTable } from './render.js';

function mount(id: string): HTMLElement {
  const el = document.getElementById(id);
  if (!el) throw new Error(`#${id} element not found`);
  return el;
}

renderSummary(mount('bill-summary'), sampleBudget);
renderProviders(mount('bill-providers'), sampleProviderSpend);
renderServices(mount('bill-services'), sampleServiceSpend);
renderTrend(mount('bill-trend'), sampleDailySpend);
renderTagTable(mount('bill-tags'), sampleTagAllocation);
