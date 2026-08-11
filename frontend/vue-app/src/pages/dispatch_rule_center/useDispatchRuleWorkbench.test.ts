// @vitest-environment jsdom
import { describe, expect, it, beforeEach, vi } from 'vitest';
import {
  useDispatchRuleWorkbench,
  ALL_DEPARTMENTS_AGGREGATE,
} from './useDispatchRuleWorkbench';
import type {
  DepartmentResponse,
  DepartmentRuleBundle,
  DispatchOrderCreateResponse,
  DispatchRulePreviewResponse,
  DispatchRuleValidationResponse,
  EquipmentTypeResponse,
  FlightGenerationRuleResponse,
  GenerationAdjustmentRuleResponse,
  RequirementVersionResponse,
  TaskTypeResponse,
} from './dispatchRuleWorkbenchApi';

function makeDept(id: string, name: string): DepartmentResponse {
  return {
    id,
    name,
    code: id.toUpperCase(),
    description: null,
    manager_id: null,
    terminal: 'T1',
    is_active: true,
  };
}

function makeTaskType(code: string, name: string, dep?: string | null): TaskTypeResponse {
  return {
    id: `task-${code}`,
    code,
    name,
    default_department_id: dep ?? null,
    category: null,
    sequence_order: 1,
    default_duration_minutes: 30,
    trigger_offset_minutes: 30,
    trigger_type: 'before_eta',
    description: null,
    is_active: true,
  };
}

function makeBundle(overrides: Partial<DepartmentRuleBundle> = {}): DepartmentRuleBundle {
  return {
    qualifications: [],
    qualificationLevels: [],
    qualificationGrants: [],
    flightGenerationRules: [],
    generationAdjustmentRules: [],
    requirementVersions: [],
    temporaryTaskTemplates: [],
    ...overrides,
  };
}

function makeFlightGenRule(id: string, taskType = 'TOWING'): FlightGenerationRuleResponse {
  return {
    id,
    department_id: 'dep-1',
    task_type: taskType,
    leg_scope: 'outbound',
    version_no: 1,
    status: 'draft',
    rule_name: `规则 ${id}`,
    conditions: {},
    generation_anchor_type: 'scheduled_time',
    start_offset_minutes: 0,
    completion_time_mode: 'start_plus_duration',
    completion_anchor_type: null,
    completion_offset_minutes: null,
    duration_minutes: 30,
    start_flex_minutes: null,
    completion_warning_lead_minutes: null,
    duration_by_crew_size: null,
    publication_state: 'prepublished',
    publish_trigger_mode: 'time',
    publish_at: null,
    publish_offset_minutes: null,
    publish_event_code: null,
    notes: null,
    published_at: null,
    created_at: null,
    updated_at: null,
  };
}

function makeAdjRule(id: string, taskType = 'TOWING'): GenerationAdjustmentRuleResponse {
  return {
    id,
    department_id: 'dep-1',
    task_type: taskType,
    version_no: 1,
    status: 'draft',
    rule_name: `调整 ${id}`,
    conditions: {},
    actions: [],
    notes: null,
    published_at: null,
    created_at: null,
    updated_at: null,
  };
}

function makeRequirementVersion(id: string, taskType = 'TOWING', status = 'draft'): RequirementVersionResponse {
  return {
    id,
    department_id: 'dep-1',
    task_type: taskType,
    version_no: 1,
    status,
    requirements: [],
    crew_requirements: [],
    equipment_requirements: [],
    turnaround_continuity_rules: [],
    notes: null,
    published_at: status === 'published' ? new Date().toISOString() : null,
    created_at: null,
    updated_at: null,
  };
}

function buildMockApi() {
  const departments: DepartmentResponse[] = [
    makeDept('dep-1', '运行中心'),
    makeDept('dep-2', '客舱保障'),
  ];
  const taskTypes: TaskTypeResponse[] = [
    makeTaskType('TOWING', '拖飞机', 'dep-1'),
    makeTaskType('CLEANING', '客舱清洁', 'dep-2'),
  ];
  const equipmentTypes: EquipmentTypeResponse[] = [
    {
      id: 'eq-tug',
      name: '飞机牵引车',
      code: 'TUG',
      category: 'tow',
      requires_driver: true,
      is_active: true,
    },
  ];
  const bundleByDep: Record<string, DepartmentRuleBundle> = {
    'dep-1': makeBundle({
      flightGenerationRules: [makeFlightGenRule('fgr-1')],
      generationAdjustmentRules: [makeAdjRule('adj-1')],
      requirementVersions: [makeRequirementVersion('req-1', 'TOWING', 'published')],
    }),
    'dep-2': makeBundle(),
  };

  const calls: { method: string; url: string; body?: unknown }[] = [];

  const api = {
    listDepartments: vi.fn(async () => {
      calls.push({ method: 'GET', url: '/departments' });
      return departments;
    }),
    listTaskTypes: vi.fn(async () => {
      calls.push({ method: 'GET', url: '/task-types' });
      return taskTypes;
    }),
    createTaskType: vi.fn(async (payload) => {
      calls.push({ method: 'POST', url: '/task-types', body: payload });
      const created = makeTaskType(payload.code, payload.name, payload.default_department_id ?? null);
      taskTypes.push(created);
      return created;
    }),
    deleteTaskType: vi.fn(async (code: string) => {
      calls.push({ method: 'DELETE', url: `/task-types/${code}` });
      const idx = taskTypes.findIndex((t) => t.code === code);
      if (idx >= 0) taskTypes.splice(idx, 1);
    }),
    listEquipmentTypes: vi.fn(async () => {
      calls.push({ method: 'GET', url: '/equipment-types' });
      return equipmentTypes;
    }),
    fetchDepartmentBundle: vi.fn(async (departmentId: string) => {
      calls.push({ method: 'GET', url: `/rules/${departmentId}/bundle` });
      return bundleByDep[departmentId] ?? makeBundle();
    }),
    saveFlightGenerationRule: vi.fn(async (departmentId: string, payload) => {
      calls.push({ method: 'POST', url: `/rules/${departmentId}/flight-generation-rules`, body: payload });
      const saved: FlightGenerationRuleResponse = {
        ...makeFlightGenRule(payload.rule_id ?? `fgr-${Date.now()}`, payload.task_type),
        rule_name: payload.rule_name ?? null,
      };
      return saved;
    }),
    deleteFlightGenerationRule: vi.fn(async (departmentId: string, ruleId: string) => {
      calls.push({ method: 'POST', url: `/rules/${departmentId}/flight-generation-rules/${ruleId}/delete` });
    }),
    saveGenerationAdjustmentRule: vi.fn(async (departmentId: string, payload) => {
      calls.push({ method: 'POST', url: `/rules/${departmentId}/generation-adjustment-rules`, body: payload });
      return makeAdjRule(payload.rule_id ?? `adj-${Date.now()}`, payload.task_type);
    }),
    saveRequirementDraft: vi.fn(async (departmentId: string, payload) => {
      calls.push({ method: 'POST', url: `/rules/${departmentId}/task-type-requirements/drafts`, body: payload });
      return makeRequirementVersion(`draft-${Date.now()}`, payload.task_type, 'draft');
    }),
    publishRequirement: vi.fn(async (departmentId: string, payload) => {
      calls.push({ method: 'POST', url: `/rules/${departmentId}/task-type-requirements/publish`, body: payload });
      return {
        published_version: makeRequirementVersion(
          payload.draft_id ?? `pub-${Date.now()}`,
          payload.task_type,
          'published',
        ),
      };
    }),
    validateRules: vi.fn(async (payload): Promise<DispatchRuleValidationResponse> => {
      calls.push({ method: 'POST', url: '/rules/validate', body: payload });
      return { valid: true, conflicts: [], messages: [] };
    }),
    previewRules: vi.fn(async (payload): Promise<DispatchRulePreviewResponse> => {
      calls.push({ method: 'POST', url: '/rules/preview', body: payload });
      return {
        generated_orders: [{ id: 'preview-order-1' }],
        applied_adjustments: [],
        turnaround_constraints: [],
        conflicts: [],
        blocking_errors: [],
      };
    }),
    createDispatchOrder: vi.fn(async (payload): Promise<DispatchOrderCreateResponse> => {
      calls.push({ method: 'POST', url: '/dispatch-orders', body: payload });
      return { id: 'order-1', status: 'created' };
    }),
    api: {} as never,
  };

  return { api, calls, departments, taskTypes, bundleByDep, equipmentTypes };
}

describe('useDispatchRuleWorkbench', () => {
  let mock = buildMockApi();
  let workbench: ReturnType<typeof useDispatchRuleWorkbench>;

  beforeEach(() => {
    mock = buildMockApi();
    workbench = useDispatchRuleWorkbench({ api: mock.api as never });
  });

  it('bootstraps departments, task types, equipment types and selects first department', async () => {
    await workbench.bootstrap();

    expect(mock.api.listDepartments).toHaveBeenCalledOnce();
    expect(mock.api.listTaskTypes).toHaveBeenCalledOnce();
    expect(mock.api.listEquipmentTypes).toHaveBeenCalledOnce();
    expect(workbench.departments.value.map((d) => d.id)).toEqual(['dep-1', 'dep-2']);
    expect(workbench.taskTypes.value).toHaveLength(2);
    expect(workbench.equipmentTypes.value).toHaveLength(1);
    expect(workbench.selectedDepartmentId.value).toBe('dep-1');
    expect(mock.api.fetchDepartmentBundle).toHaveBeenCalledWith('dep-1');
    expect(workbench.bundle.value.flightGenerationRules).toHaveLength(1);
  });

  it('switching to aggregate view clears bundle and disables department-scoped operations', async () => {
    await workbench.bootstrap();
    await workbench.selectDepartment(ALL_DEPARTMENTS_AGGREGATE);

    expect(workbench.isAggregateView.value).toBe(true);
    expect(workbench.bundle.value.flightGenerationRules).toHaveLength(0);
    await expect(
      workbench.saveGenerationRule({ task_type: 'TOWING', leg_scope: 'outbound' }),
    ).rejects.toThrow(/请先选择/);
  });

  it('selectDepartment loads department bundle', async () => {
    await workbench.bootstrap();
    await workbench.selectDepartment('dep-2');

    expect(workbench.selectedDepartmentId.value).toBe('dep-2');
    expect(mock.api.fetchDepartmentBundle).toHaveBeenLastCalledWith('dep-2');
    expect(workbench.bundle.value.flightGenerationRules).toHaveLength(0);
  });

  it('saveGenerationRule posts to department-scoped endpoint and clears dirty flag', async () => {
    await workbench.bootstrap();
    workbench.markDirty('generationDraft', true);
    expect(workbench.dirtyState.generationDraft).toBe(true);

    const saved = await workbench.saveGenerationRule({
      rule_name: '新规则',
      task_type: 'TOWING',
      leg_scope: 'outbound',
    });

    expect(saved.id).toBeDefined();
    expect(mock.api.saveFlightGenerationRule).toHaveBeenCalledWith(
      'dep-1',
      expect.objectContaining({ task_type: 'TOWING', leg_scope: 'outbound' }),
    );
    expect(workbench.dirtyState.generationDraft).toBe(false);
  });

  it('saveAdjustmentRule routes through department endpoint', async () => {
    await workbench.bootstrap();
    await workbench.saveAdjustmentRule({
      task_type: 'TOWING',
      conditions: {},
      actions: [],
    });
    expect(mock.api.saveGenerationAdjustmentRule).toHaveBeenCalledWith(
      'dep-1',
      expect.objectContaining({ task_type: 'TOWING' }),
    );
  });

  it('saveRequirementDraft and publishRequirementDraft both target department endpoints', async () => {
    await workbench.bootstrap();
    const draft = await workbench.saveRequirementDraft({
      task_type: 'TOWING',
      crew_requirements: [],
      equipment_requirements: [],
    });
    expect(mock.api.saveRequirementDraft).toHaveBeenCalledWith(
      'dep-1',
      expect.objectContaining({ task_type: 'TOWING' }),
    );
    expect(draft.status).toBe('draft');

    const published = await workbench.publishRequirementDraft({
      task_type: 'TOWING',
      draft_id: draft.id,
    });
    expect(mock.api.publishRequirement).toHaveBeenCalledWith(
      'dep-1',
      expect.objectContaining({ task_type: 'TOWING', draft_id: draft.id }),
    );
    expect(published.status).toBe('published');
  });

  it('runPreview calls /api/v2/dispatch/rules/preview and stores result', async () => {
    await workbench.bootstrap();
    const result = await workbench.runPreview({
      flight_id: 'CA1234',
      sample_flight: { task_type: 'TOWING' },
    });
    expect(mock.api.previewRules).toHaveBeenCalledWith(
      expect.objectContaining({ flight_id: 'CA1234' }),
    );
    expect(result.generated_orders).toHaveLength(1);
    expect(workbench.previewResult.value).toStrictEqual(result);
  });

  it('createManualOrder requires task_type and posts to /api/v2/dispatch-orders', async () => {
    await workbench.bootstrap();
    await expect(workbench.createManualOrder()).rejects.toThrow(/任务类型/);

    workbench.manualOrderDraft.value = {
      ...workbench.manualOrderDraft.value,
      task_type: 'TOWING',
      department_id: 'dep-1',
      remarks: '人工补单',
    };
    workbench.markDirty('manualOrderDraft', true);
    const result = await workbench.createManualOrder();

    expect(mock.api.createDispatchOrder).toHaveBeenCalledWith(
      expect.objectContaining({ task_type: 'TOWING', department_id: 'dep-1', remarks: '人工补单' }),
    );
    expect(result.id).toBe('order-1');
    expect(workbench.lastCreatedOrder.value?.id).toBe('order-1');
    expect(workbench.dirtyState.manualOrderDraft).toBe(false);
  });

  it('exportSnapshotJson and copySnapshotJson produce JSON containing bundle and bookkeeping', async () => {
    await workbench.bootstrap();
    const writeText = vi.fn(async () => undefined);
    Object.assign(navigator, { clipboard: { writeText } });

    const exported = await workbench.exportSnapshotJson();
    expect(exported).toContain('"department_id"');
    expect(exported).toContain('"flightGenerationRules"');
    expect(workbench.lastSnapshotAt.value).not.toBeNull();

    const copied = await workbench.copySnapshotJson();
    expect(writeText).toHaveBeenCalledWith(copied);
  });

  it('createTaskType and deleteTaskType update the task type list', async () => {
    await workbench.bootstrap();
    await workbench.createTaskType({ code: 'WATER', name: '加水' });
    expect(workbench.taskTypes.value.find((t) => t.code === 'WATER')).toBeDefined();
    await workbench.deleteTaskType('WATER');
    expect(workbench.taskTypes.value.find((t) => t.code === 'WATER')).toBeUndefined();
  });
});
