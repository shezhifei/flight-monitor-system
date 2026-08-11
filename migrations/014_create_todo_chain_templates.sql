-- Create TODO chain templates table


CREATE TABLE IF NOT EXISTS todo_chain_templates (
    template_id VARCHAR(64) PRIMARY KEY,
    name VARCHAR(128) NOT NULL,
    description TEXT,
    task_types JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_by VARCHAR(64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

