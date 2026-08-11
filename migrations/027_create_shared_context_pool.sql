-- Create agent_shared_context table for the Blackboard shared context pool


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

-- Primary query path: read entries for a specific TODO tree
CREATE INDEX IF NOT EXISTS idx_asc_root_todo
    ON agent_shared_context(root_todo_id);

-- Upsert path: one entry per source todo per root
CREATE UNIQUE INDEX IF NOT EXISTS idx_asc_root_source_upsert
    ON agent_shared_context(root_todo_id, source_todo_id);

-- Tag-based search (GIN index on array column)
CREATE INDEX IF NOT EXISTS idx_asc_tags
    ON agent_shared_context USING GIN (tags);

-- Cleanup path: find old entries
CREATE INDEX IF NOT EXISTS idx_asc_created_at
    ON agent_shared_context(created_at);

