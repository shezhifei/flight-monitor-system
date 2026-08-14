import type { ApiResult } from '@/composables/useApi';
import { useApi } from '@/composables/useApi';
import { readApiErrorMessage, unwrapApiDataOrThrow } from '@/shared/apiEnvelope';
import type {
  McpServerDefinition,
  McpServerCapabilities,
  McpEntityBinding,
  SkillRegistryEntry,
  SkillEntityBinding,
  EnrichedCapabilitySnapshot,
  ValidationResult,
  CacheMetricsSummary,
} from './aiConfigTypes';

const AI_BASE = '/api/v2/ai';

export interface AiEntitySummary {
  id: string;
}

export interface AiEntityDetail {
  id?: string;
  system_prompt?: string;
  task_template?: string;
  providers?: Record<string, unknown>;
  model_routing?: Record<string, unknown>;
  models?: Record<string, unknown>;
  tooling?: Record<string, unknown>;
  media?: Record<string, unknown>;
  [key: string]: unknown;
}

export interface AiEntityListResponse {
  entities?: AiEntitySummary[];
}

export interface AiModelOption {
  id?: string;
  name?: string;
  provider?: string;
  owned_by?: string;
  value?: string;
}

export interface AiModelsResponse {
  models?: AiModelOption[];
}

export interface AiToolCategory {
  name?: string;
  tools?: string[];
}

export interface AiToolCategoriesResponse {
  categories?: AiToolCategory[];
}

export interface AiTestConnectionPayload {
  entity_id: string;
  base_url?: string;
  api_key?: string;
  include_models?: boolean;
}

export interface AiTestConnectionResponse {
  models?: AiModelOption[];
  ok?: boolean;
  [key: string]: unknown;
}

export interface AiOntologyFieldDef {
  name?: string;
  field_type?: string;
  type?: string;
  description?: string;
  required?: boolean;
}

export interface AiOntologyRelationDef {
  name?: string;
  target_object?: string;
  relation_type?: string;
  cardinality?: string;
  description?: string;
}

export interface AiOntologyObjectDef {
  name?: string;
  description?: string;
  object_id_strategy?: string;
  fields?: Record<string, AiOntologyFieldDef>;
  relations?: Record<string, AiOntologyRelationDef>;
  actions?: Record<string, unknown>;
  is_active?: boolean;
  enabled?: boolean;
  disabled?: boolean;
}

export interface AiOntologyActionRouteRow {
  object?: string;
  action?: string;
  is_active?: boolean;
  enabled?: boolean;
  disabled?: boolean;
  definition?: {
    name?: string;
    description?: string;
    category?: string;
    parameters?: Record<string, AiOntologyFieldDef>;
    required_permissions?: string[];
    risk_level?: string;
    approval_strategy?: string;
    approval_policy?: string;
    constraints?: unknown[];
    is_active?: boolean;
    enabled?: boolean;
    disabled?: boolean;
  };
}

export interface AiEntitySavePayload {
  config_version?: number;
  system_prompt?: string;
  task_template?: string;
  providers?: Record<string, unknown>;
  model_routing?: Record<string, unknown>;
  models?: Record<string, unknown>;
  tooling?: Record<string, unknown>;
  media?: Record<string, unknown>;
  mcp?: Record<string, unknown>;
  skills?: Record<string, unknown>;
  subagents?: Record<string, unknown>;
  context_policy?: Record<string, unknown>;
  cache_policy?: Record<string, unknown>;
  security?: Record<string, unknown>;
}

function unwrapEnvelope<T>(result: ApiResult<unknown>): T {
  if (!result.data || typeof result.data !== 'object') {
    throw new Error('AI 服务响应缺少数据负载');
  }
  return unwrapApiDataOrThrow<T>(result.data, readApiErrorMessage(result, 'AI 服务请求失败'));
}

function requireOk<T>(result: ApiResult<unknown>, fallbackMessage: string): T {
  if (!result.ok) {
    throw new Error(readApiErrorMessage(result, fallbackMessage));
  }
  return unwrapEnvelope<T>(result);
}

export function useAiConfigApi() {
  const api = useApi();

  async function listEntities(): Promise<AiEntitySummary[]> {
    const result = await api.get<AiEntityListResponse>(`${AI_BASE}/entities`);
    const payload = requireOk<AiEntityListResponse>(result, '加载实体列表失败');
    const rows = Array.isArray(payload.entities) ? payload.entities : [];
    return rows
      .map((row) => ({ id: String((row as { id?: unknown }).id || '') }))
      .filter((row) => row.id);
  }

  async function getEntity(entityId: string): Promise<AiEntityDetail> {
    const result = await api.get<AiEntityDetail>(
      `${AI_BASE}/entities/${encodeURIComponent(entityId)}`,
    );
    return requireOk<AiEntityDetail>(result, '加载实体详情失败');
  }

  async function saveEntity(
    entityId: string,
    payload: AiEntitySavePayload,
  ): Promise<AiEntityDetail> {
    const result = await api.put<AiEntityDetail>(
      `${AI_BASE}/entities/${encodeURIComponent(entityId)}`,
      payload,
    );
    return requireOk<AiEntityDetail>(result, '保存实体配置失败');
  }

  async function listModels(): Promise<AiModelOption[]> {
    const result = await api.get<AiModelsResponse>(`${AI_BASE}/models`);
    const payload = requireOk<AiModelsResponse>(result, '加载模型列表失败');
    return Array.isArray(payload.models) ? payload.models : [];
  }

  async function listToolCategories(): Promise<AiToolCategory[]> {
    const result = await api.get<AiToolCategoriesResponse>(`${AI_BASE}/tools/categories`);
    const payload = requireOk<AiToolCategoriesResponse>(result, '加载工具分类失败');
    return Array.isArray(payload.categories) ? payload.categories : [];
  }

  async function testConnection(
    payload: AiTestConnectionPayload,
  ): Promise<AiTestConnectionResponse> {
    const result = await api.post<AiTestConnectionResponse>(
      `${AI_BASE}/connection/test`,
      payload,
    );
    return requireOk<AiTestConnectionResponse>(result, '连通测试失败');
  }

  async function listOntologyObjects(): Promise<AiOntologyObjectDef[]> {
    const result = await api.get<AiOntologyObjectDef[]>(`${AI_BASE}/ontology/objects`);
    const payload = requireOk<AiOntologyObjectDef[] | { items?: AiOntologyObjectDef[] }>(
      result,
      '加载 Ontology 对象失败',
    );
    if (Array.isArray(payload)) {
      return payload;
    }
    return Array.isArray(payload.items) ? payload.items : [];
  }

  async function listOntologyActions(): Promise<AiOntologyActionRouteRow[]> {
    const result = await api.get<AiOntologyActionRouteRow[]>(`${AI_BASE}/ontology/actions`);
    const payload = requireOk<AiOntologyActionRouteRow[] | { items?: AiOntologyActionRouteRow[] }>(
      result,
      '加载 Ontology 动作失败',
    );
    if (Array.isArray(payload)) {
      return payload;
    }
    return Array.isArray(payload.items) ? payload.items : [];
  }

  async function getEntityCapabilities(entityId: string): Promise<EnrichedCapabilitySnapshot> {
    const result = await api.get<EnrichedCapabilitySnapshot>(`${AI_BASE}/entities/${entityId}/capabilities`);
    return requireOk<EnrichedCapabilitySnapshot>(result, '加载能力快照失败');
  }

  async function validateEntityCapabilities(entityId: string): Promise<ValidationResult> {
    const result = await api.post(`${AI_BASE}/entities/${entityId}/capabilities/validate`);
    return requireOk<ValidationResult>(result, '校验能力失败');
  }

  async function listMcpServers(entityId: string): Promise<McpServerDefinition[]> {
    const result = await api.get<McpServerDefinition[]>(`${AI_BASE}/entities/${entityId}/mcp/servers`);
    return requireOk<McpServerDefinition[]>(result, '加载 MCP 服务器失败');
  }

  async function probeMcpServer(
    entityId: string,
    serverId: string,
  ): Promise<{ status: string; capabilities?: McpServerCapabilities }> {
    const result = await api.post(`${AI_BASE}/entities/${entityId}/mcp/servers/${serverId}/probe`);
    return requireOk(result, '探测 MCP 服务器失败');
  }

  async function listMcpBindings(entityId: string): Promise<McpEntityBinding[]> {
    const result = await api.get<McpEntityBinding[]>(`${AI_BASE}/entities/${entityId}/mcp/bindings`);
    return requireOk<McpEntityBinding[]>(result, '加载 MCP 绑定失败');
  }

  async function saveMcpBinding(
    entityId: string,
    binding: Partial<McpEntityBinding>,
  ): Promise<McpEntityBinding> {
    const result = await api.post(`${AI_BASE}/entities/${entityId}/mcp/bindings`, binding);
    return requireOk<McpEntityBinding>(result, '保存 MCP 绑定失败');
  }

  async function listSkillRegistry(): Promise<SkillRegistryEntry[]> {
    const result = await api.get<SkillRegistryEntry[]>(`${AI_BASE}/skills`);
    return requireOk<SkillRegistryEntry[]>(result, '加载 Skill 注册表失败');
  }

  async function listEntitySkills(entityId: string): Promise<SkillEntityBinding[]> {
    const result = await api.get<SkillEntityBinding[]>(`${AI_BASE}/entities/${entityId}/skills`);
    return requireOk<SkillEntityBinding[]>(result, '加载实体 Skill 失败');
  }

  async function saveSkillBinding(
    entityId: string,
    binding: Partial<SkillEntityBinding>,
  ): Promise<SkillEntityBinding> {
    const result = await api.post(`${AI_BASE}/entities/${entityId}/skills/bindings`, binding);
    return requireOk<SkillEntityBinding>(result, '保存 Skill 绑定失败');
  }

  async function deleteSkillBinding(entityId: string, bindingId: string): Promise<boolean> {
    const result = await api.delete<boolean>(`${AI_BASE}/entities/${entityId}/skills/bindings/${bindingId}`);
    return requireOk<boolean>(result, '删除 Skill 绑定失败');
  }

  async function getCacheMetrics(entityId?: string, hours: number = 24): Promise<CacheMetricsSummary> {
    const params = new URLSearchParams();
    if (entityId) params.set('entity_id', entityId);
    params.set('hours', hours.toString());
    const result = await api.get<CacheMetricsSummary>(`${AI_BASE}/cache/metrics?${params.toString()}`);
    return requireOk<CacheMetricsSummary>(result, '加载缓存指标失败');
  }

  async function invalidateCache(entityId: string, cacheType?: string): Promise<{ invalidated: number }> {
    const result = await api.post(`${AI_BASE}/cache/invalidate`, {
      entity_id: entityId,
      cache_type: cacheType,
    });
    return requireOk(result, '失效缓存失败');
  }

  async function testConnectionWithCapabilities(entityId: string): Promise<{
    connected: boolean;
    models: string[];
    capabilities: Record<string, unknown>;
  }> {
    const result = await api.post(`${AI_BASE}/connection/test`, {
      entity_id: entityId,
      include_models: true,
      include_capabilities: true,
    });
    return requireOk(result, '连通测试失败');
  }

  return {
    listEntities,
    getEntity,
    saveEntity,
    listModels,
    listToolCategories,
    testConnection,
    listOntologyObjects,
    listOntologyActions,
    getEntityCapabilities,
    validateEntityCapabilities,
    listMcpServers,
    probeMcpServer,
    listMcpBindings,
    saveMcpBinding,
    listSkillRegistry,
    listEntitySkills,
    saveSkillBinding,
    deleteSkillBinding,
    getCacheMetrics,
    invalidateCache,
    testConnectionWithCapabilities,
  };
}
