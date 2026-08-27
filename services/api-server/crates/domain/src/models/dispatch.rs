//! 派工系统领域模型
//!
//! 对应 Python `src/domain/models/dispatch.py`。

use crate::error::DomainError;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// 枚举
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeamStatus {
    OnDuty,
    OffDuty,
    Break,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EquipmentStatus {
    Available,
    InUse,
    Maintenance,
    Retired,
}

/// 人员在岗运行时状态（personnel_runtime）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonnelStatus {
    OnDuty,
    OffDuty,
    Break,
    OnLeave,
}

impl AsRef<str> for PersonnelStatus {
    fn as_ref(&self) -> &str {
        match self {
            Self::OnDuty => "on_duty",
            Self::OffDuty => "off_duty",
            Self::Break => "break",
            Self::OnLeave => "on_leave",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DispatchOrderStatus {
    Pending,
    Assigned,
    InProgress,
    Completed,
    Cancelled,
}

impl AsRef<str> for DispatchOrderStatus {
    fn as_ref(&self) -> &str {
        match self {
            Self::Pending => "pending",
            Self::Assigned => "assigned",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssigneeType {
    Team,
    Individual,
}

impl AsRef<str> for AssigneeType {
    fn as_ref(&self) -> &str {
        match self {
            Self::Team => "team",
            Self::Individual => "individual",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DispatchType {
    Auto,
    Manual,
}

impl AsRef<str> for DispatchType {
    fn as_ref(&self) -> &str {
        match self {
            Self::Auto => "auto",
            Self::Manual => "manual",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DispatchLockLevel {
    Active,
    Frozen,
    ManualLock,
    Optimizable,
}

impl AsRef<str> for DispatchLockLevel {
    fn as_ref(&self) -> &str {
        match self {
            Self::Active => "active",
            Self::Frozen => "frozen",
            Self::ManualLock => "manual_lock",
            Self::Optimizable => "optimizable",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleSource {
    ShiftInstance,
    CurrentStatusFallback,
}

impl AsRef<str> for ScheduleSource {
    fn as_ref(&self) -> &str {
        match self {
            Self::ShiftInstance => "shift_instance",
            Self::CurrentStatusFallback => "current_status_fallback",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DepartmentRuleStatus {
    Draft,
    Published,
    Archived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DispatchPublicationState {
    Prepublished,
    Published,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegScope {
    Inbound,
    Outbound,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublishTriggerMode {
    Time,
    Event,
    Either,
    BothRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnaroundConstraintMode {
    SamePerson,
    SoftPreferSamePerson,
    HandoverRequired,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualificationGrantStatus {
    Active,
    Expired,
    Suspended,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberRole {
    Leader,
    Member,
    Driver,
}

impl AsRef<str> for MemberRole {
    fn as_ref(&self) -> &str {
        match self {
            Self::Leader => "leader",
            Self::Member => "member",
            Self::Driver => "driver",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

// ---------------------------------------------------------------------------
// 值对象
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub lat: f64,
    pub lng: f64,
}

// ---------------------------------------------------------------------------
// 实体
// ---------------------------------------------------------------------------

/// 科室/部门
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Department {
    pub id: String,
    pub name: String,
    pub code: Option<String>,
    pub description: Option<String>,
    pub manager_id: Option<String>,
    pub terminal: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default = "default_true")]
    pub is_active: bool,
}

/// 班组类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamType {
    pub id: String,
    pub name: String,
    pub department_id: Option<String>,
    pub code: Option<String>,
    pub description: Option<String>,
    pub color: Option<String>,
    #[serde(default)]
    pub is_driver_type: bool,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default = "default_true")]
    pub is_active: bool,
    #[serde(default)]
    pub task_types: Vec<String>,
}

/// 班组
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Team {
    pub id: String,
    pub name: String,
    /// 所属科室（PR2 起写在班组上；历史无类型班组可为空，创建必填）。
    #[serde(default)]
    pub department_id: Option<String>,
    /// 只读历史值：班组类型已降为只读目录，写路径不再接受/写入。
    pub team_type_id: Option<String>,
    pub code: Option<String>,
    pub leader_id: Option<String>,
    #[serde(default = "default_off_duty")]
    pub current_status: TeamStatus,
    pub current_position_lat: Option<f64>,
    pub current_position_lng: Option<f64>,
    pub current_stand_id: Option<String>,
    pub last_position_update: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default = "default_true")]
    pub is_active: bool,
    pub team_type: Option<TeamType>,
    #[serde(default)]
    pub members: Vec<TeamMember>,
}

fn default_off_duty() -> TeamStatus {
    TeamStatus::OffDuty
}

impl Team {
    /// 班组只有启用且处于 on-duty 状态时才能作为可调度资源。
    pub fn is_on_duty(&self) -> bool {
        self.is_active && self.current_status == TeamStatus::OnDuty
    }

    /// 判断用户是否是当前班组的有效成员。
    pub fn has_member(&self, user_id: &str) -> bool {
        let user_id = user_id.trim();
        !user_id.is_empty()
            && self
                .members
                .iter()
                .any(|member| member.is_active && member.user_id == user_id)
    }

    pub fn active_member_count(&self) -> usize {
        self.members.iter().filter(|member| member.is_active).count()
    }

    pub fn can_accept_dispatch(&self) -> bool {
        self.is_on_duty() && self.active_member_count() > 0
    }

    pub fn mark_on_duty(&mut self) {
        self.current_status = TeamStatus::OnDuty;
    }

    pub fn mark_off_duty(&mut self) {
        self.current_status = TeamStatus::OffDuty;
    }

    pub fn start_break(&mut self) {
        self.current_status = TeamStatus::Break;
    }
}

/// 班组成员
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMember {
    pub id: String,
    pub team_id: String,
    pub user_id: String,
    #[serde(default = "default_member_role")]
    pub role: MemberRole,
    #[serde(default)]
    pub can_drive: bool,
    pub joined_at: Option<DateTime<Utc>>,
    pub left_at: Option<DateTime<Utc>>,
    #[serde(default = "default_true")]
    pub is_active: bool,
    pub username: Option<String>,
    pub user_display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShiftTemplate {
    pub id: String,
    pub name: String,
    pub resource_type: String,
    pub resource_id: String,
    pub terminal: Option<String>,
    #[serde(default = "default_shift_start_local")]
    pub start_time_local: String,
    #[serde(default = "default_shift_end_local")]
    pub end_time_local: String,
    #[serde(default)]
    pub weekdays: Vec<i32>,
    pub max_continuous_minutes: Option<i32>,
    pub min_rest_minutes: Option<i32>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

fn default_shift_start_local() -> String {
    "08:00".to_string()
}

fn default_shift_end_local() -> String {
    "16:00".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShiftInstance {
    pub id: String,
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
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

fn default_shift_status() -> String {
    "scheduled".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepartmentQualificationCatalog {
    pub id: String,
    pub department_id: String,
    pub qualification_code: String,
    pub qualification_name: String,
    pub description: Option<String>,
    #[serde(default = "default_true")]
    pub is_active: bool,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepartmentQualificationLevel {
    pub id: String,
    pub department_id: String,
    pub qualification_code: String,
    pub level_code: String,
    pub level_name: String,
    #[serde(default)]
    pub level_rank: i32,
    #[serde(default)]
    pub covered_level_codes: Vec<String>,
    #[serde(default = "default_true")]
    pub is_active: bool,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualificationGrant {
    pub id: String,
    pub user_id: String,
    pub department_id: String,
    pub qualification_code: String,
    pub level_code: String,
    pub valid_from: Option<DateTime<Utc>>,
    pub valid_to: Option<DateTime<Utc>>,
    #[serde(default = "default_qualification_grant_status")]
    pub status: QualificationGrantStatus,
    pub source_team_id: Option<String>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

fn default_qualification_grant_status() -> QualificationGrantStatus {
    QualificationGrantStatus::Active
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskTypeCrewSlotRequirement {
    pub slot_code: String,
    pub qualification_code: String,
    pub min_level_code: Option<String>,
    #[serde(default = "default_required_count")]
    pub required_count: i32,
    #[serde(default = "default_true")]
    pub must_be_distinct: bool,
    pub exclusive_group: Option<String>,
    pub remarks: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskTypeEquipmentRequirement {
    pub slot_code: String,
    pub equipment_type_id: Option<String>,
    pub equipment_type_code: Option<String>,
    #[serde(default = "default_required_count")]
    pub required_count: i32,
    #[serde(default = "default_true")]
    pub must_be_distinct: bool,
    #[serde(default)]
    pub requires_driver: bool,
    pub driver_qualification_code: Option<String>,
    pub driver_min_level_code: Option<String>,
    pub remarks: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnaroundSlotPair {
    pub inbound_slot_code: String,
    pub outbound_slot_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnaroundContinuityRule {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_turnaround_counterpart_leg_scope")]
    pub counterpart_leg_scope: LegScope,
    pub counterpart_task_type: String,
    #[serde(default)]
    pub slot_pairs: Vec<TurnaroundSlotPair>,
    #[serde(default = "default_turnaround_constraint_mode")]
    pub constraint_mode: TurnaroundConstraintMode,
    pub tight_threshold_minutes: Option<i32>,
    pub relax_threshold_minutes: Option<i32>,
    #[serde(default)]
    pub flight_filters: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub aircraft_type_filters: Vec<String>,
    pub notes: Option<String>,
}

fn default_required_count() -> i32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepartmentTaskTypeRequirementVersion {
    pub id: String,
    pub department_id: String,
    pub task_type: String,
    #[serde(default = "default_version_no")]
    pub version_no: i32,
    #[serde(default = "default_department_rule_status")]
    pub status: DepartmentRuleStatus,
    #[serde(default)]
    pub requirements: Vec<TaskTypeCrewSlotRequirement>,
    #[serde(default)]
    pub crew_requirements: Vec<TaskTypeCrewSlotRequirement>,
    #[serde(default)]
    pub equipment_requirements: Vec<TaskTypeEquipmentRequirement>,
    #[serde(default)]
    pub turnaround_continuity_rules: Vec<TurnaroundContinuityRule>,
    pub notes: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightGenerationRule {
    pub id: String,
    pub department_id: String,
    pub task_type: String,
    pub leg_scope: LegScope,
    #[serde(default = "default_version_no")]
    pub version_no: i32,
    #[serde(default = "default_department_rule_status")]
    pub status: DepartmentRuleStatus,
    pub rule_name: Option<String>,
    #[serde(default)]
    pub conditions: HashMap<String, serde_json::Value>,
    #[serde(default = "default_generation_anchor_type")]
    pub generation_anchor_type: String,
    #[serde(default)]
    pub start_offset_minutes: i32,
    /// 预计完成时间计算方式：开始时间加时长，或完成锚点加偏移。
    #[serde(default = "default_completion_time_mode")]
    pub completion_time_mode: String,
    pub completion_anchor_type: Option<String>,
    pub completion_offset_minutes: Option<i32>,
    pub duration_minutes: Option<i32>,
    /// 重排时该作业开始时间允许后滑的分钟数。
    /// `None` 表示部门未配置,由重排服务回退到系统默认值。
    pub start_flex_minutes: Option<i32>,
    /// 预排冲突预警提前量（分钟）。
    /// `None` 表示未配置,由预警服务回退到部门默认值或系统默认值。
    pub completion_warning_lead_minutes: Option<i32>,
    /// 人数 -> 作业时长(分钟)映射,如 `{"1":45,"2":30,"3":25}`。
    ///
    /// 保持原始 JSON 而不在此解析:该列是 JSONB,历史行里可能存着任何形状,
    /// 领域模型只负责原样往返。键值合法性由读取端逐条校验(非法条目忽略),
    /// 写入端做归一化。`None` 表示部门未配置,重排回退到 `duration_minutes` 常量。
    #[serde(default)]
    pub duration_by_crew_size: Option<serde_json::Value>,
    #[serde(default = "default_dispatch_publication_state")]
    pub publication_state: DispatchPublicationState,
    #[serde(default = "default_publish_trigger_mode")]
    pub publish_trigger_mode: PublishTriggerMode,
    pub publish_at: Option<DateTime<Utc>>,
    pub publish_offset_minutes: Option<i32>,
    pub publish_event_code: Option<String>,
    pub notes: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationAdjustmentRule {
    pub id: String,
    pub department_id: String,
    pub task_type: String,
    #[serde(default = "default_version_no")]
    pub version_no: i32,
    #[serde(default = "default_department_rule_status")]
    pub status: DepartmentRuleStatus,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporaryTaskTemplate {
    pub id: String,
    pub department_id: String,
    pub template_code: String,
    pub template_name: String,
    pub task_type: String,
    #[serde(default)]
    pub crew_requirements: Vec<TaskTypeCrewSlotRequirement>,
    #[serde(default)]
    pub equipment_requirements: Vec<TaskTypeEquipmentRequirement>,
    pub notes: Option<String>,
    #[serde(default = "default_true")]
    pub is_active: bool,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

fn default_version_no() -> i32 {
    1
}

fn default_department_rule_status() -> DepartmentRuleStatus {
    DepartmentRuleStatus::Draft
}

fn default_turnaround_counterpart_leg_scope() -> LegScope {
    LegScope::Outbound
}

fn default_turnaround_constraint_mode() -> TurnaroundConstraintMode {
    TurnaroundConstraintMode::Disabled
}

fn default_generation_anchor_type() -> String {
    "scheduled_time".to_string()
}

fn default_completion_time_mode() -> String {
    "start_plus_duration".to_string()
}

fn default_dispatch_publication_state() -> DispatchPublicationState {
    DispatchPublicationState::Prepublished
}

fn default_publish_trigger_mode() -> PublishTriggerMode {
    PublishTriggerMode::Time
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaveRecord {
    pub id: String,
    pub user_id: String,
    pub team_id: Option<String>,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub reason: Option<String>,
    #[serde(default = "default_leave_status")]
    pub status: String,
    pub created_at: Option<DateTime<Utc>>,
}

fn default_leave_status() -> String {
    "approved".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquipmentDowntime {
    pub id: String,
    pub equipment_id: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub reason: Option<String>,
    #[serde(default = "default_equipment_downtime_status")]
    pub status: String,
    pub created_at: Option<DateTime<Utc>>,
}

fn default_equipment_downtime_status() -> String {
    "scheduled".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchLockRule {
    pub id: String,
    pub dispatch_order_id: Option<String>,
    pub flight_id: Option<String>,
    pub team_id: Option<String>,
    #[serde(default = "default_dispatch_lock_level")]
    pub lock_level: DispatchLockLevel,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub reason: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}

fn default_dispatch_lock_level() -> DispatchLockLevel {
    DispatchLockLevel::Optimizable
}

fn default_member_role() -> MemberRole {
    MemberRole::Member
}

/// 航站楼目录。构成事实是成员表（terminal_stands/gates/carousels），
/// 不是反查 `terminal` 列。一口/一机位/一转盘同时只属于一座楼。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Terminal {
    pub terminal_id: String,
    pub code: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub is_active: bool,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// 登机口目录。必须挂楼；成员关系在 `terminal_gates`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gate {
    pub gate_id: String,
    pub code: String,
    pub name: Option<String>,
    #[serde(default = "default_true")]
    pub is_active: bool,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// 行李转盘目录。必须挂楼；成员关系在 `terminal_carousels`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaggageCarousel {
    pub carousel_id: String,
    pub code: String,
    pub name: Option<String>,
    #[serde(default = "default_true")]
    pub is_active: bool,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// 一个航站楼的只读上下文：楼 + 三类成员列表（目录行）。
/// 供 `Terminal.get_context` 等只读动作与资源管理页使用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalDirectory {
    pub terminal: Terminal,
    #[serde(default)]
    pub stands: Vec<Stand>,
    #[serde(default)]
    pub gates: Vec<Gate>,
    #[serde(default)]
    pub carousels: Vec<BaggageCarousel>,
}

/// 机位/停机位
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stand {
    pub id: String,
    pub code: String,
    pub name: Option<String>,
    pub terminal: Option<String>,
    pub area: Option<String>,
    #[serde(default)]
    pub position_lat: f64,
    #[serde(default)]
    pub position_lng: f64,
    pub stand_type: Option<String>,
    pub size_category: Option<String>,
    #[serde(default = "default_true")]
    pub is_active: bool,
    pub created_at: Option<DateTime<Utc>>,
}

/// 作业类型定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskType {
    pub id: String,
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
    #[serde(default = "default_true")]
    pub is_active: bool,
    pub created_at: Option<DateTime<Utc>>,
}

fn default_trigger_offset() -> i32 {
    30
}
fn default_trigger_type() -> String {
    "before_eta".to_string()
}

/// 设备类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquipmentType {
    pub id: String,
    pub name: String,
    pub code: Option<String>,
    pub category: Option<String>,
    #[serde(default)]
    pub requires_driver: bool,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default = "default_true")]
    pub is_active: bool,
    #[serde(default)]
    pub task_types: Vec<String>,
}

/// 设备
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Equipment {
    pub id: String,
    pub code: String,
    pub equipment_type_id: Option<String>,
    /// 所属科室（PR2 起挂在设备上；历史设备可为空，创建必填）。
    #[serde(default)]
    pub department_id: Option<String>,
    pub name: Option<String>,
    pub license_plate: Option<String>,
    #[serde(default = "default_available")]
    pub status: EquipmentStatus,
    pub current_position_lat: Option<f64>,
    pub current_position_lng: Option<f64>,
    pub current_stand_id: Option<String>,
    pub last_position_update: Option<DateTime<Utc>>,
    pub current_dispatch_id: Option<String>,
    pub last_maintenance_date: Option<NaiveDate>,
    pub next_maintenance_date: Option<NaiveDate>,
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default = "default_true")]
    pub is_active: bool,
    pub equipment_type: Option<EquipmentType>,
}

fn default_available() -> EquipmentStatus {
    EquipmentStatus::Available
}

impl Equipment {
    /// 设备只有启用且状态为 available 时才可分配。
    pub fn is_available(&self) -> bool {
        self.is_active && self.status == EquipmentStatus::Available
    }

    pub fn is_assigned(&self) -> bool {
        self.status == EquipmentStatus::InUse && self.current_dispatch_id.is_some()
    }

    /// 尝试将设备分配给派工单；不可用时保持原状并返回 false。
    pub fn assign(&mut self, dispatch_id: impl Into<String>) -> bool {
        let dispatch_id = dispatch_id.into().trim().to_string();
        if dispatch_id.is_empty() || !self.is_available() {
            return false;
        }

        self.status = EquipmentStatus::InUse;
        self.current_dispatch_id = Some(dispatch_id);
        true
    }

    /// 释放当前派工占用；未占用时保持原状并返回 false。
    pub fn release(&mut self) -> bool {
        if self.status != EquipmentStatus::InUse && self.current_dispatch_id.is_none() {
            return false;
        }

        self.status = EquipmentStatus::Available;
        self.current_dispatch_id = None;
        true
    }

    pub fn send_to_maintenance(&mut self) {
        self.status = EquipmentStatus::Maintenance;
        self.current_dispatch_id = None;
    }

    pub fn retire(&mut self) {
        self.status = EquipmentStatus::Retired;
        self.current_dispatch_id = None;
        self.is_active = false;
    }
}

/// 人员在岗运行时（personnel_runtime）。
///
/// `user_id` = 个人账号 `users.id`；无行视为 `off_duty`。位置为可选的当前坐标/
/// 机位，位置更新只对该行生效（班组位置不由此传播）。表的 status CHECK 允许
/// `on_duty | off_duty | break | on_leave`（与 `TeamStatus` 不同——人员还有 `on_leave`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonnelRuntime {
    pub user_id: String,
    #[serde(default = "default_personnel_off_duty")]
    pub current_status: PersonnelStatus,
    pub current_stand_id: Option<String>,
    pub current_position_lat: Option<f64>,
    pub current_position_lng: Option<f64>,
    pub last_position_update: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub updated_by: Option<String>,
}

fn default_personnel_off_duty() -> PersonnelStatus {
    PersonnelStatus::OffDuty
}

/// 派工单
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchOrder {
    pub id: String,
    pub flight_id: String,
    pub task_type: String,
    pub stand_id: Option<String>,
    pub task_type_name: Option<String>,
    pub stand_code: Option<String>,
    pub terminal: Option<String>,

    // 分配单位（班组不再是工单指派对象：人员按槽挂，见 members / task_crew）
    pub department: Option<String>,
    pub individual_user_id: Option<String>,
    pub individual_username: Option<String>,

    // 司机资源（司机资质在槽上表达，driver 只指向个人）
    pub driver_type: Option<AssigneeType>,
    pub driver_user_id: Option<String>,

    // 时间节点
    pub planned_start_time: Option<DateTime<Utc>>,
    pub planned_end_time: Option<DateTime<Utc>>,
    pub actual_start_time: Option<DateTime<Utc>>,
    pub actual_end_time: Option<DateTime<Utc>>,
    pub estimated_completion_time: Option<DateTime<Utc>>,
    pub estimated_completion_reported_by: Option<String>,
    pub estimated_completion_reported_at: Option<DateTime<Utc>>,
    pub estimated_completion_note: Option<String>,

    // 状态
    #[serde(default = "default_pending")]
    pub status: DispatchOrderStatus,
    #[serde(default = "default_auto")]
    pub dispatch_type: DispatchType,
    pub dispatched_at: Option<DateTime<Utc>>,
    pub dispatched_by: Option<String>,

    // 快照
    pub snapshot_assignee_position: Option<serde_json::Value>,
    pub snapshot_equipment_positions: Option<Vec<serde_json::Value>>,
    pub estimated_arrival_minutes: Option<i32>,

    // 流程编排
    pub process_instance_id: Option<String>,
    pub process_task_id: Option<String>,
    #[serde(default)]
    pub workflow_context: serde_json::Value,
    #[serde(default = "default_workflow_status")]
    pub workflow_status: String,
    #[serde(default = "default_source")]
    pub source: String,
    #[serde(default = "default_schedule_source")]
    pub schedule_source: ScheduleSource,
    #[serde(default = "default_dispatch_lock_level")]
    pub lock_level: DispatchLockLevel,
    #[serde(default = "default_publication_state")]
    pub publication_state: String,
    #[serde(default = "default_order_source_type")]
    pub source_type: String,
    pub department_id: Option<String>,
    #[serde(default = "default_leg_scope")]
    pub leg_scope: String,
    pub generation_rule_id: Option<String>,
    pub generation_rule_version: Option<i32>,
    pub generation_anchor_type: Option<String>,
    pub generation_anchor_time: Option<DateTime<Utc>>,
    /// 生成时使用的预计完成时间规则快照；手工单和历史订单允许为空。
    pub completion_time_mode: Option<String>,
    pub completion_anchor_type: Option<String>,
    pub completion_anchor_time: Option<DateTime<Utc>>,
    pub completion_offset_minutes: Option<i32>,
    /// 预排冲突预警提前量（分钟）。
    ///
    /// 生成订单时从规则快照；调度员可在工单上覆盖。`None` 表示回退到
    /// 当前生成规则值（部门默认）或系统默认值。
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
    #[serde(default)]
    pub task_crew: serde_json::Value,
    #[serde(default)]
    pub equipment_assignment: Vec<serde_json::Value>,
    #[serde(default)]
    pub qualification_gap: Vec<serde_json::Value>,
    #[serde(default)]
    pub equipment_gap: Vec<serde_json::Value>,
    pub availability_reason: Option<String>,
    #[serde(default)]
    pub score_breakdown: serde_json::Value,
    pub conflict_reason: Option<String>,
    #[serde(default)]
    pub recommended_assignees: Vec<serde_json::Value>,
    pub recommendation_score: Option<f64>,
    #[serde(default)]
    pub supervisor_notified: bool,
    pub supervisor_notified_at: Option<DateTime<Utc>>,
    pub assignment_deadline: Option<DateTime<Utc>>,

    // 完成信息
    pub completed_by: Option<String>,
    pub completion_notes: Option<String>,
    pub gate: Option<String>,

    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,

    // 关联
    #[serde(default)]
    pub members: Vec<DispatchOrderMember>,
    #[serde(default)]
    pub equipment_list: Vec<Equipment>,
}

fn default_pending() -> DispatchOrderStatus {
    DispatchOrderStatus::Pending
}
fn default_auto() -> DispatchType {
    DispatchType::Auto
}
fn default_workflow_status() -> String {
    "pending_assignment".to_string()
}
fn default_source() -> String {
    "system".to_string()
}
fn default_schedule_source() -> ScheduleSource {
    ScheduleSource::CurrentStatusFallback
}
fn default_publication_state() -> String {
    "published".to_string()
}
fn default_order_source_type() -> String {
    "manual".to_string()
}
fn default_leg_scope() -> String {
    "none".to_string()
}
fn default_true() -> bool {
    true
}

impl DispatchOrder {
    /// 验证用户是否可以开始此派工单
    pub fn can_be_started_by(&self, user_id: &str) -> bool {
        if self.individual_user_id.as_deref() == Some(user_id) {
            return true;
        }
        self.members.iter().any(|m| m.user_id == user_id && m.is_active)
    }

    /// 验证用户是否可以完成此派工单
    pub fn can_be_completed_by(&self, user_id: &str) -> bool {
        self.can_be_started_by(user_id)
    }
}

/// 派工单人员明细
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchOrderMember {
    pub id: String,
    pub dispatch_order_id: String,
    pub user_id: String,
    #[serde(default = "default_member_role")]
    pub role: MemberRole,
    #[serde(default = "default_source_team")]
    pub source_type: AssigneeType,
    pub source_team_id: Option<String>,
    pub slot_code: Option<String>,
    pub qualification_code: Option<String>,
    pub qualification_level_code: Option<String>,
    pub assigned_at: Option<DateTime<Utc>>,
    pub check_in_time: Option<DateTime<Utc>>,
    pub check_out_time: Option<DateTime<Utc>>,
    #[serde(default = "default_true")]
    pub is_active: bool,
    pub username: Option<String>,
}

fn default_source_team() -> AssigneeType {
    AssigneeType::Team
}

/// 派工单操作日志
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchOrderLog {
    pub id: String,
    pub dispatch_order_id: String,
    pub action: String,
    pub actor_id: Option<String>,
    pub details: Option<serde_json::Value>,
    pub created_at: Option<DateTime<Utc>>,
}

/// 派工告警
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchAlert {
    pub id: String,
    pub flight_id: Option<String>,
    pub task_type: Option<String>,
    #[serde(default)]
    pub alert_type: String,
    #[serde(default = "default_warning")]
    pub severity: AlertSeverity,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub is_resolved: bool,
    pub resolved_at: Option<DateTime<Utc>>,
    pub resolved_by: Option<String>,
    pub resolution_notes: Option<String>,
    #[serde(default)]
    pub notify_users: Vec<String>,
    pub created_at: Option<DateTime<Utc>>,
    /// 幂等键,如 `dispatch_schedule_overrun:{current_order_id}:{next_order_id}`。
    /// 同一冲突只保留一条告警;关闭后再次出现时复用键但递增 `occurrence_count`。
    pub dedupe_key: Option<String>,
    /// 预排冲突中仍未完成的当前工单。
    pub current_order_id: Option<String>,
    /// 即将开始的下一工单。
    pub next_order_id: Option<String>,
    /// 最近一次检测到冲突的时间。
    pub last_detected_at: Option<DateTime<Utc>>,
    /// 同一冲突关闭后再次出现的次数,至少为 1。
    #[serde(default = "default_occurrence_count")]
    pub occurrence_count: i32,
    /// 调度员确认时间;确认不等于关闭。
    pub acknowledged_at: Option<DateTime<Utc>>,
    pub acknowledged_by: Option<String>,
    /// 告警结构化详情(共享人员/倒计时/预计冲突分钟/ETA 状态等)。
    #[serde(default)]
    pub details: serde_json::Value,
}

fn default_occurrence_count() -> i32 {
    1
}

/// 预排冲突告警幂等键前缀。
pub const DISPATCH_OVERRUN_DEDUPE_KEY_PREFIX: &str = "dispatch_schedule_overrun";

/// 构造预排冲突告警幂等键。
pub fn dispatch_overrun_dedupe_key(current_order_id: &str, next_order_id: &str) -> String {
    format!("{DISPATCH_OVERRUN_DEDUPE_KEY_PREFIX}:{current_order_id}:{next_order_id}")
}

fn default_warning() -> AlertSeverity {
    AlertSeverity::Warning
}

/// 系统默认预排冲突预警提前量（分钟）。
pub const DEFAULT_COMPLETION_WARNING_LEAD_MINUTES: i32 = 5;

/// 预排冲突预警提前量有效范围（分钟）。
pub const COMPLETION_WARNING_LEAD_RANGE_MIN: i32 = 0;
pub const COMPLETION_WARNING_LEAD_RANGE_MAX: i32 = 60;

/// 预警提前量来源，用于向调度员展示“实际生效值及来源”。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionWarningLeadSource {
    /// 工单级值：调度员单次覆盖或生成规则快照。
    Order,
    /// 部门级值：当前生成规则配置。
    Department,
    /// 系统默认值。
    System,
}

/// 校验单次预排冲突预警提前量（0..=60 分钟）。
pub fn validate_completion_warning_lead_minutes(value: i32) -> Result<(), DomainError> {
    if (COMPLETION_WARNING_LEAD_RANGE_MIN..=COMPLETION_WARNING_LEAD_RANGE_MAX).contains(&value) {
        Ok(())
    } else {
        Err(DomainError::ValidationError(format!(
            "completion_warning_lead_minutes 必须在 {}..={} 分钟之间",
            COMPLETION_WARNING_LEAD_RANGE_MIN, COMPLETION_WARNING_LEAD_RANGE_MAX
        )))
    }
}

/// 解析生效的预排冲突预警提前量。
///
/// 优先级：工单级值（单次覆盖或生成规则快照）> 部门级值（当前生成规则）>
/// 系统默认值 `DEFAULT_COMPLETION_WARNING_LEAD_MINUTES`。
/// `0` 表示下一单到达计划开始时间才触发预警。
pub fn resolve_completion_warning_lead_minutes(
    order_value: Option<i32>,
    department_value: Option<i32>,
) -> Result<(i32, CompletionWarningLeadSource), DomainError> {
    if let Some(value) = order_value {
        validate_completion_warning_lead_minutes(value)?;
        return Ok((value, CompletionWarningLeadSource::Order));
    }
    if let Some(value) = department_value {
        validate_completion_warning_lead_minutes(value)?;
        return Ok((value, CompletionWarningLeadSource::Department));
    }
    Ok((
        DEFAULT_COMPLETION_WARNING_LEAD_MINUTES,
        CompletionWarningLeadSource::System,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_order(
        _assignee: AssigneeType,
        individual_uid: Option<&str>,
        members: Vec<DispatchOrderMember>,
    ) -> DispatchOrder {
        let mut base = json!({
            "id": "do-1",
            "flight_id": "fl-1",
            "task_type": "boarding",
            "members": members,
        });
        if let Some(uid) = individual_uid {
            base["individual_user_id"] = json!(uid);
        }
        serde_json::from_value(base).unwrap()
    }

    fn member(uid: &str, active: bool) -> DispatchOrderMember {
        DispatchOrderMember {
            id: format!("m-{uid}"),
            dispatch_order_id: "do-1".into(),
            user_id: uid.into(),
            role: MemberRole::Member,
            source_type: AssigneeType::Team,
            source_team_id: None,
            slot_code: None,
            qualification_code: None,
            qualification_level_code: None,
            assigned_at: None,
            check_in_time: None,
            check_out_time: None,
            is_active: active,
            username: None,
        }
    }

    #[test]
    fn legacy_generation_rule_json_defaults_to_start_plus_duration() {
        let rule: FlightGenerationRule = serde_json::from_value(json!({
            "id": "rule-1",
            "department_id": "dept-1",
            "task_type": "boarding",
            "leg_scope": "outbound"
        }))
        .expect("legacy generation rule should remain readable");

        assert_eq!(rule.completion_time_mode, "start_plus_duration");
        assert_eq!(rule.completion_anchor_type, None);
        assert_eq!(rule.completion_offset_minutes, None);
    }

    fn team_member(uid: &str, active: bool) -> TeamMember {
        TeamMember {
            id: format!("tm-{uid}"),
            team_id: "team-1".into(),
            user_id: uid.into(),
            role: MemberRole::Member,
            can_drive: false,
            joined_at: None,
            left_at: None,
            is_active: active,
            username: None,
            user_display_name: None,
        }
    }

    fn team(status: TeamStatus, active: bool, members: Vec<TeamMember>) -> Team {
        Team {
            id: "team-1".into(),
            name: "Ramp Team".into(),
            department_id: None,
            team_type_id: None,
            code: None,
            leader_id: None,
            current_status: status,
            current_position_lat: None,
            current_position_lng: None,
            current_stand_id: None,
            last_position_update: None,
            created_at: None,
            updated_at: None,
            is_active: active,
            team_type: None,
            members,
        }
    }

    fn equipment(status: EquipmentStatus, active: bool, dispatch_id: Option<&str>) -> Equipment {
        Equipment {
            id: "eq-1".into(),
            code: "GPU-01".into(),
            equipment_type_id: None,
            department_id: None,
            name: None,
            license_plate: None,
            status,
            current_position_lat: None,
            current_position_lng: None,
            current_stand_id: None,
            last_position_update: None,
            current_dispatch_id: dispatch_id.map(str::to_string),
            last_maintenance_date: None,
            next_maintenance_date: None,
            metadata: None,
            created_at: None,
            updated_at: None,
            is_active: active,
            equipment_type: None,
        }
    }

    #[test]
    fn team_is_on_duty_requires_active_on_duty_status() {
        assert!(team(TeamStatus::OnDuty, true, vec![]).is_on_duty());
        assert!(!team(TeamStatus::OffDuty, true, vec![]).is_on_duty());
        assert!(!team(TeamStatus::Break, true, vec![]).is_on_duty());
        assert!(!team(TeamStatus::OnDuty, false, vec![]).is_on_duty());
    }

    #[test]
    fn team_has_member_only_counts_active_members() {
        let team = team(
            TeamStatus::OnDuty,
            true,
            vec![team_member("u1", true), team_member("u2", false)],
        );

        assert!(team.has_member("u1"));
        assert!(!team.has_member("u2"));
        assert!(!team.has_member("missing"));
        assert!(!team.has_member(" "));
    }

    #[test]
    fn team_can_accept_dispatch_requires_on_duty_active_members() {
        assert!(team(TeamStatus::OnDuty, true, vec![team_member("u1", true)]).can_accept_dispatch());
        assert!(!team(TeamStatus::OnDuty, true, vec![]).can_accept_dispatch());
        assert!(!team(TeamStatus::Break, true, vec![team_member("u1", true)]).can_accept_dispatch());
    }

    #[test]
    fn team_status_transitions_update_current_status() {
        let mut team = team(TeamStatus::OffDuty, true, vec![]);

        team.mark_on_duty();
        assert_eq!(team.current_status, TeamStatus::OnDuty);

        team.start_break();
        assert_eq!(team.current_status, TeamStatus::Break);

        team.mark_off_duty();
        assert_eq!(team.current_status, TeamStatus::OffDuty);
    }

    #[test]
    fn equipment_is_available_requires_active_available_status() {
        assert!(equipment(EquipmentStatus::Available, true, None).is_available());
        assert!(!equipment(EquipmentStatus::InUse, true, Some("do-1")).is_available());
        assert!(!equipment(EquipmentStatus::Maintenance, true, None).is_available());
        assert!(!equipment(EquipmentStatus::Retired, true, None).is_available());
        assert!(!equipment(EquipmentStatus::Available, false, None).is_available());
    }

    #[test]
    fn equipment_assigns_only_when_available() {
        let mut available = equipment(EquipmentStatus::Available, true, None);
        assert!(available.assign("do-1"));
        assert_eq!(available.status, EquipmentStatus::InUse);
        assert_eq!(available.current_dispatch_id.as_deref(), Some("do-1"));
        assert!(available.is_assigned());

        let mut in_use = equipment(EquipmentStatus::InUse, true, Some("do-1"));
        assert!(!in_use.assign("do-2"));
        assert_eq!(in_use.current_dispatch_id.as_deref(), Some("do-1"));

        let mut blank_dispatch = equipment(EquipmentStatus::Available, true, None);
        assert!(!blank_dispatch.assign(" "));
        assert_eq!(blank_dispatch.status, EquipmentStatus::Available);
    }

    #[test]
    fn equipment_release_clears_dispatch_and_restores_availability() {
        let mut equipment = equipment(EquipmentStatus::InUse, true, Some("do-1"));

        assert!(equipment.release());
        assert_eq!(equipment.status, EquipmentStatus::Available);
        assert_eq!(equipment.current_dispatch_id, None);
        assert!(equipment.is_available());

        assert!(!equipment.release());
    }

    #[test]
    fn equipment_maintenance_and_retire_clear_assignment() {
        let mut maintenance = equipment(EquipmentStatus::InUse, true, Some("do-1"));
        maintenance.send_to_maintenance();
        assert_eq!(maintenance.status, EquipmentStatus::Maintenance);
        assert_eq!(maintenance.current_dispatch_id, None);
        assert!(!maintenance.is_available());

        let mut retired = equipment(EquipmentStatus::Available, true, Some("do-1"));
        retired.retire();
        assert_eq!(retired.status, EquipmentStatus::Retired);
        assert_eq!(retired.current_dispatch_id, None);
        assert!(!retired.is_active);
        assert!(!retired.is_available());
    }

    #[test]
    fn team_order_member_can_start() {
        let order = make_order(AssigneeType::Team, None, vec![member("u1", true)]);
        assert!(order.can_be_started_by("u1"));
    }

    #[test]
    fn team_order_inactive_member_cannot_start() {
        let order = make_order(AssigneeType::Team, None, vec![member("u1", false)]);
        assert!(!order.can_be_started_by("u1"));
    }

    #[test]
    fn team_order_unknown_user_cannot_start() {
        let order = make_order(AssigneeType::Team, None, vec![member("u1", true)]);
        assert!(!order.can_be_started_by("u2"));
    }

    #[test]
    fn individual_order_assigned_user_can_start() {
        let order = make_order(AssigneeType::Individual, Some("u1"), vec![]);
        assert!(order.can_be_started_by("u1"));
    }

    #[test]
    fn individual_order_other_user_cannot_start() {
        let order = make_order(AssigneeType::Individual, Some("u1"), vec![]);
        assert!(!order.can_be_started_by("u2"));
    }

    #[test]
    fn can_be_completed_uses_same_logic_as_start() {
        let order = make_order(AssigneeType::Team, None, vec![member("u1", true)]);
        assert!(order.can_be_completed_by("u1"));
        assert!(!order.can_be_completed_by("u2"));
    }

    #[test]
    fn completion_warning_lead_resolver_prefers_order_value() {
        let (value, source) =
            resolve_completion_warning_lead_minutes(Some(12), Some(30)).expect("order value must resolve");
        assert_eq!(value, 12);
        assert_eq!(source, CompletionWarningLeadSource::Order);
    }

    #[test]
    fn completion_warning_lead_resolver_falls_back_to_department_value() {
        let (value, source) =
            resolve_completion_warning_lead_minutes(None, Some(30)).expect("department value must resolve");
        assert_eq!(value, 30);
        assert_eq!(source, CompletionWarningLeadSource::Department);
    }

    #[test]
    fn completion_warning_lead_resolver_defaults_to_system_five() {
        let (value, source) = resolve_completion_warning_lead_minutes(None, None).expect("system default must resolve");
        assert_eq!(value, DEFAULT_COMPLETION_WARNING_LEAD_MINUTES);
        assert_eq!(source, CompletionWarningLeadSource::System);
    }

    #[test]
    fn completion_warning_lead_resolver_accepts_zero_and_sixty() {
        let (value, _) = resolve_completion_warning_lead_minutes(Some(0), None).expect("0 is valid");
        assert_eq!(value, 0);
        let (value, _) = resolve_completion_warning_lead_minutes(None, Some(60)).expect("60 is valid");
        assert_eq!(value, 60);
    }

    #[test]
    fn completion_warning_lead_resolver_rejects_values_outside_range() {
        assert!(resolve_completion_warning_lead_minutes(Some(-1), None).is_err());
        assert!(resolve_completion_warning_lead_minutes(Some(61), None).is_err());
        assert!(resolve_completion_warning_lead_minutes(None, Some(-1)).is_err());
        assert!(resolve_completion_warning_lead_minutes(None, Some(61)).is_err());
    }
}
