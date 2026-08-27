import { computed, reactive, ref } from 'vue';
import { downloadTextFile } from '@/lib/download';
import {
  createWorkbenchApi,
  type DepartmentResponse,
  type DepartmentRuleBundle,
  type DispatchOrderCreatePayload,
  type DispatchOrderCreateResponse,
  type DispatchRulePreviewRequest,
  type DispatchRulePreviewResponse,
  type DispatchRuleValidationResponse,
  type EquipmentTypeResponse,
  type FlightGenerationRulePayload,
  type FlightGenerationRuleResponse,
  type GenerationAdjustmentRulePayload,
  type GenerationAdjustmentRuleResponse,
  type RequirementDraftPayload,
  type RequirementPublishPayload,
  type RequirementVersionResponse,
  type TaskTypeCreatePayload,
  type TaskTypeResponse,
  type WorkbenchApi,
} from './dispatchRuleWorkbenchApi';

export type WorkbenchPanel =
  | 'generation'
  | 'adjustment'
  | 'requirement'
  | 'preview'
  | 'manualOrder';

export const ALL_DEPARTMENTS_AGGREGATE = '__all__';

export interface DirtyFlags {
  generationDraft: boolean;
  adjustmentDraft: boolean;
  requirementDraft: boolean;
  manualOrderDraft: boolean;
}

export interface ManualOrderDraft {
  flight_id: string;
  task_type: string;
  department_id: string;
  individual_user_id: string;
  stand_id: string;
  location: string;
  planned_start_time: string;
  planned_end_time: string;
  priority: number;
  publication_state: string;
  source_type: string;
  leg_scope: string;
  manual_lock: boolean;
  remarks: string;
}

function emptyBundle(): DepartmentRuleBundle {
  return {
    qualifications: [],
    qualificationLevels: [],
    qualificationGrants: [],
    flightGenerationRules: [],
    generationAdjustmentRules: [],
    requirementVersions: [],
    temporaryTaskTemplates: [],
  };
}

function emptyManualOrderDraft(departmentId = ''): ManualOrderDraft {
  return {
    flight_id: '',
    task_type: '',
    department_id: departmentId,
    individual_user_id: '',
    stand_id: '',
    location: '',
    planned_start_time: '',
    planned_end_time: '',
    priority: 100,
    publication_state: 'prepublished',
    source_type: 'manual',
    leg_scope: 'outbound',
    manual_lock: false,
    remarks: '',
  };
}

export interface UseDispatchRuleWorkbenchOptions {
  api?: WorkbenchApi;
}

export function useDispatchRuleWorkbench(options: UseDispatchRuleWorkbenchOptions = {}) {
  const api = options.api ?? createWorkbenchApi();

  const departments = ref<DepartmentResponse[]>([]);
  const taskTypes = ref<TaskTypeResponse[]>([]);
  const equipmentTypes = ref<EquipmentTypeResponse[]>([]);
  const bundle = ref<DepartmentRuleBundle>(emptyBundle());
  const selectedDepartmentId = ref<string>('');
  const selectedTaskTypeCode = ref<string>('');
  const activePanel = ref<WorkbenchPanel>('generation');

  const loading = ref(false);
  const saving = ref(false);
  const bootstrapped = ref(false);
  const error = ref<string | null>(null);
  const lastSnapshotAt = ref<string | null>(null);

  const dirtyState = reactive<DirtyFlags>({
    generationDraft: false,
    adjustmentDraft: false,
    requirementDraft: false,
    manualOrderDraft: false,
  });

  const validationState = ref<DispatchRuleValidationResponse | null>(null);
  const previewResult = ref<DispatchRulePreviewResponse | null>(null);
  const lastCreatedOrder = ref<DispatchOrderCreateResponse | null>(null);
  const manualOrderDraft = ref<ManualOrderDraft>(emptyManualOrderDraft());

  const isAggregateView = computed(
    () => selectedDepartmentId.value === ALL_DEPARTMENTS_AGGREGATE || !selectedDepartmentId.value,
  );

  const selectedDepartment = computed<DepartmentResponse | null>(() => {
    if (isAggregateView.value) return null;
    return (
      departments.value.find((d) => d.id === selectedDepartmentId.value) ?? null
    );
  });

  const dirtyCount = computed(() =>
    Object.values(dirtyState).filter((flag) => flag === true).length,
  );

  const taskTypesForSelectedDepartment = computed(() => {
    if (!selectedDepartment.value) return taskTypes.value;
    const depId = selectedDepartment.value.id;
    return taskTypes.value.filter(
      (t) => !t.default_department_id || t.default_department_id === depId,
    );
  });

  function setError(message: string | null) {
    error.value = message;
  }

  async function bootstrap(): Promise<void> {
    loading.value = true;
    error.value = null;
    try {
      const [deps, types, equipTypes] = await Promise.all([
        api.listDepartments(),
        api.listTaskTypes(),
        api.listEquipmentTypes(),
      ]);
      departments.value = deps;
      taskTypes.value = types;
      equipmentTypes.value = equipTypes;
      bootstrapped.value = true;
      if (!selectedDepartmentId.value && deps.length > 0) {
        await selectDepartment(deps[0].id);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : '初始化工作台失败');
      throw e;
    } finally {
      loading.value = false;
    }
  }

  async function selectDepartment(departmentId: string): Promise<void> {
    selectedDepartmentId.value = departmentId;
    selectedTaskTypeCode.value = '';
    bundle.value = emptyBundle();
    manualOrderDraft.value = emptyManualOrderDraft(
      departmentId === ALL_DEPARTMENTS_AGGREGATE ? '' : departmentId,
    );
    resetDirty();
    if (departmentId === ALL_DEPARTMENTS_AGGREGATE || !departmentId) return;
    await refreshDepartmentBundle();
  }

  async function refreshDepartmentBundle(): Promise<void> {
    if (isAggregateView.value || !selectedDepartmentId.value) return;
    loading.value = true;
    error.value = null;
    try {
      bundle.value = await api.fetchDepartmentBundle(selectedDepartmentId.value);
    } catch (e) {
      setError(e instanceof Error ? e.message : '刷新部门规则失败');
      throw e;
    } finally {
      loading.value = false;
    }
  }

  function ensureDepartmentScoped(): string {
    if (isAggregateView.value || !selectedDepartmentId.value) {
      throw new Error('请先选择具体科室');
    }
    return selectedDepartmentId.value;
  }

  async function createTaskType(payload: TaskTypeCreatePayload): Promise<TaskTypeResponse> {
    saving.value = true;
    try {
      const created = await api.createTaskType(payload);
      taskTypes.value = [...taskTypes.value, created];
      return created;
    } catch (e) {
      const msg = e instanceof Error ? e.message : '创建任务类型失败';
      setError(msg);
      throw e;
    } finally {
      saving.value = false;
    }
  }

  async function deleteTaskType(code: string): Promise<void> {
    saving.value = true;
    try {
      await api.deleteTaskType(code);
      taskTypes.value = taskTypes.value.filter((t) => t.code !== code);
      if (selectedTaskTypeCode.value === code) {
        selectedTaskTypeCode.value = '';
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : '删除任务类型失败';
      setError(msg);
      throw e;
    } finally {
      saving.value = false;
    }
  }

  async function saveGenerationRule(
    payload: FlightGenerationRulePayload,
  ): Promise<FlightGenerationRuleResponse> {
    const departmentId = ensureDepartmentScoped();
    saving.value = true;
    try {
      const saved = await api.saveFlightGenerationRule(departmentId, payload);
      const existingIdx = bundle.value.flightGenerationRules.findIndex(
        (r) => r.id === saved.id,
      );
      const next = [...bundle.value.flightGenerationRules];
      if (existingIdx >= 0) next[existingIdx] = saved;
      else next.push(saved);
      bundle.value = { ...bundle.value, flightGenerationRules: next };
      dirtyState.generationDraft = false;
      return saved;
    } finally {
      saving.value = false;
    }
  }

  async function deleteGenerationRule(ruleId: string): Promise<void> {
    const departmentId = ensureDepartmentScoped();
    saving.value = true;
    try {
      await api.deleteFlightGenerationRule(departmentId, ruleId);
      bundle.value = {
        ...bundle.value,
        flightGenerationRules: bundle.value.flightGenerationRules.filter(
          (r) => r.id !== ruleId,
        ),
      };
    } finally {
      saving.value = false;
    }
  }

  async function saveAdjustmentRule(
    payload: GenerationAdjustmentRulePayload,
  ): Promise<GenerationAdjustmentRuleResponse> {
    const departmentId = ensureDepartmentScoped();
    saving.value = true;
    try {
      const saved = await api.saveGenerationAdjustmentRule(departmentId, payload);
      const existingIdx = bundle.value.generationAdjustmentRules.findIndex(
        (r) => r.id === saved.id,
      );
      const next = [...bundle.value.generationAdjustmentRules];
      if (existingIdx >= 0) next[existingIdx] = saved;
      else next.push(saved);
      bundle.value = { ...bundle.value, generationAdjustmentRules: next };
      dirtyState.adjustmentDraft = false;
      return saved;
    } finally {
      saving.value = false;
    }
  }

  async function saveRequirementDraft(
    payload: RequirementDraftPayload,
  ): Promise<RequirementVersionResponse> {
    const departmentId = ensureDepartmentScoped();
    saving.value = true;
    try {
      const saved = await api.saveRequirementDraft(departmentId, payload);
      const next = [...bundle.value.requirementVersions];
      const idx = next.findIndex((v) => v.id === saved.id);
      if (idx >= 0) next[idx] = saved;
      else next.unshift(saved);
      bundle.value = { ...bundle.value, requirementVersions: next };
      dirtyState.requirementDraft = false;
      return saved;
    } finally {
      saving.value = false;
    }
  }

  async function publishRequirementDraft(
    payload: RequirementPublishPayload,
  ): Promise<RequirementVersionResponse> {
    const departmentId = ensureDepartmentScoped();
    saving.value = true;
    try {
      const result = await api.publishRequirement(departmentId, payload);
      const published = result.published_version;
      const next = [...bundle.value.requirementVersions];
      const idx = next.findIndex((v) => v.id === published.id);
      if (idx >= 0) next[idx] = published;
      else next.unshift(published);
      bundle.value = { ...bundle.value, requirementVersions: next };
      return published;
    } finally {
      saving.value = false;
    }
  }

  async function runPreview(payload: DispatchRulePreviewRequest): Promise<DispatchRulePreviewResponse> {
    saving.value = true;
    previewResult.value = null;
    try {
      const result = await api.previewRules(payload);
      previewResult.value = result;
      return result;
    } finally {
      saving.value = false;
    }
  }

  async function validateRulesNow(payload: {
    generation_rule?: FlightGenerationRulePayload;
    adjustment_rule?: GenerationAdjustmentRulePayload;
  }): Promise<DispatchRuleValidationResponse> {
    saving.value = true;
    try {
      const result = await api.validateRules(payload);
      validationState.value = result;
      return result;
    } finally {
      saving.value = false;
    }
  }

  async function createManualOrder(): Promise<DispatchOrderCreateResponse> {
    const draft = manualOrderDraft.value;
    if (!draft.task_type) throw new Error('任务类型必填');
    if (!draft.department_id) draft.department_id = ensureDepartmentScoped();

    const payload: DispatchOrderCreatePayload = {
      flight_id: draft.flight_id || null,
      task_type: draft.task_type,
      department_id: draft.department_id || null,
      individual_user_id: draft.individual_user_id || null,
      stand_id: draft.stand_id || null,
      location: draft.location || null,
      planned_start_time: draft.planned_start_time
        ? new Date(draft.planned_start_time).toISOString()
        : null,
      planned_end_time: draft.planned_end_time
        ? new Date(draft.planned_end_time).toISOString()
        : null,
      priority: draft.priority,
      publication_state: draft.publication_state,
      source_type: draft.source_type,
      leg_scope: draft.leg_scope,
      manual_lock: draft.manual_lock,
      remarks: draft.remarks || null,
    };

    saving.value = true;
    try {
      const result = await api.createDispatchOrder(payload);
      lastCreatedOrder.value = result;
      manualOrderDraft.value = emptyManualOrderDraft(draft.department_id);
      dirtyState.manualOrderDraft = false;
      return result;
    } finally {
      saving.value = false;
    }
  }

  function buildSnapshot() {
    const department = selectedDepartment.value;
    return {
      exported_at: new Date().toISOString(),
      department_id: department?.id ?? null,
      department_name: department?.name ?? null,
      task_types: taskTypes.value,
      equipment_types: equipmentTypes.value,
      bundle: bundle.value,
    };
  }

  async function exportSnapshotJson(): Promise<string> {
    const snapshot = buildSnapshot();
    const text = JSON.stringify(snapshot, null, 2);
    const stamp = snapshot.exported_at.replace(/[:.]/g, '-');
    const dept = snapshot.department_id ?? 'all';
    downloadTextFile({
      content: text,
      filename: `dispatch-rules-${dept}-${stamp}.json`,
      mimeType: 'application/json;charset=utf-8',
    });
    lastSnapshotAt.value = snapshot.exported_at;
    return text;
  }

  async function copySnapshotJson(): Promise<string> {
    const snapshot = buildSnapshot();
    const text = JSON.stringify(snapshot, null, 2);
    if (typeof navigator === 'undefined' || !navigator.clipboard?.writeText) {
      throw new Error('当前环境不支持剪贴板写入');
    }
    await navigator.clipboard.writeText(text);
    lastSnapshotAt.value = snapshot.exported_at;
    return text;
  }

  function resetDirty(): void {
    dirtyState.generationDraft = false;
    dirtyState.adjustmentDraft = false;
    dirtyState.requirementDraft = false;
    dirtyState.manualOrderDraft = false;
  }

  function markDirty(panel: keyof DirtyFlags, value: boolean): void {
    dirtyState[panel] = value;
  }

  function selectTaskType(code: string): void {
    selectedTaskTypeCode.value = code;
  }

  function setActivePanel(panel: WorkbenchPanel): void {
    activePanel.value = panel;
  }

  return {
    // state
    departments,
    taskTypes,
    equipmentTypes,
    bundle,
    selectedDepartmentId,
    selectedDepartment,
    selectedTaskTypeCode,
    activePanel,
    loading,
    saving,
    bootstrapped,
    error,
    dirtyState,
    dirtyCount,
    validationState,
    previewResult,
    manualOrderDraft,
    lastCreatedOrder,
    lastSnapshotAt,
    isAggregateView,
    taskTypesForSelectedDepartment,
    // actions
    bootstrap,
    selectDepartment,
    refreshDepartmentBundle,
    createTaskType,
    deleteTaskType,
    saveGenerationRule,
    deleteGenerationRule,
    saveAdjustmentRule,
    saveRequirementDraft,
    publishRequirementDraft,
    runPreview,
    validateRulesNow,
    createManualOrder,
    exportSnapshotJson,
    copySnapshotJson,
    markDirty,
    resetDirty,
    selectTaskType,
    setActivePanel,
  };
}

export type DispatchRuleWorkbench = ReturnType<typeof useDispatchRuleWorkbench>;
