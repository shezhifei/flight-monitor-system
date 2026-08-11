-- Migration: 090_add_ai_copilot_workflow_dispatch_retry_schedule
-- Description: Schedule automatic retries for AI Copilot workflow/notification dispatch failures.

ALTER TABLE ai_copilot_business_case_batches
ADD COLUMN IF NOT EXISTS workflow_dispatch_next_retry_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_ai_copilot_business_case_batches_workflow_dispatch_due
    ON ai_copilot_business_case_batches (workflow_dispatch_next_retry_at ASC, updated_at DESC)
    WHERE workflow_dispatch_status = 'failed'
      AND workflow_dispatch_next_retry_at IS NOT NULL;
