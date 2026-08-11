-- Migration 000: Create base tables for a clean sqlx bootstrap.
-- Purpose: Ensure core tables exist before early migrations ALTER or FK-reference them.
--          Later migrations still own historical evolution; this migration only creates
--          the stable base shape needed for a fresh database to run the chain.


CREATE TABLE IF NOT EXISTS flights (
    flight_id VARCHAR(26) PRIMARY KEY,
    airline_code VARCHAR(8),
    flight_number VARCHAR(32),
    registration VARCHAR(32),
    aircraft_type_detail VARCHAR(64),
    status INTEGER NOT NULL DEFAULT 0,
    scheduled_departure TIMESTAMPTZ,
    scheduled_arrival TIMESTAMPTZ,
    estimated_departure TIMESTAMPTZ,
    estimated_arrival TIMESTAMPTZ,
    actual_departure TIMESTAMPTZ,
    actual_arrival TIMESTAMPTZ,
    execution_date DATE,
    workspace_date DATE,
    stand VARCHAR(32),
    gate VARCHAR(32),
    terminal VARCHAR(32),
    position VARCHAR(32),
    baggage_carousel VARCHAR(32),
    origin VARCHAR(16),
    destination VARCHAR(16),
    has_boarding_restriction BOOLEAN DEFAULT FALSE,
    is_quick_turnaround BOOLEAN DEFAULT FALSE,
    is_commercial_signed BOOLEAN DEFAULT FALSE,
    missions SMALLINT[],
    stand_types TEXT,
    flight_remarks TEXT,
    load_planning_remarks TEXT,
    aircraft_maintenance_remarks TEXT,
    aircraft_check_remarks TEXT,
    version BIGINT NOT NULL DEFAULT 0,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_flights_flight_number ON flights(flight_number);
CREATE INDEX IF NOT EXISTS idx_flights_scheduled_departure ON flights(scheduled_departure);
CREATE INDEX IF NOT EXISTS idx_flights_status ON flights(status);

CREATE TABLE IF NOT EXISTS users (
    id VARCHAR(26) PRIMARY KEY,
    username VARCHAR(64) NOT NULL UNIQUE,
    display_name VARCHAR(128),
    email VARCHAR(128) UNIQUE,
    password_hash VARCHAR(256),
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    is_verified BOOLEAN NOT NULL DEFAULT FALSE,
    is_admin BOOLEAN NOT NULL DEFAULT FALSE,
    verification_token VARCHAR(128),
    verification_token_expires TIMESTAMPTZ,
    verified_at TIMESTAMPTZ,
    password_reset_token VARCHAR(128),
    password_reset_token_expires TIMESTAMPTZ,
    password_changed_at TIMESTAMPTZ,
    last_login_at TIMESTAMPTZ,
    department VARCHAR(64),
    department_id VARCHAR(26),
    job_level SMALLINT DEFAULT 1,
    job_title VARCHAR(64),
    permission_version INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);
CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);
CREATE INDEX IF NOT EXISTS idx_users_department_id ON users(department_id) WHERE department_id IS NOT NULL;

-- =====================================================
-- 权限与角色基线表
-- =====================================================
CREATE TABLE IF NOT EXISTS permissions (
    id VARCHAR(26) PRIMARY KEY,
    name VARCHAR(100) NOT NULL UNIQUE,
    description TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    is_active BOOLEAN DEFAULT TRUE
);

CREATE TABLE IF NOT EXISTS roles (
    id VARCHAR(26) PRIMARY KEY,
    name VARCHAR(100) NOT NULL UNIQUE,
    description TEXT,
    is_system BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    is_active BOOLEAN DEFAULT TRUE
);

CREATE TABLE IF NOT EXISTS role_permissions (
    role_id VARCHAR(26) NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    permission_id VARCHAR(26) NOT NULL REFERENCES permissions(id) ON DELETE CASCADE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (role_id, permission_id)
);

CREATE TABLE IF NOT EXISTS user_roles (
    user_id VARCHAR(26) NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role_id VARCHAR(26) NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (user_id, role_id)
);

CREATE INDEX IF NOT EXISTS idx_roles_name ON roles(name);
CREATE INDEX IF NOT EXISTS idx_permissions_name ON permissions(name);

-- =====================================================
-- 待办事项基线表（被早期异常规则外键引用）
-- =====================================================
CREATE TABLE IF NOT EXISTS todos (
    todo_id VARCHAR(26) PRIMARY KEY,
    title VARCHAR(255) NOT NULL,
    description TEXT,
    priority VARCHAR(16) NOT NULL DEFAULT 'medium',
    status VARCHAR(16) NOT NULL DEFAULT 'pending',
    category VARCHAR(16),
    due_date TIMESTAMPTZ,
    assigned_to VARCHAR(64),
    tags TEXT[] NOT NULL DEFAULT '{}',
    estimated_duration INTEGER,
    actual_duration INTEGER,
    progress INTEGER NOT NULL DEFAULT 0,
    is_recurring BOOLEAN NOT NULL DEFAULT FALSE,
    recurring_pattern TEXT,
    parent_todo_id VARCHAR(26) REFERENCES todos(todo_id) ON DELETE SET NULL,
    execution_order INTEGER NOT NULL DEFAULT 0,
    depends_on TEXT[] NOT NULL DEFAULT '{}',
    source_type VARCHAR(32),
    source_id VARCHAR(64),
    agent_entity_id VARCHAR(255) DEFAULT 'default',
    agent_run_id VARCHAR(26),
    agent_status VARCHAR(50) DEFAULT 'pending',
    is_deleted BOOLEAN NOT NULL DEFAULT FALSE,
    deleted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by VARCHAR(64) NOT NULL DEFAULT 'system',
    updated_by VARCHAR(64) NOT NULL DEFAULT 'system',
    version INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS idx_todos_status ON todos(status) WHERE is_deleted = FALSE;
CREATE INDEX IF NOT EXISTS idx_todos_assigned_to ON todos(assigned_to) WHERE is_deleted = FALSE;
CREATE INDEX IF NOT EXISTS idx_todos_source ON todos(source_type, source_id) WHERE is_deleted = FALSE;
CREATE INDEX IF NOT EXISTS idx_todos_parent ON todos(parent_todo_id) WHERE is_deleted = FALSE;
CREATE INDEX IF NOT EXISTS idx_todos_created_at ON todos(created_at DESC) WHERE is_deleted = FALSE;

-- =====================================================
-- 待办事项状态变更基线表
-- =====================================================
CREATE TABLE IF NOT EXISTS todo_state_changes (
    id SERIAL PRIMARY KEY,
    change_id VARCHAR(26) NOT NULL UNIQUE,
    change_type VARCHAR(50) NOT NULL,
    todo_id VARCHAR(26) NOT NULL,
    change_data JSONB NOT NULL,
    metadata JSONB DEFAULT '{}',
    version INTEGER NOT NULL,
    occurred_at TIMESTAMP WITH TIME ZONE NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_tsc_todo_id ON todo_state_changes(todo_id);
CREATE INDEX IF NOT EXISTS idx_tsc_occurred_at ON todo_state_changes(occurred_at);

CREATE TABLE IF NOT EXISTS departments (
    id VARCHAR(26) PRIMARY KEY,
    name VARCHAR(100) NOT NULL UNIQUE,
    code VARCHAR(20) UNIQUE,
    description TEXT,
    manager_id VARCHAR(26) REFERENCES users(id),
    terminal VARCHAR(20),
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    is_active BOOLEAN DEFAULT TRUE
);

CREATE INDEX IF NOT EXISTS idx_departments_name ON departments(name);
CREATE INDEX IF NOT EXISTS idx_departments_terminal ON departments(terminal);

CREATE TABLE IF NOT EXISTS anomaly_rules (
    rule_id VARCHAR(64) PRIMARY KEY,
    rule_type VARCHAR(64) NOT NULL,
    name VARCHAR(128) NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    config JSONB NOT NULL DEFAULT '{}'::jsonb,
    severity VARCHAR(16) NOT NULL DEFAULT 'medium',
    auto_create_todo BOOLEAN NOT NULL DEFAULT TRUE,
    todo_priority VARCHAR(16) NOT NULL DEFAULT 'HIGH',
    escalation_intervals JSONB NOT NULL DEFAULT '[5, 15, 30]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT chk_anomaly_rules_severity CHECK (severity IN ('low', 'medium', 'high', 'critical'))
);

-- =====================================================
-- 航班状态变更表 (flight_state_changes)
-- =====================================================
CREATE TABLE IF NOT EXISTS flight_state_changes (
    id SERIAL PRIMARY KEY,
    change_id VARCHAR(26) NOT NULL UNIQUE,
    flight_id VARCHAR(26) NOT NULL,
    flight_number VARCHAR(7),
    change_type VARCHAR(255) NOT NULL,
    change_data JSONB NOT NULL,
    metadata JSONB,
    version INTEGER NOT NULL,
    occurred_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(flight_id, version)
);

CREATE INDEX IF NOT EXISTS idx_fsc_flight_id ON flight_state_changes(flight_id);
CREATE INDEX IF NOT EXISTS idx_fsc_occurred_at ON flight_state_changes(occurred_at);
CREATE INDEX IF NOT EXISTS idx_flight_state_changes_flight_id
    ON flight_state_changes (flight_id, occurred_at DESC);

-- =====================================================
-- 业务事项表 (flight_business_cases) 及其类型
-- =====================================================
CREATE TABLE IF NOT EXISTS business_case_types (
    id VARCHAR(26) PRIMARY KEY,
    code VARCHAR(64) NOT NULL UNIQUE,
    name VARCHAR(100) NOT NULL,
    bpmn_xml TEXT,
    description TEXT,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    visibility_scope VARCHAR(20) NOT NULL DEFAULT 'COMMON',
    department_id VARCHAR(64),
    department_name_snapshot VARCHAR(100),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS flight_business_cases (
    id SERIAL PRIMARY KEY,
    case_id VARCHAR(26) NOT NULL UNIQUE,
    flight_id VARCHAR(26) NOT NULL,
    case_type VARCHAR(255) NOT NULL,
    description TEXT,
    context JSONB NOT NULL,
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
CREATE INDEX IF NOT EXISTS idx_flight_business_cases_flight_id
    ON flight_business_cases (flight_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_flight_business_cases_visibility_department
    ON flight_business_cases (visibility_scope, department_id, created_at DESC);

-- =====================================================
-- 快照表 (snapshots)
-- =====================================================
CREATE TABLE IF NOT EXISTS snapshots (
    id SERIAL PRIMARY KEY,
    snapshot_id VARCHAR(26) NOT NULL UNIQUE,
    flight_id VARCHAR(26) NOT NULL,
    aggregate_id VARCHAR(26),
    aggregate_type VARCHAR(255),
    version BIGINT NOT NULL,
    state JSONB,
    snapshot_data JSONB NOT NULL,
    created_at TIMESTAMP NOT NULL,
    event_count BIGINT NOT NULL DEFAULT 0,
    created_at_timestamp DOUBLE PRECISION NOT NULL DEFAULT 0,
    integrity_hash VARCHAR(64),
    snapshot_strategy VARCHAR(50),
    metadata JSONB,
    timestamp TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_snapshots_aggregate_id ON snapshots(aggregate_id);
CREATE INDEX IF NOT EXISTS idx_snapshots_flight_id ON snapshots(flight_id);
CREATE INDEX IF NOT EXISTS idx_snapshots_version ON snapshots(flight_id, version);
CREATE INDEX IF NOT EXISTS idx_snapshots_timestamp ON snapshots(timestamp);

-- =====================================================
-- 事件流版本表 (event_stream_versions)
-- =====================================================
CREATE TABLE IF NOT EXISTS event_stream_versions (
    flight_id VARCHAR(26) PRIMARY KEY,
    current_version BIGINT NOT NULL DEFAULT 0,
    event_count BIGINT NOT NULL DEFAULT 0,
    last_event_time TIMESTAMP,
    last_snapshot_time TIMESTAMP,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    locked_at TIMESTAMP,
    locked_by VARCHAR(100),
    lock_version BIGINT NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_stream_versions_lock ON event_stream_versions(flight_id, lock_version);

