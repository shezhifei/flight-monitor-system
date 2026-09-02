import { useApi } from '@/composables/useApi';
import { describeApiError } from '@/composables/useApiError';

export interface DepartmentResponse {
  id: string;
  name: string;
  code?: string | null;
  description?: string | null;
  manager_id?: string | null;
  terminal?: string | null;
  is_active: boolean;
}

export interface TaskTypeResponse {
  id: string;
  code: string;
  name: string;
  default_department_id?: string | null;
  category?: string | null;
  sequence_order?: number | null;
  default_duration_minutes?: number | null;
  trigger_offset_minutes: number;
  trigger_type: string;
  description?: string | null;
  is_active: boolean;
  attributes?: Record<string, unknown>;
}

export interface TaskTypeCreatePayload {
  code: string;
  name: string;
  default_department_id?: string | null;
  category?: string | null;
  default_duration_minutes?: number | null;
  trigger_offset_minutes?: number;
  trigger_type?: string;
  description?: string | null;
  attributes?: Record<string, unknown>;
}

export interface EquipmentTypeResponse {
  id: string;
  name: string;
  code?: string | null;
  category?: string | null;
  requires_driver: boolean;
  is_active: boolean;
}

export interface DepartmentQualificationResponse {
  id: string;
  department_id: string;
  qualification_code: string;
  qualification_name: string;
  description?: string | null;
  is_active: boolean;
}

export interface DepartmentQualificationLevelResponse {
  id: string;
  department_id: string;
  qualification_code: string;
  level_code: string;
  level_name: string;
  level_rank: number;
  covered_level_codes: string[];
  is_active: boolean;
}

export interface QualificationGrantResponse {
  id: string;
  user_id: string;
  department_id: string;
  qualification_code: string;
  level_code: string;
  status: string;
}

export interface CrewSlotRequirement {
  slot_code: string;
  qualification_code: string;
  min_level_code?: string | null;
  required_count?: number;
  must_be_distinct?: boolean;
  exclusive_group?: string | null;
  remarks?: string | null;
}

export interface EquipmentRequirement {
  slot_code: string;
  equipment_type_id?: string | null;
  equipment_type_code?: string | null;
  required_count?: number;
  must_be_distinct?: boolean;
  requires_driver?: boolean;
  driver_qualification_code?: string | null;
  driver_min_level_code?: string | null;
  remarks?: string | null;
}

export interface RequirementVersionResponse {
  id: string;
  department_id: string;
  task_type: string;
  version_no: number;
  status: string;
  requirements: CrewSlotRequirement[];
  crew_requirements: CrewSlotRequirement[];
  equipment_requirements: EquipmentRequirement[];
  turnaround_continuity_rules: unknown[];
  notes?: string | null;
  published_at?: string | null;
  created_at?: string | null;
  updated_at?: string | null;
}

export interface RequirementDraftPayload {
  task_type: string;
  requirements?: CrewSlotRequirement[];
  crew_requirements?: CrewSlotRequirement[];
  equipment_requirements?: EquipmentRequirement[];
  turnaround_continuity_rules?: unknown[];
  notes?: string | null;
}

export interface RequirementPublishPayload {
  task_type: string;
  draft_id?: string | null;
}

export type CompletionTimeMode = 'start_plus_duration' | 'completion_anchor_offset';

export interface FlightGenerationRuleResponse {
  id: string;
  department_id: string;
  task_type: string;
  leg_scope: string;
  version_no: number;
  status: string;
  rule_name?: string | null;
  conditions: Record<string, unknown>;
  generation_anchor_type: string;
  start_offset_minutes: number;
  completion_time_mode: CompletionTimeMode;
  completion_anchor_type?: string | null;
  completion_offset_minutes?: number | null;
  duration_minutes?: number | null;
  /** 重排时该作业开始时间允许后滑的分钟数；null 表示未配置，回退系统默认值。 */
  start_flex_minutes?: number | null;
  /**
   * 完工预排冲突预警提前量（分钟，0..60）；null 表示未配置，回退系统默认值。
   * 0 表示下一单到计划开始时间才触发。
   */
  completion_warning_lead_minutes?: number | null;
  /**
   * 人数 -> 作业时长(分钟) 映射，如 `{"1":45,"2":30,"3":25}`。
   * null 表示未配置，重排时退回 duration_minutes 常量。
   */
  duration_by_crew_size?: Record<string, number> | null;
  publication_state: string;
  publish_trigger_mode: string;
  publish_at?: string | null;
  publish_offset_minutes?: number | null;
  publish_event_code?: string | null;
  notes?: string | null;
  published_at?: string | null;
  created_at?: string | null;
  updated_at?: string | null;
}

export interface FlightGenerationRulePayload {
  rule_id?: string | null;
  rule_name?: string | null;
  task_type: string;
  leg_scope: string;
  status?: string;
  conditions?: Record<string, unknown>;
  generation_anchor_type?: string;
  start_offset_minutes?: number;
  completion_time_mode?: CompletionTimeMode;
  completion_anchor_type?: string | null;
  completion_offset_minutes?: number | null;
  duration_minutes?: number | null;
  start_flex_minutes?: number | null;
  completion_warning_lead_minutes?: number | null;
  duration_by_crew_size?: Record<string, number> | null;
  publication_state?: string;
  publish_trigger_mode?: string;
  publish_offset_minutes?: number | null;
  publish_event_code?: string | null;
  notes?: string | null;
}

export interface GenerationAdjustmentRuleResponse {
  id: string;
  department_id: string;
  task_type: string;
  version_no: number;
  status: string;
  rule_name?: string | null;
  conditions: Record<string, unknown>;
  actions: unknown[];
  notes?: string | null;
  published_at?: string | null;
  created_at?: string | null;
  updated_at?: string | null;
}

export interface GenerationAdjustmentRulePayload {
  rule_id?: string | null;
  rule_name?: string | null;
  task_type: string;
  status?: string;
  conditions?: Record<string, unknown>;
  actions?: unknown[];
  notes?: string | null;
}

export interface TemporaryTaskTemplateResponse {
  id: string;
  department_id: string;
  template_code: string;
  template_name: string;
  task_type: string;
  crew_requirements: CrewSlotRequirement[];
  equipment_requirements: EquipmentRequirement[];
  notes?: string | null;
  is_active: boolean;
}

export interface DispatchRuleValidationResponse {
  valid: boolean;
  conflicts: unknown[];
  messages: string[];
}

export interface DispatchRulePreviewRequest {
  flight_id?: string | null;
  sample_flight?: Record<string, unknown>;
}

export interface DispatchRulePreviewResponse {
  generated_orders: unknown[];
  applied_adjustments: unknown[];
  turnaround_constraints: unknown[];
  conflicts: unknown[];
  blocking_errors: string[];
}

export interface DispatchOrderCreatePayload {
  flight_id?: string | null;
  task_type?: string | null;
  temporary_task_template_code?: string | null;
  department_id?: string | null;
  stand_id?: string | null;
  location?: string | null;
  individual_user_id?: string | null;
  planned_start_time?: string | null;
  planned_end_time?: string | null;
  priority?: number;
  publication_state?: string;
  source_type?: string;
  leg_scope?: string;
  manual_lock?: boolean;
  remarks?: string | null;
  workflow_context?: Record<string, unknown>;
  crew_requirement_snapshot?: unknown[];
  equipment_requirement_snapshot?: unknown[];
  task_crew?: Record<string, unknown>;
  equipment_assignment?: unknown[];
  attributes?: Record<string, unknown>;
}

export interface DispatchOrderCreateResponse {
  id?: string;
  status?: string;
  message?: string;
  [key: string]: unknown;
}

export interface DepartmentRuleBundle {
  qualifications: DepartmentQualificationResponse[];
  qualificationLevels: DepartmentQualificationLevelResponse[];
  qualificationGrants: QualificationGrantResponse[];
  flightGenerationRules: FlightGenerationRuleResponse[];
  generationAdjustmentRules: GenerationAdjustmentRuleResponse[];
  requirementVersions: RequirementVersionResponse[];
  temporaryTaskTemplates: TemporaryTaskTemplateResponse[];
}

export type WorkbenchApi = ReturnType<typeof createWorkbenchApi>;

interface ListResponse<T> {
  items?: T[];
  data?: T[];
}

function unwrapList<T>(payload: ListResponse<T> | T[] | null | undefined): T[] {
  if (!payload) return [];
  if (Array.isArray(payload)) return payload;
  if (Array.isArray(payload.items)) return payload.items;
  if (Array.isArray(payload.data)) return payload.data;
  return [];
}

export function createWorkbenchApi(api: ReturnType<typeof useApi> = useApi()) {
  const ruleBase = (departmentId: string) =>
    `/api/v2/dispatch/rules/departments/${encodeURIComponent(departmentId)}`;

  async function getOk<T>(url: string, fallback: string): Promise<T> {
    const res = await api.get<T>(url);
    if (!res.ok) throw new Error(describeApiError(res.data, fallback));
    return res.data as T;
  }
  async function postOk<T>(url: string, body: unknown, fallback: string): Promise<T> {
    const res = await api.post<T>(url, body);
    if (!res.ok) throw new Error(describeApiError(res.data, fallback));
    return res.data as T;
  }

  return {
    api,
    async listDepartments(): Promise<DepartmentResponse[]> {
      const data = await getOk<ListResponse<DepartmentResponse> | DepartmentResponse[]>(
        '/api/v2/dispatch/resources/departments?page_size=500',
        '加载科室列表失败',
      );
      return unwrapList(data);
    },
    async listTaskTypes(): Promise<TaskTypeResponse[]> {
      const data = await getOk<ListResponse<TaskTypeResponse> | TaskTypeResponse[]>(
        '/api/v2/dispatch/task-types?page_size=200',
        '加载任务类型失败',
      );
      return unwrapList(data);
    },
    async createTaskType(payload: TaskTypeCreatePayload): Promise<TaskTypeResponse> {
      return postOk<TaskTypeResponse>('/api/v2/dispatch/task-types', payload, '创建任务类型失败');
    },
    async deleteTaskType(code: string): Promise<void> {
      const res = await api.delete(`/api/v2/dispatch/task-types/${encodeURIComponent(code)}`);
      if (!res.ok) throw new Error(describeApiError(res.data, '删除任务类型失败'));
    },
    async listEquipmentTypes(): Promise<EquipmentTypeResponse[]> {
      const data = await getOk<ListResponse<EquipmentTypeResponse> | EquipmentTypeResponse[]>(
        '/api/v2/dispatch/resources/equipment-types?page_size=200',
        '加载设备类型失败',
      );
      return unwrapList(data);
    },
    async fetchDepartmentBundle(departmentId: string): Promise<DepartmentRuleBundle> {
      const base = ruleBase(departmentId);
      const [
        qualifications,
        qualificationLevels,
        qualificationGrants,
        flightGenerationRules,
        generationAdjustmentRules,
        requirementVersions,
        temporaryTaskTemplates,
      ] = await Promise.all([
        getOk<ListResponse<DepartmentQualificationResponse> | DepartmentQualificationResponse[]>(
          `${base}/qualifications`,
          '加载科室资质失败',
        ),
        getOk<ListResponse<DepartmentQualificationLevelResponse> | DepartmentQualificationLevelResponse[]>(
          `${base}/qualification-levels`,
          '加载资质等级失败',
        ),
        getOk<ListResponse<QualificationGrantResponse> | QualificationGrantResponse[]>(
          `${base}/qualification-grants`,
          '加载人员资质失败',
        ),
        getOk<ListResponse<FlightGenerationRuleResponse> | FlightGenerationRuleResponse[]>(
          `${base}/flight-generation-rules`,
          '加载航班生成规则失败',
        ),
        getOk<ListResponse<GenerationAdjustmentRuleResponse> | GenerationAdjustmentRuleResponse[]>(
          `${base}/generation-adjustment-rules`,
          '加载生成调整规则失败',
        ),
        getOk<ListResponse<RequirementVersionResponse> | RequirementVersionResponse[]>(
          `${base}/task-type-requirements/versions`,
          '加载资质要求版本失败',
        ),
        getOk<ListResponse<TemporaryTaskTemplateResponse> | TemporaryTaskTemplateResponse[]>(
          `${base}/temporary-task-templates`,
          '加载临时任务模板失败',
        ),
      ]);
      return {
        qualifications: unwrapList(qualifications),
        qualificationLevels: unwrapList(qualificationLevels),
        qualificationGrants: unwrapList(qualificationGrants),
        flightGenerationRules: unwrapList(flightGenerationRules),
        generationAdjustmentRules: unwrapList(generationAdjustmentRules),
        requirementVersions: unwrapList(requirementVersions),
        temporaryTaskTemplates: unwrapList(temporaryTaskTemplates),
      };
    },
    async saveFlightGenerationRule(
      departmentId: string,
      payload: FlightGenerationRulePayload,
    ): Promise<FlightGenerationRuleResponse> {
      return postOk<FlightGenerationRuleResponse>(
        `${ruleBase(departmentId)}/flight-generation-rules`,
        payload,
        '保存航班生成规则失败',
      );
    },
    async deleteFlightGenerationRule(departmentId: string, ruleId: string): Promise<void> {
      const url = `${ruleBase(departmentId)}/flight-generation-rules/${encodeURIComponent(ruleId)}/delete`;
      const res = await api.post(url, {});
      if (!res.ok) throw new Error(describeApiError(res.data, '删除航班生成规则失败'));
    },
    async saveGenerationAdjustmentRule(
      departmentId: string,
      payload: GenerationAdjustmentRulePayload,
    ): Promise<GenerationAdjustmentRuleResponse> {
      return postOk<GenerationAdjustmentRuleResponse>(
        `${ruleBase(departmentId)}/generation-adjustment-rules`,
        payload,
        '保存生成调整规则失败',
      );
    },
    async saveRequirementDraft(
      departmentId: string,
      payload: RequirementDraftPayload,
    ): Promise<RequirementVersionResponse> {
      return postOk<RequirementVersionResponse>(
        `${ruleBase(departmentId)}/task-type-requirements/drafts`,
        payload,
        '保存资质要求草稿失败',
      );
    },
    async publishRequirement(
      departmentId: string,
      payload: RequirementPublishPayload,
    ): Promise<{ published_version: RequirementVersionResponse }> {
      return postOk<{ published_version: RequirementVersionResponse }>(
        `${ruleBase(departmentId)}/task-type-requirements/publish`,
        payload,
        '发布资质要求版本失败',
      );
    },
    async validateRules(payload: {
      generation_rule?: FlightGenerationRulePayload;
      adjustment_rule?: GenerationAdjustmentRulePayload;
    }): Promise<DispatchRuleValidationResponse> {
      return postOk<DispatchRuleValidationResponse>(
        '/api/v2/dispatch/rules/validate',
        payload,
        '校验规则失败',
      );
    },
    async previewRules(payload: DispatchRulePreviewRequest): Promise<DispatchRulePreviewResponse> {
      return postOk<DispatchRulePreviewResponse>(
        '/api/v2/dispatch/rules/preview',
        payload,
        '规则预览失败',
      );
    },
    async createDispatchOrder(payload: DispatchOrderCreatePayload): Promise<DispatchOrderCreateResponse> {
      return postOk<DispatchOrderCreateResponse>(
        '/api/v2/dispatch-orders',
        payload,
        '创建派工单失败',
      );
    },
  };
}
