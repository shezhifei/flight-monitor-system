use std::sync::Arc;

use crate::schemas::dispatch_schemas::{
    DepartmentCreate, DepartmentUpdate, EquipmentCreate, EquipmentTypeCreate, EquipmentTypeUpdate, EquipmentUpdate,
    PositionUpdate, StandCreate, TaskTypeCreate, TeamCreate, TeamMemberAdd, TeamTypeCreate, TeamTypeUpdate, TeamUpdate,
};
use fms_domain::error::DomainError;
use fms_domain::models::dispatch::{
    Department, Equipment, EquipmentStatus, EquipmentType, MemberRole, PersonnelRuntime, PersonnelStatus, Stand,
    TaskType, Team, TeamMember, TeamStatus, TeamType,
};
use fms_domain::ports::dispatch_repository::{
    DepartmentRepository, EquipmentRepository, EquipmentTypeRepository, PersonnelRuntimeRepository, StandRepository,
    TaskTypeRepository, TeamMemberRepository, TeamRepository, TeamTypeRepository,
};
use fms_domain::ports::user_repository::UserRepository;

use super::mappers::{equipment_status_value, parse_personnel_status};

pub struct DispatchResourceService<
    DR: DepartmentRepository + ?Sized,
    TTR: TeamTypeRepository + ?Sized,
    TR: TeamRepository + ?Sized,
    TMR: TeamMemberRepository + ?Sized,
    ETR: EquipmentTypeRepository + ?Sized,
    ER: EquipmentRepository + ?Sized,
    SR: StandRepository + ?Sized,
    TTR2: TaskTypeRepository + ?Sized,
    PRR: PersonnelRuntimeRepository + ?Sized,
    UR: UserRepository + ?Sized,
> {
    department_repo: Arc<DR>,
    team_type_repo: Arc<TTR>,
    team_repo: Arc<TR>,
    team_member_repo: Arc<TMR>,
    equipment_type_repo: Arc<ETR>,
    equipment_repo: Arc<ER>,
    stand_repo: Arc<SR>,
    task_type_repo: Arc<TTR2>,
    personnel_runtime_repo: Arc<PRR>,
    user_repo: Arc<UR>,
}

impl<
        DR: DepartmentRepository + ?Sized,
        TTR: TeamTypeRepository + ?Sized,
        TR: TeamRepository + ?Sized,
        TMR: TeamMemberRepository + ?Sized,
        ETR: EquipmentTypeRepository + ?Sized,
        ER: EquipmentRepository + ?Sized,
        SR: StandRepository + ?Sized,
        TTR2: TaskTypeRepository + ?Sized,
        PRR: PersonnelRuntimeRepository + ?Sized,
        UR: UserRepository + ?Sized,
    > DispatchResourceService<DR, TTR, TR, TMR, ETR, ER, SR, TTR2, PRR, UR>
{
    pub fn new(
        department_repo: Arc<DR>,
        team_type_repo: Arc<TTR>,
        team_repo: Arc<TR>,
        team_member_repo: Arc<TMR>,
        equipment_type_repo: Arc<ETR>,
        equipment_repo: Arc<ER>,
        stand_repo: Arc<SR>,
        task_type_repo: Arc<TTR2>,
        personnel_runtime_repo: Arc<PRR>,
        user_repo: Arc<UR>,
    ) -> Self {
        Self {
            department_repo,
            team_type_repo,
            team_repo,
            team_member_repo,
            equipment_type_repo,
            equipment_repo,
            stand_repo,
            task_type_repo,
            personnel_runtime_repo,
            user_repo,
        }
    }

    pub async fn list_departments(
        &self,
        include_inactive: bool,
        page: i64,
        page_size: i64,
    ) -> Result<Vec<Department>, DomainError> {
        self.department_repo
            .find_all(
                include_inactive,
                normalize_page_size(page_size),
                offset(page, page_size),
            )
            .await
    }

    pub async fn get_department(&self, department_id: &str) -> Result<Option<Department>, DomainError> {
        self.department_repo.find_by_id(department_id).await
    }

    pub async fn create_department(&self, payload: DepartmentCreate) -> Result<Department, DomainError> {
        let department = Department {
            id: ulid::Ulid::new().to_string(),
            name: require_non_empty(&payload.name, "name")?,
            code: normalize_optional_string(payload.code),
            description: normalize_optional_string(payload.description),
            manager_id: normalize_optional_string(payload.manager_id),
            terminal: normalize_optional_string(payload.terminal),
            created_at: None,
            updated_at: None,
            is_active: true,
        };

        self.department_repo.save(&department).await
    }

    pub async fn update_department(
        &self,
        department_id: &str,
        payload: DepartmentUpdate,
    ) -> Result<Department, DomainError> {
        let mut department = self
            .department_repo
            .find_by_id(department_id)
            .await?
            .ok_or_else(|| not_found("department", department_id))?;

        if let Some(name) = payload.name {
            department.name = require_non_empty(&name, "name")?;
        }
        if payload.code.is_some() {
            department.code = normalize_optional_string(payload.code);
        }
        if payload.description.is_some() {
            department.description = normalize_optional_string(payload.description);
        }
        if payload.manager_id.is_some() {
            department.manager_id = normalize_optional_string(payload.manager_id);
        }
        if payload.terminal.is_some() {
            department.terminal = normalize_optional_string(payload.terminal);
        }
        if let Some(is_active) = payload.is_active {
            department.is_active = is_active;
        }

        self.department_repo.save(&department).await
    }

    pub async fn list_team_types(
        &self,
        include_inactive: bool,
        page: i64,
        page_size: i64,
    ) -> Result<Vec<TeamType>, DomainError> {
        self.team_type_repo
            .find_all(
                include_inactive,
                normalize_page_size(page_size),
                offset(page, page_size),
            )
            .await
    }

    pub async fn get_team_type(&self, team_type_id: &str) -> Result<Option<TeamType>, DomainError> {
        self.team_type_repo.find_by_id(team_type_id).await
    }

    pub async fn create_team_type(&self, payload: TeamTypeCreate) -> Result<TeamType, DomainError> {
        let team_type = TeamType {
            id: ulid::Ulid::new().to_string(),
            name: require_non_empty(&payload.name, "name")?,
            department_id: normalize_optional_string(payload.department_id),
            code: normalize_optional_string(payload.code),
            description: normalize_optional_string(payload.description),
            color: normalize_optional_string(payload.color),
            is_driver_type: payload.is_driver_type,
            created_at: None,
            updated_at: None,
            is_active: true,
            task_types: normalize_string_list(payload.task_types),
        };

        self.team_type_repo.save(&team_type).await
    }

    pub async fn update_team_type(&self, team_type_id: &str, payload: TeamTypeUpdate) -> Result<TeamType, DomainError> {
        let mut team_type = self
            .team_type_repo
            .find_by_id(team_type_id)
            .await?
            .ok_or_else(|| not_found("team_type", team_type_id))?;

        if let Some(name) = payload.name {
            team_type.name = require_non_empty(&name, "name")?;
        }
        if payload.department_id.is_some() {
            team_type.department_id = normalize_optional_string(payload.department_id);
        }
        if payload.code.is_some() {
            team_type.code = normalize_optional_string(payload.code);
        }
        if payload.description.is_some() {
            team_type.description = normalize_optional_string(payload.description);
        }
        if payload.color.is_some() {
            team_type.color = normalize_optional_string(payload.color);
        }
        if let Some(is_driver_type) = payload.is_driver_type {
            team_type.is_driver_type = is_driver_type;
        }
        if let Some(task_types) = payload.task_types {
            team_type.task_types = normalize_string_list(task_types);
        }

        self.team_type_repo.save(&team_type).await
    }

    pub async fn delete_team_type(&self, team_type_id: &str) -> Result<(), DomainError> {
        let updated = self.team_type_repo.set_active(team_type_id, false).await?;
        if updated.is_none() {
            return Err(not_found("team_type", team_type_id));
        }
        Ok(())
    }

    pub async fn list_teams(
        &self,
        include_inactive: bool,
        team_type_id: Option<&str>,
        terminal: Option<&str>,
        page: i64,
        page_size: i64,
    ) -> Result<Vec<Team>, DomainError> {
        self.team_repo
            .find_all(
                include_inactive,
                normalize_optional_ref(team_type_id),
                normalize_optional_ref(terminal),
                normalize_page_size(page_size),
                offset(page, page_size),
            )
            .await
    }

    pub async fn get_team(&self, team_id: &str, load_members: bool) -> Result<Option<Team>, DomainError> {
        self.team_repo.find_by_id(team_id, load_members).await
    }

    /// 创建班组：`department_id` 必填且科室须存在；该科室经理或 admin 可建（计划 :182）。
    /// PR2 起不再写 team_type_id / terminal（保留列仅供历史读取）。
    pub async fn create_team(&self, payload: TeamCreate, actor_id: &str) -> Result<Team, DomainError> {
        let department_id = self.require_existing_department(&payload.department_id).await?;
        self.assert_department_scope(actor_id, Some(&department_id), "teams")
            .await?;
        let team = Team {
            id: ulid::Ulid::new().to_string(),
            name: require_non_empty(&payload.name, "name")?,
            department_id: Some(department_id),
            team_type_id: None,
            code: normalize_optional_string(payload.code),
            leader_id: normalize_optional_string(payload.leader_id),
            current_status: TeamStatus::OffDuty,
            current_position_lat: None,
            current_position_lng: None,
            current_stand_id: None,
            last_position_update: None,
            created_at: None,
            updated_at: None,
            is_active: true,
            team_type: None,
            members: Vec::new(),
        };

        self.team_repo.save(&team).await
    }

    pub async fn update_team(&self, team_id: &str, payload: TeamUpdate, actor_id: &str) -> Result<Team, DomainError> {
        let mut team = self
            .team_repo
            .find_by_id(team_id, true)
            .await?
            .ok_or_else(|| not_found("team", team_id))?;
        self.assert_department_scope(actor_id, team.department_id.as_deref(), "teams")
            .await?;

        if let Some(name) = payload.name {
            team.name = require_non_empty(&name, "name")?;
        }
        if let Some(department_id) = payload.department_id {
            // 换科室：新科室须存在，且操作者须同时是新科室经理或 admin。
            let department_id = self.require_existing_department(&department_id).await?;
            self.assert_department_scope(actor_id, Some(&department_id), "teams")
                .await?;
            team.department_id = Some(department_id);
        }
        if payload.code.is_some() {
            team.code = normalize_optional_string(payload.code);
        }
        if payload.leader_id.is_some() {
            team.leader_id = normalize_optional_string(payload.leader_id);
        }
        if let Some(status) = payload.current_status {
            team.current_status = parse_team_status(&status)?;
        }
        if let Some(is_active) = payload.is_active {
            team.is_active = is_active;
        }

        self.team_repo.save(&team).await
    }

    pub async fn delete_team(&self, team_id: &str, actor_id: &str) -> Result<(), DomainError> {
        let mut team = self
            .team_repo
            .find_by_id(team_id, true)
            .await?
            .ok_or_else(|| not_found("team", team_id))?;
        self.assert_department_scope(actor_id, team.department_id.as_deref(), "teams")
            .await?;
        team.is_active = false;
        self.team_repo.save(&team).await?;
        Ok(())
    }

    pub async fn update_team_position(&self, team_id: &str, payload: PositionUpdate) -> Result<(), DomainError> {
        validate_position(payload.lat, payload.lng)?;
        let updated = self
            .team_repo
            .update_position(
                team_id,
                payload.lat,
                payload.lng,
                normalize_optional_ref(payload.stand_id.as_deref()),
            )
            .await?;
        if !updated {
            return Err(not_found("team", team_id));
        }
        Ok(())
    }

    pub async fn update_team_status(&self, team_id: &str, status: &str) -> Result<(), DomainError> {
        let normalized = super::mappers::team_status_value(parse_team_status(status)?);
        let updated = self.team_repo.update_status(team_id, normalized).await?;
        if !updated {
            return Err(not_found("team", team_id));
        }
        Ok(())
    }

    pub async fn list_team_members(
        &self,
        team_id: &str,
        include_inactive: bool,
    ) -> Result<Vec<TeamMember>, DomainError> {
        self.team_member_repo.find_by_team(team_id, include_inactive).await
    }

    /// 入组（计划 :191）：必须是个人账号、科室等于班组科室、一人一条活跃
    /// `team_members`；岗位账号或跨科室入组 409。边界：该班组科室经理或 admin。
    pub async fn add_team_member(
        &self,
        team_id: &str,
        payload: TeamMemberAdd,
        actor_id: &str,
    ) -> Result<TeamMember, DomainError> {
        let team = self
            .team_repo
            .find_by_id(team_id, false)
            .await?
            .ok_or_else(|| not_found("team", team_id))?;
        self.assert_department_scope(actor_id, team.department_id.as_deref(), "team roster")
            .await?;
        let role = if payload.role.trim().is_empty() {
            MemberRole::Member
        } else {
            parse_member_role(&payload.role)?
        };
        let user_id = require_non_empty(&payload.user_id, "user_id")?;
        self.validate_team_membership_rules(&team, &user_id).await?;
        let member = TeamMember {
            id: ulid::Ulid::new().to_string(),
            team_id: team_id.to_string(),
            user_id,
            role,
            can_drive: payload.can_drive,
            joined_at: None,
            left_at: None,
            is_active: true,
            username: None,
            user_display_name: None,
        };

        self.team_member_repo.save(&member).await
    }

    pub async fn remove_team_member(&self, team_id: &str, user_id: &str, actor_id: &str) -> Result<(), DomainError> {
        let team = self
            .team_repo
            .find_by_id(team_id, false)
            .await?
            .ok_or_else(|| not_found("team", team_id))?;
        self.assert_department_scope(actor_id, team.department_id.as_deref(), "team roster")
            .await?;
        let removed = self.team_member_repo.remove_from_team(team_id, user_id).await?;
        if !removed {
            return Err(not_found("team_member", user_id));
        }
        Ok(())
    }

    pub async fn list_equipment_types(
        &self,
        include_inactive: bool,
        page: i64,
        page_size: i64,
    ) -> Result<Vec<EquipmentType>, DomainError> {
        self.equipment_type_repo
            .find_all(
                include_inactive,
                normalize_page_size(page_size),
                offset(page, page_size),
            )
            .await
    }

    pub async fn create_equipment_type(&self, payload: EquipmentTypeCreate) -> Result<EquipmentType, DomainError> {
        let equipment_type = EquipmentType {
            id: ulid::Ulid::new().to_string(),
            name: require_non_empty(&payload.name, "name")?,
            code: normalize_optional_string(payload.code),
            category: normalize_optional_string(payload.category),
            requires_driver: payload.requires_driver,
            icon: normalize_optional_string(payload.icon),
            description: normalize_optional_string(payload.description),
            created_at: None,
            is_active: true,
            task_types: Vec::new(),
        };

        self.equipment_type_repo.save(&equipment_type).await
    }

    pub async fn update_equipment_type(
        &self,
        equipment_type_id: &str,
        payload: EquipmentTypeUpdate,
    ) -> Result<EquipmentType, DomainError> {
        let mut equipment_type = self
            .equipment_type_repo
            .find_by_id(equipment_type_id)
            .await?
            .ok_or_else(|| not_found("equipment_type", equipment_type_id))?;

        if let Some(name) = payload.name {
            equipment_type.name = require_non_empty(&name, "name")?;
        }
        if payload.code.is_some() {
            equipment_type.code = normalize_optional_string(payload.code);
        }
        if payload.category.is_some() {
            equipment_type.category = normalize_optional_string(payload.category);
        }
        if let Some(requires_driver) = payload.requires_driver {
            equipment_type.requires_driver = requires_driver;
        }
        if payload.icon.is_some() {
            equipment_type.icon = normalize_optional_string(payload.icon);
        }
        if payload.description.is_some() {
            equipment_type.description = normalize_optional_string(payload.description);
        }

        self.equipment_type_repo.save(&equipment_type).await
    }

    pub async fn delete_equipment_type(&self, equipment_type_id: &str) -> Result<(), DomainError> {
        let updated = self.equipment_type_repo.set_active(equipment_type_id, false).await?;
        if updated.is_none() {
            return Err(not_found("equipment_type", equipment_type_id));
        }
        Ok(())
    }

    pub async fn list_equipment(
        &self,
        include_inactive: bool,
        equipment_type_id: Option<&str>,
        terminal: Option<&str>,
        status: Option<&str>,
        page: i64,
        page_size: i64,
    ) -> Result<Vec<Equipment>, DomainError> {
        let normalized_status = if let Some(value) = normalize_optional_ref(status) {
            Some(equipment_status_value(parse_equipment_status(value)?))
        } else {
            None
        };

        self.equipment_repo
            .find_all(
                include_inactive,
                normalize_optional_ref(equipment_type_id),
                normalize_optional_ref(terminal),
                normalized_status,
                normalize_page_size(page_size),
                offset(page, page_size),
            )
            .await
    }

    pub async fn get_equipment(&self, equipment_id: &str) -> Result<Option<Equipment>, DomainError> {
        self.equipment_repo.find_by_id(equipment_id).await
    }

    /// 创建设备：`department_id` 必填且科室须存在；该科室经理或 admin 可建（计划 :182）。
    /// PR2 起不再写 terminal（设备无常驻楼字段，保留列仅供历史查询）。
    pub async fn create_equipment(&self, payload: EquipmentCreate, actor_id: &str) -> Result<Equipment, DomainError> {
        let department_id = self.require_existing_department(&payload.department_id).await?;
        self.assert_department_scope(actor_id, Some(&department_id), "equipment")
            .await?;
        let equipment = Equipment {
            id: ulid::Ulid::new().to_string(),
            code: require_non_empty(&payload.code, "code")?,
            equipment_type_id: normalize_optional_string(payload.equipment_type_id),
            department_id: Some(department_id),
            name: normalize_optional_string(payload.name),
            license_plate: normalize_optional_string(payload.license_plate),
            status: EquipmentStatus::Available,
            current_position_lat: None,
            current_position_lng: None,
            current_stand_id: None,
            last_position_update: None,
            current_dispatch_id: None,
            last_maintenance_date: None,
            next_maintenance_date: payload.next_maintenance_date,
            metadata: None,
            created_at: None,
            updated_at: None,
            is_active: true,
            equipment_type: None,
        };

        self.equipment_repo.save(&equipment).await
    }

    pub async fn update_equipment(
        &self,
        equipment_id: &str,
        payload: EquipmentUpdate,
        actor_id: &str,
    ) -> Result<Equipment, DomainError> {
        let mut equipment = self
            .equipment_repo
            .find_by_id(equipment_id)
            .await?
            .ok_or_else(|| not_found("equipment", equipment_id))?;
        // 历史设备 department_id 可能为空：仅 admin 可改（见 assert_department_scope）。
        self.assert_department_scope(actor_id, equipment.department_id.as_deref(), "equipment")
            .await?;

        if let Some(code) = payload.code {
            equipment.code = require_non_empty(&code, "code")?;
        }
        if payload.equipment_type_id.is_some() {
            equipment.equipment_type_id = normalize_optional_string(payload.equipment_type_id);
        }
        if let Some(department_id) = payload.department_id {
            // 换科室：新科室须存在，且操作者须同时是新科室经理或 admin。
            let department_id = self.require_existing_department(&department_id).await?;
            self.assert_department_scope(actor_id, Some(&department_id), "equipment")
                .await?;
            equipment.department_id = Some(department_id);
        }
        if payload.name.is_some() {
            equipment.name = normalize_optional_string(payload.name);
        }
        if payload.license_plate.is_some() {
            equipment.license_plate = normalize_optional_string(payload.license_plate);
        }
        if let Some(status) = payload.status {
            equipment.status = parse_equipment_status(&status)?;
        }
        if payload.next_maintenance_date.is_some() {
            equipment.next_maintenance_date = payload.next_maintenance_date;
        }
        if let Some(is_active) = payload.is_active {
            equipment.is_active = is_active;
        }

        self.equipment_repo.save(&equipment).await
    }

    pub async fn update_equipment_position(
        &self,
        equipment_id: &str,
        payload: PositionUpdate,
    ) -> Result<(), DomainError> {
        validate_position(payload.lat, payload.lng)?;
        let updated = self
            .equipment_repo
            .update_position(
                equipment_id,
                payload.lat,
                payload.lng,
                normalize_optional_ref(payload.stand_id.as_deref()),
            )
            .await?;
        if !updated {
            return Err(not_found("equipment", equipment_id));
        }
        Ok(())
    }

    pub async fn update_equipment_status(&self, equipment_id: &str, status: &str) -> Result<(), DomainError> {
        let normalized = equipment_status_value(parse_equipment_status(status)?);
        let updated = self.equipment_repo.update_status(equipment_id, normalized).await?;
        if !updated {
            return Err(not_found("equipment", equipment_id));
        }
        Ok(())
    }

    pub async fn list_stands(
        &self,
        terminal: Option<&str>,
        include_inactive: bool,
        page: i64,
        page_size: i64,
    ) -> Result<Vec<Stand>, DomainError> {
        self.stand_repo
            .find_all(
                normalize_optional_ref(terminal),
                include_inactive,
                normalize_page_size(page_size),
                offset(page, page_size),
            )
            .await
    }

    pub async fn get_stand(&self, stand_id: &str) -> Result<Option<Stand>, DomainError> {
        self.stand_repo.find_by_id(stand_id).await
    }

    pub async fn create_stand(&self, payload: StandCreate) -> Result<Stand, DomainError> {
        validate_position(payload.position_lat, payload.position_lng)?;
        let stand = Stand {
            id: ulid::Ulid::new().to_string(),
            code: require_non_empty(&payload.code, "code")?,
            name: normalize_optional_string(payload.name),
            terminal: normalize_optional_string(payload.terminal),
            area: normalize_optional_string(payload.area),
            position_lat: payload.position_lat,
            position_lng: payload.position_lng,
            stand_type: normalize_optional_string(payload.stand_type),
            size_category: normalize_optional_string(payload.size_category),
            is_active: true,
            created_at: None,
        };

        self.stand_repo.save(&stand).await
    }

    pub async fn list_task_types(
        &self,
        category: Option<&str>,
        page: i64,
        page_size: i64,
    ) -> Result<Vec<TaskType>, DomainError> {
        self.task_type_repo
            .find_all(
                normalize_optional_ref(category),
                normalize_page_size(page_size),
                offset(page, page_size),
            )
            .await
    }

    pub async fn create_task_type(&self, payload: TaskTypeCreate) -> Result<TaskType, DomainError> {
        let task_type = TaskType {
            id: ulid::Ulid::new().to_string(),
            code: require_non_empty(&payload.code, "code")?,
            name: require_non_empty(&payload.name, "name")?,
            default_department_id: normalize_optional_string(payload.default_department_id),
            category: normalize_optional_string(payload.category),
            sequence_order: payload.sequence_order,
            default_duration_minutes: payload.default_duration_minutes,
            trigger_offset_minutes: payload.trigger_offset_minutes,
            trigger_type: payload.trigger_type,
            description: normalize_optional_string(payload.description),
            is_active: true,
            created_at: None,
        };

        self.task_type_repo.save(&task_type).await
    }

    pub async fn delete_task_type(&self, task_type_id: &str) -> Result<(), DomainError> {
        let mut task_type = self
            .task_type_repo
            .find_by_id(task_type_id)
            .await?
            .or(self.task_type_repo.find_by_code(task_type_id).await?)
            .ok_or_else(|| not_found("task_type", task_type_id))?;
        task_type.is_active = false;
        self.task_type_repo.save(&task_type).await?;
        Ok(())
    }

    /// `Personnel.update_status`：更新个人在岗 runtime。本人可直接更新（本人改自己在岗
    /// 不要求经理）；改别人必须是该人科室经理或 admin（科室边界在领域层再验）。
    pub async fn update_personnel_status(
        &self,
        user_id: &str,
        status: &str,
        actor_id: &str,
    ) -> Result<PersonnelRuntime, DomainError> {
        let parsed = parse_personnel_status(status)?;
        self.assert_self_or_department_manager(actor_id, user_id).await?;
        let mut runtime = self.get_runtime_or_default(user_id).await?;
        runtime.current_status = parsed;
        runtime.updated_by = Some(actor_id.to_string());
        // 无行时建行（无行视为 off_duty），有行则整体 upsert。
        self.personnel_runtime_repo.save(&runtime).await
    }

    /// `Personnel.change_location`：更新个人位置。边界同上：本人可直接改，改别人须经理/admin。
    pub async fn update_personnel_position(
        &self,
        user_id: &str,
        lat: f64,
        lng: f64,
        stand_id: Option<&str>,
        actor_id: &str,
    ) -> Result<PersonnelRuntime, DomainError> {
        validate_position(lat, lng)?;
        self.assert_self_or_department_manager(actor_id, user_id).await?;
        let mut runtime = self.get_runtime_or_default(user_id).await?;
        runtime.current_position_lat = Some(lat);
        runtime.current_position_lng = Some(lng);
        runtime.current_stand_id = normalize_optional_string(stand_id.map(str::to_string));
        runtime.last_position_update = Some(chrono::Utc::now());
        runtime.updated_by = Some(actor_id.to_string());
        self.personnel_runtime_repo.save(&runtime).await
    }

    /// `Personnel.assign_to_team`（入组）：必须是个人账号（岗位账号 409）、
    /// 科室等于班组科室（teams.department_id，PR2 起不再经 team_type 间接推导）、
    /// 一人一条活跃 `team_members`。边界：该人科室经理或 admin。
    pub async fn assign_person_to_team(
        &self,
        person_user_id: &str,
        team_id: &str,
        actor_id: &str,
    ) -> Result<(), DomainError> {
        self.assert_department_manager(actor_id, person_user_id).await?;
        let team = self
            .team_repo
            .find_by_id(team_id, false)
            .await?
            .ok_or_else(|| not_found("team", team_id))?;
        self.validate_team_membership_rules(&team, person_user_id).await?;
        let member = TeamMember {
            id: ulid::Ulid::new().to_string(),
            team_id: team_id.to_string(),
            user_id: person_user_id.to_string(),
            role: MemberRole::Member,
            can_drive: false,
            joined_at: None,
            left_at: None,
            is_active: true,
            username: None,
            user_display_name: None,
        };
        self.team_member_repo.save(&member).await?;
        Ok(())
    }

    /// `Personnel.leave_team`（出组）：从班组移除个人。边界：该人科室经理或 admin。
    pub async fn remove_person_from_team(
        &self,
        person_user_id: &str,
        team_id: &str,
        actor_id: &str,
    ) -> Result<(), DomainError> {
        self.assert_department_manager(actor_id, person_user_id).await?;
        let removed = self
            .team_member_repo
            .remove_from_team(team_id, person_user_id)
            .await?;
        if !removed {
            return Err(not_found("team_member", person_user_id));
        }
        Ok(())
    }

    async fn get_runtime_or_default(&self, user_id: &str) -> Result<PersonnelRuntime, DomainError> {
        Ok(self.personnel_runtime_repo.find_by_user(user_id).await?.unwrap_or(
            PersonnelRuntime {
                user_id: user_id.to_string(),
                current_status: PersonnelStatus::OffDuty,
                current_stand_id: None,
                current_position_lat: None,
                current_position_lng: None,
                last_position_update: None,
                updated_at: None,
                updated_by: None,
            },
        ))
    }

    /// 本人或目标用户科室经理 / admin 才能改目标在岗（runtime）。admin 旁路。
    async fn assert_self_or_department_manager(&self, actor_id: &str, target_user_id: &str) -> Result<(), DomainError> {
        if actor_id == target_user_id {
            return Ok(());
        }
        self.assert_department_manager(actor_id, target_user_id).await
    }

    /// 科室须存在（班组/设备创建、换科室时校验引用完整性；120 后迁移不加 FK）。
    async fn require_existing_department(&self, department_id: &str) -> Result<String, DomainError> {
        let department_id = require_non_empty(department_id, "department_id")?;
        if self.department_repo.find_by_id(&department_id).await?.is_none() {
            return Err(DomainError::ValidationError(format!(
                "department {department_id} does not exist"
            )));
        }
        Ok(department_id)
    }

    /// 科室边界（计划 :182）：该科室 `manager_id` 或系统管理员可改本科室班组/设备/名册。
    /// admin 旁路；`department_id` 为空（历史遗留行）时仅 admin 可改；
    /// 全局 `team:manage` 不足以改别的科室（403）。
    async fn assert_department_scope(
        &self,
        actor_id: &str,
        department_id: Option<&str>,
        resource: &str,
    ) -> Result<(), DomainError> {
        let actor = self
            .user_repo
            .find_by_id(actor_id)
            .await?
            .ok_or_else(|| not_found("actor", actor_id))?;
        if actor.is_admin {
            return Ok(());
        }
        let Some(department_id) = department_id else {
            return Err(DomainError::PermissionDenied(format!(
                "only a system admin may modify {resource} without a department"
            )));
        };
        if let Some(dept) = self.department_repo.find_by_id(department_id).await? {
            if dept.manager_id.as_deref() == Some(actor_id) {
                return Ok(());
            }
        }
        Err(DomainError::PermissionDenied(format!(
            "only the department manager or a system admin may modify {resource} of department {department_id}"
        )))
    }

    /// 入组规则（计划 :191）：必须是个人账号（岗位账号 409）、科室等于班组科室
    /// （否则 409）、一人一条活跃 `team_members`（否则 409）。
    async fn validate_team_membership_rules(&self, team: &Team, user_id: &str) -> Result<(), DomainError> {
        let person = self
            .user_repo
            .find_by_id(user_id)
            .await?
            .ok_or_else(|| not_found("person", user_id))?;
        if person.is_position() {
            return Err(DomainError::Conflict(format!(
                "position account {user_id} cannot join a team"
            )));
        }
        if person.department_id != team.department_id {
            return Err(DomainError::Conflict(format!(
                "person {user_id} department does not match team {} department",
                team.id
            )));
        }
        // 一人一条活跃 team_members：已有活跃入组则冲突。
        let existing = self
            .team_member_repo
            .find_by_user(user_id)
            .await?
            .into_iter()
            .filter(|m| m.is_active)
            .count();
        if existing > 0 {
            return Err(DomainError::Conflict(format!(
                "person {user_id} already has an active team membership"
            )));
        }
        Ok(())
    }

    /// 目标用户科室经理或 admin 才能改目标。admin 旁路；非经理（含同科室普通成员）403。
    async fn assert_department_manager(&self, actor_id: &str, target_user_id: &str) -> Result<(), DomainError> {
        let actor = self
            .user_repo
            .find_by_id(actor_id)
            .await?
            .ok_or_else(|| not_found("actor", actor_id))?;
        if actor.is_admin {
            return Ok(());
        }
        let target = self
            .user_repo
            .find_by_id(target_user_id)
            .await?
            .ok_or_else(|| not_found("person", target_user_id))?;
        if let (Some(actor_dept), Some(target_dept)) = (&actor.department_id, &target.department_id) {
            if actor_dept == target_dept {
                if let Some(dept) = self.department_repo.find_by_id(actor_dept).await? {
                    if dept.manager_id.as_deref() == Some(actor_id) {
                        return Ok(());
                    }
                }
            }
        }
        Err(DomainError::PermissionDenied(
            "only the department manager or a system admin may modify another person".to_string(),
        ))
    }
}

fn not_found(entity_type: &'static str, id: &str) -> DomainError {
    DomainError::NotFound {
        entity_type,
        id: id.to_string(),
    }
}

fn offset(page: i64, page_size: i64) -> i64 {
    let safe_page = page.max(1);
    let safe_size = normalize_page_size(page_size);
    (safe_page - 1) * safe_size
}

fn normalize_page_size(page_size: i64) -> i64 {
    page_size.clamp(1, 500)
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|item| {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_optional_ref<'a>(value: Option<&'a str>) -> Option<&'a str> {
    value.and_then(|item| if item.trim().is_empty() { None } else { Some(item) })
}

fn normalize_string_list(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .filter_map(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .collect()
}

fn require_non_empty(value: &str, field: &str) -> Result<String, DomainError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(DomainError::ValidationError(format!("{field} is required")));
    }
    Ok(trimmed.to_string())
}

fn validate_position(lat: f64, lng: f64) -> Result<(), DomainError> {
    if !(-90.0..=90.0).contains(&lat) {
        return Err(DomainError::ValidationError("lat must be between -90 and 90".into()));
    }
    if !(-180.0..=180.0).contains(&lng) {
        return Err(DomainError::ValidationError("lng must be between -180 and 180".into()));
    }
    Ok(())
}

fn parse_team_status(value: &str) -> Result<TeamStatus, DomainError> {
    match value.trim() {
        "on_duty" => Ok(TeamStatus::OnDuty),
        "off_duty" => Ok(TeamStatus::OffDuty),
        "break" => Ok(TeamStatus::Break),
        _ => Err(DomainError::ValidationError(
            "current_status must be one of: on_duty, off_duty, break".into(),
        )),
    }
}

fn parse_member_role(value: &str) -> Result<MemberRole, DomainError> {
    match value.trim() {
        "leader" => Ok(MemberRole::Leader),
        "member" => Ok(MemberRole::Member),
        "driver" => Ok(MemberRole::Driver),
        _ => Err(DomainError::ValidationError(
            "role must be one of: leader, member, driver".into(),
        )),
    }
}

fn parse_equipment_status(value: &str) -> Result<EquipmentStatus, DomainError> {
    match value.trim() {
        "available" => Ok(EquipmentStatus::Available),
        "in_use" => Ok(EquipmentStatus::InUse),
        "maintenance" => Ok(EquipmentStatus::Maintenance),
        "retired" => Ok(EquipmentStatus::Retired),
        _ => Err(DomainError::ValidationError(
            "status must be one of: available, in_use, maintenance, retired".into(),
        )),
    }
}
