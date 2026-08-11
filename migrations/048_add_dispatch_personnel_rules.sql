
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

