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

## Delivery validation

After changing rules or contact points, trigger a non-production test alert and
confirm receipt in the configured on-call channel. Repository configuration can
prove rule loading and runbook coverage, but channel delivery remains an
environment-specific operational check.
