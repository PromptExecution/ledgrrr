//! `ledgerr-gcp-billing` — GCP BigQuery Billing Export (FOCUS-conformant)
//! ingestion.
//!
//! Shells out to the `bq` CLI via `tokio::process::Command` to query a
//! FOCUS-conformant BigQuery billing export table, then maps each row into
//! a [`ledgerr_focus::CostAndUsageRow`]. Mirrors `ledgerr_cloud::gcp`'s
//! subprocess-shell-out convention (`ledgerr-cloud`'s crate docs: "no cloud
//! SDK dependencies") — this crate links no `google-cloud-*` crate either.
//!
//! # No live export yet
//! The real GCP BigQuery Billing Export (FOCUS-conformant) does not exist
//! in any environment this crate has been built against — enabling it is a
//! manual step in the GCP console that has not yet been performed. Row
//! shapes here are therefore guessed defensively from Google's published
//! column names (which are FOCUS's own CamelCase column names — see
//! <https://cloud.google.com/billing/docs/how-to/export-data-bigquery-tables/focus-schema>)
//! and validated against a synthetic fixture in `tests/fixtures/`, not a
//! real export sample. [`map_row_to_focus`] pulls fields defensively
//! (missing optional columns become `None`; missing mandatory columns are a
//! [`MappingError`]) so that once a real export exists, only the field-name
//! guesses — not the overall shape — should need correction.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use thiserror::Error;

use ledgerr_focus::{ChargeCategory, ChargeFrequency, CostAndUsageRow};

/// Default wall-clock limit for one `bq query` subprocess. Overridable via
/// `GCP_BILLING_QUERY_TIMEOUT_SECS`. Mirrors `PdfIngestOp`'s subprocess
/// timeout convention — without it, a `bq` hanging on auth/network blocks
/// the (single-threaded, stdio) MCP server indefinitely.
pub const DEFAULT_QUERY_TIMEOUT_SECS: u64 = 120;

/// Default explicit `--max_rows` for `bq query`. Overridable via
/// `GCP_BILLING_MAX_ROWS`. MUST be passed explicitly: `bq`'s own default is
/// 100 rows and it silently truncates beyond that with a zero exit status —
/// a monthly billing export would quietly ingest only its first 100 rows.
pub const DEFAULT_MAX_ROWS: u64 = 50_000;

/// A single loosely-typed row from `bq query --format=json`.
///
/// Kept as a raw [`serde_json::Value`] rather than a strongly-typed struct
/// because no live GCP FOCUS export schema sample exists yet to validate a
/// typed shape against — see the crate-level docs.
pub type RawBillingRow = Value;

/// Errors from querying the `bq` CLI.
#[derive(Debug, Error)]
pub enum BigQueryError {
    /// Spawning or waiting on the `bq` CLI process failed at the OS level.
    #[error("bq CLI process error: {0}")]
    Io(#[from] std::io::Error),
    /// `bq query` ran and exited non-zero.
    #[error("bq query failed: {0}")]
    QueryFailed(String),
    /// `bq query --format=json` exited zero but its stdout could not be
    /// parsed as the expected JSON array of row objects.
    #[error("failed to parse bq query output: {0}")]
    Parse(String),
}

/// Errors mapping a [`RawBillingRow`] into a [`CostAndUsageRow`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum MappingError {
    /// A FOCUS mandatory column was absent from the row.
    #[error("missing required field '{0}' in billing row")]
    MissingField(String),
    /// A column was present but its value could not be interpreted as the
    /// expected type (e.g. an unparseable decimal, timestamp, or enum
    /// string).
    #[error("invalid value for field '{field}': {reason}")]
    InvalidValue { field: String, reason: String },
}

/// A GCP BigQuery FOCUS Billing Export data source.
///
/// Identifies one `project.dataset.table` BigQuery billing export table.
/// Ports the `_b00t_` bash datum convention of shelling out to an
/// already-installed CLI (here `bq`, the BigQuery command-line tool) rather
/// than linking a cloud SDK — see `ledgerr_cloud::gcp` for the analogous
/// `gcloud`-based budget-cap check this mirrors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BigQueryFocusSource {
    pub project_id: String,
    pub dataset: String,
    pub table: String,
}

impl BigQueryFocusSource {
    pub fn new(
        project_id: impl Into<String>,
        dataset: impl Into<String>,
        table: impl Into<String>,
    ) -> Self {
        Self {
            project_id: project_id.into(),
            dataset: dataset.into(),
            table: table.into(),
        }
    }

    /// Validate the three identifier components against BigQuery Standard SQL
    /// identifier rules. A value containing a backtick (or any other
    /// non-identifier character) could break out of the `` `p.d.t` `` quoting
    /// in [`qualified_table`] and inject arbitrary SQL — identifiers come
    /// from env vars / scheduled-event config, which are semi-trusted at
    /// best. Rejecting them here keeps the interpolation safe by
    /// construction.
    pub fn validate(&self) -> Result<(), MappingError> {
        for (field, value) in [
            ("project_id", &self.project_id),
            ("dataset", &self.dataset),
            ("table", &self.table),
        ] {
            if value.is_empty()
                || !value
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':'))
            {
                return Err(MappingError::InvalidValue {
                    field: field.to_string(),
                    reason: format!(
                        "'{value}' is not a valid BigQuery identifier component \
                         (allowed: ASCII alphanumerics, '_', '-', '.', ':')"
                    ),
                });
            }
        }
        Ok(())
    }

    /// The fully-qualified `` `project.dataset.table` `` BigQuery table
    /// reference, backtick-quoted per BigQuery Standard SQL identifier
    /// syntax. Callers MUST [`validate`](Self::validate) first — the
    /// backtick quoting is only injection-safe for identifier-shaped
    /// components.
    fn qualified_table(&self) -> String {
        format!("`{}.{}.{}`", self.project_id, self.dataset, self.table)
    }

    /// The result of one `bq query` + FOCUS mapping pass: the mapped rows,
    /// per-row mapping errors, and whether `bq` truncated the result set.
    ///
    /// Query all billing rows charged on or after `since`.
    ///
    /// Shells `bq query --use_legacy_sql=false --max_rows=<cap>
    /// --format=json "SELECT * FROM \`{project}.{dataset}.{table}\` WHERE
    /// ChargePeriodStart >= TIMESTAMP(...) ORDER BY ChargePeriodStart"` via
    /// `tokio::process::Command`, mirroring
    /// `ledgerr_cloud::gcp::GcpProvider::fetch_budget`'s subprocess pattern.
    ///
    /// Two hardening measures over a naive `bq query` call:
    /// - `--max_rows` is ALWAYS passed explicitly (default
    ///   [`DEFAULT_MAX_ROWS`], override `GCP_BILLING_MAX_ROWS`): `bq`'s own
    ///   default is 100 rows and it silently truncates beyond that with a
    ///   zero exit status.
    /// - The whole subprocess is bounded by a wall-clock timeout (default
    ///   [`DEFAULT_QUERY_TIMEOUT_SECS`] seconds, override
    ///   `GCP_BILLING_QUERY_TIMEOUT_SECS`), mirroring `PdfIngestOp`'s
    ///   subprocess timeout — a hung `bq` must not hang the caller forever.
    ///
    /// Returns the rows plus a `truncated` flag: `true` when the row count
    /// hit the `--max_rows` cap, meaning the window contained more rows
    /// than one query returned and the caller MUST narrow `since` (or page)
    /// rather than treat the result as complete.
    pub async fn query_rows(
        &self,
        since: DateTime<Utc>,
    ) -> Result<QueryResult, BigQueryError> {
        self.validate().map_err(|e| {
            BigQueryError::QueryFailed(format!("invalid BigQuery table identifier: {e}"))
        })?;

        let max_rows = env_u64("GCP_BILLING_MAX_ROWS", DEFAULT_MAX_ROWS);
        let timeout_secs = env_u64("GCP_BILLING_QUERY_TIMEOUT_SECS", DEFAULT_QUERY_TIMEOUT_SECS);

        let query = format!(
            "SELECT * FROM {} WHERE ChargePeriodStart >= TIMESTAMP('{}') ORDER BY ChargePeriodStart",
            self.qualified_table(),
            since.to_rfc3339(),
        );

        let max_rows_arg = format!("--max_rows={max_rows}");
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            tokio::process::Command::new("bq")
                .args([
                    "query",
                    "--use_legacy_sql=false",
                    &max_rows_arg,
                    "--format=json",
                    &query,
                ])
                .output(),
        )
        .await
        .map_err(|_| {
            BigQueryError::QueryFailed(format!(
                "bq query timed out after {timeout_secs}s (override: GCP_BILLING_QUERY_TIMEOUT_SECS)"
            ))
        })??;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(BigQueryError::QueryFailed(stderr.into_owned()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let rows = parse_bq_json_rows(&stdout)?;
        Ok(QueryResult {
            truncated: rows.len() as u64 >= max_rows,
            rows,
        })
    }
}

/// Rows from one `bq query`, plus whether `--max_rows` truncated them.
#[derive(Debug, Clone)]
pub struct QueryResult {
    /// `true` when the returned row count hit the `--max_rows` cap — the
    /// window contains more rows than this query returned.
    pub truncated: bool,
    pub rows: Vec<RawBillingRow>,
}

fn env_u64(var: &str, default: u64) -> u64 {
    std::env::var(var)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(default)
}

/// Parse `bq query --format=json` stdout (a top-level JSON array of row
/// objects) into raw rows.
fn parse_bq_json_rows(stdout: &str) -> Result<Vec<RawBillingRow>, BigQueryError> {
    let value: Value = serde_json::from_str(stdout)
        .map_err(|e| BigQueryError::Parse(format!("invalid bq query JSON: {e}")))?;
    match value {
        Value::Array(rows) => Ok(rows),
        _ => Err(BigQueryError::Parse(
            "expected top-level JSON array from `bq query --format=json`".to_string(),
        )),
    }
}

// ── Row -> FOCUS mapping ──────────────────────────────────────────────────

/// Map one raw `bq query --format=json` row into a
/// [`ledgerr_focus::CostAndUsageRow`].
///
/// Field names are Google's published FOCUS BigQuery export column names
/// (FOCUS's own CamelCase names, e.g. `BillingAccountId`,
/// `ChargePeriodStart`, `BilledCost`). Every mandatory FOCUS column is
/// required here; every conditional column defaults to `None` when absent
/// from the row rather than erroring, since a real export's column set is
/// still unconfirmed. `ServiceProviderName` — mandatory in FOCUS but not
/// confirmed present in Google's actual export — defaults to `"Google
/// Cloud"` when absent, since every row from this source is by definition
/// billed by Google Cloud.
pub fn map_row_to_focus(row: &RawBillingRow) -> Result<CostAndUsageRow, MappingError> {
    let billing_account_id = required_str(row, "BillingAccountId")?;
    let billing_account_name = optional_str(row, "BillingAccountName");
    let billing_currency = required_str(row, "BillingCurrency")?;
    let billing_period_start = required_datetime(row, "BillingPeriodStart")?;
    let billing_period_end = required_datetime(row, "BillingPeriodEnd")?;
    let charge_period_start = required_datetime(row, "ChargePeriodStart")?;
    let charge_period_end = required_datetime(row, "ChargePeriodEnd")?;
    let charge_category = parse_charge_category(&required_str(row, "ChargeCategory")?)?;
    let charge_frequency = parse_charge_frequency(&required_str(row, "ChargeFrequency")?)?;
    let billed_cost = required_decimal(row, "BilledCost")?;
    let effective_cost = required_decimal(row, "EffectiveCost")?;
    let service_provider_name =
        optional_str(row, "ServiceProviderName").unwrap_or_else(|| "Google Cloud".to_string());
    let service_name = required_str(row, "ServiceName")?;
    let sku_id = required_str(row, "SkuId")?;

    Ok(CostAndUsageRow {
        billing_account_id,
        billing_account_name,
        billing_currency,
        billing_period_start,
        billing_period_end,
        charge_period_start,
        charge_period_end,
        charge_category,
        charge_frequency,
        billed_cost,
        effective_cost,
        service_provider_name,
        service_name,
        sku_id,

        billing_account_type: optional_str(row, "BillingAccountType"),
        charge_class: None,
        charge_description: optional_str(row, "ChargeDescription"),
        commitment_discount_id: optional_str(row, "CommitmentDiscountId"),
        commitment_discount_name: optional_str(row, "CommitmentDiscountName"),
        commitment_discount_category: optional_str(row, "CommitmentDiscountCategory"),
        commitment_discount_type: optional_str(row, "CommitmentDiscountType"),
        commitment_discount_status: optional_str(row, "CommitmentDiscountStatus"),
        commitment_discount_quantity: optional_decimal(row, "CommitmentDiscountQuantity")?,
        commitment_discount_unit: optional_str(row, "CommitmentDiscountUnit"),
        consumed_quantity: optional_decimal(row, "ConsumedQuantity")?,
        consumed_unit: optional_str(row, "ConsumedUnit"),
        contracted_cost: optional_decimal(row, "ContractedCost")?,
        contracted_unit_price: optional_decimal(row, "ContractedUnitPrice")?,
        invoice_id: optional_str(row, "InvoiceId"),
        invoice_issuer_name: optional_str(row, "InvoiceIssuerName"),
        list_cost: optional_decimal(row, "ListCost")?,
        list_unit_price: optional_decimal(row, "ListUnitPrice")?,
        pricing_category: None,
        pricing_quantity: optional_decimal(row, "PricingQuantity")?,
        pricing_unit: optional_str(row, "PricingUnit"),
        region_id: optional_str(row, "RegionId"),
        region_name: optional_str(row, "RegionName"),
        resource_id: optional_str(row, "ResourceId"),
        resource_name: optional_str(row, "ResourceName"),
        resource_type: optional_str(row, "ResourceType"),
        service_category: optional_str(row, "ServiceCategory"),
        service_subcategory: optional_str(row, "ServiceSubcategory"),
        sku_meter: optional_str(row, "SkuMeter"),
        sku_price_id: optional_str(row, "SkuPriceId"),
        sku_price_details: optional_str(row, "SkuPriceDetails"),
        sub_account_id: optional_str(row, "SubAccountId"),
        sub_account_name: optional_str(row, "SubAccountName"),
        sub_account_type: optional_str(row, "SubAccountType"),
        availability_zone: optional_str(row, "AvailabilityZone"),
        capacity_reservation_id: optional_str(row, "CapacityReservationId"),
        capacity_reservation_status: optional_str(row, "CapacityReservationStatus"),
        host_provider_name: optional_str(row, "HostProviderName"),

        tags: extract_tags(row, "Tags"),

        x_experiment_id: None,
        x_variant: None,
        x_personality: None,
        x_experiment_score: None,
        x_agent_id: None,
        x_reasoning_review: None,
    })
}

fn required_str(row: &Value, field: &str) -> Result<String, MappingError> {
    optional_str(row, field).ok_or_else(|| MappingError::MissingField(field.to_string()))
}

fn optional_str(row: &Value, field: &str) -> Option<String> {
    match row.get(field) {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(other) => Some(other.to_string()),
    }
}

fn required_decimal(row: &Value, field: &str) -> Result<Decimal, MappingError> {
    optional_decimal(row, field)?.ok_or_else(|| MappingError::MissingField(field.to_string()))
}

/// Parse a money/quantity column. `bq query --format=json` commonly renders
/// `NUMERIC`/`FLOAT64` columns as JSON strings (to avoid float precision
/// loss); a plain JSON number is also accepted defensively.
fn optional_decimal(row: &Value, field: &str) -> Result<Option<Decimal>, MappingError> {
    match row.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => {
            Decimal::from_str(s.trim())
                .map(Some)
                .map_err(|e| MappingError::InvalidValue {
                    field: field.to_string(),
                    reason: format!("invalid decimal '{s}': {e}"),
                })
        }
        Some(Value::Number(n)) => {
            Decimal::from_str(&n.to_string())
                .map(Some)
                .map_err(|e| MappingError::InvalidValue {
                    field: field.to_string(),
                    reason: format!("invalid decimal '{n}': {e}"),
                })
        }
        Some(other) => Err(MappingError::InvalidValue {
            field: field.to_string(),
            reason: format!("expected string or number, got {other}"),
        }),
    }
}

fn required_datetime(row: &Value, field: &str) -> Result<DateTime<Utc>, MappingError> {
    let value = row
        .get(field)
        .filter(|v| !v.is_null())
        .ok_or_else(|| MappingError::MissingField(field.to_string()))?;
    parse_datetime(field, value)
}

/// Parse a timestamp column. Accepts RFC 3339 strings (the common case for
/// a hand-inspected `bq` JSON sample) and, defensively, a Unix-epoch-seconds
/// number or numeric string — `bq query --format=json` is known to render
/// `TIMESTAMP` columns as raw epoch-seconds in some `bq` CLI versions.
fn parse_datetime(field: &str, value: &Value) -> Result<DateTime<Utc>, MappingError> {
    match value {
        Value::String(s) => {
            if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
                return Ok(dt.with_timezone(&Utc));
            }
            if let Ok(epoch) = s.parse::<f64>() {
                return epoch_seconds_to_datetime(field, epoch);
            }
            Err(MappingError::InvalidValue {
                field: field.to_string(),
                reason: format!("unrecognized timestamp format: {s}"),
            })
        }
        Value::Number(n) => {
            let epoch = n.as_f64().ok_or_else(|| MappingError::InvalidValue {
                field: field.to_string(),
                reason: "timestamp number out of range".to_string(),
            })?;
            epoch_seconds_to_datetime(field, epoch)
        }
        other => Err(MappingError::InvalidValue {
            field: field.to_string(),
            reason: format!("expected string or number for timestamp, got {other}"),
        }),
    }
}

fn epoch_seconds_to_datetime(field: &str, epoch: f64) -> Result<DateTime<Utc>, MappingError> {
    let secs = epoch.trunc() as i64;
    let nanos = (epoch.fract().abs() * 1e9).round() as u32;
    DateTime::from_timestamp(secs, nanos).ok_or_else(|| MappingError::InvalidValue {
        field: field.to_string(),
        reason: format!("timestamp out of range: {epoch}"),
    })
}

/// Parse the FOCUS `ChargeCategory` column. Values are FOCUS's own
/// PascalCase category names (`Usage`, `Purchase`, `Tax`, `Credit`,
/// `Adjustment`).
fn parse_charge_category(s: &str) -> Result<ChargeCategory, MappingError> {
    match s {
        "Usage" => Ok(ChargeCategory::Usage),
        "Purchase" => Ok(ChargeCategory::Purchase),
        "Tax" => Ok(ChargeCategory::Tax),
        "Credit" => Ok(ChargeCategory::Credit),
        "Adjustment" => Ok(ChargeCategory::Adjustment),
        other => Err(MappingError::InvalidValue {
            field: "ChargeCategory".to_string(),
            reason: format!("unrecognized charge category: {other}"),
        }),
    }
}

/// Parse the FOCUS `ChargeFrequency` column. The FOCUS spec's own string
/// values are hyphenated (`One-Time`, `Recurring`, `Usage-Based`); this
/// normalizes case and strips hyphens/spaces/underscores before matching so
/// both the spec's hyphenated form and `ledgerr_focus::ChargeFrequency`'s
/// Rust variant-name form (`OneTime`, `UsageBased`) are accepted.
fn parse_charge_frequency(s: &str) -> Result<ChargeFrequency, MappingError> {
    let normalized: String = s
        .chars()
        .filter(|c| !matches!(c, '-' | '_' | ' '))
        .collect::<String>()
        .to_lowercase();
    match normalized.as_str() {
        "onetime" => Ok(ChargeFrequency::OneTime),
        "recurring" => Ok(ChargeFrequency::Recurring),
        "usagebased" => Ok(ChargeFrequency::UsageBased),
        _ => Err(MappingError::InvalidValue {
            field: "ChargeFrequency".to_string(),
            reason: format!("unrecognized charge frequency: {s}"),
        }),
    }
}

/// Extract the `Tags` column into a flat string-to-string map.
///
/// Handles the shapes a BigQuery `Tags`/`Labels` column plausibly takes:
/// - a JSON object (`{"env": "prod"}`) — the common case for a `STRING`
///   column holding serialized JSON, or a `bq` JSON export of a `JSON`-typed
///   column.
/// - a JSON array of `{"key": ..., "value": ...}` structs — the shape of a
///   BigQuery `ARRAY<STRUCT<key STRING, value STRING>>` column.
/// - anything else (absent, null, wrong shape) becomes an empty map rather
///   than an error, since tags are metadata, not a mandatory FOCUS column.
fn extract_tags(row: &Value, field: &str) -> HashMap<String, String> {
    match row.get(field) {
        Some(Value::Object(map)) => map
            .iter()
            .map(|(k, v)| (k.clone(), value_to_string(v)))
            .collect(),
        Some(Value::Array(entries)) => entries
            .iter()
            .filter_map(|entry| {
                let key = entry.get("key")?.as_str()?.to_string();
                let value = entry.get("value").map(value_to_string).unwrap_or_default();
                Some((key, value))
            })
            .collect(),
        Some(Value::String(s)) => serde_json::from_str::<HashMap<String, Value>>(s)
            .map(|map| {
                map.into_iter()
                    .map(|(k, v)| (k, value_to_string(&v)))
                    .collect()
            })
            .unwrap_or_default(),
        _ => HashMap::new(),
    }
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

// ── Persistence sink ─────────────────────────────────────────────────────

/// Errors from the FOCUS row sink.
#[derive(Debug, Error)]
pub enum SinkError {
    #[error("sink I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("sink serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

/// Append-only JSONL sidecar sink for mapped [`CostAndUsageRow`]s.
///
/// This is what makes "ingest" real: previously mapped rows were counted and
/// discarded. Mirrors `ledgerr_mcp::focus_tool`'s sidecar convention —
/// env-var path (`GCP_BILLING_SIDECAR_PATH`, default
/// `~/.local/share/b00t/focus/gcp_billing_focus_rows.jsonl`), one JSON object
/// per line, atomic write via tmp+rename, and content-hash dedupe: each row's
/// blake3 digest is keyed on its full serialized content, so re-ingesting the
/// same `ChargePeriodStart` window (the op's documented idempotency contract)
/// appends nothing new.
#[derive(Debug, Clone)]
pub struct FocusSink {
    path: PathBuf,
}

impl FocusSink {
    /// Sink at an explicit path (tests, embedders).
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Sink at `GCP_BILLING_SIDECAR_PATH`, defaulting to
    /// `~/.local/share/b00t/focus/gcp_billing_focus_rows.jsonl` (same base
    /// dir as `focus_tool`'s sidecar). When `$HOME` is unset the default
    /// falls back to a `.ledgerr-gcp-billing` dir under the system temp dir.
    pub fn from_env() -> Self {
        let path = std::env::var("GCP_BILLING_SIDECAR_PATH")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(default_sink_path);
        Self::new(path)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Append rows, skipping any whose content hash already appears in the
    /// file. Returns `(appended, deduped)` counts. Atomic: writes the full
    /// new content to a sibling tmp file then renames over the sink.
    pub fn append(&self, rows: &[CostAndUsageRow]) -> Result<(usize, usize), SinkError> {
        let mut lines: Vec<String> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        if self.path.exists() {
            for line in std::fs::read_to_string(&self.path)?.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                seen.insert(content_hash(line));
                lines.push(line.to_string());
            }
        }

        let mut appended = 0usize;
        let mut deduped = 0usize;
        for row in rows {
            let serialized = serde_json::to_string(row)?;
            if seen.insert(content_hash(&serialized)) {
                lines.push(serialized);
                appended += 1;
            } else {
                deduped += 1;
            }
        }

        if appended == 0 {
            return Ok((0, deduped));
        }

        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = self.path.with_extension("jsonl.tmp");
        let mut content = lines.join("\n");
        content.push('\n');
        std::fs::write(&tmp, content)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok((appended, deduped))
    }

    /// Read every row currently in the sink (for summaries/verification).
    pub fn read_all(&self) -> Result<Vec<CostAndUsageRow>, SinkError> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        std::fs::read_to_string(&self.path)?
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| Ok(serde_json::from_str(l)?))
            .collect()
    }
}

fn default_sink_path() -> PathBuf {
    let base = std::env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .map(|h| h.join(".local/share/b00t/focus"))
        .unwrap_or_else(|| std::env::temp_dir().join(".ledgerr-gcp-billing"));
    base.join("gcp_billing_focus_rows.jsonl")
}

/// Blake3 hex digest of a row's serialized content — the dedupe key, same
/// hashing convention `RecordCostOp`/`IngestStatementOp` use for tx ids.
fn content_hash(serialized: &str) -> String {
    blake3::hash(serialized.as_bytes()).to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_rows() -> Vec<RawBillingRow> {
        let path = format!(
            "{}/tests/fixtures/bq_focus_sample.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let contents = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read fixture {path}: {e}"));
        serde_json::from_str(&contents).expect("fixture is a JSON array of row objects")
    }

    #[test]
    fn maps_compute_engine_row_with_full_fields() {
        let rows = fixture_rows();
        let row = map_row_to_focus(&rows[0]).expect("row 0 maps cleanly");

        assert_eq!(row.billing_account_id, "012345-6789AB-CDEF01");
        assert_eq!(
            row.billing_account_name.as_deref(),
            Some("Acme Corp Billing")
        );
        assert_eq!(row.billing_currency, "USD");
        assert_eq!(row.charge_category, ChargeCategory::Usage);
        assert_eq!(row.charge_frequency, ChargeFrequency::UsageBased);
        assert_eq!(row.billed_cost, Decimal::from_str("142.37").unwrap());
        assert_eq!(row.effective_cost, Decimal::from_str("128.11").unwrap());
        assert_eq!(row.service_provider_name, "Google Cloud");
        assert_eq!(row.service_name, "Compute Engine");
        assert_eq!(row.sku_id, "6F81-5844-456A");
        assert_eq!(
            row.resource_id.as_deref(),
            Some("projects/acme-prod/zones/us-central1-a/instances/web-01")
        );
        assert_eq!(row.region_id.as_deref(), Some("us-central1"));
        assert_eq!(row.sub_account_id.as_deref(), Some("987654321000"));
        assert_eq!(row.sub_account_name.as_deref(), Some("acme-prod"));
        assert_eq!(
            row.pricing_quantity,
            Some(Decimal::from_str("720").unwrap())
        );
        assert_eq!(row.pricing_unit.as_deref(), Some("Hour"));
        assert_eq!(row.tags.get("env").map(String::as_str), Some("prod"));
        assert_eq!(row.tags.get("team").map(String::as_str), Some("platform"));
        assert_eq!(
            row.charge_period_start,
            DateTime::parse_from_rfc3339("2024-06-15T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc)
        );
    }

    #[test]
    fn maps_cloud_storage_row_with_hyphenless_frequency() {
        let rows = fixture_rows();
        let row = map_row_to_focus(&rows[1]).expect("row 1 maps cleanly");

        assert_eq!(row.service_name, "Cloud Storage");
        assert_eq!(row.charge_frequency, ChargeFrequency::UsageBased);
        assert_eq!(row.billed_cost, Decimal::from_str("18.90").unwrap());
        assert_eq!(row.tags.len(), 1);
        assert_eq!(row.tags.get("env").map(String::as_str), Some("prod"));
    }

    #[test]
    fn maps_minimal_row_with_defaults_for_missing_optional_fields() {
        let rows = fixture_rows();
        let row = map_row_to_focus(&rows[2]).expect("row 2 maps cleanly");

        assert_eq!(row.charge_category, ChargeCategory::Tax);
        assert_eq!(row.charge_frequency, ChargeFrequency::OneTime);
        assert_eq!(row.billing_account_name, None);
        // ServiceProviderName absent from the row -> defaulted.
        assert_eq!(row.service_provider_name, "Google Cloud");
        assert_eq!(row.resource_id, None);
        assert!(row.tags.is_empty());
    }

    #[test]
    fn missing_mandatory_field_is_a_mapping_error() {
        let mut row = fixture_rows().into_iter().next().unwrap();
        row.as_object_mut().unwrap().remove("BilledCost");

        let err = map_row_to_focus(&row).unwrap_err();
        assert_eq!(err, MappingError::MissingField("BilledCost".to_string()));
    }

    #[test]
    fn unrecognized_charge_category_is_a_mapping_error() {
        let mut row = fixture_rows().into_iter().next().unwrap();
        row.as_object_mut().unwrap().insert(
            "ChargeCategory".to_string(),
            Value::String("Bogus".to_string()),
        );

        let err = map_row_to_focus(&row).unwrap_err();
        assert!(
            matches!(err, MappingError::InvalidValue { field, .. } if field == "ChargeCategory")
        );
    }

    #[test]
    fn epoch_seconds_timestamp_is_accepted() {
        let mut row = fixture_rows().into_iter().next().unwrap();
        // 2024-06-15T00:00:00Z as Unix epoch seconds.
        row.as_object_mut().unwrap().insert(
            "ChargePeriodStart".to_string(),
            Value::String("1718409600".to_string()),
        );

        let mapped = map_row_to_focus(&row).expect("epoch-seconds timestamp maps cleanly");
        assert_eq!(
            mapped.charge_period_start,
            DateTime::parse_from_rfc3339("2024-06-15T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc)
        );
    }

    #[test]
    fn parses_bq_query_json_array_output() {
        let stdout = std::fs::read_to_string(format!(
            "{}/tests/fixtures/bq_focus_sample.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        let rows = parse_bq_json_rows(&stdout).expect("parses as a row array");
        assert_eq!(rows.len(), 3);
    }

    #[test]
    fn validate_rejects_backtick_injection_in_identifiers() {
        let evil = BigQueryFocusSource::new(
            "acme`.`evil-- ",
            "billing_export",
            "gcp_billing_export_resource_v1",
        );
        let err = evil.validate().unwrap_err();
        assert!(
            matches!(&err, MappingError::InvalidValue { field, .. } if field == "project_id"),
            "backtick in project_id must be rejected, got {err:?}"
        );

        let good = BigQueryFocusSource::new(
            "acme-billing",
            "billing_export",
            "gcp_billing_export_resource_v1",
        );
        assert!(good.validate().is_ok());

        let empty = BigQueryFocusSource::new("", "d", "t");
        assert!(empty.validate().is_err());
    }

    #[test]
    fn query_rows_rejects_invalid_identifier_before_spawning_bq() {
        // Async fn, but validate() runs before any subprocess spawn — a
        // current-thread runtime is enough and no `bq` binary is needed.
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let evil = BigQueryFocusSource::new("acme`--", "d", "t");
        let err = rt
            .block_on(evil.query_rows(chrono::Utc::now()))
            .unwrap_err();
        assert!(
            matches!(err, BigQueryError::QueryFailed(ref msg) if msg.contains("identifier")),
            "expected identifier validation error, got {err}"
        );
    }

    // ── FocusSink ────────────────────────────────────────────────────────

    fn sink_test_rows() -> Vec<CostAndUsageRow> {
        fixture_rows()
            .iter()
            .map(map_row_to_focus)
            .collect::<Result<Vec<_>, _>>()
            .expect("fixture rows map cleanly")
    }

    #[test]
    fn sink_appends_dedupes_and_survives_reopen() {
        let dir = tempfile::tempdir().expect("tempdir");
        let sink = FocusSink::new(dir.path().join("rows.jsonl"));

        let rows = sink_test_rows();
        let (appended, deduped) = sink.append(&rows).expect("first append");
        assert_eq!((appended, deduped), (rows.len(), 0));

        // Re-ingest the same window: everything dedupes, nothing new lands.
        let (appended, deduped) = sink.append(&rows).expect("second append");
        assert_eq!((appended, deduped), (0, rows.len()));

        // File content survived both passes and reopens cleanly.
        let reopened = FocusSink::new(dir.path().join("rows.jsonl"));
        let read_back = reopened.read_all().expect("read back");
        assert_eq!(read_back, rows);

        // A changed row is NOT a duplicate — content hash keys on full
        // serialized content.
        let mut changed = rows.clone();
        changed[0].billed_cost += Decimal::from_str("0.01").unwrap();
        let (appended, deduped) = sink.append(&changed).expect("third append");
        assert_eq!((appended, deduped), (1, rows.len() - 1));
    }

    #[test]
    fn sink_from_env_honors_sidecar_path_override() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("custom.jsonl");
        // Single-test env mutation: SIDECAR_PATH is read at from_env() time.
        std::env::set_var("GCP_BILLING_SIDECAR_PATH", &path);
        let sink = FocusSink::from_env();
        std::env::remove_var("GCP_BILLING_SIDECAR_PATH");
        assert_eq!(sink.path(), path);
    }
}
