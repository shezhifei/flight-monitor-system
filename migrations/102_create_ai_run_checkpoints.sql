-- Phase 2: ai_run_checkpoints

CREATE TABLE IF NOT EXISTS ai_run_checkpoints (
    checkpoint_id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL REFERENCES ai_jobs(job_id) ON DELETE CASCADE,
    run_id TEXT NOT NULL REFERENCES ai_runs(run_id) ON DELETE CASCADE,
    sequence_no BIGINT NOT NULL,
    checkpoint_type TEXT NOT NULL,
    tool_call_pk TEXT REFERENCES ai_tool_calls(tool_call_pk),
    proposal_id TEXT REFERENCES ai_action_proposals(proposal_id),
    snapshot_hash TEXT NOT NULL,
    snapshot JSONB NOT NULL,
    snapshot_size_bytes INTEGER NOT NULL DEFAULT 0,
    mq_message_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(run_id, sequence_no)
);

CREATE INDEX IF NOT EXISTS idx_ai_run_checkpoints_run_sequence
    ON ai_run_checkpoints(run_id, sequence_no DESC);
