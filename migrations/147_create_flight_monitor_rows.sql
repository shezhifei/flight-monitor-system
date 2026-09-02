-- F1: persisted hot-path read model. Deliberately no physical FKs after migration 120.
CREATE TABLE IF NOT EXISTS flight_monitor_rows (
    row_id VARCHAR(64) PRIMARY KEY,
    link_id VARCHAR(64),
    kind VARCHAR(16) NOT NULL DEFAULT 'single',
    inbound_flight_id VARCHAR(64),
    outbound_flight_id VARCHAR(64),
    inbound_flight_no VARCHAR(64),
    outbound_flight_no VARCHAR(64),
    inbound_scheduled_at TIMESTAMPTZ,
    outbound_scheduled_at TIMESTAMPTZ,
    inbound_estimated_at TIMESTAMPTZ,
    outbound_estimated_at TIMESTAMPTZ,
    inbound_actual_at TIMESTAMPTZ,
    outbound_actual_at TIMESTAMPTZ,
    inbound_station_code VARCHAR(16),
    outbound_station_code VARCHAR(16),
    inbound_is_vip BOOLEAN NOT NULL DEFAULT FALSE,
    outbound_is_vip BOOLEAN NOT NULL DEFAULT FALSE,
    registration VARCHAR(32),
    aircraft_type VARCHAR(64),
    stand_code VARCHAR(32),
    gate_code VARCHAR(32),
    terminal_code VARCHAR(32),
    baggage_carousel_code VARCHAR(32),
    status VARCHAR(32),
    workspace_date DATE,
    sort_time TIMESTAMPTZ,
    has_open_anomaly BOOLEAN NOT NULL DEFAULT FALSE,
    version INTEGER NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT chk_flight_monitor_rows_kind CHECK (kind IN ('turnaround', 'single'))
);

CREATE INDEX IF NOT EXISTS idx_flight_monitor_rows_workspace_sort
    ON flight_monitor_rows (workspace_date, sort_time DESC, row_id);
CREATE INDEX IF NOT EXISTS idx_flight_monitor_rows_inbound_no
    ON flight_monitor_rows (inbound_flight_no);
CREATE INDEX IF NOT EXISTS idx_flight_monitor_rows_outbound_no
    ON flight_monitor_rows (outbound_flight_no);
