-- Add composite indexes for todo agent context query hot paths


CREATE INDEX IF NOT EXISTS idx_tac_agent_status_updated_at
    ON todo_agent_context(agent_status, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_tac_agent_entity_updated_at
    ON todo_agent_context(agent_entity_id, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_tac_agent_run_id_updated_at
    ON todo_agent_context(agent_run_id, updated_at DESC)
    WHERE agent_run_id IS NOT NULL;

