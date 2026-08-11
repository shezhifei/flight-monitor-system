-- Migration: 083_create_ai_copilot_business_case_batches
-- Description: Store AI Copilot business-case draft batches for human confirmation.

CREATE TABLE IF NOT EXISTS ai_copilot_business_case_batches (
    batch_id           VARCHAR(64) PRIMARY KEY,
    entity_id          VARCHAR(128) NOT NULL,
    source_page        VARCHAR(128) NOT NULL,
    transcript_summary TEXT NOT NULL DEFAULT '',
    transcript_text    TEXT NOT NULL DEFAULT '',
    draft_actions      JSONB NOT NULL DEFAULT '[]'::jsonb,
    status             VARCHAR(24) NOT NULL DEFAULT 'draft',
    created_by         VARCHAR(128) NOT NULL,
    committed_case_ids TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
    idempotency_key    VARCHAR(128),
    created_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at         TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_ai_copilot_business_case_batches_created_by
    ON ai_copilot_business_case_batches (created_by, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_ai_copilot_business_case_batches_status_expires
    ON ai_copilot_business_case_batches (status, expires_at);

CREATE INDEX IF NOT EXISTS idx_ai_copilot_business_case_batches_idempotency
    ON ai_copilot_business_case_batches (batch_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;
