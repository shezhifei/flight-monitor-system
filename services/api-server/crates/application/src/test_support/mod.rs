//! 测试专用桩件。仅在 `cfg(test)` 或 `test-support` feature 下编译，不进入生产二进制。
//!
//! 存在的唯一理由：`DispatchServiceDependencies` 的字段全部必填——这是刻意的，漏接依赖
//! 必须是编译错误而不是运行期 500。测试通常只关心其中一两个端口，其余用**会报错的桩**
//! 填充（[`UnwiredRepository`]）。桩被真的调用时错误会点名端口，所以「这个测试到底依赖
//! 了什么」是显式的：
//!
//! ```ignore
//! let mut deps = stub_dispatch_dependencies();
//! deps.order.order_repo = my_recording_repo.clone();   // 本测试只用到这一个端口
//! let service = DispatchService::new(deps);
//! ```

mod unwired_repository;

use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::{Equipment, Team};
use fms_domain::ports::notification_repository::{NotificationPreferenceRepository, NotificationRepository};

use crate::services::dispatch_service::dispatch_overrun_warning_service::DispatchOverrunWarningService;
use crate::services::dispatch_service::{
    DispatchChatOrderSyncer, DispatchNotificationSender, DispatchNotificationServiceDependencies,
    DispatchOrderServiceDependencies, DispatchResourceServiceDependencies, DispatchRuleServiceDependencies,
    DispatchServiceDependencies,
};
use crate::services::notification_service::{
    DispatchBatchNotificationCreate, NoCollaborationEvents, NotificationService,
};
use crate::services::resource_availability_service::{ResourceAvailability, ResourceAvailabilityGateway};
use crate::types::{
    NoopNotificationDeliveryPublisher, NoopNotificationMetricsRecorder, NoopNotificationReceiptGroupSync,
};

pub use unwired_repository::UnwiredRepository;

fn unwired(port: &str) -> DomainError {
    DomainError::Internal(format!("test stub: {port} was not wired for this test"))
}

/// 未接线的资源可用性网关：任何调用都失败并点名自己。
pub struct UnwiredResourceAvailability;

impl ResourceAvailabilityGateway for UnwiredResourceAvailability {
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
        Box::pin(async { Err(unwired("ResourceAvailabilityGateway::list_team_availability")) })
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
        Box::pin(async { Err(unwired("ResourceAvailabilityGateway::evaluate_equipment")) })
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
        Box::pin(async { Err(unwired("ResourceAvailabilityGateway::list_employee_availability")) })
    }
}

/// 未接线的派工通知发送器：任何调用都失败并点名自己。
pub struct UnwiredNotificationSender;

#[async_trait]
impl DispatchNotificationSender for UnwiredNotificationSender {
    async fn send_dispatch_batch(
        &self,
        _dto: DispatchBatchNotificationCreate,
    ) -> Result<serde_json::Value, DomainError> {
        Err(unwired("DispatchNotificationSender::send_dispatch_batch"))
    }
}

/// 未接线的群聊同步器。该端口返回 `()`，无法报错，因此只能静默——与接线前的行为一致。
pub struct UnwiredChatSyncer;

#[async_trait]
impl DispatchChatOrderSyncer for UnwiredChatSyncer {
    async fn sync_dispatch_order_chat(&self, _order_id: &str) {}
}

/// 全端口皆为桩的 `DispatchServiceDependencies`。按需覆盖本测试真正用到的字段。
///
/// 预排冲突预警的 feature flag 显式关掉，桩服务不会被扫描逻辑唤醒。
pub fn stub_dispatch_dependencies() -> DispatchServiceDependencies {
    let unwired_repo = Arc::new(UnwiredRepository);
    DispatchServiceDependencies {
        order: DispatchOrderServiceDependencies {
            order_repo: unwired_repo.clone(),
            order_tx_repo: unwired_repo.clone(),
            member_repo: unwired_repo.clone(),
            member_tx_repo: unwired_repo.clone(),
            todo_repo: unwired_repo.clone(),
        },
        rules: DispatchRuleServiceDependencies {
            department_repo: unwired_repo.clone(),
            task_type_repo: unwired_repo.clone(),
            task_type_requirement_repo: unwired_repo.clone(),
            flight_repo: unwired_repo.clone(),
            generation_rule_repo: unwired_repo.clone(),
            adjustment_rule_repo: unwired_repo.clone(),
            temporary_task_template_repo: unwired_repo.clone(),
        },
        resources: DispatchResourceServiceDependencies {
            team_repo: unwired_repo.clone(),
            team_type_repo: unwired_repo.clone(),
            stand_repo: unwired_repo.clone(),
            qualification_repo: unwired_repo.clone(),
            qualification_grant_repo: unwired_repo.clone(),
            equipment_repo: unwired_repo.clone(),
            team_member_repo: unwired_repo.clone(),
            travel_stats_repo: unwired_repo.clone(),
            checklist_repo: unwired_repo.clone(),
            resource_availability_service: Arc::new(UnwiredResourceAvailability),
        },
        notifications: DispatchNotificationServiceDependencies {
            anomaly_repo: unwired_repo.clone(),
            collaboration_repo: unwired_repo.clone(),
            alert_repo: unwired_repo.clone(),
            notification_service: Arc::new(UnwiredNotificationSender),
            dispatch_chat_service: Arc::new(UnwiredChatSyncer),
        },
        overrun_warning_service: Arc::new(
            DispatchOverrunWarningService::new(unwired_repo.clone(), unwired_repo.clone()).with_feature_flags(false, false),
        ),
    }
}

/// 四个副作用端口都显式为「不做」的 `NotificationService`。
///
/// 这不是「默认值」：类型名里写着 `WithoutSideChannels`，读测试的人一眼能看出这个
/// 服务不会投递、不打点、不落协作事件、不同步回执组。生产装配拿不到它——`di::shared`
/// 必须点名传入真实现。
pub type NotificationServiceWithoutSideChannels<NR, PR> = NotificationService<
    NR,
    PR,
    NoCollaborationEvents,
    NoopNotificationDeliveryPublisher,
    NoopNotificationMetricsRecorder,
    NoopNotificationReceiptGroupSync,
>;

/// 建一个只接两个必需仓储、副作用端口全部显式关掉的通知服务。
///
/// 给「只关心存取行为」的测试用。要断言投递/协作流的测试请直接调
/// `NotificationService::new`，把真桩件放在对应位置上——哪个端口被测试到，从调用处可见。
pub fn notification_service_without_side_channels<NR, PR>(
    repo: Arc<NR>,
    preference_repo: Arc<PR>,
) -> NotificationServiceWithoutSideChannels<NR, PR>
where
    NR: NotificationRepository + ?Sized,
    PR: NotificationPreferenceRepository + ?Sized,
{
    NotificationService::new(
        repo,
        preference_repo,
        Arc::new(NoCollaborationEvents),
        Arc::new(NoopNotificationDeliveryPublisher),
        Arc::new(NoopNotificationMetricsRecorder),
        Arc::new(NoopNotificationReceiptGroupSync),
    )
}
