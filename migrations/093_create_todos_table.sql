-- Migration 093: Create todos table
-- Description: Creates the core todos table used by the todo service and
--              DomainActionExecutor. This table was previously missing from
--              the migration chain (only the repository code referenced it).


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

