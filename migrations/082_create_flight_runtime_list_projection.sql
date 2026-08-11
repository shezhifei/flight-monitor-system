
CREATE TABLE IF NOT EXISTS flight_runtime_list_projection (
    flight_id VARCHAR(26) PRIMARY KEY,
    timeline_snapshot JSONB NOT NULL DEFAULT '{}'::jsonb,
    business_cases JSONB NOT NULL DEFAULT '[]'::jsonb,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_flight_runtime_list_projection_updated
    ON flight_runtime_list_projection(updated_at);

CREATE INDEX IF NOT EXISTS idx_fbc_flight_created_desc
    ON flight_business_cases(flight_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_flight_dispatch_timeline_flight_milestone_occurred_desc
    ON flight_dispatch_timeline_events(flight_id, milestone_code, occurred_at DESC, created_at DESC);



