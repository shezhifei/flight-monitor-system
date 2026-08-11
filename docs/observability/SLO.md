# Service Level Objectives (SLOs) — Flight Monitor System

This document defines the SLOs, SLIs, error budgets, and alerting thresholds for the
Flight Monitor System (FMS). SLIs are derived from the Prometheus metrics scraped from the
Rust API (`fms-rust-api`), the Python AI sidecar (`fms-ai-sidecar`), and Vault
(`fms-vault`). Dashboards: `deploy/docker/grafana/dashboards/fms-overview.json`.
Versioned Prometheus rules: `deploy/docker/prometheus-rules/fms-slo-alerts.yml`.
Response procedures: `docs/observability/ALERT_RESPONSE.md`.

## Summary

| Objective | SLO target | Window | Error budget |
|-----------|------------|--------|--------------|
| API availability | ≥ 99.5% | 30 days | 0.5% (3.6h/mo) |
| Read p99 latency | < 500 ms | 30 days | 0.5% of requests over 500 ms |
| Write p99 latency | < 2 s | 30 days | 0.5% of requests over 2 s |
| MQ end-to-end latency | < 5 s | 30 days | 0.5% of events slower than 5 s |
| Outbox backlog | < 100 pending events | rolling | breach when backlog ≥ 100 |

## SLI definitions

All SLIs assume the metric `fms_http_requests_total` is a counter labelled at least by
`route` and `status` (HTTP status code), and `fms_http_request_duration_seconds` is a
histogram with bucket label `le`.

### 1. API availability

SLI = 1 − (rate of 5xx responses / rate of all responses)

```
availability =
  1 - (
    sum(rate(fms_http_requests_total{status=~"5.."}[5m]))
    /
    sum(rate(fms_http_requests_total[5m]))
  )
```

- **Good** event = any non-5xx response.
- **Valid** event = any response.
- SLO: `availability >= 0.995`.

### 2. Read latency (GET, non-mutating routes)

SLI = share of read requests served faster than 500 ms (p99 < 500 ms).

```
read_p99 = histogram_quantile(
  0.99,
  sum(rate(fms_http_request_duration_seconds_bucket{route=~"GET .*"}[5m])) by (le)
)
```

- SLO: `read_p99 < 0.5` seconds.

### 3. Write latency (POST/PUT/DELETE, mutating routes)

```
write_p99 = histogram_quantile(
  0.99,
  sum(rate(fms_http_request_duration_seconds_bucket{route=~"POST .*|PUT .*|DELETE .*"}[5m])) by (le)
)
```

- SLO: `write_p99 < 2` seconds.

### 4. MQ end-to-end latency

SLI = share of messages consumed within 5 s of being published.
Use the consume-vs-publish gap derived from `fms_mq_publish_total` and
`fms_mq_consume_total`, or the broker-side delivery latency histogram if available.

```
mq_publish_rate = sum(rate(fms_mq_publish_total[5m])) by (topic)
mq_consume_rate = sum(rate(fms_mq_consume_total[5m])) by (topic)
# end-to-end latency is observed on the consume side; alert when backlog grows.
e2e_ok = (mq_consume_rate / mq_publish_rate) >= 0.999
```

- SLO: end-to-end latency p99 < 5 s (observed by consumer-side timing); sustained
  consume rate should track publish rate within 0.1%.

### 5. Outbox backlog

SLI = current number of unpublished outbox events.

```
outbox_backlog = sum(fms_outbox_pending_events)
```

- SLO: `outbox_backlog < 100`.
- Also monitor publish duration: `histogram_quantile(0.99, sum(rate(fms_outbox_publish_duration_seconds_bucket[5m])) by (le))`.

### 6. AI runtime (supporting, no hard SLO)

Health indicators only — not tied to an availability budget:

```
fms_ai_llm_calls_total     # LLM invocation count (by model/status)
fms_ai_tool_calls_total    # tool/function execution count (by tool/status)
fms_ai_mq_gate_decisions_total  # MQ-gate allow/deny decisions
```

Surface anomalies (error rate of calls) but no formal SLO.

## Error budgets

For a 30-day window and 99.5% availability, allowable downtime:

- Availability 99.5% → 0.5% × 43,200 min = **216 minutes (3.6 h)** per 30 days.

Burn-rate alerts (multi-window, multi-burn-rate):

- **Fast burn** (14.4×, ~30 min exhaustion): `1 - availability` over 1h > 0.072
  (≈ budget consumes in ~14h). Page on-call.
- **Slow burn** (6×, ~2% per 6h): `1 - availability` over 6h > 0.03. Ticket, no page.

## Alerting thresholds

| Alert | Expr (PromQL) | For | Severity |
|-------|---------------|-----|----------|
| FmsApiAvailabilityLow | `1 - (sum(rate(fms_http_requests_total{status=~"5.."}[5m])) / sum(rate(fms_http_requests_total[5m]))) < 0.995` | 10m | critical |
| FmsReadLatencyHigh | `histogram_quantile(0.99, sum(rate(fms_http_request_duration_seconds_bucket{route=~"GET .*"}[5m])) by (le)) > 0.5` | 10m | warning |
| FmsWriteLatencyHigh | `histogram_quantile(0.99, sum(rate(fms_http_request_duration_seconds_bucket{route=~"POST .*|PUT .*|DELETE .*"}[5m])) by (le)) > 2` | 10m | warning |
| FmsMqLag | `sum(rate(fms_mq_publish_total[5m])) - sum(rate(fms_mq_consume_total[5m])) > 5` | 5m | warning |
| FmsOutboxBacklogHigh | `sum(fms_outbox_pending_events) >= 100` | 5m | critical |
| FmsOutboxPublishSlow | `histogram_quantile(0.99, sum(rate(fms_outbox_publish_duration_seconds_bucket[5m])) by (le)) > 1` | 10m | warning |
| FmsAiSidecarDown | `up{job="fms-ai-sidecar"} == 0` | 2m | warning |
| FmsRustApiDown | `up{job="fms-rust-api"} == 0` | 2m | critical |

## Notes

- Scrape jobs and targets are defined in `deploy/docker/prometheus.yml`.
- The repository-owned availability, write-latency, and outbox-backlog rules are
  loaded by `deploy/docker/docker-compose.observability.yml`. Alert delivery is
  configured per environment through Alertmanager or Grafana contact points;
  validate the channel with the runbook's non-production test procedure.
- Vault metrics (`fms-vault`) are optional and scraped from
  `vault:8200/v1/sys/metrics?format=prometheus` with `honor_labels: true`.
- Metrics flow: `fms_http_requests_total`, `fms_http_request_duration_seconds`,
  `fms_db_pool_connections`, `fms_redis_commands_total`, `fms_mq_publish_total`,
  `fms_mq_consume_total`, `fms_ai_llm_calls_total`, `fms_ai_tool_calls_total`,
  `fms_ai_mq_gate_decisions_total`, `fms_outbox_pending_events`,
  `fms_outbox_publish_duration_seconds`.
