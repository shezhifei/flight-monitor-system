-- AI experience v2 foundation: approval metadata, KB FTS tables, memory/profile tables


-- ---------------------------------------------------------------------------
-- ai_pending_actions: additive columns and indexes (backward-compatible)
-- ---------------------------------------------------------------------------
ALTER TABLE ai_pending_actions
    ADD COLUMN IF NOT EXISTS risk_level TEXT NOT NULL DEFAULT 'NORMAL',
    ADD COLUMN IF NOT EXISTS entity_type TEXT NULL,
    ADD COLUMN IF NOT EXISTS entity_id TEXT NULL,
    ADD COLUMN IF NOT EXISTS before_snapshot JSONB NULL,
    ADD COLUMN IF NOT EXISTS after_snapshot JSONB NULL,
    ADD COLUMN IF NOT EXISTS json_patch JSONB NULL,
    ADD COLUMN IF NOT EXISTS diff_summary JSONB NULL,
    ADD COLUMN IF NOT EXISTS execution_receipt JSONB NULL,
    ADD COLUMN IF NOT EXISTS status_code TEXT NULL,
    ADD COLUMN IF NOT EXISTS error_payload JSONB NULL,
    ADD COLUMN IF NOT EXISTS correlation_id UUID NULL,
    ADD COLUMN IF NOT EXISTS ui_hints JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS expires_at TIMESTAMPTZ NULL;

CREATE INDEX IF NOT EXISTS idx_ai_pending_actions_status_risk_created
    ON ai_pending_actions(status, risk_level, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_ai_pending_actions_correlation_id
    ON ai_pending_actions(correlation_id);

CREATE INDEX IF NOT EXISTS idx_ai_pending_actions_json_patch_gin
    ON ai_pending_actions USING GIN (json_patch jsonb_path_ops);

CREATE INDEX IF NOT EXISTS idx_ai_pending_actions_diff_summary_gin
    ON ai_pending_actions USING GIN (diff_summary jsonb_path_ops);

CREATE INDEX IF NOT EXISTS idx_ai_pending_actions_expires_at
    ON ai_pending_actions(expires_at)
    WHERE status = 'pending' AND expires_at IS NOT NULL;

-- ---------------------------------------------------------------------------
-- Knowledge base full text tables
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS ai_kb_documents (
    id BIGSERIAL PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    mime_type TEXT NULL,
    title TEXT NULL,
    content_hash TEXT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS ai_kb_chunks (
    id BIGSERIAL PRIMARY KEY,
    document_id BIGINT NOT NULL REFERENCES ai_kb_documents(id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL,
    content TEXT NOT NULL,
    token_count INTEGER NOT NULL DEFAULT 0,
    heading_path TEXT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    content_tsv tsvector NULL,
    CONSTRAINT uq_ai_kb_chunk_doc_chunk UNIQUE (document_id, chunk_index)
);

CREATE INDEX IF NOT EXISTS idx_ai_kb_chunks_content_tsv
    ON ai_kb_chunks USING GIN (content_tsv);

CREATE INDEX IF NOT EXISTS idx_ai_kb_chunks_doc_chunk
    ON ai_kb_chunks(document_id, chunk_index);

CREATE INDEX IF NOT EXISTS idx_ai_kb_documents_path
    ON ai_kb_documents(path);

CREATE INDEX IF NOT EXISTS idx_ai_kb_documents_content_hash
    ON ai_kb_documents(content_hash);

-- ---------------------------------------------------------------------------
-- Conversation memory and user profile tables
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS ai_conversation_memory (
    id BIGSERIAL PRIMARY KEY,
    conversation_id VARCHAR(64) NOT NULL,
    turn_no INTEGER NOT NULL,
    summary TEXT NOT NULL,
    entities JSONB NOT NULL DEFAULT '[]'::jsonb,
    constraints JSONB NOT NULL DEFAULT '{}'::jsonb,
    tool_outcomes JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_ai_conversation_memory_conversation_turn
    ON ai_conversation_memory(conversation_id, turn_no DESC);

CREATE TABLE IF NOT EXISTS ai_user_profile (
    user_id VARCHAR(255) PRIMARY KEY,
    role TEXT NULL,
    timezone TEXT NULL,
    preferences JSONB NOT NULL DEFAULT '{}'::jsonb,
    pinned_metrics JSONB NOT NULL DEFAULT '{}'::jsonb,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

