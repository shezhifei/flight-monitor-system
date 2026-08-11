-- Migration: 088_add_ai_copilot_batch_ops_indexes
-- Description: Support operator listing of AI Copilot draft batches by status.

CREATE INDEX IF NOT EXISTS idx_ai_copilot_business_case_batches_status_updated
    ON ai_copilot_business_case_batches (status, updated_at DESC);
