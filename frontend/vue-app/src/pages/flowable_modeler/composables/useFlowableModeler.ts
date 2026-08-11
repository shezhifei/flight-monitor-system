import { computed, nextTick, ref } from 'vue';
import { useApi } from '@/composables/useApi';
import { useToast } from '@/composables/useToast';
import { downloadTextFile } from '@/lib/download';
import CustomPaletteModule from '../custom';
import {
  createDefaultFormTaskConfig, normalizeFormTaskConfig, sanitizeIdentifier,
} from '../formTaskDesigner';
import {
  injectFormBindingsIntoBpmnXml, parseFormBindingsFromBpmnXml,
} from '../formBindingXml';
import {
  defaultNotificationRule,
  guessNodeTypeFromName,
  injectNotificationRulesIntoBpmnXml,
  normalizeNotificationRule,
  parseNotificationRulesFromBpmnXml,
  type BusinessNodeType,
  type NodeRuleState,
  type NotificationRule,
} from '../notificationRule';
import { cleanBpmnXml, ensureRenderableBpmnXml } from '../bpmnXml';
import type { CaseTypeItem, FormTaskBindingConfig } from '../types';

const COMMON_TENANT = 'COMMON';

type ScopeMode = 'department' | 'common';
type LegType = 'outbound' | 'inbound';

interface UserContextPayload {
  name?: string; username?: string; display_name?: string; role?: string;
  department?: string; department_id?: string; departmentId?: string;
}


interface CaseProperties {
  binding_policy: {
    flight_required: boolean;
    allowed_leg_types: LegType[];
    default_leg_type: LegType | null;
    leg_type_required: boolean;
    flight_match_policy: {
      allow_numeric_suffix: boolean;
      exclude_cancelled: boolean;
      exclude_departed: boolean;
      exclude_actual_departure: boolean;
      time_window_hours_before: number;
      time_window_hours_after: number;
      min_auto_match_score: number;
    };
  };
  extra_info_schema: { fields: Record<string, unknown>; summary_template: string };
  workflow_policy: { batch_notification_enabled: boolean; batch_receipt_mode: 'shared_group' | 'per_case' };
  duplicate_policy: {
    enabled: boolean;
    fields: string[];
    include_extra_info: boolean;
    include_bound_leg: boolean;
    active_statuses: string[];
  };
}

interface AiExtractionConfig {
  enabled: boolean;
  leg_binding: {
    allowed: string[];
    default: string | null;
    required: boolean;
  };
  flight_matching: {
    window_hours_before: number;
    window_hours_after: number;
    prefer_leg: string | null;
    min_auto_match_score: number;
    exclude_cancelled: boolean;
    exclude_departed: boolean;
    exclude_actual_departure: boolean;
  };
  description_template: string;
  remarks_template: string;
  forbidden_fields: string[];
  extraction_fields: Record<string, unknown>;
}

interface BpmnModelerEventBus {
  on: (event: string, callback: (event: Record<string, unknown>) => void) => void;
}

interface BpmnCanvasService {
  resized?: () => void;
  zoom?: (level: string | number, center?: string) => void;
}

interface BpmnModelerInstance {
  get: (service: string) => unknown;
  importXML: (xml: string) => Promise<unknown>;
  saveXML: (options?: { format?: boolean }) => Promise<{ xml: string }>;
  destroy?: () => void;
  [key: string]: unknown;
}

/**
 * 页面级单例：FlowableModeler.vue 与 PropertiesPanel.vue 都会调用本 composable，
 * 必须共享同一份状态，否则属性面板与画布会脱节。
 */
let sharedModelerState: ReturnType<typeof createFlowableModelerState> | null = null;

export function useFlowableModeler() {
  if (!sharedModelerState) {
    sharedModelerState = createFlowableModelerState();
  }
  return sharedModelerState;
}

function createFlowableModelerState() {
  const api = useApi();
  const toast = useToast();

  const canvasRef = ref<HTMLElement | null>(null);
  const modeler = ref<BpmnModelerInstance | null>(null);
  const connectionStatus = ref('配置模式');
  const userName = ref('加载中...');
  const userRole = ref('流程设计器');
  const userAvatar = ref('A');
  const departmentTenant = ref('');
  const departmentLabel = ref('未配置部门');
  const currentScope = ref<ScopeMode>('department');
  const eventList = ref<CaseTypeItem[]>([]);
  const searchQuery = ref('');
  const filteredEventList = ref<CaseTypeItem[]>([]);
  const caseTypeLoadError = ref('');
  const selectedCaseId = ref<string | null>(null);
  const diagramName = ref('-');
  const diagramCode = ref('-');
  const caseDescription = ref('');
  const isLoading = ref(false);
  const hasSelectedDiagram = ref(false);
  const selectedTaskId = ref<string | null>(null);
  const selectedTaskName = ref('');
  const formTaskBindings = ref<Record<string, FormTaskBindingConfig>>({});
  /** 业务节点规则（通知/等待回执等），按画布 element id 索引 */
  const nodeRules = ref<Record<string, NodeRuleState>>({});
  const referenceDepartments = ref<Array<{ id: string; name: string }>>([]);
  const referenceDepartmentsLoading = ref(false);
  const referenceDepartmentsError = ref('');
  const importedXml = ref<string | null>(null);
  const showAiChat = ref(false);

  // 新建业务事项流程弹窗
  const showCreateCaseModal = ref(false);
  const createCaseCode = ref('');
  const createCaseName = ref('');
  const createCaseScope = ref<'DEPARTMENT' | 'COMMON'>('DEPARTMENT');
  const createCaseError = ref('');
  const createCaseSubmitting = ref(false);

  const isAiConfigExpanded = ref(false);
  const isCasePropertiesExpanded = ref(false);
  const aiConfig = ref<AiExtractionConfig>(createDefaultAiExtractionConfig());
  const aiConfigAliasesText = ref('');
  const aiConfigTriggerText = ref('');
  const aiConfigForbiddenText = ref('');
  const aiConfigFieldsJsonText = ref('');
  const aiConfigFieldsJsonError = ref('');
  const casePropertiesFieldsJsonText = ref('');
  const casePropertiesFieldsJsonError = ref('');
  const duplicatePolicyFieldsText = ref('');
  const duplicatePolicyStatusesText = ref('');

  const contextVariables = ref<Array<{ key: string; label: string }>>([
    { key: 'flight_no', label: '航班号' },
    { key: 'arrival_time', label: '到达时间' },
    { key: 'departure_time', label: '出发时间' },
    { key: 'stand_id', label: '机位' },
    { key: 'terminal', label: '航站楼' },
    { key: 'airline', label: '航司' },
    { key: 'flight_type', label: '航班类型' },
  ]);

  const hasDepartmentScope = computed(() => Boolean(departmentTenant.value));
  const activeTenantId = computed(() => (
    currentScope.value === 'common' || !departmentTenant.value
      ? COMMON_TENANT
      : departmentTenant.value
  ));
  const activeScopeLabel = computed(() => (
    currentScope.value === 'common' ? '通用视图' : `当前部门: ${departmentLabel.value}`
  ));
  const activeTenantLabel = computed(() => (
    currentScope.value === 'common' ? COMMON_TENANT : departmentTenant.value || departmentLabel.value
  ));
  const selectedNodeType = computed<BusinessNodeType>(() => {
    const taskId = selectedTaskId.value;
    if (!taskId) return 'none';
    const fromRule = nodeRules.value[taskId]?.nodeType;
    if (fromRule && fromRule !== 'none') return fromRule;
    return guessNodeTypeFromName(selectedTaskName.value, taskId);
  });

  const selectedNotificationRule = computed<NotificationRule | null>(() => {
    const taskId = selectedTaskId.value;
    if (!taskId || selectedNodeType.value !== 'notification') return null;
    const rule = nodeRules.value[taskId]?.notificationRule;
    return normalizeNotificationRule(rule);
  });

  const selectedFormTaskConfig = computed<FormTaskBindingConfig | null>(() => {
    const taskId = selectedTaskId.value;
    if (!taskId) return null;
    if (selectedNodeType.value === 'notification' || selectedNodeType.value === 'wait_receipts') {
      return null;
    }
    return formTaskBindings.value[taskId] ?? null;
  });
  const selectedTaskRolesText = computed(() => (
    selectedFormTaskConfig.value ? selectedFormTaskConfig.value.roles?.join(', ') ?? '' : ''
  ));

  const persistedFormTaskCount = computed(() => (
    Object.values(formTaskBindings.value).length
  ));

  function firstNonEmpty(...values: Array<unknown>): string | undefined {
    for (const value of values) {
      if (typeof value === 'string' && value.trim()) return value.trim();
    }
    return undefined;
  }

  function buildQuery(params: Record<string, string | undefined>): string {
    const search = new URLSearchParams();
    Object.entries(params).forEach(([key, value]) => {
      if (value && value.trim()) search.set(key, value);
    });
    const query = search.toString();
    return query ? `?${query}` : '';
  }

  function handleSearch(): void {
    const query = searchQuery.value.trim().toLowerCase();
    const matched = query
      ? eventList.value.filter((item) => item.name.toLowerCase().includes(query) || item.code.toLowerCase().includes(query))
      : [...eventList.value];
    // 与 legacy 一致：未弃用优先，弃用沉底
    matched.sort((a, b) => {
      const aDep = a.is_active === false ? 1 : 0;
      const bDep = b.is_active === false ? 1 : 0;
      if (aDep !== bDep) return aDep - bDep;
      return a.name.localeCompare(b.name, 'zh-CN');
    });
    filteredEventList.value = matched;
  }

  function createDefaultCaseProperties(): CaseProperties {
    return {
      binding_policy: {
        flight_required: true, allowed_leg_types: [] as LegType[],
        default_leg_type: null as LegType | null, leg_type_required: false,
        flight_match_policy: {
          allow_numeric_suffix: true, exclude_cancelled: true, exclude_departed: true,
          exclude_actual_departure: true, time_window_hours_before: 3,
          time_window_hours_after: 8, min_auto_match_score: 0.85,
        },
      },
      extra_info_schema: { fields: {} as Record<string, unknown>, summary_template: '' },
      workflow_policy: { batch_notification_enabled: false, batch_receipt_mode: 'per_case' as 'shared_group' | 'per_case' },
      duplicate_policy: { enabled: false, fields: [] as string[], include_extra_info: false, include_bound_leg: true, active_statuses: [] as string[] },
    };
  }

  function createDefaultAiExtractionConfig(): AiExtractionConfig {
    return {
      enabled: false,
      leg_binding: { allowed: [], default: null, required: false },
      flight_matching: {
        window_hours_before: 3,
        window_hours_after: 8,
        prefer_leg: null,
        min_auto_match_score: 0.85,
        exclude_cancelled: true,
        exclude_departed: true,
        exclude_actual_departure: true,
      },
      description_template: '',
      remarks_template: '',
      forbidden_fields: [],
      extraction_fields: {},
    };
  }

  function readStringArray(value: unknown): string[] {
    return Array.isArray(value)
      ? value.filter((item: unknown): item is string => typeof item === 'string' && item.trim().length > 0)
      : [];
  }

  function readRecord(value: unknown): Record<string, unknown> {
    return value && typeof value === 'object' && !Array.isArray(value)
      ? value as Record<string, unknown>
      : {};
  }

  function normalizeAiExtractionConfig(raw: Record<string, unknown>): AiExtractionConfig {
    const defaults = createDefaultAiExtractionConfig();
    const legBinding = readRecord(raw.leg_binding);
    const flightMatching = readRecord(raw.flight_matching);
    return {
      enabled: true,
      leg_binding: {
        allowed: readStringArray(legBinding.allowed),
        default: typeof legBinding.default === 'string' && legBinding.default.trim() ? legBinding.default : null,
        required: Boolean(legBinding.required),
      },
      flight_matching: {
        window_hours_before: typeof flightMatching.window_hours_before === 'number' ? flightMatching.window_hours_before : defaults.flight_matching.window_hours_before,
        window_hours_after: typeof flightMatching.window_hours_after === 'number' ? flightMatching.window_hours_after : defaults.flight_matching.window_hours_after,
        prefer_leg: typeof flightMatching.prefer_leg === 'string' && flightMatching.prefer_leg.trim() ? flightMatching.prefer_leg : null,
        min_auto_match_score: typeof flightMatching.min_auto_match_score === 'number' ? flightMatching.min_auto_match_score : defaults.flight_matching.min_auto_match_score,
        exclude_cancelled: flightMatching.exclude_cancelled !== undefined ? Boolean(flightMatching.exclude_cancelled) : defaults.flight_matching.exclude_cancelled,
        exclude_departed: flightMatching.exclude_departed !== undefined ? Boolean(flightMatching.exclude_departed) : defaults.flight_matching.exclude_departed,
        exclude_actual_departure: flightMatching.exclude_actual_departure !== undefined ? Boolean(flightMatching.exclude_actual_departure) : defaults.flight_matching.exclude_actual_departure,
      },
      description_template: typeof raw.description_template === 'string' ? raw.description_template : '',
      remarks_template: typeof raw.remarks_template === 'string' ? raw.remarks_template : '',
      forbidden_fields: readStringArray(raw.forbidden_fields),
      extraction_fields: readRecord(raw.extraction_fields),
    };
  }

  function normalizeCaseProperties(raw: unknown): CaseProperties {
    const defaults = createDefaultCaseProperties();
    if (!raw || typeof raw !== 'object') return defaults;
    const source = raw as Record<string, unknown>;
    const binding = source.binding_policy && typeof source.binding_policy === 'object' ? source.binding_policy as Record<string, unknown> : {};
    const matchPolicy = binding.flight_match_policy && typeof binding.flight_match_policy === 'object' ? binding.flight_match_policy as Record<string, unknown> : {};
    const extraInfo = source.extra_info_schema && typeof source.extra_info_schema === 'object' ? source.extra_info_schema as Record<string, unknown> : {};
    const workflow = source.workflow_policy && typeof source.workflow_policy === 'object' ? source.workflow_policy as Record<string, unknown> : {};
    const duplicate = source.duplicate_policy && typeof source.duplicate_policy === 'object' ? source.duplicate_policy as Record<string, unknown> : {};
    return {
      binding_policy: {
        flight_required: binding.flight_required !== undefined ? Boolean(binding.flight_required) : defaults.binding_policy.flight_required,
        allowed_leg_types: Array.isArray(binding.allowed_leg_types) ? binding.allowed_leg_types.filter((item: unknown): item is LegType => item === 'outbound' || item === 'inbound') : defaults.binding_policy.allowed_leg_types,
        default_leg_type: binding.default_leg_type === 'outbound' || binding.default_leg_type === 'inbound' ? binding.default_leg_type : null,
        leg_type_required: binding.leg_type_required !== undefined ? Boolean(binding.leg_type_required) : defaults.binding_policy.leg_type_required,
        flight_match_policy: {
          allow_numeric_suffix: matchPolicy.allow_numeric_suffix !== undefined ? Boolean(matchPolicy.allow_numeric_suffix) : defaults.binding_policy.flight_match_policy.allow_numeric_suffix,
          exclude_cancelled: matchPolicy.exclude_cancelled !== undefined ? Boolean(matchPolicy.exclude_cancelled) : defaults.binding_policy.flight_match_policy.exclude_cancelled,
          exclude_departed: matchPolicy.exclude_departed !== undefined ? Boolean(matchPolicy.exclude_departed) : defaults.binding_policy.flight_match_policy.exclude_departed,
          exclude_actual_departure: matchPolicy.exclude_actual_departure !== undefined ? Boolean(matchPolicy.exclude_actual_departure) : defaults.binding_policy.flight_match_policy.exclude_actual_departure,
          time_window_hours_before: matchPolicy.time_window_hours_before !== undefined ? Number(matchPolicy.time_window_hours_before) : defaults.binding_policy.flight_match_policy.time_window_hours_before,
          time_window_hours_after: matchPolicy.time_window_hours_after !== undefined ? Number(matchPolicy.time_window_hours_after) : defaults.binding_policy.flight_match_policy.time_window_hours_after,
          min_auto_match_score: matchPolicy.min_auto_match_score !== undefined ? Number(matchPolicy.min_auto_match_score) : defaults.binding_policy.flight_match_policy.min_auto_match_score,
        },
      },
      extra_info_schema: { fields: (extraInfo.fields && typeof extraInfo.fields === 'object' && !Array.isArray(extraInfo.fields) ? extraInfo.fields : defaults.extra_info_schema.fields) as Record<string, unknown>, summary_template: typeof extraInfo.summary_template === 'string' ? extraInfo.summary_template : '' },
      workflow_policy: { batch_notification_enabled: workflow.batch_notification_enabled !== undefined ? Boolean(workflow.batch_notification_enabled) : defaults.workflow_policy.batch_notification_enabled, batch_receipt_mode: workflow.batch_receipt_mode === 'shared_group' ? 'shared_group' : 'per_case' },
      duplicate_policy: { enabled: duplicate.enabled !== undefined ? Boolean(duplicate.enabled) : defaults.duplicate_policy.enabled, fields: Array.isArray(duplicate.fields) ? duplicate.fields.filter((item: unknown): item is string => typeof item === 'string' && item.trim().length > 0) : defaults.duplicate_policy.fields, include_extra_info: duplicate.include_extra_info !== undefined ? Boolean(duplicate.include_extra_info) : defaults.duplicate_policy.include_extra_info, include_bound_leg: duplicate.include_bound_leg !== undefined ? Boolean(duplicate.include_bound_leg) : defaults.duplicate_policy.include_bound_leg, active_statuses: Array.isArray(duplicate.active_statuses) ? duplicate.active_statuses.filter((item: unknown): item is string => typeof item === 'string' && item.trim().length > 0) : defaults.duplicate_policy.active_statuses },
    };
  }

  const caseProperties = ref(createDefaultCaseProperties());

  function unwrapPayload<T>(payload: unknown): T | null {
    if (payload == null) return null;
    if (typeof payload === 'object' && 'data' in (payload as Record<string, unknown>)) return ((payload as Record<string, unknown>).data ?? null) as T | null;
    return payload as T;
  }

  function readErrorMessage(payload: unknown, fallback: string): string {
    if (typeof payload === 'string' && payload.trim()) return payload;
    if (payload && typeof payload === 'object') {
      const record = payload as Record<string, unknown>;
      const detail = record.detail;
      if (typeof detail === 'string' && detail.trim()) return detail;
      if (detail && typeof detail === 'object') {
        const nested = detail as Record<string, unknown>;
        const nestedMessage = firstNonEmpty(nested.message, nested.detail);
        if (nestedMessage) return nestedMessage;
      }
      const message = firstNonEmpty(record.message, record.error);
      if (message) return message;
    }
    return fallback;
  }

  function normalizeCaseType(raw: unknown, fallback?: Partial<CaseTypeItem>): CaseTypeItem | null {
    if (typeof raw === 'string') {
      if (!fallback?.code) return null;
      return {
        id: fallback.id || fallback.code,
        name: fallback.name || fallback.code,
        code: fallback.code,
        description: fallback.description,
        is_active: fallback.is_active !== false,
        bpmn_xml: cleanBpmnXml(raw),
        xml_data: cleanBpmnXml(raw),
      };
    }
    if (!raw || typeof raw !== 'object') {
      if (!fallback?.code) return null;
      return {
        id: fallback.id || fallback.code,
        name: fallback.name || fallback.code,
        code: fallback.code,
        description: fallback.description,
        is_active: fallback.is_active !== false,
        bpmn_xml: cleanBpmnXml(fallback.bpmn_xml),
        xml_data: cleanBpmnXml(fallback.xml_data),
      };
    }
    const data = raw as Record<string, unknown>;
    const code = firstNonEmpty(data.code, data.case_type_code, fallback?.code);
    const id = firstNonEmpty(data.id, code, fallback?.id);
    const name = firstNonEmpty(data.name, data.display_name, code, fallback?.name, id);
    if (!id || !name || !code) return null;
    const rawActive = data.is_active ?? data.isActive ?? data.active;
    const isActive =
      rawActive === undefined || rawActive === null
        ? fallback?.is_active !== false
        : rawActive !== false && rawActive !== 0 && rawActive !== 'false' && rawActive !== '0';
    return {
      id,
      name,
      code,
      description: firstNonEmpty(data.description, fallback?.description),
      is_active: isActive,
      bpmn_xml: cleanBpmnXml(firstNonEmpty(data.bpmn_xml, data.bpmnXml, data.xml, fallback?.bpmn_xml)),
      xml_data: cleanBpmnXml(firstNonEmpty(data.xml_data, data.xmlData, fallback?.xml_data)),
      ai_extraction_config: (data.ai_extraction_config || fallback?.ai_extraction_config) as Record<string, unknown> | null | undefined,
      case_properties: (data.case_properties || fallback?.case_properties) as Record<string, unknown> | null | undefined,
    };
  }

  function findCaseType(key: string): CaseTypeItem | undefined {
    return eventList.value.find((item) => item.id === key || item.code === key);
  }

  /** 对齐 legacy toggleCaseTypeStatus：PUT /business-case-types/{code}/status */
  async function toggleCaseTypeStatus(caseIdOrCode: string, isActive: boolean): Promise<void> {
    const item = findCaseType(caseIdOrCode);
    const code = item?.code || caseIdOrCode;
    const label = item?.name || code;
    const action = isActive ? '恢复使用' : '挂起/弃用';
    if (
      !window.confirm(
        `确定要${action}业务事项类型「${label}」吗？\n${
          isActive
            ? '恢复后将重新允许触发流程。'
            : '弃用后系统将不会再自动触发该类型事件。'
        }`,
      )
    ) {
      return;
    }
    isLoading.value = true;
    try {
      const res = await api.put<unknown>(
        `/api/v2/business-case-types/${encodeURIComponent(code)}/status`,
        { is_active: isActive },
      );
      if (!res.ok) {
        toast.showToast('error', readErrorMessage(res.data, `${action}失败`), { duration: 5000 });
        return;
      }
      toast.showToast('success', `业务事项「${label}」已${action}`, { duration: 3000 });
      // 若弃用的是当前选中项，保持可选中查看，仅刷新列表状态
      await fetchCaseTypes();
      if (selectedCaseId.value) {
        const still = eventList.value.find(
          (c) => c.id === selectedCaseId.value || c.code === selectedCaseId.value,
        );
        if (!still) resetDiagramSelection();
      }
    } catch (err) {
      toast.showToast(
        'error',
        `${action}失败: ${err instanceof Error ? err.message : String(err)}`,
        { duration: 5000 },
      );
    } finally {
      isLoading.value = false;
    }
  }

  function deprecateCaseType(caseIdOrCode: string): void {
    void toggleCaseTypeStatus(caseIdOrCode, false);
  }

  function restoreCaseType(caseIdOrCode: string): void {
    void toggleCaseTypeStatus(caseIdOrCode, true);
  }

  function destroyModeler(): void {
    if (modeler.value) {
      try {
        modeler.value.destroy?.();
      } catch {
        // ignore destroy errors from bpmn-js
      }
      modeler.value = null;
    }
  }

  function resetDiagramSelection(): void {
    selectedCaseId.value = null; diagramName.value = '-'; diagramCode.value = '-';
    caseDescription.value = ''; hasSelectedDiagram.value = false; selectedTaskId.value = null;
    selectedTaskName.value = ''; formTaskBindings.value = {}; nodeRules.value = {};
    importedXml.value = null;
    destroyModeler();
    canvasRef.value = null;
  }

  async function fetchReferenceDepartments(): Promise<void> {
    if (referenceDepartments.value.length > 0 || referenceDepartmentsLoading.value) return;
    referenceDepartmentsLoading.value = true;
    referenceDepartmentsError.value = '';
    try {
      const res = await api.get<unknown>('/api/v2/reference/departments?page_size=200');
      if (!res.ok || !res.data) {
        referenceDepartmentsError.value = '科室列表加载失败';
        return;
      }
      const payload = unwrapPayload<unknown>(res.data);
      const rawList = Array.isArray(payload)
        ? payload
        : (payload && typeof payload === 'object'
          ? ((payload as Record<string, unknown>).items
            ?? (payload as Record<string, unknown>).records
            ?? (payload as Record<string, unknown>).list)
          : null);
      if (!Array.isArray(rawList)) {
        referenceDepartmentsError.value = '科室列表格式无效';
        return;
      }
      referenceDepartments.value = rawList
        .map((item) => {
          if (!item || typeof item !== 'object') return null;
          const row = item as Record<string, unknown>;
          const id = firstNonEmpty(row.id, row.department_id, row.code);
          const name = firstNonEmpty(row.name, row.department_name, row.label, id);
          if (!id || !name) return null;
          return { id, name };
        })
        .filter((item): item is { id: string; name: string } => Boolean(item));
    } catch (error) {
      referenceDepartmentsError.value = error instanceof Error ? error.message : '科室列表加载失败';
    } finally {
      referenceDepartmentsLoading.value = false;
    }
  }

  function upsertNodeRule(taskId: string, patch: Partial<NodeRuleState> & { nodeType: BusinessNodeType }): void {
    if (!taskId) return;
    const prev = nodeRules.value[taskId];
    const next: NodeRuleState = {
      nodeType: patch.nodeType,
      notificationRule: patch.nodeType === 'notification'
        ? normalizeNotificationRule(patch.notificationRule || prev?.notificationRule || defaultNotificationRule())
        : undefined,
    };
    nodeRules.value = { ...nodeRules.value, [taskId]: next };
  }

  function updateSelectedNotificationRule(partial: Partial<NotificationRule>): void {
    const taskId = selectedTaskId.value;
    if (!taskId) return;
    const current = normalizeNotificationRule(nodeRules.value[taskId]?.notificationRule);
    upsertNodeRule(taskId, {
      nodeType: 'notification',
      notificationRule: normalizeNotificationRule({ ...current, ...partial }),
    });
  }

  function toggleNotificationDepartment(departmentId: string, checked: boolean): void {
    const taskId = selectedTaskId.value;
    if (!taskId) return;
    const current = normalizeNotificationRule(nodeRules.value[taskId]?.notificationRule);
    const id = String(departmentId || '').trim();
    if (!id) return;
    const set = new Set(current.departmentIds);
    if (checked) set.add(id);
    else set.delete(id);
    const snapshots = { ...current.departmentSnapshots };
    if (checked) {
      const name = referenceDepartments.value.find((d) => d.id === id)?.name || snapshots[id] || id;
      snapshots[id] = name;
    }
    updateSelectedNotificationRule({
      departmentIds: Array.from(set),
      departmentSnapshots: snapshots,
    });
  }

  function toggleNotificationRole(role: string, checked: boolean): void {
    const current = normalizeNotificationRule(selectedNotificationRule.value);
    const set = new Set(current.roles);
    if (checked) set.add(role);
    else set.delete(role);
    updateSelectedNotificationRule({ roles: Array.from(set) });
  }

  function loadNodeRulesFromXml(xml: string): void {
    nodeRules.value = parseNotificationRulesFromBpmnXml(xml);
  }

  function applyBindingsAndRulesFromXml(xml: string): void {
    const parsed = parseFormBindingsFromBpmnXml(xml);
    const nextBindings: Record<string, FormTaskBindingConfig> = {};
    Object.entries(parsed.bindings || {}).forEach(([taskId, binding]) => {
      nextBindings[taskId] = normalizeFormTaskConfig(
        binding as Partial<FormTaskBindingConfig>,
        { taskId, taskName: (binding as { title?: string })?.title || taskId },
      );
    });
    formTaskBindings.value = nextBindings;
    loadNodeRulesFromXml(xml);
  }

  function decorateBpmnXmlForSave(xml: string): string {
    const cleaned = cleanBpmnXml(xml) || xml;
    const withForms = injectFormBindingsIntoBpmnXml(cleaned, importedXml.value || null, formTaskBindings.value);
    return injectNotificationRulesIntoBpmnXml(withForms, nodeRules.value);
  }

  function applyUserContext(source: Partial<UserContextPayload> | null | undefined): void {
    if (!source) return;
    const nextUserName = firstNonEmpty(source.name, source.display_name, source.username);
    if (nextUserName) { userName.value = nextUserName; userAvatar.value = nextUserName.charAt(0).toUpperCase(); }
    const nextRole = firstNonEmpty(source.role);
    if (nextRole) userRole.value = nextRole;
    const departmentId = firstNonEmpty(source.department_id, source.departmentId);
    const departmentName = firstNonEmpty(source.department);
    if (departmentId || departmentName) { departmentTenant.value = departmentId || departmentName || ''; departmentLabel.value = departmentName || departmentId || '未配置部门'; }
  }

  function getSelectedCaseItem(): CaseTypeItem | undefined {
    if (!selectedCaseId.value) return undefined;
    const key = selectedCaseId.value;
    return eventList.value.find((item) => item.id === key || item.code === key);
  }

  const createCaseScopeHint = computed(() => {
    if (createCaseScope.value === 'COMMON') {
      return '创建为通用流程后，所有可见范围内的用户都能看到该配置。';
    }
    if (departmentTenant.value) {
      return `默认按当前部门创建流程，归属部门为 ${departmentLabel.value}。`;
    }
    return '当前未识别到部门归属，将默认使用通用视图。';
  });

  function openCreateCaseModal(): void {
    createCaseError.value = '';
    createCaseCode.value = '';
    createCaseName.value = '';
    createCaseScope.value = currentScope.value === 'common' || !departmentTenant.value ? 'COMMON' : 'DEPARTMENT';
    showCreateCaseModal.value = true;
  }

  function closeCreateCaseModal(): void {
    showCreateCaseModal.value = false;
    createCaseError.value = '';
    createCaseSubmitting.value = false;
  }

  async function submitCreateCase(): Promise<void> {
    const code = createCaseCode.value.trim();
    const name = createCaseName.value.trim();
    const visibilityScope = createCaseScope.value;

    if (!code || !name) {
      createCaseError.value = '请完整填写业务事项代码和名称';
      return;
    }
    if (!/^[a-zA-Z0-9_]+$/.test(code)) {
      createCaseError.value = '业务事项代码只能包含字母、数字和下划线';
      return;
    }
    if (eventList.value.some((item) => item.code.toLowerCase() === code.toLowerCase())) {
      createCaseError.value = '该业务事项代码已存在，请更换后重试';
      return;
    }
    if (visibilityScope === 'DEPARTMENT' && !departmentTenant.value) {
      createCaseError.value = '当前未识别到部门归属，请改用通用范围';
      return;
    }

    createCaseError.value = '';
    createCaseSubmitting.value = true;
    isLoading.value = true;
    try {
      const res = await api.post<unknown>('/api/v2/business-case-types', {
        code,
        name,
        visibility_scope: visibilityScope,
      });
      if (!res.ok) {
        createCaseError.value = readErrorMessage(res.data, '创建失败，请稍后重试');
        return;
      }
      const created = normalizeCaseType(unwrapPayload(res.data), { code, name, id: code });
      closeCreateCaseModal();
      toast.showToast('success', '业务事项类型已创建', { duration: 3000 });
      // 切到对应作用域并刷新列表
      if (visibilityScope === 'COMMON' && currentScope.value !== 'common') {
        currentScope.value = 'common';
      } else if (visibilityScope === 'DEPARTMENT' && currentScope.value !== 'department' && departmentTenant.value) {
        currentScope.value = 'department';
      }
      await fetchCaseTypes();
      const selectKey = created?.id || created?.code || code;
      await selectCaseType(selectKey);
    } catch (err) {
      createCaseError.value = err instanceof Error ? err.message : '创建失败，请稍后重试';
    } finally {
      createCaseSubmitting.value = false;
      isLoading.value = false;
    }
  }

  function waitFrames(count = 1): Promise<void> {
    return new Promise((resolve) => {
      let left = Math.max(1, count);
      const step = () => {
        left -= 1;
        if (left <= 0) resolve();
        else requestAnimationFrame(step);
      };
      requestAnimationFrame(step);
    });
  }

  function isCanvasUsable(el: HTMLElement | null | undefined): el is HTMLElement {
    if (!el || !el.isConnected) return false;
    const rect = el.getBoundingClientRect();
    return rect.width > 8 && rect.height > 8;
  }

  function resizeAndFitViewport(): void {
    if (!modeler.value) return;
    try {
      const canvas = modeler.value.get('canvas') as BpmnCanvasService | undefined;
      canvas?.resized?.();
      canvas?.zoom?.('fit-viewport', 'auto');
    } catch {
      // ignore resize errors
    }
  }

  async function initModeler(): Promise<void> {
    try {
      if (!isCanvasUsable(canvasRef.value)) return;
      // 画布 DOM 被 v-if 重建后需要重新挂载 modeler
      destroyModeler();
      // Vite 可解析无扩展名；显式 .js 兼容部分打包器
      const mod = await import('bpmn-js/lib/Modeler');
      const BpmnModeler = (mod as { default?: unknown }).default ?? mod;
      if (!isCanvasUsable(canvasRef.value)) return;
      const container = canvasRef.value;
      // 清空可能残留的旧 DOM，避免重复 palette / canvas
      container.innerHTML = '';
      // bpmn-js / diagram-js 新版本已隐式绑定键盘，禁止传 keyboard.bindTo（会 console.error 且无意义）
      const instance = new (BpmnModeler as new (opts: Record<string, unknown>) => BpmnModelerInstance)({
        container,
        additionalModules: [CustomPaletteModule],
      });
      modeler.value = instance;
      const eventBus = instance.get('eventBus') as BpmnModelerEventBus;
      eventBus.on('selection.changed', (event: { newSelection?: unknown[] }) => {
        const selected = (event.newSelection?.[0] ?? null) as DiagramElement | null;
        selectDiagramElement(selected);
      });
      eventBus.on('create.end', (event: {
        context?: { shape?: DiagramElement; elements?: DiagramElement[] };
        shape?: DiagramElement;
        elements?: DiagramElement[];
      }) => {
        const shape = event?.context?.shape
          || event?.shape
          || event?.context?.elements?.find((el) => isUserTaskElement(el))
          || event?.elements?.find((el) => isUserTaskElement(el));
        if (!shape || !isUserTaskElement(shape)) return;

        const nodeType = String(shape.businessNodePreset?.nodeType || '').trim() as BusinessNodeType | '';
        const modeling = instance.get('modeling') as {
          updateProperties?: (el: unknown, props: Record<string, unknown>) => void;
        } | undefined;

        // 等待回执：尽量固定名称（id 在部分版本不可直接改）
        if (nodeType === 'wait_receipts') {
          try {
            modeling?.updateProperties?.(shape, { name: '等待回执' });
          } catch {
            const bo = getBusinessObject(shape);
            if (bo) bo.name = '等待回执';
          }
        }

        // 自定义节点 / 用户任务：创建后立即进入右侧可编辑
        selectDiagramElement(shape, {
          forceFormConfig: true,
          preferredName:
            shape.businessNodePreset?.defaultName
            || getBusinessObject(shape)?.name
            || (nodeType === 'wait_receipts' ? '等待回执' : nodeType === 'notification' ? '发送调度通知' : '表单任务节点'),
          presetNodeType: nodeType || undefined,
        });
      });
      // 画布上改名时同步到右侧
      eventBus.on('elements.changed', (event: { elements?: DiagramElement[] }) => {
        const el = (event.elements || []).find((item) => item?.id && item.id === selectedTaskId.value);
        if (!el?.id) return;
        const name = getBusinessObject(el)?.name;
        if (typeof name === 'string' && name !== selectedTaskName.value) {
          selectedTaskName.value = name;
          if (formTaskBindings.value[el.id]) {
            formTaskBindings.value = {
              ...formTaskBindings.value,
              [el.id]: normalizeFormTaskConfig(
                { ...formTaskBindings.value[el.id], title: name },
                { taskId: el.id, taskName: name },
              ),
            };
          }
        }
      });
    } catch (error) {
      console.warn('Failed to load BPMN modeler:', error);
      toast.showToast('error', `BPMN 建模器加载失败: ${error instanceof Error ? error.message : String(error)}`, { duration: 6000 });
    }
  }

  /** 确保画布已挂载且 modeler 可用（画布在 hasSelectedDiagram 后才渲染） */
  async function ensureModelerReady(forceRecreate = false): Promise<boolean> {
    // 等待 v-if 画布挂载 + 布局计算完成（flex 高度就绪）
    for (let attempt = 0; attempt < 12; attempt += 1) {
      await nextTick();
      if (!isCanvasUsable(canvasRef.value)) {
        await waitFrames(1);
        continue;
      }
      if (forceRecreate || !modeler.value) {
        await initModeler();
      }
      if (modeler.value) {
        resizeAndFitViewport();
        return true;
      }
      await waitFrames(1);
    }
    return false;
  }

  type DiagramElement = {
    id?: string;
    type?: string;
    businessObject?: { $type?: string; name?: string; id?: string };
    businessNodePreset?: { nodeType?: string; defaultName?: string };
    labelTarget?: DiagramElement;
  };

  function getBusinessObject(element: DiagramElement | null | undefined): { $type?: string; name?: string; id?: string } | null {
    if (!element) return null;
    return element.businessObject ?? null;
  }

  function getElementType(element: DiagramElement | null | undefined): string {
    if (!element) return '';
    return String(element.type || getBusinessObject(element)?.$type || '').trim();
  }

  /** 点到外部 label 时回退到其宿主 shape */
  function resolveSelectableElement(element: DiagramElement | null | undefined): DiagramElement | null {
    if (!element) return null;
    if (element.labelTarget) return element.labelTarget;
    if (getElementType(element) === 'label' && (element as { businessObject?: unknown }).businessObject == null) {
      return element.labelTarget || null;
    }
    return element;
  }

  function isUserTaskElement(element: DiagramElement | null | undefined): boolean {
    const type = getElementType(element);
    return type === 'bpmn:UserTask' || type === 'bpmn:Task';
  }

  /** 是否应按「表单任务」打开右侧编辑（含自定义 palette 节点） */
  function isFormTaskElement(element: DiagramElement | null | undefined): boolean {
    // 自定义节点本质是 UserTask：一律允许在右侧编辑
    return Boolean(element?.id && isUserTaskElement(element));
  }

  function selectDiagramElement(
    element: DiagramElement | null,
    options?: { forceFormConfig?: boolean; preferredName?: string; presetNodeType?: BusinessNodeType | string },
  ): void {
    const resolved = resolveSelectableElement(element);
    if (!resolved?.id || !isUserTaskElement(resolved)) {
      selectedTaskId.value = null;
      selectedTaskName.value = '';
      return;
    }
    const bo = getBusinessObject(resolved);
    const taskName = String(
      options?.preferredName
      || bo?.name
      || resolved.businessNodePreset?.defaultName
      || resolved.id
      || '',
    );
    selectedTaskId.value = resolved.id;
    selectedTaskName.value = taskName;

    const presetType = String(
      options?.presetNodeType
      || resolved.businessNodePreset?.nodeType
      || nodeRules.value[resolved.id]?.nodeType
      || '',
    ).trim() as BusinessNodeType | '';

    let nodeType: BusinessNodeType = presetType && presetType !== 'none'
      ? presetType
      : guessNodeTypeFromName(taskName, resolved.id);

    if (nodeType === 'none' && formTaskBindings.value[resolved.id]) {
      nodeType = 'form_task';
    }

    if (nodeType === 'notification') {
      upsertNodeRule(resolved.id, {
        nodeType: 'notification',
        notificationRule: nodeRules.value[resolved.id]?.notificationRule || defaultNotificationRule(),
      });
      void fetchReferenceDepartments();
      return;
    }

    if (nodeType === 'wait_receipts') {
      upsertNodeRule(resolved.id, { nodeType: 'wait_receipts' });
      return;
    }

    if (nodeType === 'dispatch_task' || nodeType === 'business_case_action') {
      upsertNodeRule(resolved.id, { nodeType });
      // 这两类节点暂以基础信息编辑为主
    }

    // 表单任务 / 其它 UserTask：建立表单绑定编辑态
    if (options?.forceFormConfig || nodeType === 'form_task' || nodeType === 'none') {
      if (nodeType === 'form_task' || nodeType === 'none') {
        upsertNodeRule(resolved.id, { nodeType: 'form_task' });
      }
      ensureFormTaskConfig(resolved.id, taskName);
    }
  }

  function ensureFormTaskConfig(taskId: string, taskName: string): void {
    if (!taskId) return;
    const existing = formTaskBindings.value[taskId];
    formTaskBindings.value = {
      ...formTaskBindings.value,
      [taskId]: existing
        ? normalizeFormTaskConfig(existing, { taskId, taskName })
        : createDefaultFormTaskConfig({ taskId, taskName }),
    };
  }

  function syncSelectedTaskNameToCanvas(name: string): void {
    const taskId = selectedTaskId.value;
    if (!taskId || !modeler.value) return;
    try {
      const elementRegistry = modeler.value.get('elementRegistry') as {
        get?: (id: string) => DiagramElement | undefined;
      } | undefined;
      const modeling = modeler.value.get('modeling') as {
        updateProperties?: (el: unknown, props: Record<string, unknown>) => void;
      } | undefined;
      const element = elementRegistry?.get?.(taskId);
      if (element && modeling?.updateProperties) {
        modeling.updateProperties(element, { name });
      }
    } catch {
      // ignore canvas sync errors
    }
  }

  function updateSelectedTaskName(name: string) {
    selectedTaskName.value = name;
    const taskId = selectedTaskId.value;
    if (!taskId) return;
    if (selectedNodeType.value === 'form_task' || formTaskBindings.value[taskId]) {
      ensureFormTaskConfig(taskId, name);
      formTaskBindings.value = {
        ...formTaskBindings.value,
        [taskId]: normalizeFormTaskConfig(
          { ...formTaskBindings.value[taskId], title: name },
          { taskId, taskName: name },
        ),
      };
    }
    syncSelectedTaskNameToCanvas(name);
  }

  function updateSelectedTaskConfig(partial: Partial<FormTaskBindingConfig>) {
    const taskId = selectedTaskId.value;
    if (!taskId) return;
    ensureFormTaskConfig(taskId, selectedTaskName.value || taskId);
    const next = normalizeFormTaskConfig(
      { ...formTaskBindings.value[taskId], ...partial },
      { taskId, taskName: selectedTaskName.value || taskId },
    );
    formTaskBindings.value = {
      ...formTaskBindings.value,
      [taskId]: next,
    };
    // 标题变更同步到画布
    if (partial.title !== undefined && partial.title !== selectedTaskName.value) {
      selectedTaskName.value = next.title;
      syncSelectedTaskNameToCanvas(next.title);
    }
  }

  function generateEmptyBpmn(processKey: string, processName: string): string {
    return `<?xml version="1.0" encoding="UTF-8"?><bpmn:definitions xmlns:bpmn="http://www.omg.org/spec/BPMN/20100524/MODEL" xmlns:bpmndi="http://www.omg.org/spec/BPMN/20100524/DI" xmlns:dc="http://www.omg.org/spec/DD/20100524/DC" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" targetNamespace="http://bpmn.io/schema/bpmn"><bpmn:process id="${sanitizeIdentifier(processKey)}" name="${processName}" isExecutable="true"><bpmn:startEvent id="StartEvent_1" name="开始"/></bpmn:process><bpmndi:BPMNDiagram id="BPMNDiagram_1"><bpmndi:BPMNPlane id="BPMNPlane_1" bpmnElement="${sanitizeIdentifier(processKey)}"><bpmndi:BPMNShape id="_BPMNShape_StartEvent_1" bpmnElement="StartEvent_1"><dc:Bounds x="173" y="102" width="36" height="36"/></bpmndi:BPMNShape></bpmndi:BPMNPlane></bpmndi:BPMNDiagram></bpmn:definitions>`;
  }

  async function loadBpmnDiagram(xml: string): Promise<void> {
    const ready = await ensureModelerReady(true);
    if (!ready || !modeler.value) {
      toast.showToast('warning', '建模画布尚未就绪，请重选事项类型。', { duration: 4000 });
      return;
    }
    try {
      const renderable = ensureRenderableBpmnXml(xml, diagramCode.value || 'process', diagramName.value || 'Process');
      await modeler.value.importXML(renderable);
      // 容器尺寸就绪后再 fit，否则会出现空白画布
      await waitFrames(1);
      resizeAndFitViewport();
    } catch (err) {
      console.warn('BPMN import failed:', err);
      toast.showToast('warning', '流程图加载失败，部分节点可能不兼容。', { duration: 4000 });
    }
  }

  async function selectCaseType(idOrAction: string) {
    if (idOrAction === 'new') {
      openCreateCaseModal();
      return;
    }
    selectedCaseId.value = idOrAction; isLoading.value = true;
    try {
      // 列表已含数据时优先本地；单条 GET 路由当前未开放，回落重新拉列表
      let existing = getSelectedCaseItem();
      if (!existing) {
        await fetchCaseTypes();
        existing = getSelectedCaseItem();
      }
      if (!existing) {
        toast.showToast('error', '未找到该事项类型', { duration: 4000 });
        return;
      }
      diagramName.value = existing.name; diagramCode.value = existing.code; caseDescription.value = existing.description || '';
      hasSelectedDiagram.value = true;
      const xml = existing.bpmn_xml || existing.xml_data || generateEmptyBpmn(existing.code, existing.name);
      importedXml.value = xml;
      applyBindingsAndRulesFromXml(xml);
      const config = existing.ai_extraction_config;
      if (config) {
        aiConfig.value = normalizeAiExtractionConfig(config);
        aiConfigAliasesText.value = readStringArray(config.aliases).join(', ');
        aiConfigTriggerText.value = readStringArray(config.triggers).join(', ');
        aiConfigForbiddenText.value = aiConfig.value.forbidden_fields.join(', ');
        aiConfigFieldsJsonText.value = JSON.stringify(aiConfig.value.extraction_fields, null, 2);
        aiConfigFieldsJsonError.value = '';
      } else {
        aiConfig.value = createDefaultAiExtractionConfig();
        aiConfigAliasesText.value = ''; aiConfigTriggerText.value = ''; aiConfigForbiddenText.value = ''; aiConfigFieldsJsonText.value = ''; aiConfigFieldsJsonError.value = '';
      }
      const props = existing.case_properties;
      caseProperties.value = props ? normalizeCaseProperties(props) : createDefaultCaseProperties();
      casePropertiesFieldsJsonText.value = Object.keys(caseProperties.value.extra_info_schema.fields).length > 0 ? JSON.stringify(caseProperties.value.extra_info_schema.fields, null, 2) : '';
      casePropertiesFieldsJsonError.value = '';
      duplicatePolicyFieldsText.value = caseProperties.value.duplicate_policy.fields.join(', ');
      duplicatePolicyStatusesText.value = caseProperties.value.duplicate_policy.active_statuses.join(', ');
      // 销毁旧实例，强制按新 DOM 重新挂载
      destroyModeler();
      await loadBpmnDiagram(xml);
    } catch (err) { console.warn('selectCaseType error:', err); toast.showToast('error', `流程数据加载失败: ${err instanceof Error ? err.message : String(err)}`, { duration: 5000 }); }
    finally { isLoading.value = false; }
  }

  function getActiveCaseCode(): string {
    return String(diagramCode.value || getSelectedCaseItem()?.code || '').trim();
  }

  function parseJsonObjectField(raw: string, errorRef: { value: string }, label: string): Record<string, unknown> | null {
    const text = raw.trim();
    if (!text) {
      errorRef.value = '';
      return {};
    }
    try {
      const parsed = JSON.parse(text) as unknown;
      if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
        errorRef.value = `${label} 必须是 JSON 对象`;
        return null;
      }
      errorRef.value = '';
      return parsed as Record<string, unknown>;
    } catch {
      errorRef.value = `${label} JSON 格式无效`;
      return null;
    }
  }

  function buildAiExtractionPayload(): Record<string, unknown> | null {
    const fields = parseJsonObjectField(aiConfigFieldsJsonText.value, aiConfigFieldsJsonError, '抽取字段定义');
    if (fields === null) return null;
    const aliases = aiConfigAliasesText.value.split(',').map((s) => s.trim()).filter(Boolean);
    const triggers = aiConfigTriggerText.value.split(',').map((s) => s.trim()).filter(Boolean);
    const forbidden = aiConfigForbiddenText.value.split(',').map((s) => s.trim()).filter(Boolean);
    return {
      ...aiConfig.value,
      aliases,
      triggers,
      forbidden_fields: forbidden,
      extraction_fields: fields,
    };
  }

  function buildCasePropertiesPayload(): CaseProperties | null {
    const fields = parseJsonObjectField(casePropertiesFieldsJsonText.value, casePropertiesFieldsJsonError, '额外信息字段定义');
    if (fields === null) return null;
    return {
      ...caseProperties.value,
      extra_info_schema: {
        ...caseProperties.value.extra_info_schema,
        fields,
      },
      duplicate_policy: {
        ...caseProperties.value.duplicate_policy,
        fields: duplicatePolicyFieldsText.value.split(',').map((s) => s.trim()).filter(Boolean),
        active_statuses: duplicatePolicyStatusesText.value.split(',').map((s) => s.trim()).filter(Boolean),
      },
    };
  }

  async function fetchCaseTypes(): Promise<void> {
    caseTypeLoadError.value = '';
    // 与原页一致：列出全部（含未配置 BPMN 的新类型）
    const requestPaths = [
      `/api/v2/business-case-types${buildQuery({ active_only: 'false', tenant_id: activeTenantId.value })}`,
      '/api/v2/business-case-types?active_only=false',
      '/api/v2/business-case-types',
    ];
    try {
      for (const path of requestPaths) {
        const response = await api.get<unknown>(path);
        if (!response.ok || !response.data) continue;
        const items = unwrapPayload<unknown>(response.data);
        const rawList = Array.isArray(items) ? items : (items && typeof items === 'object' ? ((items as Record<string, unknown>).items ?? (items as Record<string, unknown>).records ?? (items as Record<string, unknown>).list) : null);
        if (!Array.isArray(rawList)) continue;
        eventList.value = rawList.map((item) => normalizeCaseType(item)).filter((item): item is CaseTypeItem => Boolean(item));
        handleSearch(); return;
      }
    } catch (error) {
      console.warn('Failed to fetch case types:', error);
      caseTypeLoadError.value = error instanceof Error ? error.message : '事项类型加载失败';
    }
    if (!caseTypeLoadError.value) caseTypeLoadError.value = '事项类型服务暂不可用';
    eventList.value = []; handleSearch();
  }

  function switchScope(scope: ScopeMode) { currentScope.value = scope; resetDiagramSelection(); fetchCaseTypes(); }

  async function downloadBpmn() {
    if (!modeler.value) return;
    try {
      const { xml } = await modeler.value.saveXML({ format: true });
      downloadTextFile({ content: xml, filename: `${diagramCode.value || 'process'}.bpmn`, mimeType: 'application/xml' });
    } catch (err) { console.warn('Download failed:', err); toast.showToast('error', '下载失败', { duration: 3000 }); }
  }

  async function generateDraft() {
    if (!selectedCaseId.value) return;
    isLoading.value = true;
    try {
      const res = await api.post<unknown>('/api/v2/ai/tools/execute', { tool_name: 'generate_flowable_draft', tool_args: { case_type_id: selectedCaseId.value } });
      if (res.ok && res.data) {
        const payload = unwrapPayload<Record<string, unknown>>(res.data);
        if (payload) {
          const xml = cleanBpmnXml(String(payload.bpmn_xml || payload.xml || ''));
          if (xml) {
            importedXml.value = xml;
            applyBindingsAndRulesFromXml(xml);
            await loadBpmnDiagram(xml);
            toast.showToast('success', '草案已生成', { duration: 3000 });
            return;
          }
        }
      }
      toast.showToast('warning', 'AI 暂未生成有效草案，请检查后端配置或稍后重试。', { duration: 5000 });
    } catch (err) { console.warn('Generate draft failed:', err); toast.showToast('error', `草案生成失败: ${err instanceof Error ? err.message : String(err)}`, { duration: 5000 }); }
    finally { isLoading.value = false; }
  }

  async function deployDiagram() {
    const code = getActiveCaseCode();
    if (!modeler.value || !code) return;
    isLoading.value = true;
    try {
      const { xml } = await modeler.value.saveXML({ format: true });
      const bound = decorateBpmnXmlForSave(xml);
      // 先落库，再部署到 Flowable（对齐原页面「保存 + 发布」能力）
      const saveRes = await api.put<unknown>(
        `/api/v2/business-case-types/${encodeURIComponent(code)}/bpmn`,
        { bpmn_xml: bound, description: caseDescription.value || undefined, tenant_id: activeTenantId.value },
      );
      if (!saveRes.ok) {
        toast.showToast('error', readErrorMessage(saveRes.data, '部署前保存失败'), { duration: 5000 });
        return;
      }
      const deployRes = await api.post<unknown>('/api/v2/workflows/deployments', {
        bpmn_xml: bound,
        deployment_name: code,
        tenant_id: activeTenantId.value,
      });
      if (deployRes.ok) {
        toast.showToast('success', '流程部署成功', { duration: 3000 });
        await fetchCaseTypes();
      } else {
        toast.showToast('error', readErrorMessage(deployRes.data, '部署失败'), { duration: 5000 });
      }
    } catch (err) {
      console.warn('Deploy failed:', err);
      toast.showToast('error', `部署失败: ${err instanceof Error ? err.message : String(err)}`, { duration: 5000 });
    } finally {
      isLoading.value = false;
    }
  }

  async function saveConfig() {
    const code = getActiveCaseCode();
    if (!modeler.value || !code) return;
    isLoading.value = true;
    try {
      const { xml } = await modeler.value.saveXML({ format: true });
      const bound = decorateBpmnXmlForSave(xml);
      const res = await api.put<unknown>(
        `/api/v2/business-case-types/${encodeURIComponent(code)}/bpmn`,
        { bpmn_xml: bound, description: caseDescription.value || undefined, tenant_id: activeTenantId.value },
      );
      if (res.ok) {
        importedXml.value = bound;
        toast.showToast('success', '流程已保存到数据库', { duration: 3000 });
        await fetchCaseTypes();
      } else {
        toast.showToast('error', readErrorMessage(res.data, '保存失败'), { duration: 5000 });
      }
    } catch (err) {
      console.warn('Save config failed:', err);
      toast.showToast('error', `保存失败: ${err instanceof Error ? err.message : String(err)}`, { duration: 5000 });
    } finally {
      isLoading.value = false;
    }
  }

  async function saveAiConfig() {
    const code = getActiveCaseCode();
    if (!code) {
      toast.showToast('warning', '请先选择事项类型', { duration: 3000 });
      return;
    }
    const payload = buildAiExtractionPayload();
    if (!payload) {
      toast.showToast('error', aiConfigFieldsJsonError.value || 'AI 配置无效', { duration: 4000 });
      return;
    }
    isLoading.value = true;
    try {
      const res = await api.put<unknown>(
        `/api/v2/business-case-types/${encodeURIComponent(code)}/ai-extraction-config`,
        { ai_extraction_config: payload },
      );
      if (res.ok) toast.showToast('success', 'AI 抽取配置已保存', { duration: 3000 });
      else toast.showToast('error', readErrorMessage(res.data, 'AI 配置保存失败'), { duration: 5000 });
    } catch (err) {
      toast.showToast('error', `AI 配置保存失败: ${err instanceof Error ? err.message : String(err)}`, { duration: 5000 });
    } finally {
      isLoading.value = false;
    }
  }

  async function saveCaseProperties() {
    const code = getActiveCaseCode();
    if (!code) {
      toast.showToast('warning', '请先选择事项类型', { duration: 3000 });
      return;
    }
    const payload = buildCasePropertiesPayload();
    if (!payload) {
      toast.showToast('error', casePropertiesFieldsJsonError.value || '业务规则无效', { duration: 4000 });
      return;
    }
    isLoading.value = true;
    try {
      const res = await api.put<unknown>(
        `/api/v2/business-case-types/${encodeURIComponent(code)}/case-properties`,
        { case_properties: payload },
      );
      if (res.ok) toast.showToast('success', '业务规则属性已保存', { duration: 3000 });
      else toast.showToast('error', readErrorMessage(res.data, '业务规则保存失败'), { duration: 5000 });
    } catch (err) {
      toast.showToast('error', `业务规则保存失败: ${err instanceof Error ? err.message : String(err)}`, { duration: 5000 });
    } finally {
      isLoading.value = false;
    }
  }

  return {
    api,
    canvasRef, modeler, connectionStatus, userName, userRole, userAvatar,
    departmentTenant, departmentLabel, currentScope,
    eventList, searchQuery, filteredEventList, caseTypeLoadError,
    selectedCaseId, diagramName, diagramCode, caseDescription,
    isLoading, hasSelectedDiagram, selectedTaskId, selectedTaskName,
    formTaskBindings, importedXml, showAiChat, contextVariables,
    hasDepartmentScope, activeTenantId, activeScopeLabel, activeTenantLabel,
    selectedFormTaskConfig, selectedTaskRolesText, persistedFormTaskCount,
    selectedNodeType, selectedNotificationRule, nodeRules,
    referenceDepartments, referenceDepartmentsLoading, referenceDepartmentsError,
    showCreateCaseModal, createCaseCode, createCaseName, createCaseScope,
    createCaseError, createCaseSubmitting, createCaseScopeHint,
    firstNonEmpty, buildQuery, handleSearch, switchScope,
    isAiConfigExpanded, isCasePropertiesExpanded, aiConfig,
    aiConfigAliasesText, aiConfigTriggerText, aiConfigForbiddenText,
    aiConfigFieldsJsonText, aiConfigFieldsJsonError,
    caseProperties, casePropertiesFieldsJsonText, casePropertiesFieldsJsonError,
    duplicatePolicyFieldsText, duplicatePolicyStatusesText,
    createDefaultCaseProperties, normalizeCaseProperties, unwrapPayload,
    readErrorMessage, normalizeCaseType, resetDiagramSelection,
    applyUserContext, getSelectedCaseItem, initModeler, ensureModelerReady, isFormTaskElement,
    ensureFormTaskConfig, updateSelectedTaskName, updateSelectedTaskConfig,
    updateSelectedNotificationRule, toggleNotificationDepartment, toggleNotificationRole,
    fetchReferenceDepartments,
    generateEmptyBpmn, loadBpmnDiagram, selectCaseType,
    openCreateCaseModal, closeCreateCaseModal, submitCreateCase,
    fetchCaseTypes, downloadBpmn, generateDraft, deployDiagram, saveConfig,
    saveAiConfig, saveCaseProperties,
    toggleCaseTypeStatus, deprecateCaseType, restoreCaseType,
  };
}
