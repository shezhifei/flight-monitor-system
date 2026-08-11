-- Migration: 091_add_ai_copilot_commit_saga_recovery
-- Description: Persist AI Copilot commit saga state for durable recovery.

ALTER TABLE ai_copilot_business_case_batches
ADD COLUMN IF NOT EXISTS commit_request JSONB,
ADD COLUMN IF NOT EXISTS created_action_case_ids JSONB NOT NULL DEFAULT '{}'::jsonb,
ADD COLUMN IF NOT EXISTS commit_started_at TIMESTAMPTZ,
ADD COLUMN IF NOT EXISTS commit_attempts INTEGER NOT NULL DEFAULT 0,
ADD COLUMN IF NOT EXISTS commit_next_recovery_at TIMESTAMPTZ;

UPDATE ai_copilot_business_case_batches
SET created_action_case_ids = '{}'::jsonb
WHERE created_action_case_ids IS NULL
   OR jsonb_typeof(created_action_case_ids) <> 'object';

UPDATE ai_copilot_business_case_batches
SET commit_attempts = 0
WHERE commit_attempts IS NULL;

ALTER TABLE ai_copilot_business_case_batches
ALTER COLUMN created_action_case_ids SET DEFAULT '{}'::jsonb,
ALTER COLUMN created_action_case_ids SET NOT NULL,
ALTER COLUMN commit_attempts SET DEFAULT 0,
ALTER COLUMN commit_attempts SET NOT NULL;

ALTER TABLE ai_copilot_business_case_batches
DROP CONSTRAINT IF EXISTS chk_ai_copilot_action_case_ids_object;

ALTER TABLE ai_copilot_business_case_batches
ADD CONSTRAINT chk_ai_copilot_action_case_ids_object
    CHECK (jsonb_typeof(created_action_case_ids) = 'object');

DROP INDEX IF EXISTS idx_ai_copilot_business_case_batches_commit_recovery_due;

CREATE INDEX idx_ai_copilot_business_case_batches_commit_recovery_due
    ON ai_copilot_business_case_batches (
        COALESCE(commit_next_recovery_at, commit_started_at),
        commit_started_at
    )
    WHERE status = 'committing'
      AND commit_started_at IS NOT NULL;
