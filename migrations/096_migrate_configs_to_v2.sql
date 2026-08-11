-- 095_migrate_configs_to_v2.sql
-- 将现有 v1 配置迁移为 v2 格式

-- 迁移 default 实体
UPDATE ai_entities 
SET config = jsonb_build_object(
    'config_version', 2,
    'provider', jsonb_build_object(
        'type', 'openai_compatible',
        'base_url', config->>'base_url',
        'api_key', config->>'api_key',
        'api_format', COALESCE(config->>'api_format', 'chat_completions'),
        'timeout', COALESCE((config->>'timeout')::numeric, 30),
        'max_retries', COALESCE((config->>'max_retries')::int, 3),
        'retry_delay', COALESCE((config->>'retry_delay')::numeric, 0.5)
    ),
    'model_routing', jsonb_build_object(
        'default', COALESCE(config->>'default_model', 'gpt-3.5-turbo'),
        'chat', COALESCE(config->>'default_model', 'gpt-3.5-turbo'),
        'summary', 'gpt-4o-mini',
        'vision', null,
        'audio_transcription', null,
        'audio_speech', null,
        'embedding', null
    ),
    'models', jsonb_build_object(
        COALESCE(config->>'default_model', 'gpt-3.5-turbo'), jsonb_build_object(
            'provider_model', COALESCE(config->>'default_model', 'gpt-3.5-turbo'),
            'api_format', COALESCE(config->>'api_format', 'chat_completions'),
            'context_window', COALESCE((config->>'context_window')::int, 128000),
            'max_output_tokens', COALESCE((config->>'max_tokens')::int, 4096),
            'modalities', '{"input": ["text"], "output": ["text"]}'::jsonb,
            'capabilities', jsonb_build_object(
                'tool_calling', true,
                'parallel_tool_calls', false,
                'streaming', true,
                'structured_output', false,
                'prompt_cache', COALESCE((config->'prompt_cache'->>'enabled')::boolean, false)
            ),
            'cost', jsonb_build_object(
                'currency', 'USD',
                'input_per_1k', COALESCE((config->>'cost_per_1k_input')::numeric, 0),
                'output_per_1k', COALESCE((config->>'cost_per_1k_output')::numeric, 0),
                'cached_input_per_1k', 0
            )
        )
    ),
    'tooling', jsonb_build_object(
        'enabled', true,
        'max_rounds', 5,
        'allow_parallel', false,
        'allowed_tool_sources', '["builtin"]'::jsonb,
        'allowed_tool_categories', COALESCE(config->'allowed_tool_categories', '[]'::jsonb),
        'allowed_tools', config->'allowed_tools',
        'denied_tools', COALESCE(config->'denied_tools', '[]'::jsonb),
        'write_action_policy', 'proposal_only'
    ),
    'mcp', '{"enabled": false, "servers": [], "tool_name_prefix": "mcp", "discovery_cache_ttl_seconds": 300, "fail_closed": false}'::jsonb,
    'skills', '{"enabled": false, "allowlist": [], "bindings": [], "fail_closed": false}'::jsonb,
    'subagents', '{"enabled": false, "mode": "entity_handoff", "allowed_entity_ids": [], "max_depth": 1, "max_concurrency": 2, "inherit_parent_context": true, "require_tool_calling_capability": true}'::jsonb,
    'context_policy', jsonb_build_object(
        'strategy', 'hybrid',
        'max_context_tokens', 64000,
        'compression_threshold_tokens', 48000,
        'preserve_recent_messages', 12,
        'summary_model', 'gpt-4o-mini',
        'summary_max_tokens', 1200,
        'persist_summaries', true
    ),
    'cache_policy', jsonb_build_object(
        'enabled', true,
        'provider_prompt_cache', jsonb_build_object(
            'enabled', COALESCE((config->'prompt_cache'->>'enabled')::boolean, false),
            'retention', COALESCE(config->'prompt_cache'->>'retention', '24h'),
            'key_namespace', COALESCE(config->'prompt_cache'->>'namespace', 'flight_monitor')
        ),
        'context_cache', '{"backend": "redis", "ttl_seconds": 86400}'::jsonb,
        'tool_result_cache', '{"enabled": false, "ttl_seconds": 60, "cacheable_tools": []}'::jsonb,
        'mcp_resource_cache', '{"enabled": false, "ttl_seconds": 300}'::jsonb
    ),
    'security', '{"mask_sensitive": true, "log_prompts": false, "max_input_bytes": 26214400, "allowed_input_mime_types": ["text/plain", "image/png", "image/jpeg", "audio/wav"]}'::jsonb,
    'system_prompt', COALESCE(config->>'system_prompt', '你是一个航班监控系统的 AI 助手。'),
    'task_template', config->>'task_template'
),
config_version = 2,
config_revision = config_revision + 1,
updated_at = now()
WHERE config_version < 2 OR config_version IS NULL;
