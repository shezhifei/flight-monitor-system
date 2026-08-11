
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

