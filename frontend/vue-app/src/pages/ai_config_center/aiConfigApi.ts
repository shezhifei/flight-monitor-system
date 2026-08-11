import type { ApiResult } from '@/composables/useApi';
import { useApi } from '@/composables/useApi';

const AI_BASE = '/api/v2/ai';

export interface AiEntitySummary {
  id: string;
}

export interface AiEntityDetail {
  id?: string;
  base_url?: string;
  api_key?: string;
  default_model?: string;
  asr_model?: string;
  tts_model?: string;
  tts_voice?: string;
  api_format?: string;
  timeout?: number;
  max_retries?: number;
  retry_delay?: number;
  system_prompt?: string;
  task_template?: string;
  denied_tools?: string[];
  /**
   * 多 provider 字典（键如 'default'/'asr'/'tts'）。后端 normalizer 会用顶层
   * base_url/api_key/api_format 补齐 providers['default']，旧 config 缺省即只有 'default'。
   */
  providers?: Record<string, unknown>;
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
  base_url?: string;
  api_key?: string;
  default_model?: string;
  asr_model?: string;
  tts_model?: string;
  tts_voice?: string;
  api_format?: string;
  timeout?: number;
  max_retries?: number;
  retry_delay?: number;
  system_prompt?: string;
  task_template?: string;
  denied_tools?: string[];
  allowed_tool_categories?: string[];
  // 多 provider 字典；'default' 与顶层 base_url/api_key/api_format 镜像保持一致。
  providers?: Record<string, unknown>;
  model_routing?: Record<string, unknown>;
  models?: Record<string, unknown>;
  tooling?: Record<string, unknown>;
  mcp?: Record<string, unknown>;
  skills?: Record<string, unknown>;
  subagents?: Record<string, unknown>;
  context_policy?: Record<string, unknown>;
  cache_policy?: Record<string, unknown>;
  security?: Record<string, unknown>;
}

function unwrapEnvelope<T>(result: ApiResult<unknown>): T {
  const body = result.data as Record<string, unknown> | null;
  if (body && typeof body === 'object' && 'data' in body) {
    return body.data as T;
  }
  return (body ?? {}) as T;
}

function readErrorMessage(result: ApiResult<unknown>, fallback: string): string {
  const body = result.data as Record<string, unknown> | null;
  if (body && typeof body === 'object') {
    const err = body.error;
    if (typeof err === 'string' && err) return err;
    if (err && typeof err === 'object' && typeof (err as { message?: unknown }).message === 'string') {
      return (err as { message: string }).message;
    }
    const msg = body.message;
    if (typeof msg === 'string' && msg) return msg;
  }
  return `${fallback} (HTTP ${result.status})`;
}

function requireOk<T>(result: ApiResult<unknown>, fallbackMessage: string): T {
  if (!result.ok) {
    throw new Error(readErrorMessage(result, fallbackMessage));
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

  return {
    listEntities,
    getEntity,
    saveEntity,
    listModels,
    listToolCategories,
    testConnection,
    listOntologyObjects,
    listOntologyActions,
  };
}
