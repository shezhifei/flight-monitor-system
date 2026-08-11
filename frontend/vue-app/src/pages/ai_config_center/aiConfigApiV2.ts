/**
 * AI 配置 v2 API 扩展
 *
 * 新增 MCP server、Agent Skill、缓存指标、能力验证等端点。
 */

import { useApi } from '@/composables/useApi';
import type {
  AiEntityConfigV2,
  McpServerDefinition,
  McpServerCapabilities,
  McpEntityBinding,
  SkillRegistryEntry,
  SkillEntityBinding,
  EnrichedCapabilitySnapshot,
  ValidationResult,
  CacheMetricsSummary,
} from './aiConfigTypesV2';

// === 错误处理 ===

function unwrapEnvelope<T>(result: { data?: unknown }): T {
  // result is ApiResult<T>: { ok, status, data }
  // Backend returns { success: true, data: payload } in data field
  const body = result.data;
  if (body && typeof body === 'object' && 'data' in body) {
    return (body as { data: T }).data;
  }
  return body as T;
}

function readErrorMessage(err: unknown): string {
  if (!err) return 'Unknown error';
  if (typeof err === 'string') return err;
  const e = err as { message?: string; error?: { message?: string } | string };
  if (e.message) return e.message;
  if (e.error) {
    if (typeof e.error === 'string') return e.error;
    if (e.error.message) return e.error.message;
  }
  return JSON.stringify(err);
}

function requireOk<T>(result: { ok?: boolean; data?: unknown }): T {
  if (result && result.ok === false) {
    throw new Error(readErrorMessage(result.data || result));
  }
  const body = result.data;
  if (body && typeof body === 'object' && 'success' in body && (body as { success: unknown }).success === false) {
    throw new Error(readErrorMessage(body));
  }
  return unwrapEnvelope<T>(result);
}

// === API 函数 ===

export function useAiConfigApiV2() {
  const api = useApi();

  // === Entity Config V2 ===

  async function getEntityConfigV2(entityId: string): Promise<AiEntityConfigV2> {
    const result = await api.get<AiEntityConfigV2>(`/api/v2/ai/entities/${entityId}`);
    return requireOk<AiEntityConfigV2>(result);
  }

  async function saveEntityConfigV2(
    entityId: string,
    config: Partial<AiEntityConfigV2>
  ): Promise<AiEntityConfigV2> {
    const result = await api.put(`/api/v2/ai/entities/${entityId}`, config);
    return requireOk(result);
  }

  // === Capabilities ===

  async function getEntityCapabilities(
    entityId: string
  ): Promise<EnrichedCapabilitySnapshot> {
    const result = await api.get<EnrichedCapabilitySnapshot>(
      `/api/v2/ai/entities/${entityId}/capabilities`
    );
    return requireOk<EnrichedCapabilitySnapshot>(result);
  }

  async function validateEntityCapabilities(
    entityId: string
  ): Promise<ValidationResult> {
    const result = await api.post(
      `/api/v2/ai/entities/${entityId}/capabilities/validate`
    );
    return requireOk(result);
  }

  // === MCP Servers ===

  async function listMcpServers(
    entityId: string
  ): Promise<McpServerDefinition[]> {
    const result = await api.get<McpServerDefinition[]>(
      `/api/v2/ai/entities/${entityId}/mcp/servers`
    );
    return requireOk<McpServerDefinition[]>(result);
  }

  async function probeMcpServer(
    entityId: string,
    serverId: string
  ): Promise<{ status: string; capabilities?: McpServerCapabilities }> {
    const result = await api.post(
      `/api/v2/ai/entities/${entityId}/mcp/servers/${serverId}/probe`
    );
    return requireOk(result);
  }

  async function createMcpServer(
    entityId: string,
    server: Partial<McpServerDefinition>
  ): Promise<McpServerDefinition> {
    const result = await api.post(
      `/api/v2/ai/entities/${entityId}/mcp/servers`,
      server
    );
    return requireOk<McpServerDefinition>(result);
  }

  async function updateMcpServer(
    entityId: string,
    serverId: string,
    server: Partial<McpServerDefinition>
  ): Promise<McpServerDefinition> {
    const result = await api.put(
      `/api/v2/ai/entities/${entityId}/mcp/servers/${serverId}`,
      server
    );
    return requireOk<McpServerDefinition>(result);
  }

  async function deleteMcpServer(
    entityId: string,
    serverId: string
  ): Promise<boolean> {
    const result = await api.delete<boolean>(
      `/api/v2/ai/entities/${entityId}/mcp/servers/${serverId}`
    );
    return requireOk<boolean>(result);
  }

  // === MCP Bindings ===

  async function listMcpBindings(
    entityId: string
  ): Promise<McpEntityBinding[]> {
    const result = await api.get<McpEntityBinding[]>(
      `/api/v2/ai/entities/${entityId}/mcp/bindings`
    );
    return requireOk<McpEntityBinding[]>(result);
  }

  async function saveMcpBinding(
    entityId: string,
    binding: Partial<McpEntityBinding>
  ): Promise<McpEntityBinding> {
    const result = await api.post(
      `/api/v2/ai/entities/${entityId}/mcp/bindings`,
      binding
    );
    return requireOk<McpEntityBinding>(result);
  }

  // === Agent Skills ===

  async function listSkillRegistry(): Promise<SkillRegistryEntry[]> {
    const result = await api.get<SkillRegistryEntry[]>('/api/v2/ai/skills');
    return requireOk<SkillRegistryEntry[]>(result);
  }

  async function listEntitySkills(
    entityId: string
  ): Promise<SkillEntityBinding[]> {
    const result = await api.get<SkillEntityBinding[]>(
      `/api/v2/ai/entities/${entityId}/skills`
    );
    return requireOk<SkillEntityBinding[]>(result);
  }

  async function probeSkill(
    entityId: string,
    skillSlug: string
  ): Promise<{ status: string; content_hash?: string }> {
    const result = await api.post(
      `/api/v2/ai/entities/${entityId}/skills/${skillSlug}/probe`
    );
    return requireOk(result);
  }

  async function saveSkillBinding(
    entityId: string,
    binding: Partial<SkillEntityBinding>
  ): Promise<SkillEntityBinding> {
    const result = await api.post(
      `/api/v2/ai/entities/${entityId}/skills/bindings`,
      binding
    );
    return requireOk<SkillEntityBinding>(result);
  }

  async function deleteSkillBinding(
    entityId: string,
    bindingId: string
  ): Promise<boolean> {
    const result = await api.delete<boolean>(
      `/api/v2/ai/entities/${entityId}/skills/bindings/${bindingId}`
    );
    return requireOk<boolean>(result);
  }

  // === Cache Metrics ===

  async function getCacheMetrics(
    entityId?: string,
    hours: number = 24
  ): Promise<CacheMetricsSummary> {
    const params = new URLSearchParams();
    if (entityId) params.set('entity_id', entityId);
    params.set('hours', hours.toString());

    const result = await api.get<CacheMetricsSummary>(
      `/api/v2/ai/cache/metrics?${params.toString()}`
    );
    return requireOk<CacheMetricsSummary>(result);
  }

  async function invalidateCache(
    entityId: string,
    cacheType?: string
  ): Promise<{ invalidated: number }> {
    const result = await api.post('/api/v2/ai/cache/invalidate', {
      entity_id: entityId,
      cache_type: cacheType,
    });
    return requireOk(result);
  }

  // === Connection Test with Capability Probe ===

  async function testConnectionWithCapabilities(
    entityId: string
  ): Promise<{
    connected: boolean;
    models: string[];
    capabilities: Record<string, unknown>;
  }> {
    const result = await api.post('/api/v2/ai/connection/test', {
      entity_id: entityId,
      include_models: true,
      include_capabilities: true,
    });
    return requireOk(result);
  }

  return {
    // Entity Config
    getEntityConfigV2,
    saveEntityConfigV2,
    // Capabilities
    getEntityCapabilities,
    validateEntityCapabilities,
    // MCP
    listMcpServers,
    probeMcpServer,
    createMcpServer,
    updateMcpServer,
    deleteMcpServer,
    listMcpBindings,
    saveMcpBinding,
    // Skills
    listSkillRegistry,
    listEntitySkills,
    probeSkill,
    saveSkillBinding,
    deleteSkillBinding,
    // Cache
    getCacheMetrics,
    invalidateCache,
    // Connection
    testConnectionWithCapabilities,
  };
}
