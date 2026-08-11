//! 派工排班服务。

use chrono::{DateTime, Utc};
use std::sync::Arc;

use crate::schemas::dispatch_schemas::{
    ScheduleAvailabilityResponse, ScheduleExceptionCreate, ScheduleExceptionResponse, ShiftInstanceCreate,
    ShiftTemplateCreate,
};
use crate::services::resource_availability_service::{NullAvailabilityGateway, ResourceAvailabilityGateway};
use fms_domain::error::DomainError;
use fms_domain::models::dispatch::{
    DispatchLockLevel, DispatchLockRule, EquipmentDowntime, LeaveRecord, ShiftInstance, ShiftTemplate,
};
use fms_domain::ports::dispatch_repository::{
    EquipmentRepository, ScheduleExceptionRepository, ShiftInstanceRepository, ShiftTemplateRepository,
    TeamMemberRepository, TeamRepository,
};

pub struct DispatchScheduleService<
    STR: ShiftTemplateRepository = fms_domain::ports::NullRepository,
    SIR: ShiftInstanceRepository = fms_domain::ports::NullRepository,
    SER: ScheduleExceptionRepository = fms_domain::ports::NullRepository,
    TR: TeamRepository = fms_domain::ports::NullRepository,
    TMR: TeamMemberRepository = fms_domain::ports::NullRepository,
    ER: EquipmentRepository = fms_domain::ports::NullRepository,
    AG: ResourceAvailabilityGateway = NullAvailabilityGateway,
> {
    shift_template_repo: Arc<STR>,
    shift_instance_repo: Arc<SIR>,
    schedule_exception_repo: Arc<SER>,
    team_repo: Arc<TR>,
    team_member_repo: Arc<TMR>,
    equipment_repo: Arc<ER>,
    availability_service: Arc<AG>,
}

impl<
        STR: ShiftTemplateRepository,
        SIR: ShiftInstanceRepository,
        SER: ScheduleExceptionRepository,
        TR: TeamRepository,
        TMR: TeamMemberRepository,
        ER: EquipmentRepository,
        AG: ResourceAvailabilityGateway,
    > DispatchScheduleService<STR, SIR, SER, TR, TMR, ER, AG>
{
    pub fn new(
        shift_template_repo: Arc<STR>,
        shift_instance_repo: Arc<SIR>,
        schedule_exception_repo: Arc<SER>,
        team_repo: Arc<TR>,
        team_member_repo: Arc<TMR>,
        equipment_repo: Arc<ER>,
        availability_service: Arc<AG>,
    ) -> Self {
        Self {
            shift_template_repo,
            shift_instance_repo,
            schedule_exception_repo,
            team_repo,
            team_member_repo,
            equipment_repo,
            availability_service,
        }
    }

    pub async fn list_templates(
        &self,
        resource_type: Option<&str>,
        resource_id: Option<&str>,
        enabled: Option<bool>,
        limit: i64,
    ) -> Result<Vec<ShiftTemplate>, DomainError> {
        self.shift_template_repo
            .find_all(
                normalize_optional_ref(resource_type),
                normalize_optional_ref(resource_id),
                enabled,
                limit.max(1),
                0,
            )
            .await
    }

    pub async fn create_template(&self, payload: ShiftTemplateCreate) -> Result<ShiftTemplate, DomainError> {
        let template = ShiftTemplate {
            id: ulid::Ulid::new().to_string(),
            name: require_non_empty(&payload.name, "name")?,
            resource_type: parse_resource_type(&payload.resource_type)?.to_string(),
            resource_id: require_non_empty(&payload.resource_id, "resource_id")?,
            terminal: normalize_optional_string(payload.terminal),
            start_time_local: require_non_empty(&payload.start_time_local, "start_time_local")?,
            end_time_local: require_non_empty(&payload.end_time_local, "end_time_local")?,
            weekdays: payload.weekdays,
            max_continuous_minutes: payload.max_continuous_minutes,
            min_rest_minutes: payload.min_rest_minutes,
            enabled: payload.enabled,
            created_at: None,
            updated_at: None,
        };
        self.shift_template_repo.save(&template).await
    }

    pub async fn list_instances(
        &self,
        resource_type: Option<&str>,
        resource_id: Option<&str>,
        window_start: Option<DateTime<Utc>>,
        window_end: Option<DateTime<Utc>>,
        limit: i64,
    ) -> Result<Vec<ShiftInstance>, DomainError> {
        self.shift_instance_repo
            .find_all(
                normalize_optional_ref(resource_type),
                normalize_optional_ref(resource_id),
                window_start,
                window_end,
                limit.max(1),
                0,
            )
            .await
    }

    pub async fn create_instance(&self, payload: ShiftInstanceCreate) -> Result<ShiftInstance, DomainError> {
        let instance = ShiftInstance {
            id: ulid::Ulid::new().to_string(),
            template_id: normalize_optional_string(payload.template_id),
            resource_type: parse_resource_type(&payload.resource_type)?.to_string(),
            resource_id: require_non_empty(&payload.resource_id, "resource_id")?,
            terminal: normalize_optional_string(payload.terminal),
            start_time: payload.start_time,
            end_time: payload.end_time,
            status: normalize_optional_string(Some(payload.status)).unwrap_or_else(|| "scheduled".to_string()),
            max_continuous_minutes: payload.max_continuous_minutes,
            min_rest_minutes: payload.min_rest_minutes,
            created_at: None,
            updated_at: None,
        };
        self.shift_instance_repo.save(&instance).await
    }

    pub async fn list_exceptions(
        &self,
        window_start: Option<DateTime<Utc>>,
        window_end: Option<DateTime<Utc>>,
        limit: i64,
    ) -> Result<Vec<ScheduleExceptionResponse>, DomainError> {
        let items = self
            .schedule_exception_repo
            .list_exceptions(window_start, window_end, limit.max(1))
            .await?;
        items.into_iter().map(value_to_schedule_exception).collect()
    }

    pub async fn create_exception(
        &self,
        payload: ScheduleExceptionCreate,
    ) -> Result<ScheduleExceptionResponse, DomainError> {
        match payload.exception_type.trim() {
            "leave" => {
                let record = LeaveRecord {
                    id: ulid::Ulid::new().to_string(),
                    user_id: require_non_empty(payload.user_id.as_deref().unwrap_or_default(), "user_id")?,
                    team_id: normalize_optional_string(payload.team_id),
                    start_time: payload.start_time,
                    end_time: payload.end_time,
                    reason: normalize_optional_string(payload.reason),
                    status: normalize_optional_string(payload.status).unwrap_or_else(|| "approved".to_string()),
                    created_at: None,
                };
                let saved = self.schedule_exception_repo.save_leave_record(&record).await?;
                Ok(ScheduleExceptionResponse {
                    id: saved.id,
                    exception_type: "leave".to_string(),
                    resource_id: Some(saved.user_id),
                    team_id: saved.team_id,
                    dispatch_order_id: None,
                    start_time: saved.start_time,
                    end_time: saved.end_time,
                    status: saved.status,
                    reason: saved.reason,
                })
            }
            "equipment_downtime" => {
                let downtime = EquipmentDowntime {
                    id: ulid::Ulid::new().to_string(),
                    equipment_id: require_non_empty(
                        payload.equipment_id.as_deref().unwrap_or_default(),
                        "equipment_id",
                    )?,
                    start_time: payload.start_time,
                    end_time: payload.end_time,
                    reason: normalize_optional_string(payload.reason),
                    status: normalize_optional_string(payload.status).unwrap_or_else(|| "scheduled".to_string()),
                    created_at: None,
                };
                let saved = self.schedule_exception_repo.save_equipment_downtime(&downtime).await?;
                Ok(ScheduleExceptionResponse {
                    id: saved.id,
                    exception_type: "equipment_downtime".to_string(),
                    resource_id: Some(saved.equipment_id),
                    team_id: None,
                    dispatch_order_id: None,
                    start_time: saved.start_time,
                    end_time: saved.end_time,
                    status: saved.status,
                    reason: saved.reason,
                })
            }
            "dispatch_lock" => {
                let rule = DispatchLockRule {
                    id: ulid::Ulid::new().to_string(),
                    dispatch_order_id: normalize_optional_string(payload.dispatch_order_id),
                    flight_id: normalize_optional_string(payload.flight_id),
                    team_id: normalize_optional_string(payload.team_id),
                    lock_level: parse_lock_level(payload.lock_level.as_deref())?,
                    start_time: payload.start_time,
                    end_time: payload.end_time,
                    reason: normalize_optional_string(payload.reason),
                    created_at: None,
                };
                let saved = self.schedule_exception_repo.save_lock_rule(&rule).await?;
                Ok(ScheduleExceptionResponse {
                    id: saved.id,
                    exception_type: "dispatch_lock".to_string(),
                    resource_id: saved
                        .team_id
                        .clone()
                        .or(saved.flight_id.clone())
                        .or(saved.dispatch_order_id.clone()),
                    team_id: saved.team_id,
                    dispatch_order_id: saved.dispatch_order_id,
                    start_time: saved.start_time,
                    end_time: saved.end_time,
                    status: lock_level_value(saved.lock_level).to_string(),
                    reason: saved.reason,
                })
            }
            _ => Err(DomainError::ValidationError("不支持的异常类型".to_string())),
        }
    }

    pub async fn get_availability(
        &self,
        resource_type: &str,
        planned_start_time: DateTime<Utc>,
        planned_end_time: DateTime<Utc>,
        terminal: Option<&str>,
        resource_ids: &[String],
    ) -> Result<Vec<ScheduleAvailabilityResponse>, DomainError> {
        match parse_resource_type(resource_type)? {
            "team" => {
                let teams = self
                    .team_repo
                    .find_all(false, None, terminal, (resource_ids.len().max(1) as i64).max(200), 0)
                    .await?;
                let filtered = filter_by_ids(teams, resource_ids, |item| item.id.clone());
                let items = self
                    .availability_service
                    .list_team_availability(&filtered, planned_start_time, planned_end_time, terminal)
                    .await?;
                Ok(items.into_iter().map(map_availability).collect())
            }
            "equipment" => {
                let equipment = self
                    .equipment_repo
                    .find_all(
                        false,
                        None,
                        terminal,
                        None,
                        (resource_ids.len().max(1) as i64).max(200),
                        0,
                    )
                    .await?;
                let filtered = filter_by_ids(equipment, resource_ids, |item| item.id.clone());
                let mut results = Vec::with_capacity(filtered.len());
                for item in filtered {
                    results.push(map_availability(
                        self.availability_service
                            .evaluate_equipment(&item, planned_start_time, planned_end_time, terminal, None)
                            .await?,
                    ));
                }
                Ok(results)
            }
            "employee" => {
                let user_ids = if resource_ids.is_empty() {
                    self.team_member_repo.list_active_users().await?
                } else {
                    resource_ids.to_vec()
                };
                let items = self
                    .availability_service
                    .list_employee_availability(&user_ids, planned_start_time, planned_end_time, terminal)
                    .await?;
                Ok(items.into_iter().map(map_availability).collect())
            }
            _ => unreachable!(),
        }
    }
}

fn map_availability(
    item: crate::services::resource_availability_service::ResourceAvailability,
) -> ScheduleAvailabilityResponse {
    ScheduleAvailabilityResponse {
        resource_type: item.resource_type,
        resource_id: item.resource_id,
        available: item.available,
        schedule_source: schedule_source_value(item.schedule_source).to_string(),
        reason: item.reason,
        reasons: item.reasons,
        lock_level: lock_level_value(item.lock_level).to_string(),
        score_breakdown: item.score_breakdown,
        metadata: item.metadata,
    }
}

fn value_to_schedule_exception(value: serde_json::Value) -> Result<ScheduleExceptionResponse, DomainError> {
    serde_json::from_value(value).map_err(|err| DomainError::Internal(err.to_string()))
}

fn filter_by_ids<T, F>(items: Vec<T>, ids: &[String], key_fn: F) -> Vec<T>
where
    F: Fn(&T) -> String,
{
    if ids.is_empty() {
        return items;
    }
    items
        .into_iter()
        .filter(|item| ids.iter().any(|id| id == &key_fn(item)))
        .collect()
}

fn parse_resource_type<'a>(value: &'a str) -> Result<&'a str, DomainError> {
    match value.trim() {
        "team" | "equipment" | "employee" => Ok(value.trim()),
        other => Err(DomainError::ValidationError(format!("不支持的资源类型: {other}"))),
    }
}

fn parse_lock_level(value: Option<&str>) -> Result<DispatchLockLevel, DomainError> {
    match value.unwrap_or("manual_lock").trim() {
        "active" => Ok(DispatchLockLevel::Active),
        "frozen" => Ok(DispatchLockLevel::Frozen),
        "manual_lock" | "" => Ok(DispatchLockLevel::ManualLock),
        "optimizable" => Ok(DispatchLockLevel::Optimizable),
        other => Err(DomainError::ValidationError(format!("未知锁定级别: {other}"))),
    }
}

fn schedule_source_value(value: fms_domain::models::dispatch::ScheduleSource) -> &'static str {
    match value {
        fms_domain::models::dispatch::ScheduleSource::ShiftInstance => "shift_instance",
        fms_domain::models::dispatch::ScheduleSource::CurrentStatusFallback => "current_status_fallback",
    }
}

fn lock_level_value(value: DispatchLockLevel) -> &'static str {
    match value {
        DispatchLockLevel::Active => "active",
        DispatchLockLevel::Frozen => "frozen",
        DispatchLockLevel::ManualLock => "manual_lock",
        DispatchLockLevel::Optimizable => "optimizable",
    }
}

fn require_non_empty(value: &str, field: &str) -> Result<String, DomainError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(DomainError::ValidationError(format!("{field} 不能为空")));
    }
    Ok(trimmed.to_string())
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
