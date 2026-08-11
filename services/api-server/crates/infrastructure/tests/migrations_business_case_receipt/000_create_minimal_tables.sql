-- Minimal schema for testing business case repository receipt JOIN behavior.
-- Mirrors the relevant subset of production tables so that
-- PgBusinessCaseRepository::find_by_id can be exercised end-to-end with
-- realistic notifications + workflow runs + business case rows.

CREATE TABLE IF NOT EXISTS users (
    id VARCHAR(26) PRIMARY KEY,
    username VARCHAR(64) NOT NULL UNIQUE,
    display_name VARCHAR(128),
    department VARCHAR(64),
    department_id VARCHAR(26),
    job_title VARCHAR(64),
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    is_admin BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS flights (
    flight_id VARCHAR(26) PRIMARY KEY,
    flight_number VARCHAR(32) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS business_case_types (
    id VARCHAR(26) PRIMARY KEY,
    code VARCHAR(64) NOT NULL UNIQUE,
    name VARCHAR(100) NOT NULL,
    description TEXT,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS flight_business_cases (
    id SERIAL PRIMARY KEY,
    case_id VARCHAR(26) NOT NULL UNIQUE,
    flight_id VARCHAR(26) NOT NULL,
    case_type VARCHAR(255) NOT NULL,
    description TEXT,
    context JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_by VARCHAR(100),
    updated_by VARCHAR(100),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    status VARCHAR(20) NOT NULL DEFAULT 'PENDING',
    stand VARCHAR(10),
    gate VARCHAR(10),
    visibility_scope VARCHAR(20) NOT NULL DEFAULT 'COMMON',
    department_id VARCHAR(64),
    department_name_snapshot VARCHAR(100),
    finished_at TIMESTAMP WITH TIME ZONE,
    cancelled_at TIMESTAMP WITH TIME ZONE,
    log TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
    processed_at TIMESTAMP WITH TIME ZONE,
    FOREIGN KEY (flight_id) REFERENCES flights(flight_id)
);

CREATE INDEX IF NOT EXISTS idx_fbc_flight_id ON flight_business_cases(flight_id);

CREATE TABLE IF NOT EXISTS flight_business_case_appends (
    id SERIAL PRIMARY KEY,
    append_id VARCHAR(26) NOT NULL UNIQUE,
    case_id VARCHAR(26) NOT NULL,
    content TEXT NOT NULL,
    submitted_by VARCHAR(100) NOT NULL,
    submitted_operator_name VARCHAR(100),
    appended_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    client_action_id VARCHAR(128),
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    CONSTRAINT fk_business_case_append_case
        FOREIGN KEY (case_id) REFERENCES flight_business_cases(case_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS business_case_workflow_runs (
    run_id VARCHAR(26) PRIMARY KEY,
    template_code VARCHAR(100) NOT NULL,
    case_id VARCHAR(26) NOT NULL,
    flight_id VARCHAR(26) NOT NULL,
    process_definition_key VARCHAR(100) NOT NULL,
    process_instance_id VARCHAR(64) NOT NULL,
    waiting_task_id VARCHAR(64),
    receipt_group_id VARCHAR(26),
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    outcome VARCHAR(32),
    recipient_snapshot JSONB NOT NULL DEFAULT '[]'::jsonb,
    flight_context_snapshot JSONB NOT NULL DEFAULT '{}'::jsonb,
    start_payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    started_by VARCHAR(100) NOT NULL,
    completed_at TIMESTAMPTZ,
    failed_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT fk_business_case_workflow_run_case
        FOREIGN KEY (case_id) REFERENCES flight_business_cases(case_id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_business_case_workflow_runs_case_id
    ON business_case_workflow_runs(case_id);

CREATE TABLE IF NOT EXISTS notifications (
    notification_id VARCHAR(26) PRIMARY KEY,
    user_id VARCHAR(26) NOT NULL,
    title VARCHAR(255) NOT NULL,
    body TEXT,
    category VARCHAR(32) NOT NULL DEFAULT 'system',
    severity VARCHAR(16) NOT NULL DEFAULT 'info',
    is_read BOOLEAN NOT NULL DEFAULT FALSE,
    related_entity_type VARCHAR(32),
    related_entity_id VARCHAR(64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    read_at TIMESTAMPTZ,
    origin_type VARCHAR(32) NOT NULL DEFAULT 'manual',
    receipt_required BOOLEAN NOT NULL DEFAULT FALSE,
    receipt_group_id VARCHAR(26),
    sender_user_id VARCHAR(26),
    sender_username_snapshot VARCHAR(128),
    delivery_status VARCHAR(32) NOT NULL DEFAULT 'pending',
    delivered_at TIMESTAMPTZ,
    ack_status VARCHAR(32) NOT NULL DEFAULT 'pending',
    ack_at TIMESTAMPTZ,
    ack_note TEXT,
    recipient_username_snapshot VARCHAR(128),
    recipient_display_name_snapshot VARCHAR(128),
    recipient_department_snapshot VARCHAR(64),
    recipient_job_title_snapshot VARCHAR(64),
    flight_id VARCHAR(26),
    dispatch_order_id VARCHAR(26),
    group_id VARCHAR(26),
    event_id VARCHAR(26),
    CONSTRAINT fk_notifications_user FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_notifications_receipt_group
    ON notifications (receipt_group_id, created_at DESC);
