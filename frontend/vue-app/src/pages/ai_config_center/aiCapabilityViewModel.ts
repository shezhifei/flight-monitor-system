/**
 * AI 能力快照 → UI View Model 纯函数
 *
 * 将后端 EnrichedCapabilitySnapshot 转换为 UI 展示用的结构化数据。
 * 不包含副作用，可独立测试。
 */

import type {
  EnrichedCapabilitySnapshot,
  ValidationError,
  ValidationResult,
  McpServerDiscoveryEntry,
  CacheMetricsSummary,
} from './aiConfigTypesV2';
import { computeHitRate } from './aiConfigTypesV2';

// === Status Badge Types ===

export type StatusLevel = 'ok' | 'disabled' | 'warning' | 'error';

export interface StatusBadge {
  label: string;
  level: StatusLevel;
  detail?: string;
}

// === Section View Models ===

export interface BasicInfoRow {
  key: string;
  label: string;
  value: string;
}

export interface ToolSection {
  total: number;
  builtin: number;
  mcp: number;
  subagentToolEnabled: boolean;
  emptyWarning: StatusBadge | null;
}

export interface McpServerRow {
  serverId: string;
  displayName: string;
  transport: string;
  commandRefPresent: boolean;
  commandAllowlisted: boolean | null;
  discoveryStatus: string;
  statusBadge: StatusBadge;
}

export interface McpSection {
  enabled: boolean;
  serverCount: number;
  bindingCount: number;
  allowlistConfigured: boolean;
  servers: McpServerRow[];
  noServersWarning: StatusBadge | null;
}

export interface SkillBindingRow {
  skillSlug: string;
  version: string;
  enabled: boolean;
  activationPolicy: string;
  priority: number;
  maxInstructionTokens: number;
}

export interface SkillSection {
  enabled: boolean;
  skillCount: number;
  instructionCount: number;
  failClosed: boolean;
  bindings: SkillBindingRow[];
  noBindingsWarning: StatusBadge | null;
}

export interface SubagentSection {
  enabled: boolean;
  allowedEntityIds: string[];
  maxDepth: number;
  maxConcurrency: number;
  inheritParentContext: boolean;
  riskBadge: StatusBadge | null;
}

export interface CacheBackendRow {
  key: string;
  label: string;
  enabled: boolean;
  note: string;
  detail?: string;
}

export interface CacheMetricRow {
  cacheType: string;
  events: number;
  hits: number;
  misses: number;
  hitRate: string;
  cachedTokens: number;
}

export interface CacheSection {
  enabled: boolean;
  backends: CacheBackendRow[];
  metrics: CacheMetricRow[];
}

export interface CapabilityViewModel {
  basicInfo: BasicInfoRow[];
  tools: ToolSection;
  mcp: McpSection;
  skills: SkillSection;
  subagents: SubagentSection;
  cache: CacheSection;
}

// === Conversion Functions ===

export function snapshotToViewModel(
  snapshot: EnrichedCapabilitySnapshot,
  cacheMetrics?: CacheMetricsSummary | null,
): CapabilityViewModel {
  return {
    basicInfo: buildBasicInfo(snapshot),
    tools: buildToolSection(snapshot),
    mcp: buildMcpSection(snapshot),
    skills: buildSkillSection(snapshot),
    subagents: buildSubagentSection(snapshot),
    cache: buildCacheSection(snapshot, cacheMetrics),
  };
}

export function buildBasicInfo(snapshot: EnrichedCapabilitySnapshot): BasicInfoRow[] {
  return [
    { key: 'entity_id', label: '实体 ID', value: snapshot.entity_id },
    { key: 'model_id', label: '模型', value: snapshot.model_id },
    { key: 'provider_model', label: 'Provider 模型', value: snapshot.provider_model },
    { key: 'api_format', label: 'API 格式', value: snapshot.api_format },
    { key: 'config_revision', label: '配置版本', value: String(snapshot.config_revision) },
    { key: 'snapshot_hash', label: '快照哈希', value: snapshot.snapshot_hash },
  ];
}

export function buildToolSection(snapshot: EnrichedCapabilitySnapshot): ToolSection {
  const total = snapshot.tool_count;
  const emptyWarning: StatusBadge | null =
    total === 0
      ? { label: '无可用工具', level: 'warning', detail: 'tool_count=0' }
      : null;

  return {
    total,
    builtin: snapshot.builtin_tool_count,
    mcp: snapshot.mcp_tool_count,
    subagentToolEnabled: snapshot.subagent_tool_enabled,
    emptyWarning,
  };
}

export function buildMcpSection(snapshot: EnrichedCapabilitySnapshot): McpSection {
  const servers: McpServerRow[] = (snapshot.mcp_servers || []).map((s) => ({
    serverId: s.server_id,
    displayName: s.display_name,
    transport: s.transport,
    commandRefPresent: s.command_ref_present,
    commandAllowlisted: s.command_allowlisted,
    discoveryStatus: s.discovery_status,
    statusBadge: mcpDiscoveryBadge(s),
  }));

  const noServersWarning: StatusBadge | null =
    snapshot.mcp_enabled && snapshot.mcp_server_count === 0
      ? { label: 'MCP 已启用但无活跃绑定', level: 'warning' }
      : null;

  return {
    enabled: snapshot.mcp_enabled,
    serverCount: snapshot.mcp_server_count,
    bindingCount: snapshot.mcp_binding_count,
    allowlistConfigured: snapshot.mcp_allowlist_configured,
    servers,
    noServersWarning,
  };
}

export function buildSkillSection(snapshot: EnrichedCapabilitySnapshot): SkillSection {
  const bindings: SkillBindingRow[] = (snapshot.skill_bindings || []).map((b) => ({
    skillSlug: b.skill_slug,
    version: b.version,
    enabled: b.enabled,
    activationPolicy: b.activation_policy,
    priority: b.priority,
    maxInstructionTokens: b.max_instruction_tokens,
  }));

  const noBindingsWarning: StatusBadge | null =
    snapshot.skills_enabled && snapshot.skill_instruction_count === 0
      ? { label: 'Skills 已启用但无活跃绑定', level: 'warning' }
      : null;

  return {
    enabled: snapshot.skills_enabled,
    skillCount: snapshot.skill_instruction_count,
    instructionCount: snapshot.skill_instruction_count,
    failClosed: snapshot.skills?.fail_closed ?? false,
    bindings,
    noBindingsWarning,
  };
}

export function buildSubagentSection(snapshot: EnrichedCapabilitySnapshot): SubagentSection {
  const risk = snapshot.subagent_risk;
  let riskBadge: StatusBadge | null = null;

  if (snapshot.subagents_enabled && risk?.enabled_no_allowlist) {
    riskBadge = {
      label: '启用但禁止委派',
      level: 'warning',
      detail: 'allowed_entity_ids 为空，不会委派任何实体',
    };
  } else if (snapshot.subagents_enabled && risk?.high_depth) {
    riskBadge = {
      label: '递归深度 > 1',
      level: 'warning',
      detail: `max_depth=${snapshot.subagents?.max_depth ?? '?'}`,
    };
  }

  return {
    enabled: snapshot.subagents_enabled,
    allowedEntityIds: snapshot.subagents?.allowed_entity_ids ?? [],
    maxDepth: snapshot.subagents?.max_depth ?? 1,
    maxConcurrency: snapshot.subagents?.max_concurrency ?? 2,
    inheritParentContext: snapshot.subagents?.inherit_parent_context ?? true,
    riskBadge,
  };
}

export function buildCacheSection(
  snapshot: EnrichedCapabilitySnapshot,
  cacheMetrics?: CacheMetricsSummary | null,
): CacheSection {
  const backends: CacheBackendRow[] = [];
  const cb = snapshot.cache_backends;

  if (cb) {
    backends.push({
      key: 'provider_prompt_cache',
      label: 'Provider Prompt Cache',
      enabled: cb.provider_prompt_cache,
      note: cb.provider_prompt_cache ? 'active' : 'disabled',
      detail: cb.provider_prompt_cache
        ? `retention=${cb.provider_prompt_cache_retention ?? '—'}, namespace=${cb.provider_prompt_cache_namespace ?? '—'}`
        : undefined,
    });
    backends.push({
      key: 'tool_result_cache',
      label: 'Tool Result Cache',
      enabled: cb.tool_result_cache,
      note: cb.tool_result_cache ? 'active' : 'disabled',
      detail: cb.tool_result_cache
        ? `ttl=${cb.tool_result_cache_ttl}s, tools=[${(cb.tool_result_cacheable_tools || []).join(', ')}]`
        : undefined,
    });
    backends.push({
      key: 'mcp_resource_cache',
      label: 'MCP Resource Cache',
      enabled: cb.mcp_resource_cache,
      note: cb.mcp_resource_cache_note,
    });
    backends.push({
      key: 'context_cache',
      label: 'Context Cache',
      enabled: cb.context_cache,
      note: cb.context_cache_note,
      detail: cb.context_cache ? `ttl=${cb.context_cache_ttl}s` : undefined,
    });
  }

  const metrics: CacheMetricRow[] = (cacheMetrics?.by_cache_type || []).map((m) => ({
    cacheType: m.cache_type,
    events: m.total_events,
    hits: m.hits,
    misses: m.misses,
    hitRate: computeHitRate(m.hits, m.misses),
    cachedTokens: m.total_cached_tokens,
  }));

  return {
    enabled: snapshot.cache_enabled,
    backends,
    metrics,
  };
}

// === Badge Helpers ===

export function mcpDiscoveryBadge(server: McpServerDiscoveryEntry): StatusBadge {
  switch (server.discovery_status) {
    case 'discovered':
      return { label: '已发现', level: 'ok' };
    case 'not_discovered':
      return { label: '未发现', level: 'warning' };
    case 'unsupported_transport':
      return { label: '不支持的传输', level: 'warning' };
    case 'command_not_allowlisted':
      return {
        label: '配置未授权',
        level: 'error',
        detail: 'command_ref 不在 AI_MCP_COMMAND_ALLOWLIST_JSON 中',
      };
    case 'error':
      return { label: '发现失败', level: 'error' };
    default:
      return { label: server.discovery_status, level: 'warning' };
  }
}

export function validationErrorBadge(err: ValidationError): StatusBadge {
  return {
    label: err.code,
    level: err.severity === 'error' ? 'error' : 'warning',
    detail: err.message,
  };
}

/**
 * 所有验证条目（error + warning）都应展示，与 valid 无关。
 *
 * 后端在“只有 warning”时仍返回 valid=true（warning 不阻断），但这些
 * warning 代表真实的配置风险（如 MCP 已启用但无服务器、Subagent 空白名单），
 * 不能因为 valid=true 而被前端吞掉。
 */
export function validationBadges(validation: ValidationResult | null): StatusBadge[] {
  if (!validation || !validation.errors) return [];
  return validation.errors.map(validationErrorBadge);
}

export interface ValidationToast {
  level: 'success' | 'warning' | 'error';
  message: string;
  errorCount: number;
  warnCount: number;
}

/**
 * 根据验证结果决定提示级别与文案：
 *  - 有 error          -> error  “验证失败: N 个错误[, M 个警告]”
 *  - 仅 warning (valid) -> warning“验证通过，存在 M 个警告”
 *  - 无任何条目         -> success“能力配置验证通过”
 */
export function summarizeValidation(validation: ValidationResult | null): ValidationToast {
  const errors = validation?.errors ?? [];
  const errorCount = errors.filter((e) => e.severity === 'error').length;
  const warnCount = errors.filter((e) => e.severity === 'warning').length;

  if (errorCount > 0) {
    const parts: string[] = [`${errorCount} 个错误`];
    if (warnCount > 0) parts.push(`${warnCount} 个警告`);
    return { level: 'error', message: `验证失败: ${parts.join(', ')}`, errorCount, warnCount };
  }
  if (warnCount > 0) {
    return { level: 'warning', message: `验证通过，存在 ${warnCount} 个警告`, errorCount, warnCount };
  }
  return { level: 'success', message: '能力配置验证通过', errorCount, warnCount };
}
