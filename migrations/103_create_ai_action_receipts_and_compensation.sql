-- Phase 3: ai_action_receipts + ai_compensation_plans
-- Durable records for the Phase 3 Compensation + Rollback slice of the
-- AI agent resilient tool architecture. Every executed business action
-- gets a receipt; every reversible receipt gets a compensation plan
-- that drives rollback / correction.

CREATE TABLE IF NOT EXISTS ai_action_receipts (
    receipt_id TEXT PRIMARY KEY,
    proposal_id TEXT NOT NULL REFERENCES ai_action_proposals(proposal_id),
    job_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    tool_call_pk TEXT REFERENCES ai_tool_calls(tool_call_pk),
    object_type TEXT NOT NULL,
    object_id TEXT NOT NULL,
    action_name TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    before_checkpoint_id TEXT REFERENCES ai_run_checkpoints(checkpoint_id),
    after_checkpoint_id TEXT REFERENCES ai_run_checkpoints(checkpoint_id),
    outbox_event_id TEXT,
    execution_result JSONB NOT NULL DEFAULT '{}'::jsonb,
    executed_by TEXT NOT NULL,
    executed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(idempotency_key)
);

CREATE INDEX IF NOT EXISTS idx_ai_action_receipts_proposal
    ON ai_action_receipts(proposal_id);

CREATE INDEX IF NOT EXISTS idx_ai_action_receipts_tool_call
    ON ai_action_receipts(tool_call_pk);

CREATE TABLE IF NOT EXISTS ai_compensation_plans (
    compensation_id TEXT PRIMARY KEY,
    receipt_id TEXT NOT NULL REFERENCES ai_action_receipts(receipt_id),
    proposal_id TEXT NOT NULL REFERENCES ai_action_proposals(proposal_id),
    status TEXT NOT NULL,
    mode TEXT NOT NULL,
    plan JSONB NOT NULL,
    requires_approval BOOLEAN NOT NULL DEFAULT true,
    approved_by TEXT,
    approved_at TIMESTAMPTZ,
    executed_by TEXT,
    executed_at TIMESTAMPTZ,
    execution_result JSONB,
    execution_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(receipt_id, mode)
);

CREATE INDEX IF NOT EXISTS idx_ai_compensation_plans_status_created
    ON ai_compensation_plans(status, created_at);
