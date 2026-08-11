CREATE TABLE ai_jobs (
    job_id TEXT PRIMARY KEY,
    job_type TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    requester_user_id TEXT,
    ontology_version TEXT,
    context_policy JSONB,
    risk_ceiling TEXT,
    correlation_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    cancelled_at TIMESTAMPTZ,
    error_code TEXT,
    error_message TEXT
);

CREATE TABLE ai_runs (
    run_id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL REFERENCES ai_jobs(job_id) ON DELETE CASCADE,
    runtime_engine TEXT NOT NULL DEFAULT 'python-ai-runtime',
    model_id TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    input_envelope JSONB,
    output_raw JSONB,
    output_validated JSONB,
    token_usage JSONB,
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    error_code TEXT,
    error_message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE ai_run_events (
    event_id BIGSERIAL PRIMARY KEY,
    job_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    payload JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_ai_jobs_status ON ai_jobs(status);
CREATE INDEX idx_ai_jobs_type ON ai_jobs(job_type);
CREATE INDEX idx_ai_jobs_created_at ON ai_jobs(created_at DESC);
CREATE INDEX idx_ai_jobs_correlation_id ON ai_jobs(correlation_id);
CREATE INDEX idx_ai_jobs_requester_user_id ON ai_jobs(requester_user_id);
CREATE INDEX idx_ai_runs_job_id ON ai_runs(job_id);
CREATE INDEX idx_ai_runs_status ON ai_runs(status);
CREATE INDEX idx_ai_runs_created_at ON ai_runs(created_at DESC);
CREATE INDEX idx_ai_run_events_run_id ON ai_run_events(run_id);
CREATE INDEX idx_ai_run_events_job_id ON ai_run_events(job_id);
CREATE INDEX idx_ai_run_events_created_at ON ai_run_events(created_at DESC);
CREATE INDEX idx_ai_run_events_event_type ON ai_run_events(event_type);
