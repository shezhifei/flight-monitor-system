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
use fms_domain::models::field_overlay::OntologyFieldType;
use fms_domain::ports::dispatch_repository::{
    DepartmentRepository, EquipmentRepository, EquipmentTypeRepository, PersonnelRuntimeRepository, StandRepository,
    TaskTypeRepository, TeamMemberRepository, TeamRepository, TeamTypeRepository,
};
use fms_domain::ports::user_repository::UserRepository;
use fms_domain::ports::field_overlay_repository::FieldOverlayRepository;
use fms_domain::ports::ontology_attribute_reference_repository::OntologyAttributeReferenceRepository;

use super::mappers::{equipment_status_value, parse_personnel_status};
use crate::services::attribute_validation::{collect_attribute_references, sync_attribute_references, validate_attributes};
use crate::services::personnel_runtime_writer::PersonnelRuntimeAttributeTransactionalWriter;
use crate::services::department_writer::DepartmentAttributeTransactionalWriter;
use crate::services::team_writer::TeamAttributeTransactionalWriter;
use crate::services::equipment_type_writer::EquipmentTypeAttributeTransactionalWriter;
use crate::services::equipment_writer::EquipmentAttributeTransactionalWriter;
use crate::services::task_type_writer::TaskTypeAttributeTransactionalWriter;
use crate::services::team_type_writer::TeamTypeAttributeTransactionalWriter;

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
    field_overlay_repo: Option<Arc<dyn FieldOverlayRepository + Send + Sync>>,
    reference_repo: Option<Arc<dyn OntologyAttributeReferenceRepository + Send + Sync>>,
    personnel_runtime_writer: Option<Arc<dyn PersonnelRuntimeAttributeTransactionalWriter>>,
    department_writer: Option<Arc<dyn DepartmentAttributeTransactionalWriter>>,
    team_writer: Option<Arc<dyn TeamAttributeTransactionalWriter>>,
    equipment_type_writer: Option<Arc<dyn EquipmentTypeAttributeTransactionalWriter>>,
    equipment_writer: Option<Arc<dyn EquipmentAttributeTransactionalWriter>>,
    task_type_writer: Option<Arc<dyn TaskTypeAttributeTransactionalWriter>>,
    team_type_writer: Option<Arc<dyn TeamTypeAttributeTransactionalWriter>>,
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
            field_overlay_repo: None,
            reference_repo: None,
            personnel_runtime_writer: None,
            department_writer: None,
            team_writer: None,
            equipment_type_writer: None,
            equipment_writer: None,
            task_type_writer: None,
            team_type_writer: None,
        }
    }

    pub fn with_field_overlay_repository(
        mut self,
        repo: Arc<dyn FieldOverlayRepository + Send + Sync>,
    ) -> Self {
        self.field_overlay_repo = Some(repo);
        self
    }

    pub fn with_reference_repository(
        mut self,
        repo: Arc<dyn OntologyAttributeReferenceRepository + Send + Sync>,
    ) -> Self {
        self.reference_repo = Some(repo);
        self
    }

    pub fn with_personnel_runtime_writer(
        mut self,
        writer: Arc<dyn PersonnelRuntimeAttributeTransactionalWriter>,
    ) -> Self {
        self.personnel_runtime_writer = Some(writer);
        self
    }

    pub fn with_department_writer(
        mut self,
        writer: Arc<dyn DepartmentAttributeTransactionalWriter>,
    ) -> Self {
        self.department_writer = Some(writer);
        self
    }

    pub fn with_team_writer(
        mut self,
        writer: Arc<dyn TeamAttributeTransactionalWriter>,
    ) -> Self {
        self.team_writer = Some(writer);
        self
    }

    pub fn with_equipment_type_writer(
        mut self,
        writer: Arc<dyn EquipmentTypeAttributeTransactionalWriter>,
    ) -> Self {
        self.equipment_type_writer = Some(writer);
        self
    }

    pub fn with_equipment_writer(
        mut self,
        writer: Arc<dyn EquipmentAttributeTransactionalWriter>,
    ) -> Self {
        self.equipment_writer = Some(writer);
        self
    }

    pub fn with_task_type_writer(
        mut self,
        writer: Arc<dyn TaskTypeAttributeTransactionalWriter>,
    ) -> Self {
        self.task_type_writer = Some(writer);
        self
    }

    pub fn with_team_type_writer(
        mut self,
        writer: Arc<dyn TeamTypeAttributeTransactionalWriter>,
    ) -> Self {
        self.team_type_writer = Some(writer);
        self
    }

    async fn sync_references(&self, object_name: &str, object_id: &str, attributes: &serde_json::Value) -> Result<(), DomainError> {
        sync_attribute_references(
            object_name,
            object_id,
            attributes,
            self.field_overlay_repo.as_ref(),
            self.reference_repo.as_ref(),
        )
        .await
    }

    async fn ensure_not_referenced(&self, object_name: &str, object_id: &str) -> Result<(), DomainError> {
        let Some(reference_repo) = self.reference_repo.as_ref() else {
            return Ok(());
        };
        let refs = reference_repo.find_by_target(object_name, object_id).await?;
        if refs.is_empty() {
            return Ok(());
        }
        let details = refs
            .iter()
            .map(|reference| format!("{}:{}", reference.owner_object_name, reference.field_name))
            .collect::<Vec<_>>()
            .join(", ");
        Err(DomainError::Conflict(format!(
            "{object_name} {object_id} 仍被扩展字段引用，无法停用；引用: {details}"
        )))
    }

    async fn ensure_key_change_allowed(
        &self,
        object_name: &str,
        object_id: &str,
        previous_code: Option<&str>,
        next_code: Option<&str>,
    ) -> Result<(), DomainError> {
        let Some(previous_code) = previous_code.map(str::trim).filter(|value| !value.is_empty()) else {
            return Ok(());
        };
        if next_code.map(str::trim) == Some(previous_code) {
            return Ok(());
        }
        self.ensure_not_referenced(object_name, object_id).await?;
        if let Some(reference_repo) = self.reference_repo.as_ref() {
            let refs = reference_repo.find_by_target(object_name, previous_code).await?;
            if !refs.is_empty() {
                return Err(DomainError::Conflict(format!(
                    "{object_name} code {previous_code} 仍被扩展字段引用，无法改 code"
                )));
            }
        }
        Ok(())
    }

    async fn validate_object_references(&self, object_name: &str, attributes: &serde_json::Value) -> Result<(), DomainError> {
        let (Some(field_repo), Some(map)) = (self.field_overlay_repo.as_ref(), attributes.as_object()) else {
            return Ok(());
        };
        let overlays = field_repo.list(Some(object_name), false).await?;
        for field in overlays.iter().filter(|item| item.is_active) {
            let Some(field_type) = OntologyFieldType::parse(&field.field_type) else { continue; };
            if !field_type.is_object() { continue; }
            let Some(target) = field.object_name_target.as_deref() else { continue; };
            let Some(raw) = map.get(&field.field_name) else { continue; };
            let keys: Vec<&str> = match field_type {
                OntologyFieldType::ObjectRef => raw.as_str().into_iter().collect(),
                OntologyFieldType::ObjectRefArray => raw.as_array().map(|v| v.iter().filter_map(serde_json::Value::as_str).collect()).unwrap_or_default(),
                _ => Vec::new(),
            };
            for key in keys {
                let active = match target {
                    "Department" => self.department_repo.find_by_id(key).await?.map(|v| v.is_active),
                    "Team" => self.team_repo.find_by_id(key, false).await?.map(|v| v.is_active),
                    "TeamType" => self.team_type_repo.find_by_id(key).await?.map(|v| v.is_active),
                    "EquipmentType" => self.equipment_type_repo.find_by_id(key).await?.map(|v| v.is_active),
                    "Equipment" => self.equipment_repo.find_by_id(key).await?.map(|v| v.is_active),
                    "Stand" => self.stand_repo.find_by_id(key).await?.or(self.stand_repo.find_by_code(key).await?).map(|v| v.is_active),
                    "TaskType" => self.task_type_repo.find_by_id(key).await?.or(self.task_type_repo.find_by_code(key).await?).map(|v| v.is_active),
                    "Personnel" => self.user_repo.find_by_id(key).await?.map(|v| v.is_active),
                    _ => None,
                };
                if active != Some(true) {
                    return Err(DomainError::Conflict(format!("扩展字段 {object_name}.{} 引用了不存在或已停用的 {target}: {key}", field.field_name)));
                }
            }
        }
        Ok(())
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
        let attributes = validate_attributes("Department", payload.attributes, self.field_overlay_repo.as_ref()).await?;
        self.validate_object_references("Department", &attributes).await?;
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
            attributes,
        };

        if let Some(writer) = self.department_writer.as_ref() {
            let references = collect_attribute_references(
                "Department",
                &department.id,
                &department.attributes,
                self.field_overlay_repo.as_ref(),
            )
            .await?;
            return writer.save_with_references(&department, &references).await;
        }
        let saved = self.department_repo.save(&department).await?;
        self.sync_references("Department", &saved.id, &saved.attributes).await?;
        Ok(saved)
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
            self.ensure_key_change_allowed("Department", department_id, department.code.as_deref(), payload.code.as_deref()).await?;
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
            if !is_active && department.is_active {
                self.ensure_not_referenced("Department", department_id).await?;
            }
            department.is_active = is_active;
        }
        if let Some(attributes) = payload.attributes {
            department.attributes = validate_attributes("Department", attributes, self.field_overlay_repo.as_ref()).await?;
            self.validate_object_references("Department", &department.attributes).await?;
        }

        if let Some(writer) = self.department_writer.as_ref() {
            let references = collect_attribute_references(
                "Department",
                &department.id,
                &department.attributes,
                self.field_overlay_repo.as_ref(),
            )
            .await?;
            return writer.save_with_references(&department, &references).await;
        }
        let saved = self.department_repo.save(&department).await?;
        self.sync_references("Department", &saved.id, &saved.attributes).await?;
        Ok(saved)
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
        let attributes = validate_attributes("TeamType", payload.attributes, self.field_overlay_repo.as_ref()).await?;
        self.validate_object_references("TeamType", &attributes).await?;
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
            attributes,
        };

        if let Some(writer) = self.team_type_writer.as_ref() {
            let references = collect_attribute_references(
                "TeamType",
                &team_type.id,
                &team_type.attributes,
                self.field_overlay_repo.as_ref(),
            )
            .await?;
            return writer.save_with_references(&team_type, &references).await;
        }
        let saved = self.team_type_repo.save(&team_type).await?;
        self.sync_references("TeamType", &saved.id, &saved.attributes).await?;
        Ok(saved)
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
            self.ensure_key_change_allowed("TeamType", team_type_id, team_type.code.as_deref(), payload.code.as_deref()).await?;
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
        if let Some(attributes) = payload.attributes {
            team_type.attributes = validate_attributes("TeamType", attributes, self.field_overlay_repo.as_ref()).await?;
            self.validate_object_references("TeamType", &team_type.attributes).await?;
        }

        if let Some(writer) = self.team_type_writer.as_ref() {
            let references = collect_attribute_references(
                "TeamType",
                &team_type.id,
                &team_type.attributes,
                self.field_overlay_repo.as_ref(),
            )
            .await?;
            return writer.save_with_references(&team_type, &references).await;
        }
        let saved = self.team_type_repo.save(&team_type).await?;
        self.sync_references("TeamType", &saved.id, &saved.attributes).await?;
        Ok(saved)
    }

    pub async fn delete_team_type(&self, team_type_id: &str) -> Result<(), DomainError> {
        self.ensure_not_referenced("TeamType", team_type_id).await?;
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
        let attributes = validate_attributes("Team", payload.attributes, self.field_overlay_repo.as_ref()).await?;
        self.validate_object_references("Team", &attributes).await?;
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
            attributes,
        };

        if let Some(writer) = self.team_writer.as_ref() {
            let references = collect_attribute_references(
                "Team",
                &team.id,
                &team.attributes,
                self.field_overlay_repo.as_ref(),
            )
            .await?;
            return writer.save_with_references(&team, &references).await;
        }
        let saved = self.team_repo.save(&team).await?;
        self.sync_references("Team", &saved.id, &saved.attributes).await?;
        Ok(saved)
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
            self.ensure_key_change_allowed("Team", team_id, team.code.as_deref(), payload.code.as_deref()).await?;
            team.code = normalize_optional_string(payload.code);
        }
        if payload.leader_id.is_some() {
            team.leader_id = normalize_optional_string(payload.leader_id);
        }
        if let Some(status) = payload.current_status {
            team.current_status = parse_team_status(&status)?;
        }
        if let Some(is_active) = payload.is_active {
            if !is_active && team.is_active {
                self.ensure_not_referenced("Team", team_id).await?;
            }
            team.is_active = is_active;
        }
        if let Some(attributes) = payload.attributes {
            team.attributes = validate_attributes("Team", attributes, self.field_overlay_repo.as_ref()).await?;
            self.validate_object_references("Team", &team.attributes).await?;
        }

        if let Some(writer) = self.team_writer.as_ref() {
            let references = collect_attribute_references(
                "Team",
                &team.id,
                &team.attributes,
                self.field_overlay_repo.as_ref(),
            )
            .await?;
            return writer.save_with_references(&team, &references).await;
        }
        let saved = self.team_repo.save(&team).await?;
        self.sync_references("Team", &saved.id, &saved.attributes).await?;
        Ok(saved)
    }

    pub async fn delete_team(&self, team_id: &str, actor_id: &str) -> Result<(), DomainError> {
        let mut team = self
            .team_repo
            .find_by_id(team_id, true)
            .await?
            .ok_or_else(|| not_found("team", team_id))?;
        self.assert_department_scope(actor_id, team.department_id.as_deref(), "teams")
            .await?;
        self.ensure_not_referenced("Team", team_id).await?;
        team.is_active = false;
        if let Some(writer) = self.team_writer.as_ref() {
            let references = collect_attribute_references(
                "Team",
                &team.id,
                &team.attributes,
                self.field_overlay_repo.as_ref(),
            )
            .await?;
            writer.save_with_references(&team, &references).await?;
            return Ok(());
        }
        let saved = self.team_repo.save(&team).await?;
        self.sync_references("Team", &saved.id, &saved.attributes).await?;
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
        let attributes = validate_attributes("EquipmentType", payload.attributes, self.field_overlay_repo.as_ref()).await?;
        self.validate_object_references("EquipmentType", &attributes).await?;
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
            attributes,
        };

        if let Some(writer) = self.equipment_type_writer.as_ref() {
            let references = collect_attribute_references(
                "EquipmentType",
                &equipment_type.id,
                &equipment_type.attributes,
                self.field_overlay_repo.as_ref(),
            )
            .await?;
            return writer.save_with_references(&equipment_type, &references).await;
        }
        let saved = self.equipment_type_repo.save(&equipment_type).await?;
        self.sync_references("EquipmentType", &saved.id, &saved.attributes).await?;
        Ok(saved)
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
            self.ensure_key_change_allowed("EquipmentType", equipment_type_id, equipment_type.code.as_deref(), payload.code.as_deref()).await?;
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
        if let Some(attributes) = payload.attributes {
            equipment_type.attributes = validate_attributes("EquipmentType", attributes, self.field_overlay_repo.as_ref()).await?;
            self.validate_object_references("EquipmentType", &equipment_type.attributes).await?;
        }

        if let Some(writer) = self.equipment_type_writer.as_ref() {
            let references = collect_attribute_references(
                "EquipmentType",
                &equipment_type.id,
                &equipment_type.attributes,
                self.field_overlay_repo.as_ref(),
            )
            .await?;
            return writer.save_with_references(&equipment_type, &references).await;
        }
        let saved = self.equipment_type_repo.save(&equipment_type).await?;
        self.sync_references("EquipmentType", &saved.id, &saved.attributes).await?;
        Ok(saved)
    }

    pub async fn delete_equipment_type(&self, equipment_type_id: &str) -> Result<(), DomainError> {
        self.ensure_not_referenced("EquipmentType", equipment_type_id).await?;
        if let Some(writer) = self.equipment_type_writer.as_ref() {
            let mut equipment_type = self
                .equipment_type_repo
                .find_by_id(equipment_type_id)
                .await?
                .ok_or_else(|| not_found("equipment_type", equipment_type_id))?;
            equipment_type.is_active = false;
            let references = collect_attribute_references(
                "EquipmentType",
                &equipment_type.id,
                &equipment_type.attributes,
                self.field_overlay_repo.as_ref(),
            )
            .await?;
            writer.save_with_references(&equipment_type, &references).await?;
            return Ok(());
        }
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
        let attributes = validate_attributes("Equipment", payload.attributes, self.field_overlay_repo.as_ref()).await?;
        self.validate_object_references("Equipment", &attributes).await?;
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
            attributes,
        };

        if let Some(writer) = self.equipment_writer.as_ref() {
            let references = collect_attribute_references(
                "Equipment",
                &equipment.id,
                &equipment.attributes,
                self.field_overlay_repo.as_ref(),
            )
            .await?;
            return writer.save_with_references(&equipment, &references).await;
        }
        let saved = self.equipment_repo.save(&equipment).await?;
        self.sync_references("Equipment", &saved.id, &saved.attributes).await?;
        Ok(saved)
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
            self.ensure_key_change_allowed("Equipment", equipment_id, Some(&equipment.code), Some(&code)).await?;
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
            if !is_active && equipment.is_active {
                self.ensure_not_referenced("Equipment", equipment_id).await?;
            }
            equipment.is_active = is_active;
        }
        if let Some(attributes) = payload.attributes {
            equipment.attributes = validate_attributes("Equipment", attributes, self.field_overlay_repo.as_ref()).await?;
            self.validate_object_references("Equipment", &equipment.attributes).await?;
        }

        if let Some(writer) = self.equipment_writer.as_ref() {
            let references = collect_attribute_references(
                "Equipment",
                &equipment.id,
                &equipment.attributes,
                self.field_overlay_repo.as_ref(),
            )
            .await?;
            return writer.save_with_references(&equipment, &references).await;
        }
        let saved = self.equipment_repo.save(&equipment).await?;
        self.sync_references("Equipment", &saved.id, &saved.attributes).await?;
        Ok(saved)
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
        let attributes = validate_attributes("Stand", payload.attributes, self.field_overlay_repo.as_ref()).await?;
        let code = require_non_empty(&payload.code, "code")?;
        self.validate_stand_composition(&code, &attributes).await?;
        let stand = Stand {
            id: ulid::Ulid::new().to_string(),
            code,
            name: normalize_optional_string(payload.name),
            terminal: normalize_optional_string(payload.terminal),
            area: normalize_optional_string(payload.area),
            position_lat: payload.position_lat,
            position_lng: payload.position_lng,
            stand_type: normalize_optional_string(payload.stand_type),
            size_category: normalize_optional_string(payload.size_category),
            attributes,
            is_active: true,
            created_at: None,
        };

        self.stand_repo.save(&stand).await
    }

    async fn validate_stand_composition(&self, stand_code: &str, attributes: &serde_json::Value) -> Result<(), DomainError> {
        let composed_of = crate::services::stand_composition::composed_of_codes(attributes);
        if composed_of.is_empty() {
            return Ok(());
        }
        // 全量机位快照（含停用）交给共用纯函数：自引用 / 重复 / 子不存在或停用 /
        // 成环 / 双父统一 409。与 TerminalResourceService 走同一份不变量。
        let stands = self.stand_repo.find_all(None, true, 500, 0).await?;
        crate::services::stand_composition::validate_stand_composition(stand_code, &composed_of, &stands)
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
        let category = normalize_optional_string(payload.category);
        let anchor = normalize_task_type_anchor(payload.anchor.as_deref(), category.as_deref())?;
        let attributes = validate_attributes("TaskType", payload.attributes, self.field_overlay_repo.as_ref()).await?;
        self.validate_object_references("TaskType", &attributes).await?;
        let task_type = TaskType {
            id: ulid::Ulid::new().to_string(),
            code: require_non_empty(&payload.code, "code")?,
            name: require_non_empty(&payload.name, "name")?,
            default_department_id: normalize_optional_string(payload.default_department_id),
            category,
            anchor,
            sequence_order: payload.sequence_order,
            default_duration_minutes: payload.default_duration_minutes,
            trigger_offset_minutes: payload.trigger_offset_minutes,
            trigger_type: payload.trigger_type,
            description: normalize_optional_string(payload.description),
            attributes,
            is_active: true,
            created_at: None,
        };

        if let Some(writer) = self.task_type_writer.as_ref() {
            let references = collect_attribute_references(
                "TaskType",
                &task_type.id,
                &task_type.attributes,
                self.field_overlay_repo.as_ref(),
            )
            .await?;
            return writer.save_with_references(&task_type, &references).await;
        }
        let saved = self.task_type_repo.save(&task_type).await?;
        self.sync_references("TaskType", &saved.id, &saved.attributes).await?;
        Ok(saved)
    }

    pub async fn delete_task_type(&self, task_type_id: &str) -> Result<(), DomainError> {
        let mut task_type = self
            .task_type_repo
            .find_by_id(task_type_id)
            .await?
            .or(self.task_type_repo.find_by_code(task_type_id).await?)
            .ok_or_else(|| not_found("task_type", task_type_id))?;
        self.ensure_not_referenced("TaskType", &task_type.id).await?;
        task_type.is_active = false;
        if let Some(writer) = self.task_type_writer.as_ref() {
            let references = collect_attribute_references(
                "TaskType",
                &task_type.id,
                &task_type.attributes,
                self.field_overlay_repo.as_ref(),
            )
            .await?;
            writer.save_with_references(&task_type, &references).await?;
            return Ok(());
        }
        let saved = self.task_type_repo.save(&task_type).await?;
        self.sync_references("TaskType", &saved.id, &saved.attributes).await?;
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
        self.ensure_personal_user(user_id).await?;
        let mut runtime = self.get_runtime_or_default(user_id).await?;
        runtime.current_status = parsed;
        runtime.updated_by = Some(actor_id.to_string());
        // 无行时建行（无行视为 off_duty），有行则整体 upsert。
        if let Some(writer) = self.personnel_runtime_writer.as_ref() {
            let references = collect_attribute_references(
                "Personnel",
                &runtime.user_id,
                &runtime.attributes,
                self.field_overlay_repo.as_ref(),
            )
            .await?;
            writer.save_with_references(&runtime, &references).await?;
            return Ok(runtime);
        }
        let saved = self.personnel_runtime_repo.save(&runtime).await?;
        self.sync_references("Personnel", &saved.user_id, &saved.attributes).await?;
        Ok(saved)
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
        self.ensure_personal_user(user_id).await?;
        let mut runtime = self.get_runtime_or_default(user_id).await?;
        runtime.current_position_lat = Some(lat);
        runtime.current_position_lng = Some(lng);
        runtime.current_stand_id = normalize_optional_string(stand_id.map(str::to_string));
        runtime.last_position_update = Some(chrono::Utc::now());
        runtime.updated_by = Some(actor_id.to_string());
        if let Some(writer) = self.personnel_runtime_writer.as_ref() {
            let references = collect_attribute_references(
                "Personnel",
                &runtime.user_id,
                &runtime.attributes,
                self.field_overlay_repo.as_ref(),
            )
            .await?;
            writer.save_with_references(&runtime, &references).await?;
            return Ok(runtime);
        }
        let saved = self.personnel_runtime_repo.save(&runtime).await?;
        self.sync_references("Personnel", &saved.user_id, &saved.attributes).await?;
        Ok(saved)
    }

    /// Read the personnel runtime projection. A missing row is represented as
    /// an explicit off-duty runtime so the form can safely edit attributes
    /// without special-casing absent rows.
    pub async fn get_personnel_runtime(&self, user_id: &str) -> Result<PersonnelRuntime, DomainError> {
        self.ensure_personal_user(user_id).await?;
        self.get_runtime_or_default(user_id).await
    }

    /// Update only extensible Personnel attributes. Core runtime fields remain
    /// behind their dedicated status/location actions and permission checks.
    pub async fn update_personnel_attributes(
        &self,
        user_id: &str,
        attributes: serde_json::Value,
        actor_id: &str,
    ) -> Result<PersonnelRuntime, DomainError> {
        self.assert_self_or_department_manager(actor_id, user_id).await?;
        self.ensure_personal_user(user_id).await?;
        let attributes = validate_attributes("Personnel", attributes, self.field_overlay_repo.as_ref()).await?;
        self.validate_object_references("Personnel", &attributes).await?;
        let mut runtime = self.get_runtime_or_default(user_id).await?;
        runtime.attributes = attributes;
        runtime.updated_by = Some(actor_id.to_string());
        if let Some(writer) = self.personnel_runtime_writer.as_ref() {
            let references = collect_attribute_references(
                "Personnel",
                &runtime.user_id,
                &runtime.attributes,
                self.field_overlay_repo.as_ref(),
            )
            .await?;
            writer.save_with_references(&runtime, &references).await?;
            return Ok(runtime);
        }
        let saved = self.personnel_runtime_repo.save(&runtime).await?;
        self.sync_references("Personnel", &saved.user_id, &saved.attributes).await?;
        Ok(saved)
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
        let removed = self.team_member_repo.remove_from_team(team_id, person_user_id).await?;
        if !removed {
            return Err(not_found("team_member", person_user_id));
        }
        Ok(())
    }

    async fn get_runtime_or_default(&self, user_id: &str) -> Result<PersonnelRuntime, DomainError> {
        Ok(self
            .personnel_runtime_repo
            .find_by_user(user_id)
            .await?
            .unwrap_or(PersonnelRuntime {
                user_id: user_id.to_string(),
                current_status: PersonnelStatus::OffDuty,
                current_stand_id: None,
                current_position_lat: None,
                current_position_lng: None,
                last_position_update: None,
                updated_at: None,
                updated_by: None,
                attributes: serde_json::json!({}),
            }))
    }

    async fn ensure_personal_user(&self, user_id: &str) -> Result<(), DomainError> {
        let user = self
            .user_repo
            .find_by_id(user_id)
            .await?
            .ok_or_else(|| not_found("person", user_id))?;
        if user.is_position() {
            return Err(DomainError::Conflict(format!(
                "position account {user_id} has no Personnel runtime"
            )));
        }
        Ok(())
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

fn normalize_task_type_anchor(anchor: Option<&str>, category: Option<&str>) -> Result<String, DomainError> {
    let value = anchor
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
        .unwrap_or_else(
            || match category.map(|value| value.trim().to_ascii_lowercase()).as_deref() {
                Some("arrival") => "inbound".to_string(),
                Some("departure") => "outbound".to_string(),
                _ => "link".to_string(),
            },
        );
    match value.as_str() {
        "inbound" | "outbound" | "link" => Ok(value),
        _ => Err(DomainError::ValidationError(
            "anchor 仅支持 inbound、outbound 或 link".to_string(),
        )),
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

#[cfg(test)]
mod task_type_anchor_tests {
    use super::normalize_task_type_anchor;

    #[test]
    fn anchor_defaults_from_category() {
        assert_eq!(normalize_task_type_anchor(None, Some("arrival")).unwrap(), "inbound");
        assert_eq!(normalize_task_type_anchor(None, Some("departure")).unwrap(), "outbound");
        assert_eq!(normalize_task_type_anchor(None, Some("turnaround")).unwrap(), "link");
    }

    #[test]
    fn anchor_is_normalized_and_rejects_unknown_values() {
        assert_eq!(
            normalize_task_type_anchor(Some(" OUTBOUND "), None).unwrap(),
            "outbound"
        );
        assert!(normalize_task_type_anchor(Some("both"), None).is_err());
    }
}
