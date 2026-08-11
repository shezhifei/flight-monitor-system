-- Create todo agent context extension table and backfill from legacy todo columns


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

