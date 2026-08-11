-- Create AI pending actions table for human approval workflow


CREATE TABLE IF NOT EXISTS ai_pending_actions (
    id SERIAL PRIMARY KEY,
    action_id VARCHAR(64) NOT NULL UNIQUE,
    tool_call_id VARCHAR(64) NOT NULL,
    tool_name VARCHAR(128) NOT NULL,
    arguments TEXT NOT NULL,
    operation_level VARCHAR(64) NOT NULL,
    invocation_mode VARCHAR(64) NOT NULL,
    requester_user_id VARCHAR(255),
    requester_user_roles JSONB NOT NULL DEFAULT '[]'::jsonb,
    reason TEXT NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    approved_by VARCHAR(255),
    approved_at TIMESTAMPTZ,
    rejected_by VARCHAR(255),
    rejected_reason TEXT,
    rejected_at TIMESTAMPTZ,
    execution_result JSONB,
    execution_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT valid_ai_pending_action_status CHECK (
        status IN ('pending', 'approved', 'rejected', 'executed', 'failed', 'expired')
    )
);

CREATE INDEX IF NOT EXISTS idx_ai_pending_actions_status
    ON ai_pending_actions(status);

CREATE INDEX IF NOT EXISTS idx_ai_pending_actions_tool_name
    ON ai_pending_actions(tool_name);

CREATE INDEX IF NOT EXISTS idx_ai_pending_actions_created_at
    ON ai_pending_actions(created_at DESC);

