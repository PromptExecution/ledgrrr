// ============================================================================
// Sample FOCUS billing data
//
// TODO: replace with real data fetch once the `ledgerr_gcp_billing` MCP tool
// ships. Everything in this file is hand-written sample data for UI
// scaffolding only — there is no live billing pipeline yet. Once the MCP
// tool exists, swap `sampleData.ts` for a module that fetches and shapes
// real FOCUS (FinOps Cost and Usage Spec) records; the render/ layer below
// consumes the same shapes either way, so that swap should not require
// touching render.ts or main.ts.
// ============================================================================

export type CloudProvider = 'GCP' | 'AWS' | 'Alicloud' | 'HuggingFace';

export interface BudgetSummary {
  /** Human label for the billing period, e.g. "September 2026". */
  monthLabel: string;
  /** Total spend so far this period, in USD. */
  spentUsd: number;
  /** Configured budget cap for the period, in USD. */
  capUsd: number;
}

export interface ProviderSpend {
  provider: CloudProvider;
  amountUsd: number;
}

export interface ServiceSpend {
  service: string;
  provider: CloudProvider;
  amountUsd: number;
}

export interface DailySpend {
  /** ISO date, e.g. "2026-08-09". */
  date: string;
  amountUsd: number;
}

export interface TagAllocationRow {
  tag: string;
  provider: CloudProvider;
  amountUsd: number;
  /** Whether this tagged spend reconciles against a known payment receipt. */
  matched: boolean;
}

// ---- Budget summary --------------------------------------------------------

export const sampleBudget: BudgetSummary = {
  monthLabel: 'September 2026',
  spentUsd: 18342.17,
  capUsd: 20000,
};

// ---- Spend by provider ------------------------------------------------------
// Sums to sampleBudget.spentUsd.

export const sampleProviderSpend: ProviderSpend[] = [
  { provider: 'GCP', amountUsd: 9820.44 },
  { provider: 'AWS', amountUsd: 5110.30 },
  { provider: 'Alicloud', amountUsd: 2140.00 },
  { provider: 'HuggingFace', amountUsd: 1271.43 },
];

// ---- Spend by service (top N + remainder) -----------------------------------
// The six named services plus "Other services" sum to sampleBudget.spentUsd.

export const sampleServiceSpend: ServiceSpend[] = [
  { service: 'Compute Engine', provider: 'GCP', amountUsd: 4210.12 },
  { service: 'EC2', provider: 'AWS', amountUsd: 3102.40 },
  { service: 'BigQuery', provider: 'GCP', amountUsd: 2890.55 },
  { service: 'S3', provider: 'AWS', amountUsd: 1204.10 },
  { service: 'OSS Storage', provider: 'Alicloud', amountUsd: 980.00 },
  { service: 'Inference Endpoints', provider: 'HuggingFace', amountUsd: 861.20 },
  { service: 'Other services', provider: 'GCP', amountUsd: 5093.80 },
];

// ---- Daily spend trend (last 30 days) ---------------------------------------
// Hand-written, not randomly generated, so the sample chart looks the same
// on every reload. Roughly averages to sampleBudget.spentUsd / 30.

export const sampleDailySpend: DailySpend[] = [
  { date: '2026-08-09', amountUsd: 540 },
  { date: '2026-08-10', amountUsd: 512 },
  { date: '2026-08-11', amountUsd: 588 },
  { date: '2026-08-12', amountUsd: 601 },
  { date: '2026-08-13', amountUsd: 470 },
  { date: '2026-08-14', amountUsd: 455 },
  { date: '2026-08-15', amountUsd: 622 },
  { date: '2026-08-16', amountUsd: 649 },
  { date: '2026-08-17', amountUsd: 590 },
  { date: '2026-08-18', amountUsd: 611 },
  { date: '2026-08-19', amountUsd: 583 },
  { date: '2026-08-20', amountUsd: 502 },
  { date: '2026-08-21', amountUsd: 495 },
  { date: '2026-08-22', amountUsd: 705 },
  { date: '2026-08-23', amountUsd: 812 },
  { date: '2026-08-24', amountUsd: 690 },
  { date: '2026-08-25', amountUsd: 634 },
  { date: '2026-08-26', amountUsd: 598 },
  { date: '2026-08-27', amountUsd: 560 },
  { date: '2026-08-28', amountUsd: 571 },
  { date: '2026-08-29', amountUsd: 640 },
  { date: '2026-08-30', amountUsd: 703 },
  { date: '2026-08-31', amountUsd: 661 },
  { date: '2026-09-01', amountUsd: 615 },
  { date: '2026-09-02', amountUsd: 588 },
  { date: '2026-09-03', amountUsd: 602 },
  { date: '2026-09-04', amountUsd: 720 },
  { date: '2026-09-05', amountUsd: 745 },
  { date: '2026-09-06', amountUsd: 690 },
  { date: '2026-09-07', amountUsd: 668 },
];

// ---- Tag-based cost allocation ----------------------------------------------
// Reconciliation status against payment receipts is illustrative only.

export const sampleTagAllocation: TagAllocationRow[] = [
  { tag: 'team:ml-platform', provider: 'GCP', amountUsd: 6420.10, matched: true },
  { tag: 'team:ingest', provider: 'AWS', amountUsd: 3980.25, matched: true },
  { tag: 'env:prod', provider: 'GCP', amountUsd: 5210.60, matched: true },
  { tag: 'env:staging', provider: 'Alicloud', amountUsd: 1140.00, matched: true },
  { tag: 'project:ledgrrr-core', provider: 'AWS', amountUsd: 1130.05, matched: false },
  { tag: 'cost-center:eng', provider: 'HuggingFace', amountUsd: 1271.43, matched: false },
  { tag: 'untagged', provider: 'Alicloud', amountUsd: 1000.00, matched: false },
];
