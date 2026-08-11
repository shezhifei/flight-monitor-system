//! 派工应用服务
//!
//! 对应 Python `dispatch_command_service` + `dispatch_query_service` 核心子集。

mod generation;
mod generation_batch;
mod generation_flight;
mod generation_replan;
mod helpers;
mod helpers_ids;
mod helpers_notifications;
mod helpers_validation;
mod mobile_lifecycle;
mod mobile_ops;
mod mobile_reporting;
mod mobile_workbench;
mod order_lifecycle;
mod safety;
#[cfg(test)]
mod tests;
pub mod dispatch_overrun_warning_service;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

static NULL_VALUE: serde_json::Value = serde_json::Value::Null;

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::*;
use fms_domain::ports::anomaly_repository::AnomalyRepository;
use fms_domain::ports::dispatch_collaboration_repository::DispatchCollaborationRepository;
use fms_domain::ports::dispatch_repository::{
    DepartmentQualificationRepository, DepartmentRepository, DepartmentTaskTypeRequirementRepository,
    DispatchAlertRepository, DispatchChecklistRepository, DispatchOrderMemberRepository, DispatchOrderRepository,
    DispatchTravelStatsRepository, EquipmentRepository, FlightGenerationRuleRepository,
    GenerationAdjustmentRuleRepository, QualificationGrantRepository, StandRepository, TaskTypeRepository,
    TeamMemberRepository, TeamRepository, TeamTypeRepository, TemporaryTaskTemplateRepository,
};
use fms_domain::ports::flight_repository::FlightRepository;
use fms_domain::ports::todo_repository::TodoRepository;

use async_trait::async_trait;
use fms_domain::ports::NullRepository;

use crate::services::dispatch_chat_service::DispatchChatService;
use crate::services::dispatch_order_adjuster_handler::EventRuleOrderGateway;
use crate::services::notification_service::{DispatchBatchNotificationCreate, NotificationService};
use crate::services::resource_availability_service::ResourceAvailabilityGateway;
use crate::sqlx_transactional_repositories::{
    SqlxDispatchOrderMemberTransactionalRepository, SqlxDispatchOrderTransactionalRepository,
};

#[async_trait]
pub trait DispatchNotificationSender: Send + Sync {
    async fn send_dispatch_batch(&self, dto: DispatchBatchNotificationCreate)
        -> Result<serde_json::Value, DomainError>;
}

#[async_trait]
pub trait DispatchChatOrderSyncer: Send + Sync {
    async fn sync_dispatch_order_chat(&self, order_id: &str);
}

#[async_trait]
impl<
        NR: fms_domain::ports::notification_repository::NotificationRepository + Send + Sync + ?Sized,
        PR: fms_domain::ports::notification_repository::NotificationPreferenceRepository + Send + Sync + ?Sized,
        CR: DispatchCollaborationRepository + Send + Sync + ?Sized,
        DP: crate::services::notification_service::NotificationDeliveryPublisher + Send + Sync + ?Sized,
        MR: crate::services::notification_service::NotificationMetricsRecorder + Send + Sync + ?Sized,
        RS: crate::services::notification_service::NotificationReceiptGroupSync + Send + Sync + ?Sized,
    > DispatchNotificationSender for NotificationService<NR, PR, CR, DP, MR, RS>
{
    async fn send_dispatch_batch(
        &self,
        dto: DispatchBatchNotificationCreate,
    ) -> Result<serde_json::Value, DomainError> {
        self.send_batch(dto).await
    }
}

#[async_trait]
impl DispatchNotificationSender for NullRepository {
    async fn send_dispatch_batch(
        &self,
        _dto: DispatchBatchNotificationCreate,
    ) -> Result<serde_json::Value, DomainError> {
        Ok(serde_json::Value::Null)
    }
}

#[async_trait]
impl DispatchChatOrderSyncer for DispatchChatService {
    async fn sync_dispatch_order_chat(&self, order_id: &str) {
        if let Err(error) = self.sync_group_for_dispatch_order_id(order_id).await {
            tracing::warn!(order_id, error = %error, "failed to sync dispatch chat group");
        }
    }
}

#[async_trait]
impl DispatchChatOrderSyncer for NullRepository {
    async fn sync_dispatch_order_chat(&self, _order_id: &str) {}
}

/// 派工应用服务
pub struct DispatchService {
    order: DispatchOrderServiceDependencies,
    rules: DispatchRuleServiceDependencies,
    resources: DispatchResourceServiceDependencies,
    notifications: DispatchNotificationServiceDependencies,
    analytics: DispatchAnalyticsServiceDependencies,
    /// 预排冲突预警(可选;生命周期钩子 best-effort 调用)。
    overrun_warning_service: Option<Arc<dispatch_overrun_warning_service::DispatchOverrunWarningService>>,
}

struct DispatchOrderServiceDependencies {
    order_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
    order_tx_repo: Option<Arc<dyn SqlxDispatchOrderTransactionalRepository>>,
    member_repo: Option<Arc<dyn DispatchOrderMemberRepository + Send + Sync>>,
    member_tx_repo: Option<Arc<dyn SqlxDispatchOrderMemberTransactionalRepository>>,
    todo_repo: Option<Arc<dyn TodoRepository + Send + Sync>>,
}

#[derive(Default)]
struct DispatchRuleServiceDependencies {
    department_repo: Option<Arc<dyn DepartmentRepository + Send + Sync>>,
    task_type_repo: Option<Arc<dyn TaskTypeRepository + Send + Sync>>,
    task_type_requirement_repo: Option<Arc<dyn DepartmentTaskTypeRequirementRepository + Send + Sync>>,
    flight_repo: Option<Arc<dyn FlightRepository + Send + Sync>>,
    generation_rule_repo: Option<Arc<dyn FlightGenerationRuleRepository + Send + Sync>>,
    adjustment_rule_repo: Option<Arc<dyn GenerationAdjustmentRuleRepository + Send + Sync>>,
    temporary_task_template_repo: Option<Arc<dyn TemporaryTaskTemplateRepository + Send + Sync>>,
}

#[derive(Default)]
struct DispatchResourceServiceDependencies {
    team_repo: Option<Arc<dyn TeamRepository + Send + Sync>>,
    team_type_repo: Option<Arc<dyn TeamTypeRepository + Send + Sync>>,
    stand_repo: Option<Arc<dyn StandRepository + Send + Sync>>,
    qualification_repo: Option<Arc<dyn DepartmentQualificationRepository + Send + Sync>>,
    qualification_grant_repo: Option<Arc<dyn QualificationGrantRepository + Send + Sync>>,
    equipment_repo: Option<Arc<dyn EquipmentRepository + Send + Sync>>,
    team_member_repo: Option<Arc<dyn TeamMemberRepository + Send + Sync>>,
    travel_stats_repo: Option<Arc<dyn DispatchTravelStatsRepository + Send + Sync>>,
    checklist_repo: Option<Arc<dyn DispatchChecklistRepository + Send + Sync>>,
    resource_availability_service: Option<Arc<dyn ResourceAvailabilityGateway + Send + Sync>>,
}

#[derive(Default)]
struct DispatchNotificationServiceDependencies {
    anomaly_repo: Option<Arc<dyn AnomalyRepository + Send + Sync>>,
    collaboration_repo: Option<Arc<dyn DispatchCollaborationRepository + Send + Sync>>,
    alert_repo: Option<Arc<dyn DispatchAlertRepository + Send + Sync>>,
    notification_service: Option<Arc<dyn DispatchNotificationSender + Send + Sync>>,
    dispatch_chat_service: Option<Arc<dyn DispatchChatOrderSyncer + Send + Sync>>,
}

struct DispatchAnalyticsServiceDependencies {
    metrics_counters: DashMap<String, i64>,
}

struct ReplanExecutionResult {
    suggestions: Vec<Value>,
    summary: Value,
}

struct GeneratedFlightDispatchRequest {
    task_type: String,
    stand_id: String,
    terminal: Option<String>,
    planned_start_time: DateTime<Utc>,
    planned_end_time: DateTime<Utc>,
    source_type: String,
    department_id: String,
    leg_scope: String,
    generation_rule_id: String,
    generation_rule_version: i32,
    generation_anchor_type: String,
    generation_anchor_time: DateTime<Utc>,
    completion_time_mode: String,
    completion_anchor_type: Option<String>,
    completion_anchor_time: Option<DateTime<Utc>>,
    completion_offset_minutes: Option<i32>,
    completion_warning_lead_minutes: Option<i32>,
    publish_trigger_mode: String,
    publish_at: Option<DateTime<Utc>>,
    turnaround_pair_key: Option<String>,
    turnaround_constraint_mode: Option<String>,
    department_rule_version: String,
    crew_requirement_snapshot: Vec<Value>,
    equipment_requirement_snapshot: Vec<Value>,
}

#[derive(Clone)]
struct WindowOptimizationCandidate {
    user_id: String,
    username: Option<String>,
    source_team_id: Option<String>,
    source_team_name: Option<String>,
    schedule_source: ScheduleSource,
    qualifications: Vec<(String, Option<String>, Option<String>, Option<String>)>,
}

#[derive(Clone)]
struct PreparedWindowOrder {
    order: DispatchOrder,
    stand_position: (f64, f64),
    department_rule_version: Option<String>,
    crew_requirement_snapshot: Vec<Value>,
    equipment_requirement_snapshot: Vec<Value>,
    level_index: HashMap<String, HashSet<String>>,
    baseline_by_slot: HashMap<String, Vec<String>>,
    available_candidates: Vec<WindowOptimizationCandidate>,
}

impl DispatchService {
    pub fn new(order_repo: Arc<dyn DispatchOrderRepository + Send + Sync>) -> Self {
        let metrics = DashMap::new();
        metrics.insert("dispatch.order.complete.blocked".to_string(), 0);
        metrics.insert("dispatch.order.complete.soft".to_string(), 0);
        metrics.insert("dispatch.order.arrival.pending_verification".to_string(), 0);
        metrics.insert("dispatch.issue_reported.text".to_string(), 0);
        metrics.insert("dispatch.issue_reported.photo".to_string(), 0);
        metrics.insert("dispatch.issue_reported.voice".to_string(), 0);

        Self {
            order: DispatchOrderServiceDependencies {
                order_repo,
                order_tx_repo: None,
                member_repo: None,
                member_tx_repo: None,
                todo_repo: None,
            },
            rules: DispatchRuleServiceDependencies::default(),
            resources: DispatchResourceServiceDependencies::default(),
            notifications: DispatchNotificationServiceDependencies::default(),
            analytics: DispatchAnalyticsServiceDependencies {
                metrics_counters: metrics,
            },
            overrun_warning_service: None,
        }
    }

    pub fn with_dispatch_repos(
        mut self,
        team_repo: Arc<dyn TeamRepository + Send + Sync>,
        team_type_repo: Arc<dyn TeamTypeRepository + Send + Sync>,
        stand_repo: Arc<dyn StandRepository + Send + Sync>,
        task_type_repo: Arc<dyn TaskTypeRepository + Send + Sync>,
        task_type_requirement_repo: Arc<dyn DepartmentTaskTypeRequirementRepository + Send + Sync>,
        temporary_task_template_repo: Arc<dyn TemporaryTaskTemplateRepository + Send + Sync>,
    ) -> Self {
        self.resources.team_repo = Some(team_repo);
        self.resources.team_type_repo = Some(team_type_repo);
        self.resources.stand_repo = Some(stand_repo);
        self.rules.task_type_repo = Some(task_type_repo);
        self.rules.task_type_requirement_repo = Some(task_type_requirement_repo);
        self.rules.temporary_task_template_repo = Some(temporary_task_template_repo);
        self
    }

    pub fn with_generation_repos(
        mut self,
        department_repo: Arc<dyn DepartmentRepository + Send + Sync>,
        flight_repo: Arc<dyn FlightRepository + Send + Sync>,
        generation_rule_repo: Arc<dyn FlightGenerationRuleRepository + Send + Sync>,
        adjustment_rule_repo: Arc<dyn GenerationAdjustmentRuleRepository + Send + Sync>,
    ) -> Self {
        self.rules.department_repo = Some(department_repo);
        self.rules.flight_repo = Some(flight_repo);
        self.rules.generation_rule_repo = Some(generation_rule_repo);
        self.rules.adjustment_rule_repo = Some(adjustment_rule_repo);
        self
    }

    pub fn with_publication_preparation_repos(
        mut self,
        qualification_repo: Arc<dyn DepartmentQualificationRepository + Send + Sync>,
        qualification_grant_repo: Arc<dyn QualificationGrantRepository + Send + Sync>,
        equipment_repo: Arc<dyn EquipmentRepository + Send + Sync>,
    ) -> Self {
        self.resources.qualification_repo = Some(qualification_repo);
        self.resources.qualification_grant_repo = Some(qualification_grant_repo);
        self.resources.equipment_repo = Some(equipment_repo);
        self
    }

    pub fn with_member_repos(
        mut self,
        member_repo: Arc<dyn DispatchOrderMemberRepository + Send + Sync>,
        team_member_repo: Arc<dyn TeamMemberRepository + Send + Sync>,
        travel_stats_repo: Arc<dyn DispatchTravelStatsRepository + Send + Sync>,
        checklist_repo: Arc<dyn DispatchChecklistRepository + Send + Sync>,
    ) -> Self {
        self.order.member_repo = Some(member_repo);
        self.resources.team_member_repo = Some(team_member_repo);
        self.resources.travel_stats_repo = Some(travel_stats_repo);
        self.resources.checklist_repo = Some(checklist_repo);
        self
    }

    pub fn with_transactional_repos(
        mut self,
        order_tx_repo: Arc<dyn SqlxDispatchOrderTransactionalRepository>,
        member_tx_repo: Option<Arc<dyn SqlxDispatchOrderMemberTransactionalRepository>>,
    ) -> Self {
        self.order.order_tx_repo = Some(order_tx_repo);
        self.order.member_tx_repo = member_tx_repo;
        self
    }

    pub fn with_issue_reporting(mut self, anomaly_repo: Arc<dyn AnomalyRepository + Send + Sync>) -> Self {
        self.notifications.anomaly_repo = Some(anomaly_repo);
        self
    }

    pub fn with_collaboration_repo(
        mut self,
        collaboration_repo: Arc<dyn DispatchCollaborationRepository + Send + Sync>,
    ) -> Self {
        self.notifications.collaboration_repo = Some(collaboration_repo);
        self
    }

    pub fn with_alert_repo(mut self, alert_repo: Arc<dyn DispatchAlertRepository + Send + Sync>) -> Self {
        self.notifications.alert_repo = Some(alert_repo);
        self
    }

    pub fn with_notification_service(
        mut self,
        notification_service: Arc<dyn DispatchNotificationSender + Send + Sync>,
    ) -> Self {
        self.notifications.notification_service = Some(notification_service);
        self
    }

    pub fn with_dispatch_chat_service(
        mut self,
        dispatch_chat_service: Arc<dyn DispatchChatOrderSyncer + Send + Sync>,
    ) -> Self {
        self.notifications.dispatch_chat_service = Some(dispatch_chat_service);
        self
    }

    pub fn with_resource_availability_service(
        mut self,
        resource_availability_service: Arc<dyn ResourceAvailabilityGateway + Send + Sync>,
    ) -> Self {
        self.resources.resource_availability_service = Some(resource_availability_service);
        self
    }

    pub fn with_todo_repo(mut self, todo_repo: Arc<dyn TodoRepository + Send + Sync>) -> Self {
        self.order.todo_repo = Some(todo_repo);
        self
    }

    pub fn with_overrun_warning_service(
        mut self,
        overrun_warning_service: Arc<dispatch_overrun_warning_service::DispatchOverrunWarningService>,
    ) -> Self {
        self.overrun_warning_service = Some(overrun_warning_service);
        self
    }

    /// 订单生命周期后 best-effort 触发预排冲突评估;失败只记日志,不阻断主流程。
    pub(super) async fn maybe_evaluate_overrun_warning(&self, order_id: &str) {
        let Some(service) = self.overrun_warning_service.as_ref() else {
            return;
        };
        if let Err(error) = service.evaluate_order(order_id).await {
            tracing::warn!(
                order_id,
                error = %error,
                "dispatch overrun warning evaluation failed after order lifecycle change"
            );
        }
    }
}
