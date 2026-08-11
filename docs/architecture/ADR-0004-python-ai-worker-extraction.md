# ADR-0004: Python AI Worker Extraction

> **Status:** Accepted  
> **Date:** 2026-04-07  
> **Accepted:** 2026-07-12 (implemented in W2-3)  
> **Deciders:** Architecture team  
> **Consulted:** Backend team, AI/ML team

---

## Context

The current Python application serves both edge HTTP requests (API endpoints) and background AI/ML processing jobs. As Rust takes over the edge-facing runtime, Python should be extracted into an async AI worker that consumes jobs from a queue and writes results back to shared state.

## Decision

**Python loses edge HTTP responsibility.** Rust becomes the sole user-facing request lifecycle handler and SSE publisher. Python transitions to an async worker model.

### Architecture

```
User Request → Rust API → Returns 202 Accepted {job_id}
                         ↓
                    Redis Queue / Job Store
                         ↓
                  Python AI Worker
                         ↓
              Writes result state to Postgres
                         ↓
              Rust publishes result via SSE to user
```

### Job Lifecycle

1. Rust receives AI request at edge
2. Rust validates input, creates job record in Postgres
3. Rust pushes job to Redis queue, returns `202 Accepted` with `job_id`
4. Python worker polls queue, executes AI computation
5. Python writes result state to Postgres (status: completed/failed)
6. Rust SSE publisher reads result state, broadcasts to user
7. Frontend receives result via existing SSE subscription

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
- Push job payload to Redis queue
- Expose job status endpoint: `GET /api/v2/ai/jobs/{job_id}`
- Publish result via SSE when Python writes completion state
- Handle timeout detection (background task or cron)

### Python Side

- Consume jobs from Redis queue
- Execute AI computation
- Write result to Postgres (status: `completed` with result payload, or `failed` with error)
- Handle poison messages (move to dead-letter queue)
- Implement retry with exponential backoff
- Respect timeout_ms and cancel signals

### Shared State

- Postgres table: `ai_jobs` (see schema above)
- Redis queue: `queue:ai_jobs` (LPUSH/RPOP or BRPOP)
- Redis dead-letter: `queue:ai_jobs:dead`

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

Accepted — implemented in W2-3 (2026-07-12).

### Implementation Adaptations

The ADR's original design specified Redis as the job queue transport. The accepted implementation uses **Postgres-based leasing** (SKIP LOCKED + row-level leases) instead of Redis, for the following reasons:

1. **No new infrastructure dependency** — Postgres was already the system of record for job state; using it for leasing avoids operating a separate Redis cluster for the job queue.
2. **Transactional consistency** — Job state transitions and lease acquisition share the same transaction boundary, eliminating dual-write consistency risks.
3. **Two-layer independent lease model** — The Rust control plane leases `ai_jobs` (job lifecycle); the Python execution plane leases `ai_runtime_commands` (run execution). This separation prevents a slow Python worker from blocking Rust's job-level lease management.
4. **SSE via Outbox → CDC → SSE** — Result publication uses the existing domain event outbox and CDC relay infrastructure (ADR-0003), not direct Redis pub/sub.
5. **Python calls Rust ingest API** — Python writes run results back to Rust via authenticated internal HTTP API (ServiceIdentity JWT), not directly to Postgres, ensuring Rust remains the sole writer to the control plane.

See [AI Job Lifecycle](../operations/ai-job-lifecycle.md) for the implemented architecture and operational runbook.

---

## References

- [AI Job Lifecycle](../operations/ai-job-lifecycle.md)
- [ADR-0003: Domain Event Outbox CDC Relay](ADR-0003-domain-event-outbox-cdc-relay.md)
