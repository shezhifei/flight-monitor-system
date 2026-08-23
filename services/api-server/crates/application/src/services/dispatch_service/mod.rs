//! 派工应用服务：工单生命周期、生成、发布、移动作业。

pub mod dispatch_overrun_warning_service;
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
impl DispatchChatOrderSyncer for DispatchChatService {
    async fn sync_dispatch_order_chat(&self, order_id: &str) {
        if let Err(error) = self.sync_group_for_dispatch_order_id(order_id).await {
            tracing::warn!(order_id, error = %error, "failed to sync dispatch chat group");
        }
    }
}


/// 派工应用服务
pub struct DispatchService {
    order: DispatchOrderServiceDependencies,
    rules: DispatchRuleServiceDependencies,
    resources: DispatchResourceServiceDependencies,
    notifications: DispatchNotificationServiceDependencies,
    analytics: DispatchAnalyticsServiceDependencies,
}

struct DispatchOrderServiceDependencies {
    order_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
    order_tx_repo: Arc<dyn SqlxDispatchOrderTransactionalRepository>,
    member_repo: Arc<dyn DispatchOrderMemberRepository + Send + Sync>,
    member_tx_repo: Arc<dyn SqlxDispatchOrderMemberTransactionalRepository>,
    todo_repo: Arc<dyn TodoRepository + Send + Sync>,
}

struct DispatchRuleServiceDependencies {
    department_repo: Arc<dyn DepartmentRepository + Send + Sync>,
    task_type_repo: Arc<dyn TaskTypeRepository + Send + Sync>,
    task_type_requirement_repo: Arc<dyn DepartmentTaskTypeRequirementRepository + Send + Sync>,
    flight_repo: Arc<dyn FlightRepository + Send + Sync>,
    generation_rule_repo: Arc<dyn FlightGenerationRuleRepository + Send + Sync>,
    adjustment_rule_repo: Arc<dyn GenerationAdjustmentRuleRepository + Send + Sync>,
    temporary_task_template_repo: Arc<dyn TemporaryTaskTemplateRepository + Send + Sync>,
}

impl DispatchRuleServiceDependencies {
    pub fn new(
        department_repo: Arc<dyn DepartmentRepository + Send + Sync>,
        task_type_repo: Arc<dyn TaskTypeRepository + Send + Sync>,
        task_type_requirement_repo: Arc<dyn DepartmentTaskTypeRequirementRepository + Send + Sync>,
        flight_repo: Arc<dyn FlightRepository + Send + Sync>,
        generation_rule_repo: Arc<dyn FlightGenerationRuleRepository + Send + Sync>,
        adjustment_rule_repo: Arc<dyn GenerationAdjustmentRuleRepository + Send + Sync>,
        temporary_task_template_repo: Arc<dyn TemporaryTaskTemplateRepository + Send + Sync>,
    ) -> Self {
        Self {
            department_repo,
            task_type_repo,
            task_type_requirement_repo,
            flight_repo,
            generation_rule_repo,
            adjustment_rule_repo,
            temporary_task_template_repo,
        }
    }
}

struct DispatchResourceServiceDependencies {
    team_repo: Arc<dyn TeamRepository + Send + Sync>,
    team_type_repo: Arc<dyn TeamTypeRepository + Send + Sync>,
    stand_repo: Arc<dyn StandRepository + Send + Sync>,
    qualification_repo: Arc<dyn DepartmentQualificationRepository + Send + Sync>,
    qualification_grant_repo: Arc<dyn QualificationGrantRepository + Send + Sync>,
    equipment_repo: Arc<dyn EquipmentRepository + Send + Sync>,
    team_member_repo: Arc<dyn TeamMemberRepository + Send + Sync>,
    travel_stats_repo: Arc<dyn DispatchTravelStatsRepository + Send + Sync>,
    checklist_repo: Arc<dyn DispatchChecklistRepository + Send + Sync>,
    resource_availability_service: Arc<dyn ResourceAvailabilityGateway + Send + Sync>,
}

impl DispatchResourceServiceDependencies {
    pub fn new(
        team_repo: Arc<dyn TeamRepository + Send + Sync>,
        team_type_repo: Arc<dyn TeamTypeRepository + Send + Sync>,
        stand_repo: Arc<dyn StandRepository + Send + Sync>,
        qualification_repo: Arc<dyn DepartmentQualificationRepository + Send + Sync>,
        qualification_grant_repo: Arc<dyn QualificationGrantRepository + Send + Sync>,
        equipment_repo: Arc<dyn EquipmentRepository + Send + Sync>,
        team_member_repo: Arc<dyn TeamMemberRepository + Send + Sync>,
        travel_stats_repo: Arc<dyn DispatchTravelStatsRepository + Send + Sync>,
        checklist_repo: Arc<dyn DispatchChecklistRepository + Send + Sync>,
        resource_availability_service: Arc<dyn ResourceAvailabilityGateway + Send + Sync>,
    ) -> Self {
        Self {
            team_repo,
            team_type_repo,
            stand_repo,
            qualification_repo,
            qualification_grant_repo,
            equipment_repo,
            team_member_repo,
            travel_stats_repo,
            checklist_repo,
            resource_availability_service,
        }
    }
}

struct DispatchNotificationServiceDependencies {
    anomaly_repo: Arc<dyn AnomalyRepository + Send + Sync>,
    collaboration_repo: Arc<dyn DispatchCollaborationRepository + Send + Sync>,
    alert_repo: Arc<dyn DispatchAlertRepository + Send + Sync>,
    notification_service: Arc<dyn DispatchNotificationSender + Send + Sync>,
    dispatch_chat_service: Arc<dyn DispatchChatOrderSyncer + Send + Sync>,
}

impl DispatchNotificationServiceDependencies {
    pub fn new(
        anomaly_repo: Arc<dyn AnomalyRepository + Send + Sync>,
        collaboration_repo: Arc<dyn DispatchCollaborationRepository + Send + Sync>,
        alert_repo: Arc<dyn DispatchAlertRepository + Send + Sync>,
        notification_service: Arc<dyn DispatchNotificationSender + Send + Sync>,
        dispatch_chat_service: Arc<dyn DispatchChatOrderSyncer + Send + Sync>,
    ) -> Self {
        Self {
            anomaly_repo,
            collaboration_repo,
            alert_repo,
            notification_service,
            dispatch_chat_service,
        }
    }
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
    pub fn new(
        order_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
        order_tx_repo: Arc<dyn SqlxDispatchOrderTransactionalRepository>,
        member_repo: Arc<dyn DispatchOrderMemberRepository + Send + Sync>,
        member_tx_repo: Arc<dyn SqlxDispatchOrderMemberTransactionalRepository>,
        todo_repo: Arc<dyn TodoRepository + Send + Sync>,
        department_repo: Arc<dyn DepartmentRepository + Send + Sync>,
        task_type_repo: Arc<dyn TaskTypeRepository + Send + Sync>,
        task_type_requirement_repo: Arc<dyn DepartmentTaskTypeRequirementRepository + Send + Sync>,
        flight_repo: Arc<dyn FlightRepository + Send + Sync>,
        generation_rule_repo: Arc<dyn FlightGenerationRuleRepository + Send + Sync>,
        adjustment_rule_repo: Arc<dyn GenerationAdjustmentRuleRepository + Send + Sync>,
        temporary_task_template_repo: Arc<dyn TemporaryTaskTemplateRepository + Send + Sync>,
        team_repo: Arc<dyn TeamRepository + Send + Sync>,
        team_type_repo: Arc<dyn TeamTypeRepository + Send + Sync>,
        stand_repo: Arc<dyn StandRepository + Send + Sync>,
        qualification_repo: Arc<dyn DepartmentQualificationRepository + Send + Sync>,
        qualification_grant_repo: Arc<dyn QualificationGrantRepository + Send + Sync>,
        equipment_repo: Arc<dyn EquipmentRepository + Send + Sync>,
        team_member_repo: Arc<dyn TeamMemberRepository + Send + Sync>,
        travel_stats_repo: Arc<dyn DispatchTravelStatsRepository + Send + Sync>,
        checklist_repo: Arc<dyn DispatchChecklistRepository + Send + Sync>,
        resource_availability_service: Arc<dyn ResourceAvailabilityGateway + Send + Sync>,
        anomaly_repo: Arc<dyn AnomalyRepository + Send + Sync>,
        collaboration_repo: Arc<dyn DispatchCollaborationRepository + Send + Sync>,
        alert_repo: Arc<dyn DispatchAlertRepository + Send + Sync>,
        notification_service: Arc<dyn DispatchNotificationSender + Send + Sync>,
        dispatch_chat_service: Arc<dyn DispatchChatOrderSyncer + Send + Sync>,
    ) -> Self {
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
                order_tx_repo,
                member_repo,
                member_tx_repo,
                todo_repo,
            },
            rules: DispatchRuleServiceDependencies::new(
                department_repo,
                task_type_repo,
                task_type_requirement_repo,
                flight_repo,
                generation_rule_repo,
                adjustment_rule_repo,
                temporary_task_template_repo,
            ),
            resources: DispatchResourceServiceDependencies::new(
                team_repo,
                team_type_repo,
                stand_repo,
                qualification_repo,
                qualification_grant_repo,
                equipment_repo,
                team_member_repo,
                travel_stats_repo,
                checklist_repo,
                resource_availability_service,
            ),
            notifications: DispatchNotificationServiceDependencies::new(
                anomaly_repo,
                collaboration_repo,
                alert_repo,
                notification_service,
                dispatch_chat_service,
            ),
            analytics: DispatchAnalyticsServiceDependencies {
                metrics_counters: metrics,
            },
        }
    }


}
