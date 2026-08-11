CREATE TABLE IF NOT EXISTS users (
    id VARCHAR(26) PRIMARY KEY,
    username VARCHAR(100),
    password_hash VARCHAR(255),
    display_name VARCHAR(100),
    status VARCHAR(32),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS dispatch_alerts (
    id VARCHAR(26) PRIMARY KEY,
    flight_id VARCHAR(26),
    task_type VARCHAR(50),
    alert_type VARCHAR(50) NOT NULL,
    severity VARCHAR(20) DEFAULT 'warning',
    message TEXT NOT NULL,
    is_resolved BOOLEAN DEFAULT FALSE,
    resolved_at TIMESTAMP WITH TIME ZONE,
    resolved_by VARCHAR(26) REFERENCES users(id),
    resolution_notes TEXT,
    notify_users VARCHAR(26)[],
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    dedupe_key VARCHAR(128),
    current_order_id VARCHAR(26),
    next_order_id VARCHAR(26),
    last_detected_at TIMESTAMP WITH TIME ZONE,
    occurrence_count INTEGER NOT NULL DEFAULT 1,
    acknowledged_at TIMESTAMP WITH TIME ZONE,
    acknowledged_by VARCHAR(26) REFERENCES users(id),
    details JSONB,
    CONSTRAINT chk_dispatch_alerts_occurrence_count CHECK (occurrence_count >= 1)
);

CREATE UNIQUE INDEX idx_dispatch_alerts_dedupe_key
    ON dispatch_alerts (dedupe_key) WHERE dedupe_key IS NOT NULL;

CREATE INDEX idx_dispatch_alerts_current_next_order
    ON dispatch_alerts (current_order_id, next_order_id);

CREATE INDEX idx_dispatch_alerts_unresolved_created
    ON dispatch_alerts (is_resolved, created_at DESC);

INSERT INTO users (id, username) VALUES
    ('user-1', 'dispatcher-a'),
    ('user-2', 'dispatcher-b');
