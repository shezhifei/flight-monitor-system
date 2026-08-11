use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::*;
use fms_domain::ports::dispatch_repository::CreateDispatchOrderCommand;

use crate::schemas::dispatch_schemas::*;

use super::helpers::order_to_response;
use super::{DispatchService, NULL_VALUE};

impl DispatchService {
    /// 创建派工单
    pub async fn create_order(
        &self,
        dto: DispatchOrderCreate,
        actor_id: &str,
    ) -> Result<DispatchOrderResponse, DomainError> {
        Self::ensure_actor(actor_id)?;
        let DispatchOrderCreate {
            flight_id,
            task_type,
            temporary_task_template_code,
            department_id,
            stand_id,
            location,
            assignee_type,
            team_id,
            individual_user_id,
            planned_start_time,
            planned_end_time,
            priority,
            workflow_context,
            publication_state,
            source_type,
            leg_scope,
            crew_requirement_snapshot,
            equipment_requirement_snapshot,
            task_crew,
            equipment_assignment,
            manual_lock,
            remarks,
        } = dto;

        let flight_id = flight_id.unwrap_or_default();
        if let (Some(start), Some(end)) = (planned_start_time.as_ref(), planned_end_time.as_ref()) {
            if end <= start {
                return Err(DomainError::ValidationError(
                    "planned_end_time 必须晚于 planned_start_time".to_string(),
                ));
            }
        }

        let resolved_department_id = Self::normalize_optional_string(department_id);
        let mut resolved_task_type = Self::normalize_optional_string(task_type);
        let resolved_template_code = Self::normalize_optional_string(temporary_task_template_code);
        let mut resolved_crew_requirement_snapshot = crew_requirement_snapshot;
        let mut resolved_equipment_requirement_snapshot = equipment_requirement_snapshot;
        let mut department_rule_version: Option<String> = None;

        if let Some(template_code) = resolved_template_code.as_deref() {
            let department_id = resolved_department_id.as_deref().ok_or_else(|| {
                DomainError::ValidationError("temporary_task_template_code 提供时必须指定 department_id".to_string())
            })?;
            let template_repo = self
                .rules
                .temporary_task_template_repo
                .as_ref()
                .ok_or_else(|| DomainError::Internal("临时任务模板仓储未配置".to_string()))?;
            let template = template_repo
                .find_by_code(department_id, template_code)
                .await?
                .ok_or_else(|| DomainError::ValidationError(format!("临时任务模板 {template_code} 不存在")))?;
            if !template.is_active {
                return Err(DomainError::ValidationError(format!(
                    "临时任务模板 {template_code} 未启用"
                )));
            }
            if resolved_task_type.is_none() {
                resolved_task_type = Self::normalize_optional_string(Some(template.task_type.clone()));
            }
            if resolved_crew_requirement_snapshot.is_empty() {
                resolved_crew_requirement_snapshot =
                    Self::serialize_crew_requirement_snapshot(&template.crew_requirements);
            }
            if resolved_equipment_requirement_snapshot.is_empty() {
                resolved_equipment_requirement_snapshot =
                    Self::serialize_equipment_requirement_snapshot(&template.equipment_requirements);
            }
            department_rule_version = Some(format!("temporary-template:{}", template.id));
            if resolved_crew_requirement_snapshot.is_empty() {
                return Err(DomainError::ValidationError(format!(
                    "临时任务模板 {template_code} 缺少人员资质要求"
                )));
            }
            if resolved_equipment_requirement_snapshot.is_empty() {
                return Err(DomainError::ValidationError(format!(
                    "临时任务模板 {template_code} 缺少设备类型要求"
                )));
            }
        }

        let resolved_task_type = resolved_task_type.ok_or_else(|| {
            DomainError::ValidationError("task_type 与 temporary_task_template_code 至少提供一个".to_string())
        })?;

        let snapshots_complete =
            !resolved_crew_requirement_snapshot.is_empty() && !resolved_equipment_requirement_snapshot.is_empty();
        if !snapshots_complete {
            let department_id = resolved_department_id.as_deref().ok_or_else(|| {
                DomainError::ValidationError("必须指定 task_type 或 temporary_task_template_code".to_string())
            })?;
            let requirement_repo = self.rules.task_type_requirement_repo.as_ref().ok_or_else(|| {
                DomainError::Internal("派工规则服务未配置，无法按作业类型自动带出人员与设备需求".to_string())
            })?;
            let requirement_version = requirement_repo
                .find_published(department_id, &resolved_task_type)
                .await?
                .ok_or_else(|| {
                    DomainError::ValidationError(format!("作业类型 {resolved_task_type} 缺少已发布作业类型规则"))
                })?;

            if resolved_crew_requirement_snapshot.is_empty() {
                resolved_crew_requirement_snapshot =
                    Self::serialize_crew_requirement_snapshot(&requirement_version.crew_requirements);
                if resolved_crew_requirement_snapshot.is_empty() {
                    return Err(DomainError::ValidationError(format!(
                        "作业类型 {resolved_task_type} 缺少人员资质要求"
                    )));
                }
            }
            if resolved_equipment_requirement_snapshot.is_empty() {
                resolved_equipment_requirement_snapshot =
                    Self::serialize_equipment_requirement_snapshot(&requirement_version.equipment_requirements);
                if resolved_equipment_requirement_snapshot.is_empty() {
                    return Err(DomainError::ValidationError(format!(
                        "作业类型 {resolved_task_type} 缺少设备类型要求"
                    )));
                }
            }
            if department_rule_version.is_none() {
                department_rule_version = Some(requirement_version.id);
            }
        }

        let now = Utc::now();
        let order_id = Self::new_dispatch_id();

        let normalized_team_id = Self::normalize_optional_string(team_id);
        let normalized_individual_user_id = Self::normalize_optional_string(individual_user_id);
        let has_explicit_assignee = normalized_team_id.is_some() || normalized_individual_user_id.is_some();
        let has_inline_assignment = !task_crew.is_empty() || !equipment_assignment.is_empty();
        let assignee_type = match assignee_type.as_deref().map(str::trim) {
            Some("individual") => AssigneeType::Individual,
            Some("team") => AssigneeType::Team,
            None if normalized_individual_user_id.is_some() => AssigneeType::Individual,
            _ => AssigneeType::Team,
        };

        if has_explicit_assignee && assignee_type == AssigneeType::Team && normalized_team_id.is_none() {
            return Err(DomainError::ValidationError("班组派工必须提供 team_id".to_string()));
        }
        if has_explicit_assignee && assignee_type == AssigneeType::Individual && normalized_individual_user_id.is_none()
        {
            return Err(DomainError::ValidationError(
                "个人派工必须提供 individual_user_id".to_string(),
            ));
        }

        let normalized_publication_state = publication_state.trim();
        let publication_state = if normalized_publication_state.is_empty() {
            "published".to_string()
        } else {
            normalized_publication_state.to_string()
        };
        let normalized_source_type = source_type.trim();
        let source_type = if normalized_source_type.is_empty() {
            "manual".to_string()
        } else {
            normalized_source_type.to_string()
        };
        let normalized_leg_scope = leg_scope.trim();
        let leg_scope = if normalized_leg_scope.is_empty() {
            "none".to_string()
        } else {
            normalized_leg_scope.to_string()
        };
        let initial_status = if publication_state == "published" && (has_explicit_assignee || has_inline_assignment) {
            DispatchOrderStatus::Assigned
        } else {
            DispatchOrderStatus::Pending
        };

        let mut workflow_context = serde_json::Value::Object(serde_json::Map::from_iter(workflow_context.into_iter()));
        if let Some(location) = Self::normalize_optional_string(location) {
            workflow_context["manual_location"] = json!(location);
        }
        if let Some(remarks) = Self::normalize_optional_string(remarks) {
            workflow_context["manual_remarks"] = json!(remarks);
        }
        if let Some(priority) = priority {
            workflow_context["manual_priority"] = json!(priority);
        }
        if let Some(template_code) = resolved_template_code.clone() {
            workflow_context["temporary_task_template_code"] = json!(template_code);
        }

        let task_crew = serde_json::Value::Object(serde_json::Map::from_iter(task_crew.into_iter()));

        let mut order = DispatchOrder {
            id: order_id.clone(),
            flight_id,
            task_type: resolved_task_type.clone(),
            stand_id,
            task_type_name: None,
            stand_code: None,
            terminal: None,
            assignee_type,
            team_id: normalized_team_id.clone(),
            team_name: None,
            department: None,
            individual_user_id: normalized_individual_user_id.clone(),
            individual_username: None,
            driver_type: None,
            driver_team_id: None,
            driver_user_id: None,
            planned_start_time,
            planned_end_time,
            actual_start_time: None,
            actual_end_time: None,
            estimated_completion_time: None,
            estimated_completion_reported_by: None,
            estimated_completion_reported_at: None,
            estimated_completion_note: None,
            status: initial_status,
            dispatch_type: DispatchType::Manual,
            dispatched_at: (initial_status == DispatchOrderStatus::Assigned).then_some(now),
            dispatched_by: (initial_status == DispatchOrderStatus::Assigned).then(|| actor_id.to_string()),
            snapshot_assignee_position: None,
            snapshot_equipment_positions: None,
            estimated_arrival_minutes: None,
            process_instance_id: None,
            process_task_id: None,
            workflow_context,
            workflow_status: "pending_assignment".into(),
            source: "manual".into(),
            schedule_source: ScheduleSource::CurrentStatusFallback,
            lock_level: if manual_lock {
                DispatchLockLevel::ManualLock
            } else {
                DispatchLockLevel::Optimizable
            },
            publication_state,
            source_type,
            department_id: resolved_department_id,
            leg_scope,
            generation_rule_id: None,
            generation_rule_version: None,
            generation_anchor_type: None,
            generation_anchor_time: None,
            completion_time_mode: None,
            completion_anchor_type: None,
            completion_anchor_time: None,
            completion_offset_minutes: None,
            completion_warning_lead_minutes: None,
            publish_trigger_mode: None,
            publish_at: None,
            turnaround_pair_key: None,
            turnaround_constraint_mode: None,
            department_rule_version,
            crew_requirement_snapshot: resolved_crew_requirement_snapshot,
            equipment_requirement_snapshot: resolved_equipment_requirement_snapshot,
            task_crew,
            equipment_assignment,
            qualification_gap: vec![],
            equipment_gap: vec![],
            availability_reason: None,
            score_breakdown: Default::default(),
            conflict_reason: None,
            recommended_assignees: vec![],
            recommendation_score: None,
            supervisor_notified: false,
            supervisor_notified_at: None,
            assignment_deadline: None,
            completed_by: None,
            completion_notes: None,
            gate: None,
            created_at: Some(now),
            updated_at: Some(now),
            members: vec![],
            equipment_list: vec![],
        };

        let should_prepare_for_optimization =
            !has_explicit_assignee && !has_inline_assignment && !manual_lock && order.publication_state == "published";
        if should_prepare_for_optimization {
            let preparation_result = self.prepare_order_for_publication_internal(&mut order, false).await?;
            order.qualification_gap = preparation_result
                .get("qualification_gap")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            order.equipment_gap = preparation_result
                .get("equipment_gap")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            order.availability_reason = preparation_result
                .get("reason")
                .and_then(Value::as_str)
                .map(str::to_string);
            order.updated_at = Some(Utc::now());
        }

        let mut persisted_members = Vec::new();
        if assignee_type == AssigneeType::Team {
            if let (Some(team_id), Some(team_member_repo)) =
                (normalized_team_id.as_deref(), self.resources.team_member_repo.as_ref())
            {
                let team_members = team_member_repo.find_by_team(team_id, false).await?;
                for tm in &team_members {
                    persisted_members.push(DispatchOrderMember {
                        id: ulid::Ulid::new().to_string(),
                        dispatch_order_id: order_id.clone(),
                        user_id: tm.user_id.clone(),
                        role: tm.role,
                        source_type: AssigneeType::Team,
                        source_team_id: Some(team_id.to_string()),
                        slot_code: None,
                        qualification_code: None,
                        qualification_level_code: None,
                        assigned_at: Some(now),
                        check_in_time: None,
                        check_out_time: None,
                        is_active: true,
                        username: tm.username.clone(),
                    });
                }
            }
        }
        if should_prepare_for_optimization {
            persisted_members.extend(order.members.iter().cloned());
        }

        let equipment_ids = if should_prepare_for_optimization {
            order
                .equipment_assignment
                .iter()
                .filter_map(|item| {
                    item.get("equipment_id")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string)
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        self.order
            .order_repo
            .create_order_atomic(CreateDispatchOrderCommand {
                order: order.clone(),
                members: persisted_members,
                persist_equipment_assignments: should_prepare_for_optimization,
                equipment_ids,
                log_action: "created".to_string(),
                log_actor_id: Some(actor_id.to_string()),
                log_details: Some(serde_json::json!({
                    "event_id": Self::new_dispatch_id(),
                    "task_type": resolved_task_type,
                    "assignee_type": match assignee_type {
                        AssigneeType::Individual => "individual",
                        AssigneeType::Team => "team",
                    },
                    "team_id": normalized_team_id,
                    "individual_user_id": normalized_individual_user_id,
                    "temporary_task_template_code": resolved_template_code,
                })),
            })
            .await?;

        self.sync_dispatch_chat_for_order(&order_id).await;

        Ok(order_to_response(&order))
    }

    /// 查询单个派工单
    pub async fn get_order(&self, order_id: &str) -> Result<Option<DispatchOrderResponse>, DomainError> {
        let order = self.order.order_repo.find_by_id(order_id, true, None).await?;
        Ok(order.map(|o| order_to_response(&o)))
    }

    pub async fn get_order_domain(&self, order_id: &str) -> Result<Option<DispatchOrder>, DomainError> {
        self.order.order_repo.find_by_id(order_id, true, None).await
    }

    pub async fn reassign_order(
        &self,
        order_id: &str,
        assignee_id: &str,
        assignee_type: Option<&str>,
        actor_id: &str,
        assignment_patch: Option<&Value>,
    ) -> Result<DispatchOrderResponse, DomainError> {
        Self::ensure_actor(actor_id)?;
        let assignee_id = assignee_id.trim();
        if assignee_id.is_empty() {
            return Err(DomainError::ValidationError("assignee_id is required".into()));
        }

        let Some(mut order) = self.order.order_repo.find_by_id(order_id, true, None).await? else {
            return Err(DomainError::NotFound {
                entity_type: "DispatchOrder",
                id: order_id.to_string(),
            });
        };

        let mut assignment = Self::assignment_from_order(&order);
        match assignee_type
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("team")
        {
            "team" => {
                assignment["assignee_type"] = json!("team");
                assignment["team_id"] = json!(assignee_id);
                assignment["team_name"] = if let Some(team_repo) = self.resources.team_repo.as_ref() {
                    match team_repo.find_by_id(assignee_id, false).await? {
                        Some(team) => json!(team.name),
                        None => Value::Null,
                    }
                } else {
                    Value::Null
                };
                assignment["individual_user_id"] = Value::Null;
                assignment["individual_username"] = Value::Null;
            }
            "individual" | "user" => {
                assignment["assignee_type"] = json!("individual");
                assignment["individual_user_id"] = json!(assignee_id);
                assignment["individual_username"] = assignment_patch
                    .and_then(|patch| patch.get("individual_username"))
                    .cloned()
                    .unwrap_or(Value::Null);
                assignment["team_id"] = Value::Null;
                assignment["team_name"] = Value::Null;
            }
            other => {
                return Err(DomainError::ValidationError(format!(
                    "unsupported assignee_type: {other}"
                )));
            }
        }

        if let Some(Value::Object(patch)) = assignment_patch {
            if let Some(target) = assignment.as_object_mut() {
                for (key, value) in patch {
                    target.insert(key.clone(), value.clone());
                }
            }
        }

        Self::apply_assignment_json(&mut order, Some(&assignment));
        if order.status == DispatchOrderStatus::Pending {
            order.status = DispatchOrderStatus::Assigned;
        }
        order.dispatched_by = Some(actor_id.to_string());
        order.dispatched_at = order.dispatched_at.or_else(|| Some(Utc::now()));
        order.updated_at = Some(Utc::now());

        self.sync_assignment_members(&order, &assignment).await?;
        self.order.order_repo.save(&order).await?;
        self.order
            .order_repo
            .append_log(
                &order.id,
                "reassigned",
                Some(actor_id),
                Some(json!({
                    "assignee_id": assignee_id,
                    "assignee_type": assignment.get("assignee_type").unwrap_or(&Value::Null),
                    "source": "ai_action",
                })),
            )
            .await?;
        self.sync_dispatch_chat_for_order(&order.id).await;
        self.maybe_evaluate_overrun_warning(&order.id).await;

        Ok(order_to_response(&order))
    }

    pub async fn reassign_order_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        order_id: &str,
        assignee_id: &str,
        assignee_type: Option<&str>,
        actor_id: &str,
        assignment_patch: Option<&Value>,
    ) -> Result<DispatchOrderResponse, DomainError> {
        Self::ensure_actor(actor_id)?;
        let assignee_id = assignee_id.trim();
        if assignee_id.is_empty() {
            return Err(DomainError::ValidationError("assignee_id is required".into()));
        }

        let Some(mut order) = self.order.order_repo.find_by_id(order_id, true, None).await? else {
            return Err(DomainError::NotFound {
                entity_type: "DispatchOrder",
                id: order_id.to_string(),
            });
        };

        let mut assignment = Self::assignment_from_order(&order);
        match assignee_type
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("team")
        {
            "team" => {
                assignment["assignee_type"] = json!("team");
                assignment["team_id"] = json!(assignee_id);
                assignment["team_name"] = if let Some(team_repo) = self.resources.team_repo.as_ref() {
                    match team_repo.find_by_id(assignee_id, false).await? {
                        Some(team) => json!(team.name),
                        None => Value::Null,
                    }
                } else {
                    Value::Null
                };
                assignment["individual_user_id"] = Value::Null;
                assignment["individual_username"] = Value::Null;
            }
            "individual" | "user" => {
                assignment["assignee_type"] = json!("individual");
                assignment["individual_user_id"] = json!(assignee_id);
                assignment["individual_username"] = assignment_patch
                    .and_then(|patch| patch.get("individual_username"))
                    .cloned()
                    .unwrap_or(Value::Null);
                assignment["team_id"] = Value::Null;
                assignment["team_name"] = Value::Null;
            }
            other => {
                return Err(DomainError::ValidationError(format!(
                    "unsupported assignee_type: {other}"
                )));
            }
        }

        if let Some(Value::Object(patch)) = assignment_patch {
            if let Some(target) = assignment.as_object_mut() {
                for (key, value) in patch {
                    target.insert(key.clone(), value.clone());
                }
            }
        }

        Self::apply_assignment_json(&mut order, Some(&assignment));
        if order.status == DispatchOrderStatus::Pending {
            order.status = DispatchOrderStatus::Assigned;
        }
        order.dispatched_by = Some(actor_id.to_string());
        order.dispatched_at = order.dispatched_at.or_else(|| Some(Utc::now()));
        order.updated_at = Some(Utc::now());

        let order_tx_repo = self.order.order_tx_repo.as_ref().ok_or_else(|| {
            DomainError::Internal("DispatchService order transactional repository is not configured".to_string())
        })?;
        self.sync_assignment_members_in_tx(tx, &order, &assignment).await?;
        order_tx_repo.save_in_tx(tx, &order).await?;
        order_tx_repo
            .append_log_in_tx(
                tx,
                &order.id,
                "reassigned",
                Some(actor_id),
                Some(json!({
                    "assignee_id": assignee_id,
                    "assignee_type": assignment.get("assignee_type").unwrap_or(&Value::Null),
                    "source": "ai_action",
                })),
            )
            .await?;

        Ok(order_to_response(&order))
    }

    pub async fn prepare_order_for_publication_internal(
        &self,
        order: &mut DispatchOrder,
        persist_side_effects: bool,
    ) -> Result<Value, DomainError> {
        let order_id = order.id.trim();
        if order_id.is_empty() {
            return Ok(json!({
                "updated": false,
                "reason": "工单缺少ID",
            }));
        }

        let stand_position = if let Some(stand_id) = order
            .stand_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let Some(stand_repo) = self.resources.stand_repo.as_ref() else {
                return Ok(json!({
                    "updated": false,
                    "reason": format!("工单 {order_id} 缺少机位信息"),
                }));
            };
            let Some(stand) = stand_repo.find_by_id(stand_id).await? else {
                return Ok(json!({
                    "updated": false,
                    "reason": format!("工单 {order_id} 缺少机位信息"),
                }));
            };
            Some((stand.position_lat, stand.position_lng))
        } else {
            None
        };
        if stand_position.is_none() {
            return Ok(json!({
                "updated": false,
                "reason": format!("工单 {order_id} 缺少机位信息"),
            }));
        }

        let planned_start_time = order.planned_start_time.unwrap_or_else(Utc::now);
        let planned_end_time = order
            .planned_end_time
            .unwrap_or_else(|| planned_start_time + Duration::minutes(15));

        let Some(department_id) = self.resolve_order_department_id(order).await? else {
            return Ok(json!({
                "updated": false,
                "reason": "当前无法补齐执行编组",
                "qualification_gap": [{
                    "reason": "missing_department_rule_context",
                }],
            }));
        };

        let (crew_requirement_snapshot, equipment_requirement_snapshot, department_rule_version) =
            self.resolve_order_requirement_snapshots(order, &department_id).await?;
        if crew_requirement_snapshot.is_empty() {
            return Ok(json!({
                "updated": false,
                "reason": "作业类型缺少已发布资质规则",
                "qualification_gap": [{
                    "reason": "missing_published_rule",
                }],
                "equipment_gap": [],
            }));
        }

        let (task_crew_members, qualification_gap, availability_reason, dominant_team_id, dominant_team_name) = self
            .select_preparation_members(
                order,
                &department_id,
                planned_start_time,
                planned_end_time,
                &crew_requirement_snapshot,
            )
            .await?;
        if !qualification_gap.is_empty() {
            return Ok(json!({
                "updated": false,
                "reason": availability_reason.unwrap_or_else(|| "当前无法补齐执行编组".to_string()),
                "qualification_gap": qualification_gap,
                "equipment_gap": [],
            }));
        }

        let (equipment_assignment, equipment_gap) = self
            .assign_equipment_for_publication(
                order,
                planned_start_time,
                planned_end_time,
                stand_position,
                &equipment_requirement_snapshot,
                &task_crew_members,
            )
            .await?;
        if !equipment_gap.is_empty() {
            return Ok(json!({
                "updated": false,
                "reason": "当前无法补齐设备分配",
                "qualification_gap": [],
                "equipment_gap": equipment_gap,
            }));
        }

        let assignee_type = if task_crew_members.len() == 1 {
            AssigneeType::Individual
        } else {
            AssigneeType::Team
        };
        let individual_user_id = (assignee_type == AssigneeType::Individual)
            .then(|| {
                task_crew_members
                    .first()
                    .and_then(|item| item.get("user_id"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .flatten();
        let individual_username = (assignee_type == AssigneeType::Individual)
            .then(|| {
                task_crew_members
                    .first()
                    .and_then(|item| item.get("username"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .flatten();
        let source_team_ids = task_crew_members
            .iter()
            .filter_map(|item| item.get("source_team_id").and_then(Value::as_str))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        let source_team_names = task_crew_members
            .iter()
            .filter_map(|item| item.get("source_team_name").and_then(Value::as_str))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        let task_crew = serde_json::Value::Object(serde_json::Map::from_iter(vec![
            ("members".to_string(), json!(task_crew_members.clone())),
            ("source_team_ids".to_string(), json!(source_team_ids)),
            ("source_team_names".to_string(), json!(source_team_names)),
            ("generated_from".to_string(), json!("qualification_coverage")),
        ]));

        order.assignee_type = assignee_type;
        order.team_id = dominant_team_id.clone();
        order.team_name = dominant_team_name.clone();
        order.individual_user_id = individual_user_id.clone();
        order.individual_username = individual_username;
        if order.status == DispatchOrderStatus::Pending {
            order.status = DispatchOrderStatus::Assigned;
        }
        order.dispatched_at = order.dispatched_at.or_else(|| Some(Utc::now()));
        order.dispatch_type = DispatchType::Auto;
        order.department_id = Some(department_id);
        order.department_rule_version = department_rule_version;
        order.crew_requirement_snapshot = crew_requirement_snapshot;
        order.equipment_requirement_snapshot = equipment_requirement_snapshot;
        order.task_crew = task_crew.clone();
        order.equipment_assignment = equipment_assignment.clone();
        order.qualification_gap = Vec::new();
        order.equipment_gap = Vec::new();
        order.availability_reason = availability_reason;
        order.lock_level = DispatchLockLevel::Optimizable;
        order.updated_at = Some(Utc::now());

        if order.members.is_empty() {
            let now = Utc::now();
            order.members = task_crew_members
                .iter()
                .filter_map(|item| {
                    let user_id = item
                        .get("user_id")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())?
                        .to_string();
                    Some(DispatchOrderMember {
                        id: Self::new_dispatch_id(),
                        dispatch_order_id: order.id.clone(),
                        user_id,
                        role: MemberRole::Member,
                        source_type: assignee_type,
                        source_team_id: item
                            .get("source_team_id")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .map(str::to_string)
                            .or_else(|| dominant_team_id.clone()),
                        slot_code: item.get("slot_code").and_then(Value::as_str).map(str::to_string),
                        qualification_code: item
                            .get("qualification_code")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        qualification_level_code: item
                            .get("qualification_level_code")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        assigned_at: Some(now),
                        check_in_time: None,
                        check_out_time: None,
                        is_active: true,
                        username: item.get("username").and_then(Value::as_str).map(str::to_string),
                    })
                })
                .collect::<Vec<_>>();
        }

        if persist_side_effects {
            if let Some(member_repo) = self.order.member_repo.as_ref() {
                for member in &order.members {
                    member_repo.save(member).await?;
                }
            }

            let equipment_ids = equipment_assignment
                .iter()
                .filter_map(|item| {
                    item.get("equipment_id")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string)
                })
                .collect::<Vec<_>>();
            if !equipment_ids.is_empty() {
                self.order
                    .order_repo
                    .replace_order_equipment_assignments(&order.id, &equipment_ids)
                    .await?;
            }
        }

        Ok(json!({
            "updated": true,
            "reason": Value::Null,
            "qualification_gap": [],
            "equipment_gap": [],
            "task_crew": task_crew,
            "equipment_assignment": equipment_assignment,
        }))
    }

    pub async fn prepare_order_for_publication(&self, order: &mut DispatchOrder) -> Result<Value, DomainError> {
        self.prepare_order_for_publication_internal(order, true).await
    }

    pub async fn publish_orders(
        &self,
        order_ids: Option<&[String]>,
        actor_id: &str,
        _at_time: Option<DateTime<Utc>>,
        _event_code: Option<&str>,
        flight_id: Option<&str>,
        limit: usize,
        skip_preparation: bool,
    ) -> Result<Value, DomainError> {
        Self::ensure_actor(actor_id)?;

        let orders = match order_ids {
            Some(ids) if !ids.is_empty() => {
                let mut result = Vec::new();
                for id in ids {
                    if let Some(order) = self.order.order_repo.find_by_id(id, true, None).await? {
                        result.push(order);
                    }
                }
                result
            }
            _ => self
                .order
                .order_repo
                .find_publishable_orders(Utc::now(), limit as i64)
                .await?
                .into_iter()
                .filter(|order| {
                    if let Some(fid) = flight_id {
                        if order.flight_id != fid {
                            return false;
                        }
                    }
                    true
                })
                .collect(),
        };

        let mut published_orders = Vec::new();
        let mut skipped_orders = Vec::new();

        for mut order in orders {
            if order.publication_state.trim() != "prepublished" {
                skipped_orders.push(json!({
                    "order_id": order.id,
                    "flight_id": order.flight_id,
                    "task_type": order.task_type,
                    "reason": "工单不是预发布状态",
                }));
                continue;
            }

            if !skip_preparation && Self::order_has_required_assignments(&order).is_err() {
                let prepared = self.prepare_order_for_publication(&mut order).await?;
                if prepared.get("updated").and_then(Value::as_bool).unwrap_or(false) {
                    self.order.order_repo.save(&order).await?;
                }
                if let Err(updated_reason) = Self::order_has_required_assignments(&order) {
                    let reason = prepared
                        .get("reason")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                        .unwrap_or(updated_reason);
                    skipped_orders.push(json!({
                        "order_id": order.id,
                        "flight_id": order.flight_id,
                        "task_type": order.task_type,
                        "reason": reason,
                    }));
                    continue;
                }
            }

            order.publication_state = "published".to_string();
            self.order.order_repo.save(&order).await?;
            self.sync_dispatch_chat_for_order(&order.id).await;
            self.send_publication_notifications(&order).await;

            published_orders.push(json!({
                "order_id": order.id,
                "flight_id": order.flight_id,
                "task_type": order.task_type,
                "publication_state": order.publication_state,
            }));
        }

        Ok(json!({
            "published_count": published_orders.len(),
            "published_orders": published_orders,
            "skipped_orders": skipped_orders,
        }))
    }

    pub async fn publish_order(&self, order_id: &str, actor_id: &str) -> Result<Value, DomainError> {
        let order_ids = vec![order_id.to_string()];
        self.publish_orders(Some(order_ids.as_slice()), actor_id, None, None, None, 1, true)
            .await
    }

    pub async fn publish_order_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        order_id: &str,
        actor_id: &str,
    ) -> Result<Value, DomainError> {
        Self::ensure_actor(actor_id)?;
        let mut order = self
            .order
            .order_repo
            .find_by_id(order_id, true, None)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "DispatchOrder",
                id: order_id.to_string(),
            })?;

        let mut published_orders = Vec::new();
        let mut skipped_orders = Vec::new();

        if order.publication_state.trim() != "prepublished" {
            skipped_orders.push(json!({
                "order_id": order.id,
                "flight_id": order.flight_id,
                "task_type": order.task_type,
                "reason": "工单不是预发布状态",
            }));
            return Ok(json!({
                "published_count": 0,
                "published_orders": published_orders,
                "skipped_orders": skipped_orders,
            }));
        }

        if Self::order_has_required_assignments(&order).is_err() {
            let prepared = self.prepare_order_for_publication(&mut order).await?;
            if prepared.get("updated").and_then(Value::as_bool).unwrap_or(false) {
                let order_tx_repo = self.order.order_tx_repo.as_ref().ok_or_else(|| {
                    DomainError::Internal(
                        "DispatchService order transactional repository is not configured".to_string(),
                    )
                })?;
                order_tx_repo.save_in_tx(tx, &order).await?;
            }
            if let Err(updated_reason) = Self::order_has_required_assignments(&order) {
                let reason = prepared
                    .get("reason")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or(updated_reason);
                skipped_orders.push(json!({
                    "order_id": order.id,
                    "flight_id": order.flight_id,
                    "task_type": order.task_type,
                    "reason": reason,
                }));
                return Ok(json!({
                    "published_count": 0,
                    "published_orders": published_orders,
                    "skipped_orders": skipped_orders,
                }));
            }
        }

        order.publication_state = "published".to_string();
        let order_tx_repo = self.order.order_tx_repo.as_ref().ok_or_else(|| {
            DomainError::Internal("DispatchService order transactional repository is not configured".to_string())
        })?;
        order_tx_repo.save_in_tx(tx, &order).await?;

        published_orders.push(json!({
            "order_id": order.id,
            "flight_id": order.flight_id,
            "task_type": order.task_type,
            "publication_state": order.publication_state,
        }));

        Ok(json!({
            "published_count": 1,
            "published_orders": published_orders,
            "skipped_orders": skipped_orders,
        }))
    }

    /// 按航班查询派工单
    pub async fn list_orders_by_flight(&self, flight_id: &str) -> Result<Vec<DispatchOrderResponse>, DomainError> {
        let orders = self.order.order_repo.find_by_flight(flight_id).await?;
        Ok(orders.iter().map(order_to_response).collect())
    }

    /// 查询派工单列表
    pub async fn list_orders(
        &self,
        status: Option<&str>,
        department: Option<&str>,
        page: i64,
        size: i64,
    ) -> Result<Vec<DispatchOrderResponse>, DomainError> {
        let offset = (page - 1) * size;
        let orders = self.order.order_repo.find_all(status, department, size, offset).await?;
        Ok(orders.iter().map(order_to_response).collect())
    }

    pub async fn on_domain_event(&self, event: &Value) -> Result<(), DomainError> {
        let event_type = event
            .get("event_type")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        if !event_type.starts_with("flight.") {
            return Ok(());
        }

        let flight_id = event
            .get("aggregate_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        if flight_id.is_empty() {
            return Ok(());
        }

        let pending_orders = self.order.order_repo.find_pending_for_flight(flight_id).await?;
        if pending_orders.is_empty() {
            return Ok(());
        }

        let empty_obj = serde_json::Value::Object(serde_json::Map::new());
        let details = json!({
            "event_id": event.get("event_id").unwrap_or(&NULL_VALUE),
            "event_type": event_type,
            "payload": event.get("payload").unwrap_or(&empty_obj),
            "source_change_id": event
                .get("source_change_id")
                .unwrap_or(&NULL_VALUE),
        });

        for order in pending_orders {
            self.order
                .order_repo
                .append_log(
                    &order.id,
                    "flight_context_changed",
                    Some("system:event-bus"),
                    Some(details.clone()),
                )
                .await?;
        }

        Ok(())
    }

    pub async fn save_event_generated_order(
        &self,
        order: &DispatchOrder,
        log_details: Value,
    ) -> Result<(), DomainError> {
        self.order.order_repo.save(order).await?;
        self.order
            .order_repo
            .append_log(
                &order.id,
                "event_rule_generated",
                Some("system:event-rules"),
                Some(log_details),
            )
            .await?;
        Ok(())
    }

    pub async fn save_event_generated_order_once(
        &self,
        order: &DispatchOrder,
        log_details: Value,
    ) -> Result<bool, DomainError> {
        if self.has_existing_event_generated_order(order).await? {
            return Ok(false);
        }

        self.save_event_generated_order(order, log_details).await?;
        Ok(true)
    }

    async fn has_existing_event_generated_order(&self, order: &DispatchOrder) -> Result<bool, DomainError> {
        let Some(rule_id) = order.generation_rule_id.as_deref() else {
            return Ok(false);
        };

        let existing_orders = self.order.order_repo.find_by_flight(&order.flight_id).await?;
        Ok(existing_orders.iter().any(|existing| {
            existing.id != order.id
                && existing.status != DispatchOrderStatus::Cancelled
                && existing.source_type == "event_generated"
                && existing.task_type == order.task_type
                && existing.generation_rule_id.as_deref() == Some(rule_id)
                && existing.department_id == order.department_id
        }))
    }

    /// 派工验证 — 检查时间窗口冲突
    pub async fn validate_order(&self, dto: ValidateOrderRequest) -> Result<serde_json::Value, DomainError> {
        let mut errors: Vec<serde_json::Value> = Vec::new();
        let mut conflicts = Vec::<Value>::new();
        let start = dto.planned_start_time;
        let end = dto.planned_end_time;

        if end <= start {
            errors.push(serde_json::json!({
                "field": "planned_end_time",
                "message": "planned_end_time 必须晚于 planned_start_time"
            }));
        } else {
            let exclude_order_id = dto
                .dispatch_order_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let orders = self
                .order
                .order_repo
                .find_orders_in_window(start, end, &Self::ACTIVE_CONFLICT_STATUSES, None, None, None, false)
                .await?;

            let requested_team_id = dto.team_id.as_deref().map(str::trim).filter(|value| !value.is_empty());
            let requested_user_id = dto
                .individual_user_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let requested_stand_id = dto.stand_id.as_deref().map(str::trim).filter(|value| !value.is_empty());
            let requested_equipment_ids = dto
                .equipment_ids
                .iter()
                .map(|item| item.trim())
                .filter(|item| !item.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>();

            for order in orders {
                if exclude_order_id == Some(order.id.as_str()) {
                    continue;
                }
                let (order_start, order_end) = Self::effective_interval(&order, start);
                if order_end < start || end < order_start {
                    continue;
                }

                if let Some(team_id) = requested_team_id {
                    if order.team_id.as_deref() == Some(team_id) {
                        conflicts.push(Self::build_conflict(
                            "team_overlap",
                            "high",
                            Some(team_id.to_string()),
                            order.team_name.clone(),
                            vec![order.id.clone()],
                            "班组在目标时间段已被占用",
                            Some("更换班组或调整任务时间"),
                            json!({ "conflict_order_status": order.status.as_ref() }),
                        ));
                    }
                }

                if let Some(user_id) = requested_user_id {
                    let matched_user_ids = Self::order_member_user_ids(&order)
                        .into_iter()
                        .filter(|candidate| candidate == user_id)
                        .collect::<Vec<_>>();
                    if !matched_user_ids.is_empty() {
                        conflicts.push(Self::build_conflict(
                            "individual_overlap",
                            "high",
                            Some(user_id.to_string()),
                            order.individual_username.clone(),
                            vec![order.id.clone()],
                            "人员在目标时间段已有派工任务",
                            Some("更换人员或调整任务时间"),
                            json!({
                                "conflict_order_status": format!("{:?}", order.status).to_lowercase(),
                                "matched_user_ids": matched_user_ids,
                            }),
                        ));
                    }
                }

                if let Some(stand_id) = requested_stand_id {
                    if order.stand_id.as_deref() == Some(stand_id) {
                        conflicts.push(Self::build_conflict(
                            "stand_overlap",
                            "medium",
                            Some(stand_id.to_string()),
                            order.stand_code.clone(),
                            vec![order.id.clone()],
                            "机位同时间段存在保障任务重叠",
                            Some("确认机位占用计划并调整时间窗口"),
                            json!({ "conflict_order_status": order.status.as_ref() }),
                        ));
                    }
                }
            }

            if !requested_equipment_ids.is_empty() {
                for row in self
                    .order
                    .order_repo
                    .find_equipment_conflicts(&requested_equipment_ids, start, end, exclude_order_id)
                    .await?
                {
                    let related_order_id = row
                        .get("dispatch_order_id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    conflicts.push(Self::build_conflict(
                        "equipment_overlap",
                        "high",
                        row.get("equipment_id").and_then(Value::as_str).map(ToString::to_string),
                        None,
                        if related_order_id.is_empty() {
                            Vec::new()
                        } else {
                            vec![related_order_id]
                        },
                        "设备在目标时间段已被占用",
                        Some("更换设备或调整任务时间"),
                        json!({}),
                    ));
                }
            }
        }

        conflicts = Self::deduplicate_conflicts(conflicts);
        for conflict in &conflicts {
            let conflict_order_id = conflict
                .get("related_dispatch_order_ids")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .and_then(Value::as_str)
                .unwrap_or_default();
            errors.push(json!({
                "field": "time_window",
                "message": conflict.get("message").and_then(Value::as_str).unwrap_or("派工存在冲突"),
                "conflict_order_id": if conflict_order_id.is_empty() { Value::Null } else { json!(conflict_order_id) },
                "conflict_type": conflict.get("conflict_type").unwrap_or(&NULL_VALUE),
                "severity": conflict.get("severity").unwrap_or(&NULL_VALUE),
            }));
        }

        Ok(serde_json::json!({
            "valid": errors.is_empty(),
            "has_conflicts": !conflicts.is_empty(),
            "conflict_count": conflicts.len(),
            "conflicts": conflicts,
            "errors": errors,
        }))
    }

    pub async fn validate_order_conflicts_only(
        &self,
        dto: ValidateOrderRequest,
    ) -> Result<serde_json::Value, DomainError> {
        let result = self.validate_order(dto).await?;
        Ok(serde_json::json!({
            "has_conflicts": result
                .get("has_conflicts")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            "conflict_count": result
                .get("conflict_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            "conflicts": result
                .get("conflicts")
                .cloned()
                .unwrap_or_else(|| serde_json::json!([])),
        }))
    }
}
