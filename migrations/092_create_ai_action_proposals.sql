-- Canonical AI action proposal lifecycle for Rust-owned AIP execution.


CREATE TABLE IF NOT EXISTS ai_action_proposals (
    proposal_id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    ontology_version TEXT NOT NULL,
    object_type TEXT NOT NULL,
    object_id TEXT NOT NULL,
    action_name TEXT NOT NULL,
    arguments JSONB NOT NULL DEFAULT '{}'::jsonb,
    risk_level SMALLINT NOT NULL DEFAULT 0,
    required_permissions JSONB NOT NULL DEFAULT '[]'::jsonb,
    approval_policy SMALLINT NOT NULL DEFAULT 1,
    before_snapshot JSONB,
    after_preview JSONB,
    constraint_results JSONB NOT NULL DEFAULT '[]'::jsonb,
    confidence DOUBLE PRECISION NOT NULL DEFAULT 0,
    reasoning TEXT NOT NULL DEFAULT '',
    status SMALLINT NOT NULL DEFAULT 0,
    pending_action_id TEXT,
    approved_by TEXT,
    approved_at TIMESTAMPTZ,
    rejected_by TEXT,
    rejected_reason TEXT,
    rejected_at TIMESTAMPTZ,
    executed_by TEXT,
    executed_at TIMESTAMPTZ,
    execution_result JSONB,
    execution_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    expires_at TIMESTAMPTZ,
    correlation_id TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX IF NOT EXISTS idx_ai_action_proposals_job
    ON ai_action_proposals(job_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_ai_action_proposals_run
    ON ai_action_proposals(run_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_ai_action_proposals_object
    ON ai_action_proposals(object_type, object_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_ai_action_proposals_status
    ON ai_action_proposals(status, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_ai_action_proposals_pending_action
    ON ai_action_proposals(pending_action_id)
    WHERE pending_action_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_ai_action_proposals_expires
    ON ai_action_proposals(expires_at)
    WHERE status = 2 AND expires_at IS NOT NULL;

