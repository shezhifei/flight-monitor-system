-- W2-3: Add lease/timeout/retry metadata to ai_jobs for async job processing.
-- Mirrors ai_runtime_commands lease columns (migrations 101 + 104) so that
-- ai_jobs can be claimed with a lease, heartbeated by workers, and reaped
-- when the lease expires — independent of the command-level lease.

ALTER TABLE ai_jobs
    ADD COLUMN IF NOT EXISTS timeout_ms BIGINT,
    ADD COLUMN IF NOT EXISTS lease_owner TEXT,
    ADD COLUMN IF NOT EXISTS lease_expires_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS last_heartbeat_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS attempt_count INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS max_attempts INTEGER NOT NULL DEFAULT 1,
    ADD COLUMN IF NOT EXISTS expires_at TIMESTAMPTZ;

-- Reaper: scan claimed/running jobs whose lease has expired.
CREATE INDEX IF NOT EXISTS idx_ai_jobs_lease_expiry
    ON ai_jobs(lease_expires_at)
    WHERE status IN ('claimed', 'running') AND lease_expires_at IS NOT NULL;

-- Reaper fallback: scan running jobs by started_at + timeout_ms (no lease).
CREATE INDEX IF NOT EXISTS idx_ai_jobs_running_started
    ON ai_jobs(started_at)
    WHERE status = 'running' AND started_at IS NOT NULL;
