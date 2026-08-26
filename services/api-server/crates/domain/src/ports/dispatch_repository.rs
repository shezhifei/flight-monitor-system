//! 派工系统仓储 trait
//!
//! 对应 Python `src/domain/repositories/dispatch_repository.py`。

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::error::DomainError;
use crate::models::dispatch::*;

/// 部门仓储接口
#[async_trait]
pub trait DepartmentRepository {
    async fn save(&self, dept: &Department) -> Result<Department, DomainError>;
    async fn find_by_id(&self, id: &str) -> Result<Option<Department>, DomainError>;
    async fn find_by_name(&self, name: &str) -> Result<Option<Department>, DomainError>;
    async fn find_all(&self, include_inactive: bool, limit: i64, offset: i64) -> Result<Vec<Department>, DomainError>;
    async fn has_dependencies(&self, department_id: &str) -> Result<bool, DomainError>;
    async fn delete_permanently(&self, department_id: &str) -> Result<bool, DomainError>;
}

/// 班组类型仓储接口
#[async_trait]
pub trait TeamTypeRepository {
    async fn save(&self, tt: &TeamType) -> Result<TeamType, DomainError>;
    async fn find_by_id(&self, id: &str) -> Result<Option<TeamType>, DomainError>;
    async fn find_all(&self, include_inactive: bool, limit: i64, offset: i64) -> Result<Vec<TeamType>, DomainError>;
    /// 根据作业类型查找关联的班组类型
    async fn find_by_task_type(&self, task_type: &str) -> Result<Vec<TeamType>, DomainError>;
    /// 软删除/恢复班组类型（is_active）。返回更新后的实体，找不到时返回 None。
    async fn set_active(&self, id: &str, is_active: bool) -> Result<Option<TeamType>, DomainError>;
}

/// 设备类型仓储接口
#[async_trait]
pub trait EquipmentTypeRepository {
    async fn save(&self, et: &EquipmentType) -> Result<EquipmentType, DomainError>;
    async fn find_by_id(&self, id: &str) -> Result<Option<EquipmentType>, DomainError>;
    async fn find_all(
        &self,
        include_inactive: bool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<EquipmentType>, DomainError>;
    /// 软删除/恢复设备类型（is_active）。返回更新后的实体，找不到时返回 None。
    async fn set_active(&self, id: &str, is_active: bool) -> Result<Option<EquipmentType>, DomainError>;
}

/// 派工单仓储接口
#[async_trait]
pub trait DispatchOrderRepository {
    async fn save(&self, order: &DispatchOrder) -> Result<(), DomainError>;
    async fn create_order_atomic(&self, command: CreateDispatchOrderCommand) -> Result<(), DomainError>;
    async fn save_orders_atomic(&self, commands: Vec<CreateDispatchOrderCommand>) -> Result<(), DomainError>;
    async fn find_by_id(
        &self,
        id: &str,
        load_members: bool,
        department: Option<&str>,
    ) -> Result<Option<DispatchOrder>, DomainError>;
    async fn find_by_flight(&self, flight_id: &str) -> Result<Vec<DispatchOrder>, DomainError>;
    async fn find_by_flight_with_filters(
        &self,
        flight_id: &str,
        status: Option<&str>,
        source: Option<&str>,
        department: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<DispatchOrder>, DomainError>;
    async fn find_by_team(
        &self,
        team_id: &str,
        status: Option<&str>,
        start_date: Option<DateTime<Utc>>,
        end_date: Option<DateTime<Utc>>,
    ) -> Result<Vec<DispatchOrder>, DomainError>;
    async fn find_by_team_filtered(
        &self,
        team_id: &str,
        status: Option<&str>,
        source: Option<&str>,
        department: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<DispatchOrder>, DomainError>;
    async fn find_by_user(&self, user_id: &str, status: Option<&str>) -> Result<Vec<DispatchOrder>, DomainError>;
    async fn find_all(
        &self,
        status: Option<&str>,
        department: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<DispatchOrder>, DomainError>;
    async fn find_all_filtered(
        &self,
        status: Option<&str>,
        source: Option<&str>,
        department: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<DispatchOrder>, DomainError>;
    async fn find_orders_in_window(
        &self,
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
        statuses: &[&str],
        source: Option<&str>,
        department: Option<&str>,
        terminal: Option<&str>,
        include_cancelled: bool,
    ) -> Result<Vec<DispatchOrder>, DomainError>;
    async fn find_overlapping_orders(
        &self,
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
        team_id: Option<&str>,
        individual_user_id: Option<&str>,
        stand_id: Option<&str>,
        exclude_order_id: Option<&str>,
    ) -> Result<Vec<DispatchOrder>, DomainError>;
    async fn find_equipment_conflicts(
        &self,
        equipment_ids: &[String],
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
        exclude_order_id: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, DomainError>;
    async fn list_logs(&self, dispatch_order_id: &str, limit: i64) -> Result<Vec<serde_json::Value>, DomainError>;
    async fn find_pending_for_flight(&self, flight_id: &str) -> Result<Vec<DispatchOrder>, DomainError>;
    async fn find_publishable_orders(
        &self,
        as_of: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<DispatchOrder>, DomainError>;
    async fn update_status(
        &self,
        id: &str,
        status: &str,
        actor_id: Option<&str>,
        enforce_actor_assignment: bool,
    ) -> Result<bool, DomainError>;
    async fn start_order(&self, id: &str, actual_start: DateTime<Utc>, actor_id: &str) -> Result<bool, DomainError>;
    async fn complete_order(
        &self,
        id: &str,
        actual_end: DateTime<Utc>,
        actor_id: &str,
        notes: Option<&str>,
    ) -> Result<bool, DomainError>;
    async fn append_log(
        &self,
        dispatch_order_id: &str,
        action: &str,
        actor_id: Option<&str>,
        details: Option<serde_json::Value>,
    ) -> Result<(), DomainError>;
    async fn append_log_once(
        &self,
        dispatch_order_id: &str,
        action: &str,
        actor_id: Option<&str>,
        details: serde_json::Value,
    ) -> Result<bool, DomainError>;
    async fn has_logged_action(
        &self,
        dispatch_order_id: &str,
        action: &str,
        actor_id: Option<&str>,
        client_action_id: Option<&str>,
    ) -> Result<bool, DomainError>;
    async fn report_estimated_completion(
        &self,
        id: &str,
        estimated_time: DateTime<Utc>,
        actor_id: &str,
        note: Option<&str>,
    ) -> Result<bool, DomainError>;
    async fn update_planned_times(
        &self,
        id: &str,
        planned_start: DateTime<Utc>,
        planned_end: DateTime<Utc>,
    ) -> Result<bool, DomainError>;
    async fn replace_order_equipment_assignments(&self, id: &str, equipment_ids: &[String]) -> Result<(), DomainError>;
}

#[async_trait]
pub trait DispatchOrderTransactionalRepository<Tx>: Send + Sync {
    async fn save_in_tx(&self, tx: &mut Tx, order: &DispatchOrder) -> Result<(), DomainError>;

    async fn append_log_in_tx(
        &self,
        tx: &mut Tx,
        dispatch_order_id: &str,
        action: &str,
        actor_id: Option<&str>,
        details: Option<serde_json::Value>,
    ) -> Result<(), DomainError>;
}

#[derive(Debug, Clone)]
pub struct CreateDispatchOrderCommand {
    pub order: DispatchOrder,
    pub members: Vec<DispatchOrderMember>,
    pub persist_equipment_assignments: bool,
    pub equipment_ids: Vec<String>,
    pub log_action: String,
    pub log_actor_id: Option<String>,
    pub log_details: Option<serde_json::Value>,
}

/// 班组仓储接口
#[async_trait]
pub trait TeamRepository {
    async fn save(&self, team: &Team) -> Result<Team, DomainError>;
    async fn find_by_id(&self, id: &str, load_members: bool) -> Result<Option<Team>, DomainError>;
    async fn find_by_code(&self, code: &str) -> Result<Option<Team>, DomainError>;
    async fn find_available_for_dispatch(
        &self,
        team_type_id: Option<&str>,
        terminal: Option<&str>,
    ) -> Result<Vec<Team>, DomainError>;
    async fn find_all(
        &self,
        include_inactive: bool,
        team_type_id: Option<&str>,
        terminal: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Team>, DomainError>;
    async fn update_position(&self, id: &str, lat: f64, lng: f64, stand_id: Option<&str>) -> Result<bool, DomainError>;
    async fn update_status(&self, id: &str, status: &str) -> Result<bool, DomainError>;
}

/// 班组成员仓储接口
#[async_trait]
pub trait TeamMemberRepository {
    async fn save(&self, member: &TeamMember) -> Result<TeamMember, DomainError>;
    async fn find_by_team(&self, team_id: &str, include_inactive: bool) -> Result<Vec<TeamMember>, DomainError>;
    async fn find_by_user(&self, user_id: &str) -> Result<Vec<TeamMember>, DomainError>;
    async fn list_active_users(&self) -> Result<Vec<String>, DomainError>;
    async fn remove_from_team(&self, team_id: &str, user_id: &str) -> Result<bool, DomainError>;
}

#[async_trait]
pub trait DepartmentQualificationRepository {
    async fn save_catalog(
        &self,
        catalog: &DepartmentQualificationCatalog,
    ) -> Result<DepartmentQualificationCatalog, DomainError>;
    async fn list_catalogs(
        &self,
        department_id: &str,
        include_inactive: bool,
    ) -> Result<Vec<DepartmentQualificationCatalog>, DomainError>;
    async fn save_level(
        &self,
        level: &DepartmentQualificationLevel,
    ) -> Result<DepartmentQualificationLevel, DomainError>;
    async fn list_levels(
        &self,
        department_id: &str,
        qualification_code: Option<&str>,
        include_inactive: bool,
    ) -> Result<Vec<DepartmentQualificationLevel>, DomainError>;
}

#[async_trait]
pub trait QualificationGrantRepository {
    async fn save(&self, grant: &QualificationGrant) -> Result<QualificationGrant, DomainError>;
    async fn find_by_department(
        &self,
        department_id: &str,
        at_time: Option<DateTime<Utc>>,
        user_ids: &[String],
        include_inactive: bool,
    ) -> Result<Vec<QualificationGrant>, DomainError>;
}

#[async_trait]
pub trait DepartmentTaskTypeRequirementRepository {
    async fn next_version_no(&self, department_id: &str, task_type: &str) -> Result<i32, DomainError>;
    async fn save(
        &self,
        version: &DepartmentTaskTypeRequirementVersion,
    ) -> Result<DepartmentTaskTypeRequirementVersion, DomainError>;
    async fn list_versions(
        &self,
        department_id: &str,
        task_type: Option<&str>,
        status: Option<&str>,
    ) -> Result<Vec<DepartmentTaskTypeRequirementVersion>, DomainError>;
    async fn find_by_id(&self, version_id: &str) -> Result<Option<DepartmentTaskTypeRequirementVersion>, DomainError>;
    async fn find_latest_draft(
        &self,
        department_id: &str,
        task_type: &str,
    ) -> Result<Option<DepartmentTaskTypeRequirementVersion>, DomainError>;
    async fn find_published(
        &self,
        department_id: &str,
        task_type: &str,
    ) -> Result<Option<DepartmentTaskTypeRequirementVersion>, DomainError>;
    async fn archive_published(&self, department_id: &str, task_type: &str) -> Result<i64, DomainError>;
}

#[async_trait]
pub trait FlightGenerationRuleRepository {
    async fn next_version_no(&self, department_id: &str, task_type: &str, leg_scope: &str) -> Result<i32, DomainError>;
    async fn save(&self, rule: &FlightGenerationRule) -> Result<FlightGenerationRule, DomainError>;
    /// Persist a newly published version and archive the version it replaces in one transaction.
    async fn save_replacing_published(
        &self,
        rule: &FlightGenerationRule,
        previous_rule_id: &str,
    ) -> Result<FlightGenerationRule, DomainError>;
    async fn find_by_id(&self, id: &str) -> Result<Option<FlightGenerationRule>, DomainError>;
    async fn list_rules(
        &self,
        department_id: &str,
        status: Option<&str>,
    ) -> Result<Vec<FlightGenerationRule>, DomainError>;
}

#[async_trait]
pub trait GenerationAdjustmentRuleRepository {
    async fn next_version_no(&self, department_id: &str, task_type: &str) -> Result<i32, DomainError>;
    async fn save(&self, rule: &GenerationAdjustmentRule) -> Result<GenerationAdjustmentRule, DomainError>;
    async fn save_replacing_published(
        &self,
        rule: &GenerationAdjustmentRule,
        previous_rule_id: &str,
    ) -> Result<GenerationAdjustmentRule, DomainError>;
    async fn find_by_id(&self, id: &str) -> Result<Option<GenerationAdjustmentRule>, DomainError>;
    async fn list_rules(
        &self,
        department_id: &str,
        status: Option<&str>,
    ) -> Result<Vec<GenerationAdjustmentRule>, DomainError>;
}

#[async_trait]
pub trait TemporaryTaskTemplateRepository {
    async fn save(&self, template: &TemporaryTaskTemplate) -> Result<TemporaryTaskTemplate, DomainError>;
    async fn find_by_code(
        &self,
        department_id: &str,
        template_code: &str,
    ) -> Result<Option<TemporaryTaskTemplate>, DomainError>;
    async fn list_templates(
        &self,
        department_id: &str,
        include_inactive: bool,
    ) -> Result<Vec<TemporaryTaskTemplate>, DomainError>;
}

#[async_trait]
pub trait ShiftTemplateRepository {
    async fn save(&self, template: &ShiftTemplate) -> Result<ShiftTemplate, DomainError>;
    async fn find_all(
        &self,
        resource_type: Option<&str>,
        resource_id: Option<&str>,
        enabled: Option<bool>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ShiftTemplate>, DomainError>;
}

#[async_trait]
pub trait ShiftInstanceRepository {
    async fn save(&self, instance: &ShiftInstance) -> Result<ShiftInstance, DomainError>;
    async fn find_all(
        &self,
        resource_type: Option<&str>,
        resource_id: Option<&str>,
        window_start: Option<DateTime<Utc>>,
        window_end: Option<DateTime<Utc>>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ShiftInstance>, DomainError>;
    async fn find_for_resource_window(
        &self,
        resource_type: &str,
        resource_id: &str,
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
    ) -> Result<Vec<ShiftInstance>, DomainError>;
}

#[async_trait]
pub trait ScheduleExceptionRepository {
    async fn save_leave_record(&self, record: &LeaveRecord) -> Result<LeaveRecord, DomainError>;
    async fn find_leave_records(
        &self,
        user_ids: &[String],
        team_id: Option<&str>,
        window_start: Option<DateTime<Utc>>,
        window_end: Option<DateTime<Utc>>,
    ) -> Result<Vec<LeaveRecord>, DomainError>;
    async fn save_equipment_downtime(&self, downtime: &EquipmentDowntime) -> Result<EquipmentDowntime, DomainError>;
    async fn find_equipment_downtimes(
        &self,
        equipment_ids: &[String],
        window_start: Option<DateTime<Utc>>,
        window_end: Option<DateTime<Utc>>,
    ) -> Result<Vec<EquipmentDowntime>, DomainError>;
    async fn save_lock_rule(&self, rule: &DispatchLockRule) -> Result<DispatchLockRule, DomainError>;
    async fn find_lock_rules(
        &self,
        dispatch_order_ids: &[String],
        team_id: Option<&str>,
        window_start: Option<DateTime<Utc>>,
        window_end: Option<DateTime<Utc>>,
    ) -> Result<Vec<DispatchLockRule>, DomainError>;
    async fn list_exceptions(
        &self,
        window_start: Option<DateTime<Utc>>,
        window_end: Option<DateTime<Utc>>,
        limit: i64,
    ) -> Result<Vec<serde_json::Value>, DomainError>;
}

/// 设备仓储接口
#[async_trait]
pub trait EquipmentRepository {
    async fn save(&self, equipment: &Equipment) -> Result<Equipment, DomainError>;
    async fn find_by_id(&self, id: &str) -> Result<Option<Equipment>, DomainError>;
    async fn find_by_code(&self, code: &str) -> Result<Option<Equipment>, DomainError>;
    async fn find_available_for_dispatch(
        &self,
        equipment_type_id: Option<&str>,
        terminal: Option<&str>,
    ) -> Result<Vec<Equipment>, DomainError>;
    async fn find_all(
        &self,
        include_inactive: bool,
        equipment_type_id: Option<&str>,
        terminal: Option<&str>,
        status: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Equipment>, DomainError>;
    async fn update_position(&self, id: &str, lat: f64, lng: f64, stand_id: Option<&str>) -> Result<bool, DomainError>;
    async fn update_status(&self, id: &str, status: &str) -> Result<bool, DomainError>;
}

/// 人员在岗运行时仓储接口（personnel_runtime）。
///
/// 无行视为 `off_duty`：`update_status` / `update_position` 返回 `false` 表示目标
/// 行不存在，调用方按需 upsert。`save` 负责写入/更新整行（含首次建行）。
#[async_trait]
pub trait PersonnelRuntimeRepository {
    async fn save(&self, runtime: &PersonnelRuntime) -> Result<PersonnelRuntime, DomainError>;
    async fn find_by_user(&self, user_id: &str) -> Result<Option<PersonnelRuntime>, DomainError>;
    async fn update_status(
        &self,
        user_id: &str,
        status: &str,
        updated_by: Option<&str>,
    ) -> Result<bool, DomainError>;
    async fn update_position(
        &self,
        user_id: &str,
        lat: f64,
        lng: f64,
        stand_id: Option<&str>,
    ) -> Result<bool, DomainError>;
}

/// 目录设施 allocate 前校验的落点（PR3「allocate 校验楼成员」）。
///
/// 依据「code 在目录、is_active、且成员表挂在启用的楼上」三条件：
/// - `Unknown`：code 不在目录中；
/// - `Inactive`：目录行存在但未启用；
/// - `NoTerminal`：目录行存在且启用，但未挂在任何楼上；
/// - `Terminal { code, active }`：挂在楼上，`active` 为该楼是否启用。
#[derive(Debug, Clone)]
pub enum FacilityLocale {
    Unknown,
    Inactive,
    NoTerminal,
    Terminal { code: String, active: bool },
}

/// 航站楼目录仓储接口。
///
/// 目录行（terminal/gate/carousel）与楼成员表（terminal_stands/gates/carousels）
/// 由同一仓储维护。成员关系是构成事实：新建口/转盘的目录行必须由楼 `add_*`
/// 原子带上（create + add 同事务）。无数据库外键，参照完整性在应用层保证。
#[async_trait]
pub trait TerminalRepository {
    // -- Terminal 目录 --
    async fn save_terminal(&self, terminal: &Terminal) -> Result<Terminal, DomainError>;
    async fn find_terminal_by_id(&self, terminal_id: &str) -> Result<Option<Terminal>, DomainError>;
    async fn find_terminal_by_code(&self, code: &str) -> Result<Option<Terminal>, DomainError>;
    async fn find_terminals(&self, include_inactive: bool) -> Result<Vec<Terminal>, DomainError>;
    /// 软启停楼（is_active）。返回更新后的实体，找不到返回 None。
    async fn set_terminal_active(&self, terminal_id: &str, is_active: bool) -> Result<Option<Terminal>, DomainError>;

    // -- Gate 目录 --
    async fn save_gate(&self, gate: &Gate) -> Result<Gate, DomainError>;
    async fn find_gate_by_id(&self, gate_id: &str) -> Result<Option<Gate>, DomainError>;
    async fn find_gate_by_code(&self, code: &str) -> Result<Option<Gate>, DomainError>;
    /// 软启停登机口（is_active）。返回更新后的实体，找不到返回 None。
    async fn set_gate_active(&self, gate_id: &str, is_active: bool) -> Result<Option<Gate>, DomainError>;

    // -- BaggageCarousel 目录 --
    async fn save_carousel(&self, carousel: &BaggageCarousel) -> Result<BaggageCarousel, DomainError>;
    async fn find_carousel_by_id(&self, carousel_id: &str) -> Result<Option<BaggageCarousel>, DomainError>;
    async fn find_carousel_by_code(&self, code: &str) -> Result<Option<BaggageCarousel>, DomainError>;
    /// 软启停行李转盘（is_active）。返回更新后的实体，找不到返回 None。
    async fn set_carousel_active(
        &self,
        carousel_id: &str,
        is_active: bool,
    ) -> Result<Option<BaggageCarousel>, DomainError>;

    // -- Terminal 成员关系（构成事实）--
    /// 按 id 取机位目录行（用于把 stand_id 映射到 code 以做占用守卫）。
    async fn find_stand_by_id(&self, stand_id: &str) -> Result<Option<Stand>, DomainError>;
    async fn add_stand(&self, terminal_id: &str, stand_id: &str) -> Result<(), DomainError>;
    async fn remove_stand(&self, stand_id: &str) -> Result<(), DomainError>;
    async fn add_gate(&self, terminal_id: &str, gate_id: &str) -> Result<(), DomainError>;
    async fn remove_gate(&self, gate_id: &str) -> Result<(), DomainError>;
    async fn add_carousel(&self, terminal_id: &str, carousel_id: &str) -> Result<(), DomainError>;
    async fn remove_carousel(&self, carousel_id: &str) -> Result<(), DomainError>;

    // -- 占用守卫（移出成员/停用楼时校验“未结束占用”）--
    /// 返回指定机位 code 的未结束（status='active' 且 ends_at > now）占用明细。
    async fn active_stand_occupations(&self, stand_code: &str) -> Result<Vec<serde_json::Value>, DomainError>;
    /// 返回指定登机口 code 的未结束分配明细。
    async fn active_gate_assignments(&self, gate_code: &str) -> Result<Vec<serde_json::Value>, DomainError>;
    /// 返回指定转盘 code 的未结束分配明细。
    async fn active_carousel_assignments(&self, carousel_code: &str) -> Result<Vec<serde_json::Value>, DomainError>;

    // -- 只读上下文 --
    /// 返回楼 + 三类成员目录行；楼不存在返回 Ok(None)。
    async fn terminal_directory(&self, terminal_id: &str) -> Result<Option<TerminalDirectory>, DomainError>;

    // -- allocate 前校验楼成员（PR3）--
    /// 按机位 code 解析落点：目录存在且启用、且挂在某座楼上（楼启用与否见 `Terminal.active`）。
    async fn stand_locale_by_code(&self, code: &str) -> Result<FacilityLocale, DomainError>;
    /// 按登机口 code 解析落点（同上）。
    async fn gate_locale_by_code(&self, code: &str) -> Result<FacilityLocale, DomainError>;
    /// 按转盘 code 解析落点（同上）。
    async fn carousel_locale_by_code(&self, code: &str) -> Result<FacilityLocale, DomainError>;
}

/// 机位仓储接口
#[async_trait]
pub trait StandRepository {
    async fn save(&self, stand: &Stand) -> Result<Stand, DomainError>;
    async fn find_by_id(&self, id: &str) -> Result<Option<Stand>, DomainError>;
    async fn find_by_code(&self, code: &str) -> Result<Option<Stand>, DomainError>;
    async fn find_all(
        &self,
        terminal: Option<&str>,
        include_inactive: bool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Stand>, DomainError>;
    /// Whether a stand matching `id_or_code` (id OR code) exists and is active.
    ///
    /// Returns `Ok(true)` if active, `Ok(false)` if found but inactive, and
    /// `Err(DomainError::NotFound { entity_type: "stand", .. })` if no stand matches.
    async fn is_active(&self, id_or_code: &str) -> Result<bool, DomainError>;
}

/// 作业类型仓储接口
#[async_trait]
pub trait TaskTypeRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<TaskType>, DomainError>;
    async fn find_by_code(&self, code: &str) -> Result<Option<TaskType>, DomainError>;
    async fn find_all(&self, category: Option<&str>, limit: i64, offset: i64) -> Result<Vec<TaskType>, DomainError>;
    async fn save(&self, task_type: &TaskType) -> Result<TaskType, DomainError>;
}

/// 派工告警仓储接口
#[async_trait]
pub trait DispatchAlertRepository {
    async fn save(&self, alert: &DispatchAlert) -> Result<(), DomainError>;
    async fn find_by_id(&self, id: &str) -> Result<Option<DispatchAlert>, DomainError>;
    async fn find_unresolved(&self, flight_id: Option<&str>) -> Result<Vec<DispatchAlert>, DomainError>;
    async fn resolve(&self, id: &str, resolved_by: &str, notes: Option<&str>) -> Result<bool, DomainError>;
    /// 按 `dedupe_key` 幂等写入预排冲突告警。
    ///
    /// 已存在且未关闭的告警原地更新(刷新消息/详情/最近检测时间),不重复通知;
    /// 已关闭的告警重新打开,递增 `occurrence_count` 并清空确认状态。
    /// 必须对事件与扫描器的并发调用安全。
    async fn upsert_overrun(&self, alert: &DispatchAlert) -> Result<OverrunAlertUpsert, DomainError>;
    /// 记录调度员确认;确认不等于关闭,已关闭告警返回 false。
    async fn acknowledge(&self, id: &str, acknowledged_by: &str) -> Result<bool, DomainError>;
    /// 系统自动关闭告警(实际完成/下一单取消/人员调整/冲突消失),无人工操作者。
    async fn auto_resolve(&self, id: &str) -> Result<bool, DomainError>;
}

/// 预排冲突告警幂等写入结果。
#[derive(Debug, Clone)]
pub struct OverrunAlertUpsert {
    /// 写入后的告警。
    pub alert: DispatchAlert,
    /// 本次是新建(此前不存在同键告警)。
    pub inserted: bool,
    /// 本次把已关闭的告警重新打开(occurrence_count 已递增)。
    pub reopened: bool,
}

/// 派工单成员仓储接口（对应 Python member_repo）
#[async_trait]
pub trait DispatchOrderMemberRepository {
    async fn save(&self, member: &DispatchOrderMember) -> Result<(), DomainError>;
    async fn find_by_order(&self, order_id: &str) -> Result<Vec<DispatchOrderMember>, DomainError>;
    async fn find_by_order_and_user(
        &self,
        order_id: &str,
        user_id: &str,
    ) -> Result<Option<DispatchOrderMember>, DomainError>;
    async fn find_latest_checkout_for_user(
        &self,
        user_id: &str,
        before: DateTime<Utc>,
    ) -> Result<Option<serde_json::Value>, DomainError>;

    /// 批量查询一批个人用户在已发布且进行中的工单上的活跃槽位。
    ///
    /// 供调度网关在线列表使用：一次查出多个在线用户的所有活跃工单槽，再按人聚合，
    /// 避免对每人单独 `find_by_user`。每条返回
    /// `user_id / order_id / flight_id / flight_no / task_type / task_type_name /
    /// slot_code / slot_name / status / planned_start_time`。默认空实现供测试替身。
    async fn find_active_slots_for_users(
        &self,
        _user_ids: &[String],
    ) -> Result<Vec<serde_json::Value>, DomainError> {
        Ok(Vec::new())
    }
}

#[async_trait]
pub trait DispatchOrderMemberTransactionalRepository<Tx>: Send + Sync {
    async fn save_in_tx(&self, tx: &mut Tx, member: &DispatchOrderMember) -> Result<(), DomainError>;
}

/// 机位间穿梭时间统计仓储（对应 Python travel_stats_repo）
#[async_trait]
pub trait DispatchTravelStatsRepository {
    /// 记录一次从 from_stand 到 to_stand 的实际旅途时间
    async fn record_travel(
        &self,
        from_stand_id: &str,
        to_stand_id: &str,
        travel_minutes: f64,
    ) -> Result<(), DomainError>;
    /// 查询两个机位之间的历史平均旅途分钟数
    async fn get_average_travel(&self, from_stand_id: &str, to_stand_id: &str) -> Result<Option<f64>, DomainError>;
}

/// 安全检查清单仓储（对应 Python checklist_service 的数据层）
#[async_trait]
pub trait DispatchChecklistRepository {
    /// 获取某作业类型的安全检查模板
    async fn get_template(&self, task_type: &str) -> Result<Option<serde_json::Value>, DomainError>;
    /// 新增或更新某作业类型的安全检查模板
    async fn upsert_template(
        &self,
        template_id: &str,
        task_type: &str,
        checklist_version: &str,
        checklist_items: &[serde_json::Value],
        is_active: bool,
        actor_user_id: Option<&str>,
    ) -> Result<serde_json::Value, DomainError>;
    /// 获取派工单已提交的检查项记录
    async fn list_records(&self, dispatch_order_id: &str) -> Result<Vec<serde_json::Value>, DomainError>;
    /// 提交一条检查项结果
    async fn submit_item_result(
        &self,
        dispatch_order_id: &str,
        task_type: &str,
        item_code: &str,
        result: Option<&str>,
        note: Option<&str>,
        checked_by: &str,
    ) -> Result<serde_json::Value, DomainError>;
    /// 评估完工门禁：是否还有未完成的安全检查项
    async fn evaluate_completion_gate(&self, dispatch_order_id: &str, task_type: &str) -> Result<bool, DomainError>;
}
