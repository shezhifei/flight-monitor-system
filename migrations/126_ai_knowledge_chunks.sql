-- 126_ai_knowledge_chunks.sql
-- Phase K4 (docs/plans/2026-08-18-ai-agent-optimization.md): persist knowledge
-- chunks so the hybrid retriever's keyword search (_search_by_keywords) has a
-- real backing table and index_chunk() can upsert chunks.
-- Style: idempotent (safe to re-run), NO foreign keys (per migration 120
-- policy which dropped all FKs and relies on application-level integrity).
-- Vector backend stays a port: `embedding` is stored as nullable JSONB and is
-- None by default; pgvector/HNSW wiring is deferred until an embedding model
-- is configured.

BEGIN;

CREATE TABLE IF NOT EXISTS ai_knowledge_chunks (
    id          UUID PRIMARY KEY,
    content     TEXT NOT NULL,
    metadata    JSONB NOT NULL DEFAULT '{}'::jsonb,
    source_uri  VARCHAR(500),
    version     INTEGER NOT NULL DEFAULT 1,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    embedding   JSONB  -- optional vector payload (list of floats); NULL = keyword-only
);

-- Full-text search path used by HybridRetriever._search_by_keywords:
-- to_tsvector('simple', content) @@ websearch_to_tsquery('simple', $q)
CREATE INDEX IF NOT EXISTS idx_ai_knowledge_chunks_content_fts
    ON ai_knowledge_chunks USING gin (to_tsvector('simple', content));

CREATE INDEX IF NOT EXISTS idx_ai_knowledge_chunks_source
    ON ai_knowledge_chunks (source_uri);

COMMIT;

-- ============================================================
-- Rollback
-- ============================================================
-- BEGIN;
-- DROP INDEX IF EXISTS idx_ai_knowledge_chunks_source;
-- DROP INDEX IF EXISTS idx_ai_knowledge_chunks_content_fts;
-- DROP TABLE IF EXISTS ai_knowledge_chunks;
-- COMMIT;
