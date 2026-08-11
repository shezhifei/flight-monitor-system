-- 095_ai_capability_extension.sql
-- AI capability extension: model catalog, MCP servers, skills, cache metrics
--
-- ai_entities historically lived only in setup_postgresql.sql. Create it here so
-- numbered migrations bootstrap a clean database without the legacy bootstrap script.

CREATE TABLE IF NOT EXISTS ai_entities (
    id TEXT PRIMARY KEY,
    config JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 1. ai_entities new columns
ALTER TABLE ai_entities
    ADD COLUMN IF NOT EXISTS config_version INTEGER NOT NULL DEFAULT 1,
    ADD COLUMN IF NOT EXISTS config_revision BIGINT NOT NULL DEFAULT 1;

-- 2. ai_model_catalog
CREATE TABLE IF NOT EXISTS ai_model_catalog (
    model_id TEXT PRIMARY KEY,
    provider TEXT NOT NULL,
    provider_model TEXT NOT NULL,
    api_format TEXT NOT NULL DEFAULT 'chat_completions',
    input_modalities JSONB NOT NULL DEFAULT '["text"]'::jsonb,
    output_modalities JSONB NOT NULL DEFAULT '["text"]'::jsonb,
    capabilities JSONB NOT NULL DEFAULT '{}'::jsonb,
    context_window INTEGER NOT NULL DEFAULT 128000,
    max_output_tokens INTEGER NOT NULL DEFAULT 4096,
    cost JSONB NOT NULL DEFAULT '{}'::jsonb,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 3. ai_cache_metrics
CREATE TABLE IF NOT EXISTS ai_cache_metrics (
    id BIGSERIAL PRIMARY KEY,
    entity_id TEXT NOT NULL,
    run_id TEXT NULL,
    model_id TEXT NULL,
    cache_type TEXT NOT NULL,
    cache_key_hash TEXT NOT NULL,
    hit BOOLEAN NOT NULL DEFAULT FALSE,
    read_tokens INTEGER NOT NULL DEFAULT 0,
    write_tokens INTEGER NOT NULL DEFAULT 0,
    cached_tokens INTEGER NOT NULL DEFAULT 0,
    estimated_savings JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_ai_cache_metrics_entity_created
    ON ai_cache_metrics(entity_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_ai_cache_metrics_run_id
    ON ai_cache_metrics(run_id);

-- 4. ai_mcp_servers
CREATE TABLE IF NOT EXISTS ai_mcp_servers (
    server_id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    description TEXT NULL,
    transport TEXT NOT NULL CHECK (transport IN ('stdio', 'streamable_http')),
    command_ref TEXT NULL,
    endpoint_url TEXT NULL,
    args JSONB NOT NULL DEFAULT '[]'::jsonb,
    env_secret_refs JSONB NOT NULL DEFAULT '[]'::jsonb,
    risk_policy JSONB NOT NULL DEFAULT '{}'::jsonb,
    timeout_seconds INTEGER NOT NULL DEFAULT 10,
    startup_timeout_seconds INTEGER NOT NULL DEFAULT 5,
    max_concurrency INTEGER NOT NULL DEFAULT 4,
    status TEXT NOT NULL DEFAULT 'draft',
    last_probe_status TEXT NULL,
    last_probe_at TIMESTAMPTZ NULL,
    last_error TEXT NULL,
    config_hash TEXT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 5. ai_mcp_server_capabilities
CREATE TABLE IF NOT EXISTS ai_mcp_server_capabilities (
    server_id TEXT PRIMARY KEY REFERENCES ai_mcp_servers(server_id) ON DELETE CASCADE,
    protocol_version TEXT NULL,
    tools JSONB NOT NULL DEFAULT '[]'::jsonb,
    resources JSONB NOT NULL DEFAULT '[]'::jsonb,
    prompts JSONB NOT NULL DEFAULT '[]'::jsonb,
    schema_hash TEXT NOT NULL,
    discovered_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NULL
);

-- 6. ai_entity_mcp_bindings
CREATE TABLE IF NOT EXISTS ai_entity_mcp_bindings (
    binding_id TEXT PRIMARY KEY,
    entity_id TEXT NOT NULL REFERENCES ai_entities(id) ON DELETE CASCADE,
    server_id TEXT NOT NULL REFERENCES ai_mcp_servers(server_id) ON DELETE RESTRICT,
    enabled BOOLEAN NOT NULL DEFAULT FALSE,
    allowed_tools JSONB NOT NULL DEFAULT '[]'::jsonb,
    denied_tools JSONB NOT NULL DEFAULT '[]'::jsonb,
    allowed_resources JSONB NOT NULL DEFAULT '[]'::jsonb,
    allowed_prompts JSONB NOT NULL DEFAULT '[]'::jsonb,
    tool_defaults JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(entity_id, server_id)
);

CREATE INDEX IF NOT EXISTS idx_ai_entity_mcp_bindings_entity
    ON ai_entity_mcp_bindings(entity_id);

-- 7. ai_agent_skill_registry
CREATE TABLE IF NOT EXISTS ai_agent_skill_registry (
    skill_slug TEXT NOT NULL,
    version TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NULL,
    source TEXT NOT NULL,
    canonical_path TEXT NOT NULL,
    entry_file TEXT NOT NULL DEFAULT 'SKILL.md',
    frontmatter JSONB NOT NULL DEFAULT '{}'::jsonb,
    content_hash TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'draft',
    indexed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    reviewed_by TEXT NULL,
    reviewed_at TIMESTAMPTZ NULL,
    last_probe_status TEXT NULL,
    last_probe_at TIMESTAMPTZ NULL,
    last_error TEXT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (skill_slug, version)
);

-- 8. ai_entity_skill_bindings
CREATE TABLE IF NOT EXISTS ai_entity_skill_bindings (
    binding_id TEXT PRIMARY KEY,
    entity_id TEXT NOT NULL REFERENCES ai_entities(id) ON DELETE CASCADE,
    skill_slug TEXT NOT NULL,
    version TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT FALSE,
    priority INTEGER NOT NULL DEFAULT 100,
    activation_policy TEXT NOT NULL DEFAULT 'task_routed',
    allowed_task_types JSONB NOT NULL DEFAULT '[]'::jsonb,
    allowed_reference_paths JSONB NOT NULL DEFAULT '[]'::jsonb,
    allow_scripts BOOLEAN NOT NULL DEFAULT FALSE,
    max_instruction_tokens INTEGER NOT NULL DEFAULT 3000,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    FOREIGN KEY (skill_slug, version) REFERENCES ai_agent_skill_registry(skill_slug, version) ON DELETE RESTRICT,
    UNIQUE(entity_id, skill_slug, version)
);

CREATE INDEX IF NOT EXISTS idx_ai_entity_skill_bindings_entity
    ON ai_entity_skill_bindings(entity_id, priority ASC);
