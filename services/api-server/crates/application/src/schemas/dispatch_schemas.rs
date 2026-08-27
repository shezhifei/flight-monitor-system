//! 派工系统 DTO 模式。

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const MOBILE_SYNC_ACTION_TYPES: &[&str] = &[
    "accept",
    "checkin",
    "checkout",
    "start",
    "complete",
    "report_issue",
    "eta_report",
];

// ---------------------------------------------------------------------------
// 通用
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionSchema {
    pub lat: f64,
    pub lng: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PositionUpdate {
    pub lat: f64,
    pub lng: f64,
    pub stand_id: Option<String>,
}

// ---------------------------------------------------------------------------
// 科室
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct DepartmentCreate {
    pub name: String,
    pub code: Option<String>,
    pub description: Option<String>,
    pub manager_id: Option<String>,
    pub terminal: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DepartmentUpdate {
    pub name: Option<String>,
    pub code: Option<String>,
    pub description: Option<String>,
    pub manager_id: Option<String>,
    pub terminal: Option<String>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DepartmentResponse {
    pub id: String,
    pub name: String,
    pub code: Option<String>,
    pub description: Option<String>,
    pub manager_id: Option<String>,
    pub terminal: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub is_active: bool,
}

// ---------------------------------------------------------------------------
// 班组类型
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct TeamTypeCreate {
    pub name: String,
    pub department_id: Option<String>,
    pub code: Option<String>,
    pub description: Option<String>,
    pub color: Option<String>,
    #[serde(default)]
    pub is_driver_type: bool,
    #[serde(default)]
    pub task_types: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct TeamTypeUpdate {
    pub name: Option<String>,
    pub department_id: Option<String>,
    pub code: Option<String>,
    pub description: Option<String>,
    pub color: Option<String>,
    pub is_driver_type: Option<bool>,
    pub task_types: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TeamTypeResponse {
    pub id: String,
    pub name: String,
    pub department_id: Option<String>,
    pub code: Option<String>,
    pub description: Option<String>,
    pub color: Option<String>,
    pub is_driver_type: bool,
    #[serde(default)]
    pub task_types: Vec<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub is_active: bool,
}

// ---------------------------------------------------------------------------
// 班组
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct TeamCreate {
    pub name: String,
    /// 所属科室，创建必填（PR2 起班组直接挂科室）。
    pub department_id: String,
    pub code: Option<String>,
    pub leader_id: Option<String>,
    // PR2 起不再接受 team_type_id / terminal：serde 默认忽略未知字段，
    // 旧客户端多带这两个键不会报错，只是被忽略。
}

#[derive(Debug, Clone, Deserialize)]
pub struct TeamUpdate {
    pub name: Option<String>,
    pub department_id: Option<String>,
    pub code: Option<String>,
    pub leader_id: Option<String>,
    pub current_status: Option<String>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TeamMemberAdd {
    pub user_id: String,
    #[serde(default = "default_member_role")]
    pub role: String,
    #[serde(default)]
    pub can_drive: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TeamMemberResponse {
    pub id: String,
    pub team_id: String,
    pub user_id: String,
    pub role: String,
    pub can_drive: bool,
    pub joined_at: Option<DateTime<Utc>>,
    pub is_active: bool,
    pub username: Option<String>,
    pub user_display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TeamResponse {
    pub id: String,
    pub name: String,
    pub department_id: Option<String>,
    /// 只读历史值：班组类型已降为只读目录（PR2）。
    pub team_type_id: Option<String>,
    pub code: Option<String>,
    pub leader_id: Option<String>,
    pub current_status: String,
    pub current_position: Option<PositionSchema>,
    pub current_stand_id: Option<String>,
    pub last_position_update: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub is_active: bool,
    #[serde(default)]
    pub member_count: i32,
    #[serde(default)]
    pub members: Vec<TeamMemberResponse>,
}

// ---------------------------------------------------------------------------
// 排班
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct ShiftTemplateCreate {
    pub name: String,
    pub resource_type: String,
    pub resource_id: String,
    pub terminal: Option<String>,
    pub start_time_local: String,
    pub end_time_local: String,
    #[serde(default)]
    pub weekdays: Vec<i32>,
    pub max_continuous_minutes: Option<i32>,
    pub min_rest_minutes: Option<i32>,
    #[serde(default = "default_true_flag")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShiftTemplateResponse {
    pub id: String,
    pub name: String,
    pub resource_type: String,
    pub resource_id: String,
    pub terminal: Option<String>,
    pub start_time_local: String,
    pub end_time_local: String,
    #[serde(default)]
    pub weekdays: Vec<i32>,
    pub max_continuous_minutes: Option<i32>,
    pub min_rest_minutes: Option<i32>,
    pub enabled: bool,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ShiftInstanceCreate {
    pub template_id: Option<String>,
    pub resource_type: String,
    pub resource_id: String,
    pub terminal: Option<String>,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    #[serde(default = "default_shift_status")]
    pub status: String,
    pub max_continuous_minutes: Option<i32>,
    pub min_rest_minutes: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShiftInstanceResponse {
    pub id: String,
    pub template_id: Option<String>,
    pub resource_type: String,
    pub resource_id: String,
    pub terminal: Option<String>,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub status: String,
    pub max_continuous_minutes: Option<i32>,
    pub min_rest_minutes: Option<i32>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScheduleExceptionCreate {
    pub exception_type: String,
    pub user_id: Option<String>,
    pub team_id: Option<String>,
    pub equipment_id: Option<String>,
    pub dispatch_order_id: Option<String>,
    pub flight_id: Option<String>,
    pub lock_level: Option<String>,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub status: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleExceptionResponse {
    pub id: String,
    pub exception_type: String,
    pub resource_id: Option<String>,
    pub team_id: Option<String>,
    pub dispatch_order_id: Option<String>,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub status: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScheduleAvailabilityResponse {
    pub resource_type: String,
    pub resource_id: String,
    pub available: bool,
    pub schedule_source: String,
    pub reason: String,
    #[serde(default)]
    pub reasons: Vec<String>,
    pub lock_level: String,
    #[serde(default)]
    pub score_breakdown: HashMap<String, f64>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DispatchAnalyticsSummaryResponse {
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub assigned_order_count: i32,
    pub completed_order_count: i32,
    pub conflict_count: i32,
    pub conflict_order_count: i32,
    pub conflict_rate: f64,
    pub replanned_order_count: i32,
    pub replan_rate: f64,
    pub avg_dispatch_response_minutes: f64,
    pub team_load_balance_score: f64,
    pub equipment_idle_rate: f64,
    pub key_order_count: i32,
    pub key_order_ontime_rate: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DispatchAnalyticsBreakdownItem {
    pub group_key: String,
    pub group_label: String,
    pub order_count: i32,
    pub assigned_order_count: i32,
    pub completed_order_count: i32,
    pub occupied_minutes: f64,
    pub conflict_order_count: i32,
    pub conflict_rate: f64,
    pub replanned_order_count: i32,
    pub replan_rate: f64,
    pub avg_dispatch_response_minutes: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DispatchAnalyticsTrendItem {
    pub bucket_start: DateTime<Utc>,
    pub bucket_end: DateTime<Utc>,
    pub order_count: i32,
    pub conflict_order_count: i32,
    pub replanned_order_count: i32,
    pub avg_dispatch_response_minutes: f64,
}

// ---------------------------------------------------------------------------
// 部门资质与规则
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct DepartmentQualificationCatalogCreate {
    pub qualification_code: String,
    pub qualification_name: String,
    pub description: Option<String>,
    #[serde(default = "default_true_flag")]
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DepartmentQualificationCatalogResponse {
    pub id: String,
    pub department_id: String,
    pub qualification_code: String,
    pub qualification_name: String,
    pub description: Option<String>,
    pub is_active: bool,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DepartmentQualificationLevelCreate {
    pub qualification_code: String,
    pub level_code: String,
    pub level_name: String,
    pub level_rank: i32,
    #[serde(default)]
    pub covered_level_codes: Vec<String>,
    #[serde(default = "default_true_flag")]
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DepartmentQualificationLevelResponse {
    pub id: String,
    pub department_id: String,
    pub qualification_code: String,
    pub level_code: String,
    pub level_name: String,
    pub level_rank: i32,
    #[serde(default)]
    pub covered_level_codes: Vec<String>,
    pub is_active: bool,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QualificationGrantCreate {
    pub user_id: String,
    pub qualification_code: String,
    pub level_code: String,
    pub valid_from: Option<DateTime<Utc>>,
    pub valid_to: Option<DateTime<Utc>>,
    #[serde(default = "default_qualification_grant_status_value")]
    pub status: String,
    pub source_team_id: Option<String>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QualificationGrantResponse {
    pub id: String,
    pub user_id: String,
    pub department_id: String,
    pub qualification_code: String,
    pub level_code: String,
    pub valid_from: Option<DateTime<Utc>>,
    pub valid_to: Option<DateTime<Utc>>,
    pub status: String,
    pub source_team_id: Option<String>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskTypeCrewSlotRequirementSchema {
    pub slot_code: String,
    pub qualification_code: String,
    pub min_level_code: Option<String>,
    #[serde(default = "default_required_count")]
    pub required_count: i32,
    #[serde(default = "default_true_flag")]
    pub must_be_distinct: bool,
    pub exclusive_group: Option<String>,
    pub remarks: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskTypeEquipmentRequirementSchema {
    pub slot_code: String,
    pub equipment_type_id: Option<String>,
    pub equipment_type_code: Option<String>,
    #[serde(default = "default_required_count")]
    pub required_count: i32,
    #[serde(default = "default_true_flag")]
    pub must_be_distinct: bool,
    #[serde(default)]
    pub requires_driver: bool,
    pub driver_qualification_code: Option<String>,
    pub driver_min_level_code: Option<String>,
    pub remarks: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnaroundSlotPairSchema {
    pub inbound_slot_code: String,
    pub outbound_slot_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnaroundContinuityRuleSchema {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_leg_scope_outbound")]
    pub counterpart_leg_scope: String,
    pub counterpart_task_type: String,
    #[serde(default)]
    pub slot_pairs: Vec<TurnaroundSlotPairSchema>,
    #[serde(default = "default_turnaround_constraint_mode")]
    pub constraint_mode: String,
    pub tight_threshold_minutes: Option<i32>,
    pub relax_threshold_minutes: Option<i32>,
    #[serde(default)]
    pub flight_filters: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub aircraft_type_filters: Vec<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DepartmentTaskTypeRequirementDraftCreate {
    pub task_type: String,
    #[serde(default)]
    pub requirements: Vec<TaskTypeCrewSlotRequirementSchema>,
    #[serde(default)]
    pub crew_requirements: Vec<TaskTypeCrewSlotRequirementSchema>,
    #[serde(default)]
    pub equipment_requirements: Vec<TaskTypeEquipmentRequirementSchema>,
    #[serde(default)]
    pub turnaround_continuity_rules: Vec<TurnaroundContinuityRuleSchema>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DepartmentTaskTypeRequirementPublishRequest {
    pub task_type: String,
    pub draft_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DepartmentTaskTypeRequirementVersionResponse {
    pub id: String,
    pub department_id: String,
    pub task_type: String,
    pub version_no: i32,
    pub status: String,
    #[serde(default)]
    pub requirements: Vec<TaskTypeCrewSlotRequirementSchema>,
    #[serde(default)]
    pub crew_requirements: Vec<TaskTypeCrewSlotRequirementSchema>,
    #[serde(default)]
    pub equipment_requirements: Vec<TaskTypeEquipmentRequirementSchema>,
    #[serde(default)]
    pub turnaround_continuity_rules: Vec<TurnaroundContinuityRuleSchema>,
    pub notes: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DepartmentTaskTypeRequirementPublishResponse {
    pub published_version: DepartmentTaskTypeRequirementVersionResponse,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FlightGenerationRuleCreate {
    pub rule_id: Option<String>,
    pub rule_name: Option<String>,
    pub task_type: String,
    pub leg_scope: String,
    #[serde(default = "default_department_rule_status_value")]
    pub status: String,
    #[serde(default)]
    pub conditions: HashMap<String, serde_json::Value>,
    /// 生成时间锚点。允许 `scheduled_time`，或
    /// `actual|estimated|scheduled` 与 `arrival|departure` 的明确组合。
    #[serde(default = "default_generation_anchor_type")]
    pub generation_anchor_type: String,
    #[serde(default)]
    pub start_offset_minutes: i32,
    /// 预计完成时间模式：`start_plus_duration` 或 `completion_anchor_offset`。
    #[serde(default = "default_completion_time_mode")]
    pub completion_time_mode: String,
    /// 完成锚点模式下必填；使用与开始锚点相同的明确锚点词表。
    pub completion_anchor_type: Option<String>,
    /// 完成锚点模式下必填，可为负数。
    pub completion_offset_minutes: Option<i32>,
    pub duration_minutes: Option<i32>,
    /// 重排时该作业开始时间允许后滑的分钟数;省略/null 表示未配置,回退系统默认 5 分钟。
    #[serde(default)]
    pub start_flex_minutes: Option<i32>,
    /// 人数->作业时长(分钟)映射,如 `{"1":45,"2":30,"3":25}`;
    /// 省略/null 表示未配置,回退 `duration_minutes` 常量。非法条目会被保存接口拒绝。
    #[serde(default)]
    pub duration_by_crew_size: Option<serde_json::Value>,
    /// 预排冲突预警提前量(分钟),0..60;省略/null 表示未配置,回退系统默认 5 分钟。
    #[serde(default)]
    pub completion_warning_lead_minutes: Option<i32>,
    #[serde(default = "default_dispatch_publication_state_value")]
    pub publication_state: String,
    #[serde(default = "default_publish_trigger_mode_value")]
    pub publish_trigger_mode: String,
    pub publish_offset_minutes: Option<i32>,
    pub publish_event_code: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FlightGenerationRuleResponse {
    pub id: String,
    pub department_id: String,
    pub task_type: String,
    pub leg_scope: String,
    pub version_no: i32,
    pub status: String,
    pub rule_name: Option<String>,
    #[serde(default)]
    pub conditions: HashMap<String, serde_json::Value>,
    pub generation_anchor_type: String,
    pub start_offset_minutes: i32,
    pub completion_time_mode: String,
    pub completion_anchor_type: Option<String>,
    pub completion_offset_minutes: Option<i32>,
    pub duration_minutes: Option<i32>,
    pub start_flex_minutes: Option<i32>,
    pub duration_by_crew_size: Option<serde_json::Value>,
    pub completion_warning_lead_minutes: Option<i32>,
    pub publication_state: String,
    pub publish_trigger_mode: String,
    pub publish_at: Option<DateTime<Utc>>,
    pub publish_offset_minutes: Option<i32>,
    pub publish_event_code: Option<String>,
    pub notes: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GenerationAdjustmentRuleCreate {
    pub rule_id: Option<String>,
    pub rule_name: Option<String>,
    pub task_type: String,
    #[serde(default = "default_department_rule_status_value")]
    pub status: String,
    #[serde(default)]
    pub conditions: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub actions: Vec<serde_json::Value>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GenerationAdjustmentRuleResponse {
    pub id: String,
    pub department_id: String,
    pub task_type: String,
    pub version_no: i32,
    pub status: String,
    pub rule_name: Option<String>,
    #[serde(default)]
    pub conditions: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub actions: Vec<serde_json::Value>,
    pub notes: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TemporaryTaskTemplateCreate {
    pub template_code: String,
    pub template_name: String,
    pub task_type: String,
    #[serde(default)]
    pub crew_requirements: Vec<TaskTypeCrewSlotRequirementSchema>,
    #[serde(default)]
    pub equipment_requirements: Vec<TaskTypeEquipmentRequirementSchema>,
    pub notes: Option<String>,
    #[serde(default = "default_true_flag")]
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TemporaryTaskTemplateResponse {
    pub id: String,
    pub department_id: String,
    pub template_code: String,
    pub template_name: String,
    pub task_type: String,
    #[serde(default)]
    pub crew_requirements: Vec<TaskTypeCrewSlotRequirementSchema>,
    #[serde(default)]
    pub equipment_requirements: Vec<TaskTypeEquipmentRequirementSchema>,
    pub notes: Option<String>,
    pub is_active: bool,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DispatchRuleValidationRequest {
    pub generation_rule: Option<FlightGenerationRuleCreate>,
    pub adjustment_rule: Option<GenerationAdjustmentRuleCreate>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DispatchRuleValidationResponse {
    pub valid: bool,
    #[serde(default)]
    pub conflicts: Vec<serde_json::Value>,
    #[serde(default)]
    pub messages: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DispatchRulePreviewRequest {
    pub flight_id: Option<String>,
    #[serde(default)]
    pub sample_flight: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DispatchRulePreviewResponse {
    #[serde(default)]
    pub generated_orders: Vec<serde_json::Value>,
    #[serde(default)]
    pub applied_adjustments: Vec<serde_json::Value>,
    #[serde(default)]
    pub turnaround_constraints: Vec<serde_json::Value>,
    #[serde(default)]
    pub conflicts: Vec<serde_json::Value>,
    #[serde(default)]
    pub blocking_errors: Vec<String>,
}

// ---------------------------------------------------------------------------
// 场景预览
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct DispatchScenarioDelayItem {
    pub dispatch_order_id: String,
    pub delay_minutes: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DispatchScenarioPreviewRequest {
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    #[serde(default)]
    pub equipment_unavailable_ids: Vec<String>,
    #[serde(default)]
    pub closed_stand_ids: Vec<String>,
    #[serde(default)]
    pub delayed_orders: Vec<DispatchScenarioDelayItem>,
    #[serde(default)]
    pub frozen_order_ids: Vec<String>,
}

fn default_true_flag() -> bool {
    true
}

fn default_shift_status() -> String {
    "scheduled".to_string()
}

fn default_qualification_grant_status_value() -> String {
    "active".to_string()
}

fn default_department_rule_status_value() -> String {
    "draft".to_string()
}

fn default_generation_anchor_type() -> String {
    "scheduled_time".to_string()
}

fn default_completion_time_mode() -> String {
    "start_plus_duration".to_string()
}

fn default_dispatch_publication_state_value() -> String {
    "prepublished".to_string()
}

fn default_publish_trigger_mode_value() -> String {
    "time".to_string()
}

fn default_leg_scope_outbound() -> String {
    "outbound".to_string()
}

fn default_turnaround_constraint_mode() -> String {
    "disabled".to_string()
}

fn default_trigger_offset() -> i32 {
    30
}

fn default_trigger_type() -> String {
    "before_eta".to_string()
}

fn default_required_count() -> i32 {
    1
}

// ---------------------------------------------------------------------------
// 设备类型
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct EquipmentTypeCreate {
    pub name: String,
    pub code: Option<String>,
    pub category: Option<String>,
    #[serde(default)]
    pub requires_driver: bool,
    pub icon: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct EquipmentTypeUpdate {
    pub name: Option<String>,
    pub code: Option<String>,
    pub category: Option<String>,
    pub requires_driver: Option<bool>,
    pub icon: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EquipmentTypeResponse {
    pub id: String,
    pub name: String,
    pub code: Option<String>,
    pub category: Option<String>,
    pub requires_driver: bool,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub is_active: bool,
}

// ---------------------------------------------------------------------------
// 设备
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct EquipmentCreate {
    pub code: String,
    pub equipment_type_id: Option<String>,
    /// 所属科室，创建必填（PR2 起设备直接挂科室）。
    pub department_id: String,
    pub name: Option<String>,
    pub license_plate: Option<String>,
    pub next_maintenance_date: Option<NaiveDate>,
    // PR2 起不再接受 terminal（设备无常驻楼字段）。
}

#[derive(Debug, Clone, Deserialize)]
pub struct EquipmentUpdate {
    pub code: Option<String>,
    pub equipment_type_id: Option<String>,
    pub department_id: Option<String>,
    pub name: Option<String>,
    pub license_plate: Option<String>,
    pub status: Option<String>,
    pub next_maintenance_date: Option<NaiveDate>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EquipmentStatusUpdate {
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EquipmentResponse {
    pub id: String,
    pub code: String,
    pub equipment_type_id: Option<String>,
    pub department_id: Option<String>,
    pub name: Option<String>,
    pub license_plate: Option<String>,
    pub status: String,
    pub current_position: Option<PositionSchema>,
    pub current_stand_id: Option<String>,
    pub last_position_update: Option<DateTime<Utc>>,
    pub next_maintenance_date: Option<NaiveDate>,
    pub current_dispatch_id: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub is_active: bool,
    pub equipment_type_name: Option<String>,
}

// ---------------------------------------------------------------------------
// 机位
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct StandCreate {
    pub code: String,
    pub name: Option<String>,
    pub terminal: Option<String>,
    pub area: Option<String>,
    pub position_lat: f64,
    pub position_lng: f64,
    pub stand_type: Option<String>,
    pub size_category: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StandResponse {
    pub id: String,
    pub code: String,
    pub name: Option<String>,
    pub terminal: Option<String>,
    pub area: Option<String>,
    pub position: PositionSchema,
    pub stand_type: Option<String>,
    pub size_category: Option<String>,
    pub is_active: bool,
}

// ---------------------------------------------------------------------------
// 作业类型
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct TaskTypeCreate {
    pub code: String,
    pub name: String,
    pub default_department_id: Option<String>,
    pub category: Option<String>,
    pub sequence_order: Option<i32>,
    pub default_duration_minutes: Option<i32>,
    #[serde(default = "default_trigger_offset")]
    pub trigger_offset_minutes: i32,
    #[serde(default = "default_trigger_type")]
    pub trigger_type: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskTypeResponse {
    pub id: String,
    pub code: String,
    pub name: String,
    pub default_department_id: Option<String>,
    pub category: Option<String>,
    pub sequence_order: Option<i32>,
    pub default_duration_minutes: Option<i32>,
    pub trigger_offset_minutes: i32,
    pub trigger_type: String,
    pub description: Option<String>,
    pub is_active: bool,
}

// ---------------------------------------------------------------------------
// 派工单
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct DispatchOrderCreate {
    pub flight_id: Option<String>,
    pub task_type: Option<String>,
    pub temporary_task_template_code: Option<String>,
    pub department_id: Option<String>,
    pub stand_id: Option<String>,
    pub location: Option<String>,
    pub individual_user_id: Option<String>,
    pub planned_start_time: Option<DateTime<Utc>>,
    pub planned_end_time: Option<DateTime<Utc>>,
    pub priority: Option<i32>,
    #[serde(default)]
    pub workflow_context: HashMap<String, serde_json::Value>,
    #[serde(default = "default_dispatch_order_publication_state")]
    pub publication_state: String,
    #[serde(default = "default_dispatch_order_source_type")]
    pub source_type: String,
    #[serde(default = "default_dispatch_order_leg_scope")]
    pub leg_scope: String,
    #[serde(default)]
    pub crew_requirement_snapshot: Vec<serde_json::Value>,
    #[serde(default)]
    pub equipment_requirement_snapshot: Vec<serde_json::Value>,
    #[serde(default)]
    pub task_crew: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub equipment_assignment: Vec<serde_json::Value>,
    #[serde(default)]
    pub manual_lock: bool,
    pub remarks: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DispatchOrderStart {
    pub actual_start_time: Option<DateTime<Utc>>,
    pub position: Option<PositionSchema>,
    pub notes: Option<String>,
    pub client_action_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DispatchOrderCompleteReq {
    pub actual_end_time: Option<DateTime<Utc>>,
    pub position: Option<PositionSchema>,
    pub completion_notes: Option<String>,
    #[serde(default)]
    pub issues: Vec<String>,
    pub client_action_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DispatchOrderAcceptRequest {
    pub note: Option<String>,
    pub client_action_id: Option<String>,
}

impl DispatchOrderAcceptRequest {
    pub fn validate(&self) -> Result<(), String> {
        validate_optional_length(self.client_action_id.as_deref(), 64, "client_action_id")
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct DispatchOrderCancelRequest {
    pub reason: Option<String>,
    pub client_action_id: Option<String>,
}

impl DispatchOrderCancelRequest {
    pub fn validate(&self) -> Result<(), String> {
        validate_optional_length(self.reason.as_deref(), 500, "reason")?;
        validate_optional_length(self.client_action_id.as_deref(), 64, "client_action_id")
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct DispatchOrderCheckInRequest {
    pub qr_code: Option<String>,
    pub lat: Option<f64>,
    pub lng: Option<f64>,
    pub accuracy_m: Option<f64>,
    pub note: Option<String>,
    pub client_action_id: Option<String>,
}

impl DispatchOrderCheckInRequest {
    pub fn validate(&self) -> Result<(), String> {
        validate_optional_length(self.qr_code.as_deref(), 128, "qr_code")?;
        validate_optional_length(self.client_action_id.as_deref(), 64, "client_action_id")?;
        validate_optional_coordinate(self.lat, "lat")?;
        validate_optional_coordinate(self.lng, "lng")?;
        if let Some(accuracy_m) = self.accuracy_m {
            if accuracy_m < 0.0 {
                return Err("accuracy_m 不能小于 0".to_string());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct DispatchOrderCheckOutRequest {
    pub lat: Option<f64>,
    pub lng: Option<f64>,
    pub note: Option<String>,
    pub client_action_id: Option<String>,
    pub recorded_at: Option<DateTime<Utc>>,
}

impl DispatchOrderCheckOutRequest {
    pub fn validate(&self) -> Result<(), String> {
        validate_optional_length(self.client_action_id.as_deref(), 64, "client_action_id")?;
        validate_optional_coordinate(self.lat, "lat")?;
        validate_optional_coordinate(self.lng, "lng")?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DispatchOrderMemberResponse {
    pub id: String,
    pub user_id: String,
    pub role: String,
    pub source_type: String,
    pub source_team_id: Option<String>,
    pub slot_code: Option<String>,
    pub qualification_code: Option<String>,
    pub qualification_level_code: Option<String>,
    pub assigned_at: Option<DateTime<Utc>>,
    pub check_in_time: Option<DateTime<Utc>>,
    pub check_out_time: Option<DateTime<Utc>>,
    pub is_active: bool,
    pub username: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DispatchOrderResponse {
    pub id: String,
    pub flight_id: String,
    pub task_type: String,
    pub task_type_name: Option<String>,
    pub stand_id: Option<String>,
    pub stand_code: Option<String>,
    pub terminal: Option<String>,
    pub department: Option<String>,
    pub individual_user_id: Option<String>,
    pub individual_username: Option<String>,
    pub driver_type: Option<String>,
    pub driver_user_id: Option<String>,
    pub driver_assignment: Option<HashMap<String, serde_json::Value>>,
    pub planned_start_time: Option<DateTime<Utc>>,
    pub planned_end_time: Option<DateTime<Utc>>,
    pub actual_start_time: Option<DateTime<Utc>>,
    pub actual_end_time: Option<DateTime<Utc>>,
    pub estimated_completion_time: Option<DateTime<Utc>>,
    pub estimated_completion_reported_by: Option<String>,
    pub estimated_completion_reported_at: Option<DateTime<Utc>>,
    pub estimated_completion_note: Option<String>,
    pub effective_start_time: Option<DateTime<Utc>>,
    pub effective_end_time: Option<DateTime<Utc>>,
    pub effective_end_source: Option<String>,
    pub gate: Option<String>,
    pub status: String,
    pub dispatch_type: String,
    pub dispatched_at: Option<DateTime<Utc>>,
    pub estimated_arrival_minutes: Option<i32>,
    pub source: String,
    pub schedule_source: String,
    pub lock_level: String,
    #[serde(default = "default_dispatch_order_publication_state")]
    pub publication_state: String,
    #[serde(default = "default_dispatch_order_source_type")]
    pub source_type: String,
    pub department_id: Option<String>,
    #[serde(default = "default_dispatch_order_leg_scope")]
    pub leg_scope: String,
    pub generation_rule_id: Option<String>,
    pub generation_rule_version: Option<i32>,
    pub generation_anchor_type: Option<String>,
    pub generation_anchor_time: Option<DateTime<Utc>>,
    pub completion_time_mode: Option<String>,
    pub completion_anchor_type: Option<String>,
    pub completion_anchor_time: Option<DateTime<Utc>>,
    pub completion_offset_minutes: Option<i32>,
    pub completion_warning_lead_minutes: Option<i32>,
    pub publish_trigger_mode: Option<String>,
    pub publish_at: Option<DateTime<Utc>>,
    pub turnaround_pair_key: Option<String>,
    pub turnaround_constraint_mode: Option<String>,
    pub department_rule_version: Option<String>,
    #[serde(default)]
    pub crew_requirement_snapshot: Vec<serde_json::Value>,
    #[serde(default)]
    pub equipment_requirement_snapshot: Vec<serde_json::Value>,
    pub task_crew: Option<TaskCrewResponse>,
    #[serde(default)]
    pub equipment_assignment: Vec<serde_json::Value>,
    #[serde(default)]
    pub qualification_gap: Vec<serde_json::Value>,
    #[serde(default)]
    pub equipment_gap: Vec<serde_json::Value>,
    pub availability_reason: Option<String>,
    #[serde(default)]
    pub score_breakdown: HashMap<String, serde_json::Value>,
    pub conflict_reason: Option<String>,
    pub origin_type: String,
    pub origin_label: String,
    pub process_instance_id: Option<String>,
    pub process_task_id: Option<String>,
    pub workflow_status: Option<String>,
    #[serde(default)]
    pub workflow_context: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub recommended_assignees: Vec<DispatchRecommendationItem>,
    pub recommendation_score: Option<f64>,
    pub supervisor_notified: bool,
    pub supervisor_notified_at: Option<DateTime<Utc>>,
    pub assignment_deadline: Option<DateTime<Utc>>,
    pub completion_notes: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub members: Vec<DispatchOrderMemberResponse>,
    #[serde(default)]
    pub equipment_codes: Vec<String>,
    #[serde(default)]
    pub notification_receipt_summary: HashMap<String, serde_json::Value>,
}

fn default_dispatch_order_publication_state() -> String {
    "published".to_string()
}

fn default_dispatch_order_source_type() -> String {
    "manual".to_string()
}

fn default_dispatch_order_leg_scope() -> String {
    "none".to_string()
}

fn default_member_role() -> String {
    "member".to_string()
}

// ---------------------------------------------------------------------------
// 自动派工
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct AutoDispatchRequest {
    pub flight_ids: Option<Vec<String>>,
    pub task_types: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutoDispatchResult {
    pub success_count: i32,
    pub failed_count: i32,
    #[serde(default)]
    pub created_orders: Vec<String>,
    #[serde(default)]
    pub alerts: Vec<String>,
}

// ---------------------------------------------------------------------------
// 流程驱动派工
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct WorkflowDispatchCreateRequest {
    pub process_instance_id: String,
    pub process_task_id: String,
    pub process_definition_key: Option<String>,
    pub business_key: Option<String>,
    pub flight_id: String,
    pub task_type: String,
    pub stand_id: Option<String>,
    pub planned_start_time: Option<DateTime<Utc>>,
    pub planned_end_time: Option<DateTime<Utc>>,
    pub assignment_deadline: Option<DateTime<Utc>>,
    pub target_department: String,
    #[serde(default = "default_supervisor_title")]
    pub target_job_title: Option<String>,
    #[serde(default = "default_required_people")]
    pub required_people: i32,
    #[serde(default = "default_priority")]
    pub priority: String,
    pub description: Option<String>,
    #[serde(default)]
    pub context: serde_json::Map<String, serde_json::Value>,
}

impl WorkflowDispatchCreateRequest {
    pub fn validate(&self) -> Result<(), Vec<serde_json::Value>> {
        let mut detail = Vec::new();
        if self.required_people < 1 || self.required_people > 20 {
            if self.required_people < 1 {
                detail.push(serde_json::json!({
                    "type": "greater_than_equal",
                    "loc": ["body", "required_people"],
                    "msg": "Input should be greater than or equal to 1",
                    "input": self.required_people,
                    "ctx": { "ge": 1 }
                }));
            } else {
                detail.push(serde_json::json!({
                    "type": "less_than_equal",
                    "loc": ["body", "required_people"],
                    "msg": "Input should be less than or equal to 20",
                    "input": self.required_people,
                    "ctx": { "le": 20 }
                }));
            }
        }
        if detail.is_empty() {
            Ok(())
        } else {
            Err(detail)
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkflowDispatchAssignRequest {
    pub assigned_user_ids: Vec<String>,
    pub notes: Option<String>,
    #[serde(default = "default_true_bool")]
    pub complete_process_task: bool,
}

impl WorkflowDispatchAssignRequest {
    pub fn validate(&self) -> Result<(), Vec<serde_json::Value>> {
        if self.assigned_user_ids.is_empty() {
            return Err(vec![serde_json::json!({
                "type": "too_short",
                "loc": ["body", "assigned_user_ids"],
                "msg": "List should have at least 1 item after validation, not 0",
                "input": [],
                "ctx": {
                    "field_type": "List",
                    "min_length": 1,
                    "actual_length": 0
                }
            })]);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DispatchRecommendationItem {
    pub user_id: String,
    pub username: String,
    pub status: String,
    pub department: Option<String>,
    pub job_title: Option<String>,
    pub score: f64,
    pub reason: String,
    #[serde(default)]
    pub workload: i32,
}

fn default_supervisor_title() -> Option<String> {
    Some("主管".to_string())
}

fn default_required_people() -> i32 {
    1
}

fn default_priority() -> String {
    "normal".to_string()
}

fn default_true_bool() -> bool {
    true
}

// ---------------------------------------------------------------------------
// 命令侧请求 DTO
// ---------------------------------------------------------------------------

/// ETA 回报
#[derive(Debug, Clone, Deserialize)]
pub struct EtaReportRequest {
    pub estimated_completion_time: chrono::DateTime<chrono::Utc>,
    pub note: Option<String>,
    pub client_action_id: Option<String>,
}

impl EtaReportRequest {
    pub fn validate(&self) -> Result<(), String> {
        validate_optional_length(self.note.as_deref(), 500, "note")?;
        validate_optional_length(self.client_action_id.as_deref(), 64, "client_action_id")
    }
}

/// 异常上报
#[derive(Debug, Clone, Deserialize)]
pub struct ReportIssueRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub severity: Option<String>,
    pub issue_type: Option<String>,
    pub note: Option<String>,
    pub lat: Option<f64>,
    pub lng: Option<f64>,
    pub attachments: Option<Vec<String>>,
    pub client_action_id: Option<String>,
    #[serde(default = "default_issue_input_mode")]
    pub input_mode: String,
    pub voice_attachment_id: Option<String>,
}

impl ReportIssueRequest {
    pub fn validate(&self) -> Result<(), String> {
        validate_optional_length(self.title.as_deref(), 200, "title")?;
        validate_optional_length(self.issue_type.as_deref(), 64, "issue_type")?;
        validate_optional_length(self.voice_attachment_id.as_deref(), 128, "voice_attachment_id")?;
        validate_optional_length(self.client_action_id.as_deref(), 64, "client_action_id")?;
        validate_optional_coordinate(self.lat, "lat")?;
        validate_optional_coordinate(self.lng, "lng")?;

        if self.title.as_deref().is_some_and(|value| value.trim().is_empty()) {
            return Err("title 不能为空字符串".to_string());
        }

        let severity = self
            .severity
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_lowercase());
        if let Some(severity) = severity.as_deref() {
            if !matches!(severity, "low" | "medium" | "high" | "critical") {
                return Err(format!("无效异常级别: {severity}"));
            }
        }

        let input_mode = self.input_mode.trim().to_ascii_lowercase();
        if !matches!(input_mode.as_str(), "text" | "photo" | "voice") {
            return Err("input_mode 必须是 text/photo/voice 之一".to_string());
        }

        let has_text = self
            .title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some()
            || self
                .description
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_some()
            || self
                .note
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_some();
        let has_attachment = self
            .attachments
            .as_ref()
            .map(|items| items.iter().any(|item| !item.trim().is_empty()))
            .unwrap_or(false)
            || self
                .voice_attachment_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_some();

        if !has_text && !has_attachment {
            return Err("至少提供文本、附件或语音首报之一".to_string());
        }
        Ok(())
    }
}

fn default_issue_input_mode() -> String {
    "text".to_string()
}

fn validate_optional_length(value: Option<&str>, max: usize, field: &str) -> Result<(), String> {
    let Some(value) = value else {
        return Ok(());
    };
    if value.chars().count() <= max {
        Ok(())
    } else {
        Err(format!("{field} 长度不能超过 {max} 个字符"))
    }
}

fn validate_optional_coordinate(value: Option<f64>, field: &str) -> Result<(), String> {
    let Some(value) = value else {
        return Ok(());
    };
    let valid = match field {
        "lat" => (-90.0..=90.0).contains(&value),
        "lng" => (-180.0..=180.0).contains(&value),
        _ => true,
    };
    if valid {
        Ok(())
    } else {
        Err(format!("{field} 超出允许范围"))
    }
}

/// 移动端离线动作同步
#[derive(Debug, Clone, Deserialize)]
pub struct MobileSyncRequest {
    pub actions: Vec<MobileSyncAction>,
}

impl MobileSyncRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.actions.len() > 500 {
            return Err("actions 数量不能超过 500".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct MobileSyncAction {
    #[serde(deserialize_with = "deserialize_mobile_sync_action_type")]
    pub action_type: String,
    pub dispatch_order_id: String,
    #[serde(deserialize_with = "deserialize_required_non_empty_string")]
    pub client_action_id: String,
    pub action_timestamp: Option<String>,
    pub payload: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileSyncActionResult {
    pub client_action_id: String,
    pub dispatch_order_id: String,
    pub action_type: String,
    pub status: String,
    pub message: String,
    pub server_timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileSyncResponse {
    pub total: i64,
    pub applied: i64,
    pub duplicates: i64,
    pub failed: i64,
    #[serde(default)]
    pub results: Vec<MobileSyncActionResult>,
}

/// 安全检查清单项提交
#[derive(Debug, Clone, Deserialize)]
pub struct SafetyChecklistItemRequest {
    pub result: Option<String>,
    pub note: Option<String>,
    #[serde(default)]
    pub handled_on_site: bool,
}

/// 安全检查清单批量提交项
#[derive(Debug, Clone, Deserialize)]
pub struct DispatchSafetyChecklistBatchItemRequest {
    pub item_code: String,
    pub result: String,
    pub note: Option<String>,
    #[serde(default)]
    pub handled_on_site: bool,
}

/// 安全检查清单批量提交
#[derive(Debug, Clone, Deserialize)]
pub struct DispatchSafetyChecklistBatchSubmitRequest {
    pub items: Vec<DispatchSafetyChecklistBatchItemRequest>,
}

/// 派工验证请求
#[derive(Debug, Clone, Deserialize)]
pub struct ValidateOrderRequest {
    pub flight_id: Option<String>,
    pub task_type: Option<String>,
    pub dispatch_order_id: Option<String>,
    pub stand_id: Option<String>,
    pub individual_user_id: Option<String>,
    #[serde(default)]
    pub equipment_ids: Vec<String>,
    pub planned_start_time: chrono::DateTime<chrono::Utc>,
    pub planned_end_time: chrono::DateTime<chrono::Utc>,
}

fn deserialize_required_non_empty_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(serde::de::Error::custom("field is required"));
    }
    Ok(normalized.to_string())
}

fn deserialize_mobile_sync_action_type<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = deserialize_required_non_empty_string(deserializer)?;
    if MOBILE_SYNC_ACTION_TYPES.contains(&value.as_str()) {
        return Ok(value);
    }
    Err(serde::de::Error::custom(format!("unsupported action_type: {value}")))
}

/// 设备注册请求
#[derive(Debug, Clone, Deserialize)]
pub struct DeviceRegisterRequest {
    pub device_id: String,
    pub platform: Option<String>,
    pub push_channel: Option<String>,
    pub push_token: Option<String>,
    pub app_version: Option<String>,
    pub os_version: Option<String>,
    pub device_model: Option<String>,
    pub manufacturer: Option<String>,
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

/// 设备心跳请求
#[derive(Debug, Clone, Deserialize)]
pub struct DeviceHeartbeatRequest {
    pub network_status: Option<String>,
    pub battery_level: Option<i32>,
    pub sse_reconnected: Option<bool>,
    pub sse_reconnect_count: Option<i32>,
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

/// 安全检查清单模板更新请求
#[derive(Debug, Clone, Deserialize)]
pub struct SafetyTemplateUpsertRequest {
    pub checklist_version: String,
    pub checklist_items: Vec<serde_json::Value>,
    #[serde(default = "default_true")]
    pub is_active: bool,
}
fn default_true() -> bool {
    true
}

/// 安全检查清单批量进度查询
#[derive(Debug, Clone, Deserialize)]
pub struct SafetyChecklistProgressRequest {
    pub orders: Vec<SafetyChecklistProgressOrderItem>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SafetyChecklistProgressOrderItem {
    pub dispatch_order_id: String,
    pub task_type: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DispatchFollowupQueueQuery {
    pub assignee: Option<String>,
    pub source_type: Option<String>,
    #[serde(default = "default_followup_limit")]
    pub limit: i64,
}

fn default_followup_limit() -> i64 {
    50
}

/// 重规划请求
#[derive(Debug, Clone, Deserialize)]
pub struct ReplanRequest {
    pub window_start: chrono::DateTime<chrono::Utc>,
    pub window_end: chrono::DateTime<chrono::Utc>,
    #[serde(default = "default_replan_strategy")]
    pub strategy: String,
    #[serde(default)]
    pub apply_changes: bool,
    pub max_suggestions: Option<i64>,
}

impl ReplanRequest {
    pub fn normalize(mut self) -> Result<Self, String> {
        self.strategy = normalize_frontend_replan_strategy(&self.strategy)?;
        if let Some(max_suggestions) = self.max_suggestions {
            if !(1..=500).contains(&max_suggestions) {
                return Err("max_suggestions 必须在 1 到 500 之间".to_string());
            }
        }
        Ok(self)
    }
}

fn default_replan_strategy() -> String {
    default_frontend_replan_strategy()
}

fn default_frontend_replan_strategy() -> String {
    "balanced".to_string()
}

pub fn allowed_frontend_replan_strategies() -> &'static [&'static str] {
    &["stability", "balanced", "efficiency"]
}

pub fn normalize_frontend_replan_strategy(value: &str) -> Result<String, String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Ok(default_frontend_replan_strategy());
    }
    if allowed_frontend_replan_strategies()
        .iter()
        .any(|candidate| *candidate == normalized)
    {
        return Ok(normalized);
    }
    Err(format!(
        "strategy 必须是 {} 之一",
        allowed_frontend_replan_strategies().join(", ")
    ))
}

fn default_frontend_replan_max_suggestions() -> i64 {
    20
}

#[derive(Debug, Clone, Deserialize)]
pub struct DispatchReplanSnapshotQuery {
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    #[serde(default = "default_frontend_replan_strategy")]
    pub strategy: String,
    #[serde(default = "default_frontend_replan_max_suggestions")]
    pub max_suggestions: i64,
}

impl DispatchReplanSnapshotQuery {
    pub fn normalize(mut self) -> Result<Self, String> {
        self.strategy = normalize_frontend_replan_strategy(&self.strategy)?;
        if !(1..=500).contains(&self.max_suggestions) {
            return Err("max_suggestions 必须在 1 到 500 之间".to_string());
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct TaskCrewMemberResponse {
    pub user_id: String,
    pub username: Option<String>,
    pub source_team_id: Option<String>,
    pub source_team_name: Option<String>,
    pub slot_code: Option<String>,
    pub qualification_code: Option<String>,
    pub qualification_level_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskCrewResponse {
    #[serde(default)]
    pub members: Vec<TaskCrewMemberResponse>,
    #[serde(default)]
    pub source_team_ids: Vec<String>,
    #[serde(default)]
    pub source_team_names: Vec<String>,
    #[serde(default = "default_task_crew_generated_from")]
    pub generated_from: String,
}

impl Default for TaskCrewResponse {
    fn default() -> Self {
        Self {
            members: Vec::new(),
            source_team_ids: Vec::new(),
            source_team_names: Vec::new(),
            generated_from: default_task_crew_generated_from(),
        }
    }
}

fn default_task_crew_generated_from() -> String {
    "auto".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct DispatchReplanAssignment {
    pub individual_user_id: Option<String>,
    #[serde(default)]
    pub equipment_ids: Vec<String>,
    #[serde(default)]
    pub member_user_ids: Vec<String>,
    pub department_rule_version: Option<String>,
    #[serde(default)]
    pub crew_requirement_snapshot: Vec<serde_json::Value>,
    #[serde(default)]
    pub equipment_requirement_snapshot: Vec<serde_json::Value>,
    #[serde(default)]
    pub qualification_gap: Vec<serde_json::Value>,
    #[serde(default)]
    pub task_crew: TaskCrewResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchReplanCandidateUser {
    pub user_id: String,
    pub username: String,
    #[serde(default)]
    pub score: f64,
    pub source_team_id: Option<String>,
    pub source_team_name: Option<String>,
    pub qualification_code: Option<String>,
    pub qualification_level_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchReplanCandidateTeam {
    pub team_id: String,
    pub team_name: String,
    pub team_type_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchReplanCandidateEquipment {
    pub equipment_id: String,
    pub code: String,
    pub equipment_type_id: Option<String>,
    pub equipment_type_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct DispatchReplanPersonnelSlot {
    pub slot_code: String,
    pub qualification_code: Option<String>,
    pub qualification_level_code: Option<String>,
    #[serde(default)]
    pub qualification_feasible_candidate_user_ids: Vec<String>,
    #[serde(default)]
    pub schedule_feasible_candidate_user_ids: Vec<String>,
    #[serde(default)]
    pub candidate_user_ids: Vec<String>,
    /// People attached to the order who do **not** hold this slot's
    /// qualification. Excluded from `candidate_user_ids` on purpose; reported so
    /// a thin slot reads as "these people were ruled out" rather than as an
    /// unexplained gap.
    #[serde(default)]
    pub qualification_excluded_user_ids: Vec<String>,
    /// Teams the slot's candidates were granted their qualification through, for
    /// "from team X" attribution on the board. Metadata only — the solver
    /// decides over people, not teams.
    #[serde(default)]
    pub candidate_source_team_ids: Vec<String>,
    pub baseline_user_id: Option<String>,
    #[serde(default = "default_slot_workload_weight")]
    pub workload_weight: f64,
    #[serde(default)]
    pub scarcity_cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct DispatchReplanEquipmentSlot {
    pub slot_code: String,
    pub equipment_type_id: Option<String>,
    #[serde(default)]
    pub schedule_feasible_candidate_equipment_ids: Vec<String>,
    #[serde(default)]
    pub candidate_equipment_ids: Vec<String>,
    pub baseline_equipment_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct DispatchReplanBaselinePersonnelSlotAssignment {
    pub slot_code: String,
    pub user_id: Option<String>,
    pub username: Option<String>,
    pub source_team_id: Option<String>,
    pub source_team_name: Option<String>,
    pub qualification_code: Option<String>,
    pub qualification_level_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct DispatchReplanBaselineEquipmentSlotAssignment {
    pub slot_code: String,
    pub equipment_id: Option<String>,
    pub code: Option<String>,
    pub equipment_type_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct DispatchReplanBaselineAssignment {
    pub individual_user_id: Option<String>,
    #[serde(default)]
    pub equipment_ids: Vec<String>,
    #[serde(default)]
    pub member_user_ids: Vec<String>,
    pub department_rule_version: Option<String>,
    #[serde(default)]
    pub crew_requirement_snapshot: Vec<serde_json::Value>,
    #[serde(default)]
    pub equipment_requirement_snapshot: Vec<serde_json::Value>,
    #[serde(default)]
    pub qualification_gap: Vec<serde_json::Value>,
    #[serde(default)]
    pub task_crew: serde_json::Value,
    #[serde(default)]
    pub personnel_slot_assignments: Vec<DispatchReplanBaselinePersonnelSlotAssignment>,
    #[serde(default)]
    pub equipment_slot_assignments: Vec<DispatchReplanBaselineEquipmentSlotAssignment>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DispatchReplanSuggestion {
    pub dispatch_order_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub order_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flight_id: Option<String>,
    pub reason: String,
    pub suggestion_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk_level: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safety_gate_state: Option<String>,
    pub order_class: Option<String>,
    pub original_start_time: Option<DateTime<Utc>>,
    pub original_end_time: Option<DateTime<Utc>>,
    pub suggested_start_time: Option<DateTime<Utc>>,
    pub suggested_end_time: Option<DateTime<Utc>>,
    pub related_dispatch_order_id: Option<String>,
    pub current_assignment: Option<DispatchReplanAssignment>,
    pub suggested_assignment: Option<DispatchReplanAssignment>,
    pub task_crew: Option<TaskCrewResponse>,
    #[serde(default)]
    pub crew_requirement_snapshot: Vec<serde_json::Value>,
    #[serde(default)]
    pub qualification_gap: Vec<serde_json::Value>,
    pub department_rule_version: Option<String>,
    #[serde(default)]
    pub member_change_summary: serde_json::Value,
    #[serde(default)]
    pub requires_manual_confirmation: bool,
    #[serde(default)]
    pub lateness_minutes: i64,
    #[serde(default)]
    pub travel_minutes: i64,
    #[serde(default)]
    pub impact_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DispatchReplanOrderResult {
    pub dispatch_order_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub order_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flight_id: Option<String>,
    #[serde(default = "default_solver_assignment_reason")]
    pub reason: String,
    pub suggestion_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk_level: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safety_gate_state: Option<String>,
    pub order_class: Option<String>,
    pub original_start_time: Option<DateTime<Utc>>,
    pub original_end_time: Option<DateTime<Utc>>,
    pub suggested_start_time: Option<DateTime<Utc>>,
    pub suggested_end_time: Option<DateTime<Utc>>,
    #[serde(default)]
    pub lateness_minutes: i64,
    #[serde(default)]
    pub gap_count: i64,
    #[serde(default)]
    pub travel_minutes: i64,
    #[serde(default)]
    pub baseline_change_count: i64,
    #[serde(default)]
    pub impact_score: f64,
    pub current_assignment: Option<DispatchReplanAssignment>,
    pub suggested_assignment: Option<DispatchReplanAssignment>,
    pub task_crew: Option<TaskCrewResponse>,
    #[serde(default)]
    pub crew_requirement_snapshot: Vec<serde_json::Value>,
    #[serde(default)]
    pub qualification_gap: Vec<serde_json::Value>,
    #[serde(default)]
    pub member_change_summary: serde_json::Value,
    #[serde(default)]
    pub requires_manual_confirmation: bool,
    #[serde(default)]
    pub start_times: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub lateness: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub gap_summary: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub continuity_summary: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub change_summary: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub travel_summary: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub personnel_slot_assignments: Vec<serde_json::Value>,
    #[serde(default)]
    pub equipment_slot_assignments: Vec<serde_json::Value>,
    #[serde(default)]
    pub continuity_decisions: Vec<serde_json::Value>,
    #[serde(default)]
    pub objective_breakdown: HashMap<String, serde_json::Value>,
}

fn default_solver_assignment_reason() -> String {
    "solver_assignment".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DispatchReplanImpactWarning {
    pub code: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flight_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DispatchReplanImpactSummary {
    #[serde(default)]
    pub affected_order_count: i64,
    #[serde(default)]
    pub affected_flight_count: i64,
    #[serde(default)]
    pub conflicts_fixed_count: i64,
    #[serde(default)]
    pub new_assignment_count: i64,
    #[serde(default)]
    pub late_assignment_count: i64,
    #[serde(default)]
    pub locked_item_count: i64,
    #[serde(default)]
    pub high_risk_change_count: i64,
    #[serde(default)]
    pub warnings: Vec<DispatchReplanImpactWarning>,
    #[serde(default)]
    pub affected_flights: i64,
    #[serde(default)]
    pub changed_orders: i64,
    #[serde(default)]
    pub reassigned_orders: i64,
    #[serde(default)]
    pub delayed_orders: i64,
    #[serde(default)]
    pub added_delay_minutes: f64,
    #[serde(default)]
    pub replaced_member_count: i64,
    #[serde(default)]
    pub qualification_gap_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DispatchReplanSnapshotOrder {
    pub order_id: String,
    pub flight_id: String,
    pub status: String,
    #[serde(default)]
    pub is_optimizable: bool,
    #[serde(default)]
    pub is_fixed_anchor: bool,
    #[serde(default = "default_snapshot_conflict_state")]
    pub conflict_state: String,
    #[serde(default = "default_snapshot_order_class")]
    pub order_class: String,
    #[serde(default)]
    pub has_conflict: bool,
    pub planned_start_time: Option<DateTime<Utc>>,
    pub planned_end_time: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_time_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_target_time: Option<DateTime<Utc>>,
    pub earliest_start_time: Option<DateTime<Utc>>,
    pub latest_start_time: Option<DateTime<Utc>>,
    pub duration_minutes: Option<i32>,
    /// Dense `crew size -> minutes` table, index `k` being the duration when the
    /// solver fills `k` personnel slots. Expanded from the owning department's
    /// sparse config by [`resolve_duration_table`]. `None` keeps `duration_minutes`
    /// as a constant, which is the behaviour for every unconfigured department.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_by_crew_size: Option<Vec<i32>>,
    pub required_start_time: Option<DateTime<Utc>>,
    pub actual_start_time: Option<DateTime<Utc>>,
    pub actual_end_time: Option<DateTime<Utc>>,
    pub estimated_completion_time: Option<DateTime<Utc>>,
    pub effective_start_time: Option<DateTime<Utc>>,
    pub effective_end_time: Option<DateTime<Utc>>,
    pub stand_id: Option<String>,
    #[serde(default = "default_snapshot_lock_level")]
    pub lock_level: String,
    pub availability_reason: Option<String>,
    #[serde(default)]
    pub score_breakdown: HashMap<String, serde_json::Value>,
    pub conflict_reason: Option<String>,
    #[serde(default = "default_snapshot_schedule_source")]
    pub schedule_source: String,
    pub turnaround_pair_key: Option<String>,
    pub turnaround_constraint_mode: Option<String>,
    #[serde(default, skip_serializing)]
    pub leg_scope: Option<String>,
    pub current_assignment: Option<DispatchReplanAssignment>,
    #[serde(default)]
    pub baseline_assignment: DispatchReplanBaselineAssignment,
    #[serde(default)]
    pub personnel_slots: Vec<DispatchReplanPersonnelSlot>,
    #[serde(default)]
    pub equipment_slots: Vec<DispatchReplanEquipmentSlot>,
    #[serde(default, skip_serializing)]
    pub team_id: Option<String>,
    /// Owning department, carried so per-slot candidate mining can ask the
    /// qualification store who in that department holds a slot's qualification.
    /// Internal to snapshot building; the frontend never reads it.
    #[serde(default, skip_serializing)]
    pub department_id: Option<String>,
    #[serde(default, skip_serializing)]
    pub individual_user_id: Option<String>,
    #[serde(default, skip_serializing)]
    pub equipment_ids: Vec<String>,
    #[serde(default, skip_serializing)]
    pub crew_requirement_snapshot: Vec<serde_json::Value>,
    #[serde(default, skip_serializing)]
    pub equipment_requirement_snapshot: Vec<serde_json::Value>,
    #[serde(default, skip_serializing)]
    pub qualification_gap: Vec<serde_json::Value>,
    #[serde(default, skip_serializing)]
    pub candidate_users: Vec<DispatchReplanCandidateUser>,
    #[serde(default, skip_serializing)]
    pub candidate_teams: Vec<DispatchReplanCandidateTeam>,
    #[serde(default, skip_serializing)]
    pub candidate_equipments: Vec<DispatchReplanCandidateEquipment>,
    #[serde(default, skip_serializing)]
    pub candidate_assignments: Vec<DispatchReplanAssignment>,
    #[serde(default)]
    pub is_completed: bool,
    #[serde(default)]
    pub is_in_progress: bool,
    #[serde(default)]
    pub is_locked: bool,
}

fn default_snapshot_order_class() -> String {
    "locked".to_string()
}

fn default_snapshot_conflict_state() -> String {
    "none".to_string()
}

fn default_snapshot_lock_level() -> String {
    "optimizable".to_string()
}

fn default_snapshot_schedule_source() -> String {
    "current_status_fallback".to_string()
}

fn default_slot_workload_weight() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DispatchReplanTravelEdge {
    pub resource_type: String,
    pub resource_id: String,
    pub from_node: String,
    pub to_node: String,
    #[serde(default)]
    pub travel_minutes: i64,
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DispatchReplanAnchorFreeWindow {
    pub resource_type: String,
    pub resource_id: String,
    pub window_start: Option<DateTime<Utc>>,
    pub window_end: Option<DateTime<Utc>>,
    pub left_anchor_order_id: Option<String>,
    pub left_anchor_stand_id: Option<String>,
    pub right_anchor_order_id: Option<String>,
    pub right_anchor_stand_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DispatchReplanAnchorState {
    pub resource_type: String,
    pub resource_id: String,
    pub anchor_order_id: Option<String>,
    pub location_stand_id: Option<String>,
    pub available_from: Option<DateTime<Utc>>,
    #[serde(default)]
    pub free_windows: Vec<DispatchReplanAnchorFreeWindow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchReplanUnavailableBlock {
    pub resource_type: String,
    pub resource_id: String,
    pub block_type: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub reason: Option<String>,
    pub source_id: Option<String>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchReplanTurnaroundPair {
    pub pair_key: String,
    pub inbound_order_id: String,
    pub outbound_order_id: String,
    #[serde(default)]
    pub slot_pairs: Vec<TurnaroundSlotPairSchema>,
    pub inbound_slot_code: Option<String>,
    pub outbound_slot_code: Option<String>,
    pub planned_sta: Option<DateTime<Utc>>,
    pub planned_std: Option<DateTime<Utc>>,
    pub minimum_turnaround_minutes: Option<i32>,
    pub slack_minutes: Option<i32>,
    #[serde(default)]
    pub tightness_penalty: f64,
    #[serde(default)]
    pub hard_continuity_required: bool,
    #[serde(default)]
    pub continuity_penalty_weight: f64,
    pub constraint_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchReplanObjectiveConfig {
    #[serde(default = "default_true_flag")]
    pub staged_lexicographic: bool,
    #[serde(default)]
    pub objective_priority: Vec<String>,
    #[serde(default)]
    pub objective_stage_keys: Vec<String>,
    #[serde(default = "default_replan_timeout_ms")]
    pub timeout_ms: i64,
    #[serde(default = "default_snapshot_travel_time_mode")]
    pub travel_time_mode: String,
    #[serde(default)]
    pub average_workload_target: f64,
}

impl Default for DispatchReplanObjectiveConfig {
    fn default() -> Self {
        Self {
            staged_lexicographic: true,
            objective_priority: Vec::new(),
            objective_stage_keys: Vec::new(),
            timeout_ms: default_replan_timeout_ms(),
            travel_time_mode: default_snapshot_travel_time_mode(),
            average_workload_target: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchReplanSnapshotResponse {
    pub snapshot_id: String,
    #[serde(default)]
    pub model_version: String,
    pub solver_version: String,
    pub generated_at: DateTime<Utc>,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub strategy: String,
    pub max_suggestions: i64,
    #[serde(default = "default_snapshot_travel_time_mode")]
    pub travel_time_mode: String,
    #[serde(default)]
    pub constraints: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub objective_config: DispatchReplanObjectiveConfig,
    #[serde(default)]
    pub unsupported_features: Vec<String>,
    #[serde(default)]
    pub impact_summary: DispatchReplanImpactSummary,
    #[serde(default)]
    pub changed_orders: Vec<String>,
    #[serde(default = "default_risk_level")]
    pub risk_level: String,
    #[serde(default)]
    pub requires_manual_confirmation: bool,
    #[serde(default)]
    pub optimizable_orders: Vec<DispatchReplanSnapshotOrder>,
    #[serde(default)]
    pub fixed_anchor_orders: Vec<DispatchReplanSnapshotOrder>,
    #[serde(default)]
    pub orders: Vec<DispatchReplanSnapshotOrder>,
    #[serde(default)]
    pub travel_edges: Vec<DispatchReplanTravelEdge>,
    #[serde(default)]
    pub resource_travel_edges: Vec<DispatchReplanTravelEdge>,
    #[serde(default)]
    pub fixed_orders: Vec<DispatchReplanSnapshotOrder>,
    #[serde(default)]
    pub employee_anchor_states: Vec<DispatchReplanAnchorState>,
    #[serde(default)]
    pub equipment_anchor_states: Vec<DispatchReplanAnchorState>,
    #[serde(default)]
    pub employee_free_windows: Vec<DispatchReplanAnchorFreeWindow>,
    #[serde(default)]
    pub equipment_free_windows: Vec<DispatchReplanAnchorFreeWindow>,
    #[serde(default)]
    pub employee_unavailable_blocks: Vec<DispatchReplanUnavailableBlock>,
    #[serde(default)]
    pub equipment_unavailable_blocks: Vec<DispatchReplanUnavailableBlock>,
    #[serde(default)]
    pub turnaround_pairs: Vec<DispatchReplanTurnaroundPair>,
}

fn default_snapshot_travel_time_mode() -> String {
    "zero_matrix_forbidden".to_string()
}

fn default_replan_timeout_ms() -> i64 {
    10000
}

fn default_risk_level() -> String {
    "low".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct DispatchReplanApplyRequest {
    pub snapshot_id: String,
    pub solver_version: String,
    #[serde(default = "default_frontend_replan_strategy")]
    pub strategy: String,
    #[serde(default)]
    pub suggestions: Vec<DispatchReplanSuggestion>,
    #[serde(default)]
    pub solver_metadata: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub order_results: Vec<DispatchReplanOrderResult>,
    #[serde(default)]
    pub personnel_slot_assignments: Vec<serde_json::Value>,
    #[serde(default)]
    pub equipment_slot_assignments: Vec<serde_json::Value>,
    #[serde(default)]
    pub continuity_decisions: Vec<serde_json::Value>,
    #[serde(default)]
    pub objective_breakdown: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub solver_run_metadata: HashMap<String, serde_json::Value>,
}

impl DispatchReplanApplyRequest {
    pub fn normalize(mut self) -> Result<Self, String> {
        self.snapshot_id = self.snapshot_id.trim().to_string();
        self.solver_version = self.solver_version.trim().to_string();
        if self.snapshot_id.is_empty() {
            return Err("snapshot_id 不能为空".to_string());
        }
        if self.solver_version.is_empty() {
            return Err("solver_version 不能为空".to_string());
        }

        self.strategy = normalize_frontend_replan_strategy(&self.strategy)?;

        if self.order_results.is_empty() && !self.suggestions.is_empty() {
            self.order_results = self
                .suggestions
                .iter()
                .cloned()
                .map(crate::services::dispatch_frontend_replan_service::DispatchFrontendReplanService::suggestion_to_order_result)
                .collect();
        }

        if self.solver_run_metadata.is_empty() && !self.solver_metadata.is_empty() {
            self.solver_run_metadata = std::mem::take(&mut self.solver_metadata);
        }
        if self.order_results.is_empty()
            && (!self.personnel_slot_assignments.is_empty()
                || !self.equipment_slot_assignments.is_empty()
                || !self.continuity_decisions.is_empty())
        {
            return Err("order_results 不能为空".to_string());
        }
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    #[test]
    fn replan_request_defaults_to_balanced_strategy() {
        let request = ReplanRequest {
            window_start: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            window_end: Utc.with_ymd_and_hms(2026, 1, 1, 1, 0, 0).unwrap(),
            strategy: default_replan_strategy(),
            apply_changes: false,
            max_suggestions: None,
        }
        .normalize()
        .expect("replan request should normalize");

        assert_eq!(request.strategy, "balanced");
    }

    #[test]
    fn replan_request_normalizes_valid_strategy() {
        let request = ReplanRequest {
            window_start: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            window_end: Utc.with_ymd_and_hms(2026, 1, 1, 1, 0, 0).unwrap(),
            strategy: " Efficiency ".to_string(),
            apply_changes: false,
            max_suggestions: None,
        }
        .normalize()
        .expect("replan request should normalize");

        assert_eq!(request.strategy, "efficiency");
    }

    #[test]
    fn replan_request_rejects_legacy_strategy() {
        let error = ReplanRequest {
            window_start: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            window_end: Utc.with_ymd_and_hms(2026, 1, 1, 1, 0, 0).unwrap(),
            strategy: "shift_later".to_string(),
            apply_changes: false,
            max_suggestions: None,
        }
        .normalize()
        .expect_err("legacy strategy should be rejected");

        assert!(error.contains("strategy 必须是"));
    }

    #[test]
    fn replan_request_rejects_invalid_max_suggestions() {
        let error = ReplanRequest {
            window_start: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            window_end: Utc.with_ymd_and_hms(2026, 1, 1, 1, 0, 0).unwrap(),
            strategy: "balanced".to_string(),
            apply_changes: false,
            max_suggestions: Some(0),
        }
        .normalize()
        .expect_err("invalid max_suggestions should be rejected");

        assert!(error.contains("max_suggestions"));
    }

    #[test]
    fn workflow_dispatch_create_request_rejects_required_people_out_of_range() {
        let mut request = WorkflowDispatchCreateRequest {
            process_instance_id: "proc-1".to_string(),
            process_task_id: "task-1".to_string(),
            process_definition_key: None,
            business_key: None,
            flight_id: "flight-1".to_string(),
            task_type: "boarding".to_string(),
            stand_id: None,
            planned_start_time: None,
            planned_end_time: None,
            assignment_deadline: None,
            target_department: "运行部".to_string(),
            target_job_title: Some("主管".to_string()),
            required_people: 0,
            priority: "normal".to_string(),
            description: None,
            context: serde_json::Map::new(),
        };

        assert!(request.validate().is_err());
        request.required_people = 21;
        assert!(request.validate().is_err());
    }

    #[test]
    fn workflow_dispatch_create_request_accepts_required_people_boundaries() {
        let mut request = WorkflowDispatchCreateRequest {
            process_instance_id: "proc-1".to_string(),
            process_task_id: "task-1".to_string(),
            process_definition_key: None,
            business_key: None,
            flight_id: "flight-1".to_string(),
            task_type: "boarding".to_string(),
            stand_id: None,
            planned_start_time: None,
            planned_end_time: None,
            assignment_deadline: None,
            target_department: "运行部".to_string(),
            target_job_title: Some("主管".to_string()),
            required_people: 1,
            priority: "normal".to_string(),
            description: None,
            context: serde_json::Map::new(),
        };

        assert!(request.validate().is_ok());
        request.required_people = 20;
        assert!(request.validate().is_ok());
    }

    #[test]
    fn workflow_dispatch_assign_request_rejects_empty_assignees() {
        let request = WorkflowDispatchAssignRequest {
            assigned_user_ids: Vec::new(),
            notes: Some("note".to_string()),
            complete_process_task: true,
        };

        assert!(request.validate().is_err());
    }

    #[test]
    fn dispatch_order_create_deserializes_publication_and_equipment_fields() {
        let request: DispatchOrderCreate = serde_json::from_value(serde_json::json!({
            "flight_id": "01K844D60CB2487475997F5CEE",
            "task_type": "boarding",
            "assignee_type": "individual",
            "individual_user_id": "01H000000000000000000000A1",
            "publication_state": "prepublished",
            "source_type": "manual",
            "leg_scope": "none",
            "workflow_context": {
                "source_screen": "dispatch_console"
            },
            "crew_requirement_snapshot": [
                {
                    "slot_code": "slot_1",
                    "qualification_code": "boarding",
                    "required_count": 1
                }
            ],
            "equipment_requirement_snapshot": [
                {
                    "slot_code": "eq_1",
                    "equipment_type_id": "EQTYPE1",
                    "equipment_type_code": "eqtype1",
                    "required_count": 1,
                    "requires_driver": false
                }
            ],
            "equipment_assignment": [
                {
                    "slot_code": "eq_1",
                    "equipment_id": "01H000000000000000000000E1",
                    "equipment_code": "EQUIP-1"
                }
            ]
        }))
        .expect("dispatch order create payload should deserialize");

        assert_eq!(request.publication_state, "prepublished");
        assert_eq!(request.source_type, "manual");
        assert_eq!(request.leg_scope, "none");
        assert_eq!(
            request
                .workflow_context
                .get("source_screen")
                .and_then(serde_json::Value::as_str),
            Some("dispatch_console")
        );
        assert_eq!(request.crew_requirement_snapshot.len(), 1);
        assert_eq!(request.equipment_requirement_snapshot.len(), 1);
        assert_eq!(request.equipment_assignment.len(), 1);
    }

    #[test]
    fn mobile_sync_request_rejects_more_than_500_actions() {
        let request = MobileSyncRequest {
            actions: (0..501)
                .map(|index| MobileSyncAction {
                    action_type: "accept".to_string(),
                    dispatch_order_id: format!("order-{index}"),
                    client_action_id: format!("client-{index}"),
                    action_timestamp: None,
                    payload: None,
                })
                .collect(),
        };

        assert_eq!(request.validate(), Err("actions 数量不能超过 500".to_string()));
    }

    #[test]
    fn replan_impact_summary_serializes_new_preview_contract() {
        let summary = DispatchReplanImpactSummary {
            affected_order_count: 3,
            affected_flight_count: 2,
            conflicts_fixed_count: 1,
            new_assignment_count: 1,
            late_assignment_count: 1,
            locked_item_count: 1,
            high_risk_change_count: 1,
            warnings: vec![DispatchReplanImpactWarning {
                code: "qualification_gap".to_string(),
                label: "需要人工复核".to_string(),
                order_id: Some("order-1".to_string()),
                flight_id: Some("flight-1".to_string()),
            }],
            affected_flights: 2,
            changed_orders: 3,
            reassigned_orders: 1,
            delayed_orders: 1,
            added_delay_minutes: 5.0,
            replaced_member_count: 2,
            qualification_gap_count: 1,
        };

        let payload = serde_json::to_value(summary).expect("summary should serialize");

        assert_eq!(payload["affected_order_count"], 3);
        assert_eq!(payload["affected_flight_count"], 2);
        assert_eq!(payload["conflicts_fixed_count"], 1);
        assert_eq!(payload["new_assignment_count"], 1);
        assert_eq!(payload["late_assignment_count"], 1);
        assert_eq!(payload["locked_item_count"], 1);
        assert_eq!(payload["high_risk_change_count"], 1);
        assert_eq!(payload["warnings"][0]["code"], "qualification_gap");
        assert_eq!(payload["warnings"][0]["order_id"], "order-1");
    }

    #[test]
    fn replan_suggestion_serializes_typed_grouping_metadata() {
        let suggestion = DispatchReplanSuggestion {
            dispatch_order_id: "order-1".to_string(),
            order_id: Some("order-1".to_string()),
            order_ids: vec!["order-1".to_string(), "order-2".to_string()],
            flight_id: Some("flight-1".to_string()),
            suggestion_type: Some("assigned_conflict_resolution".to_string()),
            risk_level: Some("high".to_string()),
            safety_gate_state: Some("manual_review_required".to_string()),
            ..DispatchReplanSuggestion::default()
        };

        let payload = serde_json::to_value(suggestion).expect("suggestion should serialize");

        assert_eq!(payload["dispatch_order_id"], "order-1");
        assert_eq!(payload["order_id"], "order-1");
        assert_eq!(payload["order_ids"][1], "order-2");
        assert_eq!(payload["flight_id"], "flight-1");
        assert_eq!(payload["suggestion_type"], "assigned_conflict_resolution");
        assert_eq!(payload["risk_level"], "high");
        assert_eq!(payload["safety_gate_state"], "manual_review_required");
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DispatchReplanNotificationSummaryItem {
    pub dispatch_order_id: String,
    pub suggestion_type: String,
    #[serde(default)]
    pub recipient_user_ids: Vec<String>,
    #[serde(default)]
    pub sent_count: i64,
    #[serde(default)]
    pub failed_count: i64,
    pub receipt_group_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DispatchReplanNotificationSummary {
    #[serde(default)]
    pub total_sent_count: i64,
    #[serde(default)]
    pub total_failed_count: i64,
    #[serde(default)]
    pub receipt_required_count: i64,
    #[serde(default)]
    pub items: Vec<DispatchReplanNotificationSummaryItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchReplanApplyResponse {
    pub snapshot_id: String,
    pub applied: bool,
    #[serde(default)]
    pub suggestions: Vec<DispatchReplanSuggestion>,
    #[serde(default)]
    pub order_results: Vec<DispatchReplanOrderResult>,
    #[serde(default)]
    pub personnel_slot_assignments: Vec<serde_json::Value>,
    #[serde(default)]
    pub equipment_slot_assignments: Vec<serde_json::Value>,
    #[serde(default)]
    pub continuity_decisions: Vec<serde_json::Value>,
    #[serde(default)]
    pub objective_breakdown: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub solver_metadata: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub solver_run_metadata: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub notification_summary: DispatchReplanNotificationSummary,
    #[serde(default)]
    pub impact_summary: DispatchReplanImpactSummary,
    #[serde(default)]
    pub changed_orders: Vec<String>,
    #[serde(default = "default_risk_level")]
    pub risk_level: String,
    #[serde(default)]
    pub requires_manual_confirmation: bool,
    pub message: String,
}

// ---------------------------------------------------------------------------
// 事件驱动的派工规则
// ---------------------------------------------------------------------------

// Re-export domain-level types to avoid cyclic dependencies.
pub use fms_domain::ports::event_rule_repository::{
    AdjustmentActionType, ConditionItem, ConditionOperator, CreateCrewRequirement, DispatchOrderAdjustmentRuleCreate,
    DispatchOrderAdjustmentRuleUpdate, EventDrivenGenerationRuleCreate, EventDrivenGenerationRuleUpdate,
    GenerationRuleConfig,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddCrewSlotConfig {
    pub slot_code: String,
    pub qualification_code: String,
    pub required_count: i32,
    #[serde(default)]
    pub must_be_distinct: bool,
    #[serde(default)]
    pub remarks: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncreaseCrewCountConfig {
    pub slot_code: String,
    pub delta: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeCrewLevelConfig {
    pub slot_code: String,
    pub min_level_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddEquipmentSlotConfig {
    pub slot_code: String,
    pub equipment_type_code: String,
    pub required_count: i32,
    #[serde(default)]
    pub remarks: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncreaseEquipmentCountConfig {
    pub slot_code: String,
    pub delta: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendDurationConfig {
    pub delta_minutes: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvancePublishConfig {
    pub delta_minutes: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequireDriverConfig {
    pub slot_code: String,
    pub driver_qualification_code: String,
    #[serde(default)]
    pub driver_min_level_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdjustmentRuleConfig {
    pub action_type: AdjustmentActionType,
    #[serde(flatten)]
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchOrderAdjustmentRuleResponse {
    pub id: String,
    pub adjuster_type: AdjustmentActionType,
    pub name: String,
    pub description: Option<String>,
    pub event_patterns: Vec<String>,
    pub priority: i32,
    pub conditions: Option<serde_json::Value>,
    pub config: serde_json::Value,
    pub is_enabled: bool,
    pub department_id: Option<String>,
    pub department_name: Option<String>,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
    pub created_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchOrderAdjustmentRuleListResponse {
    pub items: Vec<DispatchOrderAdjustmentRuleResponse>,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventDrivenGenerationRuleResponse {
    pub id: String,
    pub generator_type: String,
    pub name: String,
    pub description: Option<String>,
    pub event_patterns: Vec<String>,
    pub priority: i32,
    pub conditions: Option<serde_json::Value>,
    pub config: GenerationRuleConfig,
    pub is_enabled: bool,
    pub department_id: Option<String>,
    pub department_name: Option<String>,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
    pub created_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventDrivenGenerationRuleListResponse {
    pub items: Vec<EventDrivenGenerationRuleResponse>,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulePreviewRequest {
    pub event_type: String,
    pub payload: serde_json::Value,
    #[serde(default)]
    pub flight_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulePreviewAffectedOrder {
    pub order_id: String,
    pub task_type: String,
    #[serde(default)]
    pub modified_fields: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulePreviewMatchedAdjustment {
    pub rule_id: String,
    pub rule_name: String,
    pub action_type: String,
    pub action_description: String,
    #[serde(default)]
    pub affected_orders: Vec<RulePreviewAffectedOrder>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulePreviewMatchedGeneration {
    pub rule_id: String,
    pub rule_name: String,
    pub would_generate: bool,
    #[serde(default)]
    pub generated_order_preview: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulePreviewResponse {
    #[serde(default)]
    pub matched_adjustment_rules: Vec<RulePreviewMatchedAdjustment>,
    #[serde(default)]
    pub matched_generation_rules: Vec<RulePreviewMatchedGeneration>,
    pub timestamp: chrono::DateTime<Utc>,
}

#[allow(dead_code)]
fn default_generator_type() -> String {
    "event_generated".to_string()
}

#[allow(dead_code)]
fn default_rule_priority() -> i32 {
    100
}
