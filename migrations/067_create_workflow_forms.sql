
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



