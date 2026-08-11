import { ref, computed, onMounted, onBeforeUnmount, watch } from 'vue';
import { useAuth } from '@/composables/useAuth';
import { useToast } from '@/composables/useToast';
import {
  useAiConfigApi,
  type AiEntityDetail,
  type AiEntitySavePayload,
  type AiEntitySummary,
  type AiModelOption,
  type AiOntologyActionRouteRow,
  type AiOntologyObjectDef,
  type AiToolCategory,
} from '../aiConfigApi';
import {
  useRealtimeAudioSession,
  type RealtimeServerEvent,
} from '@/composables/useRealtimeAudioSession';
import { useAiConfigApiV2 } from '../aiConfigApiV2';
import type {
  EnrichedCapabilitySnapshot,
  ValidationResult,
  McpServerDefinition,
  McpEntityBinding,
  SkillRegistryEntry,
  SkillEntityBinding,
  CacheMetricsSummary,
} from '../aiConfigTypesV2';
import { summarizeValidation } from '../aiCapabilityViewModel';

export interface OntologyObject {
  id: string;
  name: string;
  plural_name: string;
  description: string;
  properties: Property[];
  relationships: Relationship[];
  actions: string[];
  tags: string[];
  is_active: boolean;
}

export interface OntologyAction {
  id: string;
  name: string;
  object_type: string;
  description: string;
  category: string;
  parameters: Property[];
  requires_approval: boolean;
  risk_level: string;
  constraint_rules: unknown[];
  is_active: boolean;
}

export interface Property {
  name: string;
  type: string;
  required: boolean;
  description: string;
  enum_values?: string[];
  reference_object?: string;
  default?: unknown;
}

export interface Relationship {
  name: string;
  target_object: string;
  cardinality: string;
  description: string;
}

export interface NormalizedModelOption {
  value: string;
  label: string;
  provider?: string;
  source: 'catalog' | 'remote' | 'custom';
}

export interface ProviderFormEntry {
  key: string;
  type: string;
  base_url: string;
  api_key: string;
  api_format: string;
  timeout: number;
  max_retries: number;
  retry_delay: number;
}

export interface ModelsTabForm {
  config_version: number;
  base_url: string;
  api_key: string;
  providers: ProviderFormEntry[];
  model_provider_ref: string;
  default_model: string;
  chat_model: string;
  summary_model: string;
  vision_model: string;
  embedding_model: string;
  asr_model: string;
  tts_model: string;
  tts_voice: string;
  api_format: string;
  timeout: number;
  max_retries: number;
  retry_delay: number;
  context_window: number;
  max_output_tokens: number;
  model_input_modalities: string[];
  model_output_modalities: string[];
  model_tool_calling: boolean;
  model_parallel_tool_calls: boolean;
  model_streaming: boolean;
  model_structured_output: boolean;
  model_prompt_cache: boolean;
  system_prompt: string;
  task_template: string;
  denied_tools: string[];
  allowed_tool_sources: string[];
  allowed_tool_categories: string[];
  tooling_enabled: boolean;
  tooling_allow_parallel: boolean;
  tooling_max_rounds: number;
  write_action_policy: string;
  mcp_enabled: boolean;
  mcp_binding_ids: string;
  mcp_tool_name_prefix: string;
  mcp_discovery_cache_ttl_seconds: number;
  mcp_fail_closed: boolean;
  skills_enabled: boolean;
  skills_allowlist: string;
  skills_bindings: string;
  subagents_enabled: boolean;
  subagents_allowed_entity_ids: string;
  subagents_max_depth: number;
  subagents_max_concurrency: number;
  subagents_inherit_parent_context: boolean;
  subagents_require_tool_calling_capability: boolean;
  subagents_handoff_prompt: string;
  context_strategy: string;
  max_context_tokens: number;
  compression_threshold_tokens: number;
  preserve_recent_messages: number;
  summary_max_tokens: number;
  persist_summaries: boolean;
  cache_enabled: boolean;
  provider_prompt_cache_enabled: boolean;
  provider_prompt_cache_retention: string;
  context_cache_backend: string;
  context_cache_ttl_seconds: number;
  tool_result_cache_enabled: boolean;
  tool_result_cache_ttl_seconds: number;
  cacheable_tools: string;
  mcp_resource_cache_enabled: boolean;
  mcp_resource_cache_ttl_seconds: number;
  max_input_bytes: number;
  allowed_input_mime_types: string;
}

export interface AudioLogEntry {
  id: number;
  direction: 'in' | 'out';
  type: string;
  detail: string;
  ts: number;
}

export function useAiConfigCenter() {
  const auth = useAuth();
  const toast = useToast();
  const aiConfigApi = useAiConfigApi();
  const aiConfigV2 = useAiConfigApiV2();

  const capabilitySnapshot = ref<EnrichedCapabilitySnapshot | null>(null);
  const capabilityLoading = ref(false);
  const capabilityValidation = ref<ValidationResult | null>(null);
  const mcpServers = ref<McpServerDefinition[]>([]);
  const mcpBindings = ref<McpEntityBinding[]>([]);
  const mcpLoading = ref(false);
  const skillRegistry = ref<SkillRegistryEntry[]>([]);
  const skillBindings = ref<SkillEntityBinding[]>([]);
  const skillsLoading = ref(false);
  const cacheMetrics = ref<CacheMetricsSummary | null>(null);
  const cacheLoading = ref(false);
  const sidebarUser = computed(() => {
    const user = auth.getUser();
    const name = user?.display_name || user?.username || '当前用户';
    const role = user?.role || (user?.is_admin ? 'Administrator' : 'User');
    const avatar = name.trim().charAt(0).toUpperCase() || 'U';
    return { name, role, avatar };
  });
  function handleLogout() { auth.logout(); }
  const activeTab = ref<'objects' | 'actions' | 'models'>('objects');
  const searchQuery = ref('');
  const loading = ref(false);
  const objects = ref<OntologyObject[]>([]);
  const actions = ref<OntologyAction[]>([]);

  const filteredObjects = computed(() => {
    if (!searchQuery.value) return objects.value;
    const q = searchQuery.value.toLowerCase();
    return objects.value.filter(o =>
      o.name.toLowerCase().includes(q) ||
      o.description.toLowerCase().includes(q) ||
      o.tags.some(t => t.toLowerCase().includes(q))
    );
  });

  const filteredActions = computed(() => {
    if (!searchQuery.value) return actions.value;
    const q = searchQuery.value.toLowerCase();
    return actions.value.filter(a =>
      a.name.toLowerCase().includes(q) ||
      a.object_type.toLowerCase().includes(q) ||
      a.description.toLowerCase().includes(q)
    );
  });

  async function fetchData() {
    loading.value = true;
    try {
      const [objectRows, actionRows] = await Promise.all([
        aiConfigApi.listOntologyObjects(),
        aiConfigApi.listOntologyActions(),
      ]);
      objects.value = objectRows.map(normalizeOntologyObject);
      actions.value = actionRows.map(normalizeOntologyAction);
    } catch (err) {
      console.error('Failed to fetch ontology data:', err);
      toast.show('error', err instanceof Error ? err.message : '加载 Ontology 数据失败');
      objects.value = [];
      actions.value = [];
    } finally {
      loading.value = false;
    }
  }

  function getRiskBadgeClass(level: string): string {
    const map: Record<string, string> = {
      LOW: 'badge-low',
      NORMAL: 'badge-normal',
      MEDIUM: 'badge-medium',
      HIGH: 'badge-high',
      CRITICAL: 'badge-critical',
    };
    return map[level] || 'badge-normal';
  }

  function normalizeField(name: string, raw: Record<string, unknown>): Property {
    return {
      name: String(raw.name || name),
      type: String(raw.field_type || raw.type || raw.param_type || 'string'),
      required: Boolean(raw.required),
      description: String(raw.description || ''),
    };
  }

  function readActiveFlag(...sources: Array<Record<string, unknown> | undefined>): boolean {
    for (const source of sources) {
      if (!source) continue;
      if (typeof source.is_active === 'boolean') return source.is_active;
      if (typeof source.enabled === 'boolean') return source.enabled;
      if (typeof source.disabled === 'boolean') return !source.disabled;
    }
    return true;
  }

  function normalizeOntologyObject(raw: AiOntologyObjectDef): OntologyObject {
    const fields = raw.fields && typeof raw.fields === 'object' ? raw.fields : {};
    const relations = raw.relations && typeof raw.relations === 'object' ? raw.relations : {};
    const actionMap = raw.actions && typeof raw.actions === 'object' ? raw.actions : {};
    return {
      id: String(raw.name || ''),
      name: String(raw.name || ''),
      plural_name: String(raw.object_id_strategy || ''),
      description: String(raw.description || ''),
      properties: Object.entries(fields).map(([name, field]) => normalizeField(name, field as Record<string, unknown>)),
      relationships: Object.entries(relations).map(([name, relation]) => ({
        name,
        target_object: String((relation as Record<string, unknown>).target_object || ''),
        cardinality: String((relation as Record<string, unknown>).relation_type || (relation as Record<string, unknown>).cardinality || ''),
        description: String((relation as Record<string, unknown>).description || ''),
      })),
      actions: Object.keys(actionMap),
      tags: raw.object_id_strategy ? [String(raw.object_id_strategy)] : [],
      is_active: readActiveFlag(raw as Record<string, unknown>),
    };
  }

  function normalizeOntologyAction(raw: AiOntologyActionRouteRow): OntologyAction {
    const definition = raw.definition || {};
    const objectType = String(raw.object || '');
    const actionName = String(raw.action || definition.name || '');
    const parameters = definition.parameters && typeof definition.parameters === 'object'
      ? definition.parameters
      : {};
    const approval = String(definition.approval_policy || definition.approval_strategy || '').toLowerCase();
    return {
      id: `${objectType}.${actionName}`,
      name: actionName,
      object_type: objectType,
      description: String(definition.description || ''),
      category: String(definition.category || ''),
      parameters: Object.entries(parameters).map(([name, field]) => normalizeField(name, field as Record<string, unknown>)),
      requires_approval: Boolean(approval && approval !== 'none' && approval !== 'never'),
      risk_level: String(definition.risk_level || 'NORMAL'),
      constraint_rules: Array.isArray(definition.constraints) ? definition.constraints : [],
      is_active: readActiveFlag(raw as Record<string, unknown>, definition as Record<string, unknown>),
    };
  }

  function createDefaultProviderEntry(key: string): ProviderFormEntry {
    return {
      key,
      type: 'openai_compatible',
      base_url: '',
      api_key: '',
      api_format: 'chat_completions',
      timeout: 30,
      max_retries: 3,
      retry_delay: 0.5,
    };
  }

  function createEmptyModelsForm(): ModelsTabForm {
    return {
      config_version: 2,
      base_url: '',
      api_key: '',
      providers: [createDefaultProviderEntry('default')],
      model_provider_ref: 'default',
      default_model: '',
      chat_model: '',
      summary_model: '',
      vision_model: '',
      embedding_model: '',
      asr_model: '',
      tts_model: '',
      tts_voice: '',
      api_format: 'chat_completions',
      timeout: 30,
      max_retries: 3,
      retry_delay: 0.5,
      context_window: 128000,
      max_output_tokens: 2000,
      model_input_modalities: ['text'],
      model_output_modalities: ['text'],
      model_tool_calling: true,
      model_parallel_tool_calls: false,
      model_streaming: true,
      model_structured_output: false,
      model_prompt_cache: false,
      system_prompt: '',
      task_template: '',
      denied_tools: [],
      allowed_tool_sources: ['builtin'],
      allowed_tool_categories: [],
      tooling_enabled: true,
      tooling_allow_parallel: false,
      tooling_max_rounds: 5,
      write_action_policy: 'proposal_only',
      mcp_enabled: false,
      mcp_binding_ids: '',
      mcp_tool_name_prefix: 'mcp',
      mcp_discovery_cache_ttl_seconds: 300,
      mcp_fail_closed: false,
      skills_enabled: false,
      skills_allowlist: '',
      skills_bindings: '',
      subagents_enabled: false,
      subagents_allowed_entity_ids: '',
      subagents_max_depth: 1,
      subagents_max_concurrency: 2,
      subagents_inherit_parent_context: true,
      subagents_require_tool_calling_capability: true,
      subagents_handoff_prompt: '',
      context_strategy: 'hybrid',
      max_context_tokens: 64000,
      compression_threshold_tokens: 48000,
      preserve_recent_messages: 12,
      summary_max_tokens: 1200,
      persist_summaries: true,
      cache_enabled: true,
      provider_prompt_cache_enabled: false,
      provider_prompt_cache_retention: '24h',
      context_cache_backend: 'redis',
      context_cache_ttl_seconds: 86400,
      tool_result_cache_enabled: true,
      tool_result_cache_ttl_seconds: 60,
      cacheable_tools: '',
      mcp_resource_cache_enabled: true,
      mcp_resource_cache_ttl_seconds: 300,
      max_input_bytes: 26214400,
      allowed_input_mime_types: 'text/plain, image/png, image/jpeg, audio/wav',
    };
  }

  const entities = ref<AiEntitySummary[]>([]);
  const selectedEntityId = ref<string>('');
  const entityDetail = ref<AiEntityDetail | null>(null);
  const modelOptions = ref<NormalizedModelOption[]>([]);
  const toolCategories = ref<AiToolCategory[]>([]);
  const modelsForm = ref<ModelsTabForm>(createEmptyModelsForm());
  const modelsLoading = ref(false);
  const modelsSaving = ref(false);
  const modelsTesting = ref(false);
  const modelsBootstrapped = ref(false);
  const entitySearch = ref('');
  const savedModelsFormJson = ref(JSON.stringify(createEmptyModelsForm()));
  const isModelsDirty = computed(() => JSON.stringify(modelsForm.value) !== savedModelsFormJson.value);

  const modalityOptions = [
    { value: 'text', label: 'Text' },
    { value: 'image', label: 'Image' },
    { value: 'audio', label: 'Audio' },
    { value: 'file', label: 'File' },
  ];

  const toolSourceOptions = [
    { value: 'builtin', label: 'Builtin' },
    { value: 'mcp', label: 'MCP' },
    { value: 'skill', label: 'Skill Tool' },
  ];

  const existingCapabilityRows = [
    { name: 'AI 实体配置', current: 'ai_entities.config JSONB 持久化；支持 base_url、api_key、default_model、api_format、Prompt、工具黑名单。' },
    { name: '模型/Provider', current: '支持 OpenAI-compatible Base URL、Chat Completions 与 Responses API、ASR/TTS 模型字段。' },
    { name: '工具调用', current: '已有 builtin 工具目录；读工具本地执行，写动作走审批提案；可按 denied_tools 禁用。' },
    { name: 'Prompt Cache', current: '后端已有 prompt_cache key 与 provider cache 参数透传能力，但实体页缺少策略开关。' },
    { name: '上下文管理', current: '已有 token 计数、裁剪和基础压缩字段；缺少实体级压缩策略。' },
    { name: '运行审计', current: 'ai_jobs / ai_runs / ai_run_events 可记录 token 与运行事件。' },
    { name: '实时音频', current: '配置页已有 ASR/TTS 字段和实时音频测试面板。' },
  ];

  const entityPolicyRows = [
    '模型用途路由、默认模型能力、多模态输入/输出门控',
    '是否允许工具调用、工具来源、工具类别与写动作 proposal-only 策略',
    '是否启用 Subagents、可委派实体、并发/深度与上下文继承边界',
    'MCP 绑定引用与每实体开关；MCP server 注册、command_ref、probe 结果不内联在实体中',
    'Agent Skill 绑定引用、加载顺序和上下文预算；SKILL.md 索引与 hash 不内联在实体中',
    '上下文压缩阈值、摘要模型、摘要保留策略',
    'Provider Prompt Cache、上下文缓存、工具结果缓存、MCP resource cache 策略',
    '多模态输入大小、MIME allowlist、敏感信息日志策略',
  ];

  function asRecord(value: unknown): Record<string, unknown> {
    return value && typeof value === 'object' && !Array.isArray(value)
      ? value as Record<string, unknown>
      : {};
  }

  function readString(value: unknown, fallback = ''): string {
    return typeof value === 'string' ? value : fallback;
  }

  function readNumber(value: unknown, fallback: number): number {
    const next = Number(value);
    return Number.isFinite(next) ? next : fallback;
  }

  function readBoolean(value: unknown, fallback: boolean): boolean {
    return typeof value === 'boolean' ? value : fallback;
  }

  function readStringArray(value: unknown, fallback: string[] = []): string[] {
    if (!Array.isArray(value)) return fallback;
    return value
      .map((item) => String(item || '').trim())
      .filter(Boolean);
  }

  function splitTextList(value: string): string[] {
    return value
      .split(/[\n,，]/)
      .map((item) => item.trim())
      .filter(Boolean);
  }

  function joinTextList(value: unknown): string {
    return readStringArray(value).join(', ');
  }

  function normalizeProviderEntries(providers: Record<string, unknown>): ProviderFormEntry[] {
    const result: ProviderFormEntry[] = [];
    for (const [key, raw] of Object.entries(providers)) {
      if (key === 'default') continue;
      const cfg = asRecord(raw);
      result.push({
        key,
        type: readString(cfg.type, 'openai_compatible'),
        base_url: readString(cfg.base_url, ''),
        api_key: readString(cfg.api_key, ''),
        api_format: readString(cfg.api_format, 'chat_completions'),
        timeout: readNumber(cfg.timeout, 30),
        max_retries: readNumber(cfg.max_retries, 3),
        retry_delay: readNumber(cfg.retry_delay, 0.5),
      });
    }
    return result;
  }

  function buildProvidersPayload(form: ModelsTabForm): Record<string, unknown> {
    const dict: Record<string, unknown> = {
      default: {
        type: 'openai_compatible',
        base_url: form.base_url,
        api_key: form.api_key,
        api_format: form.api_format,
        timeout: form.timeout,
        max_retries: form.max_retries,
        retry_delay: form.retry_delay,
      },
    };
    for (const entry of form.providers) {
      const key = entry.key.trim();
      if (!key || key === 'default' || key in dict) continue;
      dict[key] = {
        type: entry.type || 'openai_compatible',
        base_url: entry.base_url,
        api_key: entry.api_key,
        api_format: entry.api_format,
        timeout: entry.timeout,
        max_retries: entry.max_retries,
        retry_delay: entry.retry_delay,
      };
    }
    return dict;
  }

  function toggleArrayItem(target: string[], value: string, enabled: boolean): string[] {
    const next = new Set(target);
    if (enabled) {
      next.add(value);
    } else {
      next.delete(value);
    }
    return Array.from(next).sort();
  }

  const providerRefOptions = computed<string[]>(() => {
    const keys = ['default'];
    for (const p of modelsForm.value.providers) {
      const key = p.key.trim();
      if (key && key !== 'default' && !keys.includes(key)) keys.push(key);
    }
    return keys;
  });

  function addProviderEntry(): void {
    const existing = new Set(modelsForm.value.providers.map((p) => p.key));
    existing.add('default');
    const candidates = ['asr', 'tts', 'embedding', 'vision'];
    let key = candidates.find((c) => !existing.has(c));
    if (!key) {
      let i = 1;
      while (existing.has(`extra${i}`)) i += 1;
      key = `extra${i}`;
    }
    modelsForm.value.providers.push(createDefaultProviderEntry(key));
  }

  function removeProviderEntry(index: number): void {
    const removed = modelsForm.value.providers[index];
    modelsForm.value.providers.splice(index, 1);
    if (removed && modelsForm.value.model_provider_ref === removed.key) {
      modelsForm.value.model_provider_ref = 'default';
    }
  }

  function toggleModelInputModality(value: string, enabled: boolean): void {
    const next = toggleArrayItem(modelsForm.value.model_input_modalities, value, enabled);
    modelsForm.value.model_input_modalities = next.length > 0 ? next : ['text'];
  }

  function toggleModelOutputModality(value: string, enabled: boolean): void {
    const next = toggleArrayItem(modelsForm.value.model_output_modalities, value, enabled);
    modelsForm.value.model_output_modalities = next.length > 0 ? next : ['text'];
  }

  function toggleToolSource(value: string, enabled: boolean): void {
    const next = toggleArrayItem(modelsForm.value.allowed_tool_sources, value, enabled);
    modelsForm.value.allowed_tool_sources = next.length > 0 ? next : ['builtin'];
  }

  function toggleAllowedToolCategory(category: string, enabled: boolean): void {
    modelsForm.value.allowed_tool_categories = toggleArrayItem(
      modelsForm.value.allowed_tool_categories,
      category,
      enabled,
    );
  }

  function normalizeModelOption(input: AiModelOption, source: NormalizedModelOption['source']): NormalizedModelOption | null {
    const rawId = String(input.id || input.value || '').trim();
    const rawName = String(input.name || '').trim();
    const value = rawId || rawName;
    if (!value) {
      return null;
    }
    const provider = String(input.provider || input.owned_by || '').trim();
    const label = rawName || value;
    return {
      value,
      label: provider ? `${label} (${provider})` : label,
      provider: provider || undefined,
      source,
    };
  }

  function mergeModelOptions(current: NormalizedModelOption[], incoming: AiModelOption[], source: NormalizedModelOption['source']): NormalizedModelOption[] {
    const merged = new Map<string, NormalizedModelOption>();
    current.forEach((item) => merged.set(item.value, item));
    incoming.forEach((item) => {
      const normalized = normalizeModelOption(item, source);
      if (!normalized) return;
      const existing = merged.get(normalized.value);
      merged.set(
        normalized.value,
        existing ? { ...existing, ...normalized, source } : normalized,
      );
    });
    return Array.from(merged.values()).sort((a, b) => a.value.localeCompare(b.value));
  }

  function ensureCustomModelOption(current: NormalizedModelOption[], rawValue: unknown): NormalizedModelOption[] {
    const value = String(rawValue || '').trim();
    if (!value || current.some((item) => item.value === value)) {
      return current;
    }
    return [
      { value, label: `${value} (自定义)`, source: 'custom' },
      ...current,
    ];
  }

  const filteredEntities = computed(() => {
    const q = entitySearch.value.trim().toLowerCase();
    if (!q) return entities.value;
    return entities.value.filter((e) => e.id.toLowerCase().includes(q));
  });

  const categoryToolMap = computed<Record<string, string[]>>(() => {
    const map: Record<string, string[]> = {};
    toolCategories.value.forEach((cat) => {
      const name = String(cat.name || 'uncategorized');
      map[name] = Array.isArray(cat.tools) ? (cat.tools as string[]) : [];
    });
    return map;
  });

  const deniedToolsSet = computed(() => new Set(modelsForm.value.denied_tools));
  const allowedToolCategoriesSet = computed(() => new Set(modelsForm.value.allowed_tool_categories));

  function toggleDeniedTool(toolName: string, allowed: boolean) {
    const current = new Set(modelsForm.value.denied_tools);
    if (allowed) {
      current.delete(toolName);
    } else {
      current.add(toolName);
    }
    modelsForm.value.denied_tools = Array.from(current).sort();
  }

  function serializeSkillBindings(value: unknown): string {
    if (Array.isArray(value) && value.length > 0) {
      return JSON.stringify(value, null, 2);
    }
    return '';
  }

  function parseSkillBindings(value: string): unknown[] {
    const trimmed = value.trim();
    if (!trimmed) return [];
    if (trimmed.startsWith('[')) {
      const parsed = JSON.parse(trimmed);
      if (!Array.isArray(parsed)) {
        throw new Error('Agent Skill bindings 必须是 JSON 数组');
      }
      return parsed;
    }
    return splitTextList(trimmed).map((skillSlug) => ({
      skill_slug: skillSlug,
      enabled: true,
      activation_policy: 'task_routed',
    }));
  }

  function applyEntityDetailToForm(detail: AiEntityDetail) {
    const modelRouting = asRecord(detail.model_routing);
    const modelMap = asRecord(detail.models);
    const defaultModelId = String(detail.default_model || modelRouting.default || '');
    const defaultModelConfig = asRecord(modelMap[defaultModelId]);
    const modelModalities = asRecord(defaultModelConfig.modalities);
    const modelCapabilities = asRecord(defaultModelConfig.capabilities);
    const tooling = asRecord(detail.tooling);
    const mcp = asRecord(detail.mcp);
    const skills = asRecord(detail.skills);
    const subagents = asRecord(detail.subagents);
    const contextPolicy = asRecord(detail.context_policy);
    const cachePolicy = asRecord(detail.cache_policy);
    const providerPromptCache = asRecord(cachePolicy.provider_prompt_cache);
    const contextCache = asRecord(cachePolicy.context_cache);
    const toolResultCache = asRecord(cachePolicy.tool_result_cache);
    const mcpResourceCache = asRecord(cachePolicy.mcp_resource_cache);
    const security = asRecord(detail.security);
    const providerEntries = normalizeProviderEntries(asRecord(detail.providers));
    const defaultCategories = Object.keys(categoryToolMap.value);
    const configuredCategories = readStringArray(
      tooling.allowed_tool_categories ?? detail.allowed_tool_categories,
      defaultCategories,
    );
    const nextForm = {
      config_version: readNumber(detail.config_version, 2),
      base_url: String(detail.base_url || ''),
      api_key: String(detail.api_key || ''),
      providers: providerEntries,
      model_provider_ref: readString(defaultModelConfig.provider_ref, 'default') || 'default',
      default_model: defaultModelId,
      chat_model: readString(modelRouting.chat, defaultModelId),
      summary_model: readString(modelRouting.summary ?? contextPolicy.summary_model, ''),
      vision_model: readString(modelRouting.vision, ''),
      embedding_model: readString(modelRouting.embedding, ''),
      asr_model: String(detail.asr_model || ''),
      tts_model: String(detail.tts_model || ''),
      tts_voice: String(detail.tts_voice || ''),
      api_format: String(detail.api_format || 'chat_completions'),
      timeout: Number(detail.timeout ?? 30),
      max_retries: Number(detail.max_retries ?? 3),
      retry_delay: Number(detail.retry_delay ?? 0.5),
      context_window: readNumber(defaultModelConfig.context_window ?? detail.context_window, 128000),
      max_output_tokens: readNumber(defaultModelConfig.max_output_tokens ?? detail.max_tokens, 2000),
      model_input_modalities: readStringArray(modelModalities.input, ['text']),
      model_output_modalities: readStringArray(modelModalities.output, ['text']),
      model_tool_calling: readBoolean(modelCapabilities.tool_calling, true),
      model_parallel_tool_calls: readBoolean(modelCapabilities.parallel_tool_calls, false),
      model_streaming: readBoolean(modelCapabilities.streaming, true),
      model_structured_output: readBoolean(modelCapabilities.structured_output, false),
      model_prompt_cache: readBoolean(modelCapabilities.prompt_cache, false),
      system_prompt: String(detail.system_prompt || ''),
      task_template: String(detail.task_template || ''),
      denied_tools: Array.isArray(detail.denied_tools)
        ? (detail.denied_tools as string[]).slice()
        : [],
      allowed_tool_sources: readStringArray(tooling.allowed_tool_sources, ['builtin']),
      allowed_tool_categories: configuredCategories.length > 0 ? configuredCategories : defaultCategories,
      tooling_enabled: readBoolean(tooling.enabled, true),
      tooling_allow_parallel: readBoolean(tooling.allow_parallel, false),
      tooling_max_rounds: readNumber(tooling.max_rounds, 5),
      write_action_policy: readString(tooling.write_action_policy, 'proposal_only'),
      mcp_enabled: readBoolean(mcp.enabled, false),
      mcp_binding_ids: joinTextList(mcp.binding_ids),
      mcp_tool_name_prefix: readString(mcp.tool_name_prefix, 'mcp'),
      mcp_discovery_cache_ttl_seconds: readNumber(mcp.discovery_cache_ttl_seconds, 300),
      mcp_fail_closed: readBoolean(mcp.fail_closed, false),
      skills_enabled: readBoolean(skills.enabled, false),
      skills_allowlist: joinTextList(skills.allowlist),
      skills_bindings: serializeSkillBindings(skills.bindings),
      subagents_enabled: readBoolean(subagents.enabled, false),
      subagents_allowed_entity_ids: joinTextList(subagents.allowed_entity_ids),
      subagents_max_depth: readNumber(subagents.max_depth, 1),
      subagents_max_concurrency: readNumber(subagents.max_concurrency, 2),
      subagents_inherit_parent_context: readBoolean(subagents.inherit_parent_context, true),
      subagents_require_tool_calling_capability: readBoolean(subagents.require_tool_calling_capability, true),
      subagents_handoff_prompt: readString(subagents.handoff_prompt, ''),
      context_strategy: readString(contextPolicy.strategy, 'hybrid'),
      max_context_tokens: readNumber(contextPolicy.max_context_tokens, 64000),
      compression_threshold_tokens: readNumber(contextPolicy.compression_threshold_tokens, 48000),
      preserve_recent_messages: readNumber(contextPolicy.preserve_recent_messages, 12),
      summary_max_tokens: readNumber(contextPolicy.summary_max_tokens, 1200),
      persist_summaries: readBoolean(contextPolicy.persist_summaries, true),
      cache_enabled: readBoolean(cachePolicy.enabled, true),
      provider_prompt_cache_enabled: readBoolean(providerPromptCache.enabled, false),
      provider_prompt_cache_retention: readString(providerPromptCache.retention, '24h'),
      context_cache_backend: readString(contextCache.backend, 'redis'),
      context_cache_ttl_seconds: readNumber(contextCache.ttl_seconds, 86400),
      tool_result_cache_enabled: readBoolean(toolResultCache.enabled, true),
      tool_result_cache_ttl_seconds: readNumber(toolResultCache.ttl_seconds, 60),
      cacheable_tools: joinTextList(toolResultCache.cacheable_tools),
      mcp_resource_cache_enabled: readBoolean(mcpResourceCache.enabled, true),
      mcp_resource_cache_ttl_seconds: readNumber(mcpResourceCache.ttl_seconds, 300),
      max_input_bytes: readNumber(security.max_input_bytes, 26214400),
      allowed_input_mime_types: joinTextList(security.allowed_input_mime_types) || 'text/plain, image/png, image/jpeg, audio/wav',
    };
    modelsForm.value = nextForm;
    savedModelsFormJson.value = JSON.stringify(nextForm);
  }

  function confirmDiscardModelChanges(): boolean {
    if (!isModelsDirty.value) {
      return true;
    }
    return window.confirm('当前模型配置有未保存修改，确定放弃这些修改吗？');
  }

  async function loadEntityList(): Promise<void> {
    try {
      const list = await aiConfigApi.listEntities();
      entities.value = list;
      if (!selectedEntityId.value && list.length > 0) {
        selectedEntityId.value = list[0].id;
      }
    } catch (err) {
      toast.show('error', err instanceof Error ? err.message : '加载实体列表失败');
    }
  }

  async function loadModelsCatalog(): Promise<void> {
    try {
      const models = await aiConfigApi.listModels();
      modelOptions.value = mergeModelOptions([], models, 'catalog');
    } catch (err) {
      toast.show('error', err instanceof Error ? err.message : '加载模型列表失败');
    }
  }

  async function loadToolCategories(): Promise<void> {
    try {
      toolCategories.value = await aiConfigApi.listToolCategories();
    } catch (err) {
      toast.show('error', err instanceof Error ? err.message : '加载工具分类失败');
    }
  }

  async function loadEntityDetail(entityId: string): Promise<void> {
    if (!entityId) {
      entityDetail.value = null;
      modelsForm.value = createEmptyModelsForm();
      return;
    }
    modelsLoading.value = true;
    try {
      const detail = await aiConfigApi.getEntity(entityId);
      entityDetail.value = detail;
      modelOptions.value = [
        detail.default_model,
        asRecord(detail.model_routing).chat,
        asRecord(detail.model_routing).summary,
        asRecord(detail.model_routing).vision,
        asRecord(detail.model_routing).embedding,
        detail.asr_model,
        detail.tts_model,
      ].reduce<NormalizedModelOption[]>(
        (acc, value) => ensureCustomModelOption(acc, value),
        modelOptions.value,
      );
      applyEntityDetailToForm(detail);
      loadAllV2Data(entityId);
    } catch (err) {
      toast.show('error', err instanceof Error ? err.message : '加载实体详情失败');
    } finally {
      modelsLoading.value = false;
    }
  }

  async function loadCapabilitySnapshot(entityId: string): Promise<void> {
    if (!entityId) { capabilitySnapshot.value = null; return; }
    capabilityLoading.value = true;
    try {
      capabilitySnapshot.value = await aiConfigV2.getEntityCapabilities(entityId);
    } catch {
      capabilitySnapshot.value = null;
    } finally {
      capabilityLoading.value = false;
    }
  }

  async function runCapabilityValidation(options: { silent?: boolean } = {}): Promise<ValidationResult | null> {
    const silent = options.silent === true;
    if (!selectedEntityId.value) {
      if (!silent) toast.show('warning', '请先选择一个实体');
      return null;
    }
    try {
      capabilityValidation.value = await aiConfigV2.validateEntityCapabilities(selectedEntityId.value);
      if (!silent) {
        const summary = summarizeValidation(capabilityValidation.value);
        toast.show(summary.level, summary.message);
      }
      return capabilityValidation.value;
    } catch (err) {
      if (!silent) toast.show('error', err instanceof Error ? err.message : '能力验证失败');
      return null;
    }
  }

  async function loadMcpData(entityId: string): Promise<void> {
    if (!entityId) { mcpServers.value = []; mcpBindings.value = []; return; }
    mcpLoading.value = true;
    try {
      const [servers, bindings] = await Promise.all([
        aiConfigV2.listMcpServers(entityId),
        aiConfigV2.listMcpBindings(entityId),
      ]);
      mcpServers.value = servers;
      mcpBindings.value = bindings;
    } catch {
      mcpServers.value = [];
      mcpBindings.value = [];
    } finally {
      mcpLoading.value = false;
    }
  }

  async function saveMcpBindingForEntity(serverId: string): Promise<void> {
    if (!selectedEntityId.value) { toast.show('warning', '请先选择一个实体'); return; }
    try {
      await aiConfigV2.saveMcpBinding(selectedEntityId.value, {
        server_id: serverId,
        enabled: true,
      });
      toast.show('success', 'MCP 绑定已保存');
      await loadMcpData(selectedEntityId.value);
    } catch (err) {
      toast.show('error', err instanceof Error ? err.message : '保存 MCP 绑定失败');
    }
  }

  async function probeMcpServerAndRefresh(serverId: string): Promise<void> {
    if (!selectedEntityId.value) { toast.show('warning', '请先选择一个实体'); return; }
    try {
      const result = await aiConfigV2.probeMcpServer(selectedEntityId.value, serverId);
      if (result.status === 'discovered') {
        const tools = result.capabilities?.tools?.length ?? 0;
        const resources = result.capabilities?.resources?.length ?? 0;
        toast.show('success', `MCP Server 探测成功: ${tools} tools, ${resources} resources`);
      } else if (result.status === 'unsupported_transport') {
        toast.show('warning', `MCP Server 传输协议不支持实时探测`);
      } else {
        toast.show('warning', `MCP Server 探测结果: ${result.status}`);
      }
      await loadMcpData(selectedEntityId.value);
      if (selectedEntityId.value) {
        await loadCapabilitySnapshot(selectedEntityId.value);
      }
    } catch (err) {
      toast.show('error', err instanceof Error ? err.message : 'MCP 探测失败');
    }
  }

  async function loadSkillData(entityId: string): Promise<void> {
    if (!entityId) { skillRegistry.value = []; skillBindings.value = []; return; }
    skillsLoading.value = true;
    try {
      const [registry, bindings] = await Promise.all([
        aiConfigV2.listSkillRegistry(),
        aiConfigV2.listEntitySkills(entityId),
      ]);
      skillRegistry.value = registry;
      skillBindings.value = bindings;
    } catch {
      skillRegistry.value = [];
      skillBindings.value = [];
    } finally {
      skillsLoading.value = false;
    }
  }

  async function saveSkillBindingForEntity(skillSlug: string): Promise<void> {
    if (!selectedEntityId.value) { toast.show('warning', '请先选择一个实体'); return; }
    try {
      await aiConfigV2.saveSkillBinding(selectedEntityId.value, {
        skill_slug: skillSlug,
        enabled: true,
      });
      toast.show('success', 'Skill 绑定已保存');
      await loadSkillData(selectedEntityId.value);
    } catch (err) {
      toast.show('error', err instanceof Error ? err.message : '保存 Skill 绑定失败');
    }
  }

  async function deleteSkillBindingById(bindingId: string): Promise<void> {
    if (!selectedEntityId.value) return;
    try {
      await aiConfigV2.deleteSkillBinding(selectedEntityId.value, bindingId);
      toast.show('success', 'Skill 绑定已删除');
      await loadSkillData(selectedEntityId.value);
    } catch (err) {
      toast.show('error', err instanceof Error ? err.message : '删除 Skill 绑定失败');
    }
  }

  async function loadCacheMetrics(): Promise<void> {
    if (!selectedEntityId.value) { cacheMetrics.value = null; return; }
    cacheLoading.value = true;
    try {
      cacheMetrics.value = await aiConfigV2.getCacheMetrics(selectedEntityId.value, 24);
    } catch {
      cacheMetrics.value = null;
    } finally {
      cacheLoading.value = false;
    }
  }

  async function runCacheInvalidate(): Promise<void> {
    if (!selectedEntityId.value) { toast.show('warning', '请先选择一个实体'); return; }
    try {
      const result = await aiConfigV2.invalidateCache(selectedEntityId.value);
      toast.show('success', `缓存已失效: ${result.invalidated} 条`);
      await loadCacheMetrics();
    } catch (err) {
      toast.show('error', err instanceof Error ? err.message : '缓存失效失败');
    }
  }

  async function runConnectionTestWithCapabilities(): Promise<void> {
    if (!selectedEntityId.value) {
      toast.show('warning', '请先选择一个实体');
      return;
    }
    modelsTesting.value = true;
    try {
      const result = await aiConfigV2.testConnectionWithCapabilities(selectedEntityId.value);
      const remoteModelNames = Array.isArray(result.models) ? result.models : [];
      const remoteModelOptions: AiModelOption[] = remoteModelNames.map((m: string) => ({ id: m, name: m }));
      let merged = mergeModelOptions([], remoteModelOptions, 'remote');
      merged = ensureCustomModelOption(merged, modelsForm.value.default_model);
      merged = ensureCustomModelOption(merged, modelsForm.value.asr_model);
      merged = ensureCustomModelOption(merged, modelsForm.value.tts_model);
      modelOptions.value = merged;
      const capSummary = result.capabilities
        ? ` | model=${result.capabilities.model_id ?? '?'} tools=${result.capabilities.tool_count ?? '?'}`
        : '';
      toast.show('success', `连通成功，发现 ${remoteModelNames.length} 个模型${capSummary}`);
    } catch (err) {
      toast.show('error', err instanceof Error ? err.message : '连通测试失败');
    } finally {
      modelsTesting.value = false;
    }
  }

  function loadAllV2Data(entityId: string): void {
    loadCapabilitySnapshot(entityId);
    loadMcpData(entityId);
    loadSkillData(entityId);
    loadCacheMetrics();
  }

  async function bootstrapModelsTab(): Promise<void> {
    if (modelsBootstrapped.value) return;
    modelsBootstrapped.value = true;
    await Promise.all([loadEntityList(), loadModelsCatalog(), loadToolCategories()]);
    if (selectedEntityId.value) {
      await loadEntityDetail(selectedEntityId.value);
    }
  }

  async function refreshModelsTab(): Promise<void> {
    if (!confirmDiscardModelChanges()) {
      return;
    }
    await Promise.all([loadEntityList(), loadModelsCatalog(), loadToolCategories()]);
    if (selectedEntityId.value) {
      await loadEntityDetail(selectedEntityId.value);
    }
  }

  async function selectEntity(entityId: string): Promise<void> {
    if (entityId === selectedEntityId.value) {
      return;
    }
    if (!confirmDiscardModelChanges()) {
      return;
    }
    selectedEntityId.value = entityId;
    await loadEntityDetail(entityId);
  }

  async function saveModelsForm(): Promise<void> {
    if (!selectedEntityId.value) {
      toast.show('warning', '请先选择一个实体');
      return;
    }
    modelsSaving.value = true;
    try {
      const skillBindings = parseSkillBindings(modelsForm.value.skills_bindings);
      const defaultModel = modelsForm.value.default_model.trim();
      const payload: AiEntitySavePayload = {
        config_version: modelsForm.value.config_version,
        base_url: modelsForm.value.base_url,
        api_key: modelsForm.value.api_key,
        default_model: defaultModel,
        asr_model: modelsForm.value.asr_model,
        tts_model: modelsForm.value.tts_model,
        tts_voice: modelsForm.value.tts_voice,
        api_format: modelsForm.value.api_format,
        timeout: modelsForm.value.timeout,
        max_retries: modelsForm.value.max_retries,
        retry_delay: modelsForm.value.retry_delay,
        system_prompt: modelsForm.value.system_prompt,
        task_template: modelsForm.value.task_template,
        denied_tools: modelsForm.value.denied_tools,
        allowed_tool_categories: modelsForm.value.allowed_tool_categories,
        providers: buildProvidersPayload(modelsForm.value),
        model_routing: {
          default: defaultModel,
          chat: modelsForm.value.chat_model.trim() || defaultModel,
          summary: modelsForm.value.summary_model.trim() || null,
          vision: modelsForm.value.vision_model.trim() || null,
          audio_transcription: modelsForm.value.asr_model.trim() || null,
          audio_speech: modelsForm.value.tts_model.trim() || null,
          embedding: modelsForm.value.embedding_model.trim() || null,
        },
        models: defaultModel
          ? {
              [defaultModel]: {
                provider_model: defaultModel,
                provider_ref: modelsForm.value.model_provider_ref.trim() || 'default',
                api_format: modelsForm.value.api_format,
                context_window: modelsForm.value.context_window,
                max_output_tokens: modelsForm.value.max_output_tokens,
                modalities: {
                  input: modelsForm.value.model_input_modalities,
                  output: modelsForm.value.model_output_modalities,
                },
                capabilities: {
                  tool_calling: modelsForm.value.model_tool_calling,
                  parallel_tool_calls: modelsForm.value.model_parallel_tool_calls,
                  streaming: modelsForm.value.model_streaming,
                  structured_output: modelsForm.value.model_structured_output,
                  prompt_cache: modelsForm.value.model_prompt_cache,
                },
              },
            }
          : {},
        tooling: {
          enabled: modelsForm.value.tooling_enabled,
          max_rounds: modelsForm.value.tooling_max_rounds,
          allow_parallel: modelsForm.value.tooling_allow_parallel,
          allowed_tool_sources: modelsForm.value.allowed_tool_sources,
          allowed_tool_categories: modelsForm.value.allowed_tool_categories,
          allowed_tools: null,
          denied_tools: modelsForm.value.denied_tools,
          write_action_policy: modelsForm.value.write_action_policy,
        },
        mcp: {
          enabled: modelsForm.value.mcp_enabled,
          binding_ids: splitTextList(modelsForm.value.mcp_binding_ids),
          tool_name_prefix: modelsForm.value.mcp_tool_name_prefix,
          discovery_cache_ttl_seconds: modelsForm.value.mcp_discovery_cache_ttl_seconds,
          fail_closed: modelsForm.value.mcp_fail_closed,
        },
        skills: {
          enabled: modelsForm.value.skills_enabled,
          allowlist: splitTextList(modelsForm.value.skills_allowlist),
          bindings: skillBindings,
        },
        subagents: {
          enabled: modelsForm.value.subagents_enabled,
          mode: 'entity_handoff',
          allowed_entity_ids: splitTextList(modelsForm.value.subagents_allowed_entity_ids),
          max_depth: modelsForm.value.subagents_max_depth,
          max_concurrency: modelsForm.value.subagents_max_concurrency,
          inherit_parent_context: modelsForm.value.subagents_inherit_parent_context,
          require_tool_calling_capability: modelsForm.value.subagents_require_tool_calling_capability,
          handoff_prompt: modelsForm.value.subagents_handoff_prompt,
        },
        context_policy: {
          strategy: modelsForm.value.context_strategy,
          max_context_tokens: modelsForm.value.max_context_tokens,
          compression_threshold_tokens: modelsForm.value.compression_threshold_tokens,
          preserve_recent_messages: modelsForm.value.preserve_recent_messages,
          summary_model: modelsForm.value.summary_model.trim() || modelsForm.value.default_model.trim() || null,
          summary_max_tokens: modelsForm.value.summary_max_tokens,
          persist_summaries: modelsForm.value.persist_summaries,
        },
        cache_policy: {
          enabled: modelsForm.value.cache_enabled,
          provider_prompt_cache: {
            enabled: modelsForm.value.provider_prompt_cache_enabled,
            retention: modelsForm.value.provider_prompt_cache_retention,
            key_namespace: 'flight_monitor',
          },
          context_cache: {
            backend: modelsForm.value.context_cache_backend,
            ttl_seconds: modelsForm.value.context_cache_ttl_seconds,
          },
          tool_result_cache: {
            enabled: modelsForm.value.tool_result_cache_enabled,
            ttl_seconds: modelsForm.value.tool_result_cache_ttl_seconds,
            cacheable_tools: splitTextList(modelsForm.value.cacheable_tools),
          },
          mcp_resource_cache: {
            enabled: modelsForm.value.mcp_resource_cache_enabled,
            ttl_seconds: modelsForm.value.mcp_resource_cache_ttl_seconds,
          },
        },
        security: {
          mask_sensitive: true,
          log_prompts: false,
          max_input_bytes: modelsForm.value.max_input_bytes,
          allowed_input_mime_types: splitTextList(modelsForm.value.allowed_input_mime_types),
        },
      };
      await aiConfigApi.saveEntity(selectedEntityId.value, payload);
      await loadEntityDetail(selectedEntityId.value);
      const validation = await runCapabilityValidation({ silent: true });
      if (validation && !validation.valid) {
        const errorCount = validation.errors.filter(e => e.severity === 'error').length;
        const warnCount = validation.errors.filter(e => e.severity === 'warning').length;
        const parts: string[] = [];
        if (errorCount > 0) parts.push(`${errorCount} 个错误`);
        if (warnCount > 0) parts.push(`${warnCount} 个警告`);
        toast.show(
          errorCount > 0 ? 'error' : 'warning',
          `保存成功，但能力校验发现${parts.join('、')}，请检查能力面板`,
        );
      } else {
        toast.show('success', '保存成功，能力校验通过');
      }
    } catch (err) {
      toast.show('error', err instanceof Error ? err.message : '保存失败');
    } finally {
      modelsSaving.value = false;
    }
  }

  function revertModelsForm(): void {
    if (entityDetail.value) {
      applyEntityDetailToForm(entityDetail.value);
      toast.show('info', '已恢复为已保存配置');
    }
  }

  const audioStatus = ref<'idle' | 'connecting' | 'connected' | 'closed' | 'error'>('idle');
  const audioError = ref<string>('');
  const audioLogs = ref<AudioLogEntry[]>([]);
  const audioAsrText = ref('');
  const audioSelectedFile = ref<File | null>(null);
  let audioLogSeq = 0;
  let audioSession: ReturnType<typeof useRealtimeAudioSession> | null = null;
  let audioChunkSeq = 0;

  function appendAudioLog(direction: 'in' | 'out', type: string, detail: string) {
    audioLogSeq += 1;
    audioLogs.value = [
      { id: audioLogSeq, direction, type, detail, ts: Date.now() },
      ...audioLogs.value.slice(0, 99),
    ];
  }

  function handleAudioEvent(event: RealtimeServerEvent) {
    const evType = typeof event.type === 'string' ? event.type : 'unknown';
    switch (evType) {
      case 'session.ready':
        audioStatus.value = 'connected';
        appendAudioLog('in', evType, `session=${(event as { session_id?: string }).session_id || ''}`);
        break;
      case 'asr.partial':
      case 'asr.final': {
        const text = (event as { text?: string }).text || '';
        const conf = Number((event as { confidence?: number }).confidence || 0);
        audioAsrText.value = text;
        appendAudioLog('in', evType, `"${text}" (${(conf * 100).toFixed(0)}%)`);
        break;
      }
      case 'error': {
        const message = (event as { message?: string }).message || '实时音频服务异常';
        audioError.value = message;
        appendAudioLog('in', evType, message);
        break;
      }
      case 'session.closed': {
        audioStatus.value = 'closed';
        appendAudioLog('in', evType, (event as { reason?: string }).reason || '');
        break;
      }
      default:
        appendAudioLog('in', evType, JSON.stringify(event).slice(0, 120));
    }
  }

  async function audioConnect(): Promise<void> {
    if (!selectedEntityId.value) {
      toast.show('warning', '请先选择一个实体');
      return;
    }
    if (audioSession) {
      audioSession.cancel('user_cancelled');
      audioSession = null;
    }
    audioError.value = '';
    audioAsrText.value = '';
    audioChunkSeq = 0;
    audioStatus.value = 'connecting';
    appendAudioLog('out', 'connect', `entity=${selectedEntityId.value}`);
    audioSession = useRealtimeAudioSession({
      entityId: selectedEntityId.value,
      onEvent: handleAudioEvent,
    });
    try {
      await audioSession.connect();
    } catch (err) {
      audioStatus.value = 'error';
      audioError.value = err instanceof Error ? err.message : '实时音频连接失败';
      appendAudioLog('in', 'error', audioError.value);
    }
  }

  function audioDisconnect(): void {
    if (audioSession) {
      audioSession.cancel('user_cancelled');
      audioSession = null;
    }
    audioStatus.value = 'closed';
    appendAudioLog('out', 'disconnect', '');
  }

  function audioHandleFile(event: Event): void {
    const input = event.target as HTMLInputElement;
    audioSelectedFile.value = input.files?.[0] ?? null;
  }

  function fileToBase64(file: File): Promise<string> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onerror = () => reject(new Error('读取音频文件失败'));
      reader.onload = () => {
        const raw = String(reader.result || '');
        const [, base64 = ''] = raw.split(',');
        if (!base64) {
          reject(new Error('音频文件内容为空'));
          return;
        }
        resolve(base64);
      };
      reader.readAsDataURL(file);
    });
  }

  async function audioSendSelectedChunk(): Promise<void> {
    if (!audioSession || !audioSession.isConnected.value) {
      toast.show('warning', '请先连接实时音频会话');
      return;
    }
    if (!audioSelectedFile.value) {
      toast.show('warning', '请先选择真实音频文件');
      return;
    }
    const file = audioSelectedFile.value;
    audioChunkSeq += 1;
    try {
      const audioBase64 = await fileToBase64(file);
      audioSession.sendAudioChunk(audioBase64);
      appendAudioLog('out', 'audio.chunk', `seq=${audioChunkSeq}; file=${file.name}; bytes=${file.size}`);
    } catch (err) {
      toast.show('error', err instanceof Error ? err.message : '音频发送失败');
    }
  }

  function audioSendEnd(): void {
    if (!audioSession || !audioSession.isConnected.value) {
      toast.show('warning', '请先连接实时音频会话');
      return;
    }
    audioSession.endAudio();
    appendAudioLog('out', 'audio.end', '');
  }

  watch(activeTab, (tab) => {
    if (tab === 'models') {
      bootstrapModelsTab();
    }
  });

  onBeforeUnmount(() => {
    if (audioSession) {
      audioSession.cancel('user_cancelled');
      audioSession = null;
    }
  });

  onMounted(() => {
    fetchData();
  });

  return {
    activeTab, searchQuery, loading, objects, actions,
    filteredObjects, filteredActions, fetchData, getRiskBadgeClass,
    sidebarUser, handleLogout,
    capabilitySnapshot, capabilityLoading, capabilityValidation,
    mcpServers, mcpBindings, mcpLoading,
    skillRegistry, skillBindings, skillsLoading,
    cacheMetrics, cacheLoading,
    entities, selectedEntityId, entityDetail, modelOptions, toolCategories,
    modelsForm, modelsLoading, modelsSaving, modelsTesting,
    modelsBootstrapped, entitySearch, savedModelsFormJson, isModelsDirty,
    modalityOptions, toolSourceOptions, existingCapabilityRows, entityPolicyRows,
    providerRefOptions, addProviderEntry, removeProviderEntry,
    toggleModelInputModality, toggleModelOutputModality,
    toggleToolSource, toggleAllowedToolCategory,
    categoryToolMap, deniedToolsSet, allowedToolCategoriesSet,
    toggleDeniedTool,
    filteredEntities, ensureCustomModelOption, mergeModelOptions,
    loadEntityList, loadModelsCatalog, loadToolCategories,
    bootstrapModelsTab, refreshModelsTab, selectEntity,
    loadEntityDetail, loadMcpData, loadSkillData, loadCacheMetrics,
    saveMcpBindingForEntity, probeMcpServerAndRefresh,
    saveSkillBindingForEntity, deleteSkillBindingById,
    runCacheInvalidate, runConnectionTestWithCapabilities,
    runCapabilityValidation,
    saveModelsForm, revertModelsForm,
    audioStatus, audioError, audioLogs, audioAsrText, audioSelectedFile,
    audioConnect, audioDisconnect, audioHandleFile,
    audioSendSelectedChunk, audioSendEnd,
  };
}
