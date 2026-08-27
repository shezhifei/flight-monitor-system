/**
 * AI 配置 TypeScript 类型定义
 *
 * 与后端 AiEntityConfig 对齐的前端类型。
 */

// === 枚举 ===

export type ApiFormat = 'chat_completions' | 'responses';
export type ModalityType = 'text' | 'image' | 'audio' | 'file';
export type WriteActionPolicy = 'proposal_only' | 'direct_execute';
export type ContextStrategy = 'sliding_window' | 'summary_compression' | 'hybrid';
export type McpTransport = 'stdio' | 'streamable_http';
export type SkillActivationPolicy = 'always' | 'task_routed' | 'manual';
export type SubagentsMode = 'entity_handoff';

// === 模型配置 ===

export interface ModelCapabilities {
  tool_calling: boolean;
  parallel_tool_calls: boolean;
  streaming: boolean;
  structured_output: boolean;
  prompt_cache: boolean;
}

export interface ModelModalities {
  input: ModalityType[];
  output: ModalityType[];
}

export interface ModelCost {
  currency: string;
  input_per_1k: number;
  output_per_1k: number;
  cached_input_per_1k: number;
}

export interface ModelConfig {
  provider_model: string;
  api_format: ApiFormat;
  context_window: number;
  max_output_tokens: number;
  modalities: ModelModalities;
  capabilities: ModelCapabilities;
  cost: ModelCost;
  /**
   * 指向 AiEntityConfig.providers 的键（如 'default'/'asr'/'tts'）。
   * 缺省为 'default'，解析端在键缺失时回退到 'default'。
   */
  provider_ref: string;
}

// === Provider 配置 ===

export interface ProviderConfig {
  type: string;
  base_url: string;
  api_key: string;
  api_format: ApiFormat;
  timeout: number;
  max_retries: number;
  retry_delay: number;
}

// === 模型路由 ===

export interface ModelRouting {
  default: string;
  chat?: string;
  summary?: string;
  vision?: string;
  audio_transcription?: string;
  audio_speech?: string;
  embedding?: string;
}

// === 工具配置 ===

export interface ToolingConfig {
  enabled: boolean;
  max_rounds: number;
  allow_parallel: boolean;
  allowed_tool_sources: string[];
  allowed_tool_categories: string[];
  allowed_tools: string[] | null;
  denied_tools: string[];
  write_action_policy: WriteActionPolicy;
}

// === MCP 配置 ===

export interface McpServerRef {
  server_id: string;
  enabled: boolean;
}

export interface McpConfig {
  enabled: boolean;
  servers: McpServerRef[];
  tool_name_prefix: string;
  discovery_cache_ttl_seconds: number;
  fail_closed: boolean;
}

// === Skills 配置 ===

export interface SkillBindingRef {
  skill_slug: string;
  version?: string;
  enabled: boolean;
}

export interface SkillsConfig {
  enabled: boolean;
  allowlist: string[];
  bindings: SkillBindingRef[];
  fail_closed: boolean;
}

// === 子代理配置 ===

export interface SubagentsConfig {
  enabled: boolean;
  mode: SubagentsMode;
  allowed_entity_ids: string[];
  max_depth: number;
  max_concurrency: number;
  inherit_parent_context: boolean;
  require_tool_calling_capability: boolean;
  handoff_prompt?: string;
}

// === 上下文策略 ===

export interface ContextPolicyConfig {
  strategy: ContextStrategy;
  max_context_tokens: number;
  compression_threshold_tokens: number;
  preserve_recent_messages: number;
  summary_model?: string;
  summary_max_tokens: number;
  persist_summaries: boolean;
  risk_ceiling: 'low' | 'medium' | 'high' | 'critical';
}

// === 缓存策略 ===

export interface ProviderPromptCacheConfig {
  enabled: boolean;
  retention: string;
  key_namespace: string;
}

export interface ContextCacheConfig {
  backend: string;
  ttl_seconds: number;
}

export interface ToolResultCacheConfig {
  enabled: boolean;
  ttl_seconds: number;
  cacheable_tools: string[];
}

export interface McpResourceCacheConfig {
  enabled: boolean;
  ttl_seconds: number;
}

export interface CachePolicyConfig {
  enabled: boolean;
  provider_prompt_cache: ProviderPromptCacheConfig;
  context_cache: ContextCacheConfig;
  tool_result_cache: ToolResultCacheConfig;
  mcp_resource_cache: McpResourceCacheConfig;
}

// === 安全配置 ===

export interface SecurityConfig {
  mask_sensitive: boolean;
  log_prompts: boolean;
  max_input_bytes: number;
  allowed_input_mime_types: string[];
}

// === 顶层实体配置 ===

export interface AiEntityConfig {
  config_version: number;
  /** provider 字典，键如 'default'/'asr'/'tts'。模型通过 provider_ref 引用。 */
  providers: Record<string, ProviderConfig>;
  model_routing: ModelRouting;
  models: Record<string, ModelConfig>;
  tooling: ToolingConfig;
  mcp: McpConfig;
  skills: SkillsConfig;
  subagents: SubagentsConfig;
  context_policy: ContextPolicyConfig;
  cache_policy: CachePolicyConfig;
  security: SecurityConfig;
  system_prompt: string;
  task_template?: string;
}

// === MCP Server 定义 ===

export interface McpRiskPolicy {
  network_access: boolean;
  filesystem_access: string;
  max_response_bytes: number;
}

export interface McpServerDefinition {
  id: string;
  enabled: boolean;
  display_name: string;
  description?: string;
  transport: McpTransport;
  command_ref?: string;
  endpoint_url?: string;
  args: string[];
  env_secret_refs: string[];
  timeout_seconds: number;
  startup_timeout_seconds: number;
  max_concurrency: number;
  risk: McpRiskPolicy;
}

export interface McpServerCapabilities {
  server_id: string;
  protocol_version?: string;
  tools: Array<{
    name: string;
    description: string;
    inputSchema: Record<string, unknown>;
  }>;
  resources: Array<{
    uri: string;
    name: string;
    mimeType: string;
  }>;
  prompts: Array<{
    name: string;
    description: string;
  }>;
  schema_hash: string;
  discovered_at: string;
  expires_at?: string;
}

export interface McpEntityBinding {
  binding_id: string;
  entity_id: string;
  server_id: string;
  enabled: boolean;
  allowed_tools: string[];
  denied_tools: string[];
  allowed_resources: string[];
  allowed_prompts: string[];
  tool_defaults: Record<string, unknown>;
}

// === Agent Skill 定义 ===

export interface SkillRegistryEntry {
  skill_slug: string;
  version: string;
  name: string;
  description?: string;
  source: string;
  canonical_path: string;
  entry_file: string;
  frontmatter: Record<string, unknown>;
  content_hash: string;
  status: string;
  indexed_at: string;
}

export interface SkillEntityBinding {
  binding_id: string;
  entity_id: string;
  skill_slug: string;
  version: string;
  enabled: boolean;
  priority: number;
  activation_policy: SkillActivationPolicy;
  allowed_task_types: string[];
  allowed_reference_paths: string[];
  allow_scripts: boolean;
  max_instruction_tokens: number;
}

// === 运行时快照 ===

export interface ResolvedCapabilitySnapshot {
  entity_id: string;
  config_version: number;
  config_revision: number;
  model_id: string;
  provider_model: string;
  api_format: ApiFormat;
  input_modalities: ModalityType[];
  output_modalities: ModalityType[];
  // 后端快照已携带 security（含 allowed_input_mime_types / max_input_bytes）；
  // 前端据此约束聊天输入的文件 accept 白名单。
  security?: SecurityConfig;
  tool_count: number;
  mcp_enabled: boolean;
  skills_enabled: boolean;
  subagents_enabled: boolean;
  cache_enabled: boolean;
  context_strategy: ContextStrategy;
  snapshot_hash: string;
}

// === 缓存指标 ===

export interface CacheMetricRecord {
  id: number;
  entity_id: string;
  run_id?: string;
  model_id?: string;
  cache_type: string;
  cache_key_hash: string;
  hit: boolean;
  read_tokens: number;
  write_tokens: number;
  cached_tokens: number;
  estimated_savings: Record<string, unknown>;
  created_at: string;
}

export interface CacheMetricsSummary {
  period_hours: number;
  by_cache_type: Array<{
    cache_type: string;
    total_events: number;
    hits: number;
    misses: number;
    total_cached_tokens: number;
    total_read_tokens: number;
    total_write_tokens: number;
  }>;
}

// === 能力快照丰富类型 (后端 enrichment) ===

export interface McpServerDiscoveryEntry {
  server_id: string;
  display_name: string;
  enabled: boolean;
  transport: string;
  command_ref_present: boolean;
  command_allowlisted: boolean | null;
  allowlist_configured: boolean;
  server_status: string;
  discovery_status: 'discovered' | 'not_discovered' | 'unsupported_transport' | 'command_not_allowlisted' | 'error';
}

export interface SkillBindingDetail {
  binding_id: string;
  skill_slug: string;
  version: string;
  enabled: boolean;
  activation_policy: SkillActivationPolicy;
  priority: number;
  max_instruction_tokens: number;
}

export interface SubagentRisk {
  enabled_no_allowlist: boolean;
  high_depth: boolean;
}

export interface CacheBackends {
  provider_prompt_cache: boolean;
  provider_prompt_cache_retention: string | null;
  provider_prompt_cache_namespace: string | null;
  tool_result_cache: boolean;
  tool_result_cache_ttl: number;
  tool_result_cacheable_tools: string[];
  mcp_resource_cache: boolean;
  mcp_resource_cache_note: string;
  context_cache: boolean;
  context_cache_note: string;
  context_cache_ttl: number;
}

export interface EnrichedCapabilitySnapshot extends ResolvedCapabilitySnapshot {
  builtin_tool_count: number;
  mcp_tool_count: number;
  subagent_tool_enabled: boolean;
  skill_instruction_count: number;
  mcp_servers: McpServerDiscoveryEntry[];
  mcp_server_count: number;
  mcp_binding_count: number;
  mcp_allowlist_configured: boolean;
  skill_bindings: SkillBindingDetail[];
  subagent_risk: SubagentRisk;
  cache_backends: CacheBackends;
  skills?: {
    fail_closed?: boolean;
  };
  subagents?: {
    allowed_entity_ids?: string[];
    max_depth?: number;
    max_concurrency?: number;
    inherit_parent_context?: boolean;
  };
}

export interface ValidationError {
  code: string;
  message: string;
  severity: 'error' | 'warning';
}

export interface ValidationResult {
  valid: boolean;
  errors: ValidationError[];
}

// === 缓存指标 hit_rate 计算 ===

export function computeHitRate(hits: number, misses: number): string {
  const total = hits + misses;
  if (total === 0) return '—';
  return `${((hits / total) * 100).toFixed(1)}%`;
}

/**
 * 按 model.provider_ref 取 provider，缺失/未知键回退 'default'。
 */
export function resolveProviderRef(
  providers: Record<string, ProviderConfig>,
  providerRef?: string | null,
): ProviderConfig {
  const ref = providerRef || 'default';
  return providers[ref] ?? providers.default;
}

// === 默认值工厂函数 ===

export function createDefaultModelConfig(): ModelConfig {
  return {
    provider_model: 'gpt-4o',
    api_format: 'chat_completions',
    context_window: 128000,
    max_output_tokens: 4096,
    modalities: {
      input: ['text'],
      output: ['text'],
    },
    capabilities: {
      tool_calling: true,
      parallel_tool_calls: false,
      streaming: true,
      structured_output: false,
      prompt_cache: false,
    },
    cost: {
      currency: 'USD',
      input_per_1k: 0,
      output_per_1k: 0,
      cached_input_per_1k: 0,
    },
    provider_ref: 'default',
  };
}

export function createDefaultProviderConfig(): ProviderConfig {
  return {
    type: 'openai_compatible',
    base_url: 'https://api.openai.com/v1',
    api_key: '',
    api_format: 'chat_completions',
    timeout: 30,
    max_retries: 3,
    retry_delay: 0.5,
  };
}

export function createDefaultToolingConfig(): ToolingConfig {
  return {
    enabled: true,
    max_rounds: 5,
    allow_parallel: false,
    allowed_tool_sources: ['builtin'],
    allowed_tool_categories: ['flight', 'flight_event', 'todo', 'business_case'],
    allowed_tools: null,
    denied_tools: [],
    write_action_policy: 'proposal_only',
  };
}

export function createDefaultMcpConfig(): McpConfig {
  return {
    enabled: false,
    servers: [],
    tool_name_prefix: 'mcp',
    discovery_cache_ttl_seconds: 300,
    fail_closed: false,
  };
}

export function createDefaultSkillsConfig(): SkillsConfig {
  return {
    enabled: false,
    allowlist: [],
    bindings: [],
    fail_closed: false,
  };
}

export function createDefaultSubagentsConfig(): SubagentsConfig {
  return {
    enabled: false,
    mode: 'entity_handoff',
    allowed_entity_ids: [],
    max_depth: 1,
    max_concurrency: 2,
    inherit_parent_context: true,
    require_tool_calling_capability: true,
  };
}

export function createDefaultContextPolicy(): ContextPolicyConfig {
  return {
    strategy: 'hybrid',
    max_context_tokens: 64000,
    compression_threshold_tokens: 48000,
    preserve_recent_messages: 12,
    summary_max_tokens: 1200,
    persist_summaries: true,
    risk_ceiling: 'medium',
  };
}

export function createDefaultCachePolicy(): CachePolicyConfig {
  return {
    enabled: true,
    provider_prompt_cache: {
      enabled: false,
      retention: '24h',
      key_namespace: 'flight_monitor',
    },
    context_cache: {
      backend: 'redis',
      ttl_seconds: 86400,
    },
    tool_result_cache: {
      enabled: false,
      ttl_seconds: 60,
      cacheable_tools: [],
    },
    mcp_resource_cache: {
      enabled: false,
      ttl_seconds: 300,
    },
  };
}

export function createDefaultSecurityConfig(): SecurityConfig {
  return {
    mask_sensitive: true,
    log_prompts: false,
    max_input_bytes: 26214400,
    allowed_input_mime_types: ['text/plain', 'image/png', 'image/jpeg', 'audio/wav'],
  };
}

export function createDefaultEntityConfig(): AiEntityConfig {
  const defaultProvider = createDefaultProviderConfig();
  return {
    config_version: 2,
    providers: { default: { ...defaultProvider } },
    model_routing: {
      default: 'gpt-4o',
      chat: 'gpt-4o',
      summary: 'gpt-4o-mini',
    },
    models: {},
    tooling: createDefaultToolingConfig(),
    mcp: createDefaultMcpConfig(),
    skills: createDefaultSkillsConfig(),
    subagents: createDefaultSubagentsConfig(),
    context_policy: createDefaultContextPolicy(),
    cache_policy: createDefaultCachePolicy(),
    security: createDefaultSecurityConfig(),
    system_prompt: '你是一个航班监控系统的 AI 助手。',
  };
}
