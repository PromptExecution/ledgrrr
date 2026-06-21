# Rotel OTel Journal Surface

l3dg3rr owns typed telemetry semantics before data reaches a collector.
Rotel is the OpenTelemetry collector boundary; the Rust core models
OTel object-shape polyfills, classifies log shapes, and emits deterministic
journal artifacts that justify metric triggers.

There are two layers:

1. **`rotel-visual` service** (port `ROTEL_PORT`, default `4318`) — the real
   OTLP ingestion, log classification, ring buffer, and WebSocket dashboard.
2. **Host proxy** (port `15115`) — the ledgerr-host internal gateway forwards
   OTLP signal requests to `rotel-visual` for real classification instead of
   stubbing.

## Contract

- `OTelLogRecord`, `OTelMetric`, and `OTelSpan` polyfill the log, metric, and
  trace object shapes l3dg3rr needs internally.
- `LogShapeClassifier` maps abstract regex types to classified journal
  artifacts.
- `ClassifiedJournalArtifact` records the matched excerpt, rule id,
  evidence digest, metric name, and metric delta.
- `TelemetryArrowBatch` provides the stable column contract for the embedded
  Rotel/OTel-Arrow classification path.
- `RotelExportPlan` keeps standard OTLP HTTP endpoints and the Arrow connector
  intent explicit.

## Internal Listener

The ledgerr-host internal gateway (port `15115`) proxies OTLP signal requests
to the `rotel-visual` service (port `ROTEL_PORT`, default `4318`):

```text
GET  http://127.0.0.1:15115/rotel/health
GET  http://127.0.0.1:15115/rotel/export-plan
POST http://127.0.0.1:15115/v1/logs       → forwarded to rotel-visual
POST http://127.0.0.1:15115/v1/metrics    → forwarded to rotel-visual
POST http://127.0.0.1:15115/v1/traces     → forwarded to rotel-visual
```

The rotel-visual service also exposes its own endpoints directly:

```text
GET  http://127.0.0.1:{ROTEL_PORT}/health
GET  http://127.0.0.1:{ROTEL_PORT}/metrics
GET  http://127.0.0.1:{ROTEL_PORT}/
WS   ws://127.0.0.1:{ROTEL_PORT}/ws/telemetry
POST http://127.0.0.1:{ROTEL_PORT}/v1/logs
POST http://127.0.0.1:{ROTEL_PORT}/v1/metrics
POST http://127.0.0.1:{ROTEL_PORT}/v1/traces
POST http://127.0.0.1:{ROTEL_PORT}/rotel/evaluate
```

The `/v1/*` OTLP paths accept JSON payloads and return the rotel-visual
classification artifact. The gateway still owns OpenAI chat at
`/v1/chat/completions` on port `15115`; Rotel proxying is additive on the
internal listener.

## Example Rule

```text
rule_id: gpu-driver-device-disappeared
abstract_regex_type: hardware.gpu.driver.device_handle_unknown
pattern: Unable to determine the device handle for GPU[0-9]+.*Unknown Error
metric_name: l3dg3rr.hardware.gpu.driver_faults
metric_delta: 1
```

When a matching error log is observed, l3dg3rr creates a classified journal
artifact. That artifact is the justification for incrementing the metric; the
metric is not triggered by raw text alone.

## Arrow Columns

The initial classification batch shape is:

```text
artifact_id
signal
abstract_regex_type
metric_name
metric_delta
severity_text
matched_excerpt
justification_digest
source_time_unix_nano
```

The `otel-arrow` Cargo feature enables an Apache Arrow schema for this batch
without forcing Arrow into the default build.

## Build Gate Pattern

The observable build-gate path reuses existing b00t interface types:

- `MetricRegistry` / `MetricValue` expose the visual SLI state.
- `SarifLog` / `LintRule` / `SarifResult` emit the build-gate SLO result.
- `datum::logic` tokenization plus NAND/NOR implements `&&` and `||`.

Example:

```text
log_shape && metric
```

If the classified log shape is present but the expected metric is missing,
the gate records visible metrics under `rotel-otel:<gate>` and emits SARIF
rule `l3dg3rr/otel/build-gate-slo`.
