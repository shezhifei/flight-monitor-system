-- Phase 1: ai_runtime_commands + ai_tool_calls
-- Durable execution control plane: command queue (Rust -> Python) and
-- per-tool-call ledger. Checkpoint / receipt / compensation tables are
-- added by later phases.

CREATE TABLE IF NOT EXISTS ai_runtime_commands (
    command_id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES ai_runs(run_id) ON DELETE CASCADE,
    command_type TEXT NOT NULL,
    command_sequence BIGINT NOT NULL DEFAULT 0,
    tool_call_pk TEXT,
    payload JSONB NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    run_owner TEXT,
    lease_owner TEXT,
    lease_expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    processed_at TIMESTAMPTZ,
    UNIQUE(run_id, command_sequence)
);

CREATE INDEX IF NOT EXISTS idx_ai_commands_pending
    ON ai_runtime_commands(status, created_at)
    WHERE status = 'pending';

CREATE INDEX IF NOT EXISTS idx_ai_commands_run
    ON ai_runtime_commands(run_id, created_at);

CREATE TABLE IF NOT EXISTS ai_tool_calls (
    tool_call_pk TEXT PRIMARY KEY,
    job_id TEXT NOT NULL REFERENCES ai_jobs(job_id) ON DELETE CASCADE,
    run_id TEXT NOT NULL REFERENCES ai_runs(run_id) ON DELETE CASCADE,
    parent_tool_call_pk TEXT REFERENCES ai_tool_calls(tool_call_pk),
    root_tool_call_pk TEXT,
    depth INTEGER NOT NULL DEFAULT 0,
    round_index INTEGER NOT NULL DEFAULT 0,
    tool_call_id TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    tool_type TEXT NOT NULL,
    status TEXT NOT NULL,
    args_hash TEXT NOT NULL,
    args_summary JSONB NOT NULL DEFAULT '{}'::jsonb,
    result_hash TEXT,
    result_summary JSONB,
    error_code TEXT,
    error_message TEXT,
    retry_count INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 2,
    timeout_seconds INTEGER NOT NULL DEFAULT 30,
    last_heartbeat_at TIMESTAMPTZ,
    idempotency_key TEXT NOT NULL,
    mq_message_id TEXT,
    mq_offset BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    UNIQUE(run_id, idempotency_key)
);

CREATE INDEX IF NOT EXISTS idx_ai_tool_calls_run_status
    ON ai_tool_calls(run_id, status, created_at);

CREATE INDEX IF NOT EXISTS idx_ai_tool_calls_heartbeat
    ON ai_tool_calls(status, last_heartbeat_at);
