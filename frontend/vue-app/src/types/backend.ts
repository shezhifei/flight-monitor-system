export interface RouteStationPayload {
  code: string;
  name: string | null;
  [key: string]: unknown;
}

export interface FlightLegPayload {
  leg_type: 'inbound' | 'outbound';
  flight_no: string;
  flight_type: string;
  mission?: number | null;
  origin_stations: RouteStationPayload[];
  destination_stations: RouteStationPayload[];
  origin_code?: string | null;
  destination_code?: string | null;
  origin_name?: string | null;
  destination_name?: string | null;
  is_vip: boolean;
  stand_type?: string | null;
  scheduled_time?: string | null;
}

export interface FlightAnomalySummary {
  has_open_anomaly: boolean;
  open_count: number;
  acknowledged_count: number;
}

export type BusinessCaseLegType = 'inbound' | 'outbound';
export type BusinessCaseStatus = 'INITIAL' | 'PENDING' | 'PROCESSING' | 'SUCCESS' | 'COMPLETED' | 'FAILED';
export type BusinessCaseVisibilityScope = 'COMMON' | 'DEPARTMENT' | string;
export type WorkflowFormTemplateStatus = 'DRAFT' | 'ACTIVE' | 'RETIRED';
export type WorkflowFormAssignmentMode = 'DEPARTMENT_ROLES' | 'TASK_CANDIDATE' | 'EXPLICIT_USERS';
export type WorkflowFormWriteBackMode = 'BUSINESS_CASE_CONTEXT' | 'APPEND_ENTRY' | 'BOTH';
export type WorkflowFormBindingSource = 'DB' | 'BPMN' | 'DB_OVERRIDE';
export type WorkflowFormSubmissionStatus = 'SUBMITTED' | 'REPLACED' | 'REVOKED';

export interface WorkflowFormUiSchemaField {
  'ui:widget'?: string;
  'ui:placeholder'?: string;
  'ui:help'?: string;
  [key: string]: unknown;
}

export interface WorkflowFormJsonSchemaProperty {
  type?: string | string[];
  title?: string;
  description?: string;
  format?: string;
  enum?: Array<string | number | boolean>;
  default?: unknown;
  items?: WorkflowFormJsonSchemaProperty | null;
  [key: string]: unknown;
}

export interface WorkflowFormJsonSchemaObject {
  type?: 'object' | string;
  title?: string;
  description?: string;
  required?: string[];
  properties?: Record<string, WorkflowFormJsonSchemaProperty>;
  [key: string]: unknown;
}

export interface WorkflowFormUiSchemaObject {
  'ui:order'?: string[];
  [key: string]: WorkflowFormUiSchemaField | string[] | unknown;
}

export interface BusinessCaseWorkflowFormProjection {
  form_code: string;
  form_version: number;
  task_definition_key: string;
  submission_id: string;
  submitted_by: string;
  submitted_operator_name?: string | null;
  submitted_department_id?: string | null;
  submitted_department_name?: string | null;
  submitted_at: string;
  write_back_key: string;
  data: Record<string, unknown>;
  summary: Record<string, unknown>;
}

export type BusinessCaseWorkflowFormsProjection = Record<string, BusinessCaseWorkflowFormProjection>;

export interface WorkflowFormTemplateResponse {
  id: string;
  form_code: string;
  name: string;
  version: number;
  schema_json: WorkflowFormJsonSchemaObject;
  ui_schema_json: WorkflowFormUiSchemaObject;
  status: WorkflowFormTemplateStatus;
  description?: string | null;
  created_by: string;
  created_at: string;
  updated_at: string;
}

export interface WorkflowFormBindingResponse {
  id: string;
  template_code: string;
  process_definition_key: string;
  task_definition_key: string;
  form_code: string;
  form_version?: number | null;
  target_department_id?: string | null;
  target_department_name?: string | null;
  target_roles: string[];
  assignment_mode: WorkflowFormAssignmentMode;
  write_back_mode: WorkflowFormWriteBackMode;
  write_back_key: string;
  flowable_variable_prefix?: string | null;
  complete_task_on_submit: boolean;
  allow_resubmit: boolean;
  source: WorkflowFormBindingSource;
  created_at: string;
  updated_at: string;
}

export interface WorkflowFormSubmissionResponse {
  submission_id: string;
  case_id: string;
  run_id?: string | null;
  process_instance_id: string;
  task_id: string;
  task_definition_key: string;
  form_code: string;
  form_version: number;
  data: Record<string, unknown>;
  summary: Record<string, unknown>;
  submitted_by: string;
  submitted_operator_name?: string | null;
  submitted_department_id?: string | null;
  submitted_department_name?: string | null;
  submitted_at: string;
  status: WorkflowFormSubmissionStatus;
}

export interface WorkflowTaskFormView {
  task_id: string;
  task_definition_key: string;
  task_name: string;
  form_code: string;
  form_version: number;
  name: string;
  schema: WorkflowFormJsonSchemaObject;
  ui_schema: WorkflowFormUiSchemaObject;
  can_submit: boolean;
  readonly_reason?: string | null;
  latest_submission?: WorkflowFormSubmissionResponse | null;
}

export interface CaseWorkflowFormsResponse {
  case_id: string;
  run_id: string;
  process_instance_id: string;
  forms: WorkflowTaskFormView[];
}

export interface SubmitWorkflowFormRequest {
  task_id: string;
  data: Record<string, unknown>;
}

export interface SubmitWorkflowFormResponse {
  submission_id: string;
  case_id: string;
  form_code: string;
  form_version: number;
  flowable_task_completed: boolean;
  business_case: BusinessCaseDetail | BusinessCaseSummary | Record<string, unknown>;
}

export interface BusinessCaseWorkflowReceiptSummary {
  total_count: number;
  pending_count: number;
  acknowledged_count: number;
  rejected_count: number;
  latest_updated_at?: string | null;
  remind_after_at?: string | null;
  is_overdue: boolean;
  overall_status: string;
}

export interface BusinessCaseWorkflowReceiptItem {
  user_id: string;
  recipient_user_id?: string | null;
  recipient_username?: string | null;
  recipient_display_name?: string | null;
  recipient_department?: string | null;
  recipient_job_title?: string | null;
  username?: string | null;
  display_name?: string | null;
  account_name?: string | null;
  ack_status: string;
  ack_at?: string | null;
  ack_note?: string | null;
  updated_at?: string | null;
}

export interface BusinessCaseWorkflowReceiptProjection {
  receipt_group_id: string;
  title?: string | null;
  severity?: string | null;
  origin_type: string;
  created_at?: string | null;
  summary: BusinessCaseWorkflowReceiptSummary;
  items?: BusinessCaseWorkflowReceiptItem[];
}

export interface BusinessCaseContext {
  bound_leg_type?: BusinessCaseLegType | null;
  bound_flight_no?: string | null;
  workflow_receipt?: BusinessCaseWorkflowReceiptProjection | null;
  forms?: BusinessCaseWorkflowFormsProjection | null;
  [key: string]: unknown;
}

export interface BusinessCaseAppendAcknowledgment {
  acknowledged_at?: string | null;
  [key: string]: unknown;
}

export interface BusinessCaseAppendMetadata {
  mention_user_ids?: string[];
  acknowledgments?: Record<string, BusinessCaseAppendAcknowledgment>;
  [key: string]: unknown;
}

export interface BusinessCaseAppendDetail {
  append_id: string;
  case_id: string;
  content: string;
  submitted_by: string;
  submitted_operator_name?: string | null;
  appended_at: string;
  metadata: BusinessCaseAppendMetadata;
}

export interface BusinessCaseDepartmentFields {
  visibility_scope?: BusinessCaseVisibilityScope | null;
  visibilityScope?: BusinessCaseVisibilityScope | null;
  department_id?: string | null;
  departmentId?: string | null;
  department_name_snapshot?: string | null;
  department_name?: string | null;
  departmentName?: string | null;
}

export interface BusinessCaseSummary extends BusinessCaseDepartmentFields {
  case_id: string;
  case_type: string;
  case_type_name?: string | null;
  flight_id: string;
  flight_no: string;
  created_at: string;
  created_by: string;
  updated_by: string;
  description: string;
  status: BusinessCaseStatus | string;
  stand?: string | null;
  gate?: string | null;
  finished_at?: string | null;
  cancelled_at?: string | null;
  context: BusinessCaseContext;
  workflow_receipt?: BusinessCaseWorkflowReceiptProjection | null;
  append_count: number;
  latest_append?: BusinessCaseAppendDetail | null;
}

export interface BusinessCaseDetail extends BusinessCaseSummary {
  log?: string[];
  append_entries: BusinessCaseAppendDetail[];
}

export interface BusinessCaseAiFieldConfig {
  type?: string | null;
  label?: string | null;
  required: boolean;
  aliases: string[];
  examples: string[];
  enum_values: string[];
}

export interface BusinessCaseAiExtractionConfig {
  enabled: boolean;
  utterance_session?: {
    final_grace_ms?: number | null;
  } | null;
  aliases: string[];
  trigger_phrases: string[];
  leg_binding: {
    allowed: string[];
    default?: string | null;
    required: boolean;
  };
  flight_matching: {
    prefer_leg?: string | null;
    exclude_cancelled?: boolean | null;
    exclude_departed?: boolean | null;
    exclude_actual_departure?: boolean | null;
    window_hours_before?: number | null;
    window_hours_after?: number | null;
    min_auto_match_score?: number | null;
  };
  fields: Record<string, BusinessCaseAiFieldConfig>;
  forbidden_fields: string[];
  description_template?: string | null;
  remarks_template?: string | null;
  examples: Record<string, unknown>[];
}

export interface BusinessCaseTypeDefinition {
  code: string;
  name: string;
  visibility_scope?: BusinessCaseVisibilityScope | null;
  visibility_scopes?: BusinessCaseVisibilityScope[] | null;
  allowed_visibility_scopes?: BusinessCaseVisibilityScope[] | null;
  allowedVisibilityScopes?: BusinessCaseVisibilityScope[] | null;
  default_visibility_scope?: BusinessCaseVisibilityScope | null;
  defaultVisibilityScope?: BusinessCaseVisibilityScope | null;
  supports_common_scope?: boolean | null;
  allow_common_scope?: boolean | null;
  allowCommonScope?: boolean | null;
  department_only?: boolean | null;
  departmentOnly?: boolean | null;
  ai_extraction_config?: BusinessCaseAiExtractionConfig | null;
  case_properties?: BusinessCaseProperties | null;
  [key: string]: unknown;
}

export interface BusinessCaseProperties {
  auto_copilot?: {
    utterance_final_grace_ms?: number | null;
  };
  binding_policy?: {
    flight_required?: boolean;
    allowed_leg_types?: Array<'outbound' | 'inbound'>;
    default_leg_type?: 'outbound' | 'inbound' | null;
    leg_type_required?: boolean;
    flight_match_policy?: {
      allow_numeric_suffix?: boolean;
      exclude_cancelled?: boolean;
      exclude_departed?: boolean;
      exclude_actual_departure?: boolean;
      time_window_hours_before?: number;
      time_window_hours_after?: number;
      min_auto_match_score?: number;
    };
  };
  extra_info_schema?: {
    fields?: Record<string, {
      type?: string;
      label?: string;
      required?: boolean;
      enum_values?: string[];
      display_in_notification?: boolean;
    }>;
    summary_template?: string;
  };
  workflow_policy?: {
    batch_notification_enabled?: boolean;
    batch_receipt_mode?: 'shared_group' | 'per_case';
  };
  duplicate_policy?: {
    enabled?: boolean;
    fields?: string[];
    include_extra_info?: boolean;
    include_bound_leg?: boolean;
    active_statuses?: string[];
  };
}

export interface FlightResponse {
  flight_id?: string | null;
  /** 监控行稳定键（flight_monitor_rows.row_id）：建链/拆链不改；列表选中键用它。 */
  row_id?: string | null;
  /** 同机周转链 id（turnaround_links.id），仅过站行有。 */
  link_id?: string | null;
  /** 行类型：turnaround | single。 */
  kind?: string | null;
  /** 进港方向航班 id（进港侧详情/单元格 PATCH 的目标）。 */
  inbound_flight_id?: string | null;
  /** 出港方向航班 id（出港侧详情/单元格 PATCH 的目标）。 */
  outbound_flight_id?: string | null;
  flight_number?: string | null;
  airline_code?: string | null;
  registration?: string | null;
  aircraft_type_detail?: string | null;
  status?: string | null;
  scheduled_departure?: string | null;
  scheduled_arrival?: string | null;
  estimated_departure?: string | null;
  estimated_arrival?: string | null;
  actual_departure?: string | null;
  actual_arrival?: string | null;
  stand?: string | null;
  gate?: string | null;
  terminal?: string | null;
  position?: string | null;
  baggage_carousel?: string | null;
  has_boarding_restriction: boolean;
  is_quick_turnaround: boolean;
  is_commercial_signed: boolean;
  inbound_leg?: FlightLegPayload | null;
  outbound_leg?: FlightLegPayload | null;
  anomaly_summary: FlightAnomalySummary;
  business_cases: BusinessCaseSummary[];
  created_at?: string | null;
  updated_at?: string | null;
  version: number;
  flight_remarks?: string | null;
  load_planning_remarks?: string | null;
  aircraft_maintenance_remarks?: string | null;
  aircraft_check_remarks?: string | null;
  cobt_time?: string | null;
  codt?: string | null;
  labels?: string[];
  boarding_allowed_time?: string | null;
  start_boarding_time?: string | null;
  end_boarding_time?: string | null;
  on_blocks_time?: string | null;
  off_blocks_time?: string | null;
  created_by?: string | null;
  updated_by?: string | null;
}

export interface FlightListResponse {
  items: FlightResponse[];
  total: number;
  page: number;
  size: number;
  pages: number;
}

export interface FlightStreamFrame {
  type?: string;
  flight_id?: string | number | null;
  changed_fields?: string[];
  flights?: FlightResponse[];
  flight?: Partial<FlightResponse>;
  patch?: Partial<FlightResponse>;
  flight_data?: FlightResponse;
  data?: FlightResponse | FlightResponse[] | AnomalyAlert | UserNotification | null;
  status?: string | null;
}

export interface AnomalyAlert {
  id?: string | number | null;
  flight_id?: string | number | null;
  anomaly_id?: string | number | null;
  anomaly_type?: string | null;
  severity?: string | null;
  status?: string | null;
  description?: string | null;
  detected_at?: string | null;
  resolved_at?: string | null;
  [key: string]: unknown;
}

export interface UserNotification {
  id?: string | number | null;
  notification_id?: string | number | null;
  user_id?: string | null;
  title?: string | null;
  body?: string | null;
  message?: string | null;
  content?: string | null;
  notification_type?: string | null;
  is_read?: boolean;
  link?: string | null;
  created_at?: string | null;
  read_at?: string | null;
  related_flight_no?: string | null;
  related_flight_label?: string | null;
  flight_no?: string | null;
  severity?: string | null;
  [key: string]: unknown;
}

export interface NotificationResponse {
  id: string;
  user_id: string;
  title: string;
  body: string;
  notification_type: string;
  is_read: boolean;
  link?: string | null;
  created_at: string;
  read_at?: string | null;
}

export interface NotificationListResponse {
  items: NotificationResponse[];
  total: number;
  unread_count: number;
}

export interface DispatchTimelineEventResponse {
  timeline_id: string;
  flight_id: string;
  milestone_code: string;
  occurred_at: string;
  leg_type?: string | null;
  recorded_by?: string | null;
  client_action_id?: string | null;
  source: string;
  payload: Record<string, unknown>;
  created_at: string;
}

export interface DispatchTimelineListResponse {
  items: DispatchTimelineEventResponse[];
}

export interface LabelDefinition {
  label_id: string;
  code: string;
  name: string;
  color: string;
  icon?: string | null;
  scope: 'flight' | 'leg' | 'both';
  category: 'system' | 'custom';
  is_active: boolean;
  sort_order?: number;
  created_by?: string | null;
  created_at: string;
  updated_at: string;
}

export interface CreateLabelRequest {
  code: string;
  name: string;
  color: string;
  icon?: string | null;
  scope: 'flight' | 'leg' | 'both';
}

export interface UpdateLabelRequest {
  name?: string;
  color?: string;
  icon?: string | null;
  is_active?: boolean;
  sort_order?: number;
}

export interface LabelListResponse {
  items: LabelDefinition[];
  total: number;
}

export const SSE_TOPICS = {
  FLIGHTS: 'flights',
  FLIGHT_STATUS_CHANGES: 'flight_status_changes',
  ANOMALY_ALERTS: 'anomaly_alerts',
  GLOBAL_STATUS: 'global_status',
  KPI_UPDATED: 'kpi_updated',
  ERROR_EVENTS: 'error_events',
  SYSTEM_ALERTS: 'system_alerts',
  AI_EXECUTION: 'ai_execution',
  SMART_MONITOR: 'smart_monitor',
  BUSINESS_CASES: 'business_cases',
} as const;

export type SSETopic = typeof SSE_TOPICS[keyof typeof SSE_TOPICS];

export function buildSSEUrl(baseUrl: string, topics: SSETopic[]): string {
  const topicParam = topics.join(',');
  return `${baseUrl}/api/v2/sse/stream?topics=${encodeURIComponent(topicParam)}`;
}

export interface ConditionOperator {
  operator?: 'AND' | 'OR';
  children?: ConditionItem[];
}

export interface ConditionItem {
  field: string;
  op: 'eq' | 'neq' | 'gt' | 'gte' | 'lt' | 'lte' | 'in' | 'nin' | 'contains';
  value: unknown;
}

export type AdjustmentActionType =
  | 'add_crew_slot'
  | 'increase_crew_count'
  | 'upgrade_crew_level'
  | 'add_equipment_slot'
  | 'increase_equipment_count'
  | 'extend_duration'
  | 'shorten_duration'
  | 'advance_publish'
  | 'delay_publish'
  | 'require_driver_for_equipment';

export interface AddCrewSlotConfig {
  slot_code: string;
  qualification_code: string;
  required_count: number;
  must_be_distinct?: boolean;
  remarks?: string;
}

export interface IncreaseCrewCountConfig {
  slot_code: string;
  delta: number;
}

export interface UpgradeCrewLevelConfig {
  slot_code: string;
  min_level_code: string;
}

export interface AddEquipmentSlotConfig {
  slot_code: string;
  equipment_type_code: string;
  required_count: number;
  remarks?: string;
}

export interface IncreaseEquipmentCountConfig {
  slot_code: string;
  delta: number;
}

export interface ExtendDurationConfig {
  delta_minutes: number;
}

export interface AdvancePublishConfig {
  delta_minutes: number;
}

export interface RequireDriverConfig {
  slot_code: string;
  driver_qualification_code: string;
  driver_min_level_code?: string;
}

export type AdjustmentActionConfig =
  | AddCrewSlotConfig
  | IncreaseCrewCountConfig
  | UpgradeCrewLevelConfig
  | AddEquipmentSlotConfig
  | IncreaseEquipmentCountConfig
  | ExtendDurationConfig
  | AdvancePublishConfig
  | RequireDriverConfig;

export interface AdjustmentRuleConfig {
  action_type: AdjustmentActionType;
  config: AdjustmentActionConfig;
}

export interface DispatchOrderAdjustmentRule {
  id: string;
  adjuster_type: AdjustmentActionType;
  name: string;
  description?: string;
  event_patterns: string[];
  priority: number;
  conditions: ConditionOperator | null;
  config: AdjustmentRuleConfig;
  is_enabled: boolean;
  department_id?: string | null;
  department_name?: string | null;
  created_at: string;
  updated_at: string;
  created_by?: string | null;
}

export interface CreateAdjustmentRuleRequest {
  adjuster_type: AdjustmentActionType;
  name: string;
  description?: string;
  event_patterns: string[];
  priority?: number;
  conditions?: ConditionOperator | null;
  config: AdjustmentRuleConfig;
  is_enabled?: boolean;
  department_id?: string | null;
}

export interface UpdateAdjustmentRuleRequest extends Partial<CreateAdjustmentRuleRequest> {
  id: string;
}

export interface DispatchOrderAdjustmentRuleListResponse {
  items: DispatchOrderAdjustmentRule[];
  total: number;
}

export interface CreateCrewRequirement {
  slot_code: string;
  qualification_code: string;
  required_count: number;
  min_level_code?: string;
}

export interface GenerationRuleConfig {
  task_type: string;
  duration_minutes_from?: string;
  fixed_duration_minutes?: number;
  crew_requirements: CreateCrewRequirement[];
  equipment_requirements?: CreateCrewRequirement[];
}

export interface EventDrivenGenerationRule {
  id: string;
  generator_type: string;
  name: string;
  description?: string;
  event_patterns: string[];
  priority: number;
  conditions: ConditionOperator | null;
  config: GenerationRuleConfig;
  is_enabled: boolean;
  department_id?: string | null;
  department_name?: string | null;
  created_at: string;
  updated_at: string;
  created_by?: string | null;
}

export interface CreateGenerationRuleRequest {
  generator_type: string;
  name: string;
  description?: string;
  event_patterns: string[];
  priority?: number;
  conditions?: ConditionOperator | null;
  config: GenerationRuleConfig;
  is_enabled?: boolean;
  department_id?: string | null;
}

export interface UpdateGenerationRuleRequest extends Partial<CreateGenerationRuleRequest> {
  id: string;
}

export interface EventDrivenGenerationRuleListResponse {
  items: EventDrivenGenerationRule[];
  total: number;
}

export interface RulePreviewRequest {
  event_type: string;
  payload: Record<string, unknown>;
  flight_id?: string;
}

export interface RulePreviewAffectedOrder {
  order_id: string;
  task_type: string;
  modified_fields: string[];
  reason: string;
}

export interface RulePreviewMatchedAdjustment {
  rule_id: string;
  rule_name: string;
  action_type: AdjustmentActionType;
  action_description: string;
  affected_orders: RulePreviewAffectedOrder[];
}

export interface RulePreviewMatchedGeneration {
  rule_id: string;
  rule_name: string;
  would_generate: boolean;
  generated_order_preview?: Record<string, unknown>;
}

export interface RulePreviewResponse {
  matched_adjustment_rules: RulePreviewMatchedAdjustment[];
  matched_generation_rules: RulePreviewMatchedGeneration[];
  timestamp: string;
}

export const SUPPORTED_EVENT_TYPES = [
  { value: 'flight.created_v2', label: '航班创建' },
  { value: 'flight.status_updated_v2', label: '航班状态更新' },
  { value: 'flight.resource_updated_v2', label: '航班资源更新' },
  { value: 'flight.leg_upserted_v2', label: '航班腿信息更新' },
  { value: 'flight.remarks_updated_v2', label: '航班备注更新' },
] as const;

export const ADJUSTMENT_ACTION_LABELS: Record<AdjustmentActionType, string> = {
  add_crew_slot: '添加人员槽位',
  increase_crew_count: '增加人员数量',
  upgrade_crew_level: '升级资质等级',
  add_equipment_slot: '添加设备槽位',
  increase_equipment_count: '增加设备数量',
  extend_duration: '延长作业时长',
  shorten_duration: '缩短作业时长',
  advance_publish: '提前发布时间',
  delay_publish: '推迟发布时间',
  require_driver_for_equipment: '要求设备司机',
};

export const CONDITION_OPERATOR_LABELS: Record<string, string> = {
  AND: '且 (AND)',
  OR: '或 (OR)',
  eq: '等于',
  neq: '不等于',
  gt: '大于',
  gte: '大于等于',
  lt: '小于',
  lte: '小于等于',
  in: '在列表中',
  nin: '不在列表中',
  contains: '包含',
};
