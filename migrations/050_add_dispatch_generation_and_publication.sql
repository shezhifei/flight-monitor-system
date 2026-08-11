
CREATE TABLE IF NOT EXISTS department_flight_generation_rules (
    id VARCHAR(26) PRIMARY KEY,
    department_id VARCHAR(26) NOT NULL REFERENCES departments(id) ON DELETE CASCADE,
    task_type VARCHAR(50) NOT NULL REFERENCES task_types(code) ON DELETE CASCADE,
    leg_scope VARCHAR(16) NOT NULL,
    version_no INT NOT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'draft',
    rule_name VARCHAR(120),
    conditions_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    generation_anchor_type VARCHAR(64) NOT NULL DEFAULT 'scheduled_time',
    start_offset_minutes INT NOT NULL DEFAULT 0,
    duration_minutes INT,
    publication_state VARCHAR(20) NOT NULL DEFAULT 'prepublished',
    publish_trigger_mode VARCHAR(20) NOT NULL DEFAULT 'time',
    publish_at TIMESTAMPTZ,
    publish_offset_minutes INT,
    publish_event_code VARCHAR(64),
    notes TEXT,
    published_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT chk_department_flight_generation_rules_leg_scope
        CHECK (leg_scope IN ('inbound', 'outbound', 'none'))
);

CREATE INDEX IF NOT EXISTS idx_department_flight_generation_rules_lookup
    ON department_flight_generation_rules(department_id, leg_scope, task_type, status, version_no DESC);

CREATE TABLE IF NOT EXISTS department_generation_adjustment_rules (
    id VARCHAR(26) PRIMARY KEY,
    department_id VARCHAR(26) NOT NULL REFERENCES departments(id) ON DELETE CASCADE,
    task_type VARCHAR(50) NOT NULL REFERENCES task_types(code) ON DELETE CASCADE,
    version_no INT NOT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'draft',
    rule_name VARCHAR(120),
    conditions_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    actions_json JSONB NOT NULL DEFAULT '[]'::jsonb,
    notes TEXT,
    published_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_department_generation_adjustment_rules_lookup
    ON department_generation_adjustment_rules(department_id, task_type, status, version_no DESC);

ALTER TABLE dispatch_orders
    ADD COLUMN IF NOT EXISTS publication_state VARCHAR(20) NOT NULL DEFAULT 'published',
    ADD COLUMN IF NOT EXISTS source_type VARCHAR(32) NOT NULL DEFAULT 'manual',
    ADD COLUMN IF NOT EXISTS department_id VARCHAR(26) REFERENCES departments(id),
    ADD COLUMN IF NOT EXISTS leg_scope VARCHAR(16) NOT NULL DEFAULT 'none',
    ADD COLUMN IF NOT EXISTS generation_rule_id VARCHAR(26),
    ADD COLUMN IF NOT EXISTS generation_rule_version INT,
    ADD COLUMN IF NOT EXISTS generation_anchor_type VARCHAR(64),
    ADD COLUMN IF NOT EXISTS generation_anchor_time TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS publish_trigger_mode VARCHAR(20),
    ADD COLUMN IF NOT EXISTS publish_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS turnaround_pair_key VARCHAR(64),
    ADD COLUMN IF NOT EXISTS turnaround_constraint_mode VARCHAR(32),
    ADD COLUMN IF NOT EXISTS equipment_requirement_snapshot JSONB NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN IF NOT EXISTS equipment_assignment JSONB NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN IF NOT EXISTS equipment_gap JSONB NOT NULL DEFAULT '[]'::jsonb;

CREATE INDEX IF NOT EXISTS idx_dispatch_orders_publication_state
    ON dispatch_orders(publication_state, planned_start_time);

CREATE INDEX IF NOT EXISTS idx_dispatch_orders_generation_rule
    ON dispatch_orders(generation_rule_id, generation_rule_version);

CREATE INDEX IF NOT EXISTS idx_dispatch_orders_turnaround_pair_key
    ON dispatch_orders(turnaround_pair_key);

