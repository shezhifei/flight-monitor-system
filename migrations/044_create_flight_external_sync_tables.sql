CREATE TABLE IF NOT EXISTS flight_sync_runs (
    run_id VARCHAR(26) PRIMARY KEY,
    source_system VARCHAR(64) NOT NULL,
    trigger VARCHAR(16) NOT NULL,
    direction VARCHAR(16) NOT NULL,
    window_start_date DATE NOT NULL,
    window_end_date DATE NOT NULL,
    status VARCHAR(32) NOT NULL,
    processed_count INTEGER NOT NULL DEFAULT 0,
    success_count INTEGER NOT NULL DEFAULT 0,
    failure_count INTEGER NOT NULL DEFAULT 0,
    created_count INTEGER NOT NULL DEFAULT 0,
    updated_count INTEGER NOT NULL DEFAULT 0,
    failure_samples JSONB NOT NULL DEFAULT '[]'::jsonb,
    error_summary JSONB NOT NULL DEFAULT '[]'::jsonb,
    started_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT chk_flight_sync_runs_trigger
        CHECK (trigger IN ('scheduled', 'manual')),
    CONSTRAINT chk_flight_sync_runs_status
        CHECK (status IN ('started', 'completed', 'failed')),
    CONSTRAINT chk_flight_sync_runs_direction
        CHECK (direction IN ('inbound', 'outbound')),
    CONSTRAINT chk_flight_sync_runs_counts
        CHECK (processed_count >= 0 AND success_count >= 0 AND failure_count >= 0 AND created_count >= 0 AND updated_count >= 0)
);

CREATE INDEX IF NOT EXISTS idx_flight_sync_runs_source_started
    ON flight_sync_runs(source_system, started_at DESC);

CREATE TABLE IF NOT EXISTS flight_sync_bindings (
    binding_id VARCHAR(26) PRIMARY KEY,
    source_system VARCHAR(64) NOT NULL,
    natural_key VARCHAR(255) NOT NULL,
    flight_id VARCHAR(26) NOT NULL,
    direction VARCHAR(16) NOT NULL,
    flight_no VARCHAR(16) NOT NULL,
    operation_date DATE NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uq_flight_sync_bindings_source_key UNIQUE (source_system, natural_key),
    CONSTRAINT chk_flight_sync_bindings_direction
        CHECK (direction IN ('inbound', 'outbound'))
);

CREATE INDEX IF NOT EXISTS idx_flight_sync_bindings_flight_id
    ON flight_sync_bindings(flight_id);

CREATE INDEX IF NOT EXISTS idx_flight_sync_bindings_lookup
    ON flight_sync_bindings(source_system, direction, flight_no, operation_date DESC);

CREATE TABLE IF NOT EXISTS flight_sync_snapshots (
    snapshot_id VARCHAR(26) PRIMARY KEY,
    run_id VARCHAR(26) NOT NULL REFERENCES flight_sync_runs(run_id) ON DELETE CASCADE,
    source_system VARCHAR(64) NOT NULL,
    natural_key VARCHAR(255) NOT NULL,
    direction VARCHAR(16) NOT NULL,
    operation_date DATE NOT NULL,
    raw_payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    normalized_payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    processing_result JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT chk_flight_sync_snapshots_direction
        CHECK (direction IN ('inbound', 'outbound'))
);

CREATE INDEX IF NOT EXISTS idx_flight_sync_snapshots_run_id
    ON flight_sync_snapshots(run_id);

CREATE INDEX IF NOT EXISTS idx_flight_sync_snapshots_source_created
    ON flight_sync_snapshots(source_system, created_at DESC);
