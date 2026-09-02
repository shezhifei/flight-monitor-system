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
pub mod writer;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

static NULL_VALUE: serde_json::Value = serde_json::Value::Null;

use crate::services::notification_service::NotificationCollaborationEvents;
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
use fms_domain::ports::field_overlay_repository::FieldOverlayRepository;

use async_trait::async_trait;

use crate::services::dispatch_chat_service::DispatchChatService;
use crate::services::dispatch_order_adjuster_handler::EventRuleOrderGateway;
use crate::services::notification_service::{DispatchBatchNotificationCreate, NotificationService};
use crate::services::resource_availability_service::ResourceAvailabilityGateway;
use crate::services::attribute_validation::ObjectReferenceValidator;

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
        CE: NotificationCollaborationEvents + Send + Sync + ?Sized,
        DP: crate::services::notification_service::NotificationDeliveryPublisher + Send + Sync + ?Sized,
        MR: crate::services::notification_service::NotificationMetricsRecorder + Send + Sync + ?Sized,
        RS: crate::services::notification_service::NotificationReceiptGroupSync + Send + Sync + ?Sized,
    > DispatchNotificationSender for NotificationService<NR, PR, CE, DP, MR, RS>
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
    /// 预排冲突预警。服务内部自带 feature flag，接线在 DI 层恒定完成。
    overrun_warning_service: Arc<dispatch_overrun_warning_service::DispatchOverrunWarningService>,
}

/// `DispatchService` 的全部外部依赖。字段全部必填：漏接一个依赖是编译错误，不是运行期 500。
/// 具名字段同时消除了同类型参数被顺序调换而静默生效的风险。
pub struct DispatchServiceDependencies {
    pub order: DispatchOrderServiceDependencies,
    pub rules: DispatchRuleServiceDependencies,
    pub resources: DispatchResourceServiceDependencies,
    pub notifications: DispatchNotificationServiceDependencies,
    pub overrun_warning_service: Arc<dispatch_overrun_warning_service::DispatchOverrunWarningService>,
}

pub struct DispatchOrderServiceDependencies {
    pub order_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
    pub member_repo: Arc<dyn DispatchOrderMemberRepository + Send + Sync>,
    pub todo_repo: Arc<dyn TodoRepository + Send + Sync>,
    pub field_overlay_repo: Option<Arc<dyn FieldOverlayRepository + Send + Sync>>,
    pub object_reference_validator: Option<Arc<dyn ObjectReferenceValidator>>,
}

pub struct DispatchRuleServiceDependencies {
    pub department_repo: Arc<dyn DepartmentRepository + Send + Sync>,
    pub task_type_repo: Arc<dyn TaskTypeRepository + Send + Sync>,
    pub task_type_requirement_repo: Arc<dyn DepartmentTaskTypeRequirementRepository + Send + Sync>,
    pub flight_repo: Arc<dyn FlightRepository + Send + Sync>,
    pub generation_rule_repo: Arc<dyn FlightGenerationRuleRepository + Send + Sync>,
    pub adjustment_rule_repo: Arc<dyn GenerationAdjustmentRuleRepository + Send + Sync>,
    pub temporary_task_template_repo: Arc<dyn TemporaryTaskTemplateRepository + Send + Sync>,
}

pub struct DispatchResourceServiceDependencies {
    pub team_repo: Arc<dyn TeamRepository + Send + Sync>,
    pub team_type_repo: Arc<dyn TeamTypeRepository + Send + Sync>,
    pub stand_repo: Arc<dyn StandRepository + Send + Sync>,
    pub qualification_repo: Arc<dyn DepartmentQualificationRepository + Send + Sync>,
    pub qualification_grant_repo: Arc<dyn QualificationGrantRepository + Send + Sync>,
    pub equipment_repo: Arc<dyn EquipmentRepository + Send + Sync>,
    pub team_member_repo: Arc<dyn TeamMemberRepository + Send + Sync>,
    pub travel_stats_repo: Arc<dyn DispatchTravelStatsRepository + Send + Sync>,
    pub checklist_repo: Arc<dyn DispatchChecklistRepository + Send + Sync>,
    pub resource_availability_service: Arc<dyn ResourceAvailabilityGateway + Send + Sync>,
}

pub struct DispatchNotificationServiceDependencies {
    pub anomaly_repo: Arc<dyn AnomalyRepository + Send + Sync>,
    pub collaboration_repo: Arc<dyn DispatchCollaborationRepository + Send + Sync>,
    pub alert_repo: Arc<dyn DispatchAlertRepository + Send + Sync>,
    pub notification_service: Arc<dyn DispatchNotificationSender + Send + Sync>,
    pub dispatch_chat_service: Arc<dyn DispatchChatOrderSyncer + Send + Sync>,
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
    pub fn new(deps: DispatchServiceDependencies) -> Self {
        let metrics = DashMap::new();
        metrics.insert("dispatch.order.complete.blocked".to_string(), 0);
        metrics.insert("dispatch.order.complete.soft".to_string(), 0);
        metrics.insert("dispatch.order.arrival.pending_verification".to_string(), 0);
        metrics.insert("dispatch.issue_reported.text".to_string(), 0);
        metrics.insert("dispatch.issue_reported.photo".to_string(), 0);
        metrics.insert("dispatch.issue_reported.voice".to_string(), 0);

        Self {
            order: deps.order,
            rules: deps.rules,
            resources: deps.resources,
            notifications: deps.notifications,
            analytics: DispatchAnalyticsServiceDependencies {
                metrics_counters: metrics,
            },
            overrun_warning_service: deps.overrun_warning_service,
        }
    }

    /// 订单生命周期变更后重新评估预排冲突预警。
    /// best-effort：评估失败只告警，不影响生命周期主流程。
    /// 是否启用由 DispatchOverrunWarningService 自身的 feature flag 决定。
    pub(super) async fn evaluate_overrun_warning(&self, order_id: &str) {
        if let Err(error) = self.overrun_warning_service.evaluate_order(order_id).await {
            tracing::warn!(
                order_id,
                error = %error,
                "dispatch overrun warning evaluation failed after order lifecycle change"
            );
        }
    }
}
