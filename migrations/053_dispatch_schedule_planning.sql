
CREATE TABLE IF NOT EXISTS shift_templates (
    id VARCHAR(26) PRIMARY KEY,
    name VARCHAR(100) NOT NULL,
    resource_type VARCHAR(20) NOT NULL,
    resource_id VARCHAR(26) NOT NULL,
    terminal VARCHAR(20),
    start_time_local VARCHAR(5) NOT NULL,
    end_time_local VARCHAR(5) NOT NULL,
    weekdays JSONB NOT NULL DEFAULT '[]'::jsonb,
    max_continuous_minutes INT,
    min_rest_minutes INT,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_shift_templates_resource
    ON shift_templates(resource_type, resource_id);

CREATE TABLE IF NOT EXISTS shift_instances (
    id VARCHAR(26) PRIMARY KEY,
    template_id VARCHAR(26) REFERENCES shift_templates(id) ON DELETE SET NULL,
    resource_type VARCHAR(20) NOT NULL,
    resource_id VARCHAR(26) NOT NULL,
    terminal VARCHAR(20),
    start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ NOT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'scheduled',
    max_continuous_minutes INT,
    min_rest_minutes INT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_shift_instances_resource_window
    ON shift_instances(resource_type, resource_id, start_time, end_time);

CREATE TABLE IF NOT EXISTS schedule_leave_records (
    id VARCHAR(26) PRIMARY KEY,
    user_id VARCHAR(26) NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    team_id VARCHAR(26) REFERENCES teams(id) ON DELETE SET NULL,
    start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ NOT NULL,
    reason TEXT,
    status VARCHAR(20) NOT NULL DEFAULT 'approved',
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_schedule_leave_records_window
    ON schedule_leave_records(user_id, start_time, end_time);

CREATE TABLE IF NOT EXISTS equipment_downtimes (
    id VARCHAR(26) PRIMARY KEY,
    equipment_id VARCHAR(26) NOT NULL REFERENCES equipment(id) ON DELETE CASCADE,
    start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ NOT NULL,
    reason TEXT,
    status VARCHAR(20) NOT NULL DEFAULT 'scheduled',
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_equipment_downtimes_window
    ON equipment_downtimes(equipment_id, start_time, end_time);

CREATE TABLE IF NOT EXISTS dispatch_lock_rules (
    id VARCHAR(26) PRIMARY KEY,
    dispatch_order_id VARCHAR(26) REFERENCES dispatch_orders(id) ON DELETE CASCADE,
    flight_id VARCHAR(26),
    team_id VARCHAR(26) REFERENCES teams(id) ON DELETE SET NULL,
    lock_level VARCHAR(20) NOT NULL DEFAULT 'optimizable',
    start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ NOT NULL,
    reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_dispatch_lock_rules_window
    ON dispatch_lock_rules(dispatch_order_id, team_id, start_time, end_time);

ALTER TABLE dispatch_orders
    ADD COLUMN IF NOT EXISTS schedule_source VARCHAR(40) NOT NULL DEFAULT 'current_status_fallback',
    ADD COLUMN IF NOT EXISTS lock_level VARCHAR(20) NOT NULL DEFAULT 'optimizable',
    ADD COLUMN IF NOT EXISTS availability_reason TEXT,
    ADD COLUMN IF NOT EXISTS score_breakdown JSONB,
    ADD COLUMN IF NOT EXISTS conflict_reason TEXT;

CREATE INDEX IF NOT EXISTS idx_dispatch_orders_lock_level
    ON dispatch_orders(lock_level);

