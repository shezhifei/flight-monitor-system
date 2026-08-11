CREATE TABLE IF NOT EXISTS runtime_diagnostic_events (
    event_id TEXT PRIMARY KEY,
    topic TEXT NOT NULL,
    event_type TEXT NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_runtime_diagnostic_events_topic_created
    ON runtime_diagnostic_events (topic, created_at DESC);
