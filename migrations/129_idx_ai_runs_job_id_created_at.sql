-- no-transaction
-- Speed up ai_runs grouped by job_id and ordered by created_at DESC.

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ai_runs_job_id_created_at
    ON ai_runs (job_id, created_at DESC);
