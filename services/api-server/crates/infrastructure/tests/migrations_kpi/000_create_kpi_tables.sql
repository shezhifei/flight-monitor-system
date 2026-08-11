CREATE TABLE IF NOT EXISTS flights (
    flight_id VARCHAR(26) PRIMARY KEY,
    scheduled_departure TIMESTAMPTZ,
    scheduled_arrival TIMESTAMPTZ,
    actual_departure TIMESTAMPTZ,
    actual_arrival TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS equipment (
    id VARCHAR(26) PRIMARY KEY
);

CREATE TABLE IF NOT EXISTS dispatch_orders (
    id VARCHAR(26) PRIMARY KEY,
    status VARCHAR(20) NOT NULL
);

CREATE TABLE IF NOT EXISTS dispatch_order_equipment (
    dispatch_order_id VARCHAR(26) REFERENCES dispatch_orders(id) ON DELETE CASCADE,
    equipment_id VARCHAR(26) REFERENCES equipment(id),
    released_at TIMESTAMPTZ,
    PRIMARY KEY (dispatch_order_id, equipment_id)
);

CREATE TABLE IF NOT EXISTS flight_dispatch_timeline_events (
    timeline_id VARCHAR(26) PRIMARY KEY,
    flight_id VARCHAR(26) NOT NULL,
    milestone_code VARCHAR(64) NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS anomalies (
    anomaly_id VARCHAR(26) PRIMARY KEY,
    flight_id VARCHAR(26) NOT NULL,
    status VARCHAR(20) NOT NULL,
    detected_at TIMESTAMPTZ NOT NULL
);
