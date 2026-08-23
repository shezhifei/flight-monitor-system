use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::*;
use fms_domain::models::flight::Flight;
use fms_domain::models::flight_leg::FlightTypeCode;
use fms_domain::ports::dispatch_repository::CreateDispatchOrderCommand;

use crate::schemas::dispatch_schemas::*;

use super::helpers;
use super::{DispatchService, GeneratedFlightDispatchRequest, PreparedWindowOrder, ReplanExecutionResult, NULL_VALUE};

fn ensure_publishable_draft_state(
    order_id: &str,
    status: DispatchOrderStatus,
    publication_state: &str,
) -> Result<(), DomainError> {
    if status == DispatchOrderStatus::Pending && publication_state == "prepublished" {
        return Ok(());
    }
    Err(DomainError::BusinessRuleViolation(format!(
        "派工单 {order_id} 不是待发布草稿，当前状态为 {status:?}/{publication_state}"
    )))
}

impl DispatchService {
    pub async fn auto_dispatch(
        &self,
        flight_id: &str,
        task_type: &str,
        stand_id: &str,
        planned_start_time: chrono::DateTime<Utc>,
        planned_end_time: Option<chrono::DateTime<Utc>>,
        terminal: Option<&str>,
        department_id: Option<&str>,
    ) -> Result<serde_json::Value, DomainError> {
        let stand_repo = &self.resources.stand_repo;
        let task_type_repo = &self.rules.task_type_repo;
        let Some(stand) = stand_repo.find_by_id(stand_id).await? else {
            return Ok(json!({
                "success": false,
                "message": format!("机位不存在: {stand_id}"),
            }));
        };

        let Some(task_type) = task_type_repo.find_by_code(task_type).await? else {
            return Ok(json!({
                "success": false,
                "message": format!("作业类型不存在: {task_type}"),
            }));
        };
        let order_id = ulid::Ulid::new().to_string();
        let now = Utc::now();
        let resolved_planned_end_time = planned_end_time.unwrap_or_else(|| {
            planned_start_time + chrono::Duration::minutes(task_type.default_duration_minutes.unwrap_or(15) as i64)
        });
        let mut order = DispatchOrder {
            id: order_id.clone(),
            flight_id: flight_id.to_string(),
            task_type: task_type.code.clone(),
            stand_id: Some(stand_id.to_string()),
            task_type_name: None,
            stand_code: Some(stand.code.clone()),
            terminal: terminal
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .or_else(|| stand.terminal.clone()),
            assignee_type: AssigneeType::Team,
            team_id: None,
            team_name: None,
            department: None,
            individual_user_id: None,
            individual_username: None,
            driver_type: None,
            driver_team_id: None,
            driver_user_id: None,
            planned_start_time: Some(planned_start_time),
            planned_end_time: Some(resolved_planned_end_time),
            actual_start_time: None,
            actual_end_time: None,
            estimated_completion_time: None,
            estimated_completion_reported_by: None,
            estimated_completion_reported_at: None,
            estimated_completion_note: None,
            status: DispatchOrderStatus::Pending,
            dispatch_type: DispatchType::Auto,
            dispatched_by: None,
            dispatched_at: None,
            snapshot_assignee_position: None,
            snapshot_equipment_positions: None,
            estimated_arrival_minutes: None,
            process_instance_id: None,
            process_task_id: None,
            workflow_context: Default::default(),
            workflow_status: "pending_assignment".into(),
            source: "auto".into(),
            schedule_source: ScheduleSource::CurrentStatusFallback,
            lock_level: DispatchLockLevel::Optimizable,
            publication_state: "prepublished".into(),
            source_type: "generated".into(),
            department_id: department_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .or_else(|| task_type.default_department_id.clone()),
            leg_scope: "none".into(),
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
            department_rule_version: None,
            crew_requirement_snapshot: vec![],
            equipment_requirement_snapshot: vec![],
            task_crew: Default::default(),
            equipment_assignment: vec![],
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

        let prepared = match self.prepare_order_for_publication_internal(&mut order, false).await {
            Ok(prepared) => prepared,
            Err(error) => {
                let alert = self
                    .create_dispatch_alert(
                        flight_id,
                        task_type.code.as_str(),
                        "dispatch_error",
                        format!("派工系统错误: {error}"),
                        AlertSeverity::Critical,
                    )
                    .await?;
                return Ok(json!({
                    "success": false,
                    "message": format!("派工失败: {error}"),
                    "alert": alert.as_ref().map(Self::alert_to_json),
                }));
            }
        };
        if !prepared.get("updated").and_then(Value::as_bool).unwrap_or(false) {
            let qualification_gap = prepared
                .get("qualification_gap")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let equipment_gap = prepared
                .get("equipment_gap")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let reason = prepared
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("当前无法自动分派");
            let (alert_type, alert_message) = if qualification_gap.iter().any(|item| {
                item.get("reason")
                    .and_then(Value::as_str)
                    .map(|value| value == "missing_department_rule_context")
                    .unwrap_or(false)
            }) {
                (
                    "missing_department_rule_context",
                    format!(
                        "航班 {flight_id} 作业类型 {} 缺少科室归属，无法执行个人级派工",
                        task_type.code
                    ),
                )
            } else if reason.contains("缺少已发布资质规则") {
                (
                    "missing_published_rule",
                    format!(
                        "航班 {flight_id} 作业类型 {} 缺少已发布资质规则，无法执行个人级派工",
                        task_type.code
                    ),
                )
            } else if !qualification_gap.is_empty() {
                (
                    "qualification_crew_unavailable",
                    format!(
                        "航班 {flight_id} 作业类型 {} 无法组出满足资质要求的执行编组",
                        task_type.code
                    ),
                )
            } else if !equipment_gap.is_empty() {
                (
                    "equipment_unavailable",
                    format!("航班 {flight_id} 作业类型 {} 无法补齐设备分配", task_type.code),
                )
            } else {
                (
                    "dispatch_error",
                    format!("航班 {flight_id} 作业类型 {} 自动派工失败: {reason}", task_type.code),
                )
            };
            let alert = self
                .create_dispatch_alert(
                    flight_id,
                    task_type.code.as_str(),
                    alert_type,
                    alert_message,
                    AlertSeverity::Warning,
                )
                .await?;
            return Ok(serde_json::json!({
                "success": false,
                "message": reason,
                "qualification_gap": qualification_gap,
                "equipment_gap": equipment_gap,
                "alert": alert.as_ref().map(Self::alert_to_json),
            }));
        }

        order.created_at = Some(now);
        order.updated_at = Some(Utc::now());
        order.dispatched_by = Some("system".to_string());
        order.dispatched_at = order.dispatched_at.or(Some(Utc::now()));
        self.order.order_repo.save(&order).await?;
        if let Some(member_repo) = self.order.member_repo.as_ref() {
            for member in &order.members {
                member_repo.save(member).await?;
            }
        }
        self.order
            .order_repo
            .replace_order_equipment_assignments(
                &order_id,
                &order
                    .equipment_assignment
                    .iter()
                    .filter_map(|item| item.get("equipment_id").and_then(Value::as_str))
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>(),
            )
            .await?;

        self.sync_dispatch_chat_for_order(&order_id).await;

        Ok(serde_json::json!({
            "success": true,
            "message": "派工成功",
            "dispatch_order_id": order_id,
            "task_crew": order.task_crew,
            "equipment_assignment": order.equipment_assignment,
        }))
    }

    /// 为航班批量派工所有作业类型
    pub async fn batch_dispatch_for_flight(
        &self,
        flight_id: &str,
        stand_id: &str,
        eta: chrono::DateTime<Utc>,
        etd: chrono::DateTime<Utc>,
        terminal: Option<&str>,
    ) -> Result<serde_json::Value, DomainError> {
        let drafted_orders = self
            .generate_draft_orders(flight_id, stand_id, eta, etd, terminal)
            .await?;
        let results = drafted_orders
            .iter()
            .map(|order| {
                serde_json::json!({
                    "task_type": order.task_type,
                    "success": true,
                    "message": "草稿工单已生成（未分配人员）",
                    "order_id": order.id,
                })
            })
            .collect::<Vec<_>>();

        Ok(serde_json::json!({
            "success": true,
            "total": drafted_orders.len(),
            "succeeded": drafted_orders.len(),
            "failed": 0,
            "results": results,
        }))
    }

    /// 根据航班上下文生成草稿工单（不分配人员）。
    pub async fn generate_draft_orders(
        &self,
        flight_id: &str,
        stand_id: &str,
        eta: DateTime<Utc>,
        etd: DateTime<Utc>,
        terminal: Option<&str>,
    ) -> Result<Vec<DispatchOrder>, DomainError> {
        let stand_repo = &self.resources.stand_repo;
        let stand = stand_repo
            .find_by_id(stand_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "Stand",
                id: stand_id.to_string(),
            })?;
        let requests = self
            .build_generation_requests(flight_id, stand_id, eta, etd, terminal)
            .await?;
        let mut drafted_orders = Vec::new();
        let mut existing_orders = self.order.order_repo.find_by_flight(flight_id).await?;

        for request in requests {
            if let Some(existing) = existing_orders.iter_mut().find(|order| {
                order.generation_rule_id.as_deref() == Some(request.generation_rule_id.as_str())
                    && order.generation_rule_version == Some(request.generation_rule_version)
                    && order.leg_scope == request.leg_scope
                    && order.status != DispatchOrderStatus::Cancelled
            }) {
                if existing.status == DispatchOrderStatus::Pending && existing.publication_state == "prepublished" {
                    existing.stand_id = Some(request.stand_id.clone());
                    existing.stand_code = Some(stand.code.clone());
                    existing.terminal = request.terminal.clone().or_else(|| stand.terminal.clone());
                    existing.planned_start_time = Some(request.planned_start_time);
                    existing.planned_end_time = Some(request.planned_end_time);
                    existing.generation_anchor_type = Some(request.generation_anchor_type.clone());
                    existing.generation_anchor_time = Some(request.generation_anchor_time);
                    existing.completion_time_mode = Some(request.completion_time_mode.clone());
                    existing.completion_anchor_type = request.completion_anchor_type.clone();
                    existing.completion_anchor_time = request.completion_anchor_time;
                    existing.completion_offset_minutes = request.completion_offset_minutes;
                    existing.completion_warning_lead_minutes = request.completion_warning_lead_minutes;
                    existing.publish_trigger_mode = Some(request.publish_trigger_mode.clone());
                    existing.publish_at = request.publish_at;
                    existing.turnaround_pair_key = request.turnaround_pair_key.clone();
                    existing.turnaround_constraint_mode = request.turnaround_constraint_mode.clone();
                    existing.department_rule_version = Some(request.department_rule_version.clone());
                    existing.crew_requirement_snapshot = request.crew_requirement_snapshot.clone();
                    existing.equipment_requirement_snapshot = request.equipment_requirement_snapshot.clone();
                    existing.updated_at = Some(Utc::now());
                    self.order.order_repo.save(existing).await?;
                    drafted_orders.push(existing.clone());
                }
                continue;
            }
            let mut order = DispatchOrder {
                id: ulid::Ulid::new().to_string(),
                flight_id: flight_id.to_string(),
                task_type: request.task_type,
                task_type_name: None,
                stand_id: Some(request.stand_id),
                stand_code: Some(stand.code.clone()),
                terminal: request.terminal.or_else(|| stand.terminal.clone()),
                assignee_type: AssigneeType::Team,
                team_id: None,
                team_name: None,
                department: None,
                individual_user_id: None,
                individual_username: None,
                driver_type: None,
                driver_team_id: None,
                driver_user_id: None,
                planned_start_time: Some(request.planned_start_time),
                planned_end_time: Some(request.planned_end_time),
                actual_start_time: None,
                actual_end_time: None,
                estimated_completion_time: None,
                estimated_completion_reported_by: None,
                estimated_completion_reported_at: None,
                estimated_completion_note: None,
                status: DispatchOrderStatus::Pending,
                dispatch_type: DispatchType::Auto,
                dispatched_at: None,
                dispatched_by: None,
                snapshot_assignee_position: None,
                snapshot_equipment_positions: None,
                estimated_arrival_minutes: None,
                process_instance_id: None,
                process_task_id: None,
                workflow_context: serde_json::Value::Object(Default::default()),
                workflow_status: "pending_assignment".to_string(),
                source: "system".to_string(),
                schedule_source: ScheduleSource::CurrentStatusFallback,
                lock_level: DispatchLockLevel::Optimizable,
                publication_state: "prepublished".to_string(),
                source_type: request.source_type,
                department_id: Some(request.department_id),
                leg_scope: request.leg_scope,
                generation_rule_id: Some(request.generation_rule_id),
                generation_rule_version: Some(request.generation_rule_version),
                generation_anchor_type: Some(request.generation_anchor_type),
                generation_anchor_time: Some(request.generation_anchor_time),
                completion_time_mode: Some(request.completion_time_mode),
                completion_anchor_type: request.completion_anchor_type,
                completion_anchor_time: request.completion_anchor_time,
                completion_offset_minutes: request.completion_offset_minutes,
                completion_warning_lead_minutes: request.completion_warning_lead_minutes,
                publish_trigger_mode: Some(request.publish_trigger_mode),
                publish_at: request.publish_at,
                turnaround_pair_key: request.turnaround_pair_key,
                turnaround_constraint_mode: request.turnaround_constraint_mode,
                department_rule_version: Some(request.department_rule_version),
                crew_requirement_snapshot: request.crew_requirement_snapshot,
                equipment_requirement_snapshot: request.equipment_requirement_snapshot,
                task_crew: serde_json::Value::Object(Default::default()),
                equipment_assignment: Vec::new(),
                qualification_gap: Vec::new(),
                equipment_gap: Vec::new(),
                availability_reason: None,
                score_breakdown: serde_json::Value::Object(Default::default()),
                conflict_reason: None,
                recommended_assignees: Vec::new(),
                recommendation_score: None,
                supervisor_notified: false,
                supervisor_notified_at: None,
                assignment_deadline: None,
                completed_by: None,
                completion_notes: None,
                gate: None,
                created_at: None,
                updated_at: None,
                members: Vec::new(),
                equipment_list: Vec::new(),
            };
            order.updated_at = Some(Utc::now());
            self.order.order_repo.save(&order).await?;
            drafted_orders.push(order);
        }

        Ok(drafted_orders)
    }

    /// Recalculate generated, unpublished orders after a flight projection changes.
    ///
    /// The method is deliberately idempotent: `generate_draft_orders` reuses the
    /// existing rule/version/leg draft instead of creating another order.
    pub async fn rebase_pending_generated_orders_for_flight(&self, flight_id: &str) -> Result<usize, DomainError> {
        let flight_id = flight_id.trim();
        if flight_id.is_empty() {
            return Ok(0);
        }
        let existing = self.order.order_repo.find_by_flight(flight_id).await?;
        let pending_generated = existing
            .iter()
            .filter(|order| {
                order.status == DispatchOrderStatus::Pending
                    && order.publication_state == "prepublished"
                    && order.generation_rule_id.is_some()
            })
            .collect::<Vec<_>>();
        if pending_generated.is_empty() {
            return Ok(0);
        }

        let flight_repo = &self.rules.flight_repo;
        let flight = flight_repo
            .find_by_id(flight_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "Flight",
                id: flight_id.to_string(),
            })?;
        let eta = flight
            .estimated_arrival
            .or(flight.scheduled_arrival)
            .or(flight.actual_arrival)
            .or_else(|| {
                pending_generated
                    .iter()
                    .find(|order| order.leg_scope == "inbound")
                    .and_then(|order| order.generation_anchor_time)
            });
        let etd = flight
            .estimated_departure
            .or(flight.scheduled_departure)
            .or(flight.actual_departure)
            .or_else(|| {
                pending_generated
                    .iter()
                    .find(|order| order.leg_scope == "outbound")
                    .and_then(|order| order.generation_anchor_time)
            });
        let (eta, etd) = match (eta, etd) {
            (Some(eta), Some(etd)) => (eta, etd),
            (Some(eta), None) => (eta, eta),
            (None, Some(etd)) => (etd, etd),
            (None, None) => {
                tracing::warn!(
                    flight_id,
                    "pending generated dispatch orders were not rebased because no flight anchor time is available"
                );
                return Ok(0);
            }
        };
        let stand_ids = pending_generated
            .iter()
            .filter_map(|order| order.stand_id.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect::<HashSet<_>>();
        let mut rebased_ids = HashSet::new();
        for stand_id in stand_ids {
            for order in self
                .generate_draft_orders(flight_id, &stand_id, eta, etd, flight.terminal.as_deref())
                .await?
            {
                rebased_ids.insert(order.id);
            }
        }
        Ok(rebased_ids.len())
    }

    /// 批量发布草稿工单：写入人员编组和设备分配。
    pub async fn batch_publish_draft_orders(
        &self,
        assignments: &[Value],
        published_by: &str,
    ) -> Result<Vec<DispatchOrder>, DomainError> {
        let mut published_orders = Vec::new();
        let mut commands = Vec::new();
        let mut seen_order_ids = HashSet::new();
        let mut batch_user_bookings = HashMap::<String, Vec<(DateTime<Utc>, DateTime<Utc>, String)>>::new();
        let mut batch_equipment_bookings = HashMap::<String, Vec<(DateTime<Utc>, DateTime<Utc>, String)>>::new();

        for item in assignments {
            let order_id = item.get("order_id").and_then(Value::as_str).unwrap_or("").trim();
            if order_id.is_empty() {
                return Err(DomainError::ValidationError("order_id 不能为空".to_string()));
            }
            if !seen_order_ids.insert(order_id.to_string()) {
                return Err(DomainError::ValidationError(format!(
                    "派工单 {order_id} 在批量发布请求中重复出现"
                )));
            }

            let Some(mut order) = self.order.order_repo.find_by_id(order_id, true, None).await? else {
                return Err(DomainError::NotFound {
                    entity_type: "DispatchOrder",
                    id: order_id.to_string(),
                });
            };

            ensure_publishable_draft_state(&order.id, order.status, &order.publication_state)?;

            order.publication_state = "published".to_string();
            order.task_crew = item
                .get("task_crew")
                .and_then(Value::as_object)
                .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default();
            order.equipment_assignment = item
                .get("equipment_assignment")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let members = Self::validate_draft_publication_assignment(&order)?;
            let equipment_ids = Self::validate_draft_publication_equipment(&order)?;
            self.validate_draft_publication_resources(&order, &members, &equipment_ids)
                .await?;
            let planned_start = order
                .planned_start_time
                .ok_or_else(|| DomainError::ValidationError(format!("派工单 {} 缺少计划开始时间", order.id)))?;
            let planned_end = order
                .planned_end_time
                .ok_or_else(|| DomainError::ValidationError(format!("派工单 {} 缺少计划完成时间", order.id)))?;
            for member in &members {
                let bookings = batch_user_bookings.entry(member.user_id.clone()).or_default();
                if let Some((_, _, conflicting_order_id)) = bookings
                    .iter()
                    .find(|(start, end, _)| planned_start < *end && *start < planned_end)
                {
                    return Err(DomainError::BusinessRuleViolation(format!(
                        "批量发布中的派工单 {} 与 {} 重复占用人员 {}",
                        order.id, conflicting_order_id, member.user_id
                    )));
                }
                bookings.push((planned_start, planned_end, order.id.clone()));
            }
            for equipment_id in &equipment_ids {
                let bookings = batch_equipment_bookings.entry(equipment_id.clone()).or_default();
                if let Some((_, _, conflicting_order_id)) = bookings
                    .iter()
                    .find(|(start, end, _)| planned_start < *end && *start < planned_end)
                {
                    return Err(DomainError::BusinessRuleViolation(format!(
                        "批量发布中的派工单 {} 与 {} 重复占用设备 {}",
                        order.id, conflicting_order_id, equipment_id
                    )));
                }
                bookings.push((planned_start, planned_end, order.id.clone()));
            }
            order.status = DispatchOrderStatus::Assigned;
            order.dispatched_at = Some(Utc::now());
            order.dispatched_by = Some(published_by.to_string());
            order.updated_at = Some(Utc::now());

            commands.push(CreateDispatchOrderCommand {
                order: order.clone(),
                members,
                persist_equipment_assignments: true,
                equipment_ids,
                log_action: "draft_published".to_string(),
                log_actor_id: Some(published_by.to_string()),
                log_details: Some(json!({ "published_by": published_by })),
            });
            published_orders.push(order);
        }

        self.order.order_repo.save_orders_atomic(commands).await?;
        for order in &published_orders {
            self.sync_dispatch_chat_for_order(&order.id).await;
        }

        Ok(published_orders)
    }

    async fn validate_draft_publication_resources(
        &self,
        order: &DispatchOrder,
        members: &[DispatchOrderMember],
        equipment_ids: &[String],
    ) -> Result<(), DomainError> {
        let planned_start = order
            .planned_start_time
            .ok_or_else(|| DomainError::ValidationError(format!("派工单 {} 缺少计划开始时间", order.id)))?;
        let planned_end = order
            .planned_end_time
            .ok_or_else(|| DomainError::ValidationError(format!("派工单 {} 缺少计划完成时间", order.id)))?;
        if planned_end <= planned_start {
            return Err(DomainError::ValidationError(format!(
                "派工单 {} 的计划完成时间必须晚于计划开始时间",
                order.id
            )));
        }

        let user_ids = members.iter().map(|member| member.user_id.clone()).collect::<Vec<_>>();
        if !order.crew_requirement_snapshot.is_empty() {
            let department_id = order
                .department_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| DomainError::ValidationError(format!("派工单 {} 缺少部门上下文", order.id)))?;
            let grant_repo = &self.resources.qualification_grant_repo;
            let qualification_repo = &self.resources.qualification_repo;
            let grants = grant_repo
                .find_by_department(department_id, Some(planned_start), &user_ids, false)
                .await?;
            let levels = qualification_repo.list_levels(department_id, None, false).await?;
            let level_index = levels
                .into_iter()
                .map(|level| {
                    let mut covered = level.covered_level_codes.into_iter().collect::<HashSet<_>>();
                    covered.insert(level.level_code.clone());
                    (level.level_code, covered)
                })
                .collect::<HashMap<_, _>>();

            for member in members {
                let slot_code = member.slot_code.as_deref().unwrap_or_default();
                let requirement = order
                    .crew_requirement_snapshot
                    .iter()
                    .find(|item| item.get("slot_code").and_then(Value::as_str) == Some(slot_code))
                    .ok_or_else(|| DomainError::ValidationError(format!("人员槽位 {slot_code} 不在规则快照中")))?;
                let qualification_code = requirement
                    .get("qualification_code")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let min_level_code = requirement.get("min_level_code").and_then(Value::as_str);
                let qualified = grants.iter().any(|grant| {
                    grant.user_id == member.user_id
                        && grant.qualification_code == qualification_code
                        && grant.valid_to.is_none_or(|valid_to| valid_to >= planned_end)
                        && Self::level_covers_requirement(&level_index, &grant.level_code, min_level_code)
                });
                if !qualified {
                    return Err(DomainError::BusinessRuleViolation(format!(
                        "人员 {} 不满足槽位 {} 的有效资质要求",
                        member.user_id, slot_code
                    )));
                }
            }
        }

        let overlapping_orders = self
            .order
            .order_repo
            .find_orders_in_window(
                planned_start,
                planned_end,
                &Self::ACTIVE_CONFLICT_STATUSES,
                None,
                None,
                order.terminal.as_deref(),
                false,
            )
            .await?;
        let requested_users = user_ids.into_iter().collect::<HashSet<_>>();
        for conflict in overlapping_orders
            .into_iter()
            .filter(|candidate| candidate.id != order.id)
        {
            if Self::order_member_user_ids(&conflict)
                .into_iter()
                .any(|user_id| requested_users.contains(&user_id))
            {
                return Err(DomainError::BusinessRuleViolation(format!(
                    "派工单 {} 的人员与工单 {} 存在时间冲突",
                    order.id, conflict.id
                )));
            }
        }

        if !equipment_ids.is_empty() {
            let equipment_repo = &self.resources.equipment_repo;
            for assignment in &order.equipment_assignment {
                let equipment_id = assignment
                    .get("equipment_id")
                    .and_then(Value::as_str)
                    .or_else(|| assignment.as_str())
                    .unwrap_or_default();
                let slot_code = assignment.get("slot_code").and_then(Value::as_str).unwrap_or_default();
                let requirement = order
                    .equipment_requirement_snapshot
                    .iter()
                    .find(|item| item.get("slot_code").and_then(Value::as_str) == Some(slot_code))
                    .ok_or_else(|| DomainError::ValidationError(format!("设备槽位 {slot_code} 不在规则快照中")))?;
                let equipment = equipment_repo
                    .find_by_id(equipment_id)
                    .await?
                    .ok_or_else(|| DomainError::ValidationError(format!("设备 {equipment_id} 不存在")))?;
                if !equipment.is_active
                    || (!equipment.is_available()
                        && equipment.current_dispatch_id.as_deref() != Some(order.id.as_str()))
                {
                    return Err(DomainError::BusinessRuleViolation(format!(
                        "设备 {equipment_id} 当前不可用"
                    )));
                }
                let required_type_id = requirement.get("equipment_type_id").and_then(Value::as_str);
                let required_type_code = requirement.get("equipment_type_code").and_then(Value::as_str);
                let type_matches = required_type_id
                    .map(|required| equipment.equipment_type_id.as_deref() == Some(required))
                    .or_else(|| {
                        required_type_code.map(|required| {
                            equipment.equipment_type.as_ref().and_then(|item| item.code.as_deref()) == Some(required)
                        })
                    })
                    .unwrap_or(true);
                if !type_matches {
                    return Err(DomainError::BusinessRuleViolation(format!(
                        "设备 {equipment_id} 不符合槽位 {slot_code} 的设备类型要求"
                    )));
                }
            }
            let conflicts = self
                .order
                .order_repo
                .find_equipment_conflicts(equipment_ids, planned_start, planned_end, Some(&order.id))
                .await?;
            if !conflicts.is_empty() {
                return Err(DomainError::BusinessRuleViolation(format!(
                    "派工单 {} 存在设备时间冲突",
                    order.id
                )));
            }
        }

        Ok(())
    }

    fn validate_draft_publication_assignment(order: &DispatchOrder) -> Result<Vec<DispatchOrderMember>, DomainError> {
        let member_payloads = order
            .task_crew
            .get("members")
            .and_then(Value::as_array)
            .ok_or_else(|| DomainError::ValidationError("发布草稿必须提供 task_crew.members".to_string()))?;
        let team_reference_id = order
            .task_crew
            .get("source_team_ids")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| order.team_id.clone());
        let mut seen_users = HashSet::new();
        let mut counts = HashMap::<String, usize>::new();
        let mut members = Vec::new();
        for payload in member_payloads {
            let user_id = payload
                .get("user_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| DomainError::ValidationError("task_crew.members.user_id 不能为空".to_string()))?;
            if !seen_users.insert(user_id.to_string()) {
                return Err(DomainError::ValidationError(format!("人员 {user_id} 被重复分配")));
            }
            let slot_code = payload
                .get("slot_code")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| DomainError::ValidationError(format!("人员 {user_id} 缺少 slot_code")))?;
            *counts.entry(slot_code.to_string()).or_default() += 1;
            members.push(DispatchOrderMember {
                id: ulid::Ulid::new().to_string(),
                dispatch_order_id: order.id.clone(),
                user_id: user_id.to_string(),
                role: MemberRole::Member,
                source_type: AssigneeType::Team,
                source_team_id: payload
                    .get("source_team_id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .or_else(|| team_reference_id.clone()),
                slot_code: Some(slot_code.to_string()),
                qualification_code: payload
                    .get("qualification_code")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                qualification_level_code: payload
                    .get("qualification_level_code")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                assigned_at: Some(Utc::now()),
                check_in_time: None,
                check_out_time: None,
                is_active: true,
                username: payload.get("username").and_then(Value::as_str).map(str::to_string),
            });
        }
        for requirement in &order.crew_requirement_snapshot {
            let Some(slot_code) = requirement.get("slot_code").and_then(Value::as_str) else {
                continue;
            };
            let required = requirement.get("required_count").and_then(Value::as_u64).unwrap_or(1) as usize;
            let actual = counts.get(slot_code).copied().unwrap_or(0);
            if actual != required {
                return Err(DomainError::ValidationError(format!(
                    "人员槽位 {slot_code} 需要 {required} 人，实际 {actual} 人"
                )));
            }
        }
        Ok(members)
    }

    fn validate_draft_publication_equipment(order: &DispatchOrder) -> Result<Vec<String>, DomainError> {
        let mut ids = Vec::new();
        let mut seen = HashSet::new();
        let mut counts = HashMap::<String, usize>::new();
        for payload in &order.equipment_assignment {
            let equipment_id = payload
                .as_str()
                .or_else(|| payload.get("equipment_id").and_then(Value::as_str))
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    DomainError::ValidationError("equipment_assignment.equipment_id 不能为空".to_string())
                })?;
            if !seen.insert(equipment_id.to_string()) {
                return Err(DomainError::ValidationError(format!("设备 {equipment_id} 被重复分配")));
            }
            let slot_code = payload
                .get("slot_code")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| DomainError::ValidationError(format!("设备 {equipment_id} 缺少 slot_code")))?;
            *counts.entry(slot_code.to_string()).or_default() += 1;
            ids.push(equipment_id.to_string());
        }
        for requirement in &order.equipment_requirement_snapshot {
            let Some(slot_code) = requirement.get("slot_code").and_then(Value::as_str) else {
                continue;
            };
            let required = requirement.get("required_count").and_then(Value::as_u64).unwrap_or(1) as usize;
            let actual = counts.get(slot_code).copied().unwrap_or(0);
            if actual != required {
                return Err(DomainError::ValidationError(format!(
                    "设备槽位 {slot_code} 需要 {required} 台，实际 {actual} 台"
                )));
            }
        }
        Ok(ids)
    }

    /// 全局最优批量派工的 Rust 近似实现
    pub async fn optimal_batch_dispatch(
        &self,
        flight_id: Option<&str>,
        stand_id: Option<&str>,
        eta: Option<DateTime<Utc>>,
        etd: Option<DateTime<Utc>>,
        terminal: Option<&str>,
        _time_limit_seconds: f64,
        scope: &str,
        window_start: Option<DateTime<Utc>>,
        window_end: Option<DateTime<Utc>>,
        freeze_order_ids: &[String],
        lock_policy: Option<&str>,
    ) -> Result<serde_json::Value, DomainError> {
        let started_at = Instant::now();
        let response_lock_policy = lock_policy
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("default")
            .to_string();
        let explicit_frozen_ids = freeze_order_ids
            .iter()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect::<HashSet<_>>();
        match scope.trim() {
            "window" => {
                let window_start = window_start.ok_or_else(|| {
                    DomainError::BusinessRuleViolation("window scope 需要 window_start/window_end".to_string())
                })?;
                let window_end = window_end.ok_or_else(|| {
                    DomainError::BusinessRuleViolation("window scope 需要 window_start/window_end".to_string())
                })?;
                if window_end <= window_start {
                    return Err(DomainError::BusinessRuleViolation(
                        "window_end 必须晚于 window_start".to_string(),
                    ));
                }
                let auto_freeze_before = Utc::now() + Duration::minutes(15);

                let orders = self
                    .order
                    .order_repo
                    .find_orders_in_window(
                        window_start,
                        window_end,
                        &["pending", "assigned", "in_progress"],
                        None,
                        None,
                        terminal,
                        false,
                    )
                    .await?;

                let mut frozen_order_ids = Vec::new();
                let mut frozen_orders = Vec::new();
                let mut candidate_orders = Vec::new();
                for order in &orders {
                    let order_id = order.id.trim();
                    if order_id.is_empty() {
                        continue;
                    }
                    let is_frozen = explicit_frozen_ids.contains(order_id)
                        || matches!(
                            order.status,
                            DispatchOrderStatus::InProgress
                                | DispatchOrderStatus::Completed
                                | DispatchOrderStatus::Cancelled
                        )
                        || matches!(
                            order.lock_level,
                            DispatchLockLevel::Frozen | DispatchLockLevel::ManualLock
                        )
                        || order
                            .planned_start_time
                            .map(|planned_start_time| planned_start_time <= auto_freeze_before)
                            .unwrap_or(false);
                    if is_frozen {
                        frozen_order_ids.push(order_id.to_string());
                        frozen_orders.push(order.clone());
                    } else {
                        candidate_orders.push(order.clone());
                    }
                }

                let mut task_errors = serde_json::Map::new();
                let mut prepared_orders = Vec::new();
                for order in candidate_orders {
                    match self
                        .prepare_window_candidate_order(&order, terminal, window_start)
                        .await
                    {
                        Ok(prepared) => prepared_orders.push(prepared),
                        Err(DomainError::ValidationError(message))
                        | Err(DomainError::BusinessRuleViolation(message)) => {
                            task_errors.insert(order.id.clone(), json!(message));
                        }
                        Err(error) => return Err(error),
                    }
                }
                prepared_orders.sort_by(|left, right| {
                    let (left_start, _) = Self::window_task_interval(&left.order, window_start);
                    let (right_start, _) = Self::window_task_interval(&right.order, window_start);
                    left_start
                        .cmp(&right_start)
                        .then_with(|| left.order.id.cmp(&right.order.id))
                });

                let mut bookings = HashMap::<String, Vec<(DateTime<Utc>, DateTime<Utc>)>>::new();
                for frozen_order in &frozen_orders {
                    let (frozen_start, frozen_end) = Self::window_task_interval(frozen_order, window_start);
                    for user_id in Self::order_member_user_ids(frozen_order) {
                        bookings.entry(user_id).or_default().push((frozen_start, frozen_end));
                    }
                }
                let mut order_rows = Vec::new();
                let mut assigned_count = 0i64;
                let mut unassigned_tasks = Vec::new();

                for prepared in prepared_orders {
                    let (planned_start_time, planned_end_time) =
                        Self::window_task_interval(&prepared.order, window_start);
                    let Some((assignment, assigned_user_ids, schedule_source, travel_time, total_distance_meters)) =
                        self.assign_window_task(&prepared, &bookings, window_start).await?
                    else {
                        unassigned_tasks.push(prepared.order.id.clone());
                        continue;
                    };

                    let mut order = prepared.order.clone();
                    Self::apply_assignment_json(&mut order, Some(&assignment));
                    if matches!(order.status, DispatchOrderStatus::Pending) {
                        order.status = DispatchOrderStatus::Assigned;
                    }
                    order.dispatched_at = order.dispatched_at.or(Some(Utc::now()));
                    order.dispatch_type = DispatchType::Auto;
                    order.schedule_source = schedule_source;
                    order.updated_at = Some(Utc::now());
                    self.order.order_repo.save(&order).await?;
                    self.sync_assignment_members(&order, &assignment).await?;
                    self.order
                        .order_repo
                        .replace_order_equipment_assignments(&order.id, &Self::assignment_equipment_ids(&assignment))
                        .await?;

                    for user_id in assigned_user_ids {
                        bookings
                            .entry(user_id)
                            .or_default()
                            .push((planned_start_time, planned_end_time));
                    }
                    assigned_count += 1;
                    order_rows.push(json!({
                        "dispatch_order_id": order.id,
                        "team_id": order.team_id,
                        "task_crew": order.task_crew,
                        "equipment_assignment": order.equipment_assignment,
                        "department_rule_version": order.department_rule_version,
                        "crew_requirement_snapshot": order.crew_requirement_snapshot,
                        "qualification_gap": order.qualification_gap,
                        "equipment_gap": order.equipment_gap,
                        "travel_time": travel_time,
                        "total_distance_meters": total_distance_meters,
                        "status": helpers::optimal_order_status(&order),
                        "schedule_source": Self::schedule_source_text(order.schedule_source),
                        "availability_reason": order.availability_reason,
                        "score_breakdown": order.score_breakdown,
                    }));
                }

                frozen_order_ids.sort();
                frozen_order_ids.dedup();
                unassigned_tasks.extend(task_errors.keys().cloned());
                unassigned_tasks.sort();
                unassigned_tasks.dedup();

                return Ok(json!({
                    "success": true,
                    "scope": "window",
                    "lock_policy": response_lock_policy,
                    "is_optimal": unassigned_tasks.is_empty() && task_errors.is_empty(),
                    "total_cost": (unassigned_tasks.len() as f64) * 1_000_000_000.0,
                    "solver_time_ms": started_at.elapsed().as_secs_f64() * 1000.0,
                    "assigned_count": assigned_count,
                    "unassigned_count": unassigned_tasks.len(),
                    "unassigned_tasks": unassigned_tasks,
                    "task_errors": Value::Object(task_errors),
                    "frozen_order_ids": frozen_order_ids,
                    "orders": order_rows,
                    "window_start": window_start.to_rfc3339(),
                    "window_end": window_end.to_rfc3339(),
                }));
            }
            _ => {}
        }

        let flight_id = flight_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                DomainError::BusinessRuleViolation("flight scope 需要 flight_id/stand_id/eta/etd".to_string())
            })?;
        let stand_id = stand_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                DomainError::BusinessRuleViolation("flight scope 需要 flight_id/stand_id/eta/etd".to_string())
            })?;
        let eta = eta.ok_or_else(|| {
            DomainError::BusinessRuleViolation("flight scope 需要 flight_id/stand_id/eta/etd".to_string())
        })?;
        let etd = etd.ok_or_else(|| {
            DomainError::BusinessRuleViolation("flight scope 需要 flight_id/stand_id/eta/etd".to_string())
        })?;

        let drafted_orders = self
            .generate_draft_orders(flight_id, stand_id, eta, etd, terminal)
            .await?;
        let order_rows = drafted_orders
            .iter()
            .map(|order| {
                json!({
                    "id": order.id,
                    "task_type": order.task_type,
                    "status": "draft",
                    "publication_state": "prepublished",
                    "message": "草稿工单已生成，请在看板中选中后优化分配",
                })
            })
            .collect::<Vec<_>>();

        Ok(json!({
            "success": true,
            "scope": "flight",
            "is_optimal": false,
            "total_cost": 0.0,
            "solver_time_ms": started_at.elapsed().as_secs_f64() * 1000.0,
            "assigned_count": 0,
            "unassigned_count": drafted_orders.len(),
            "unassigned_tasks": Vec::<String>::new(),
            "task_errors": json!({}),
            "orders": order_rows,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::ensure_publishable_draft_state;
    use fms_domain::error::DomainError;
    use fms_domain::models::dispatch::DispatchOrderStatus;

    #[test]
    fn only_pending_prepublished_orders_can_be_published() {
        ensure_publishable_draft_state("draft-1", DispatchOrderStatus::Pending, "prepublished")
            .expect("pending prepublished draft must pass");
        for status in [
            DispatchOrderStatus::Assigned,
            DispatchOrderStatus::InProgress,
            DispatchOrderStatus::Completed,
            DispatchOrderStatus::Cancelled,
        ] {
            let error = ensure_publishable_draft_state("terminal-1", status, "prepublished")
                .expect_err("non-pending order must be rejected");
            assert!(matches!(error, DomainError::BusinessRuleViolation(message) if message.contains("不是待发布草稿")));
        }
        ensure_publishable_draft_state("published-1", DispatchOrderStatus::Pending, "published")
            .expect_err("already published order must be rejected");
    }
}
