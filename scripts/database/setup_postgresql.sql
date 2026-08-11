-- =====================================================
-- Database Initialization
-- =====================================================

-- 在目标数据库内直接执行本脚本。
-- 本脚本是幂等 schema/setup 脚本，不负责 DROP/CREATE 数据库，也不会清空现有表。

-- 创建扩展
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pg_trgm";

-- =====================================================
-- 逻辑复制角色
-- =====================================================
-- 分布式 compose 默认使用 fm_replicator 读取 domain_event_outbox 的逻辑复制流。
-- 在全新初始化数据库时，只有显式传入 flight_monitor.replication_password 时才创建该角色，
-- 避免 SQL 层继续携带仓库内置默认口令。
DO $$
DECLARE
    replication_password TEXT := NULLIF(current_setting('flight_monitor.replication_password', true), '');
BEGIN
    IF replication_password IS NULL THEN
        RAISE NOTICE 'Skipping fm_replicator bootstrap because flight_monitor.replication_password is not set.';
    ELSE
        IF char_length(replication_password) < 16
           OR lower(replication_password) IN ('test', 'password', 'default', 'changeme', 'replicator_pass')
        THEN
            RAISE EXCEPTION 'flight_monitor.replication_password must be an explicit strong secret.';
        END IF;

        IF NOT EXISTS (
            SELECT 1
            FROM pg_roles
            WHERE rolname = 'fm_replicator'
        ) THEN
            EXECUTE format(
                'CREATE ROLE fm_replicator WITH LOGIN REPLICATION PASSWORD %L',
                replication_password
            );
        ELSE
            EXECUTE format(
                'ALTER ROLE fm_replicator WITH LOGIN REPLICATION PASSWORD %L',
                replication_password
            );
            ALTER ROLE fm_replicator WITH REPLICATION;
        END IF;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM pg_roles
        WHERE rolname = 'fm_replicator'
    ) THEN
        EXECUTE format('GRANT CONNECT ON DATABASE %I TO fm_replicator', current_database());
    END IF;
END $$;

-- =====================================================
-- 航班信息表 (flights)
-- =====================================================
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
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    aircraft_type_binary SMALLINT GENERATED ALWAYS AS (
        CASE
            WHEN aircraft_type_detail IN ('A330', 'A340', 'A350', 'A380', 'B747', 'B767', 'B777', 'B787') THEN 1
            ELSE 0
        END
    ) STORED,
    cobt_time TIMESTAMPTZ,
    codt TIMESTAMPTZ,
    labels JSONB NOT NULL DEFAULT '[]'::jsonb
);

-- 航班表索引
CREATE INDEX IF NOT EXISTS idx_flights_flight_id ON flights(flight_id);
CREATE INDEX IF NOT EXISTS idx_flights_airline_code ON flights(airline_code);
CREATE INDEX IF NOT EXISTS idx_flights_status ON flights(status);
CREATE INDEX IF NOT EXISTS idx_flights_scheduled_departure ON flights(scheduled_departure);
CREATE INDEX IF NOT EXISTS idx_flights_execution_date ON flights(execution_date);
CREATE INDEX IF NOT EXISTS idx_flights_workspace_date ON flights(workspace_date);
CREATE INDEX IF NOT EXISTS idx_flights_created_at ON flights(created_at);
CREATE INDEX IF NOT EXISTS idx_flights_status_date ON flights(status, scheduled_departure);

-- =====================================================
-- 高并发性能优化索引
-- 支持5000用户、200写/秒、10万航班/天
-- =====================================================

-- 航班列表排序索引 (优化 ORDER BY COALESCE)
CREATE INDEX IF NOT EXISTS idx_flights_sort_key 
ON flights (COALESCE(scheduled_departure, scheduled_arrival) DESC);

-- 状态+日期复合索引 (优化状态过滤查询)
CREATE INDEX IF NOT EXISTS idx_flights_status_execution_date 
ON flights (status, execution_date DESC);

-- 工作区日期索引 (优化按工作日查询)
CREATE INDEX IF NOT EXISTS idx_flights_workspace_date_status 
ON flights (workspace_date, status);

-- 主航班号搜索索引
CREATE INDEX IF NOT EXISTS idx_flights_flight_number ON flights(flight_number);

-- 机位分配索引 (优化机位冲突检测)
CREATE INDEX IF NOT EXISTS idx_flights_stand_execution_date 
ON flights (stand, execution_date) 
WHERE stand IS NOT NULL;

-- 登机口索引 (优化登机口分配)
CREATE INDEX IF NOT EXISTS idx_flights_gate_execution_date 
ON flights (gate, execution_date) 
WHERE gate IS NOT NULL;

-- 航司索引 (优化按航司统计)
CREATE INDEX IF NOT EXISTS idx_flights_airline_code_date 
ON flights (airline_code, execution_date);

-- 更新时间索引 (优化增量同步)
CREATE INDEX IF NOT EXISTS idx_flights_updated_at 
ON flights (updated_at DESC);

-- 部分索引 - 仅活跃航班 (减少索引大小)
-- 活跃航班: status NOT IN (7, 8, 9) 即非 DEPARTED/NEXT_ARRIVED/CANCELLED
-- DELAYED(10) 仍可能恢复活跃，需包含
CREATE INDEX IF NOT EXISTS idx_flights_active 
ON flights (flight_id, status, scheduled_departure) 
WHERE status NOT IN (7, 8, 9);

-- 标签 GIN 索引 (优化标签筛选)
CREATE INDEX IF NOT EXISTS idx_flights_labels
ON flights USING GIN (labels);

-- 触发器函数：自动更新 execution_date 和 workspace_date
CREATE OR REPLACE FUNCTION update_flight_dates()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.scheduled_departure IS NOT NULL THEN
        NEW.execution_date := (NEW.scheduled_departure AT TIME ZONE 'Asia/Shanghai')::DATE;
        NEW.workspace_date := ((NEW.scheduled_departure AT TIME ZONE 'Asia/Shanghai') - INTERVAL '5 hours')::DATE;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- 触发器：在插入或更新时自动计算日期
DROP TRIGGER IF EXISTS trg_update_flight_dates ON flights;
CREATE TRIGGER trg_update_flight_dates
    BEFORE INSERT OR UPDATE OF scheduled_departure ON flights
    FOR EACH ROW
    EXECUTE FUNCTION update_flight_dates();

COMMENT ON TABLE flights IS '航班信息主表，存储所有航班的详细信息';
COMMENT ON COLUMN flights.flight_id IS '全局唯一标识符，ULID格式';
COMMENT ON COLUMN flights.version IS '乐观并发控制版本号';
COMMENT ON COLUMN flights.registration IS '航空器注册号（机号）';
COMMENT ON COLUMN flights.flight_number IS '聚合后的主航班号，优先取出港航班号';

CREATE TABLE IF NOT EXISTS flight_legs (
    leg_id VARCHAR(26) PRIMARY KEY,
    flight_id VARCHAR(26) NOT NULL,
    leg_type VARCHAR(16) NOT NULL,
    flight_no VARCHAR(16) NOT NULL,
    flight_type VARCHAR(16) NOT NULL,
    mission SMALLINT,
    origin_stations JSONB NOT NULL DEFAULT '[]'::jsonb,
    destination_stations JSONB NOT NULL DEFAULT '[]'::jsonb,
    is_vip BOOLEAN NOT NULL DEFAULT FALSE,
    stand_type VARCHAR(64),
    scheduled_time TIMESTAMPTZ,
    labels JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT chk_flight_legs_leg_type CHECK (leg_type IN ('inbound', 'outbound')),
    CONSTRAINT chk_flight_legs_flight_type CHECK (flight_type IN ('domestic', 'intl', 'region')),
    CONSTRAINT chk_flight_legs_mission CHECK (mission IS NULL OR mission IN (1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 31)),
    CONSTRAINT uq_flight_legs_flight_leg UNIQUE (flight_id, leg_type)
);

CREATE INDEX IF NOT EXISTS idx_flight_legs_flight_id
    ON flight_legs(flight_id);
CREATE INDEX IF NOT EXISTS idx_flight_legs_flight_no
    ON flight_legs(flight_no);
CREATE INDEX IF NOT EXISTS idx_flight_legs_leg_type_scheduled
    ON flight_legs(leg_type, scheduled_time DESC);
CREATE INDEX IF NOT EXISTS idx_flight_legs_labels
    ON flight_legs USING GIN (labels);

-- 标签字典表
CREATE TABLE IF NOT EXISTS label_definitions (
    label_id    VARCHAR(26) PRIMARY KEY,
    code        VARCHAR(64) NOT NULL UNIQUE,
    name        VARCHAR(100) NOT NULL,
    color       VARCHAR(7) NOT NULL DEFAULT '#6B7280',
    icon        VARCHAR(32),
    scope       VARCHAR(8) NOT NULL DEFAULT 'flight',
    category    VARCHAR(16) NOT NULL DEFAULT 'custom',
    is_active   BOOLEAN NOT NULL DEFAULT TRUE,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_by  VARCHAR(100),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT chk_label_scope CHECK (scope IN ('flight', 'leg', 'both')),
    CONSTRAINT chk_label_category CHECK (category IN ('system', 'custom'))
);

CREATE INDEX IF NOT EXISTS idx_label_definitions_code ON label_definitions(code);
CREATE INDEX IF NOT EXISTS idx_label_definitions_scope ON label_definitions(scope);

CREATE TABLE IF NOT EXISTS flight_dispatch_timeline_events (
    timeline_id VARCHAR(26) PRIMARY KEY,
    flight_id VARCHAR(26) NOT NULL,
    milestone_code VARCHAR(64) NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    leg_type VARCHAR(16),
    recorded_by VARCHAR(128),
    client_action_id VARCHAR(128),
    source VARCHAR(64) NOT NULL DEFAULT 'manual',
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT chk_timeline_leg_type CHECK (leg_type IS NULL OR leg_type IN ('inbound', 'outbound'))
);

CREATE INDEX IF NOT EXISTS idx_flight_dispatch_timeline_flight_occurred
    ON flight_dispatch_timeline_events(flight_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_flight_dispatch_timeline_milestone_occurred
    ON flight_dispatch_timeline_events(milestone_code, occurred_at DESC);
CREATE UNIQUE INDEX IF NOT EXISTS uq_flight_dispatch_timeline_client_action
    ON flight_dispatch_timeline_events(flight_id, client_action_id)
    WHERE client_action_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS flight_custom_field_archive (
    archive_id VARCHAR(26) PRIMARY KEY,
    flight_id VARCHAR(26) NOT NULL,
    field_key VARCHAR(128) NOT NULL,
    field_value_json JSONB,
    migrated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_flight_custom_field_archive_flight_id
    ON flight_custom_field_archive(flight_id);
CREATE INDEX IF NOT EXISTS idx_flight_custom_field_archive_field_key
    ON flight_custom_field_archive(field_key);

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
    official_record_count INTEGER NOT NULL DEFAULT 0,
    registration_enriched_count INTEGER NOT NULL DEFAULT 0,
    registration_ambiguous_count INTEGER NOT NULL DEFAULT 0,
    registration_missing_count INTEGER NOT NULL DEFAULT 0,
    stitched_turnaround_count INTEGER NOT NULL DEFAULT 0,
    failure_samples JSONB NOT NULL DEFAULT '[]'::jsonb,
    error_summary JSONB NOT NULL DEFAULT '[]'::jsonb,
    started_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT chk_flight_sync_runs_trigger
        CHECK (trigger IN ('scheduled', 'manual'))
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
    CONSTRAINT uq_flight_sync_bindings_source_key UNIQUE (source_system, natural_key)
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
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_flight_sync_snapshots_run_id
    ON flight_sync_snapshots(run_id);

CREATE INDEX IF NOT EXISTS idx_flight_sync_snapshots_source_created
    ON flight_sync_snapshots(source_system, created_at DESC);

CREATE TABLE IF NOT EXISTS flight_identity_bindings (
    identity_binding_id VARCHAR(26) PRIMARY KEY,
    vendor VARCHAR(64) NOT NULL,
    vendor_movement_id VARCHAR(128) NOT NULL,
    registration VARCHAR(32),
    official_natural_key VARCHAR(255) NOT NULL,
    flight_id VARCHAR(26) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uq_flight_identity_bindings_vendor_movement UNIQUE (vendor, vendor_movement_id)
);

CREATE INDEX IF NOT EXISTS idx_flight_identity_bindings_flight_id
    ON flight_identity_bindings(flight_id);

CREATE INDEX IF NOT EXISTS idx_flight_identity_bindings_registration
    ON flight_identity_bindings(registration, last_seen_at DESC)
    WHERE registration IS NOT NULL;

CREATE TABLE IF NOT EXISTS flight_aircraft_sequences (
    sequence_binding_id VARCHAR(26) PRIMARY KEY,
    sequence_key VARCHAR(512) NOT NULL UNIQUE,
    registration VARCHAR(32) NOT NULL,
    flight_id VARCHAR(26) NOT NULL,
    inbound_natural_key VARCHAR(255),
    outbound_natural_key VARCHAR(255),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_flight_aircraft_sequences_registration
    ON flight_aircraft_sequences(registration, last_seen_at DESC);

-- =====================================================
-- 航班状态变更表 (flight_state_changes)
-- =====================================================
CREATE TABLE IF NOT EXISTS flight_state_changes (
    id SERIAL PRIMARY KEY,
    change_id VARCHAR(26) NOT NULL UNIQUE,      -- ULID格式
    flight_id VARCHAR(26) NOT NULL,
    flight_number VARCHAR(7),
    change_type VARCHAR(255) NOT NULL,
    change_data JSONB NOT NULL,
    metadata JSONB,
    version INTEGER NOT NULL,
    occurred_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(flight_id, version)
);

-- 索引
CREATE INDEX IF NOT EXISTS idx_fsc_flight_id ON flight_state_changes(flight_id);
CREATE INDEX IF NOT EXISTS idx_fsc_occurred_at ON flight_state_changes(occurred_at);
CREATE INDEX IF NOT EXISTS idx_flight_state_changes_flight_id
ON flight_state_changes (flight_id, occurred_at DESC);

COMMENT ON TABLE flight_state_changes IS '航班状态变更表，用于聚合状态重建';

-- =====================================================
-- 业务事项表 (flight_business_cases)及其类型 (business_case_types)
-- =====================================================
CREATE TABLE IF NOT EXISTS business_case_types (
    id         VARCHAR(26) PRIMARY KEY,
    code       VARCHAR(64) NOT NULL UNIQUE,
    name       VARCHAR(100) NOT NULL,
    bpmn_xml   TEXT,
    description TEXT,
    is_active  BOOLEAN NOT NULL DEFAULT TRUE,
    visibility_scope VARCHAR(20) NOT NULL DEFAULT 'COMMON',
    department_id VARCHAR(64),
    department_name_snapshot VARCHAR(100),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE  business_case_types IS '业务事项类型（关联 Flowable 流程编排）';
COMMENT ON COLUMN business_case_types.code IS '唯一编码，同时作为 Flowable process key';
COMMENT ON COLUMN business_case_types.bpmn_xml IS '该事项关联的 BPMN 流程 XML 草稿';

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

CREATE TABLE IF NOT EXISTS flight_business_case_appends (
    id SERIAL PRIMARY KEY,
    append_id VARCHAR(26) NOT NULL UNIQUE,
    case_id VARCHAR(26) NOT NULL,
    content TEXT NOT NULL,
    client_action_id VARCHAR(128),
    submitted_by VARCHAR(100) NOT NULL,
    submitted_operator_name VARCHAR(100),
    appended_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    CONSTRAINT fk_business_case_append_case
        FOREIGN KEY (case_id) REFERENCES flight_business_cases(case_id) ON DELETE CASCADE
);

ALTER TABLE flight_business_case_appends
    ADD COLUMN IF NOT EXISTS metadata JSONB NOT NULL DEFAULT '{}'::jsonb;
ALTER TABLE flight_business_case_appends
    ADD COLUMN IF NOT EXISTS client_action_id VARCHAR(128);

COMMENT ON COLUMN flight_business_case_appends.metadata
    IS '追加记录的结构化元数据（tool_calls, token_usage, thinking, step_type, sequence 等）';

CREATE INDEX IF NOT EXISTS idx_fbc_appends_case_id_time
ON flight_business_case_appends (case_id, appended_at ASC);

CREATE UNIQUE INDEX IF NOT EXISTS uq_fbc_appends_case_client_action
ON flight_business_case_appends (case_id, client_action_id)
WHERE client_action_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_fbc_policy_interception_error_code
    ON flight_business_cases ((context->>'error_code'))
    WHERE case_type = 'policy_interception';

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
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_business_case_workflow_runs_case_id
    ON business_case_workflow_runs(case_id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_business_case_workflow_runs_process_instance_id
    ON business_case_workflow_runs(process_instance_id);

CREATE INDEX IF NOT EXISTS idx_business_case_workflow_runs_template_status
    ON business_case_workflow_runs(template_code, status);

CREATE INDEX IF NOT EXISTS idx_business_case_workflow_runs_receipt_group
    ON business_case_workflow_runs(receipt_group_id);


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

-- 快照表索引
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

-- 事件流版本表索引
CREATE INDEX IF NOT EXISTS idx_stream_versions_lock ON event_stream_versions(flight_id, lock_version);

-- =====================================================
-- AI会话表 (ai_conversations)
-- =====================================================
CREATE TABLE IF NOT EXISTS ai_conversations (
    id VARCHAR(255) PRIMARY KEY,
    entity_id VARCHAR(255),
    title VARCHAR(500),
    status VARCHAR(50) NOT NULL DEFAULT 'active',
    context_id VARCHAR(255),
    model VARCHAR(100) NOT NULL DEFAULT 'gpt-3.5-turbo',
    temperature DOUBLE PRECISION NOT NULL DEFAULT 0.7,
    max_tokens INTEGER NOT NULL DEFAULT 0,
    system_prompt TEXT,
    parent_id VARCHAR(255),
    user_id VARCHAR(255),
    user_name VARCHAR(255),
    user_info JSONB,
    session_id VARCHAR(255),
    client_info JSONB,
    tags TEXT[],
    custom_data JSONB,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    last_activity_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    ended_at TIMESTAMP WITH TIME ZONE,
    message_count INTEGER NOT NULL DEFAULT 0,
    total_tokens BIGINT NOT NULL DEFAULT 0,
    total_cost DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    metadata JSONB,
    extensions JSONB
);

-- AI会话索引
CREATE INDEX IF NOT EXISTS idx_ai_conversations_entity_id ON ai_conversations(entity_id);
CREATE INDEX IF NOT EXISTS idx_ai_conversations_created_at ON ai_conversations(created_at);
CREATE INDEX IF NOT EXISTS idx_conversations_user_id ON ai_conversations(user_id);
CREATE INDEX IF NOT EXISTS idx_conversations_status ON ai_conversations(status);
CREATE INDEX IF NOT EXISTS idx_conversations_tags ON ai_conversations USING GIN(tags);

-- =====================================================
-- 用户与权限管理表
-- =====================================================

-- 权限表
CREATE TABLE IF NOT EXISTS permissions (
    id VARCHAR(26) PRIMARY KEY,
    name VARCHAR(100) NOT NULL UNIQUE,
    description TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    is_active BOOLEAN DEFAULT TRUE
);

-- 角色表
CREATE TABLE IF NOT EXISTS roles (
    id VARCHAR(26) PRIMARY KEY,
    name VARCHAR(100) NOT NULL UNIQUE,
    description TEXT,
    is_system BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    is_active BOOLEAN DEFAULT TRUE
);

-- 用户表
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
    verification_token_expires TIMESTAMP WITH TIME ZONE,
    verified_at TIMESTAMP WITH TIME ZONE,
    password_reset_token VARCHAR(128),
    password_reset_token_expires TIMESTAMP WITH TIME ZONE,
    password_changed_at TIMESTAMP WITH TIME ZONE,
    last_login_at TIMESTAMP WITH TIME ZONE,
    department VARCHAR(64),
    department_id VARCHAR(26),
    job_level SMALLINT DEFAULT 1,
    job_title VARCHAR(64),
    permission_version INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- 角色-权限关联表
CREATE TABLE IF NOT EXISTS role_permissions (
    role_id VARCHAR(26) NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    permission_id VARCHAR(26) NOT NULL REFERENCES permissions(id) ON DELETE CASCADE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (role_id, permission_id)
);

-- 用户-角色关联表
CREATE TABLE IF NOT EXISTS user_roles (
    user_id VARCHAR(26) NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role_id VARCHAR(26) NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (user_id, role_id)
);

-- 用户相关索引
CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);
CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);
CREATE INDEX IF NOT EXISTS idx_users_is_active ON users(is_active);
CREATE INDEX IF NOT EXISTS idx_users_display_name ON users(display_name)
    WHERE display_name IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_users_department ON users(department);
CREATE INDEX IF NOT EXISTS idx_users_job_level ON users(job_level);
CREATE INDEX IF NOT EXISTS idx_roles_name ON roles(name);
CREATE INDEX IF NOT EXISTS idx_permissions_name ON permissions(name);

COMMENT ON TABLE users IS '用户信息表，存储系统用户数据';
COMMENT ON COLUMN users.display_name IS '账号默认展示姓名';
COMMENT ON COLUMN users.department IS '所属科室/部门';
COMMENT ON COLUMN users.department_id IS '所属科室/部门 ID';
COMMENT ON COLUMN users.job_level IS '职级(1=一线员工, 2=班组长, 3=主管, 4=经理, 5=总监)';
COMMENT ON COLUMN users.job_title IS '职位名称';
COMMENT ON COLUMN users.permission_version IS '权限版本号，递增后使旧 JWT 失效，实现精准踢出';

CREATE TABLE IF NOT EXISTS operator_identity_contexts (
    user_id VARCHAR(26) NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    context_type VARCHAR(32) NOT NULL,
    context_id VARCHAR(128) NOT NULL,
    operator_name VARCHAR(100) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (user_id, context_type, context_id),
    CONSTRAINT chk_operator_identity_context_type
        CHECK (context_type IN ('mobile_device', 'web_client'))
);

CREATE INDEX IF NOT EXISTS idx_operator_identity_contexts_scope
    ON operator_identity_contexts(context_type, context_id, updated_at DESC);

-- =====================================================
-- 系统配置表 (system_config)
-- =====================================================
CREATE TABLE IF NOT EXISTS system_config (
    key VARCHAR(255) PRIMARY KEY,
    value JSONB NOT NULL,
    description TEXT,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- =====================================================
-- 待办事项表 (todos)
-- =====================================================
CREATE TABLE IF NOT EXISTS todos (
    todo_id VARCHAR(26) PRIMARY KEY,
    title VARCHAR(255) NOT NULL,
    description TEXT,
    priority VARCHAR(16) NOT NULL DEFAULT 'medium',
    status VARCHAR(16) NOT NULL DEFAULT 'pending',
    category VARCHAR(16),
    due_date TIMESTAMP WITH TIME ZONE,
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
    deleted_at TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by VARCHAR(64) NOT NULL DEFAULT 'system',
    updated_by VARCHAR(64) NOT NULL DEFAULT 'system',
    version INTEGER NOT NULL DEFAULT 1
);

-- 待办事项表索引
CREATE INDEX IF NOT EXISTS idx_todos_todo_id ON todos(todo_id);
CREATE INDEX IF NOT EXISTS idx_todos_status ON todos(status);
CREATE INDEX IF NOT EXISTS idx_todos_priority ON todos(priority);
CREATE INDEX IF NOT EXISTS idx_todos_category ON todos(category);
CREATE INDEX IF NOT EXISTS idx_todos_assigned_to ON todos(assigned_to);
CREATE INDEX IF NOT EXISTS idx_todos_due_date ON todos(due_date);
CREATE INDEX IF NOT EXISTS idx_todos_created_at ON todos(created_at);
CREATE INDEX IF NOT EXISTS idx_todos_is_deleted ON todos(is_deleted);
CREATE INDEX IF NOT EXISTS idx_todos_status_priority ON todos(status, priority);
CREATE INDEX IF NOT EXISTS idx_todos_tags ON todos USING gin(tags);

COMMENT ON TABLE todos IS '待办事项主表，存储当前状态';
COMMENT ON COLUMN todos.todo_id IS '全局唯一标识符，ULID格式';
COMMENT ON COLUMN todos.version IS '乐观并发控制版本号';

-- =====================================================
-- 待办事项状态变更表 (todo_state_changes)
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

-- 索引
CREATE INDEX IF NOT EXISTS idx_tsc_todo_id ON todo_state_changes(todo_id);
CREATE INDEX IF NOT EXISTS idx_tsc_occurred_at ON todo_state_changes(occurred_at);

COMMENT ON TABLE todo_state_changes IS '待办事项状态变更表，用于状态重建';

-- =====================================================
-- 待办事项快照表 (todo_snapshots)
-- =====================================================
CREATE TABLE IF NOT EXISTS todo_snapshots (
    id SERIAL PRIMARY KEY,
    snapshot_id VARCHAR(26) NOT NULL UNIQUE,
    aggregate_id VARCHAR(26) NOT NULL,
    snapshot_data JSONB NOT NULL,
    snapshot_metadata JSONB DEFAULT '{}',
    version INTEGER NOT NULL,
    change_version INTEGER NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- 待办事项快照表索引
CREATE INDEX IF NOT EXISTS idx_todo_snapshots_snapshot_id ON todo_snapshots(snapshot_id);
CREATE INDEX IF NOT EXISTS idx_todo_snapshots_aggregate_id ON todo_snapshots(aggregate_id);
CREATE INDEX IF NOT EXISTS idx_todo_snapshots_version ON todo_snapshots(version);
CREATE UNIQUE INDEX IF NOT EXISTS idx_todo_snapshots_aggregate_version ON todo_snapshots(aggregate_id, version);

COMMENT ON TABLE todo_snapshots IS '待办事项快照表，用于优化事件重放';

-- =====================================================
-- TODO 链模板表 (todo_chain_templates)
-- =====================================================
CREATE TABLE IF NOT EXISTS todo_chain_templates (
    template_id VARCHAR(64) PRIMARY KEY,
    name VARCHAR(128) NOT NULL,
    description TEXT,
    task_types JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_by VARCHAR(64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- =====================================================
-- 事件流程定义表 (event_process_definitions)
-- =====================================================
CREATE TABLE IF NOT EXISTS event_process_definitions (
    id VARCHAR(26) PRIMARY KEY,
    event_type VARCHAR(100) NOT NULL UNIQUE,
    event_type_name VARCHAR(255),
    description TEXT,
    bpmn_xml TEXT,
    flowable_deployment_id VARCHAR(255),
    flowable_process_definition_key VARCHAR(255),
    is_active BOOLEAN DEFAULT TRUE,
    is_deprecated BOOLEAN DEFAULT FALSE,
    auto_start_on_event BOOLEAN DEFAULT FALSE,
    process_timeout_minutes INTEGER DEFAULT 60,
    retry_count INTEGER DEFAULT 3,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    created_by VARCHAR(100),
    updated_by VARCHAR(100)
);

-- 事件流程定义表索引
CREATE INDEX IF NOT EXISTS idx_event_process_definitions_event_type ON event_process_definitions(event_type);
CREATE INDEX IF NOT EXISTS idx_event_process_definitions_is_active ON event_process_definitions(is_active);

-- =====================================================
-- 事件类型元数据表 (event_type_metadata)
-- =====================================================
CREATE TABLE IF NOT EXISTS event_type_metadata (
    event_type_code TEXT PRIMARY KEY,
    event_type_name TEXT NOT NULL,
    description TEXT,
    has_process BOOLEAN DEFAULT FALSE,
    process_definition_key TEXT,
    process_template_path TEXT,
    auto_start_process BOOLEAN DEFAULT TRUE,
    process_timeout_minutes INTEGER DEFAULT 60,
    retry_count INTEGER DEFAULT 3,
    custom_parameters TEXT,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE
);

-- =====================================================
-- 流程模板表 (process_templates)
-- =====================================================
CREATE TABLE IF NOT EXISTS process_templates (
    template_id TEXT PRIMARY KEY,
    event_type_code TEXT NOT NULL,
    process_definition_key TEXT NOT NULL,
    bpmn_xml TEXT NOT NULL,
    version INTEGER DEFAULT 1,
    is_active BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    deployed_at TIMESTAMP WITH TIME ZONE,
    deployment_id TEXT,
    custom_parameters TEXT,
    FOREIGN KEY (event_type_code) REFERENCES event_type_metadata(event_type_code)
);

-- 流程模板表索引
CREATE INDEX IF NOT EXISTS idx_templates_event_type ON process_templates(event_type_code);
CREATE INDEX IF NOT EXISTS idx_templates_active ON process_templates(is_active) WHERE is_active = TRUE;

-- =====================================================
-- 流程实例映射表 (process_instance_mappings)
-- =====================================================
CREATE TABLE IF NOT EXISTS process_instance_mappings (
    mapping_id TEXT PRIMARY KEY,
    event_id TEXT NOT NULL,
    process_instance_id TEXT NOT NULL,
    process_definition_key TEXT NOT NULL,
    status TEXT NOT NULL,
    started_at TIMESTAMP WITH TIME ZONE NOT NULL,
    finished_at TIMESTAMP WITH TIME ZONE,
    result TEXT,
    error_message TEXT,
    variables TEXT
);

-- 流程实例映射表索引
CREATE INDEX IF NOT EXISTS idx_process_mappings_event_id ON process_instance_mappings(event_id);
CREATE INDEX IF NOT EXISTS idx_process_mappings_process_instance ON process_instance_mappings(process_instance_id);
CREATE INDEX IF NOT EXISTS idx_process_mappings_status ON process_instance_mappings(status);

-- =====================================================
-- 流程异常日志表 (process_exception_logs)
-- =====================================================
CREATE TABLE IF NOT EXISTS process_exception_logs (
    log_id SERIAL PRIMARY KEY,
    process_instance_id TEXT NOT NULL,
    exception_type TEXT NOT NULL,
    exception_message TEXT,
    process_type TEXT,
    logged_at TIMESTAMP WITH TIME ZONE NOT NULL,
    resolved_at TIMESTAMP WITH TIME ZONE,
    resolved_by TEXT,
    resolution_notes TEXT
);

-- =====================================================
-- 流程健康检查历史表 (process_health_check_history)
-- =====================================================
CREATE TABLE IF NOT EXISTS process_health_check_history (
    check_id SERIAL PRIMARY KEY,
    total_processes INTEGER,
    active_processes INTEGER,
    completed_processes INTEGER,
    cancelled_processes INTEGER,
    failed_processes INTEGER,
    timed_out_processes INTEGER,
    average_duration REAL,
    health_score REAL,
    status TEXT,
    checked_at TIMESTAMP WITH TIME ZONE NOT NULL
);

-- =====================================================
-- 系统审计日志表 (system_audit_logs)
-- =====================================================
CREATE TABLE IF NOT EXISTS system_audit_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    entity_type VARCHAR(50) NOT NULL,
    entity_id VARCHAR(100) NOT NULL,
    action VARCHAR(50) NOT NULL,
    changes JSONB,
    user_id VARCHAR(100),
    trace_id VARCHAR(100),
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_audit_entity ON system_audit_logs(entity_type, entity_id);
CREATE INDEX IF NOT EXISTS idx_audit_time ON system_audit_logs(created_at);

COMMENT ON TABLE system_audit_logs IS '系统审计日志，记录实体变更历史';

-- =====================================================

-- =====================================================
-- 插入默认数据
-- =====================================================

-- 插入默认权限
INSERT INTO permissions (id, name, description) VALUES
    ('01H00000000000000000000001', 'user:read', '查看用户信息'),
    ('01H00000000000000000000002', 'user:create', '创建用户'),
    ('01H00000000000000000000003', 'user:update', '更新用户信息'),
    ('01H00000000000000000000004', 'user:delete', '删除用户'),
    ('01H00000000000000000000005', 'role:read', '查看角色信息'),
    ('01H00000000000000000000006', 'role:create', '创建角色'),
    ('01H00000000000000000000007', 'role:update', '更新角色'),
    ('01H00000000000000000000008', 'role:delete', '删除角色'),
    ('01H00000000000000000000009', 'system:admin', '系统管理员权限'),
    ('01H00000000000000000000010', 'flight:read', '查看航班信息'),
    ('01H00000000000000000000011', 'flight:manage', '管理航班信息'),
    ('01H00000000000000000000012', 'ai:chat', '使用AI对话'),
    ('01H00000000000000000000013', 'todo:manage', '管理待办事项'),
    ('perm_ai_view', 'ai:view', '查看AI工具与审批信息'),
    -- 航班扩展时间字段编辑权限
    ('01H00000000000000000000020', 'flight:edit:wheel_chocks', '编辑上轮挡时间'),
    ('01H00000000000000000000021', 'flight:edit:cabin_door_open', '编辑开舱门时间'),
    ('01H00000000000000000000022', 'flight:edit:deboarding', '编辑下客完成时间'),
    ('01H00000000000000000000023', 'flight:edit:cleaning_start', '编辑清洁开始时间'),
    ('01H00000000000000000000024', 'flight:edit:cleaning_end', '编辑清洁结束时间'),
    ('01H00000000000000000000025', 'flight:edit:cabin_door_close', '编辑关客舱门时间'),
    ('01H00000000000000000000026', 'flight:edit:cargo_door_close', '编辑关货舱门时间'),
    ('01H00000000000000000000027', 'flight:edit:loading_complete', '编辑装载完成时间'),
    ('01H00000000000000000000028', 'flight:edit:off_blocks', '编辑撤轮挡时间'),
    ('01H00000000000000000000029', 'flight:edit:passengers_ready', '编辑人齐时间'),
    ('01H00000000000000000000030', 'flight:edit:boarding_permission', '编辑允许登机时间'),
    -- 派工系统权限
    ('perm_dispatch_view', 'dispatch:view', '查看派工单'),
    ('perm_dispatch_manage', 'dispatch:manage', '管理派工'),
    ('perm_team_view', 'team:view', '查看班组'),
    ('perm_team_manage', 'team:manage', '管理班组'),
    ('perm_equipment_view', 'equipment:view', '查看设备'),
    ('perm_equipment_manage', 'equipment:manage', '管理设备'),
    ('perm_schedule_view', 'schedule:view', '查看排班'),
    ('perm_schedule_manage', 'schedule:manage', '管理排班'),
    -- 细粒度资源动作权限（V2）
    ('perm_flight_read_v2', 'flight.read', '查看航班信息'),
    ('perm_flight_update_v2', 'flight.update', '更新航班信息'),
    ('perm_flight_timeline_v2', 'flight.timeline_edit', '编辑航班时间线'),
    ('perm_flight_import_v2', 'flight.import_commit', '提交航班导入'),
    ('perm_flight_report_v2', 'flight.report_generate', '生成航班动态报表'),
    ('perm_bc_create_v2', 'business_case.create', '创建业务事项'),
    ('perm_bc_read_v2', 'business_case.read', '查看业务事项'),
    ('perm_bc_append_v2', 'business_case.append', '追加业务事项信息'),
    ('perm_bc_update_v2', 'business_case.update', '更新业务事项'),
    ('perm_bc_status_v2', 'business_case.status_transition', '推进业务事项状态'),
    ('perm_bc_delete_v2', 'business_case.delete', '删除业务事项'),
    ('perm_wr_start_v2', 'workflow_run.start', '发起业务事项流程实例'),
    ('perm_wr_read_v2', 'workflow_run.read', '查看流程运行实例'),
    ('perm_wr_act_v2', 'workflow_run.act', '执行流程人工节点动作'),
    ('perm_wf_def_read2', 'workflow_definition.read', '查看流程定义'),
    ('perm_wf_def_edit2', 'workflow_definition.edit', '编辑流程定义'),
    ('perm_wf_def_pub2', 'workflow_definition.publish', '发布流程定义'),
    ('perm_wf_def_depr2', 'workflow_definition.deprecate', '停用流程定义'),
    ('perm_auto_notify2', 'automation.notify_send', '在流程中使用自动通知能力'),
    ('perm_auto_dispatch2', 'automation.dispatch_create', '在流程中使用自动派工能力'),
    ('perm_auto_case_ok2', 'automation.business_case_complete', '在流程中自动完成业务事项'),
    ('perm_auto_case_ng2', 'automation.business_case_fail', '在流程中自动失败业务事项'),
    ('perm_auto_http2', 'automation.external_call', '在流程中调用外部接口'),
    ('perm_auto_ai2', 'automation.ai_execute', '在流程中执行 AI 自动化能力'),
    ('perm_do_read_v2', 'dispatch_order.read', '查看派工单'),
    ('perm_do_create_v2', 'dispatch_order.create', '创建派工单'),
    ('perm_do_update_v2', 'dispatch_order.update', '更新派工单'),
    ('perm_do_publish_v2', 'dispatch_order.publish', '发布派工单'),
    ('perm_do_cancel_v2', 'dispatch_order.cancel', '取消派工单'),
    ('perm_dc_read_v2', 'dispatch_catalog.read', '查看派工资源目录'),
    ('perm_dc_edit_v2', 'dispatch_catalog.edit', '编辑派工资源目录'),
    ('perm_sh_read_v2', 'shift_handover.read', '查看交接班'),
    ('perm_sh_create_v2', 'shift_handover.create', '创建交接班'),
    ('perm_sh_submit_v2', 'shift_handover.submit', '提交交接班'),
    ('perm_sh_ack_v2', 'shift_handover.ack', '确认交接班'),
    ('perm_notif_read2', 'notification.read', '查看通知'),
    ('perm_notif_send2', 'notification.send', '发送通知'),
    ('perm_notif_rcpt_r2', 'notification.receipt_read', '查看通知回执'),
    ('perm_notif_rcpt_m2', 'notification.receipt_manage', '管理通知回执'),
    ('perm_auth_role_r2', 'auth_role.read', '查看角色定义'),
    ('perm_auth_role_e2', 'auth_role.edit', '编辑角色定义'),
    ('perm_auth_tpl_r2', 'auth_permission_template.read', '查看权限模板'),
    ('perm_auth_tpl_e2', 'auth_permission_template.edit', '编辑权限模板'),
    ('perm_user_admin_r2', 'user_admin.read', '查看用户管理数据'),
    ('perm_user_admin_e2', 'user_admin.edit', '编辑用户管理数据'),
    ('perm_sys_cfg_r2', 'system.config_read', '查看系统配置'),
    ('perm_sys_cfg_w2', 'system.config_write', '编辑系统配置'),
    ('perm_sys_ops_admin2', 'system.ops_admin', '执行系统运维管理操作')
ON CONFLICT (name) DO UPDATE SET
    description = EXCLUDED.description,
    is_active = TRUE,
    updated_at = CURRENT_TIMESTAMP;

-- 插入默认角色
INSERT INTO roles (id, name, description, is_system) VALUES
    ('01H000000000000000000000R1', 'admin', '系统管理员', TRUE),
    ('01H000000000000000000000R2', 'operator', '操作员', TRUE),
    ('01H000000000000000000000R3', 'viewer', '只读用户', TRUE)
ON CONFLICT (name) DO NOTHING;

-- 为管理员角色分配所有权限
INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id FROM roles r, permissions p WHERE r.name = 'admin'
ON CONFLICT DO NOTHING;

-- 为操作员角色分配基础权限
INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id FROM roles r, permissions p 
WHERE r.name = 'operator' AND p.name IN (
    'user:read',
    'flight:read',
    'flight:manage',
    'flight.read',
    'flight.update',
    'flight.timeline_edit',
    'flight.report_generate',
    'business_case.create',
    'business_case.read',
    'business_case.append',
    'business_case.update',
    'business_case.status_transition',
    'workflow_run.start',
    'workflow_run.read',
    'workflow_run.act',
    'ai:chat',
    'ai:view',
    'todo:manage'
)
ON CONFLICT DO NOTHING;

-- 为只读用户角色分配查看权限
INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id FROM roles r, permissions p 
WHERE r.name = 'viewer' AND p.name IN (
    'user:read',
    'flight:read',
    'flight.read',
    'business_case.read',
    'workflow_run.read'
)
ON CONFLICT DO NOTHING;

-- 兼容旧粗粒度角色授权，回填到细粒度 resource.action 权限。
-- 对全新空库初始化基本是 no-op；对已有旧权限数据的库重复执行 setup 时会自动对齐到 066 的最终授权状态。
WITH legacy_permission_mapping(legacy_name, granular_name) AS (
    VALUES
        ('flight:read', 'flight.read'),
        ('flight:read', 'business_case.read'),
        ('flight:read', 'workflow_run.read'),
        ('flight:manage', 'flight.read'),
        ('flight:manage', 'flight.update'),
        ('flight:manage', 'flight.timeline_edit'),
        ('flight:manage', 'flight.import_commit'),
        ('flight:manage', 'flight.report_generate'),
        ('flight:manage', 'business_case.create'),
        ('flight:manage', 'business_case.read'),
        ('flight:manage', 'business_case.append'),
        ('flight:manage', 'business_case.update'),
        ('flight:manage', 'business_case.status_transition'),
        ('flight:manage', 'business_case.delete'),
        ('flight:manage', 'workflow_run.start'),
        ('flight:manage', 'workflow_run.read'),
        ('flight:manage', 'workflow_run.act'),
        ('dispatch:view', 'dispatch_order.read'),
        ('dispatch:view', 'dispatch_catalog.read'),
        ('dispatch:view', 'shift_handover.read'),
        ('dispatch:view', 'notification.read'),
        ('dispatch:view', 'notification.receipt_read'),
        ('dispatch:manage', 'dispatch_order.read'),
        ('dispatch:manage', 'dispatch_order.create'),
        ('dispatch:manage', 'dispatch_order.update'),
        ('dispatch:manage', 'dispatch_order.publish'),
        ('dispatch:manage', 'dispatch_order.cancel'),
        ('dispatch:manage', 'dispatch_catalog.read'),
        ('dispatch:manage', 'dispatch_catalog.edit'),
        ('dispatch:manage', 'shift_handover.read'),
        ('dispatch:manage', 'shift_handover.create'),
        ('dispatch:manage', 'shift_handover.submit'),
        ('dispatch:manage', 'shift_handover.ack'),
        ('dispatch:manage', 'notification.read'),
        ('dispatch:manage', 'notification.send'),
        ('dispatch:manage', 'notification.receipt_read'),
        ('dispatch:manage', 'notification.receipt_manage'),
        ('flowable:read', 'workflow_definition.read'),
        ('flowable:read', 'workflow_run.read'),
        ('flowable:manage', 'workflow_definition.read'),
        ('flowable:manage', 'workflow_definition.edit'),
        ('flowable:manage', 'workflow_definition.publish'),
        ('flowable:manage', 'workflow_definition.deprecate'),
        ('user:read', 'user_admin.read'),
        ('user:create', 'user_admin.edit'),
        ('user:update', 'user_admin.edit'),
        ('user:delete', 'user_admin.edit'),
        ('role:read', 'auth_role.read'),
        ('role:create', 'auth_role.edit'),
        ('role:update', 'auth_role.edit'),
        ('role:delete', 'auth_role.edit'),
        ('auth:view', 'user_admin.read'),
        ('auth:view', 'auth_role.read'),
        ('auth:view', 'auth_permission_template.read'),
        ('auth:manage', 'user_admin.read'),
        ('auth:manage', 'user_admin.edit'),
        ('auth:manage', 'auth_role.read'),
        ('auth:manage', 'auth_role.edit'),
        ('auth:manage', 'auth_permission_template.read'),
        ('auth:manage', 'auth_permission_template.edit'),
        ('system:admin', 'system.config_read'),
        ('system:admin', 'system.config_write'),
        ('system:admin', 'system.ops_admin')
)
INSERT INTO role_permissions (role_id, permission_id)
SELECT DISTINCT rp.role_id, granular_permission.id
FROM role_permissions rp
JOIN permissions legacy_permission
    ON legacy_permission.id = rp.permission_id
JOIN legacy_permission_mapping mapping
    ON mapping.legacy_name = legacy_permission.name
JOIN permissions granular_permission
    ON granular_permission.name = mapping.granular_name
ON CONFLICT DO NOTHING;

-- 默认不再注入固定管理员账号。
-- 仅当 PostgreSQL 启动参数显式设置 flight_monitor.seed_default_admin=true 时，
-- 才允许创建一次性 bootstrap 管理员，避免生产环境长期保留仓库内置高权限身份。
DO $$
BEGIN
    IF lower(coalesce(current_setting('flight_monitor.seed_default_admin', true), 'false')) = 'true' THEN
        INSERT INTO users (id, username, display_name, email, password_hash, is_active, is_verified, is_admin) VALUES
            ('01H000000000000000000000A1', 'admin', 'admin', 'admin@localhost', '$2a$12$F0TTpauqUDxpPyYAktaflONn6JSg2AoqV4ITOxe1SQ38Ku5HKrxXa', TRUE, TRUE, TRUE)
        ON CONFLICT (username) DO NOTHING;

        INSERT INTO user_roles (user_id, role_id)
        SELECT u.id, r.id FROM users u, roles r WHERE u.username = 'admin' AND r.name = 'admin'
        ON CONFLICT DO NOTHING;
    END IF;
END $$;

-- 插入默认配置
INSERT INTO system_config (key, value, description) VALUES
    ('system.version', '"1.0.0"', '系统版本'),
    ('system.initialized', 'true', '系统初始化标记')
ON CONFLICT (key) DO NOTHING;

-- =====================================================
-- Agent 执行表 (agent_executions)
-- =====================================================
CREATE TABLE IF NOT EXISTS agent_executions (
    id SERIAL PRIMARY KEY,
    run_id VARCHAR(26) NOT NULL UNIQUE,
    todo_id VARCHAR(26) NOT NULL,
    entity_id VARCHAR(255) NOT NULL,
    entity_config JSONB NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'pending',
    total_steps INTEGER DEFAULT 0,
    total_tokens INTEGER DEFAULT 0,
    total_tool_calls INTEGER DEFAULT 0,
    started_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    finished_at TIMESTAMP WITH TIME ZONE,
    error_message TEXT,
    metadata JSONB DEFAULT '{}',
    CONSTRAINT valid_agent_execution_status CHECK (
        status IN ('pending', 'running', 'completed', 'failed', 'cancelled')
    )
);

-- Agent执行表索引
CREATE INDEX IF NOT EXISTS idx_agent_exec_run_id ON agent_executions(run_id);
CREATE INDEX IF NOT EXISTS idx_agent_exec_todo_id ON agent_executions(todo_id);
CREATE INDEX IF NOT EXISTS idx_agent_exec_entity_id ON agent_executions(entity_id);
CREATE INDEX IF NOT EXISTS idx_agent_exec_status ON agent_executions(status);
CREATE INDEX IF NOT EXISTS idx_agent_exec_started_at ON agent_executions(started_at);

COMMENT ON TABLE agent_executions IS 'AI Agent执行记录，跟踪TODO的Agent执行情况';
COMMENT ON COLUMN agent_executions.run_id IS 'Agent运行唯一标识';
COMMENT ON COLUMN agent_executions.entity_config IS '执行时的AI实体配置快照';

-- =====================================================
-- Agent 执行作业类型表 (agent_steps)
-- =====================================================
CREATE TABLE IF NOT EXISTS agent_steps (
    id SERIAL PRIMARY KEY,
    step_id VARCHAR(26) NOT NULL UNIQUE,
    run_id VARCHAR(26) NOT NULL REFERENCES agent_executions(run_id) ON DELETE CASCADE,
    sequence INTEGER NOT NULL,
    step_type VARCHAR(50) NOT NULL,
    role VARCHAR(20) NOT NULL,
    content TEXT,
    tool_calls JSONB,
    token_usage JSONB,
    latency_ms INTEGER,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    metadata JSONB DEFAULT '{}',
    CONSTRAINT valid_step_type CHECK (
        step_type IN ('user_input', 'ai_response', 'tool_call', 'tool_result', 'system')
    ),
    UNIQUE(run_id, sequence)
);

-- Agent作业类型表索引
CREATE INDEX IF NOT EXISTS idx_agent_steps_step_id ON agent_steps(step_id);
CREATE INDEX IF NOT EXISTS idx_agent_steps_run_id ON agent_steps(run_id);
CREATE INDEX IF NOT EXISTS idx_agent_steps_sequence ON agent_steps(run_id, sequence);

COMMENT ON TABLE agent_steps IS 'Agent执行作业类型，记录每次AI请求/响应/工具调用';

-- =====================================================
-- AI 人工审批动作表 (ai_pending_actions)
-- =====================================================
CREATE TABLE IF NOT EXISTS ai_pending_actions (
    id SERIAL PRIMARY KEY,
    action_id VARCHAR(64) NOT NULL UNIQUE,
    tool_call_id VARCHAR(64) NOT NULL,
    tool_name VARCHAR(128) NOT NULL,
    arguments TEXT NOT NULL,
    operation_level VARCHAR(64) NOT NULL,
    invocation_mode VARCHAR(64) NOT NULL,
    requester_user_id VARCHAR(255),
    requester_user_roles JSONB NOT NULL DEFAULT '[]'::jsonb,
    reason TEXT NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    approved_by VARCHAR(255),
    approved_at TIMESTAMP WITH TIME ZONE,
    rejected_by VARCHAR(255),
    rejected_reason TEXT,
    rejected_at TIMESTAMP WITH TIME ZONE,
    execution_result JSONB,
    execution_error TEXT,
    expires_at TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT valid_ai_pending_action_status CHECK (
        status IN ('pending', 'approved', 'rejected', 'executed', 'failed', 'expired')
    )
);

CREATE INDEX IF NOT EXISTS idx_ai_pending_actions_status ON ai_pending_actions(status);
CREATE INDEX IF NOT EXISTS idx_ai_pending_actions_tool_name ON ai_pending_actions(tool_name);
CREATE INDEX IF NOT EXISTS idx_ai_pending_actions_created_at ON ai_pending_actions(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_ai_pending_actions_expires_at
    ON ai_pending_actions(expires_at)
    WHERE status = 'pending' AND expires_at IS NOT NULL;

COMMENT ON TABLE ai_pending_actions IS 'AI 自动触发后需要人工审批的工具动作队列';
COMMENT ON COLUMN ai_pending_actions.action_id IS '审批动作唯一标识';
COMMENT ON COLUMN ai_pending_actions.arguments IS '工具参数原文（JSON字符串）';

-- 创建 todos 表层级/来源字段的索引（字段已在建表时声明）
CREATE INDEX IF NOT EXISTS idx_todos_parent_todo_id ON todos(parent_todo_id);
CREATE INDEX IF NOT EXISTS idx_todos_source ON todos(source_type, source_id);

-- =====================================================
-- Agent 执行作业类型表 (agent_steps)
-- =====================================================
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'agent_steps' AND column_name = 'thinking') THEN
        ALTER TABLE agent_steps ADD COLUMN thinking TEXT;
    END IF;
    
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'agent_steps' AND column_name = 'decision_summary') THEN
        ALTER TABLE agent_steps ADD COLUMN decision_summary VARCHAR(500);
    END IF;
END $$;

-- =====================================================
-- AI 实体配置表
-- =====================================================
CREATE TABLE IF NOT EXISTS ai_entities (
    id VARCHAR(255) PRIMARY KEY,
    config JSONB NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- 插入默认 AI 实体配置 (v6.0 完整结构)
INSERT INTO ai_entities (id, config) VALUES (
    'default',
    '{
        "api_key": "",
        "base_url": "https://api.openai.com/v1",
        "default_model": "gpt-3.5-turbo",
        "temperature": 0.7,
        "max_tokens": 2000,
        "top_p": 0.95,
        "frequency_penalty": 0.0,
        "presence_penalty": 0.0,
        "timeout": 30.0,
        "max_retries": 3,
        "retry_delay": 0.5,
        "cost_per_1k_input": 0.0015,
        "cost_per_1k_output": 0.002,
        "context_window": 128000,
        "tools": {
            "timeout": 30,
            "max_retries": 3,
            "retry_delay": 1.0,
            "auto_execute": true
        },
        "monitoring": {
            "metrics_enabled": true,
            "trace_enabled": false,
            "log_prompts": false,
            "mask_sensitive": true
        },
        "endpoints": {
            "chat": null,
            "vision": null,
            "asr": null,
            "tts": null
        },
        "allowed_tool_categories": ["flight", "flight_event", "todo", "business_case"],
        "allowed_tools": null,
        "denied_tools": [],
        "system_prompt": "你是一个航班监控系统的AI助手，可以帮助用户查询航班信息、管理航班事件和待办事项。",
        "task_template": null
    }'::jsonb
) ON CONFLICT (id) DO UPDATE SET
    config = ai_entities.config || EXCLUDED.config,
    updated_at = CURRENT_TIMESTAMP;

-- =====================================================
-- AI 全局状态表 (Migration 001)
-- =====================================================
CREATE TABLE IF NOT EXISTS ai_global_state (
    id VARCHAR(255) PRIMARY KEY,
    state JSONB NOT NULL,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO ai_global_state (id, state)
VALUES ('overview', '{}'::jsonb)
ON CONFLICT (id) DO NOTHING;


-- =====================================================
-- Migration 004: Create Archive Tables & pgAgent Jobs
-- =====================================================

-- 1. Create Archive Tables
-- =====================================================

-- 1.1 Archived Flights (Master Table)
CREATE TABLE IF NOT EXISTS archived_flights (LIKE flights INCLUDING ALL);
ALTER TABLE archived_flights ADD COLUMN IF NOT EXISTS archived_at TIMESTAMPTZ DEFAULT NOW();
CREATE INDEX IF NOT EXISTS idx_archived_flights_flight_id ON archived_flights(flight_id);
CREATE INDEX IF NOT EXISTS idx_archived_flights_archived_at ON archived_flights(archived_at);

-- 1.2 Archived Flight State Changes
CREATE TABLE IF NOT EXISTS archived_flight_state_changes (LIKE flight_state_changes INCLUDING ALL);
ALTER TABLE archived_flight_state_changes ADD COLUMN IF NOT EXISTS archived_at TIMESTAMPTZ DEFAULT NOW();
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'flight_state_changes_flight_id_fkey' AND conrelid = 'archived_flight_state_changes'::regclass) THEN
        ALTER TABLE archived_flight_state_changes DROP CONSTRAINT flight_state_changes_flight_id_fkey;
    END IF;
END $$;
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'fk_archived_fsc_flight'
          AND conrelid = 'archived_flight_state_changes'::regclass
    ) THEN
        ALTER TABLE archived_flight_state_changes
            ADD CONSTRAINT fk_archived_fsc_flight
            FOREIGN KEY (flight_id) REFERENCES archived_flights(flight_id) ON DELETE CASCADE;
    END IF;
END $$;

-- 1.3 Archived Flight Business Cases
CREATE TABLE IF NOT EXISTS archived_flight_business_cases (LIKE flight_business_cases INCLUDING ALL);
ALTER TABLE archived_flight_business_cases ADD COLUMN IF NOT EXISTS archived_at TIMESTAMPTZ DEFAULT NOW();
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'flight_business_cases_flight_id_fkey' AND conrelid = 'archived_flight_business_cases'::regclass) THEN
        ALTER TABLE archived_flight_business_cases DROP CONSTRAINT flight_business_cases_flight_id_fkey;
    END IF;
END $$;
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'fk_archived_fbc_flight'
          AND conrelid = 'archived_flight_business_cases'::regclass
    ) THEN
        ALTER TABLE archived_flight_business_cases
            ADD CONSTRAINT fk_archived_fbc_flight
            FOREIGN KEY (flight_id) REFERENCES archived_flights(flight_id) ON DELETE CASCADE;
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_afbc_policy_interception_error_code
    ON archived_flight_business_cases ((context->>'error_code'))
    WHERE case_type = 'policy_interception';

-- 1.4 Archived Snapshots
CREATE TABLE IF NOT EXISTS archived_snapshots (LIKE snapshots INCLUDING ALL);
ALTER TABLE archived_snapshots ADD COLUMN IF NOT EXISTS archived_at TIMESTAMPTZ DEFAULT NOW();
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'snapshots_flight_id_fkey' AND conrelid = 'archived_snapshots'::regclass) THEN
        ALTER TABLE archived_snapshots DROP CONSTRAINT snapshots_flight_id_fkey;
    END IF;
END $$;
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'fk_archived_snapshot_flight'
          AND conrelid = 'archived_snapshots'::regclass
    ) THEN
        ALTER TABLE archived_snapshots
            ADD CONSTRAINT fk_archived_snapshot_flight
            FOREIGN KEY (flight_id) REFERENCES archived_flights(flight_id) ON DELETE CASCADE;
    END IF;
END $$;

-- 1.5 Archived Event Stream Versions
CREATE TABLE IF NOT EXISTS archived_event_stream_versions (LIKE event_stream_versions INCLUDING ALL);
ALTER TABLE archived_event_stream_versions ADD COLUMN IF NOT EXISTS archived_at TIMESTAMPTZ DEFAULT NOW();
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'event_stream_versions_flight_id_fkey' AND conrelid = 'archived_event_stream_versions'::regclass) THEN
        ALTER TABLE archived_event_stream_versions DROP CONSTRAINT event_stream_versions_flight_id_fkey;
    END IF;
END $$;
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'fk_archived_esv_flight'
          AND conrelid = 'archived_event_stream_versions'::regclass
    ) THEN
        ALTER TABLE archived_event_stream_versions
            ADD CONSTRAINT fk_archived_esv_flight
            FOREIGN KEY (flight_id) REFERENCES archived_flights(flight_id) ON DELETE CASCADE;
    END IF;
END $$;


-- 2. Archive Stored Procedure
-- =====================================================

CREATE OR REPLACE FUNCTION archive_flight_data(
    p_cutoff_date DATE,       
    p_target_date DATE DEFAULT NULL
) RETURNS JSONB AS $$
DECLARE
    v_flight_ids VARCHAR[];
    v_count INT;
    v_source_table TEXT;
    v_target_table TEXT;
    v_columns TEXT;
BEGIN
    -- 1. Identify Flight IDs to archive
    SELECT ARRAY_AGG(flight_id) INTO v_flight_ids
    FROM flights
    WHERE (p_target_date IS NOT NULL AND workspace_date = p_target_date)
       OR (p_target_date IS NULL AND workspace_date < p_cutoff_date); 

    IF v_flight_ids IS NULL OR array_length(v_flight_ids, 1) IS NULL THEN
        RETURN jsonb_build_object('status', 'no_data', 'archived_count', 0);
    END IF;

    v_count := array_length(v_flight_ids, 1);

    -- 2. Move to archive tables with schema-safe dynamic columns.
    FOR v_source_table, v_target_table IN
        SELECT *
        FROM unnest(
            ARRAY[
                'flights',
                'flight_state_changes',
                'flight_business_cases',
                'snapshots',
                'event_stream_versions'
            ]::TEXT[],
            ARRAY[
                'archived_flights',
                'archived_flight_state_changes',
                'archived_flight_business_cases',
                'archived_snapshots',
                'archived_event_stream_versions'
            ]::TEXT[]
        )
    LOOP
        SELECT STRING_AGG(quote_ident(c.column_name), ', ' ORDER BY c.ordinal_position)
          INTO v_columns
        FROM information_schema.columns c
        WHERE c.table_schema = 'public'
          AND c.table_name = v_target_table
          AND c.column_name <> 'archived_at'
          AND COALESCE(c.is_generated, 'NEVER') = 'NEVER'
          AND EXISTS (
              SELECT 1
              FROM information_schema.columns s
              WHERE s.table_schema = 'public'
                AND s.table_name = v_source_table
                AND s.column_name = c.column_name
                AND COALESCE(s.is_generated, 'NEVER') = 'NEVER'
          );

        IF v_columns IS NULL OR btrim(v_columns) = '' THEN
            RAISE EXCEPTION 'Archive column resolution failed for target=% source=%', v_target_table, v_source_table;
        END IF;

        EXECUTE format(
            'INSERT INTO %I (%s, archived_at) ' ||
            'SELECT %s, NOW() FROM %I WHERE flight_id = ANY($1) ON CONFLICT DO NOTHING',
            v_target_table,
            v_columns,
            v_columns,
            v_source_table
        )
        USING v_flight_ids;
    END LOOP;

    -- 3. Delete from Active Tables
    DELETE FROM flight_business_cases WHERE flight_id = ANY(v_flight_ids);
    DELETE FROM flight_state_changes WHERE flight_id = ANY(v_flight_ids); 
    DELETE FROM snapshots WHERE flight_id = ANY(v_flight_ids);
    DELETE FROM event_stream_versions WHERE flight_id = ANY(v_flight_ids);
    DELETE FROM flights WHERE flight_id = ANY(v_flight_ids);
    
    RETURN jsonb_build_object('status', 'success', 'archived_count', v_count, 'flight_ids', v_flight_ids);
END;
$$ LANGUAGE plpgsql;


-- 3. pgAgent Jobs Setup
-- =====================================================

DO $$
DECLARE
    jid integer;
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.schemata WHERE schema_name = 'pgagent') THEN
        
        -- Job 1: Clean OLD History (Daily at 00:00)
        IF NOT EXISTS (SELECT 1 FROM pgagent.pga_job WHERE jobname = 'Archive_Old_Flights') THEN
            INSERT INTO pgagent.pga_job (jobjclid, jobname, jobdesc, jobhostagent, jobenabled)
            VALUES (1, 'Archive_Old_Flights', 'Archives flights older than yesterday', '', true)
            RETURNING jobid INTO jid;

            INSERT INTO pgagent.pga_jobstep (jstjobid, jstname, jstkind, jstdbname, jstcode, jstenabled)
            VALUES (jid, 'Call Archive Function', 's', 'flight_monitor_dev', 
                    'SELECT archive_flight_data((CURRENT_DATE - INTERVAL ''1 day'')::DATE, NULL);', true);

            INSERT INTO pgagent.pga_schedule (jscjobid, jscname, jscdesc, jscenabled, 
                                            jscstart, jscminutes, jschours, jscweekdays, jscmonthdays, jscmonths)
            VALUES (jid, 'Daily_00_00', 'Run daily at midnight', true, 
                    NOW(), '{t,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f}', 
                    '{t,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f}', 
                    '{t,t,t,t,t,t,t}', '{t,t,t,t,t,t,t,t,t,t,t,t,t,t,t,t,t,t,t,t,t,t,t,t,t,t,t,t,t,t,t,t}', '{t,t,t,t,t,t,t,t,t,t,t,t}');
        END IF;
        
        -- Job 2: Archive Yesterday (Daily at 05:00)
        IF NOT EXISTS (SELECT 1 FROM pgagent.pga_job WHERE jobname = 'Archive_Yesterday_Flights') THEN
            INSERT INTO pgagent.pga_job (jobjclid, jobname, jobdesc, jobhostagent, jobenabled)
            VALUES (1, 'Archive_Yesterday_Flights', 'Archives flights from yesterday', '', true)
            RETURNING jobid INTO jid;

            INSERT INTO pgagent.pga_jobstep (jstjobid, jstname, jstkind, jstdbname, jstcode, jstenabled)
            VALUES (jid, 'Call Archive Function', 's', 'flight_monitor_dev', 
                    'SELECT archive_flight_data(NULL, (CURRENT_DATE - INTERVAL ''1 day'')::DATE);', true);

            INSERT INTO pgagent.pga_schedule (jscjobid, jscname, jscdesc, jscenabled, 
                                            jscstart, jscminutes, jschours, jscweekdays, jscmonthdays, jscmonths)
            VALUES (jid, 'Daily_05_00', 'Run daily at 5 AM', true, 
                    NOW(), '{t,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f}', 
                    '{f,f,f,f,f,t,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f,f}', 
                    '{t,t,t,t,t,t,t}', '{t,t,t,t,t,t,t,t,t,t,t,t,t,t,t,t,t,t,t,t,t,t,t,t,t,t,t,t,t,t,t,t}', '{t,t,t,t,t,t,t,t,t,t,t,t}');
        END IF;

    END IF;
END $$;

-- =====================================================
-- 派工系统表 (Migration 007)
-- =====================================================

-- 科室表（与现有 users.department 集成）
CREATE TABLE IF NOT EXISTS departments (
    id VARCHAR(26) PRIMARY KEY,
    name VARCHAR(100) NOT NULL UNIQUE,
    code VARCHAR(20) UNIQUE,
    description TEXT,
    manager_id VARCHAR(26) REFERENCES users(id),
    terminal VARCHAR(20),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    is_active BOOLEAN DEFAULT TRUE
);

CREATE INDEX IF NOT EXISTS idx_departments_name ON departments(name);
CREATE INDEX IF NOT EXISTS idx_departments_terminal ON departments(terminal);

-- 班组类型表
CREATE TABLE IF NOT EXISTS team_types (
    id VARCHAR(26) PRIMARY KEY,
    department_id VARCHAR(26) REFERENCES departments(id),
    name VARCHAR(100) NOT NULL,
    code VARCHAR(20) UNIQUE,
    description TEXT,
    color VARCHAR(7),
    is_driver_type BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    is_active BOOLEAN DEFAULT TRUE,
    UNIQUE(department_id, name)
);

CREATE INDEX IF NOT EXISTS idx_team_types_department ON team_types(department_id);

-- 机位表
CREATE TABLE IF NOT EXISTS stands (
    id VARCHAR(26) PRIMARY KEY,
    code VARCHAR(20) NOT NULL UNIQUE,
    name VARCHAR(100),
    terminal VARCHAR(20),
    area VARCHAR(20),
    position_lat DECIMAL(10, 7) NOT NULL,
    position_lng DECIMAL(10, 7) NOT NULL,
    stand_type VARCHAR(20),
    size_category VARCHAR(10),
    is_active BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_stands_code ON stands(code);
CREATE INDEX IF NOT EXISTS idx_stands_terminal ON stands(terminal);

-- 班组表
CREATE TABLE IF NOT EXISTS teams (
    id VARCHAR(26) PRIMARY KEY,
    team_type_id VARCHAR(26) REFERENCES team_types(id),
    name VARCHAR(100) NOT NULL,
    code VARCHAR(20) UNIQUE,
    leader_id VARCHAR(26) REFERENCES users(id),
    terminal VARCHAR(20),
    current_status VARCHAR(20) DEFAULT 'off_duty',
    current_position_lat DECIMAL(10, 7),
    current_position_lng DECIMAL(10, 7),
    current_stand_id VARCHAR(26) REFERENCES stands(id),
    last_position_update TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    is_active BOOLEAN DEFAULT TRUE
);

CREATE INDEX IF NOT EXISTS idx_teams_type ON teams(team_type_id);
CREATE INDEX IF NOT EXISTS idx_teams_status ON teams(current_status);
CREATE INDEX IF NOT EXISTS idx_teams_terminal ON teams(terminal);

-- 班组成员表
CREATE TABLE IF NOT EXISTS team_members (
    id VARCHAR(26) PRIMARY KEY,
    team_id VARCHAR(26) REFERENCES teams(id) ON DELETE CASCADE,
    user_id VARCHAR(26) REFERENCES users(id),
    role VARCHAR(20) DEFAULT 'member',
    can_drive BOOLEAN DEFAULT FALSE,
    joined_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    left_at TIMESTAMP WITH TIME ZONE,
    is_active BOOLEAN DEFAULT TRUE,
    UNIQUE(team_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_team_members_team ON team_members(team_id);
CREATE INDEX IF NOT EXISTS idx_team_members_user ON team_members(user_id);

-- 班组类型-作业类型能力表
CREATE TABLE IF NOT EXISTS team_type_steps (
    team_type_id VARCHAR(26) REFERENCES team_types(id) ON DELETE CASCADE,
    task_type VARCHAR(50) NOT NULL,
    priority INT DEFAULT 0,
    PRIMARY KEY (team_type_id, task_type)
);

-- 设备类型表
CREATE TABLE IF NOT EXISTS equipment_types (
    id VARCHAR(26) PRIMARY KEY,
    name VARCHAR(100) NOT NULL UNIQUE,
    code VARCHAR(20) UNIQUE,
    category VARCHAR(50),
    requires_driver BOOLEAN DEFAULT FALSE,
    driver_team_type_id VARCHAR(26) REFERENCES team_types(id),
    icon VARCHAR(100),
    description TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    is_active BOOLEAN DEFAULT TRUE
);

CREATE INDEX IF NOT EXISTS idx_equipment_types_category ON equipment_types(category);

-- 设备类型-作业类型需求表
CREATE TABLE IF NOT EXISTS equipment_type_steps (
    equipment_type_id VARCHAR(26) REFERENCES equipment_types(id) ON DELETE CASCADE,
    task_type VARCHAR(50) NOT NULL,
    min_count INT DEFAULT 1,
    max_count INT DEFAULT 1,
    is_required BOOLEAN DEFAULT TRUE,
    PRIMARY KEY (equipment_type_id, task_type)
);

-- 设备表
CREATE TABLE IF NOT EXISTS equipment (
    id VARCHAR(26) PRIMARY KEY,
    equipment_type_id VARCHAR(26) REFERENCES equipment_types(id),
    code VARCHAR(50) NOT NULL UNIQUE,
    name VARCHAR(100),
    license_plate VARCHAR(20),
    terminal VARCHAR(20),
    status VARCHAR(20) DEFAULT 'available',
    current_position_lat DECIMAL(10, 7),
    current_position_lng DECIMAL(10, 7),
    current_stand_id VARCHAR(26) REFERENCES stands(id),
    last_position_update TIMESTAMP WITH TIME ZONE,
    current_dispatch_id VARCHAR(26),
    last_maintenance_date DATE,
    next_maintenance_date DATE,
    metadata JSONB,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    is_active BOOLEAN DEFAULT TRUE
);

CREATE INDEX IF NOT EXISTS idx_equipment_type ON equipment(equipment_type_id);
CREATE INDEX IF NOT EXISTS idx_equipment_status ON equipment(status);
CREATE INDEX IF NOT EXISTS idx_equipment_terminal ON equipment(terminal);

-- 作业类型定义表
CREATE TABLE IF NOT EXISTS task_types (
    id VARCHAR(26) PRIMARY KEY,
    code VARCHAR(50) NOT NULL UNIQUE,
    name VARCHAR(100) NOT NULL,
    category VARCHAR(50),
    sequence_order INT,
    default_duration_minutes INT,
    trigger_offset_minutes INT DEFAULT 30,
    trigger_type VARCHAR(20) DEFAULT 'before_eta',
    description TEXT,
    is_active BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- 预置作业类型
INSERT INTO task_types (id, code, name, category, sequence_order, trigger_type, trigger_offset_minutes, default_duration_minutes) VALUES
    ('step_001', 'wheel_chocks_on', '上轮挡', 'arrival', 1, 'after_arrival', 0, 2),
    ('step_002', 'cabin_door_open', '开客舱门', 'arrival', 2, 'after_arrival', 2, 3),
    ('step_003', 'deboarding', '旅客下机', 'arrival', 3, 'after_arrival', 5, 15),
    ('step_004', 'cleaning', '客舱清洁', 'turnaround', 4, 'after_arrival', 20, 25),
    ('step_005', 'catering', '配餐', 'turnaround', 5, 'before_etd', 60, 20),
    ('step_006', 'boarding', '旅客登机', 'departure', 6, 'before_etd', 40, 25),
    ('step_007', 'cargo_loading', '行李装载', 'departure', 7, 'before_etd', 30, 20),
    ('step_008', 'cabin_door_close', '关客舱门', 'departure', 8, 'before_etd', 10, 3),
    ('step_009', 'cargo_door_close', '关货舱门', 'departure', 9, 'before_etd', 8, 2),
    ('step_010', 'pushback', '推出/牵引', 'departure', 10, 'before_etd', 5, 5),
    ('step_011', 'wheel_chocks_off', '撤轮挡', 'departure', 11, 'before_etd', 3, 2)
ON CONFLICT (id) DO NOTHING;

-- 派工单表
CREATE TABLE IF NOT EXISTS dispatch_orders (
    id VARCHAR(26) PRIMARY KEY,
    flight_id VARCHAR(26) NOT NULL,
    task_type VARCHAR(50) NOT NULL,
    stand_id VARCHAR(26) REFERENCES stands(id),
    assignee_type VARCHAR(20) NOT NULL,
    team_id VARCHAR(26) REFERENCES teams(id),
    individual_user_id VARCHAR(26) REFERENCES users(id),
    driver_type VARCHAR(20),
    driver_team_id VARCHAR(26) REFERENCES teams(id),
    driver_user_id VARCHAR(26) REFERENCES users(id),
    planned_start_time TIMESTAMP WITH TIME ZONE,
    planned_end_time TIMESTAMP WITH TIME ZONE,
    actual_start_time TIMESTAMP WITH TIME ZONE,
    actual_end_time TIMESTAMP WITH TIME ZONE,
    estimated_completion_time TIMESTAMP WITH TIME ZONE,
    estimated_completion_reported_by VARCHAR(26),
    estimated_completion_reported_at TIMESTAMP WITH TIME ZONE,
    estimated_completion_note TEXT,
    status VARCHAR(20) DEFAULT 'pending',
    dispatch_type VARCHAR(20) DEFAULT 'auto',
    dispatched_at TIMESTAMP WITH TIME ZONE,
    dispatched_by VARCHAR(26) REFERENCES users(id),
    snapshot_assignee_position JSONB,
    snapshot_equipment_positions JSONB,
    estimated_arrival_minutes INT,
    process_instance_id VARCHAR(64),
    process_task_id VARCHAR(64),
    workflow_context JSONB DEFAULT '{}'::jsonb,
    workflow_status VARCHAR(20) DEFAULT 'pending_assignment',
    source VARCHAR(20) DEFAULT 'system',
    recommended_assignees JSONB,
    recommendation_score NUMERIC(5, 2),
    supervisor_notified BOOLEAN DEFAULT FALSE,
    supervisor_notified_at TIMESTAMP WITH TIME ZONE,
    assignment_deadline TIMESTAMP WITH TIME ZONE,
    completed_by VARCHAR(26) REFERENCES users(id),
    completion_notes TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(flight_id, task_type),
    CHECK (
        (assignee_type = 'team' AND team_id IS NOT NULL AND individual_user_id IS NULL) OR
        (assignee_type = 'individual' AND individual_user_id IS NOT NULL AND team_id IS NULL)
    )
);

CREATE INDEX IF NOT EXISTS idx_dispatch_orders_flight ON dispatch_orders(flight_id);
CREATE INDEX IF NOT EXISTS idx_dispatch_orders_status ON dispatch_orders(status);
CREATE INDEX IF NOT EXISTS idx_dispatch_orders_team ON dispatch_orders(team_id);
CREATE INDEX IF NOT EXISTS idx_dispatch_orders_planned_time ON dispatch_orders(planned_start_time);
CREATE INDEX IF NOT EXISTS idx_dispatch_orders_process_instance ON dispatch_orders(process_instance_id);
CREATE INDEX IF NOT EXISTS idx_dispatch_orders_source ON dispatch_orders(source);
CREATE INDEX IF NOT EXISTS idx_dispatch_orders_workflow_status ON dispatch_orders(workflow_status);
CREATE INDEX IF NOT EXISTS idx_dispatch_orders_estimated_completion_time
    ON dispatch_orders(estimated_completion_time);

COMMENT ON COLUMN dispatch_orders.estimated_completion_time IS '一线回报的预计完成时间';
COMMENT ON COLUMN dispatch_orders.estimated_completion_reported_by IS '预计完成时间回报人';
COMMENT ON COLUMN dispatch_orders.estimated_completion_reported_at IS '预计完成时间回报时间';
COMMENT ON COLUMN dispatch_orders.estimated_completion_note IS '预计完成时间回报备注';

-- 派工单人员明细表
CREATE TABLE IF NOT EXISTS dispatch_order_members (
    id VARCHAR(26) PRIMARY KEY,
    dispatch_order_id VARCHAR(26) REFERENCES dispatch_orders(id) ON DELETE CASCADE,
    user_id VARCHAR(26) REFERENCES users(id),
    role VARCHAR(20) DEFAULT 'member',
    source_type VARCHAR(20) NOT NULL,
    source_team_id VARCHAR(26) REFERENCES teams(id),
    assigned_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    check_in_time TIMESTAMP WITH TIME ZONE,
    check_out_time TIMESTAMP WITH TIME ZONE,
    is_active BOOLEAN DEFAULT TRUE,
    UNIQUE(dispatch_order_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_dispatch_order_members_order ON dispatch_order_members(dispatch_order_id);
CREATE INDEX IF NOT EXISTS idx_dispatch_order_members_user ON dispatch_order_members(user_id);

-- 派工单设备关联表
CREATE TABLE IF NOT EXISTS dispatch_order_equipment (
    dispatch_order_id VARCHAR(26) REFERENCES dispatch_orders(id) ON DELETE CASCADE,
    equipment_id VARCHAR(26) REFERENCES equipment(id),
    assigned_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    released_at TIMESTAMP WITH TIME ZONE,
    PRIMARY KEY (dispatch_order_id, equipment_id)
);

-- 派工单操作日志
CREATE TABLE IF NOT EXISTS dispatch_order_logs (
    id VARCHAR(26) PRIMARY KEY,
    dispatch_order_id VARCHAR(26) REFERENCES dispatch_orders(id) ON DELETE CASCADE,
    action VARCHAR(50) NOT NULL,
    actor_id VARCHAR(26) REFERENCES users(id),
    details JSONB,
    event_id VARCHAR(26),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_dispatch_order_logs_order ON dispatch_order_logs(dispatch_order_id);
CREATE INDEX IF NOT EXISTS idx_dispatch_order_logs_event_id ON dispatch_order_logs(event_id) WHERE event_id IS NOT NULL;

-- 派工安全检查清单模板
CREATE TABLE IF NOT EXISTS dispatch_safety_checklist_templates (
    template_id VARCHAR(26) PRIMARY KEY,
    task_type VARCHAR(50) NOT NULL REFERENCES task_types(code) ON DELETE CASCADE,
    checklist_version VARCHAR(32) NOT NULL,
    checklist_items JSONB NOT NULL DEFAULT '[]'::jsonb,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_by VARCHAR(26) REFERENCES users(id) ON DELETE SET NULL,
    updated_by VARCHAR(26) REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uq_dispatch_safety_template_step_version UNIQUE (task_type, checklist_version),
    CONSTRAINT chk_dispatch_safety_template_items_array CHECK (jsonb_typeof(checklist_items) = 'array')
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_dispatch_safety_template_active_step
    ON dispatch_safety_checklist_templates(task_type)
    WHERE is_active = TRUE;

CREATE INDEX IF NOT EXISTS idx_dispatch_safety_template_step_updated
    ON dispatch_safety_checklist_templates(task_type, updated_at DESC);

-- 派工安全检查项记录
CREATE TABLE IF NOT EXISTS dispatch_safety_checklist_records (
    record_id VARCHAR(26) PRIMARY KEY,
    dispatch_order_id VARCHAR(26) NOT NULL REFERENCES dispatch_orders(id) ON DELETE CASCADE,
    item_code VARCHAR(64) NOT NULL,
    result VARCHAR(16) NOT NULL,
    checked_by VARCHAR(26) REFERENCES users(id) ON DELETE SET NULL,
    checked_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    note TEXT,
    template_version VARCHAR(32),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uq_dispatch_safety_record_order_item UNIQUE (dispatch_order_id, item_code),
    CONSTRAINT chk_dispatch_safety_record_result CHECK (result IN ('pass', 'fail', 'na'))
);

CREATE INDEX IF NOT EXISTS idx_dispatch_safety_record_order_checked
    ON dispatch_safety_checklist_records(dispatch_order_id, checked_at DESC);

UPDATE dispatch_safety_checklist_templates
SET is_active = FALSE,
    updated_at = CURRENT_TIMESTAMP
WHERE task_type IN ('cleaning', 'boarding', 'pushback')
  AND is_active = TRUE;

INSERT INTO dispatch_safety_checklist_templates (
    template_id,
    task_type,
    checklist_version,
    checklist_items,
    is_active,
    created_by,
    updated_by,
    created_at,
    updated_at
) VALUES
(
    'dsl_tpl_cleaning_v1',
    'cleaning',
    'v1',
    '[
        {"item_code":"ppe","title":"PPE check","required":true,"allow_na":false,"order":1},
        {"item_code":"cabin_clear","title":"Cabin clear of tools","required":true,"allow_na":false,"order":2},
        {"item_code":"waste_sealed","title":"Waste sealed and tagged","required":true,"allow_na":false,"order":3}
    ]'::jsonb,
    TRUE,
    NULL,
    NULL,
    CURRENT_TIMESTAMP,
    CURRENT_TIMESTAMP
),
(
    'dsl_tpl_boarding_v1',
    'boarding',
    'v1',
    '[
        {"item_code":"door_zone_clear","title":"Door area clear","required":true,"allow_na":false,"order":1},
        {"item_code":"boarding_bridge_lock","title":"Bridge or stair lock check","required":true,"allow_na":false,"order":2},
        {"item_code":"final_manifest_sync","title":"Final manifest synced","required":true,"allow_na":true,"order":3}
    ]'::jsonb,
    TRUE,
    NULL,
    NULL,
    CURRENT_TIMESTAMP,
    CURRENT_TIMESTAMP
),
(
    'dsl_tpl_pushback_v1',
    'pushback',
    'v1',
    '[
        {"item_code":"towbar_lock","title":"Towbar lock check","required":true,"allow_na":false,"order":1},
        {"item_code":"chocks_removed","title":"Wheel chocks removed","required":true,"allow_na":false,"order":2},
        {"item_code":"ground_clearance","title":"Ground clearance confirmed","required":true,"allow_na":false,"order":3}
    ]'::jsonb,
    TRUE,
    NULL,
    NULL,
    CURRENT_TIMESTAMP,
    CURRENT_TIMESTAMP
)
ON CONFLICT (task_type, checklist_version) DO UPDATE SET
    checklist_items = EXCLUDED.checklist_items,
    is_active = EXCLUDED.is_active,
    updated_at = CURRENT_TIMESTAMP;

-- 流程派工映射表
CREATE TABLE IF NOT EXISTS workflow_dispatch_mappings (
    mapping_id VARCHAR(26) PRIMARY KEY,
    process_instance_id VARCHAR(64) NOT NULL,
    process_definition_key VARCHAR(100) NOT NULL,
    dispatch_order_id VARCHAR(26) REFERENCES dispatch_orders(id) ON DELETE SET NULL,
    business_key VARCHAR(100),
    flight_id VARCHAR(26),
    context_variables JSONB DEFAULT '{}'::jsonb,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_workflow_dispatch_mapping_process ON workflow_dispatch_mappings(process_instance_id);
CREATE INDEX IF NOT EXISTS idx_workflow_dispatch_mapping_dispatch ON workflow_dispatch_mappings(dispatch_order_id);
CREATE INDEX IF NOT EXISTS idx_workflow_dispatch_mapping_flight ON workflow_dispatch_mappings(flight_id);

-- 派工告警表
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
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_dispatch_alerts_resolved ON dispatch_alerts(is_resolved);
CREATE INDEX IF NOT EXISTS idx_dispatch_alerts_flight ON dispatch_alerts(flight_id);

-- users 表增加 department_id 外键
ALTER TABLE users ADD COLUMN IF NOT EXISTS department_id VARCHAR(26) REFERENCES departments(id);
CREATE INDEX IF NOT EXISTS idx_users_department_id ON users(department_id);

-- =====================================================
-- 权限模板表 (Migration 008)
-- =====================================================
CREATE TABLE IF NOT EXISTS permission_templates (
    id VARCHAR(26) PRIMARY KEY,
    name VARCHAR(100) NOT NULL UNIQUE,
    code VARCHAR(50) UNIQUE,
    description TEXT,
    permissions TEXT[] NOT NULL DEFAULT '{}',
    is_system BOOLEAN DEFAULT FALSE,
    category VARCHAR(50),
    display_order INT DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    is_active BOOLEAN DEFAULT TRUE
);

CREATE INDEX IF NOT EXISTS idx_permission_templates_category ON permission_templates(category);
CREATE INDEX IF NOT EXISTS idx_permission_templates_is_system ON permission_templates(is_system);
CREATE INDEX IF NOT EXISTS idx_permission_templates_display_order ON permission_templates(display_order);

COMMENT ON TABLE permission_templates IS '权限模板表';
COMMENT ON COLUMN permission_templates.code IS '模板代码，用于编程访问';
COMMENT ON COLUMN permission_templates.permissions IS '权限名称数组';
COMMENT ON COLUMN permission_templates.is_system IS '系统预设模板不可删除';
COMMENT ON COLUMN permission_templates.category IS '模板分类：dispatch, flight, workflow, user, system';

INSERT INTO permission_templates (id, name, code, description, permissions, is_system, category, display_order) VALUES
('tpl_dispatch_viewer', '派工查看员', 'dispatch_viewer',
 '只能查看派工单、班组和设备信息',
 ARRAY[
     'dispatch:view',
     'dispatch_order.read',
     'dispatch_catalog.read',
     'shift_handover.read',
     'notification.read',
     'notification.receipt_read',
     'team:view',
     'equipment:view'
 ], TRUE, 'dispatch', 1),

('tpl_dispatch_operator', '派工操作员', 'dispatch_operator',
 '可查看和执行派工操作，但不能管理班组和设备',
 ARRAY[
     'dispatch:view',
     'dispatch:manage',
     'dispatch_order.read',
     'dispatch_order.create',
     'dispatch_order.update',
     'dispatch_order.publish',
     'dispatch_order.cancel',
     'dispatch_catalog.read',
     'shift_handover.read',
     'shift_handover.create',
     'shift_handover.submit',
     'shift_handover.ack',
     'notification.read',
     'notification.send',
     'notification.receipt_read',
     'notification.receipt_manage',
     'team:view',
     'equipment:view'
 ], TRUE, 'dispatch', 2),

('tpl_dispatch_admin', '派工管理员', 'dispatch_admin',
 '完全的派工系统管理权限',
 ARRAY[
     'dispatch:view',
     'dispatch:manage',
     'dispatch_order.read',
     'dispatch_order.create',
     'dispatch_order.update',
     'dispatch_order.publish',
     'dispatch_order.cancel',
     'dispatch_catalog.read',
     'dispatch_catalog.edit',
     'shift_handover.read',
     'shift_handover.create',
     'shift_handover.submit',
     'shift_handover.ack',
     'notification.read',
     'notification.send',
     'notification.receipt_read',
     'notification.receipt_manage',
     'team:view',
     'team:manage',
     'equipment:view',
     'equipment:manage',
     'schedule:view',
     'schedule:manage'
 ], TRUE, 'dispatch', 3),

('tpl_flight_viewer', '航班查看员', 'flight_viewer',
 '只能查看航班信息',
 ARRAY['flight:read', 'flight.read', 'business_case.read', 'workflow_run.read'], TRUE, 'flight', 1),

('tpl_flight_operator', '航班操作员', 'flight_operator',
 '可查看和编辑航班信息',
 ARRAY[
     'flight:read',
     'flight:manage',
     'flight.read',
     'flight.update',
     'flight.timeline_edit',
     'flight.report_generate',
     'business_case.create',
     'business_case.read',
     'business_case.append',
     'business_case.update',
     'business_case.status_transition',
     'workflow_run.start',
     'workflow_run.read',
     'workflow_run.act'
 ], TRUE, 'flight', 2),

('tpl_workflow_editor', '流程编排维护员', 'workflow_editor',
 '可维护并发布基础流程编排，包含通知与事项状态自动化能力',
 ARRAY[
     'workflow_definition.read',
     'workflow_definition.edit',
     'workflow_definition.publish',
     'workflow_definition.deprecate',
     'automation.notify_send',
     'automation.business_case_complete',
     'automation.business_case_fail'
 ], TRUE, 'workflow', 1),

('tpl_workflow_dispatch_ops', '流程派工编排维护员', 'workflow_dispatch_editor',
 '在基础流程编排权限上，额外允许自动派工能力',
 ARRAY[
     'workflow_definition.read',
     'workflow_definition.edit',
     'workflow_definition.publish',
     'workflow_definition.deprecate',
     'automation.notify_send',
     'automation.dispatch_create',
     'automation.business_case_complete',
     'automation.business_case_fail'
 ], TRUE, 'workflow', 2),

('tpl_user_viewer', '用户查看员', 'user_viewer',
 '只能查看用户信息',
 ARRAY['user:read', 'user_admin.read'], TRUE, 'user', 1),

('tpl_user_admin', '用户管理员', 'user_admin',
 '完全的用户管理权限',
 ARRAY[
     'user:read',
     'user:create',
     'user:update',
     'user:delete',
     'role:read',
     'role:create',
     'role:update',
     'role:delete',
     'user_admin.read',
     'user_admin.edit',
     'auth_role.read',
     'auth_role.edit',
     'auth_permission_template.read',
     'auth_permission_template.edit',
     'system.config_read'
 ], TRUE, 'user', 2)
ON CONFLICT (id) DO UPDATE SET
    name = EXCLUDED.name,
    code = EXCLUDED.code,
    description = EXCLUDED.description,
    permissions = EXCLUDED.permissions,
    is_system = EXCLUDED.is_system,
    category = EXCLUDED.category,
    display_order = EXCLUDED.display_order,
    is_active = TRUE,
    updated_at = CURRENT_TIMESTAMP;

-- =====================================================
-- 输出初始化完成信息
-- =====================================================
DO $$
BEGIN
    RAISE NOTICE '=== PostgreSQL数据库初始化完成 ===';
    RAISE NOTICE '数据库: %', current_database();
    RAISE NOTICE '时间: %', now();
    RAISE NOTICE '已创建表: flights, flight_state_changes, flight_business_cases, snapshots, event_stream_versions,';
    RAISE NOTICE '         ai_conversations, permissions, roles, users,';
    RAISE NOTICE '         role_permissions, user_roles, system_config,';
    RAISE NOTICE '         todos, todo_state_changes, todo_snapshots, todo_chain_templates,';
    RAISE NOTICE '         event_process_definitions, event_type_metadata,';
    RAISE NOTICE '         process_templates, process_instance_mappings,';
    RAISE NOTICE '         process_exception_logs, process_health_check_history,';
END $$;

-- =====================================================
-- 在线历史记录表 (Migration 009)
-- =====================================================
CREATE TABLE IF NOT EXISTS online_history (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id VARCHAR(50) NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    session_id VARCHAR(100) NOT NULL,
    login_time TIMESTAMP WITH TIME ZONE NOT NULL,
    logout_time TIMESTAMP WITH TIME ZONE,
    duration_seconds INTEGER,
    ip_address INET,
    device_info VARCHAR(200),
    forced_logout BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- 索引优化
CREATE INDEX IF NOT EXISTS idx_online_history_user_login 
    ON online_history(user_id, login_time DESC);
CREATE INDEX IF NOT EXISTS idx_online_history_session 
    ON online_history(session_id);
CREATE INDEX IF NOT EXISTS idx_online_history_login_time 
    ON online_history(login_time DESC);

-- 注释
COMMENT ON TABLE online_history IS '用户在线历史记录';
COMMENT ON COLUMN online_history.duration_seconds IS '在线时长（秒）';
COMMENT ON COLUMN online_history.forced_logout IS '是否被强制下线';

-- =====================================================
-- 异常监控与 KPI 视图 (Migration 011)
-- =====================================================
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
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT chk_anomaly_rules_severity CHECK (severity IN ('low', 'medium', 'high', 'critical'))
);

INSERT INTO anomaly_rules (
    rule_id,
    rule_type,
    name,
    enabled,
    config,
    severity,
    auto_create_todo,
    todo_priority,
    escalation_intervals
) VALUES
    (
        'service_node_timeout',
        'service_node_timeout',
        'Service node timeout',
        TRUE,
        '{"minutes_after_arrival": 20}'::jsonb,
        'medium',
        TRUE,
        'HIGH',
        '[5, 15, 30]'::jsonb
    ),
    (
        'gate_stand_conflict',
        'gate_stand_conflict',
        'Gate or stand conflict',
        TRUE,
        '{"conflict_window_minutes": 45}'::jsonb,
        'high',
        TRUE,
        'HIGH',
        '[5, 15, 30]'::jsonb
    ),
    (
        'kpi_degradation_otp',
        'kpi_degradation',
        'OTP degradation alert',
        TRUE,
        '{"metric":"on_time_departure_rate","threshold":0.7,"window_hours":4}'::jsonb,
        'high',
        TRUE,
        'HIGH',
        '[10,30,60]'::jsonb
    )
ON CONFLICT (rule_id) DO NOTHING;

CREATE TABLE IF NOT EXISTS anomalies (
    anomaly_id VARCHAR(26) PRIMARY KEY,
    flight_id VARCHAR(26) NOT NULL REFERENCES flights(flight_id) ON DELETE CASCADE,
    anomaly_type VARCHAR(64) NOT NULL,
    severity VARCHAR(16) NOT NULL DEFAULT 'medium',
    status VARCHAR(16) NOT NULL DEFAULT 'open',
    title VARCHAR(255) NOT NULL,
    description TEXT,
    detected_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    resolved_at TIMESTAMP WITH TIME ZONE,
    escalation_level INTEGER NOT NULL DEFAULT 0,
    last_escalated_at TIMESTAMP WITH TIME ZONE,
    linked_todo_id VARCHAR(26),
    rule_id VARCHAR(64) REFERENCES anomaly_rules(rule_id) ON DELETE SET NULL,
    context_data JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT chk_anomalies_severity CHECK (severity IN ('low', 'medium', 'high', 'critical')),
    CONSTRAINT chk_anomalies_status CHECK (status IN ('open', 'acknowledged', 'resolved'))
);

CREATE INDEX IF NOT EXISTS idx_anomalies_flight_id ON anomalies(flight_id);
CREATE INDEX IF NOT EXISTS idx_anomalies_status_detected_at ON anomalies(status, detected_at DESC);
CREATE INDEX IF NOT EXISTS idx_anomalies_type_detected_at ON anomalies(anomaly_type, detected_at DESC);
CREATE INDEX IF NOT EXISTS idx_anomalies_rule_id ON anomalies(rule_id);
CREATE INDEX IF NOT EXISTS idx_anomalies_open_signature
    ON anomalies(flight_id, anomaly_type, rule_id)
    WHERE status <> 'resolved';

CREATE TABLE IF NOT EXISTS nl_query_log (
    log_id BIGSERIAL PRIMARY KEY,
    conversation_id VARCHAR(64),
    user_id VARCHAR(64),
    query_text TEXT NOT NULL,
    interpretation TEXT,
    summary TEXT,
    visualization_hint VARCHAR(32),
    duration_ms INTEGER,
    tool_calls JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_nl_query_log_user_created ON nl_query_log(user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_nl_query_log_conversation ON nl_query_log(conversation_id);

CREATE TABLE IF NOT EXISTS notifications (
    notification_id VARCHAR(26) PRIMARY KEY,
    user_id VARCHAR(26) NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title VARCHAR(255) NOT NULL,
    body TEXT,
    category VARCHAR(32) NOT NULL DEFAULT 'system',
    severity VARCHAR(16) NOT NULL DEFAULT 'info',
    flight_id VARCHAR(26) REFERENCES flights(flight_id) ON DELETE SET NULL,
    dispatch_order_id VARCHAR(26) REFERENCES dispatch_orders(id) ON DELETE SET NULL,
    group_id VARCHAR(26),
    event_id VARCHAR(26),
    origin_type VARCHAR(32) NOT NULL DEFAULT 'manual',
    receipt_required BOOLEAN NOT NULL DEFAULT FALSE,
    receipt_group_id VARCHAR(26),
    delivery_status VARCHAR(16) NOT NULL DEFAULT 'sent',
    delivered_at TIMESTAMPTZ,
    is_read BOOLEAN NOT NULL DEFAULT FALSE,
    ack_status VARCHAR(16) NOT NULL DEFAULT 'pending',
    ack_at TIMESTAMPTZ,
    ack_note TEXT,
    related_entity_type VARCHAR(32),
    related_entity_id VARCHAR(64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    read_at TIMESTAMPTZ,
    CONSTRAINT chk_notification_delivery_status
        CHECK (delivery_status IN ('sent', 'delivered', 'failed')),
    CONSTRAINT chk_notification_ack_status
        CHECK (ack_status IN ('pending', 'acknowledged', 'rejected'))
);

CREATE INDEX IF NOT EXISTS idx_notifications_user_unread
    ON notifications(user_id, is_read, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_notifications_user_ack_status
    ON notifications(user_id, ack_status, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_notifications_flight_created_desc
    ON notifications(flight_id, created_at DESC)
    WHERE flight_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_notifications_flight_created_at
    ON notifications(flight_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_notifications_dispatch_order_created_desc
    ON notifications(dispatch_order_id, created_at DESC)
    WHERE dispatch_order_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_notifications_dispatch_order_created_at
    ON notifications(dispatch_order_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_notifications_group_created_desc
    ON notifications(group_id, created_at DESC)
    WHERE group_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_notifications_event_id
    ON notifications(event_id)
    WHERE event_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_notifications_receipt_group_created_at
    ON notifications(receipt_group_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_notifications_origin_type_created_at
    ON notifications(origin_type, created_at DESC);

CREATE TABLE IF NOT EXISTS notification_preferences (
    user_id VARCHAR(26) PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    in_app_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    external_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    external_channel VARCHAR(32) NOT NULL DEFAULT 'none',
    mute_start VARCHAR(5),
    mute_end VARCHAR(5),
    critical_override BOOLEAN NOT NULL DEFAULT TRUE,
    category_overrides JSONB NOT NULL DEFAULT '{}'::jsonb,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS shift_handovers (
    handover_id VARCHAR(26) PRIMARY KEY,
    shift_date DATE NOT NULL,
    shift_code VARCHAR(32) NOT NULL,
    from_user_id VARCHAR(26) NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    to_user_id VARCHAR(26) NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    from_operator_name VARCHAR(100),
    from_operator_job_title VARCHAR(100),
    to_operator_name VARCHAR(100),
    to_operator_job_title VARCHAR(100),
    status VARCHAR(16) NOT NULL DEFAULT 'draft',
    summary TEXT,
    risk_level VARCHAR(16) NOT NULL DEFAULT 'medium',
    signed_at TIMESTAMPTZ,
    submitted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT chk_shift_handover_status
        CHECK (status IN ('draft', 'pending', 'sign_off', 'completed')),
    CONSTRAINT chk_shift_handover_risk_level
        CHECK (risk_level IN ('low', 'medium', 'high', 'critical'))
);

CREATE INDEX IF NOT EXISTS idx_shift_handovers_shift_date_status
    ON shift_handovers(shift_date, status, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_shift_handovers_to_user_status
    ON shift_handovers(to_user_id, status, created_at DESC);

CREATE TABLE IF NOT EXISTS shift_handover_items (
    item_id VARCHAR(26) PRIMARY KEY,
    handover_id VARCHAR(26) NOT NULL REFERENCES shift_handovers(handover_id) ON DELETE CASCADE,
    item_type VARCHAR(32) NOT NULL DEFAULT 'other',
    title VARCHAR(255) NOT NULL,
    detail TEXT,
    owner_user_id VARCHAR(26) REFERENCES users(id) ON DELETE SET NULL,
    due_at TIMESTAMPTZ,
    is_mandatory BOOLEAN NOT NULL DEFAULT TRUE,
    acknowledged BOOLEAN NOT NULL DEFAULT FALSE,
    acknowledged_at TIMESTAMPTZ,
    acknowledged_by VARCHAR(26) REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT chk_shift_handover_item_type
        CHECK (item_type IN ('pending_task', 'open_anomaly', 'risk_note', 'other'))
);

CREATE INDEX IF NOT EXISTS idx_shift_handover_items_handover
    ON shift_handover_items(handover_id, created_at ASC);

CREATE INDEX IF NOT EXISTS idx_shift_handover_items_pending
    ON shift_handover_items(handover_id, is_mandatory, acknowledged);

CREATE INDEX IF NOT EXISTS idx_dispatch_order_logs_order_action
    ON dispatch_order_logs(dispatch_order_id, action, actor_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_dispatch_order_logs_client_action
    ON dispatch_order_logs((details->>'client_action_id'))
    WHERE details ? 'client_action_id';

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_matviews
        WHERE schemaname = 'public' AND matviewname = 'mv_daily_flight_kpi'
    ) THEN
        EXECUTE '
            CREATE MATERIALIZED VIEW mv_daily_flight_kpi AS
            WITH base AS (
                SELECT
                    DATE(f.scheduled_departure AT TIME ZONE ''Asia/Shanghai'') AS flight_date,
                    f.flight_id,
                    f.scheduled_departure,
                    f.scheduled_arrival,
                    f.estimated_departure,
                    f.estimated_arrival,
                    f.actual_departure,
                    f.actual_arrival
                FROM flights f
                WHERE f.scheduled_departure IS NOT NULL
            ),
            open_anomalies AS (
                SELECT DISTINCT a.flight_id
                FROM anomalies a
                WHERE a.status IN (''open'', ''acknowledged'')
            )
            SELECT
                b.flight_date,
                COUNT(*) AS total_flights,
                COUNT(*) FILTER (
                    WHERE b.actual_departure IS NOT NULL AND b.actual_arrival IS NOT NULL
                ) AS completed_flights,
                AVG(EXTRACT(EPOCH FROM (b.actual_departure - b.actual_arrival)) / 60)
                    FILTER (
                        WHERE b.actual_departure IS NOT NULL AND b.actual_arrival IS NOT NULL
                    ) AS avg_turnaround_minutes,
                PERCENTILE_CONT(0.9) WITHIN GROUP (
                    ORDER BY EXTRACT(EPOCH FROM (b.actual_departure - b.actual_arrival)) / 60
                ) FILTER (
                    WHERE b.actual_departure IS NOT NULL AND b.actual_arrival IS NOT NULL
                ) AS p90_turnaround_minutes,
                COUNT(*) FILTER (
                    WHERE b.actual_departure <= b.scheduled_departure + INTERVAL ''15 minutes''
                )::FLOAT
                    / NULLIF(COUNT(*) FILTER (WHERE b.actual_departure IS NOT NULL), 0)
                    AS on_time_departure_rate,
                COUNT(*) FILTER (
                    WHERE b.actual_arrival <= b.scheduled_arrival + INTERVAL ''15 minutes''
                )::FLOAT
                    / NULLIF(COUNT(*) FILTER (WHERE b.actual_arrival IS NOT NULL), 0)
                    AS on_time_arrival_rate,
                COUNT(*) FILTER (WHERE oa.flight_id IS NOT NULL)::FLOAT
                    / NULLIF(COUNT(*), 0)
                    AS abnormal_ratio
            FROM base b
            LEFT JOIN open_anomalies oa ON oa.flight_id = b.flight_id
            GROUP BY b.flight_date
        ';
    END IF;
END $$;

CREATE UNIQUE INDEX IF NOT EXISTS idx_mv_daily_flight_kpi_date
    ON mv_daily_flight_kpi(flight_date);

-- =====================================================
-- Migration 022: Domain Event Outbox
-- =====================================================
CREATE TABLE IF NOT EXISTS domain_event_outbox (
    event_id VARCHAR(26) PRIMARY KEY,
    aggregate_type VARCHAR(64) NOT NULL,
    aggregate_id VARCHAR(26) NOT NULL,
    event_type VARCHAR(128) NOT NULL,
    payload JSONB NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    published_at TIMESTAMPTZ,
    publish_attempts INTEGER NOT NULL DEFAULT 0,
    next_retry_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_error TEXT,
    source_change_id VARCHAR(26) NOT NULL,
    CONSTRAINT uq_domain_event_outbox_source_change UNIQUE (source_change_id)
);

CREATE INDEX IF NOT EXISTS idx_domain_event_outbox_pending
    ON domain_event_outbox(next_retry_at, occurred_at)
    WHERE published_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_domain_event_outbox_aggregate
    ON domain_event_outbox(aggregate_type, aggregate_id, occurred_at DESC);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_publication
        WHERE pubname = 'fms_domain_event_outbox_pub'
    ) THEN
        CREATE PUBLICATION fms_domain_event_outbox_pub
            FOR TABLE domain_event_outbox
            WITH (publish = 'insert');
    END IF;
END $$;

-- =====================================================
-- Migration 023: Domain Event Consumer Tables
-- =====================================================
CREATE TABLE IF NOT EXISTS domain_event_processed (
    event_id VARCHAR(64) PRIMARY KEY,
    source_change_id VARCHAR(64),
    event_type VARCHAR(128) NOT NULL,
    aggregate_type VARCHAR(64) NOT NULL,
    aggregate_id VARCHAR(64) NOT NULL,
    success BOOLEAN NOT NULL DEFAULT FALSE,
    retry_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    last_attempt_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    processed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_domain_event_processed_success_attempt
    ON domain_event_processed(success, last_attempt_at DESC);

CREATE INDEX IF NOT EXISTS idx_domain_event_processed_source_change
    ON domain_event_processed(source_change_id);

CREATE TABLE IF NOT EXISTS domain_event_consumer_offsets (
    consumer_group VARCHAR(128) NOT NULL,
    consumer_name VARCHAR(128) NOT NULL,
    topic VARCHAR(128) NOT NULL,
    last_message_id VARCHAR(64) NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (consumer_group, consumer_name, topic)
);

CREATE INDEX IF NOT EXISTS idx_domain_event_consumer_offsets_updated_at
    ON domain_event_consumer_offsets(updated_at DESC);

-- =====================================================
-- Migration 031: Domain Event Dead Letter Table
-- =====================================================
CREATE TABLE IF NOT EXISTS domain_event_dead_letters (
    event_id VARCHAR(64) PRIMARY KEY,
    source_change_id VARCHAR(64),
    aggregate_type VARCHAR(64) NOT NULL,
    aggregate_id VARCHAR(64) NOT NULL,
    event_type VARCHAR(128) NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    stream_message_id VARCHAR(64),
    retry_count INTEGER NOT NULL DEFAULT 1,
    error_message TEXT,
    dead_lettered_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_domain_event_dead_letters_type_time
    ON domain_event_dead_letters(event_type, dead_lettered_at DESC);

CREATE INDEX IF NOT EXISTS idx_domain_event_dead_letters_aggregate
    ON domain_event_dead_letters(aggregate_type, aggregate_id, dead_lettered_at DESC);


-- =====================================================
-- Migration 024: Todo Agent Context Extension Table
-- =====================================================
CREATE TABLE IF NOT EXISTS todo_agent_context (
    todo_id VARCHAR(26) PRIMARY KEY
        REFERENCES todos(todo_id) ON DELETE CASCADE,
    agent_entity_id VARCHAR(255) NOT NULL DEFAULT 'default',
    agent_run_id VARCHAR(26),
    agent_status VARCHAR(50) NOT NULL DEFAULT 'pending',
    updated_by VARCHAR(100) NOT NULL DEFAULT 'system',
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    version INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS idx_tac_agent_entity_id
    ON todo_agent_context(agent_entity_id);

CREATE INDEX IF NOT EXISTS idx_tac_agent_run_id
    ON todo_agent_context(agent_run_id);

CREATE INDEX IF NOT EXISTS idx_tac_agent_status
    ON todo_agent_context(agent_status);

CREATE INDEX IF NOT EXISTS idx_tac_updated_at
    ON todo_agent_context(updated_at DESC);

INSERT INTO todo_agent_context (
    todo_id,
    agent_entity_id,
    agent_run_id,
    agent_status,
    updated_by,
    updated_at,
    version
)
SELECT
    t.todo_id,
    COALESCE(NULLIF(BTRIM(t.agent_entity_id), ''), 'default') AS agent_entity_id,
    t.agent_run_id,
    COALESCE(NULLIF(BTRIM(t.agent_status), ''), 'pending') AS agent_status,
    COALESCE(NULLIF(BTRIM(t.updated_by), ''), 'system') AS updated_by,
    COALESCE(t.updated_at, CURRENT_TIMESTAMP) AS updated_at,
    GREATEST(COALESCE(t.version, 0), 1) AS version
FROM todos AS t
WHERE t.todo_id IS NOT NULL
ON CONFLICT (todo_id) DO UPDATE SET
    agent_entity_id = EXCLUDED.agent_entity_id,
    agent_run_id = EXCLUDED.agent_run_id,
    agent_status = EXCLUDED.agent_status,
    updated_by = EXCLUDED.updated_by,
    updated_at = EXCLUDED.updated_at,
    version = EXCLUDED.version;

-- =====================================================
-- Migration 025: Todo Agent Context Composite Indexes
-- =====================================================
CREATE INDEX IF NOT EXISTS idx_tac_agent_status_updated_at
    ON todo_agent_context(agent_status, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_tac_agent_entity_updated_at
    ON todo_agent_context(agent_entity_id, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_tac_agent_run_id_updated_at
    ON todo_agent_context(agent_run_id, updated_at DESC)
    WHERE agent_run_id IS NOT NULL;

-- =====================================================
-- Migration 026: AI Experience V2 Foundation
-- =====================================================
ALTER TABLE ai_pending_actions
    ADD COLUMN IF NOT EXISTS risk_level TEXT NOT NULL DEFAULT 'NORMAL',
    ADD COLUMN IF NOT EXISTS entity_type TEXT NULL,
    ADD COLUMN IF NOT EXISTS entity_id TEXT NULL,
    ADD COLUMN IF NOT EXISTS before_snapshot JSONB NULL,
    ADD COLUMN IF NOT EXISTS after_snapshot JSONB NULL,
    ADD COLUMN IF NOT EXISTS json_patch JSONB NULL,
    ADD COLUMN IF NOT EXISTS diff_summary JSONB NULL,
    ADD COLUMN IF NOT EXISTS execution_receipt JSONB NULL,
    ADD COLUMN IF NOT EXISTS status_code TEXT NULL,
    ADD COLUMN IF NOT EXISTS error_payload JSONB NULL,
    ADD COLUMN IF NOT EXISTS correlation_id UUID NULL,
    ADD COLUMN IF NOT EXISTS ui_hints JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS expires_at TIMESTAMP WITH TIME ZONE NULL;

CREATE INDEX IF NOT EXISTS idx_ai_pending_actions_status_risk_created
    ON ai_pending_actions(status, risk_level, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_ai_pending_actions_correlation_id
    ON ai_pending_actions(correlation_id);

CREATE INDEX IF NOT EXISTS idx_ai_pending_actions_json_patch_gin
    ON ai_pending_actions USING GIN (json_patch jsonb_path_ops);

CREATE INDEX IF NOT EXISTS idx_ai_pending_actions_diff_summary_gin
ON ai_pending_actions USING GIN (diff_summary jsonb_path_ops);

CREATE INDEX IF NOT EXISTS idx_ai_pending_actions_expires_at
ON ai_pending_actions(expires_at)
WHERE status = 'pending' AND expires_at IS NOT NULL;

CREATE TABLE IF NOT EXISTS ai_kb_documents (
    id BIGSERIAL PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    mime_type TEXT NULL,
    title TEXT NULL,
    content_hash TEXT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS ai_kb_chunks (
    id BIGSERIAL PRIMARY KEY,
    document_id BIGINT NOT NULL REFERENCES ai_kb_documents(id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL,
    content TEXT NOT NULL,
    token_count INTEGER NOT NULL DEFAULT 0,
    heading_path TEXT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    content_tsv tsvector NULL,
    CONSTRAINT uq_ai_kb_chunk_doc_chunk UNIQUE (document_id, chunk_index)
);

CREATE INDEX IF NOT EXISTS idx_ai_kb_chunks_content_tsv
    ON ai_kb_chunks USING GIN (content_tsv);

CREATE INDEX IF NOT EXISTS idx_ai_kb_chunks_doc_chunk
    ON ai_kb_chunks(document_id, chunk_index);

CREATE INDEX IF NOT EXISTS idx_ai_kb_documents_path
    ON ai_kb_documents(path);

CREATE INDEX IF NOT EXISTS idx_ai_kb_documents_content_hash
    ON ai_kb_documents(content_hash);

CREATE TABLE IF NOT EXISTS ai_conversation_memory (
    id BIGSERIAL PRIMARY KEY,
    conversation_id VARCHAR(64) NOT NULL,
    turn_no INTEGER NOT NULL,
    summary TEXT NOT NULL,
    entities JSONB NOT NULL DEFAULT '[]'::jsonb,
    constraints JSONB NOT NULL DEFAULT '{}'::jsonb,
    tool_outcomes JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_ai_conversation_memory_conversation_turn
    ON ai_conversation_memory(conversation_id, turn_no DESC);

CREATE TABLE IF NOT EXISTS ai_user_profile (
    user_id VARCHAR(255) PRIMARY KEY,
    role TEXT NULL,
    timezone TEXT NULL,
    preferences JSONB NOT NULL DEFAULT '{}'::jsonb,
    pinned_metrics JSONB NOT NULL DEFAULT '{}'::jsonb,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- =====================================================
-- Migration 027: Agent Shared Context Pool (Blackboard)
-- =====================================================
CREATE TABLE IF NOT EXISTS agent_shared_context (
    id VARCHAR(26) PRIMARY KEY,
    root_todo_id VARCHAR(26) NOT NULL,
    source_todo_id VARCHAR(26) NOT NULL,
    source_todo_title TEXT NOT NULL DEFAULT '',
    agent_entity_id VARCHAR(255) NOT NULL DEFAULT 'default',
    content_type VARCHAR(50) NOT NULL DEFAULT 'distilled_conclusion',
    content TEXT NOT NULL DEFAULT '',
    tags TEXT[] NOT NULL DEFAULT '{}',
    token_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_asc_root_todo
    ON agent_shared_context(root_todo_id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_asc_root_source_upsert
    ON agent_shared_context(root_todo_id, source_todo_id);

CREATE INDEX IF NOT EXISTS idx_asc_tags
    ON agent_shared_context USING GIN (tags);

CREATE INDEX IF NOT EXISTS idx_asc_created_at
    ON agent_shared_context(created_at);


-- Migration 028: backfill_missing_flight_seed_events
-- setup_postgresql.sql 为幂等 schema/setup 脚本，通常不在这里执行历史事件回填。
-- 这里仅同步登记迁移版本，保持与独立迁移文件语义一致。

-- =====================================================
-- Migration 029: Dispatch Chat Tables
-- =====================================================
CREATE TABLE IF NOT EXISTS dispatch_chat_groups (
    group_id VARCHAR(26) PRIMARY KEY,
    channel_type VARCHAR(32) NOT NULL DEFAULT 'system_flight_dispatch',
    flight_id VARCHAR(26) NOT NULL,
    group_name VARCHAR(120) NOT NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'active',
    read_only BOOLEAN NOT NULL DEFAULT FALSE,
    deprecated_at TIMESTAMPTZ,
    deprecation_reason VARCHAR(64),
    archive_at TIMESTAMPTZ,
    archived_at TIMESTAMPTZ,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uq_dispatch_chat_groups_channel_flight UNIQUE (channel_type, flight_id),
    CONSTRAINT fk_dispatch_chat_groups_flight FOREIGN KEY (flight_id) REFERENCES flights(flight_id) ON DELETE CASCADE,
    CONSTRAINT chk_dispatch_chat_group_status CHECK (status IN ('active', 'archived'))
);

CREATE INDEX IF NOT EXISTS idx_dispatch_chat_groups_flight_id
    ON dispatch_chat_groups(flight_id);
CREATE INDEX IF NOT EXISTS idx_dispatch_chat_groups_status_read_only
    ON dispatch_chat_groups(status, read_only);
CREATE INDEX IF NOT EXISTS idx_dispatch_chat_groups_deprecated_at
    ON dispatch_chat_groups(deprecated_at);
CREATE INDEX IF NOT EXISTS idx_dispatch_chat_groups_archive_at
    ON dispatch_chat_groups(archive_at);
CREATE INDEX IF NOT EXISTS idx_dispatch_chat_groups_updated_at_desc
    ON dispatch_chat_groups(updated_at DESC);

CREATE TABLE IF NOT EXISTS dispatch_chat_group_members (
    id VARCHAR(26) PRIMARY KEY,
    group_id VARCHAR(26) NOT NULL,
    user_id VARCHAR(26) NOT NULL,
    is_assignee BOOLEAN NOT NULL DEFAULT FALSE,
    is_dispatcher BOOLEAN NOT NULL DEFAULT FALSE,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    joined_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    left_at TIMESTAMPTZ,
    last_read_seq BIGINT NOT NULL DEFAULT 0,
    last_read_at TIMESTAMPTZ,
    CONSTRAINT uq_dispatch_chat_group_member UNIQUE (group_id, user_id),
    CONSTRAINT fk_dispatch_chat_group_members_group FOREIGN KEY (group_id) REFERENCES dispatch_chat_groups(group_id) ON DELETE CASCADE,
    CONSTRAINT fk_dispatch_chat_group_members_user FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
    CONSTRAINT chk_dispatch_chat_group_member_role CHECK (is_assignee OR is_dispatcher)
);

CREATE INDEX IF NOT EXISTS idx_dispatch_chat_group_members_user_active
    ON dispatch_chat_group_members(user_id, is_active);
CREATE INDEX IF NOT EXISTS idx_dispatch_chat_group_members_group_active
    ON dispatch_chat_group_members(group_id, is_active);
CREATE INDEX IF NOT EXISTS idx_dispatch_chat_group_members_group_read_seq
    ON dispatch_chat_group_members(group_id, last_read_seq);

CREATE TABLE IF NOT EXISTS dispatch_chat_messages (
    message_id VARCHAR(26) PRIMARY KEY,
    seq_no BIGSERIAL UNIQUE,
    group_id VARCHAR(26) NOT NULL,
    sender_user_id VARCHAR(26),
    dispatch_order_id VARCHAR(26) REFERENCES dispatch_orders(id) ON DELETE SET NULL,
    event_id VARCHAR(26),
    message_type VARCHAR(16) NOT NULL DEFAULT 'text',
    content TEXT NOT NULL,
    is_at_all BOOLEAN NOT NULL DEFAULT FALSE,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    sent_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT fk_dispatch_chat_messages_group FOREIGN KEY (group_id) REFERENCES dispatch_chat_groups(group_id) ON DELETE CASCADE,
    CONSTRAINT fk_dispatch_chat_messages_sender FOREIGN KEY (sender_user_id) REFERENCES users(id) ON DELETE SET NULL,
    CONSTRAINT chk_dispatch_chat_message_type CHECK (message_type IN ('text', 'system')),
    CONSTRAINT chk_dispatch_chat_message_content_len CHECK (char_length(trim(content)) BETWEEN 1 AND 2000)
);

CREATE INDEX IF NOT EXISTS idx_dispatch_chat_messages_group_seq_desc
    ON dispatch_chat_messages(group_id, seq_no DESC);
CREATE INDEX IF NOT EXISTS idx_dispatch_chat_messages_group_sent_desc
    ON dispatch_chat_messages(group_id, sent_at DESC);
CREATE INDEX IF NOT EXISTS idx_dispatch_chat_messages_sender_sent_desc
    ON dispatch_chat_messages(sender_user_id, sent_at DESC);
CREATE INDEX IF NOT EXISTS idx_dispatch_chat_messages_order_sent_desc
    ON dispatch_chat_messages(dispatch_order_id, sent_at DESC)
    WHERE dispatch_order_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_dispatch_chat_messages_event_id
    ON dispatch_chat_messages(event_id)
    WHERE event_id IS NOT NULL;

-- =====================================================
-- Migration 035: Dispatch Collaboration Event Ledger
-- =====================================================
CREATE TABLE IF NOT EXISTS dispatch_collaboration_events (
    event_id VARCHAR(26) PRIMARY KEY,
    flight_id VARCHAR(26) NOT NULL REFERENCES flights(flight_id) ON DELETE CASCADE,
    dispatch_order_id VARCHAR(26) REFERENCES dispatch_orders(id) ON DELETE SET NULL,
    group_id VARCHAR(26) REFERENCES dispatch_chat_groups(group_id) ON DELETE SET NULL,
    event_type VARCHAR(64) NOT NULL,
    actor_user_id VARCHAR(26) REFERENCES users(id) ON DELETE SET NULL,
    correlation_id VARCHAR(64),
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    source_table VARCHAR(64),
    source_record_id VARCHAR(64)
);

CREATE INDEX IF NOT EXISTS idx_dispatch_collab_events_flight_occurred_desc
    ON dispatch_collaboration_events(flight_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_dispatch_collab_events_order_occurred_desc
    ON dispatch_collaboration_events(dispatch_order_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_dispatch_collab_events_group_occurred_desc
    ON dispatch_collaboration_events(group_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_dispatch_collab_events_correlation_id
    ON dispatch_collaboration_events(correlation_id);
CREATE INDEX IF NOT EXISTS idx_dispatch_collab_events_type_occurred_desc
    ON dispatch_collaboration_events(event_type, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_dispatch_collab_events_source_record
    ON dispatch_collaboration_events(source_table, source_record_id)
    WHERE source_table IS NOT NULL AND source_record_id IS NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_dispatch_order_logs_event'
    ) THEN
        ALTER TABLE dispatch_order_logs
            ADD CONSTRAINT fk_dispatch_order_logs_event
            FOREIGN KEY (event_id) REFERENCES dispatch_collaboration_events(event_id) ON DELETE SET NULL;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_dispatch_chat_messages_event'
    ) THEN
        ALTER TABLE dispatch_chat_messages
            ADD CONSTRAINT fk_dispatch_chat_messages_event
            FOREIGN KEY (event_id) REFERENCES dispatch_collaboration_events(event_id) ON DELETE SET NULL;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_notifications_event'
    ) THEN
        ALTER TABLE notifications
            ADD CONSTRAINT fk_notifications_event
            FOREIGN KEY (event_id) REFERENCES dispatch_collaboration_events(event_id) ON DELETE SET NULL;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_notifications_group'
    ) THEN
        ALTER TABLE notifications
            ADD CONSTRAINT fk_notifications_group
            FOREIGN KEY (group_id) REFERENCES dispatch_chat_groups(group_id) ON DELETE SET NULL;
    END IF;
END $$;

COMMENT ON TABLE dispatch_collaboration_events IS '派工协同统一审计账本';

COMMENT ON TABLE dispatch_chat_groups IS '按航班维度自动生成的保障协同群';
COMMENT ON COLUMN dispatch_chat_groups.deprecated_at IS '群聊弃用时间';
COMMENT ON COLUMN dispatch_chat_groups.deprecation_reason IS '群聊弃用原因';
COMMENT ON TABLE dispatch_chat_group_members IS '群成员关系，含成员角色与已读游标';
COMMENT ON TABLE dispatch_chat_messages IS '群消息表，首版支持文本与系统消息';


-- =====================================================
-- Migration 030: AI Query Readonly Schema
-- =====================================================
CREATE SCHEMA IF NOT EXISTS ai_query;

-- Drop first to allow column renames (CREATE OR REPLACE VIEW cannot change column names).
-- CASCADE is needed because v_ops_overview depends on this view.
DROP VIEW IF EXISTS ai_query.v_flights CASCADE;

CREATE OR REPLACE VIEW ai_query.v_flights AS
WITH open_anomaly AS (
    SELECT
        a.flight_id,
        COUNT(*) AS open_anomaly_count
    FROM anomalies a
    WHERE a.status IN ('open', 'acknowledged')
    GROUP BY a.flight_id
)
SELECT
    f.flight_id,
    f.flight_number,
    f.airline_code,
    f.status,
    f.scheduled_departure,
    f.estimated_departure,
    f.actual_departure,
    f.scheduled_arrival,
    f.estimated_arrival,
    f.actual_arrival,
    f.execution_date,
    f.workspace_date,
    f.stand,
    f.gate,
    f.terminal,
    COALESCE(oa.open_anomaly_count, 0) AS open_anomaly_count,
    (COALESCE(oa.open_anomaly_count, 0) > 0) AS has_open_anomaly,
    inbound_leg.leg_json AS inbound_leg_json,
    outbound_leg.leg_json AS outbound_leg_json,
    CASE
        WHEN f.estimated_departure IS NOT NULL AND f.scheduled_departure IS NOT NULL THEN
            ROUND(EXTRACT(EPOCH FROM (f.estimated_departure - f.scheduled_departure)) / 60.0, 2)
        ELSE NULL
    END AS delay_minutes,
    f.created_at,
    f.updated_at
FROM public.flights AS f
LEFT JOIN open_anomaly oa ON oa.flight_id = f.flight_id
LEFT JOIN LATERAL (
    SELECT to_jsonb(l) AS leg_json
    FROM flight_legs l
    WHERE l.flight_id = f.flight_id AND l.leg_type = 'inbound'
    ORDER BY l.updated_at DESC NULLS LAST, l.created_at DESC NULLS LAST
    LIMIT 1
) inbound_leg ON TRUE
LEFT JOIN LATERAL (
    SELECT to_jsonb(l) AS leg_json
    FROM flight_legs l
    WHERE l.flight_id = f.flight_id AND l.leg_type = 'outbound'
    ORDER BY l.updated_at DESC NULLS LAST, l.created_at DESC NULLS LAST
    LIMIT 1
) outbound_leg ON TRUE;

CREATE OR REPLACE VIEW ai_query.v_anomalies AS
SELECT
    a.anomaly_id,
    a.flight_id,
    a.anomaly_type,
    a.severity,
    a.status,
    a.title,
    a.description,
    a.detected_at,
    a.resolved_at,
    a.escalation_level,
    a.rule_id,
    a.context_data,
    a.created_at,
    a.updated_at
FROM public.anomalies AS a;

CREATE OR REPLACE VIEW ai_query.v_todos AS
SELECT
    t.todo_id,
    t.title,
    t.description,
    t.priority,
    t.status,
    t.category,
    t.due_date,
    t.assigned_to,
    t.progress,
    t.tags,
    t.created_by,
    t.updated_by,
    t.created_at,
    t.updated_at,
    t.is_deleted
FROM public.todos AS t;

CREATE OR REPLACE VIEW ai_query.v_dispatch_orders AS
SELECT
    d.id AS dispatch_order_id,
    d.flight_id,
    d.task_type,
    d.stand_id,
    d.assignee_type,
    d.team_id,
    d.individual_user_id,
    d.status,
    d.dispatch_type,
    d.workflow_status,
    d.source,
    d.recommendation_score,
    d.planned_start_time,
    d.planned_end_time,
    d.actual_start_time,
    d.actual_end_time,
    d.assignment_deadline,
    d.dispatched_at,
    d.created_at,
    d.updated_at
FROM public.dispatch_orders AS d;

CREATE OR REPLACE VIEW ai_query.v_dispatch_alerts AS
SELECT
    da.id AS dispatch_alert_id,
    da.flight_id,
    da.task_type,
    da.alert_type,
    da.severity,
    da.message,
    da.is_resolved,
    da.resolved_at,
    da.resolved_by,
    da.created_at
FROM public.dispatch_alerts AS da;

CREATE OR REPLACE VIEW ai_query.v_shift_handovers AS
SELECT
    sh.handover_id,
    sh.shift_date,
    sh.shift_code,
    sh.from_user_id,
    sh.to_user_id,
    sh.from_operator_name,
    sh.from_operator_job_title,
    CASE
        WHEN COALESCE(NULLIF(BTRIM(sh.from_operator_name), ''), NULLIF(BTRIM(COALESCE(from_user.display_name, from_user.username)), '')) IS NULL
            THEN NULL
        ELSE CONCAT(
            COALESCE(NULLIF(BTRIM(sh.from_operator_name), ''), NULLIF(BTRIM(COALESCE(from_user.display_name, from_user.username)), '')),
            '-',
            COALESCE(
                NULLIF(BTRIM(sh.from_operator_job_title), ''),
                NULLIF(BTRIM(from_user.job_title), ''),
                NULLIF(BTRIM(from_role.role_name), ''),
                CASE WHEN COALESCE(from_user.is_admin, FALSE) THEN 'admin' ELSE '用户' END
            )
        )
    END AS from_operator_label,
    sh.to_operator_name,
    sh.to_operator_job_title,
    CASE
        WHEN COALESCE(NULLIF(BTRIM(sh.to_operator_name), ''), NULLIF(BTRIM(COALESCE(to_user.display_name, to_user.username)), '')) IS NULL
            THEN NULL
        ELSE CONCAT(
            COALESCE(NULLIF(BTRIM(sh.to_operator_name), ''), NULLIF(BTRIM(COALESCE(to_user.display_name, to_user.username)), '')),
            '-',
            COALESCE(
                NULLIF(BTRIM(sh.to_operator_job_title), ''),
                NULLIF(BTRIM(to_user.job_title), ''),
                NULLIF(BTRIM(to_role.role_name), ''),
                CASE WHEN COALESCE(to_user.is_admin, FALSE) THEN 'admin' ELSE '用户' END
            )
        )
    END AS to_operator_label,
    sh.status,
    sh.risk_level,
    sh.summary,
    sh.signed_at,
    sh.submitted_at,
    sh.created_at,
    sh.updated_at
FROM public.shift_handovers AS sh
LEFT JOIN public.users AS from_user ON from_user.id = sh.from_user_id
LEFT JOIN public.users AS to_user ON to_user.id = sh.to_user_id
LEFT JOIN LATERAL (
    SELECT r.name AS role_name
    FROM public.user_roles ur
    JOIN public.roles r ON r.id = ur.role_id
    WHERE ur.user_id = sh.from_user_id
    ORDER BY r.name ASC
    LIMIT 1
) AS from_role ON TRUE
LEFT JOIN LATERAL (
    SELECT r.name AS role_name
    FROM public.user_roles ur
    JOIN public.roles r ON r.id = ur.role_id
    WHERE ur.user_id = sh.to_user_id
    ORDER BY r.name ASC
    LIMIT 1
) AS to_role ON TRUE;

CREATE OR REPLACE VIEW ai_query.v_notifications AS
SELECT
    n.notification_id,
    n.user_id,
    n.title,
    n.body,
    n.category,
    n.severity,
    n.delivery_status,
    n.is_read,
    n.ack_status,
    n.related_entity_type,
    n.related_entity_id,
    n.created_at
FROM public.notifications AS n;

CREATE OR REPLACE VIEW ai_query.v_online_history AS
SELECT
    oh.id,
    oh.user_id,
    oh.session_id,
    oh.login_time,
    oh.logout_time,
    oh.duration_seconds,
    oh.forced_logout,
    oh.created_at
FROM public.online_history AS oh;

CREATE OR REPLACE VIEW ai_query.v_daily_kpi AS
SELECT
    k.flight_date,
    k.total_flights,
    k.completed_flights,
    k.avg_turnaround_minutes,
    k.p90_turnaround_minutes,
    k.on_time_departure_rate,
    k.on_time_arrival_rate,
    k.abnormal_ratio
FROM public.mv_daily_flight_kpi AS k;

CREATE OR REPLACE VIEW ai_query.v_ops_overview AS
SELECT
    (SELECT COUNT(*) FROM ai_query.v_flights) AS flights_total,
    (
        SELECT COUNT(*)
        FROM ai_query.v_flights
        WHERE status NOT IN (7, 8, 9)
    ) AS flights_active,
    (
        SELECT COUNT(*)
        FROM ai_query.v_anomalies
        WHERE status = 'open'
    ) AS anomalies_open,
    (
        SELECT COUNT(*)
        FROM ai_query.v_todos
        WHERE is_deleted = FALSE AND status IN ('待办', '进行中')
    ) AS todos_open,
    CURRENT_TIMESTAMP AS snapshot_at;

-- =====================================================
-- AI 只读查询角色 (ai_query_ro)
-- =====================================================
-- 本脚本仅创建角色、收紧权限，并设置只读事务与 statement timeout。
-- 不在仓库 SQL 中写入口令字面量，也不做伪造的环境变量插值。
-- 口令必须由 Vault / 运维在部署后独立设置或轮换，例如：
--   ALTER ROLE ai_query_ro WITH PASSWORD '<secret-from-vault>';
-- 已部署环境若曾使用仓库历史默认口令，必须立即从 Vault/运维侧轮换。
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'ai_query_ro') THEN
        CREATE ROLE ai_query_ro
            LOGIN;
    END IF;

    ALTER ROLE ai_query_ro
        WITH LOGIN
        NOSUPERUSER
        NOCREATEDB
        NOCREATEROLE
        NOINHERIT;
END $$;

DO $$
DECLARE
    target_db TEXT := current_database();
BEGIN
    EXECUTE format('GRANT CONNECT ON DATABASE %I TO ai_query_ro', target_db);
    EXECUTE format('ALTER ROLE ai_query_ro IN DATABASE %I SET default_transaction_read_only = on', target_db);
    EXECUTE format('ALTER ROLE ai_query_ro IN DATABASE %I SET statement_timeout = %L', target_db, '5000ms');
    EXECUTE format('ALTER ROLE ai_query_ro IN DATABASE %I SET idle_in_transaction_session_timeout = %L', target_db, '10000ms');
END $$;

REVOKE ALL ON SCHEMA public FROM ai_query_ro;
REVOKE CREATE ON SCHEMA ai_query FROM PUBLIC;
REVOKE ALL ON SCHEMA ai_query FROM PUBLIC;
GRANT USAGE ON SCHEMA ai_query TO ai_query_ro;
GRANT SELECT ON ALL TABLES IN SCHEMA ai_query TO ai_query_ro;
ALTER DEFAULT PRIVILEGES IN SCHEMA ai_query GRANT SELECT ON TABLES TO ai_query_ro;


-- =====================================================
-- Migration 032: Strong-cut FlightLeg/Timeline/Anomaly decoupling
-- =====================================================
-- setup_postgresql.sql 直接维护强切后的最终 schema。
-- 不在 setup 中重复执行历史 legacy 数据回填作业类型。


-- =====================================================
-- Migration 033: Create Mobile Support Tables
-- =====================================================
CREATE TABLE IF NOT EXISTS mobile_device_registrations (
    device_id VARCHAR(64) PRIMARY KEY,
    user_id VARCHAR(26) NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    platform VARCHAR(32) NOT NULL DEFAULT 'android',
    push_channel VARCHAR(32) NOT NULL DEFAULT 'none',
    push_token TEXT,
    app_version VARCHAR(64),
    os_version VARCHAR(64),
    device_model VARCHAR(128),
    manufacturer VARCHAR(64),
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    last_heartbeat_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    registered_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    CONSTRAINT chk_mobile_device_push_channel
        CHECK (push_channel IN ('none', 'fcm', 'hms', 'xiaomi', 'oppo', 'vivo', 'wecom'))
);

CREATE INDEX IF NOT EXISTS idx_mobile_devices_user_active_heartbeat
    ON mobile_device_registrations(user_id, is_active, last_heartbeat_at DESC);

CREATE INDEX IF NOT EXISTS idx_mobile_devices_push_channel_active
    ON mobile_device_registrations(push_channel, is_active, last_heartbeat_at DESC);

CREATE INDEX IF NOT EXISTS idx_mobile_devices_push_token
    ON mobile_device_registrations(push_token)
    WHERE push_token IS NOT NULL;

CREATE TABLE IF NOT EXISTS mobile_upload_assets (
    upload_id VARCHAR(26) PRIMARY KEY,
    user_id VARCHAR(26) NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    storage_key VARCHAR(255) NOT NULL UNIQUE,
    original_filename VARCHAR(255) NOT NULL,
    content_type VARCHAR(128),
    file_size BIGINT NOT NULL DEFAULT 0,
    checksum_sha256 VARCHAR(64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    CONSTRAINT chk_mobile_upload_size_non_negative
        CHECK (file_size >= 0)
);

CREATE INDEX IF NOT EXISTS idx_mobile_upload_assets_user_created
    ON mobile_upload_assets(user_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_mobile_upload_assets_checksum
    ON mobile_upload_assets(checksum_sha256)
    WHERE checksum_sha256 IS NOT NULL;


-- =====================================================
-- Migration 034: Add ai_pending_actions expires_at
-- =====================================================
-- setup 中 ai_pending_actions 基表与 Migration 026 已包含 expires_at 相关变更，
-- 这里仅登记迁移版本，确保 setup 与 migrations 版本完全一致。

-- =====================================================
-- Migration 035: Dispatch Collaboration Event Ledger
-- =====================================================

-- =====================================================
-- Migration 036: Notification Receipt Groups & Origin
-- =====================================================

-- =====================================================
-- Migration 037: Relax Dispatch Collaboration Source Record Index
-- =====================================================

-- =====================================================
-- Migration 038: Dispatch Chat Deprecation Marker
-- =====================================================
-- setup 中 dispatch_chat_groups 基表定义已合并弃用字段与索引，这里仅登记迁移版本。

-- =====================================================
-- Migration 039: Dispatch ETA Reporting
-- =====================================================
-- setup 中 dispatch_orders 基表定义已合并 ETA 回报字段与索引，这里仅登记迁移版本。

-- =====================================================
-- Migration 040: Operator Identity Contexts & Handover Snapshots
-- =====================================================
-- setup 中 users / shift_handovers / ai_query.v_shift_handovers 已合并当前值班人字段与快照字段，
-- 这里仅登记迁移版本，确保 setup 与 migrations 版本完全一致。

-- =====================================================
-- Migration 041: Normalize Flight Leg Mission To Smallint
-- =====================================================
-- setup 中 flight_legs 基表定义已直接使用 SMALLINT mission 并包含约束，这里仅登记迁移版本。

-- =====================================================
-- Migration 042: Business Case Appends
-- =====================================================

-- =====================================================
-- Migration 043: Business Case Workflow Runs
-- =====================================================

-- =====================================================
-- Migration 044: Flight External Sync Tables
-- =====================================================

-- =====================================================
-- Migration 045: Flight Identity & Sequence Bindings
-- =====================================================

-- =====================================================
-- Migration 053_dispatch_schedule_planning
-- =====================================================
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


-- =====================================================
-- Migration 046: Flight Sync Identity Counters
-- =====================================================

-- =====================================================
-- Migration 047: Dispatch Stand Travel Stats
-- =====================================================
CREATE TABLE IF NOT EXISTS dispatch_stand_travel_stats (
    from_stand_id VARCHAR(26) NOT NULL REFERENCES stands(id),
    to_stand_id   VARCHAR(26) NOT NULL REFERENCES stands(id),
    sample_count  INT NOT NULL DEFAULT 0,
    total_minutes DOUBLE PRECISION NOT NULL DEFAULT 0,
    avg_minutes   DOUBLE PRECISION NOT NULL DEFAULT 0,
    min_minutes   DOUBLE PRECISION,
    max_minutes   DOUBLE PRECISION,
    last_updated  TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (from_stand_id, to_stand_id)
);

CREATE INDEX IF NOT EXISTS idx_dispatch_stand_travel_stats_from ON dispatch_stand_travel_stats(from_stand_id);
CREATE INDEX IF NOT EXISTS idx_dispatch_stand_travel_stats_to ON dispatch_stand_travel_stats(to_stand_id);

COMMENT ON TABLE dispatch_stand_travel_stats IS '机位对间移动时间统计（基于签退→签到时间差积累）';
COMMENT ON COLUMN dispatch_stand_travel_stats.sample_count IS '采样次数';
COMMENT ON COLUMN dispatch_stand_travel_stats.avg_minutes IS '平均移动时间（分钟）';


CREATE TABLE IF NOT EXISTS department_qualification_catalog (
    id VARCHAR(26) PRIMARY KEY,
    department_id VARCHAR(26) NOT NULL REFERENCES departments(id) ON DELETE CASCADE,
    qualification_code VARCHAR(64) NOT NULL,
    qualification_name VARCHAR(100) NOT NULL,
    description TEXT,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uq_department_qualification_catalog UNIQUE (department_id, qualification_code)
);

CREATE INDEX IF NOT EXISTS idx_department_qualification_catalog_department
    ON department_qualification_catalog(department_id, qualification_code);

CREATE TABLE IF NOT EXISTS department_qualification_levels (
    id VARCHAR(26) PRIMARY KEY,
    department_id VARCHAR(26) NOT NULL REFERENCES departments(id) ON DELETE CASCADE,
    qualification_code VARCHAR(64) NOT NULL,
    level_code VARCHAR(64) NOT NULL,
    level_name VARCHAR(100) NOT NULL,
    level_rank INT NOT NULL DEFAULT 0,
    covered_level_codes JSONB NOT NULL DEFAULT '[]'::jsonb,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uq_department_qualification_levels UNIQUE (department_id, qualification_code, level_code)
);

CREATE INDEX IF NOT EXISTS idx_department_qualification_levels_department
    ON department_qualification_levels(department_id, qualification_code, level_rank DESC);

CREATE TABLE IF NOT EXISTS qualification_grants (
    id VARCHAR(26) PRIMARY KEY,
    user_id VARCHAR(26) NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    department_id VARCHAR(26) NOT NULL REFERENCES departments(id) ON DELETE CASCADE,
    qualification_code VARCHAR(64) NOT NULL,
    level_code VARCHAR(64) NOT NULL,
    valid_from TIMESTAMPTZ,
    valid_to TIMESTAMPTZ,
    status VARCHAR(20) NOT NULL DEFAULT 'active',
    source_team_id VARCHAR(26) REFERENCES teams(id) ON DELETE SET NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uq_qualification_grants UNIQUE (user_id, department_id, qualification_code, level_code)
);

CREATE INDEX IF NOT EXISTS idx_qualification_grants_department_user
    ON qualification_grants(department_id, user_id, status);

CREATE TABLE IF NOT EXISTS department_task_type_requirement_versions (
    id VARCHAR(26) PRIMARY KEY,
    department_id VARCHAR(26) NOT NULL REFERENCES departments(id) ON DELETE CASCADE,
    task_type VARCHAR(50) NOT NULL REFERENCES task_types(code) ON DELETE CASCADE,
    version_no INT NOT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'draft',
    requirements_json JSONB NOT NULL DEFAULT '[]'::jsonb,
    notes TEXT,
    published_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uq_department_task_type_requirement_version UNIQUE (department_id, task_type, version_no)
);

CREATE INDEX IF NOT EXISTS idx_department_task_type_requirement_versions_lookup
    ON department_task_type_requirement_versions(department_id, task_type, status, version_no DESC);

CREATE UNIQUE INDEX IF NOT EXISTS uq_department_task_type_requirement_published
    ON department_task_type_requirement_versions(department_id, task_type)
    WHERE status = 'published';

ALTER TABLE task_types
    ADD COLUMN IF NOT EXISTS default_department_id VARCHAR(26) REFERENCES departments(id);

ALTER TABLE dispatch_orders
    ADD COLUMN IF NOT EXISTS department_rule_version VARCHAR(26),
    ADD COLUMN IF NOT EXISTS crew_requirement_snapshot JSONB NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN IF NOT EXISTS task_crew JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS qualification_gap JSONB NOT NULL DEFAULT '[]'::jsonb;

ALTER TABLE dispatch_order_members
    ADD COLUMN IF NOT EXISTS slot_code VARCHAR(64),
    ADD COLUMN IF NOT EXISTS qualification_code VARCHAR(64),
    ADD COLUMN IF NOT EXISTS qualification_level_code VARCHAR(64);

CREATE INDEX IF NOT EXISTS idx_dispatch_orders_department_rule_version
    ON dispatch_orders(department_rule_version);

CREATE INDEX IF NOT EXISTS idx_dispatch_order_members_slot_code
    ON dispatch_order_members(slot_code);


-- =====================================================
-- Migration 049: Flight Leg Route Stations Cutover
-- setup 需要显式保证 flight_legs 达到 route-station 最终结构。
-- =====================================================
ALTER TABLE flight_legs
    ADD COLUMN IF NOT EXISTS origin_stations JSONB NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN IF NOT EXISTS destination_stations JSONB NOT NULL DEFAULT '[]'::jsonb;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'flight_legs'
          AND column_name = 'origin_code'
    ) THEN
        EXECUTE $sql$
            UPDATE flight_legs
            SET origin_stations = CASE
                    WHEN leg_type = 'inbound' AND NULLIF(BTRIM(COALESCE(origin_code, '')), '') IS NOT NULL
                        THEN jsonb_build_array(
                            jsonb_build_object(
                                'code', UPPER(BTRIM(origin_code)),
                                'name', NULLIF(BTRIM(COALESCE(origin_name, '')), '')
                            )
                        )
                    ELSE origin_stations
                END,
                destination_stations = CASE
                    WHEN leg_type = 'outbound' AND NULLIF(BTRIM(COALESCE(destination_code, '')), '') IS NOT NULL
                        THEN jsonb_build_array(
                            jsonb_build_object(
                                'code', UPPER(BTRIM(destination_code)),
                                'name', NULLIF(BTRIM(COALESCE(destination_name, '')), '')
                            )
                        )
                    ELSE destination_stations
                END
        $sql$;

        ALTER TABLE flight_legs
            DROP COLUMN IF EXISTS origin_code,
            DROP COLUMN IF EXISTS destination_code,
            DROP COLUMN IF EXISTS origin_name,
            DROP COLUMN IF EXISTS destination_name;
    END IF;
END $$;


-- =====================================================
-- Migration 050: Add dispatch generation and publication schema
-- =====================================================

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


CREATE TABLE IF NOT EXISTS dispatch_temporary_task_templates (
    id VARCHAR(26) PRIMARY KEY,
    department_id VARCHAR(26) NOT NULL REFERENCES departments(id) ON DELETE CASCADE,
    template_code VARCHAR(64) NOT NULL,
    template_name VARCHAR(120) NOT NULL,
    task_type VARCHAR(64) NOT NULL,
    requirements_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    notes TEXT,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uq_dispatch_temporary_task_templates UNIQUE (department_id, template_code)
);

CREATE INDEX IF NOT EXISTS idx_dispatch_temporary_task_templates_lookup
    ON dispatch_temporary_task_templates(department_id, is_active, template_code);





ALTER TABLE notifications
    ADD COLUMN IF NOT EXISTS sender_user_id VARCHAR(26),
    ADD COLUMN IF NOT EXISTS sender_username_snapshot VARCHAR(128);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_notifications_sender_user'
    ) THEN
        ALTER TABLE notifications
            ADD CONSTRAINT fk_notifications_sender_user
            FOREIGN KEY (sender_user_id) REFERENCES users(id) ON DELETE SET NULL;
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_notifications_sender_created_at
    ON notifications (sender_user_id, created_at DESC)
    WHERE sender_user_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_notifications_sender_receipt_group_created_at
    ON notifications (sender_user_id, receipt_group_id, created_at DESC)
    WHERE sender_user_id IS NOT NULL AND receipt_group_id IS NOT NULL;


-- =====================================================
-- Register domain event outbox publication migration (057) in setup mode
-- =====================================================
-- setup 中只登记 publication migration，实际 CREATE PUBLICATION 语义与独立迁移文件保持一致。

-- =====================================================
-- Migration 058: Business case append metadata
-- =====================================================
-- setup 中基表定义已包含 metadata 列；这里额外执行幂等 ALTER/COMMENT，保证旧 setup 基线重复执行也能收敛。
ALTER TABLE flight_business_case_appends
    ADD COLUMN IF NOT EXISTS metadata JSONB NOT NULL DEFAULT '{}'::jsonb;

COMMENT ON COLUMN flight_business_case_appends.metadata
    IS '追加记录的结构化元数据（tool_calls, token_usage, thinking, step_type, sequence 等）';


-- =====================================================
-- Migration 059: Policy interception error_code indexes
-- =====================================================
CREATE INDEX IF NOT EXISTS idx_fbc_policy_interception_error_code
    ON flight_business_cases ((context->>'error_code'))
    WHERE case_type = 'policy_interception';

CREATE INDEX IF NOT EXISTS idx_afbc_policy_interception_error_code
    ON archived_flight_business_cases ((context->>'error_code'))
    WHERE case_type = 'policy_interception';


-- Migration 060: Add wait receipts task

-- =====================================================
-- Migration 061: Add permission_version to users
-- =====================================================
ALTER TABLE users
    ADD COLUMN IF NOT EXISTS permission_version INTEGER NOT NULL DEFAULT 1;

COMMENT ON COLUMN users.permission_version
    IS '权限版本号，递增后使旧 JWT 失效，实现精准踢出';

CREATE INDEX IF NOT EXISTS idx_users_permission_version
    ON users (permission_version);


-- =====================================================
-- Migration 062: Restore flight cobt/codt to main table
-- setup 主表定义已直接包含 cobt_time / codt，这里只登记迁移已对齐。
-- =====================================================

-- =====================================================
-- Migration 063: Drop zombie milestone columns
-- setup 仍保留扩展时间列以兼容当前基线与校验测试，这里只登记迁移编号。
-- =====================================================

-- =====================================================
-- Migration 064: Flight labels
-- setup 前文已直接创建 label_definitions / labels 列与 GIN 索引，这里只登记迁移已对齐。
-- =====================================================

-- =====================================================
-- Migration 065: Add business case department scope
-- =====================================================
ALTER TABLE flight_business_cases
    ADD COLUMN IF NOT EXISTS visibility_scope VARCHAR(20) NOT NULL DEFAULT 'COMMON',
    ADD COLUMN IF NOT EXISTS department_id VARCHAR(64),
    ADD COLUMN IF NOT EXISTS department_name_snapshot VARCHAR(100);

UPDATE flight_business_cases
SET visibility_scope = CASE
        WHEN COALESCE(NULLIF(BTRIM(department_id), ''), NULLIF(BTRIM(department_name_snapshot), '')) IS NOT NULL
            THEN 'DEPARTMENT'
        ELSE 'COMMON'
    END
WHERE visibility_scope IS NULL
   OR BTRIM(visibility_scope) = '';

ALTER TABLE archived_flight_business_cases
    ADD COLUMN IF NOT EXISTS visibility_scope VARCHAR(20) NOT NULL DEFAULT 'COMMON',
    ADD COLUMN IF NOT EXISTS department_id VARCHAR(64),
    ADD COLUMN IF NOT EXISTS department_name_snapshot VARCHAR(100);

UPDATE archived_flight_business_cases
SET visibility_scope = CASE
        WHEN COALESCE(NULLIF(BTRIM(department_id), ''), NULLIF(BTRIM(department_name_snapshot), '')) IS NOT NULL
            THEN 'DEPARTMENT'
        ELSE 'COMMON'
    END
WHERE visibility_scope IS NULL
   OR BTRIM(visibility_scope) = '';

ALTER TABLE business_case_types
    ADD COLUMN IF NOT EXISTS visibility_scope VARCHAR(20) NOT NULL DEFAULT 'COMMON',
    ADD COLUMN IF NOT EXISTS department_id VARCHAR(64),
    ADD COLUMN IF NOT EXISTS department_name_snapshot VARCHAR(100);

UPDATE business_case_types
SET visibility_scope = CASE
        WHEN COALESCE(NULLIF(BTRIM(department_id), ''), NULLIF(BTRIM(department_name_snapshot), '')) IS NOT NULL
            THEN 'DEPARTMENT'
        ELSE 'COMMON'
    END
WHERE visibility_scope IS NULL
   OR BTRIM(visibility_scope) = '';

CREATE INDEX IF NOT EXISTS idx_flight_business_cases_visibility_department
    ON flight_business_cases (visibility_scope, department_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_archived_business_cases_visibility_department
    ON archived_flight_business_cases (visibility_scope, department_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_business_case_types_visibility_department
    ON business_case_types (visibility_scope, department_id, is_active, created_at);


-- =====================================================
-- Migration 066: Refine granular permissions
-- setup 前文已直接补齐 V2 resource.action 权限、模板和默认角色授予，这里只登记迁移已对齐。
-- =====================================================

-- =====================================================
-- Migration 067: Create externalized workflow form schema
-- =====================================================
CREATE TABLE IF NOT EXISTS workflow_form_templates (
    id VARCHAR(26) PRIMARY KEY,
    form_code VARCHAR(100) NOT NULL,
    name VARCHAR(200) NOT NULL,
    version INTEGER NOT NULL,
    schema_json JSONB NOT NULL,
    ui_schema_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    status VARCHAR(32) NOT NULL DEFAULT 'DRAFT',
    description TEXT,
    created_by VARCHAR(100) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (form_code, version)
);

CREATE INDEX IF NOT EXISTS idx_workflow_form_templates_code_status
    ON workflow_form_templates (form_code, status, version DESC);

CREATE TABLE IF NOT EXISTS workflow_form_bindings (
    id VARCHAR(26) PRIMARY KEY,
    template_code VARCHAR(100) NOT NULL,
    process_definition_key VARCHAR(100) NOT NULL,
    task_definition_key VARCHAR(100) NOT NULL,
    form_code VARCHAR(100) NOT NULL,
    form_version INTEGER,
    target_department_id VARCHAR(64),
    target_department_name VARCHAR(100),
    target_roles JSONB NOT NULL DEFAULT '[]'::jsonb,
    assignment_mode VARCHAR(32) NOT NULL DEFAULT 'DEPARTMENT_ROLES',
    write_back_mode VARCHAR(32) NOT NULL DEFAULT 'BUSINESS_CASE_CONTEXT',
    write_back_key VARCHAR(200) NOT NULL,
    flowable_variable_prefix VARCHAR(100),
    complete_task_on_submit BOOLEAN NOT NULL DEFAULT TRUE,
    allow_resubmit BOOLEAN NOT NULL DEFAULT FALSE,
    source VARCHAR(32) NOT NULL DEFAULT 'DB',
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (template_code, task_definition_key, form_code)
);

CREATE INDEX IF NOT EXISTS idx_workflow_form_bindings_process_task
    ON workflow_form_bindings (process_definition_key, task_definition_key);

CREATE TABLE IF NOT EXISTS workflow_form_submissions (
    id VARCHAR(26) PRIMARY KEY,
    case_id VARCHAR(26) NOT NULL,
    run_id VARCHAR(26),
    process_instance_id VARCHAR(64) NOT NULL,
    task_id VARCHAR(64) NOT NULL,
    task_definition_key VARCHAR(100) NOT NULL,
    form_code VARCHAR(100) NOT NULL,
    form_version INTEGER NOT NULL,
    data_json JSONB NOT NULL,
    normalized_summary_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    submitted_by VARCHAR(100) NOT NULL,
    submitted_operator_name VARCHAR(200),
    submitted_department_id VARCHAR(64),
    submitted_department_name VARCHAR(100),
    submitted_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    status VARCHAR(32) NOT NULL DEFAULT 'SUBMITTED'
);

CREATE INDEX IF NOT EXISTS idx_workflow_form_submissions_case
    ON workflow_form_submissions (case_id, submitted_at DESC);

CREATE INDEX IF NOT EXISTS idx_workflow_form_submissions_run
    ON workflow_form_submissions (run_id, submitted_at DESC);

CREATE INDEX IF NOT EXISTS idx_workflow_form_submissions_task
    ON workflow_form_submissions (task_id);


-- =====================================================
-- Migration 068: Create runtime diagnostic events
-- =====================================================
CREATE TABLE IF NOT EXISTS runtime_diagnostic_events (
    event_id TEXT PRIMARY KEY,
    topic TEXT NOT NULL,
    event_type TEXT NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_runtime_diagnostic_events_topic_created
    ON runtime_diagnostic_events (topic, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_runtime_diagnostic_events_event_type_created
    ON runtime_diagnostic_events (event_type, created_at DESC);


-- =====================================================
-- Migration 069: Add dispatch timeline event idempotency
-- setup 基线表定义已直接包含 client_action_id 和唯一索引；
-- 以下 ALTER/INDEX 保证已有库重复执行 setup 时也能对齐。
-- =====================================================
ALTER TABLE flight_dispatch_timeline_events
    ADD COLUMN IF NOT EXISTS client_action_id VARCHAR(128);

CREATE UNIQUE INDEX IF NOT EXISTS uq_flight_dispatch_timeline_client_action
    ON flight_dispatch_timeline_events(flight_id, client_action_id)
    WHERE client_action_id IS NOT NULL;


-- =====================================================
-- Migration 070: Add dispatch order log idempotency
-- setup 基线索引已直接包含唯一约束；以下清理历史重复日志并登记迁移版本。
-- =====================================================
WITH duplicate_client_actions AS (
    SELECT
        id,
        details,
        ROW_NUMBER() OVER (
            PARTITION BY dispatch_order_id, action, details->>'client_action_id'
            ORDER BY created_at, id
        ) AS duplicate_rank
    FROM dispatch_order_logs
    WHERE details ? 'client_action_id'
      AND NULLIF(details->>'client_action_id', '') IS NOT NULL
)
UPDATE dispatch_order_logs AS logs
SET details = jsonb_set(
        logs.details - 'client_action_id',
        '{duplicate_client_action_id}',
        to_jsonb(logs.details->>'client_action_id'),
        true
    )
FROM duplicate_client_actions AS duplicates
WHERE logs.id = duplicates.id
  AND duplicates.duplicate_rank > 1;

CREATE UNIQUE INDEX IF NOT EXISTS uq_dispatch_order_logs_client_action
    ON dispatch_order_logs(dispatch_order_id, action, (details->>'client_action_id'))
    WHERE details ? 'client_action_id'
      AND NULLIF(details->>'client_action_id', '') IS NOT NULL;


-- =====================================================
-- Migration 071: Add business case append idempotency
-- setup 基线表定义已直接包含 client_action_id 和唯一索引；
-- 以下 ALTER/INDEX 保证已有库重复执行 setup 时也能对齐。
-- =====================================================
ALTER TABLE flight_business_case_appends
    ADD COLUMN IF NOT EXISTS client_action_id VARCHAR(128);

WITH duplicate_client_actions AS (
    SELECT
        append_id,
        ROW_NUMBER() OVER (
            PARTITION BY case_id, client_action_id
            ORDER BY appended_at, append_id
        ) AS duplicate_rank
    FROM flight_business_case_appends
    WHERE client_action_id IS NOT NULL
      AND NULLIF(client_action_id, '') IS NOT NULL
)
UPDATE flight_business_case_appends AS appends
SET metadata = jsonb_set(
        appends.metadata,
        '{duplicate_client_action_id}',
        to_jsonb(appends.client_action_id),
        true
    ),
    client_action_id = NULL
FROM duplicate_client_actions AS duplicates
WHERE appends.append_id = duplicates.append_id
  AND duplicates.duplicate_rank > 1;

CREATE UNIQUE INDEX IF NOT EXISTS uq_fbc_appends_case_client_action
    ON flight_business_case_appends(case_id, client_action_id)
    WHERE client_action_id IS NOT NULL;


-- =====================================================
-- Migration 072: Dispatch Event Extension Tables
-- 工单调整规则、工单生成规则、工单事件调整日志表
-- =====================================================
CREATE TABLE IF NOT EXISTS dispatch_order_adjustment_rules (
    id VARCHAR(26) PRIMARY KEY,
    adjuster_type VARCHAR(64) NOT NULL,
    name VARCHAR(120) NOT NULL DEFAULT '',
    description TEXT,
    event_patterns JSONB NOT NULL DEFAULT '[]'::jsonb,
    priority INT NOT NULL DEFAULT 100,
    config JSONB,
    conditions JSONB,
    is_enabled BOOLEAN NOT NULL DEFAULT true,
    department_id VARCHAR(26),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by VARCHAR(100),
    CONSTRAINT fk_adjustment_rules_department FOREIGN KEY (department_id) REFERENCES departments(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_adjustment_rules_type ON dispatch_order_adjustment_rules(adjuster_type);
CREATE INDEX IF NOT EXISTS idx_adjustment_rules_enabled ON dispatch_order_adjustment_rules(is_enabled);
CREATE INDEX IF NOT EXISTS idx_adjustment_rules_department ON dispatch_order_adjustment_rules(department_id);

CREATE TABLE IF NOT EXISTS event_driven_dispatch_generation_rules (
    id VARCHAR(26) PRIMARY KEY,
    generator_type VARCHAR(64) NOT NULL,
    name VARCHAR(120) NOT NULL DEFAULT '',
    description TEXT,
    event_patterns JSONB NOT NULL DEFAULT '[]'::jsonb,
    priority INT NOT NULL DEFAULT 100,
    config JSONB NOT NULL DEFAULT '{}'::jsonb,
    conditions JSONB,
    is_enabled BOOLEAN NOT NULL DEFAULT true,
    department_id VARCHAR(26),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_by VARCHAR(100),
    CONSTRAINT fk_generation_rules_department FOREIGN KEY (department_id) REFERENCES departments(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_generation_rules_type ON event_driven_dispatch_generation_rules(generator_type);
CREATE INDEX IF NOT EXISTS idx_generation_rules_enabled ON event_driven_dispatch_generation_rules(is_enabled);
CREATE INDEX IF NOT EXISTS idx_generation_rules_department ON event_driven_dispatch_generation_rules(department_id);

CREATE TABLE IF NOT EXISTS dispatch_order_adjustment_logs (
    id VARCHAR(26) PRIMARY KEY,
    dispatch_order_id VARCHAR(26) NOT NULL,
    adjuster_id VARCHAR(64) NOT NULL,
    adjuster_type VARCHAR(64) NOT NULL,
    event_id VARCHAR(64),
    event_type VARCHAR(64) NOT NULL,
    action_type VARCHAR(32) NOT NULL,
    before_state JSONB,
    after_state JSONB,
    reason TEXT,
    metadata JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT fk_adjustment_logs_order FOREIGN KEY (dispatch_order_id) REFERENCES dispatch_orders(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_adjustment_logs_order ON dispatch_order_adjustment_logs(dispatch_order_id);
CREATE INDEX IF NOT EXISTS idx_adjustment_logs_adjuster ON dispatch_order_adjustment_logs(adjuster_id);
CREATE INDEX IF NOT EXISTS idx_adjustment_logs_event ON dispatch_order_adjustment_logs(event_id);


-- =====================================================
-- Migration 073: AIP Ontology Customization Tables
-- AIP 语义层核心表：Ontology 对象、动作、ACL、函数注册、约束
-- =====================================================
CREATE TABLE IF NOT EXISTS aip_ontology_objects (
    id VARCHAR(64) PRIMARY KEY,
    name VARCHAR(128) NOT NULL UNIQUE,
    plural_name VARCHAR(128),
    description TEXT,
    is_abstract BOOLEAN NOT NULL DEFAULT FALSE,
    properties JSONB NOT NULL DEFAULT '[]'::jsonb,
    relationships JSONB NOT NULL DEFAULT '[]'::jsonb,
    actions JSONB NOT NULL DEFAULT '[]'::jsonb,
    tags JSONB NOT NULL DEFAULT '[]'::jsonb,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_aip_ontology_objects_name ON aip_ontology_objects(name);
CREATE INDEX IF NOT EXISTS idx_aip_ontology_objects_active ON aip_ontology_objects(is_active);

CREATE TABLE IF NOT EXISTS aip_ontology_actions (
    id VARCHAR(64) PRIMARY KEY,
    name VARCHAR(128) NOT NULL,
    object_type VARCHAR(128) NOT NULL,
    description TEXT,
    category VARCHAR(32) NOT NULL DEFAULT 'mutation',
    parameters JSONB NOT NULL DEFAULT '[]'::jsonb,
    requires_approval BOOLEAN NOT NULL DEFAULT FALSE,
    risk_level VARCHAR(16) NOT NULL DEFAULT 'NORMAL',
    constraint_rules JSONB NOT NULL DEFAULT '[]'::jsonb,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uq_aip_ontology_actions_object_action UNIQUE (object_type, name)
);
CREATE INDEX IF NOT EXISTS idx_aip_ontology_actions_object ON aip_ontology_actions(object_type);
CREATE INDEX IF NOT EXISTS idx_aip_ontology_actions_risk ON aip_ontology_actions(risk_level);
CREATE INDEX IF NOT EXISTS idx_aip_ontology_actions_active ON aip_ontology_actions(is_active);

CREATE TABLE IF NOT EXISTS aip_object_policies (
    id VARCHAR(64) PRIMARY KEY,
    object_type VARCHAR(128) NOT NULL,
    object_id VARCHAR(255),
    principal_type VARCHAR(32) NOT NULL DEFAULT 'user',
    principal_id VARCHAR(255) NOT NULL,
    permission VARCHAR(32) NOT NULL,
    granted BOOLEAN NOT NULL DEFAULT TRUE,
    conditions JSONB,
    description TEXT,
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uq_aip_object_policies_principal_object_permission UNIQUE (principal_type, principal_id, object_type, object_id, permission)
);
CREATE INDEX IF NOT EXISTS idx_aip_object_policies_principal ON aip_object_policies(principal_type, principal_id);
CREATE INDEX IF NOT EXISTS idx_aip_object_policies_object ON aip_object_policies(object_type, object_id);
CREATE INDEX IF NOT EXISTS idx_aip_object_policies_permission ON aip_object_policies(permission);
CREATE INDEX IF NOT EXISTS idx_aip_object_policies_expires ON aip_object_policies(expires_at) WHERE expires_at IS NOT NULL;

CREATE TABLE IF NOT EXISTS aip_functions (
    id VARCHAR(64) PRIMARY KEY,
    name VARCHAR(128) NOT NULL UNIQUE,
    category VARCHAR(32) NOT NULL DEFAULT 'object_action',
    object_type VARCHAR(128) NOT NULL,
    action_name VARCHAR(128) NOT NULL,
    description TEXT,
    parameters_schema JSONB NOT NULL DEFAULT '{}'::jsonb,
    requires_approval BOOLEAN NOT NULL DEFAULT FALSE,
    risk_level VARCHAR(16) NOT NULL DEFAULT 'NORMAL',
    permission_required VARCHAR(255),
    tags JSONB NOT NULL DEFAULT '[]'::jsonb,
    examples JSONB NOT NULL DEFAULT '[]'::jsonb,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_aip_functions_name ON aip_functions(name);
CREATE INDEX IF NOT EXISTS idx_aip_functions_object ON aip_functions(object_type, action_name);
CREATE INDEX IF NOT EXISTS idx_aip_functions_category ON aip_functions(category);
CREATE INDEX IF NOT EXISTS idx_aip_functions_active ON aip_functions(is_active);

CREATE TABLE IF NOT EXISTS aip_tool_mappings (
    id VARCHAR(64) PRIMARY KEY,
    tool_name VARCHAR(128) NOT NULL UNIQUE,
    object_type VARCHAR(128) NOT NULL,
    action_name VARCHAR(128) NOT NULL,
    requires_approval BOOLEAN NOT NULL DEFAULT FALSE,
    risk_level VARCHAR(16) NOT NULL DEFAULT 'NORMAL',
    migration_status VARCHAR(32) NOT NULL DEFAULT 'not_started',
    custom_handler TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_aip_tool_mappings_tool ON aip_tool_mappings(tool_name);
CREATE INDEX IF NOT EXISTS idx_aip_tool_mappings_object ON aip_tool_mappings(object_type);
CREATE INDEX IF NOT EXISTS idx_aip_tool_mappings_status ON aip_tool_mappings(migration_status);

CREATE TABLE IF NOT EXISTS aip_constraints (
    id VARCHAR(64) PRIMARY KEY,
    name VARCHAR(128) NOT NULL,
    object_type VARCHAR(128) NOT NULL,
    action_name VARCHAR(128),
    constraint_type VARCHAR(32) NOT NULL,
    expression TEXT NOT NULL,
    error_message TEXT,
    severity VARCHAR(16) NOT NULL DEFAULT 'ERROR',
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT uq_aip_constraints_object_action_name UNIQUE (object_type, action_name, name)
);
CREATE INDEX IF NOT EXISTS idx_aip_constraints_object ON aip_constraints(object_type, action_name);
CREATE INDEX IF NOT EXISTS idx_aip_constraints_type ON aip_constraints(constraint_type);
CREATE INDEX IF NOT EXISTS idx_aip_constraints_active ON aip_constraints(is_active);

INSERT INTO aip_ontology_objects (id, name, plural_name, description, properties, relationships, actions, tags)
VALUES
    ('obj_flight', 'Flight', 'Flights', '航班对象',
     '[{"name": "flight_number", "type": "string", "required": true}, {"name": "stand", "type": "string"}, {"name": "status", "type": "string"}]',
     '[{"name": "stand", "target_object": "Stand", "cardinality": "one"}, {"name": "team_assignments", "target_object": "Team", "cardinality": "many"}]',
     '["change_stand", "delay_flight", "assign_team", "update_status", "mark_arrived", "mark_departed"]',
     '["core", "flight"]'),
    ('obj_stand', 'Stand', 'Stands', '停机位对象',
     '[{"name": "stand_id", "type": "string", "required": true}, {"name": "status", "type": "string"}]',
     '[{"name": "current_flight", "target_object": "Flight", "cardinality": "one"}]',
     '["occupy", "release", "reserve", "close", "update_status"]',
     '["core", "resource"]'),
    ('obj_team', 'Team', 'Teams', '班组对象',
     '[{"name": "team_id", "type": "string", "required": true}, {"name": "status", "type": "string"}]',
     '[{"name": "assigned_flights", "target_object": "Flight", "cardinality": "many"}]',
     '["assign_flight", "update_status", "change_location"]',
     '["core", "resource"]'),
    ('obj_anomaly', 'Anomaly', 'Anomalies', '异常对象',
     '[{"name": "anomaly_type", "type": "string", "required": true}, {"name": "severity", "type": "string"}]',
     '[{"name": "related_flight", "target_object": "Flight", "cardinality": "one"}]',
     '["acknowledge", "assign_team", "resolve", "escalate"]',
     '["alert", "incident"]'),
    ('obj_todo', 'Todo', 'Todos', '待办事项对象',
     '[{"name": "title", "type": "string", "required": true}, {"name": "priority", "type": "string"}]',
     '[{"name": "assignee", "target_object": "Team", "cardinality": "one"}]',
     '["create", "complete", "assign", "update_status"]',
     '["task", "workflow"]')
ON CONFLICT (name) DO NOTHING;

INSERT INTO aip_ontology_actions (id, name, object_type, description, parameters, requires_approval, risk_level)
VALUES
    ('act_change_stand', 'change_stand', 'Flight', '更改航班停机位', '[{"name": "stand_id", "type": "string", "required": true}]', true, 'MEDIUM'),
    ('act_delay_flight', 'delay_flight', 'Flight', '延迟航班', '[{"name": "delay_minutes", "type": "integer", "required": true}]', false, 'LOW'),
    ('act_assign_team', 'assign_team', 'Flight', '分配班组到航班', '[{"name": "team_id", "type": "string", "required": true}]', true, 'MEDIUM'),
    ('act_occupy', 'occupy', 'Stand', '占用停机位', '[{"name": "flight_id", "type": "string", "required": true}]', false, 'LOW'),
    ('act_release', 'release', 'Stand', '释放停机位', '[]', false, 'LOW'),
    ('act_resolve', 'resolve', 'Anomaly', '解决异常', '[{"name": "resolution", "type": "string", "required": true}]', true, 'MEDIUM'),
    ('act_create_todo', 'create', 'Todo', '创建待办', '[{"name": "title", "type": "string", "required": true}]', false, 'LOW')
ON CONFLICT (object_type, name) DO NOTHING;

INSERT INTO aip_tool_mappings (id, tool_name, object_type, action_name, requires_approval, risk_level, migration_status)
VALUES
    ('map_change_flight_stand', 'change_flight_stand', 'Flight', 'change_stand', true, 'MEDIUM', 'in_progress'),
    ('map_delay_flight', 'delay_flight', 'Flight', 'delay_flight', false, 'LOW', 'in_progress'),
    ('map_assign_team_to_flight', 'assign_team_to_flight', 'Flight', 'assign_team', true, 'MEDIUM', 'in_progress'),
    ('map_occupy_stand', 'occupy_stand', 'Stand', 'occupy', false, 'LOW', 'in_progress'),
    ('map_release_stand', 'release_stand', 'Stand', 'release', false, 'LOW', 'in_progress'),
    ('map_acknowledge_anomaly', 'acknowledge_anomaly', 'Anomaly', 'acknowledge', false, 'LOW', 'in_progress'),
    ('map_resolve_anomaly', 'resolve_anomaly', 'Anomaly', 'resolve', true, 'MEDIUM', 'in_progress')
ON CONFLICT (tool_name) DO NOTHING;

INSERT INTO aip_functions (id, name, category, object_type, action_name, description, parameters_schema, requires_approval, risk_level)
VALUES
    ('fn_change_stand', 'Flight.change_stand', 'object_action', 'Flight', 'change_stand', '更改航班停机位', '{"type": "object", "properties": {"stand_id": {"type": "string"}}, "required": ["stand_id"]}', true, 'MEDIUM'),
    ('fn_delay_flight', 'Flight.delay_flight', 'object_action', 'Flight', 'delay_flight', '延迟航班', '{"type": "object", "properties": {"delay_minutes": {"type": "integer"}}, "required": ["delay_minutes"]}', false, 'LOW'),
    ('fn_assign_team', 'Flight.assign_team', 'object_action', 'Flight', 'assign_team', '分配班组', '{"type": "object", "properties": {"team_id": {"type": "string"}}, "required": ["team_id"]}', true, 'MEDIUM'),
    ('fn_occupy_stand', 'Stand.occupy', 'object_action', 'Stand', 'occupy', '占用停机位', '{"type": "object", "properties": {"flight_id": {"type": "string"}}, "required": ["flight_id"]}', false, 'LOW'),
    ('fn_resolve_anomaly', 'Anomaly.resolve', 'object_action', 'Anomaly', 'resolve', '解决异常', '{"type": "object", "properties": {"resolution": {"type": "string"}}, "required": ["resolution"]}', true, 'MEDIUM'),
    ('fn_create_todo', 'Todo.create', 'object_action', 'Todo', 'create', '创建待办', '{"type": "object", "properties": {"title": {"type": "string"}}, "required": ["title"]}', false, 'LOW')
ON CONFLICT (name) DO NOTHING;

INSERT INTO aip_constraints (id, name, object_type, action_name, constraint_type, expression, error_message, severity)
VALUES
    ('const_stand_capacity', 'stand_capacity_check', 'Stand', 'occupy', 'capacity', 'stand.capacity > 0', '停机位容量必须大于0', 'ERROR'),
    ('const_stand_available', 'stand_availability', 'Stand', 'occupy', 'availability', 'stand.status == "available"', '停机位不可用', 'ERROR'),
    ('const_team_available', 'team_availability', 'Team', 'assign_flight', 'availability', 'team.status == "available"', '班组不可用', 'ERROR')
ON CONFLICT (object_type, action_name, name) DO NOTHING;


-- =====================================================
-- Migration 074: Fix baggage_check_01 BPMNDI plane id
-- 修正 060 历史 BPMN XML 中 BPMNDiagram/BPMNPlane 重复 ID。
-- =====================================================
UPDATE business_case_types
SET bpmn_xml = replace(
        bpmn_xml,
        '<bpmndi:BPMNPlane id="BPMNDiagram_1" bpmnElement="baggage_check_01">',
        '<bpmndi:BPMNPlane id="BPMNPlane_1" bpmnElement="baggage_check_01">'
    ),
    updated_at = NOW()
WHERE code = 'baggage_check_01'
  AND bpmn_xml LIKE '%<bpmndi:BPMNDiagram id="BPMNDiagram_1"%'
  AND bpmn_xml LIKE '%<bpmndi:BPMNPlane id="BPMNDiagram_1" bpmnElement="baggage_check_01">%';


-- =====================================================
-- Migration 075: Add event rule metadata columns
-- 补齐事件规则 CRUD 仓储依赖的名称、描述和创建人列。
-- =====================================================
ALTER TABLE IF EXISTS dispatch_order_adjustment_rules
    ADD COLUMN IF NOT EXISTS name VARCHAR(120) NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS description TEXT,
    ADD COLUMN IF NOT EXISTS created_by VARCHAR(100);

ALTER TABLE IF EXISTS event_driven_dispatch_generation_rules
    ADD COLUMN IF NOT EXISTS name VARCHAR(120) NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS description TEXT,
    ADD COLUMN IF NOT EXISTS created_by VARCHAR(100);
