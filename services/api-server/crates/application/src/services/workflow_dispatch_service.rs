use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use serde_json::{Map, Value};
use tracing::warn;

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::{
    AssigneeType, DispatchLockLevel, DispatchOrder, DispatchOrderStatus, DispatchType, ScheduleSource,
};
use fms_domain::models::session_runtime::OnlineSessionStatus;
use fms_domain::models::user::User;
use fms_domain::ports::dispatch_repository::DispatchOrderRepository;
use fms_domain::ports::user_repository::UserRepository;
use fms_domain::ports::workflow_dispatch_repository::WorkflowDispatchRepository;

use crate::schemas::dispatch_schemas::{
    DispatchRecommendationItem, WorkflowDispatchAssignRequest, WorkflowDispatchCreateRequest,
};
use crate::services::flowable_service::{FlowableService, FlowableServiceError};
use crate::services::notification_service::DispatchBatchNotificationCreate;
use crate::types::{ConcreteAuthService, ConcreteDispatchChatService, ConcreteNotificationService};

pub trait WorkflowDispatchSsePublisher: Send + Sync {
    fn publish_system_alert<'a>(
        &'a self,
        event_name: &'a str,
        payload: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;

    fn publish_ai_event<'a>(
        &'a self,
        event_name: &'a str,
        payload: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
}

pub trait DispatchRecommendationService: Send + Sync {
    fn recommend<'a>(
        &'a self,
        department: &'a str,
        task_type: &'a str,
        target_job_title: Option<&'a str>,
        required_people: i32,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<StructuredRecommendationCandidate>, DomainError>> + Send + 'a>>;
}

#[derive(Debug, Clone)]
pub struct StructuredRecommendationCandidate {
    pub user_id: String,
    pub username: String,
    pub score: f64,
    pub reason: String,
    pub online_factor: f64,
    pub workload_factor: f64,
}

pub struct WorkflowDispatchService<
    OR: DispatchOrderRepository + ?Sized,
    UR: UserRepository + ?Sized,
    WR: WorkflowDispatchRepository + ?Sized,
    SP: WorkflowDispatchSsePublisher + ?Sized = OR,
    DR: DispatchRecommendationService + ?Sized = OR,
> {
    order_repo: Arc<OR>,
    user_repo: Arc<UR>,
    workflow_repo: Arc<WR>,
    auth_service: Option<Arc<ConcreteAuthService>>,
    dispatch_chat_service: Option<Arc<ConcreteDispatchChatService>>,
    flowable_service: Option<Arc<FlowableService>>,
    notification_service: Option<Arc<ConcreteNotificationService>>,
    sse_publisher: Option<Arc<SP>>,
    dispatch_recommendation_service: Option<Arc<DR>>,
}

impl<
        OR: DispatchOrderRepository + ?Sized,
        UR: UserRepository + ?Sized,
        WR: WorkflowDispatchRepository + ?Sized,
        SP: WorkflowDispatchSsePublisher + ?Sized,
        DR: DispatchRecommendationService + ?Sized,
    > WorkflowDispatchService<OR, UR, WR, SP, DR>
{
    pub fn new(order_repo: Arc<OR>, user_repo: Arc<UR>, workflow_repo: Arc<WR>) -> Self {
        Self {
            order_repo,
            user_repo,
            workflow_repo,
            auth_service: None,
            dispatch_chat_service: None,
            flowable_service: None,
            notification_service: None,
            sse_publisher: None,
            dispatch_recommendation_service: None,
        }
    }

    pub fn with_auth_service(mut self, auth_service: Arc<ConcreteAuthService>) -> Self {
        self.auth_service = Some(auth_service);
        self
    }

    pub fn with_dispatch_chat_service(mut self, dispatch_chat_service: Arc<ConcreteDispatchChatService>) -> Self {
        self.dispatch_chat_service = Some(dispatch_chat_service);
        self
    }

    pub fn with_flowable_service(mut self, flowable_service: Arc<FlowableService>) -> Self {
        self.flowable_service = Some(flowable_service);
        self
    }

    pub fn with_notification_service(mut self, notification_service: Arc<ConcreteNotificationService>) -> Self {
        self.notification_service = Some(notification_service);
        self
    }

    pub fn with_sse_publisher(mut self, sse_publisher: Arc<SP>) -> Self {
        self.sse_publisher = Some(sse_publisher);
        self
    }

    pub fn with_dispatch_recommendation_service(mut self, service: Arc<DR>) -> Self {
        self.dispatch_recommendation_service = Some(service);
        self
    }

    async fn sync_dispatch_chat_for_order(&self, order_id: &str) {
        let Some(dispatch_chat_service) = self.dispatch_chat_service.as_ref() else {
            return;
        };
        if let Err(error) = dispatch_chat_service.sync_group_for_dispatch_order_id(order_id).await {
            warn!(order_id, error = %error, "failed to sync workflow dispatch chat group");
        }
    }

    async fn notify_supervisors_best_effort(
        &self,
        order: &mut DispatchOrder,
        supervisors: &[SupervisorCandidate],
    ) -> Result<(), DomainError> {
        if let Some(notification_service) = self.notification_service.as_ref() {
            let user_ids = supervisors.iter().map(|item| item.id.clone()).collect::<Vec<_>>();
            if !user_ids.is_empty() {
                let body = format!(
                    "Dispatch order {} for flight {} is waiting for assignment. TaskType: {}.",
                    order.id, order.flight_id, order.task_type
                );
                if let Err(error) = notification_service
                    .send_batch(DispatchBatchNotificationCreate {
                        user_ids,
                        title: "Workflow dispatch pending assignment".to_string(),
                        body,
                        category: "dispatch".to_string(),
                        severity: "warning".to_string(),
                        flight_id: Some(order.flight_id.clone()),
                        related_entity_type: Some("dispatch_order".to_string()),
                        related_entity_id: Some(order.id.clone()),
                        dispatch_order_id: Some(order.id.clone()),
                        group_id: None,
                        sender_user_id: None,
                        sender_username_snapshot: None,
                        origin_type: "workflow".to_string(),
                        receipt_required: true,
                    })
                    .await
                {
                    warn!(
                        order_id = %order.id,
                        flight_id = %order.flight_id,
                        error = %error,
                        "failed to notify workflow supervisors"
                    );
                }
            }
        }

        let notified_at = Utc::now();
        order.supervisor_notified = true;
        order.supervisor_notified_at = Some(notified_at);
        order.updated_at = Some(notified_at);
        self.order_repo.save(order).await
    }

    async fn notify_assignees_best_effort(&self, order: &DispatchOrder, assigned_user_ids: &[String]) {
        let Some(notification_service) = self.notification_service.as_ref() else {
            return;
        };
        if assigned_user_ids.is_empty() {
            return;
        }

        let body = format!(
            "You have been assigned to dispatch order {} for flight {} (step {}).",
            order.id, order.flight_id, order.task_type
        );
        let origin_type = if order.source.trim() == "workflow" {
            "workflow"
        } else {
            "manual"
        };

        if let Err(error) = notification_service
            .send_batch(DispatchBatchNotificationCreate {
                user_ids: assigned_user_ids.to_vec(),
                title: "Dispatch order assigned".to_string(),
                body,
                category: "dispatch".to_string(),
                severity: "info".to_string(),
                flight_id: Some(order.flight_id.clone()),
                related_entity_type: Some("dispatch_order".to_string()),
                related_entity_id: Some(order.id.clone()),
                dispatch_order_id: Some(order.id.clone()),
                group_id: None,
                sender_user_id: None,
                sender_username_snapshot: None,
                origin_type: origin_type.to_string(),
                receipt_required: true,
            })
            .await
        {
            warn!(
                order_id = %order.id,
                flight_id = %order.flight_id,
                error = %error,
                "failed to notify workflow assignees"
            );
        }
    }

    async fn broadcast_dispatch_created(
        &self,
        order: &DispatchOrder,
        supervisor_ids: &[String],
        required_people: i32,
        priority: &str,
        assignment_deadline: DateTime<Utc>,
    ) {
        let Some(publisher) = self.sse_publisher.as_ref() else {
            return;
        };

        let payload = serde_json::json!({
            "event": "workflow_dispatch_created",
            "dispatch_order_id": order.id,
            "process_instance_id": order.process_instance_id,
            "target_user_ids": supervisor_ids,
            "flight_id": order.flight_id,
            "task_type": order.task_type,
            "stand_id": order.stand_id,
            "required_people": required_people,
            "priority": priority,
            "assignment_deadline": assignment_deadline.to_rfc3339(),
        });

        publisher
            .publish_system_alert("workflow_dispatch_created", payload.clone())
            .await;

        let ai_event = serde_json::json!({
            "type": "workflow_dispatch_created",
            "payload": payload,
            "timestamp": Utc::now().to_rfc3339(),
        });
        publisher.publish_ai_event("workflow_dispatch_created", ai_event).await;
    }

    async fn broadcast_dispatch_assigned(&self, order: &DispatchOrder, assigned_user_ids: &[String]) {
        let Some(publisher) = self.sse_publisher.as_ref() else {
            return;
        };

        let is_workflow = order.source.trim() == "workflow";
        let payload = serde_json::json!({
            "event": "dispatch_assigned",
            "dispatch_order_id": order.id,
            "target_user_ids": assigned_user_ids,
            "flight_id": order.flight_id,
            "task_type": order.task_type,
            "stand_id": order.stand_id,
            "workflow": is_workflow,
        });

        publisher.publish_system_alert("dispatch_assigned", payload).await;
    }

    pub async fn create_dispatch_from_workflow(
        &self,
        payload: WorkflowDispatchCreateRequest,
    ) -> Result<DispatchOrder, DomainError> {
        let target_department = required_text(&payload.target_department, "缺少 target_department")?;
        let task_type = required_text(&payload.task_type, "缺少 task_type")?;
        let flight_id = required_text(&payload.flight_id, "缺少 flight_id")?;
        let process_instance_id = required_text(&payload.process_instance_id, "缺少 process_instance_id")?;
        let process_task_id = required_text(&payload.process_task_id, "缺少 process_task_id")?;
        let target_job_title = optional_text(payload.target_job_title.as_deref());
        let required_people = normalize_required_people(payload.required_people)?;

        if let Some(existing_order) = self
            .find_existing_dispatch_order(&payload, &flight_id, &process_instance_id)
            .await?
        {
            return Ok(existing_order);
        }

        let supervisors = self.find_supervisors(&target_department).await?;
        if supervisors.is_empty() {
            return Err(DomainError::BusinessRuleViolation(format!(
                "未找到部门 {target_department} 的主管或调度员"
            )));
        }

        let recommendations = self
            .recommend_assignees(
                &target_department,
                &task_type,
                target_job_title.as_deref(),
                required_people,
            )
            .await?;
        let workflow_context =
            build_workflow_context(&payload, &target_department, target_job_title.clone(), required_people);
        let recommendation_score = recommendations.first().map(|item| item.score);
        let now = Utc::now();
        let assignment_deadline = payload
            .assignment_deadline
            .unwrap_or_else(|| now + Duration::minutes(30));
        let order_id = ulid::Ulid::new().to_string();
        let owner = &supervisors[0];
        let mut order = DispatchOrder {
            id: order_id.clone(),
            flight_id,
            task_type,
            stand_id: payload.stand_id,
            task_type_name: None,
            stand_code: None,
            terminal: None,
            department: Some(target_department.clone()),
            individual_user_id: Some(owner.id.clone()),
            individual_username: None,
            driver_type: None,
            driver_user_id: None,
            planned_start_time: payload.planned_start_time,
            planned_end_time: payload.planned_end_time,
            actual_start_time: None,
            actual_end_time: None,
            estimated_completion_time: None,
            estimated_completion_reported_by: None,
            estimated_completion_reported_at: None,
            estimated_completion_note: None,
            status: DispatchOrderStatus::Pending,
            dispatch_type: DispatchType::Manual,
            dispatched_at: None,
            dispatched_by: None,
            snapshot_assignee_position: None,
            snapshot_equipment_positions: None,
            estimated_arrival_minutes: None,
            process_instance_id: Some(process_instance_id),
            process_task_id: Some(process_task_id),
            workflow_context,
            workflow_status: "pending_assignment".to_string(),
            source: "workflow".to_string(),
            schedule_source: ScheduleSource::CurrentStatusFallback,
            lock_level: DispatchLockLevel::Optimizable,
            publication_state: "published".to_string(),
            source_type: "manual".to_string(),
            department_id: None,
            leg_scope: "none".to_string(),
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
            recommended_assignees: recommendations.iter().map(recommendation_to_value).collect(),
            recommendation_score,
            supervisor_notified: false,
            supervisor_notified_at: None,
            assignment_deadline: Some(assignment_deadline),
            completed_by: None,
            completion_notes: None,
            gate: None,
            created_at: Some(now),
            updated_at: Some(now),
            members: vec![],
            equipment_list: vec![],
        };

        self.order_repo.save(&order).await?;
        self.notify_supervisors_best_effort(&mut order, &supervisors).await?;

        let supervisor_ids: Vec<String> = supervisors.iter().map(|s| s.id.clone()).collect();
        self.broadcast_dispatch_created(
            &order,
            &supervisor_ids,
            required_people,
            &payload.priority,
            assignment_deadline,
        )
        .await;
        self.order_repo
            .append_log(
                &order.id,
                "workflow_triggered",
                None,
                Some(serde_json::json!({
                    "process_instance_id": order.process_instance_id,
                    "process_task_id": order.process_task_id,
                    "target_department": target_department,
                    "target_job_title": target_job_title,
                    "required_people": required_people,
                })),
            )
            .await?;
        self.sync_dispatch_chat_for_order(&order.id).await;

        self.order_repo
            .find_by_id(&order.id, true, None)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "DispatchOrder",
                id: order.id,
            })
    }

    pub async fn assign_dispatch_from_supervisor(
        &self,
        dispatch_order_id: &str,
        payload: WorkflowDispatchAssignRequest,
        assigned_by: &str,
    ) -> Result<DispatchOrder, DomainError> {
        let assigned_user_ids = normalize_user_ids(&payload.assigned_user_ids)?;
        let mut order = self
            .order_repo
            .find_by_id(dispatch_order_id, true, None)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "DispatchOrder",
                id: dispatch_order_id.to_string(),
            })?;

        order.individual_user_id = assigned_user_ids.first().cloned();
        order.individual_username = match assigned_user_ids.first() {
            Some(first_user_id) => self
                .user_repo
                .find_by_id(first_user_id)
                .await?
                .map(|user| user.username),
            None => None,
        };
        order.status = DispatchOrderStatus::Assigned;
        order.dispatch_type = DispatchType::Manual;
        order.dispatched_by = Some(assigned_by.to_string());
        order.dispatched_at = Some(Utc::now());
        order.workflow_status = "assigned".to_string();
        if let Some(notes) = optional_text(payload.notes.as_deref()) {
            if let serde_json::Value::Object(ref mut map) = order.workflow_context {
                map.insert("assignment_notes".to_string(), Value::String(notes));
            }
        }
        order.updated_at = Some(Utc::now());

        self.order_repo.save(&order).await?;
        self.workflow_repo
            .replace_assignment_members(dispatch_order_id, &assigned_user_ids)
            .await?;
        if payload.complete_process_task {
            if let Err(error) = self
                .complete_upstream_process_task(&mut order, &assigned_user_ids, assigned_by, payload.notes.as_deref())
                .await
            {
                self.order_repo.save(&order).await?;
                return Err(error);
            }
        }
        self.order_repo
            .append_log(
                dispatch_order_id,
                "workflow_assigned",
                Some(assigned_by),
                Some(serde_json::json!({
                    "assigned_user_ids": assigned_user_ids,
                    "notes": payload.notes,
                    "complete_process_task": payload.complete_process_task,
                })),
            )
            .await?;
        self.sync_dispatch_chat_for_order(dispatch_order_id).await;
        self.notify_assignees_best_effort(&order, &assigned_user_ids).await;
        self.broadcast_dispatch_assigned(&order, &assigned_user_ids).await;

        self.order_repo
            .find_by_id(dispatch_order_id, true, None)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "DispatchOrder",
                id: dispatch_order_id.to_string(),
            })
    }

    pub async fn recommend_assignees(
        &self,
        department: &str,
        task_type: &str,
        target_job_title: Option<&str>,
        required_people: i32,
    ) -> Result<Vec<DispatchRecommendationItem>, DomainError> {
        if let Some(service) = self.dispatch_recommendation_service.as_ref() {
            match service
                .recommend(department, task_type, target_job_title, required_people)
                .await
            {
                Ok(structured) => {
                    let mut transformed = Vec::with_capacity(structured.len());
                    for candidate in structured {
                        let online_factor = candidate.online_factor;
                        let status = if online_factor >= 28.0 {
                            "active"
                        } else if online_factor >= 22.0 {
                            "idle"
                        } else if online_factor >= 16.0 {
                            "online"
                        } else {
                            "offline"
                        };
                        let workload_factor = candidate.workload_factor;
                        let workload = (0.0_f64.max((-workload_factor).min(0.0).abs() / 6.0)) as i32;
                        transformed.push(DispatchRecommendationItem {
                            user_id: candidate.user_id,
                            username: candidate.username,
                            status: status.to_string(),
                            department: Some(department.to_string()),
                            job_title: target_job_title.map(str::to_string),
                            score: candidate.score,
                            reason: candidate.reason,
                            workload,
                        });
                    }
                    return Ok(transformed);
                }
                Err(error) => {
                    warn!(
                        error = %error,
                        "recommend_assignees fallback due to structured service error"
                    );
                }
            }
        }

        let normalized_department = required_text(department, "缺少部门")?;
        let title_filter = optional_text(target_job_title);
        let users = self.find_active_department_users(&normalized_department).await?;
        let online_map = self.online_status_map().await;
        let workload_map = self
            .workflow_repo
            .get_active_workload_by_users(&users.iter().map(|user| user.id.clone()).collect::<Vec<_>>())
            .await?;

        let mut candidates = users
            .into_iter()
            .filter(|user| {
                title_filter
                    .as_deref()
                    .map(|title| user.job_title.as_deref() == Some(title))
                    .unwrap_or(true)
            })
            .map(|user| {
                let status = online_map
                    .get(&user.id)
                    .cloned()
                    .unwrap_or_else(|| "offline".to_string());
                let workload = workload_map.get(&user.id).copied().unwrap_or(0);
                let status_score = match status.as_str() {
                    "active" => 100.0,
                    "idle" => 85.0,
                    "online" => 70.0,
                    _ => 20.0,
                };
                let score = (status_score - (workload as f64 * 8.0)).max(0.0);
                DispatchRecommendationItem {
                    user_id: user.id,
                    username: user.username,
                    status: status.clone(),
                    department: user.department,
                    job_title: user.job_title,
                    score: (score * 100.0).round() / 100.0,
                    reason: format!("在线状态:{status}, 当前负载:{workload}"),
                    workload: workload as i32,
                }
            })
            .collect::<Vec<_>>();

        candidates.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        candidates.truncate(std::cmp::max(required_people * 3, 5) as usize);
        Ok(candidates)
    }

    async fn find_active_department_users(&self, department: &str) -> Result<Vec<User>, DomainError> {
        let mut users = Vec::new();
        let mut offset = 0;
        let limit = 200;
        let normalized_department = department.trim();

        loop {
            let chunk = self.user_repo.find_all(limit, offset).await?;
            if chunk.is_empty() {
                break;
            }
            let chunk_len = chunk.len();
            users.extend(
                chunk
                    .into_iter()
                    .filter(|user| user.is_active)
                    .filter(|user| user.department.as_deref() == Some(normalized_department)),
            );
            if users.len() >= 2000 {
                break;
            }
            if chunk_len < limit as usize {
                break;
            }
            offset += limit;
        }

        Ok(users)
    }

    async fn find_supervisors(&self, department: &str) -> Result<Vec<SupervisorCandidate>, DomainError> {
        let users = self.find_active_department_users(department).await?;
        Ok(users
            .into_iter()
            .filter(|user| is_supervisor(user))
            .map(|user| SupervisorCandidate { id: user.id })
            .collect())
    }

    async fn online_status_map(&self) -> HashMap<String, String> {
        let Some(auth_service) = &self.auth_service else {
            return HashMap::new();
        };

        match auth_service.get_all_online_users_status().await {
            Ok(statuses) => statuses
                .into_iter()
                .map(|status| {
                    let normalized_status = normalize_online_status(&status);
                    (status.user_id, normalized_status)
                })
                .collect(),
            Err(_) => HashMap::new(),
        }
    }

    async fn complete_upstream_process_task(
        &self,
        order: &mut DispatchOrder,
        assigned_user_ids: &[String],
        assigned_by: &str,
        notes: Option<&str>,
    ) -> Result<(), DomainError> {
        if workflow_context_flag(&order.workflow_context, "auto_completed_process_task") {
            return Ok(());
        }
        let Some(flowable_service) = self.flowable_service.as_ref() else {
            return Ok(());
        };
        let Some(process_task_id) = order
            .process_task_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Ok(());
        };

        let variables = build_assignment_complete_variables(order, assigned_user_ids, assigned_by, notes);
        match flowable_service.complete_task(process_task_id, Some(&variables)).await {
            Ok(true) => {
                order.workflow_status = "assigned".to_string();
                if let serde_json::Value::Object(ref mut map) = order.workflow_context {
                    map.remove("sync_error");
                }
                Ok(())
            }
            Ok(false) => {
                let message = format!(
                    "flowable sync failed for dispatch order {}: task not completed",
                    order.id
                );
                mark_sync_failed(order, &message);
                Err(DomainError::Internal(message))
            }
            Err(error) => {
                let message = format!(
                    "flowable sync failed for dispatch order {}: {}",
                    order.id,
                    flowable_error_message(error)
                );
                mark_sync_failed(order, &message);
                Err(DomainError::Internal(message))
            }
        }
    }

    async fn find_existing_dispatch_order(
        &self,
        payload: &WorkflowDispatchCreateRequest,
        flight_id: &str,
        process_instance_id: &str,
    ) -> Result<Option<DispatchOrder>, DomainError> {
        let workflow_node_id = payload
            .context
            .get("workflow_node_id")
            .and_then(Value::as_str)
            .and_then(|value| optional_text(Some(value)));
        let workflow_idempotency_key = payload
            .context
            .get("workflow_idempotency_key")
            .and_then(Value::as_str)
            .and_then(|value| optional_text(Some(value)));

        let (Some(workflow_node_id), Some(workflow_idempotency_key)) = (workflow_node_id, workflow_idempotency_key)
        else {
            return Ok(None);
        };

        let existing_order = self
            .order_repo
            .find_by_flight(flight_id)
            .await?
            .into_iter()
            .find(|order| {
                order.process_instance_id.as_deref().map(str::trim) == Some(process_instance_id)
                    && workflow_context_text(&order.workflow_context, "workflow_node_id").as_deref()
                        == Some(workflow_node_id.as_str())
                    && workflow_context_text(&order.workflow_context, "workflow_idempotency_key").as_deref()
                        == Some(workflow_idempotency_key.as_str())
            });

        let Some(existing_order) = existing_order else {
            return Ok(None);
        };

        match self.order_repo.find_by_id(&existing_order.id, true, None).await? {
            Some(order) => Ok(Some(order)),
            None => Ok(Some(existing_order)),
        }
    }
}

#[derive(Debug, Clone)]
struct SupervisorCandidate {
    id: String,
}

fn is_supervisor(user: &User) -> bool {
    let title = user.job_title.as_deref().unwrap_or_default();
    if title == "主管" || title == "调度员" {
        return true;
    }

    user.roles.iter().any(|role| {
        let normalized = role.name.trim().to_ascii_lowercase();
        matches!(normalized.as_str(), "supervisor" | "dispatcher" | "dispatch_manager")
    })
}

fn build_workflow_context(
    payload: &WorkflowDispatchCreateRequest,
    target_department: &str,
    target_job_title: Option<String>,
    required_people: i32,
) -> serde_json::Value {
    let mut context = serde_json::Map::from_iter(payload.context.clone().into_iter());
    context.insert("stand_id".to_string(), option_to_value(payload.stand_id.clone()));
    context.insert(
        "priority".to_string(),
        Value::String(payload.priority.trim().to_string()),
    );
    context.insert("description".to_string(), option_to_value(payload.description.clone()));
    context.insert(
        "target_department".to_string(),
        Value::String(target_department.to_string()),
    );
    context.insert("target_job_title".to_string(), option_to_value(target_job_title));
    context.insert("required_people".to_string(), Value::Number(required_people.into()));
    context.insert(
        "process_definition_key".to_string(),
        option_to_value(payload.process_definition_key.clone()),
    );
    context.insert(
        "business_key".to_string(),
        option_to_value(payload.business_key.clone()),
    );
    serde_json::Value::Object(context)
}

fn option_to_value(value: Option<String>) -> Value {
    match value {
        Some(value) => Value::String(value),
        None => Value::Null,
    }
}

fn recommendation_to_value(item: &DispatchRecommendationItem) -> Value {
    let mut object = Map::new();
    object.insert("user_id".to_string(), Value::String(item.user_id.clone()));
    object.insert("username".to_string(), Value::String(item.username.clone()));
    object.insert("status".to_string(), Value::String(item.status.clone()));
    object.insert("department".to_string(), option_to_value(item.department.clone()));
    object.insert("job_title".to_string(), option_to_value(item.job_title.clone()));
    object.insert(
        "score".to_string(),
        serde_json::Number::from_f64(item.score)
            .map(Value::Number)
            .unwrap_or_else(|| Value::Number(serde_json::Number::from(0))),
    );
    object.insert("reason".to_string(), Value::String(item.reason.clone()));
    object.insert("workload".to_string(), Value::Number(item.workload.into()));
    Value::Object(object)
}

fn normalize_required_people(value: i32) -> Result<i32, DomainError> {
    if (1..=20).contains(&value) {
        Ok(value)
    } else {
        Err(DomainError::ValidationError(
            "required_people 必须在 1 到 20 之间".to_string(),
        ))
    }
}

fn normalize_user_ids(user_ids: &[String]) -> Result<Vec<String>, DomainError> {
    let mut normalized = Vec::new();
    for user_id in user_ids {
        let value = required_text(user_id, "assigned_user_ids 不能为空")?;
        if !normalized.iter().any(|item| item == &value) {
            normalized.push(value);
        }
    }
    if normalized.is_empty() {
        return Err(DomainError::ValidationError("assigned_user_ids 不能为空".to_string()));
    }
    Ok(normalized)
}

fn required_text(value: &str, error_message: &str) -> Result<String, DomainError> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(DomainError::ValidationError(error_message.to_string()));
    }
    Ok(normalized.to_string())
}

fn optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn normalize_online_status(status: &OnlineSessionStatus) -> String {
    let normalized = status.status.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "active" | "idle" | "online" => normalized,
        _ => "offline".to_string(),
    }
}

fn workflow_context_text(workflow_context: &Value, key: &str) -> Option<String> {
    workflow_context.get(key).and_then(|value| match value {
        Value::String(text) => optional_text(Some(text.as_str())),
        _ => None,
    })
}

fn workflow_context_flag(workflow_context: &Value, key: &str) -> bool {
    workflow_context.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn build_assignment_complete_variables(
    order: &DispatchOrder,
    assigned_user_ids: &[String],
    assigned_by: &str,
    notes: Option<&str>,
) -> serde_json::Map<String, Value> {
    let mut variables = serde_json::Map::new();
    variables.insert("dispatchOrderId".to_string(), Value::String(order.id.clone()));
    variables.insert(
        "assignedUserIds".to_string(),
        Value::Array(assigned_user_ids.iter().cloned().map(Value::String).collect()),
    );
    variables.insert("assignedBy".to_string(), Value::String(assigned_by.to_string()));
    variables.insert("assignedAt".to_string(), Value::String(Utc::now().to_rfc3339()));
    variables.insert(
        "assignmentNotes".to_string(),
        Value::String(notes.unwrap_or_default().trim().to_string()),
    );
    if let Some(individual_user_id) = order
        .individual_user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        variables.insert(
            "assignedUserId".to_string(),
            Value::String(individual_user_id.to_string()),
        );
    }
    variables
}

fn mark_sync_failed(order: &mut DispatchOrder, message: &str) {
    order.workflow_status = "sync_failed".to_string();
    if let serde_json::Value::Object(ref mut map) = order.workflow_context {
        map.insert("sync_error".to_string(), Value::String(message.to_string()));
    }
}

fn flowable_error_message(error: FlowableServiceError) -> String {
    match error {
        FlowableServiceError::Validation(message)
        | FlowableServiceError::NotFound(message)
        | FlowableServiceError::Upstream(message) => message,
    }
}
