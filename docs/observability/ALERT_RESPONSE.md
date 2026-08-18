# SLO Alert Response Runbook

This runbook covers the actionable SLO alerts defined in
`deploy/docker/prometheus-rules/fms-slo-alerts.yml`. Prometheus evaluates the
rules; Alertmanager or Grafana contact points deliver them to the on-call
channel.

## Common first checks

1. Open the FMS overview dashboard and confirm the alert is still firing.
2. Check `up{job="fms-rust-api"}` and the Rust API logs for the same interval.
3. Record the alert start time, affected routes, deployment version, and any
   recent configuration or database changes in the incident ticket.
4. Do not restart services until logs and current metric values are captured.

## FmsApiAvailabilityLow

1. Group `fms_http_requests_total{status=~"5.."}` by route and status to find
   the dominant failure path.
2. Check database, Redis, MQ gateway, and AI-sidecar health only when the failed
   route depends on them.
3. If the alert follows a deployment, use the normal rollback procedure; do not
   bypass migrations or disable authentication to restore traffic.
4. Resolve after availability remains at or above 99.5% for 10 minutes.

## FmsWriteLatencyHigh

1. Compare write latency by route and inspect database pool saturation and slow
   query logs.
2. Check lock contention and outbox publication duration before scaling API
   replicas; scaling does not resolve database contention.
3. Preserve write consistency. Do not bypass optimistic locking or the outbox
   path as a latency mitigation.
4. Resolve after write p99 remains below two seconds for 10 minutes.

## FmsOutboxBacklogHigh

1. Check the outbox relay health, lease owner, retry count, and dead-letter
   metrics/logs.
2. Verify PostgreSQL and the configured downstream transport are reachable.
3. Restart only the unhealthy relay consumer if ownership is stale; do not
   delete pending events or mark them published manually.
4. Escalate when backlog continues to grow for 15 minutes or the oldest pending
   event exceeds the business recovery objective.
5. Resolve after backlog remains below 100 and event age is decreasing.

## FmsAiUngroundedSpike

1. Open the FMS AI Agent dashboard and confirm the `ungrounded` increase panel
   is still climbing; note which `task_type` dominates.
2. Inspect sidecar logs for `EVIDENCE_COVERAGE: ungrounded identifiers` to find
   the affected runs and the identifiers the model invented.
3. Check whether query tools (`ontology.lookup` and the read-only entity
   tools) are erroring or being blocked — invented IDs usually follow tool
   failures, not prompt regressions.
4. Do not loosen the evidence-coverage hook as a mitigation; fix the upstream
   tooling or the template that drives the hallucinated citations.
5. Resolve after the 15-minute ungrounded count stays at or below 10.

## FmsAiSidecarDown

1. Confirm the sidecar process is running and check its health endpoint and
   logs; confirm Prometheus can reach the scrape target network-wise.
2. Check the `/metrics` surface on the sidecar directly before restarting
   anything; capture the current metric values first.
3. Restart only the sidecar if it is unresponsive; runs checkpoint through
   the Rust control plane and resume on the next contact.
4. Resolve after `up{job="fms-ai-sidecar"}` stays at 1 for five minutes.

## FmsAiBudgetExhausted

1. On the FMS AI Agent dashboard, check the run-stop reason panel and the
   tool-call status panel: budget exhaustion means consecutive tool rounds
   failed, so identify the failing tool and its `blocked_by` gate.
2. Inspect sidecar logs for `Consecutive tool failures` warnings and correlate
   with tool errors (lease denials, Rust endpoint failures, schema errors).
3. Fix the underlying tool failure path; do not raise the consecutive-failure
   threshold or the round budget as a first response.
4. Resolve after the budget_exhausted share of run stops stays below 5% for
   15 minutes.

## FmsAiFirstProgressSlow

1. Check the first-progress p95 panel per `task_type` and the LLM call rate
   panel: slow first progress usually means slow first token from the model
   provider or queueing before the run starts.
2. Inspect control-plane latency (lease / checkpoint paths) and sidecar CPU
   before blaming the provider.
3. Do not increase concurrency limits as a latency mitigation; check for
   provider degradation and recent model configuration changes.
4. Resolve after first-progress p95 remains below three seconds for 15
   minutes.

## Delivery validation

After changing rules or contact points, trigger a non-production test alert and
confirm receipt in the configured on-call channel. Repository configuration can
prove rule loading and runbook coverage, but channel delivery remains an
environment-specific operational check.
