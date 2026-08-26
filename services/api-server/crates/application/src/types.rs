//! Concrete type aliases for all generic services.
//!
//! These aliases bind the generic service types to the Postgres repository
//! implementations used in production, allowing other services to reference
//! them without repeating the full generic parameter lists.

use std::future::Future;
use std::pin::Pin;

use async_trait::async_trait;

use fms_domain::error::DomainError;
use fms_domain::ports::business_case_repository::BusinessCaseRepository;
use fms_domain::ports::dispatch_repository::DispatchOrderRepository;
use fms_domain::ports::notification_repository::{NotificationPreferenceRepository, NotificationRepository};
use fms_domain::ports::user_repository::UserRepository;
use fms_domain::ports::workflow_dispatch_repository::WorkflowDispatchRepository;

use crate::services::ai_action_proposal_service::AiActionProposalService;
use crate::services::anomaly_service::AnomalyService;
use crate::services::auth_service::AuthService;
use crate::services::business_case_service::BusinessCaseService;
use crate::services::business_case_service::{BusinessCaseEventPublisher, BusinessCaseMentionAudience};
use crate::services::business_case_type_service::BusinessCaseTypeService;
use crate::services::business_case_workflow_service::BusinessCaseWorkflowService;
use crate::services::dashboard_workbench_service::DashboardWorkbenchService;
use crate::services::dispatch_analytics_service::DispatchAnalyticsService;
use crate::services::dispatch_chat_service::DispatchChatEventPublisher;
use crate::services::dispatch_chat_service::DispatchChatService;
use crate::services::dispatch_query_service::DispatchQueryService;
use crate::services::flight_service::FlightService;
use crate::services::label_service::LabelService;
use crate::services::mobile_device_service::MobileDeviceService;
use crate::services::mobile_device_service::MobileRealtimeMetricsRecorder;
use crate::services::mobile_operations_service::MobileOperationsService;
use crate::services::mobile_workbench_service::MobileWorkbenchService;
use crate::services::nl_query_service::NLQueryService;
use crate::services::notification_service::{
    NotificationCollaborationEvents, NotificationDeliveryPublisher, NotificationMetricsRecorder,
    NotificationReceiptGroupSync, NotificationResponse, NotificationService,
};
use crate::services::resource_utilization_service::ResourceUtilizationService;
use crate::services::shift_handover_service::ShiftHandoverService;
use crate::services::system_ops_service::SystemOpsService;
use crate::services::terminal_resource_service::TerminalResourceService;
use crate::services::todo_service::TodoService;
use crate::services::workflow_dispatch_service::{
    DispatchRecommendationService, StructuredRecommendationCandidate, WorkflowDispatchService,
    WorkflowDispatchSsePublisher,
};

// 显式的「此处不做这件事」实现。以前它们是默认类型参数的填充物（忘记接线和
// 故意不接是同一个状态）；现在必须由构造点点名传入，二者在装配代码里可见地分开。
pub struct NoopNotificationDeliveryPublisher;

impl NotificationDeliveryPublisher for NoopNotificationDeliveryPublisher {
    fn publish_user_notification<'a>(
        &'a self,
        _notification: &'a NotificationResponse,
        _unread_count: i64,
    ) -> Pin<Box<dyn Future<Output = Result<usize, DomainError>> + Send + 'a>> {
        Box::pin(async { Ok(0) })
    }

    fn publish_sender_receipt_update<'a>(
        &'a self,
        _sender_user_id: &'a str,
        _payload: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<usize, DomainError>> + Send + 'a>> {
        Box::pin(async { Ok(0) })
    }
}

pub struct NoopNotificationMetricsRecorder;

impl NotificationMetricsRecorder for NoopNotificationMetricsRecorder {
    fn record_delivery_attempt(&self, _channel: &str, _success: bool) {}
    fn record_backfill_pending(&self) {}
}

pub struct NoopNotificationReceiptGroupSync;

impl NotificationReceiptGroupSync for NoopNotificationReceiptGroupSync {
    fn sync_receipt_group<'a>(
        &'a self,
        _receipt_group_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), DomainError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
}

// No-op implementations for WorkflowDispatchService default type parameters
pub struct NoopWorkflowDispatchSsePublisher;

impl WorkflowDispatchSsePublisher for NoopWorkflowDispatchSsePublisher {
    fn publish_system_alert<'a>(
        &'a self,
        _event_name: &'a str,
        _payload: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async {})
    }

    fn publish_ai_event<'a>(
        &'a self,
        _event_name: &'a str,
        _payload: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async {})
    }
}

pub struct NoopDispatchRecommendationService;

impl DispatchRecommendationService for NoopDispatchRecommendationService {
    fn recommend<'a>(
        &'a self,
        _department: &'a str,
        _task_type: &'a str,
        _target_job_title: Option<&'a str>,
        _required_people: i32,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<StructuredRecommendationCandidate>, DomainError>> + Send + 'a>> {
        Box::pin(async { Ok(vec![]) })
    }
}

// 显式 no-op 事件发布器：给不关心事件外发的测试与 DI 分支使用。
// （以前它还兼任默认类型参数的填充物，默认参数已删除。）
pub struct NoopBusinessCaseEventPublisher;

impl BusinessCaseEventPublisher for NoopBusinessCaseEventPublisher {
    fn publish_appended<'a>(
        &'a self,
        _business_case: &'a fms_domain::models::business_case::FlightBusinessCase,
        _append_entry_id: &'a str,
        _operator: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), DomainError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }

    fn publish_updated<'a>(
        &'a self,
        _business_case: &'a fms_domain::models::business_case::FlightBusinessCase,
        _event_name: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), DomainError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
}

// No-op implementations for DispatchChatService default type parameters
pub struct NoopDispatchChatEventPublisher;

impl DispatchChatEventPublisher for NoopDispatchChatEventPublisher {
    fn publish_user_event<'a>(
        &'a self,
        _event_name: &'a str,
        _events: Vec<(String, serde_json::Value)>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async {})
    }
}

// No-op implementations for MobileDeviceService default type parameters
pub struct NoopMobileRealtimeMetricsRecorder;

impl MobileRealtimeMetricsRecorder for NoopMobileRealtimeMetricsRecorder {
    fn record_sse_reconnects(&self, _count: u64) {}
}

// No-op broadcaster for LabelService type alias
pub struct NoopBroadcaster;

#[async_trait]
impl fms_domain::broadcaster::Broadcaster for NoopBroadcaster {
    async fn broadcast_event(&self, _topic: &str, _event_name: Option<&str>, _payload: serde_json::Value) {}
}

// Service type aliases
pub type ConcreteAuthService = AuthService;

pub type ConcreteFlightService = FlightService;

pub type ConcreteNotificationService = NotificationService<
    dyn NotificationRepository + Send + Sync,
    dyn NotificationPreferenceRepository + Send + Sync,
    dyn NotificationCollaborationEvents,
    dyn NotificationDeliveryPublisher,
    dyn NotificationMetricsRecorder,
    dyn NotificationReceiptGroupSync,
>;

pub type ConcreteTodoService = TodoService;

pub type ConcreteAnomalyService = AnomalyService;

pub type ConcreteDispatchQueryService = DispatchQueryService;

pub type ConcreteLabelService = LabelService;

pub type ConcreteBusinessCaseService = BusinessCaseService<
    dyn BusinessCaseRepository + Send + Sync,
    dyn BusinessCaseEventPublisher,
    dyn BusinessCaseMentionAudience,
>;

pub type ConcreteBusinessCaseTypeService = BusinessCaseTypeService;

pub type ConcreteWorkflowDispatchService = WorkflowDispatchService<
    dyn DispatchOrderRepository + Send + Sync,
    dyn UserRepository + Send + Sync,
    dyn WorkflowDispatchRepository + Send + Sync,
    dyn WorkflowDispatchSsePublisher,
    dyn DispatchRecommendationService,
>;

pub type ConcreteBusinessCaseWorkflowService = BusinessCaseWorkflowService;

pub type ConcreteShiftHandoverService = ShiftHandoverService;

pub type ConcreteMobileDeviceService = MobileDeviceService;

pub type ConcreteResourceUtilizationService = ResourceUtilizationService;

pub type ConcreteDispatchAnalyticsService = DispatchAnalyticsService;

pub type ConcreteDispatchChatService = DispatchChatService;

pub type ConcreteMobileWorkbenchService = MobileWorkbenchService;

pub type ConcreteMobileOperationsService = MobileOperationsService;

pub type ConcreteDashboardWorkbenchService = DashboardWorkbenchService;

pub type ConcreteSystemOpsService = SystemOpsService;

pub type ConcreteNLQueryService = NLQueryService;

pub type ConcreteAiActionProposalService = AiActionProposalService;

use crate::services::dispatch_resource_service::DispatchResourceService;
use fms_domain::ports::dispatch_repository::{
    DepartmentRepository, EquipmentRepository, EquipmentTypeRepository, StandRepository, TaskTypeRepository,
    TeamMemberRepository, TeamRepository, TeamTypeRepository,
};

pub type ConcreteDispatchResourceService = DispatchResourceService<
    dyn DepartmentRepository + Send + Sync,
    dyn TeamTypeRepository + Send + Sync,
    dyn TeamRepository + Send + Sync,
    dyn TeamMemberRepository + Send + Sync,
    dyn EquipmentTypeRepository + Send + Sync,
    dyn EquipmentRepository + Send + Sync,
    dyn StandRepository + Send + Sync,
    dyn TaskTypeRepository + Send + Sync,
>;

/// 空间目录资源服务，绑定 Postgres 仓储。
pub type ConcreteTerminalResourceService = TerminalResourceService<dyn TerminalRepository + Send + Sync>;

use crate::services::dispatch_schedule_service::DispatchScheduleService;
use crate::services::resource_availability_service::ResourceAvailabilityGateway;
use fms_domain::ports::dispatch_repository::{
    ScheduleExceptionRepository, ShiftInstanceRepository, ShiftTemplateRepository, TerminalRepository,
};

/// 排班服务的生产单态。API 处理器与 DI 必须引用同一个别名——
/// 此前处理器写的是裸 `DispatchScheduleService`，默认类型参数把它解析成一组空实现桩，
/// 与 DI 注册的类型不是同一个单态，`web::Data` 取不到，7 个排班端点全部 500。
pub type ConcreteDispatchScheduleService = DispatchScheduleService<
    dyn ShiftTemplateRepository + Send + Sync,
    dyn ShiftInstanceRepository + Send + Sync,
    dyn ScheduleExceptionRepository + Send + Sync,
    dyn TeamRepository + Send + Sync,
    dyn TeamMemberRepository + Send + Sync,
    dyn EquipmentRepository + Send + Sync,
    dyn ResourceAvailabilityGateway + Send + Sync,
>;
