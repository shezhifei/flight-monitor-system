//! 资源可用性服务。

use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::{
    DispatchLockLevel, Equipment, EquipmentStatus, ScheduleSource, ShiftInstance, Team, TeamStatus,
};
use fms_domain::ports::dispatch_repository::{
    DispatchOrderMemberRepository, DispatchOrderRepository, ScheduleExceptionRepository, ShiftInstanceRepository,
    TeamMemberRepository, TeamRepository,
};

#[derive(Debug, Clone)]
pub struct ResourceAvailability {
    pub resource_type: String,
    pub resource_id: String,
    pub available: bool,
    pub schedule_source: ScheduleSource,
    pub reason: String,
    pub reasons: Vec<String>,
    pub lock_level: DispatchLockLevel,
    pub score_breakdown: HashMap<String, f64>,
    pub metadata: HashMap<String, Value>,
}

use std::pin::Pin;

pub trait ResourceAvailabilityGateway: Send + Sync {
    fn list_team_availability<'life0, 'life1>(
        &'life0 self,
        teams: &'life1 [Team],
        planned_start_time: DateTime<Utc>,
        planned_end_time: DateTime<Utc>,
        terminal: Option<&'life1 str>,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Vec<ResourceAvailability>, DomainError>> + Send + 'life0>>
    where
        'life1: 'life0;

    fn evaluate_equipment<'life0, 'life1>(
        &'life0 self,
        equipment: &'life1 Equipment,
        planned_start_time: DateTime<Utc>,
        planned_end_time: DateTime<Utc>,
        terminal: Option<&'life1 str>,
        exclude_order_id: Option<&'life1 str>,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<ResourceAvailability, DomainError>> + Send + 'life0>>
    where
        'life1: 'life0;

    fn list_employee_availability<'life0, 'life1>(
        &'life0 self,
        user_ids: &'life1 [String],
        planned_start_time: DateTime<Utc>,
        planned_end_time: DateTime<Utc>,
        terminal: Option<&'life1 str>,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Vec<ResourceAvailability>, DomainError>> + Send + 'life0>>
    where
        'life1: 'life0;
}

#[derive(Clone)]
pub struct ResourceAvailabilityService {
    shift_instance_repo: Arc<dyn ShiftInstanceRepository + Send + Sync>,
    schedule_exception_repo: Arc<dyn ScheduleExceptionRepository + Send + Sync>,
    team_member_repo: Arc<dyn TeamMemberRepository + Send + Sync>,
    team_repo: Arc<dyn TeamRepository + Send + Sync>,
    order_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
    order_member_repo: Arc<dyn DispatchOrderMemberRepository + Send + Sync>,
}

impl ResourceAvailabilityService {
    pub fn new(
        shift_instance_repo: Arc<dyn ShiftInstanceRepository + Send + Sync>,
        schedule_exception_repo: Arc<dyn ScheduleExceptionRepository + Send + Sync>,
        team_member_repo: Arc<dyn TeamMemberRepository + Send + Sync>,
        team_repo: Arc<dyn TeamRepository + Send + Sync>,
        order_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
        order_member_repo: Arc<dyn DispatchOrderMemberRepository + Send + Sync>,
    ) -> Self {
        Self {
            shift_instance_repo,
            schedule_exception_repo,
            team_member_repo,
            team_repo,
            order_repo,
            order_member_repo,
        }
    }

    pub async fn evaluate_team(
        &self,
        team: &Team,
        planned_start_time: DateTime<Utc>,
        planned_end_time: DateTime<Utc>,
        terminal: Option<&str>,
        exclude_order_id: Option<&str>,
    ) -> Result<ResourceAvailability, DomainError> {
        let resource_id = team.id.clone();
        if !team.is_active {
            return Ok(unavailable(
                "team",
                &resource_id,
                ScheduleSource::CurrentStatusFallback,
                "班组已停用",
                HashMap::new(),
                DispatchLockLevel::Optimizable,
            ));
        }
        if let Some(value) = terminal {
            if team.terminal.as_deref().is_some_and(|item| item != value) {
                return Ok(unavailable(
                    "team",
                    &resource_id,
                    ScheduleSource::CurrentStatusFallback,
                    "班组不在目标航站楼值守",
                    HashMap::new(),
                    DispatchLockLevel::Optimizable,
                ));
            }
        }

        let instances = self
            .shift_instance_repo
            .find_for_resource_window("team", &resource_id, planned_start_time, planned_end_time)
            .await?;
        let has_shift_instance = !instances.is_empty();
        let schedule_source = if has_shift_instance {
            ScheduleSource::ShiftInstance
        } else {
            ScheduleSource::CurrentStatusFallback
        };
        let mut metadata = HashMap::new();
        let mut reasons = Vec::new();
        let min_rest_minutes =
            if let Some(active_instance) = pick_covering_instance(&instances, planned_start_time, planned_end_time) {
                metadata.insert("shift_instance_id".to_string(), json!(active_instance.id));
                if let Some(max_continuous_minutes) = active_instance.max_continuous_minutes {
                    let span_minutes =
                        ((planned_end_time - planned_start_time).num_seconds().max(0) as f64 / 60.0).round() as i32;
                    if span_minutes > max_continuous_minutes {
                        return Ok(unavailable(
                            "team",
                            &resource_id,
                            schedule_source,
                            "任务时长超过班组连续作业上限",
                            metadata,
                            DispatchLockLevel::Optimizable,
                        ));
                    }
                }
                active_instance.min_rest_minutes.unwrap_or(15)
            } else if has_shift_instance {
                return Ok(unavailable(
                    "team",
                    &resource_id,
                    schedule_source,
                    "目标时间窗没有排班实例覆盖",
                    metadata,
                    DispatchLockLevel::Optimizable,
                ));
            } else {
                if team.current_status != TeamStatus::OnDuty {
                    return Ok(unavailable(
                        "team",
                        &resource_id,
                        schedule_source,
                        "班组当前不在岗，且无排班实例兜底",
                        metadata,
                        DispatchLockLevel::Optimizable,
                    ));
                }
                reasons.push("无排班实例，已回退到 current_status=on_duty".to_string());
                metadata.insert("fallback".to_string(), json!(true));
                15
            };

        let members = self.team_member_repo.find_by_team(&resource_id, false).await?;
        let member_ids = members
            .iter()
            .map(|item| item.user_id.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect::<Vec<_>>();

        if !member_ids.is_empty() {
            let leave_records = self
                .schedule_exception_repo
                .find_leave_records(
                    &member_ids,
                    Some(&resource_id),
                    Some(planned_start_time),
                    Some(planned_end_time),
                )
                .await?;
            if !leave_records.is_empty() {
                let mut leave_meta = HashMap::new();
                leave_meta.insert(
                    "leave_user_ids".to_string(),
                    json!(leave_records.into_iter().map(|item| item.user_id).collect::<Vec<_>>()),
                );
                return Ok(unavailable(
                    "team",
                    &resource_id,
                    schedule_source,
                    "班组成员存在请休假冲突",
                    leave_meta,
                    DispatchLockLevel::Optimizable,
                ));
            }

            if let Some(rest_violation) = self
                .check_member_rest(&member_ids, planned_start_time, min_rest_minutes)
                .await?
            {
                return Ok(unavailable(
                    "team",
                    &resource_id,
                    schedule_source,
                    "班组成员未满足最小休息时间",
                    rest_violation,
                    DispatchLockLevel::Optimizable,
                ));
            }
        }

        let overlaps = self
            .order_repo
            .find_overlapping_orders(
                planned_start_time,
                planned_end_time,
                Some(&resource_id),
                None,
                None,
                exclude_order_id,
            )
            .await?;
        if !overlaps.is_empty() {
            let mut overlap_meta = HashMap::new();
            overlap_meta.insert(
                "overlapping_order_ids".to_string(),
                json!(overlaps.into_iter().map(|item| item.id).collect::<Vec<_>>()),
            );
            return Ok(unavailable(
                "team",
                &resource_id,
                schedule_source,
                "班组在目标时间窗已有派工占用",
                overlap_meta,
                DispatchLockLevel::Optimizable,
            ));
        }

        let no_dispatch_order_ids: Vec<String> = Vec::new();
        let lock_rules = self
            .schedule_exception_repo
            .find_lock_rules(
                &no_dispatch_order_ids,
                Some(&resource_id),
                Some(planned_start_time),
                Some(planned_end_time),
            )
            .await?;
        let lock_level = strongest_lock_level(&lock_rules);
        if matches!(lock_level, DispatchLockLevel::Frozen | DispatchLockLevel::ManualLock) {
            let mut lock_meta = HashMap::new();
            lock_meta.insert(
                "lock_rule_ids".to_string(),
                json!(lock_rules.into_iter().map(|item| item.id).collect::<Vec<_>>()),
            );
            return Ok(unavailable(
                "team",
                &resource_id,
                schedule_source,
                "班组存在冻结/人工锁定规则",
                lock_meta,
                lock_level,
            ));
        }

        let reason = reasons
            .first()
            .cloned()
            .unwrap_or_else(|| "班组在目标时间窗可用".to_string());
        let mut score_breakdown = HashMap::new();
        score_breakdown.insert("availability".to_string(), 100.0);
        score_breakdown.insert(
            "fallback_penalty".to_string(),
            if has_shift_instance { 0.0 } else { -8.0 },
        );
        score_breakdown.insert(
            "terminal_match".to_string(),
            if terminal.is_some() && team.terminal.as_deref() == terminal {
                10.0
            } else {
                0.0
            },
        );
        score_breakdown.insert(
            "member_ready".to_string(),
            if member_ids.is_empty() { 6.0 } else { 12.0 },
        );

        Ok(ResourceAvailability {
            resource_type: "team".to_string(),
            resource_id,
            available: true,
            schedule_source,
            reason,
            reasons,
            lock_level,
            score_breakdown,
            metadata,
        })
    }

    pub async fn evaluate_equipment(
        &self,
        equipment: &Equipment,
        planned_start_time: DateTime<Utc>,
        planned_end_time: DateTime<Utc>,
        terminal: Option<&str>,
        exclude_order_id: Option<&str>,
    ) -> Result<ResourceAvailability, DomainError> {
        let resource_id = equipment.id.clone();
        if !equipment.is_active {
            return Ok(unavailable(
                "equipment",
                &resource_id,
                ScheduleSource::CurrentStatusFallback,
                "设备已停用",
                HashMap::new(),
                DispatchLockLevel::Optimizable,
            ));
        }
        if equipment.status != EquipmentStatus::Available {
            return Ok(unavailable(
                "equipment",
                &resource_id,
                ScheduleSource::CurrentStatusFallback,
                "设备当前不可用",
                HashMap::new(),
                DispatchLockLevel::Optimizable,
            ));
        }
        if let Some(value) = terminal {
            if equipment.terminal.as_deref().is_some_and(|item| item != value) {
                return Ok(unavailable(
                    "equipment",
                    &resource_id,
                    ScheduleSource::CurrentStatusFallback,
                    "设备不在目标航站楼",
                    HashMap::new(),
                    DispatchLockLevel::Optimizable,
                ));
            }
        }

        let equipment_ids = vec![resource_id.clone()];
        let downtimes = self
            .schedule_exception_repo
            .find_equipment_downtimes(&equipment_ids, Some(planned_start_time), Some(planned_end_time))
            .await?;
        if !downtimes.is_empty() {
            let mut downtime_meta = HashMap::new();
            downtime_meta.insert(
                "downtime_ids".to_string(),
                json!(downtimes.into_iter().map(|item| item.id).collect::<Vec<_>>()),
            );
            return Ok(unavailable(
                "equipment",
                &resource_id,
                ScheduleSource::CurrentStatusFallback,
                "设备停机窗口与任务时间冲突",
                downtime_meta,
                DispatchLockLevel::Optimizable,
            ));
        }

        let conflicts = self
            .order_repo
            .find_equipment_conflicts(&equipment_ids, planned_start_time, planned_end_time, exclude_order_id)
            .await?;
        if !conflicts.is_empty() {
            let mut conflict_meta = HashMap::new();
            let ids = conflicts
                .iter()
                .filter_map(|item| item.get("dispatch_order_id").cloned())
                .collect::<Vec<_>>();
            conflict_meta.insert("conflicting_order_ids".to_string(), Value::Array(ids));
            return Ok(unavailable(
                "equipment",
                &resource_id,
                ScheduleSource::CurrentStatusFallback,
                "设备在目标时间窗已有占用",
                conflict_meta,
                DispatchLockLevel::Optimizable,
            ));
        }

        let mut score_breakdown = HashMap::new();
        score_breakdown.insert("availability".to_string(), 100.0);
        Ok(ResourceAvailability {
            resource_type: "equipment".to_string(),
            resource_id,
            available: true,
            schedule_source: ScheduleSource::CurrentStatusFallback,
            reason: "设备在目标时间窗可用".to_string(),
            reasons: Vec::new(),
            lock_level: DispatchLockLevel::Optimizable,
            score_breakdown,
            metadata: HashMap::new(),
        })
    }

    pub async fn evaluate_employee(
        &self,
        user_id: &str,
        planned_start_time: DateTime<Utc>,
        planned_end_time: DateTime<Utc>,
        terminal: Option<&str>,
        exclude_order_id: Option<&str>,
    ) -> Result<ResourceAvailability, DomainError> {
        let resource_id = user_id.trim().to_string();
        if resource_id.is_empty() {
            return Ok(unavailable(
                "employee",
                "",
                ScheduleSource::CurrentStatusFallback,
                "人员标识缺失",
                HashMap::new(),
                DispatchLockLevel::Optimizable,
            ));
        }

        let instances = self
            .shift_instance_repo
            .find_for_resource_window("employee", &resource_id, planned_start_time, planned_end_time)
            .await?;
        let has_shift_instance = !instances.is_empty();
        let schedule_source = if has_shift_instance {
            ScheduleSource::ShiftInstance
        } else {
            ScheduleSource::CurrentStatusFallback
        };
        let mut reasons = Vec::new();
        let mut metadata = HashMap::new();

        let min_rest_minutes =
            if let Some(active_instance) = pick_covering_instance(&instances, planned_start_time, planned_end_time) {
                metadata.insert("shift_instance_id".to_string(), json!(active_instance.id));
                active_instance.min_rest_minutes.unwrap_or(15)
            } else if has_shift_instance {
                return Ok(unavailable(
                    "employee",
                    &resource_id,
                    schedule_source,
                    "目标时间窗没有个人排班实例覆盖",
                    metadata,
                    DispatchLockLevel::Optimizable,
                ));
            } else if let Some(team) = self.resolve_employee_fallback_team(&resource_id, terminal).await? {
                reasons.push("无个人排班实例，已回退到换班归属班组 on_duty 判定".to_string());
                metadata.insert("fallback".to_string(), json!(true));
                metadata.insert("source_team_id".to_string(), json!(team.id));
                metadata.insert("source_team_name".to_string(), json!(team.name));
                15
            } else {
                return Ok(unavailable(
                    "employee",
                    &resource_id,
                    schedule_source,
                    "人员当前不在岗，且无个人排班实例兜底",
                    metadata,
                    DispatchLockLevel::Optimizable,
                ));
            };

        let leave_records = self
            .schedule_exception_repo
            .find_leave_records(
                &[resource_id.clone()],
                None,
                Some(planned_start_time),
                Some(planned_end_time),
            )
            .await?;
        if !leave_records.is_empty() {
            let mut leave_meta = HashMap::new();
            leave_meta.insert(
                "leave_user_ids".to_string(),
                json!(leave_records.into_iter().map(|item| item.user_id).collect::<Vec<_>>()),
            );
            return Ok(unavailable(
                "employee",
                &resource_id,
                schedule_source,
                "人员存在请休假冲突",
                leave_meta,
                DispatchLockLevel::Optimizable,
            ));
        }

        if let Some(rest_violation) = self
            .check_member_rest(&[resource_id.clone()], planned_start_time, min_rest_minutes)
            .await?
        {
            return Ok(unavailable(
                "employee",
                &resource_id,
                schedule_source,
                "人员未满足最小休息时间",
                rest_violation,
                DispatchLockLevel::Optimizable,
            ));
        }

        let overlaps = self
            .order_repo
            .find_overlapping_orders(
                planned_start_time,
                planned_end_time,
                None,
                Some(&resource_id),
                None,
                exclude_order_id,
            )
            .await?;
        if !overlaps.is_empty() {
            let mut overlap_meta = HashMap::new();
            overlap_meta.insert(
                "overlapping_order_ids".to_string(),
                json!(overlaps.into_iter().map(|item| item.id).collect::<Vec<_>>()),
            );
            return Ok(unavailable(
                "employee",
                &resource_id,
                schedule_source,
                "人员在目标时间窗已有派工占用",
                overlap_meta,
                DispatchLockLevel::Optimizable,
            ));
        }

        let reason = reasons
            .first()
            .cloned()
            .unwrap_or_else(|| "人员在目标时间窗可用".to_string());
        let mut score_breakdown = HashMap::new();
        score_breakdown.insert("availability".to_string(), 100.0);
        score_breakdown.insert(
            "fallback_penalty".to_string(),
            if has_shift_instance { 0.0 } else { -8.0 },
        );
        Ok(ResourceAvailability {
            resource_type: "employee".to_string(),
            resource_id,
            available: true,
            schedule_source,
            reason,
            reasons,
            lock_level: DispatchLockLevel::Optimizable,
            score_breakdown,
            metadata,
        })
    }

    pub async fn list_team_availability(
        &self,
        teams: &[Team],
        planned_start_time: DateTime<Utc>,
        planned_end_time: DateTime<Utc>,
        terminal: Option<&str>,
    ) -> Result<Vec<ResourceAvailability>, DomainError> {
        let mut results = Vec::with_capacity(teams.len());
        for team in teams {
            results.push(
                self.evaluate_team(team, planned_start_time, planned_end_time, terminal, None)
                    .await?,
            );
        }
        Ok(results)
    }

    pub async fn list_employee_availability(
        &self,
        user_ids: &[String],
        planned_start_time: DateTime<Utc>,
        planned_end_time: DateTime<Utc>,
        terminal: Option<&str>,
    ) -> Result<Vec<ResourceAvailability>, DomainError> {
        let mut results = Vec::with_capacity(user_ids.len());
        for user_id in user_ids {
            results.push(
                self.evaluate_employee(user_id, planned_start_time, planned_end_time, terminal, None)
                    .await?,
            );
        }
        Ok(results)
    }

    async fn check_member_rest(
        &self,
        user_ids: &[String],
        planned_start_time: DateTime<Utc>,
        min_rest_minutes: i32,
    ) -> Result<Option<HashMap<String, Value>>, DomainError> {
        let required_rest_minutes = min_rest_minutes.max(0) as f64;
        for user_id in user_ids {
            let latest = self
                .order_member_repo
                .find_latest_checkout_for_user(user_id, planned_start_time)
                .await?;
            let Some(mut latest) = latest else {
                continue;
            };
            let checkout_time = latest
                .get("check_out_time")
                .cloned()
                .and_then(|item| serde_json::from_value::<DateTime<Utc>>(item).ok());
            let Some(checkout_time) = checkout_time else {
                continue;
            };
            let gap_minutes = (planned_start_time - checkout_time).num_seconds() as f64 / 60.0;
            if gap_minutes < required_rest_minutes {
                let mut metadata = HashMap::new();
                metadata.insert("user_id".to_string(), json!(user_id));
                let latest_checkout_order_id = latest
                    .as_object_mut()
                    .and_then(|object| object.remove("dispatch_order_id"))
                    .unwrap_or(Value::Null);
                metadata.insert("latest_checkout_order_id".to_string(), latest_checkout_order_id);
                metadata.insert("gap_minutes".to_string(), json!(round_to_2(gap_minutes.max(0.0))));
                metadata.insert("required_rest_minutes".to_string(), json!(required_rest_minutes as i32));
                return Ok(Some(metadata));
            }
        }
        Ok(None)
    }

    async fn resolve_employee_fallback_team(
        &self,
        user_id: &str,
        terminal: Option<&str>,
    ) -> Result<Option<Team>, DomainError> {
        let memberships = self.team_member_repo.find_by_user(user_id).await?;
        for membership in memberships {
            let Some(team) = self.team_repo.find_by_id(&membership.team_id, false).await? else {
                continue;
            };
            if !team.is_active {
                continue;
            }
            if let Some(value) = terminal {
                if team.terminal.as_deref().is_some_and(|item| item != value) {
                    continue;
                }
            }
            if team.current_status == TeamStatus::OnDuty {
                return Ok(Some(team));
            }
        }
        Ok(None)
    }
}

impl ResourceAvailabilityGateway for ResourceAvailabilityService {
    fn list_team_availability<'life0, 'life1>(
        &'life0 self,
        teams: &'life1 [Team],
        planned_start_time: DateTime<Utc>,
        planned_end_time: DateTime<Utc>,
        terminal: Option<&'life1 str>,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Vec<ResourceAvailability>, DomainError>> + Send + 'life0>>
    where
        'life1: 'life0,
    {
        let this = self.clone();
        let teams = teams.to_vec();
        let terminal = terminal.map(|s| s.to_string());
        Box::pin(async move {
            let mut results = Vec::with_capacity(teams.len());
            for team in teams {
                let availability = this
                    .evaluate_team(&team, planned_start_time, planned_end_time, terminal.as_deref(), None)
                    .await?;
                results.push(availability);
            }
            Ok(results)
        })
    }

    fn evaluate_equipment<'life0, 'life1>(
        &'life0 self,
        equipment: &'life1 Equipment,
        planned_start_time: DateTime<Utc>,
        planned_end_time: DateTime<Utc>,
        terminal: Option<&'life1 str>,
        exclude_order_id: Option<&'life1 str>,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<ResourceAvailability, DomainError>> + Send + 'life0>>
    where
        'life1: 'life0,
    {
        let this = self.clone();
        let equipment = equipment.clone();
        let terminal = terminal.map(|s| s.to_string());
        let exclude_order_id = exclude_order_id.map(|s| s.to_string());
        Box::pin(async move {
            this.evaluate_equipment(
                &equipment,
                planned_start_time,
                planned_end_time,
                terminal.as_deref(),
                exclude_order_id.as_deref(),
            )
            .await
        })
    }

    fn list_employee_availability<'life0, 'life1>(
        &'life0 self,
        user_ids: &'life1 [String],
        planned_start_time: DateTime<Utc>,
        planned_end_time: DateTime<Utc>,
        terminal: Option<&'life1 str>,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Vec<ResourceAvailability>, DomainError>> + Send + 'life0>>
    where
        'life1: 'life0,
    {
        let this = self.clone();
        let user_ids = user_ids.to_vec();
        let terminal = terminal.map(|s| s.to_string());
        Box::pin(async move {
            let mut results = Vec::with_capacity(user_ids.len());
            for user_id in user_ids {
                let availability = this
                    .evaluate_employee(
                        &user_id,
                        planned_start_time,
                        planned_end_time,
                        terminal.as_deref(),
                        None,
                    )
                    .await?;
                results.push(availability);
            }
            Ok(results)
        })
    }
}

pub struct NullAvailabilityGateway;

impl ResourceAvailabilityGateway for NullAvailabilityGateway {
    fn list_team_availability<'life0, 'life1>(
        &'life0 self,
        _teams: &'life1 [Team],
        _planned_start_time: DateTime<Utc>,
        _planned_end_time: DateTime<Utc>,
        _terminal: Option<&'life1 str>,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Vec<ResourceAvailability>, DomainError>> + Send + 'life0>>
    where
        'life1: 'life0,
    {
        Box::pin(async { Ok(vec![]) })
    }

    fn evaluate_equipment<'life0, 'life1>(
        &'life0 self,
        _equipment: &'life1 Equipment,
        _planned_start_time: DateTime<Utc>,
        _planned_end_time: DateTime<Utc>,
        _terminal: Option<&'life1 str>,
        _exclude_order_id: Option<&'life1 str>,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<ResourceAvailability, DomainError>> + Send + 'life0>>
    where
        'life1: 'life0,
    {
        Box::pin(async { Err(DomainError::Internal("NullAvailabilityGateway".into())) })
    }

    fn list_employee_availability<'life0, 'life1>(
        &'life0 self,
        _user_ids: &'life1 [String],
        _planned_start_time: DateTime<Utc>,
        _planned_end_time: DateTime<Utc>,
        _terminal: Option<&'life1 str>,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Vec<ResourceAvailability>, DomainError>> + Send + 'life0>>
    where
        'life1: 'life0,
    {
        Box::pin(async { Ok(vec![]) })
    }
}

fn unavailable(
    resource_type: &str,
    resource_id: &str,
    schedule_source: ScheduleSource,
    reason: &str,
    metadata: HashMap<String, Value>,
    lock_level: DispatchLockLevel,
) -> ResourceAvailability {
    ResourceAvailability {
        resource_type: resource_type.to_string(),
        resource_id: resource_id.to_string(),
        available: false,
        schedule_source,
        reason: reason.to_string(),
        reasons: vec![reason.to_string()],
        lock_level,
        score_breakdown: HashMap::new(),
        metadata,
    }
}

fn pick_covering_instance(
    instances: &[ShiftInstance],
    planned_start_time: DateTime<Utc>,
    planned_end_time: DateTime<Utc>,
) -> Option<ShiftInstance> {
    instances
        .iter()
        .find(|instance| instance.start_time <= planned_start_time && instance.end_time >= planned_end_time)
        .cloned()
        .or_else(|| instances.first().cloned())
}

fn strongest_lock_level(items: &[fms_domain::models::dispatch::DispatchLockRule]) -> DispatchLockLevel {
    items
        .iter()
        .map(|item| item.lock_level)
        .max_by_key(|level| lock_rank(*level))
        .unwrap_or(DispatchLockLevel::Optimizable)
}

fn lock_rank(level: DispatchLockLevel) -> i32 {
    match level {
        DispatchLockLevel::Optimizable => 1,
        DispatchLockLevel::Active => 2,
        DispatchLockLevel::Frozen => 3,
        DispatchLockLevel::ManualLock => 4,
    }
}

fn round_to_2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}
