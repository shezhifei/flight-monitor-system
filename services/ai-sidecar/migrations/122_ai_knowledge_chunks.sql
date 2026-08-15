-- 122_ai_knowledge_chunks.sql
-- Knowledge base chunk storage for hybrid retriever.
-- Supports both keyword search (ts_vector) and optional vector similarity.

CREATE TABLE ai_knowledge_chunks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Content is the main searchable text
    content TEXT NOT NULL,
    
    -- Metadata stores document type, source URI, versioning
    metadata JSONB NOT NULL DEFAULT '{}',
    source_uri VARCHAR(500),
    version INTEGER NOT NULL DEFAULT 1,
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Optional embedding column for vector search (Redis HNSW or ChromaDB)
    embedding VECTOR(1536),
    
    -- Full-text search index (ts_vector automatically generated)
    content_tsvector TSVECTOR,
    
    CONSTRAINT chk_version_positive CHECK (version >= 1)
);

-- Create GIN index for full-text search
CREATE INDEX idx_knowledge_chunks_content_gin 
    ON ai_knowledge_chunks USING gin(content_tsvector);

-- Create index on source_uri for fast filtering by document type
CREATE INDEX idx_knowledge_chunks_source_uri 
    ON ai_knowledge_chunks(source_uri);

-- Create index on metadata -> 'document_type' for category filtering
CREATE INDEX idx_knowledge_chunks_doc_type 
    ON ai_knowledge_chunks USING gin((metadata->>'document_type'));

-- Add computed column for ts_vector (automatically maintained via trigger)
ALTER TABLE ai_knowledge_chunks 
ADD COLUMN IF NOT EXISTS content_searchable TSVECTOR;

-- Function to update tsvector on INSERT/UPDATE
CREATE OR REPLACE FUNCTION ai_knowledge_chunks_update_tsvector()
RETURNS TRIGGER AS $$
BEGIN
    NEW.content_searchable := to_tsvector('simple', NEW.content);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger to auto-populate tsvector
DROP TRIGGER IF EXISTS tsvector_update_trigger ON ai_knowledge_chunks;
CREATE TRIGGER tsvector_update_trigger
    BEFORE INSERT OR UPDATE ON ai_knowledge_chunks
    FOR EACH ROW EXECUTE FUNCTION ai_knowledge_chunks_update_tsvector();

-- Enable pgvector extension if not already enabled with permission check
DO $$
BEGIN
    IF NOT EXISTS (SELECT FROM pg_extension WHERE extname = 'pgvector') THEN
        -- Check if current user can create extensions
        IF has_schema_privilege('public', 'CREATE') THEN
            CREATE EXTENSION pgvector;
        ELSE
            RAISE NOTICE 'Cannot create pgvector extension: insufficient permissions';
        END IF;
    END IF;
END
$$;

-- Comment
COMMENT ON TABLE ai_knowledge_chunks IS 'Knowledge base chunks for hybrid retrieval (keyword + vector)';
COMMENT ON COLUMN ai_knowledge_chunks.embedding IS 'Optional 1536-dim embedding vector for semantic search (e.g., sentence-transformers output)';
COMMENT ON COLUMN ai_knowledge_chunks.content_tsvector IS 'Full-text search vector (PostgreSQL tsvector) for BM25-like keyword matching';
