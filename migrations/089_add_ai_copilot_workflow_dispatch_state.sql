-- Migration: 089_add_ai_copilot_workflow_dispatch_state
-- Description: Track AI Copilot workflow/notification dispatch separately from business-case creation.

ALTER TABLE ai_copilot_business_case_batches
ADD COLUMN IF NOT EXISTS workflow_dispatch_status VARCHAR(32) NOT NULL DEFAULT 'not_required',
ADD COLUMN IF NOT EXISTS workflow_dispatch_request JSONB,
ADD COLUMN IF NOT EXISTS workflow_dispatch_error JSONB,
ADD COLUMN IF NOT EXISTS workflow_dispatch_attempts INTEGER NOT NULL DEFAULT 0,
ADD COLUMN IF NOT EXISTS workflow_dispatched_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_ai_copilot_business_case_batches_workflow_dispatch
    ON ai_copilot_business_case_batches (workflow_dispatch_status, updated_at DESC)
    WHERE workflow_dispatch_status IN ('pending', 'failed');
