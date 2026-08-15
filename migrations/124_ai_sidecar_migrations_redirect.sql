-- 124_ai_sidecar_migrations_redirect.sql
-- Redirect to ai-sidecar migrations for knowledge chunks and eval jobs.
-- This file imports schema changes from services/ai-sidecar/migrations/
-- 
-- IMPORTANT: Apply these after running 123 or later in main migration directory.

-- ============================================================================
-- Import 122_ai_knowledge_chunks.sql (Hybrid Retriever Schema)
-- ============================================================================

-- Enable pgvector extension if not already enabled with permission check
DO $$
BEGIN
    IF NOT EXISTS (SELECT FROM pg_extension WHERE extname = 'pgvector') THEN
        -- Check if current user can create extensions
        IF has_schema_privilege('public', 'CREATE') THEN
            CREATE EXTENSION IF NOT EXISTS pgvector;
        ELSE
            RAISE NOTICE 'Cannot create pgvector extension: insufficient permissions';
        END IF;
    END IF;
END
$$;

-- Create knowledge chunks table (mimicking 122_ai_knowledge_chunks.sql)
CREATE TABLE IF NOT EXISTS ai_knowledge_chunks (
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
    
    -- Optional embedding column for vector search
    embedding VECTOR(1536),
    
    -- Full-text search vector
    content_tsvector TSVECTOR,
    
    CONSTRAINT chk_version_positive CHECK (version >= 1)
);

-- Create GIN index for full-text search
CREATE INDEX IF NOT EXISTS idx_knowledge_chunks_content_gin 
    ON ai_knowledge_chunks USING gin(content_tsvector);

-- Create index on source_uri for fast filtering by document type
CREATE INDEX IF NOT EXISTS idx_knowledge_chunks_source_uri 
    ON ai_knowledge_chunks(source_uri);

-- Create index on metadata -> 'document_type' for category filtering
CREATE INDEX IF NOT EXISTS idx_knowledge_chunks_doc_type 
    ON ai_knowledge_chunks USING gin((metadata->>'document_type'));

-- Function to update tsvector on INSERT/UPDATE
CREATE OR REPLACE FUNCTION ai_knowledge_chunks_update_tsvector()
RETURNS TRIGGER AS $$
BEGIN
    NEW.content_tsvector := to_tsvector('simple', NEW.content);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger to auto-populate tsvector
DROP TRIGGER IF EXISTS tsvector_update_trigger ON ai_knowledge_chunks;
CREATE TRIGGER tsvector_update_trigger
    BEFORE INSERT OR UPDATE ON ai_knowledge_chunks
    FOR EACH ROW EXECUTE FUNCTION ai_knowledge_chunks_update_tsvector();

COMMENT ON TABLE ai_knowledge_chunks IS 'Knowledge base chunks for hybrid retrieval (keyword + vector)';
COMMENT ON COLUMN ai_knowledge_chunks.embedding IS 'Optional 1536-dim embedding vector for semantic search';
COMMENT ON COLUMN ai_knowledge_chunks.content_tsvector IS 'Full-text search vector (PostgreSQL tsvector)';

-- ============================================================================
-- Import 123_ai_eval_jobs_persistent.sql (Eval Jobs Schema)
-- ============================================================================

-- Create evaluation jobs table
CREATE TABLE IF NOT EXISTS ai_eval_jobs (
    job_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Basic job metadata
    name VARCHAR(100) NOT NULL,
    description TEXT,
    dataset_path VARCHAR(500),
    
    -- Status lifecycle
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    progress_percent FLOAT NOT NULL DEFAULT 0.0,
    total_runs INTEGER NOT NULL DEFAULT 0,
    completed_runs INTEGER NOT NULL DEFAULT 0,
    
    -- Metrics configuration (gate thresholds)
    metrics_config JSONB NOT NULL DEFAULT '{}',
    
    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    
    -- Error handling
    error_message TEXT,
    
    -- Budget tracking
    total_cost_usd DOUBLE PRECISION DEFAULT 0.0,
    
    CONSTRAINT chk_status_valid CHECK (status IN ('pending', 'running', 'completed', 'failed')),
    CONSTRAINT chk_progress_range CHECK (progress_percent >= 0.0 AND progress_percent <= 100.0),
    CONSTRAINT chk_runs_positive CHECK (total_runs >= 0 AND completed_runs >= 0),
    CONSTRAINT chk_completed_le_total CHECK (completed_runs <= total_runs)
);

CREATE INDEX IF NOT EXISTS idx_eval_jobs_status ON ai_eval_jobs(status);
CREATE INDEX IF NOT EXISTS idx_eval_jobs_created_at ON ai_eval_jobs(created_at DESC);

COMMENT ON TABLE ai_eval_jobs IS 'Persistent evaluation job definitions and status tracking';

-- Evaluation spans table
CREATE TABLE IF NOT EXISTS ai_eval_spans (
    span_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Job and run association
    job_id UUID NOT NULL REFERENCES ai_eval_jobs(job_id) ON DELETE CASCADE,
    run_id VARCHAR(100) NOT NULL,
    
    -- Span metadata
    span_type VARCHAR(50) NOT NULL,
    start_time FLOAT NOT NULL,
    end_time FLOAT NOT NULL,
    
    -- Context and result
    context JSONB NOT NULL DEFAULT '{}',
    result JSONB NOT NULL DEFAULT '{}',
    
    -- Performance metrics
    metrics JSONB NOT NULL DEFAULT '{}',
    
    -- Optional LLM-specific fields
    model_name VARCHAR(100),
    input_tokens INTEGER DEFAULT 0,
    output_tokens INTEGER DEFAULT 0,
    
    -- Error details
    error_message TEXT,
    
    -- Parent-child span relationship
    parent_span_id UUID REFERENCES ai_eval_spans(span_id) ON DELETE SET NULL,
    
    CONSTRAINT chk_span_type_valid CHECK (span_type IN ('llm_call', 'tool_call', 'checkpoint', 'error'))
);

CREATE INDEX IF NOT EXISTS idx_eval_spans_job_id ON ai_eval_spans(job_id);
CREATE INDEX IF NOT EXISTS idx_eval_spans_run_id ON ai_eval_spans(run_id);
CREATE INDEX IF NOT EXISTS idx_eval_spans_span_type ON ai_eval_spans(span_type);
CREATE INDEX IF NOT EXISTS idx_eval_spans_start_time ON ai_eval_spans(start_time);

COMMENT ON TABLE ai_eval_spans IS 'Detailed span traces for each evaluation run';

-- Gate metrics summary table
CREATE TABLE IF NOT EXISTS ai_eval_metrics_summary (
    id SERIAL PRIMARY KEY,
    
    -- Association
    job_id UUID NOT NULL REFERENCES ai_eval_jobs(job_id) ON DELETE CASCADE,
    
    -- Metric definition
    metric_name VARCHAR(100) NOT NULL,
    value FLOAT NOT NULL,
    threshold FLOAT NOT NULL,
    
    -- Pass/fail/warn status
    status VARCHAR(20) NOT NULL,
    details JSONB DEFAULT '{}',
    
    -- Snapshot timestamp
    snapshot_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_eval_summary_job_id ON ai_eval_metrics_summary(job_id);
CREATE INDEX IF NOT EXISTS idx_eval_summary_metric_name ON ai_eval_metrics_summary(metric_name);
CREATE INDEX IF NOT EXISTS idx_eval_summary_status ON ai_eval_metrics_summary(status);

COMMENT ON TABLE ai_eval_metrics_summary IS 'Gate metrics summary for each evaluation job';

-- ============================================================================
-- Migration Complete
-- ============================================================================
COMMENT ON VIEW IF EXISTS ai_migration_redirect IS 'Redirects to ai-sidecar migrations 122+123';
