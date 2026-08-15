-- 123_ai_eval_jobs_persistent.sql
-- Persistent evaluation job storage and metrics tracking.

CREATE TABLE ai_eval_jobs (
    job_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Basic job metadata
    name VARCHAR(100) NOT NULL,
    description TEXT,
    dataset_path VARCHAR(500),
    
    -- Status lifecycle
    status VARCHAR(20) NOT NULL DEFAULT 'pending',  -- pending | running | completed | failed
    progress_percent FLOAT NOT NULL DEFAULT 0.0,
    total_runs INTEGER NOT NULL DEFAULT 0,
    completed_runs INTEGER NOT NULL DEFAULT 0,
    
    -- Metrics configuration (gate thresholds)
    metrics_config JSONB NOT NULL DEFAULT '{}',
    -- {
    --   "tool_accuracy_min": 0.95,
    --   "hallucination_rate_max": 0.05,
    --   "zero_violations_required": true,
    --   "avg_rounds_target": 8,
    --   "plan_board_compliance_min": 0.90
    -- },
    
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

CREATE INDEX idx_eval_jobs_status ON ai_eval_jobs(status);
CREATE INDEX idx_eval_jobs_created_at ON ai_eval_jobs(created_at DESC);

COMMENT ON TABLE ai_eval_jobs IS 'Persistent evaluation job definitions and status tracking';

-- Evaluation spans table (detailed trace per run)
CREATE TABLE ai_eval_spans (
    span_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- Job and run association
    job_id UUID NOT NULL REFERENCES ai_eval_jobs(job_id) ON DELETE CASCADE,
    run_id VARCHAR(100) NOT NULL,  -- Unique identifier for this test execution
    
    -- Span metadata
    span_type VARCHAR(50) NOT NULL,  -- llm_call | tool_call | checkpoint | error
    start_time FLOAT NOT NULL,  -- Unix timestamp
    end_time FLOAT NOT NULL,
    
    -- Context (JSON blob of inputs/prompts)
    context JSONB NOT NULL DEFAULT '{}',
    
    -- Result (JSON blob of outputs)
    result JSONB NOT NULL DEFAULT '{}',
    
    -- Performance metrics
    metrics JSONB NOT NULL DEFAULT '{}',
    -- {
    --   "tokens_used": {"input": 2500, "output": 450},
    --   "duration_ms": 5500,
    --   "success": true,
    --   "cost_usd": 0.012
    -- },
    
    -- Optional LLM-specific fields
    model_name VARCHAR(100),
    input_tokens INTEGER DEFAULT 0,
    output_tokens INTEGER DEFAULT 0,
    
    -- Cost tracking
    total_cost_usd DOUBLE PRECISION DEFAULT 0.0,
    
    -- Error details (if any)
    error_message TEXT,
    
    -- Parent-child span relationship
    parent_span_id UUID REFERENCES ai_eval_spans(span_id) ON DELETE SET NULL,
    
    CONSTRAINT chk_span_type_valid CHECK (span_type IN ('llm_call', 'tool_call', 'checkpoint', 'error'))
);

CREATE INDEX idx_eval_spans_job_id ON ai_eval_spans(job_id);
CREATE INDEX idx_eval_spans_run_id ON ai_eval_spans(run_id);
CREATE INDEX idx_eval_spans_span_type ON ai_eval_spans(span_type);
CREATE INDEX idx_eval_spans_start_time ON ai_eval_spans(start_time);

COMMENT ON TABLE ai_eval_spans IS 'Detailed span traces for each evaluation run (model calls, tool usage, checkpoints)';

-- Gate metrics summary (final results per job)
CREATE TABLE ai_eval_metrics_summary (
    id SERIAL PRIMARY KEY,
    
    -- Association
    job_id UUID NOT NULL REFERENCES ai_eval_jobs(job_id) ON DELETE CASCADE,
    metric_name VARCHAR(100) NOT NULL,
    value FLOAT NOT NULL,
    threshold FLOAT NOT NULL,
    
    -- Pass/fail/warn status
    status VARCHAR(20) NOT NULL,  -- pass | fail | warn
    details JSONB DEFAULT '{}',   -- Additional context
    
    -- Snapshot timestamp
    snapshot_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_eval_summary_job_id ON ai_eval_metrics_summary(job_id);
CREATE INDEX idx_eval_summary_metric_name ON ai_eval_metrics_summary(metric_name);
CREATE INDEX idx_eval_summary_status ON ai_eval_metrics_summary(status);

COMMENT ON TABLE ai_eval_metrics_summary IS 'Gate metrics summary for each evaluation job (pass/fail/warn)';

-- Sample data (for testing - can be removed in production)
INSERT INTO ai_eval_jobs (name, dataset_path, metrics_config, status)
VALUES (
    'Dispatch Ops Baseline',
    'eval/datasets/dispatch_tests.jsonl',
    '{"tool_accuracy_min": 0.95, "hallucination_rate_max": 0.05, "zero_violations_required": true, "avg_rounds_target": 8, "plan_board_compliance_min": 0.90}'::jsonb,
    'pending'
) ON CONFLICT DO NOTHING;
