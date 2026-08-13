# ADR-0004: Python AI Worker Extraction

> **Status:** Accepted  
> **Date:** 2026-04-07  
> **Accepted:** 2026-07-12 (implemented in W2-3)  
> **Deciders:** Architecture team  
> **Consulted:** Backend team, AI/ML team

---

## Context

The edge-runtime extraction is complete. Rust owns the public HTTP lifecycle, control-plane job state, proposal ingestion, and SSE publication. Python is an internal AI runtime behind Service Identity JWT: it receives run envelopes, performs LLM/tool/MCP work, and returns structured results. The implemented transport is Postgres leasing for `ai_jobs` and `ai_runtime_commands`; Python does not write control-plane or business truth tables directly.

## Decision

**Python has no edge HTTP responsibility.** Rust is the sole user-facing request lifecycle handler and SSE publisher. Python remains an internal AI runtime behind the authenticated runtime API and Postgres lease protocol.

### Architecture

```
User Request → Rust API → ai_jobs / runtime-command lease
                         ↓
                  Python AI Runtime
                         ↓
              Rust ingest / proposal pipeline
                         ↓
              Rust publishes result via SSE
```

### Job Lifecycle

1. Rust receives and validates the AI request at the edge.
2. Rust creates the `ai_jobs` record and leases the runtime command from Postgres.
3. Rust calls the authenticated Python `/internal/ai/v1/runs` or streaming endpoint.
4. Python executes AI computation and returns structured output or a terminal error.
5. Rust ingests proposals and completes job/run state in the control plane.
6. Rust SSE/outbox publication broadcasts the result to the frontend.

### Consequences

**Positive:**
- Rust owns request lifecycle end-to-end
- Python focuses on what it does best: AI/ML computation
- No blocking long-running requests at the edge
- Natural backpressure via queue depth
- Clear failure boundaries

**Negative:**
- Added latency from async processing
- Job state management complexity
- Need retry/dead-letter handling
- Frontend must poll or subscribe for results

---

## Job Model Specification

### Job Record Schema

```sql
CREATE TABLE ai_jobs (
    job_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    task_type VARCHAR(100) NOT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    payload JSONB NOT NULL,
    result JSONB,
    error_code VARCHAR(50),
    error_message TEXT,
    retry_count INTEGER DEFAULT 0,
    max_retries INTEGER DEFAULT 3,
    timeout_ms INTEGER DEFAULT 300000,
    priority INTEGER DEFAULT 0,
    user_id VARCHAR(100),
    correlation_id VARCHAR(100),
    created_at TIMESTAMPTZ DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    dead_letter BOOLEAN DEFAULT FALSE,
    dead_letter_reason TEXT
);

CREATE INDEX idx_ai_jobs_status ON ai_jobs(status) WHERE status IN ('pending', 'running');
CREATE INDEX idx_ai_jobs_created ON ai_jobs(created_at);
CREATE INDEX idx_ai_jobs_user ON ai_jobs(user_id);
```

### Job Status State Machine

```
                    ┌─────────────┐
                    │   pending   │
                    └──────┬──────┘
                           │ worker picks up
                           ▼
                    ┌─────────────┐
           timeout  │   running   │
          ┌─────────┴──────┬──────┘
          │                │ success
          ▼                ▼
   ┌─────────────┐  ┌─────────────┐
   │  timed_out  │  │  succeeded  │
   └──────┬──────┘  └─────────────┘
          │
          │ retry
          ▼
   ┌─────────────┐
   │   pending   │ (retry_count++)
   └──────┬──────┘
          │ max retries
          ▼
   ┌─────────────┐
   │ dead_letter │
   └─────────────┘

   Also: cancelled (user-initiated)
```

### Valid Status Values

| Status | Description | Transitions From |
|--------|-------------|------------------|
| `pending` | Job queued, awaiting worker | initial |
| `running` | Worker processing | pending |
| `succeeded` | Job completed successfully | running |
| `failed` | Job failed (retryable) | running |
| `timed_out` | Job exceeded timeout | running |
| `cancelled` | User cancelled job | pending, running |
| `dead_letter` | Job failed after max retries | failed, timed_out |

---

## Implementation Requirements

### Rust Side

- Accept AI requests, validate, return `202 Accepted`
- Persist job with status `pending` in Postgres
- Acquire and advance Postgres-backed job/runtime-command leases through application ports
- Expose job status endpoint: `GET /api/v2/ai/jobs/{job_id}`
- Ingest Python results through the authenticated Rust runtime contract
- Publish committed result state via outbox/CDC/SSE
- Handle timeout detection (background task or cron)

### Python Side

- Accept only authenticated internal runtime requests from Rust.
- Execute AI computation and return structured output.
- Do not write `ai_jobs`, `ai_runtime_commands`, or business truth tables directly.
- Respect runtime timeout, cancellation, and fail-closed tool authorization signals.

### Shared State

- Postgres tables: `ai_jobs` and `ai_runtime_commands`.
- Lease ownership and status transitions remain in Rust/application ports.
- Result publication uses the domain event outbox/CDC path.

---

## Failure Mode Handling

### Duplicate Delivery

- Jobs are idempotent by design
- Worker checks current status before processing
- Use `UPDATE ... WHERE status = 'pending'` for atomic transition

### Worker Crash

- Jobs in `running` state without heartbeat for > timeout are reset to `pending`
- Background reaper task in Rust resets orphaned jobs

### Partial DB Write

- Use transactions for job state updates
- Result payload written atomically with status change

### Callback Publication Failure

- Job result is persisted to DB first
- SSE publication is best-effort; frontend can poll status endpoint
- Outbox pattern ensures eventual consistency

### Stale Frontend Polling

- Jobs have `expires_at` for TTL
- Frontend should stop polling after expiry
- Expired jobs are cleaned up by background task

---

## Task Types

| Task Type | Description | Timeout | Max Retries |
|-----------|-------------|---------|-------------|
| `nl_query` | Natural language query processing | 60s | 3 |
| `ai_analysis` | AI-powered flight analysis | 120s | 2 |
| `dispatch_replan` | Dispatch schedule optimization | 300s | 1 |
| `report_generation` | Async report generation | 180s | 2 |

---

## API Contract

### Submit Job

```
POST /api/v2/ai/jobs
Content-Type: application/json

{
    "task_type": "nl_query",
    "payload": {"query": "show me delayed flights"},
    "timeout_ms": 60000
}

Response: 202 Accepted
{
    "success": true,
    "data": {
        "job_id": "uuid-here",
        "status": "pending",
        "created_at": "2026-04-07T12:00:00Z"
    }
}
```

### Check Job Status

```
GET /api/v2/ai/jobs/{job_id}

Response: 200 OK
{
    "success": true,
    "data": {
        "job_id": "uuid-here",
        "status": "succeeded",
        "result": {"flights": [...]},
        "finished_at": "2026-04-07T12:00:05Z"
    }
}
```

### Cancel Job

```
DELETE /api/v2/ai/jobs/{job_id}

Response: 200 OK
{
    "success": true,
    "data": {
        "job_id": "uuid-here",
        "status": "cancelled"
    }
}
```

---

## Status

Accepted — implemented and superseded by the current Postgres lease + internal AI runtime design (2026-08-13).

### Implementation Adaptations

The original proposal specified Redis and direct Python result writes. The implemented design uses **Postgres-based leasing** (SKIP LOCKED + row-level leases) and Rust-owned ingestion:

1. **No new infrastructure dependency** — Postgres was already the system of record for job state; using it for leasing avoids operating a separate Redis cluster for the job queue.
2. **Transactional consistency** — Job state transitions and lease acquisition share the same transaction boundary, eliminating dual-write consistency risks.
3. **Two-layer independent lease model** — The Rust control plane leases `ai_jobs` (job lifecycle); the Python execution plane leases `ai_runtime_commands` (run execution). This separation prevents a slow Python worker from blocking Rust's job-level lease management.
4. **SSE via Outbox → CDC → SSE** — Result publication uses the existing domain event outbox and CDC relay infrastructure (ADR-0003), not direct Redis pub/sub.
5. **Python calls the Rust-owned runtime contract** — Python returns run results to Rust via authenticated internal HTTP API (ServiceIdentity JWT), not directly to Postgres, ensuring Rust remains the sole writer to the control plane.

See [AI Job Lifecycle](../operations/ai-job-lifecycle.md) for the implemented architecture and operational runbook.

---

## References

- [AI Job Lifecycle](../operations/ai-job-lifecycle.md)
- [ADR-0003: Domain Event Outbox CDC Relay](ADR-0003-domain-event-outbox-cdc-relay.md)
