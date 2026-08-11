
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



