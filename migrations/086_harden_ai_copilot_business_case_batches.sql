-- Migration: 086_harden_ai_copilot_business_case_batches
-- Description: Persist AI Copilot commit outcomes for retry-safe production operations.

ALTER TABLE ai_copilot_business_case_batches
ADD COLUMN IF NOT EXISTS notification_groups JSONB NOT NULL DEFAULT '[]'::jsonb,
ADD COLUMN IF NOT EXISTS commit_error JSONB,
ADD COLUMN IF NOT EXISTS committed_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_ai_copilot_business_case_batches_failed
    ON ai_copilot_business_case_batches (updated_at DESC)
    WHERE status = 'failed';
