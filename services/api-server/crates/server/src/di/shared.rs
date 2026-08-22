//! 共享仓库、基础设施（SSE / MQ / Flowable / publishers / metrics）以及
//! 跨领域服务（notification / todo_scheduler / todo）的装配。
//!
//! 这些对象被 auth / flight / dispatch / business_case / ai / observability /
//! mobile 多个领域复用，因此集中构建后通过引用下发给各领域 builder。

use std::sync::Arc;

use crate::di::types::*;
use fms_api::services::performance_metrics::PerformanceMetricsService;
use fms_api::sse::hub::SseHub;

use fms_application::services::business_case_service::BusinessCaseEventPublisher;
use fms_application::services::flowable_service::FlowableService;
use fms_application::services::notification_service::{
    NotificationMetricsRecorder, NotificationReceiptGroupSync, NotificationService,
};
use fms_application::services::todo_scheduler_service::TodoSchedulerService;
use fms_application::services::todo_service::TodoService;
use fms_application::sqlx_transactional_repositories::{
    SqlxNotificationTransactionalRepository, SqlxTodoTransactionalRepository,
};

use fms_domain::ports::dispatch_collaboration_repository::DispatchCollaborationRepository;

use fms_domain::ports::notification_repository::{NotificationPreferenceRepository, NotificationRepository};
use fms_domain::ports::user_repository::{RoleRepository, UserRepository};

use fms_domain::ports::flowable_gateway::FlowableGateway;
use fms_infrastructure::integrations::embedded_flowable::EmbeddedFlowableEngine;
use fms_infrastructure::integrations::flowable_client::FlowableClient;
use fms_infrastructure::messaging::MessageQueueGatewayClient;
use fms_infrastructure::repositories::cached_user_repository::{CachedRoleRepository, CachedUserRepository};
use fms_infrastructure::repositories::pg_ai_copilot_repository::PgAiCopilotBusinessCaseBatchRepository;
use fms_infrastructure::repositories::pg_ai_entity_config_repository::PgAiEntityConfigRepository;
use fms_infrastructure::repositories::pg_anomaly_repository::PgAnomalyRepository;
use fms_infrastructure::repositories::pg_business_case_repository::PgBusinessCaseRepository;
use fms_infrastructure::repositories::pg_business_case_type_repository::PgBusinessCaseTypeRepository;
use fms_infrastructure::repositories::pg_business_case_workflow_run_repository::PgBusinessCaseWorkflowRunRepository;
use fms_infrastructure::repositories::pg_department_repository::PgDepartmentRepository;
use fms_infrastructure::repositories::pg_dispatch_alert_repository::PgDispatchAlertRepository;
use fms_infrastructure::repositories::pg_dispatch_checklist_repository::PgDispatchChecklistRepository;
use fms_infrastructure::repositories::pg_dispatch_collaboration_repository::PgDispatchCollaborationRepository;
use fms_infrastructure::repositories::pg_dispatch_order_member_repository::PgDispatchOrderMemberRepository;
use fms_infrastructure::repositories::pg_dispatch_order_repository::PgDispatchOrderRepository;
use fms_infrastructure::repositories::pg_dispatch_personnel_rules_repository::{
    PgDepartmentQualificationRepository, PgDepartmentTaskTypeRequirementRepository, PgFlightGenerationRuleRepository,
    PgGenerationAdjustmentRuleRepository, PgQualificationGrantRepository, PgTemporaryTaskTemplateRepository,
};
use fms_infrastructure::repositories::pg_dispatch_schedule_repository::{
    PgScheduleExceptionRepository, PgShiftInstanceRepository, PgShiftTemplateRepository,
};
use fms_infrastructure::repositories::pg_dispatch_travel_stats_repository::PgDispatchTravelStatsRepository;
use fms_infrastructure::repositories::pg_domain_event_outbox_repository::PgDomainEventOutboxRepository;
use fms_infrastructure::repositories::pg_equipment_repository::PgEquipmentRepository;
use fms_infrastructure::repositories::pg_equipment_type_repository::PgEquipmentTypeRepository;
use fms_infrastructure::repositories::pg_flight_archive_repository::PgFlightArchiveRepository;
use fms_infrastructure::repositories::pg_flight_repository::PgFlightRepository;
use fms_infrastructure::repositories::pg_flight_runtime_projection_repository::PgFlightRuntimeProjectionRepository;
use fms_infrastructure::repositories::pg_label_repository::PgLabelRepository;
use fms_infrastructure::repositories::pg_mobile_device_repository::PgMobileDeviceRepository;
use fms_infrastructure::repositories::pg_mobile_upload_repository::PgMobileUploadRepository;
use fms_infrastructure::repositories::pg_notification_repository::PgNotificationRepository;
use fms_infrastructure::repositories::pg_online_history_repository::PgOnlineHistoryRepository;
use fms_infrastructure::repositories::pg_operator_identity_repository::PgOperatorIdentityRepository;
use fms_infrastructure::repositories::pg_permission_repository::PgPermissionRepository;
use fms_infrastructure::repositories::pg_permission_template_repository::PgPermissionTemplateRepository;
use fms_infrastructure::repositories::pg_role_repository::PgRoleRepository;
use fms_infrastructure::repositories::pg_shift_handover_repository::PgShiftHandoverRepository;
use fms_infrastructure::repositories::pg_stand_repository::PgStandRepository;
use fms_infrastructure::repositories::pg_task_type_repository::PgTaskTypeRepository;
use fms_infrastructure::repositories::pg_team_member_repository::PgTeamMemberRepository;
use fms_infrastructure::repositories::pg_team_repository::PgTeamRepository;
use fms_infrastructure::repositories::pg_team_type_repository::PgTeamTypeRepository;
use fms_infrastructure::repositories::pg_todo_agent_context_repository::PgTodoAgentContextRepository;
use fms_infrastructure::repositories::pg_todo_repository::PgTodoRepository;
use fms_infrastructure::repositories::pg_user_repository::PgUserRepository;
use fms_infrastructure::repositories::pg_workflow_dispatch_repository::PgWorkflowDispatchRepository;
use fms_infrastructure::repositories::pg_workflow_form_repository::PgWorkflowFormRepository;
use fms_infrastructure::repositories::session_runtime_repository::InMemorySessionRuntimeRepository;

use crate::config::env_string;
use crate::di::adapters::*;

/// 所有 Postgres 仓库（+ 缓存型 user/role 仓库、session runtime 仓库）的集合。
pub(crate) struct SharedRepos {
    pub pool: sqlx::PgPool,
    pub flight_repo: Arc<PgFlightRepository>,
    pub label_repo: Arc<PgLabelRepository>,
    pub flight_archive_repo: Arc<PgFlightArchiveRepository>,
    pub user_repo: Arc<PgUserRepository>,
    pub role_repo: Arc<PgRoleRepository>,
    pub permission_repo: Arc<PgPermissionRepository>,
    pub todo_repo: Arc<PgTodoRepository>,
    pub todo_agent_context_repo: Arc<PgTodoAgentContextRepository>,
    pub dispatch_order_repo: Arc<PgDispatchOrderRepository>,
    pub dispatch_alert_repo: Arc<PgDispatchAlertRepository>,
    pub workflow_dispatch_repo: Arc<PgWorkflowDispatchRepository>,
    pub dispatch_collaboration_repo: Arc<PgDispatchCollaborationRepository>,
    pub mobile_device_repo: Arc<PgMobileDeviceRepository>,
    pub mobile_upload_repo: Arc<PgMobileUploadRepository>,
    pub notification_repo: Arc<PgNotificationRepository>,
    pub online_history_repo: Arc<PgOnlineHistoryRepository>,
    pub operator_identity_repo: Arc<PgOperatorIdentityRepository>,
    pub permission_template_repo: Arc<PgPermissionTemplateRepository>,
    pub ai_copilot_business_case_batch_repo: Arc<PgAiCopilotBusinessCaseBatchRepository>,
    pub auth_user_repo: Arc<dyn UserRepository + Send + Sync>,
    pub auth_role_repo: Arc<dyn RoleRepository + Send + Sync>,
    pub session_runtime_repo: Arc<InMemorySessionRuntimeRepository>,
    pub anomaly_repo: Arc<PgAnomalyRepository>,
    pub ai_entity_config_repo: Arc<PgAiEntityConfigRepository>,
    pub department_repo: Arc<PgDepartmentRepository>,
    pub team_type_repo: Arc<PgTeamTypeRepository>,
    pub team_repo: Arc<PgTeamRepository>,
    pub team_member_repo: Arc<PgTeamMemberRepository>,
    pub equipment_type_repo: Arc<PgEquipmentTypeRepository>,
    pub equipment_repo: Arc<PgEquipmentRepository>,
    pub stand_repo: Arc<PgStandRepository>,
    pub task_type_repo: Arc<PgTaskTypeRepository>,
    pub business_case_repo: Arc<PgBusinessCaseRepository>,
    pub flight_runtime_projection_repo: Arc<PgFlightRuntimeProjectionRepository>,
    pub business_case_type_repo: Arc<PgBusinessCaseTypeRepository>,
    pub business_case_workflow_run_repo: Arc<PgBusinessCaseWorkflowRunRepository>,
    pub shift_handover_repo: Arc<PgShiftHandoverRepository>,
    pub workflow_form_repo: Arc<PgWorkflowFormRepository>,
    pub dispatch_member_repo: Arc<PgDispatchOrderMemberRepository>,
    pub dispatch_travel_stats_repo: Arc<PgDispatchTravelStatsRepository>,
    pub dispatch_checklist_repo: Arc<PgDispatchChecklistRepository>,
    pub qualification_repo: Arc<PgDepartmentQualificationRepository>,
    pub qualification_grant_repo: Arc<PgQualificationGrantRepository>,
    pub task_type_requirement_repo: Arc<PgDepartmentTaskTypeRequirementRepository>,
    pub generation_rule_repo: Arc<PgFlightGenerationRuleRepository>,
    pub adjustment_rule_repo: Arc<PgGenerationAdjustmentRuleRepository>,
    pub temporary_task_template_repo: Arc<PgTemporaryTaskTemplateRepository>,
    pub shift_template_repo: Arc<PgShiftTemplateRepository>,
    pub shift_instance_repo: Arc<PgShiftInstanceRepository>,
    pub schedule_exception_repo: Arc<PgScheduleExceptionRepository>,
    /// Domain-event outbox repository. Held as the concrete Pg type so
    /// mark_published / mark_failed / claim_in_tx stay available; coerce to
    /// `Arc<dyn DomainEventOutboxRepository>` for port-only consumers.
    pub domain_event_outbox_repo: Arc<PgDomainEventOutboxRepository>,
}

pub(crate) fn build_shared_repos(
    pool: sqlx::PgPool,
    redis_manager: &Option<Arc<fms_infrastructure::cache::RedisPool>>,
) -> SharedRepos {
    let flight_repo = Arc::new(PgFlightRepository::new(pool.clone()));
    let label_repo = Arc::new(PgLabelRepository::new(pool.clone()));
    let flight_archive_repo = Arc::new(PgFlightArchiveRepository::new(pool.clone()));
    let user_repo = Arc::new(PgUserRepository::new(pool.clone()));
    let role_repo = Arc::new(PgRoleRepository::new(pool.clone()));
    let permission_repo = Arc::new(PgPermissionRepository::new(pool.clone()));
    let todo_repo = Arc::new(PgTodoRepository::new(pool.clone()));
    let todo_agent_context_repo = Arc::new(PgTodoAgentContextRepository::new(pool.clone()));
    let dispatch_order_repo = Arc::new(PgDispatchOrderRepository::new(pool.clone()));
    let dispatch_alert_repo = Arc::new(PgDispatchAlertRepository::new(pool.clone()));
    let workflow_dispatch_repo = Arc::new(PgWorkflowDispatchRepository::new(pool.clone()));
    let dispatch_collaboration_repo = Arc::new(PgDispatchCollaborationRepository::new(pool.clone()));
    let mobile_device_repo = Arc::new(PgMobileDeviceRepository::new(pool.clone()));
    let mobile_upload_repo = Arc::new(PgMobileUploadRepository::new(pool.clone()));
    let notification_repo = Arc::new(PgNotificationRepository::new(pool.clone()));
    let online_history_repo = Arc::new(PgOnlineHistoryRepository::new(pool.clone()));
    let operator_identity_repo = Arc::new(PgOperatorIdentityRepository::new(pool.clone()));
    let permission_template_repo = Arc::new(PgPermissionTemplateRepository::new(pool.clone()));
    let ai_copilot_business_case_batch_repo = Arc::new(PgAiCopilotBusinessCaseBatchRepository::new(pool.clone()));

    let auth_user_repo: Arc<dyn UserRepository + Send + Sync> = match redis_manager.as_ref() {
        Some(redis_manager) => Arc::new(CachedUserRepository::new(
            PgUserRepository::new(pool.clone()),
            redis_manager.as_ref().clone(),
        )),
        None => user_repo.clone(),
    };
    let auth_role_repo: Arc<dyn RoleRepository + Send + Sync> = match redis_manager.as_ref() {
        Some(redis_manager) => Arc::new(CachedRoleRepository::new(
            PgRoleRepository::new(pool.clone()),
            redis_manager.as_ref().clone(),
        )),
        None => role_repo.clone(),
    };

    let idle_threshold_seconds = std::env::var("AUTH_IDLE_THRESHOLD_SECONDS")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(60);
    let session_runtime_repo = match redis_manager.clone() {
        Some(redis_manager) => Arc::new(InMemorySessionRuntimeRepository::with_redis(
            redis_manager.as_ref().clone(),
            idle_threshold_seconds,
            std::env::var("AUTH_ONLINE_TTL_SECONDS")
                .ok()
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or(300),
            std::env::var("AUTH_REFRESH_TTL_SECONDS")
                .ok()
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or(604_800),
        )),
        None => Arc::new(InMemorySessionRuntimeRepository::new(idle_threshold_seconds)),
    };
    let anomaly_repo = Arc::new(PgAnomalyRepository::new(pool.clone()));
    let ai_entity_config_repo = Arc::new(PgAiEntityConfigRepository::new(pool.clone()));
    let department_repo = Arc::new(PgDepartmentRepository::new(pool.clone()));
    let team_type_repo = Arc::new(PgTeamTypeRepository::new(pool.clone()));
    let team_repo = Arc::new(PgTeamRepository::new(pool.clone()));
    let team_member_repo = Arc::new(PgTeamMemberRepository::new(pool.clone()));
    let equipment_type_repo = Arc::new(PgEquipmentTypeRepository::new(pool.clone()));
    let equipment_repo = Arc::new(PgEquipmentRepository::new(pool.clone()));
    let stand_repo = Arc::new(PgStandRepository::new(pool.clone()));
    let task_type_repo = Arc::new(PgTaskTypeRepository::new(pool.clone()));
    let business_case_repo = Arc::new(PgBusinessCaseRepository::new(pool.clone()));
    let flight_runtime_projection_repo = Arc::new(PgFlightRuntimeProjectionRepository::new(pool.clone()));
    let business_case_type_repo = Arc::new(PgBusinessCaseTypeRepository::new(pool.clone()));
    let business_case_workflow_run_repo = Arc::new(PgBusinessCaseWorkflowRunRepository::new(pool.clone()));
    let shift_handover_repo = Arc::new(PgShiftHandoverRepository::new(pool.clone()));
    let workflow_form_repo = Arc::new(PgWorkflowFormRepository::new(pool.clone()));
    let dispatch_member_repo = Arc::new(PgDispatchOrderMemberRepository::new(pool.clone()));
    let dispatch_travel_stats_repo = Arc::new(PgDispatchTravelStatsRepository::new(pool.clone()));
    let dispatch_checklist_repo = Arc::new(PgDispatchChecklistRepository::new(pool.clone()));
    let qualification_repo = Arc::new(PgDepartmentQualificationRepository::new(pool.clone()));
    let qualification_grant_repo = Arc::new(PgQualificationGrantRepository::new(pool.clone()));
    let task_type_requirement_repo = Arc::new(PgDepartmentTaskTypeRequirementRepository::new(pool.clone()));
    let generation_rule_repo = Arc::new(PgFlightGenerationRuleRepository::new(pool.clone()));
    let adjustment_rule_repo = Arc::new(PgGenerationAdjustmentRuleRepository::new(pool.clone()));
    let temporary_task_template_repo = Arc::new(PgTemporaryTaskTemplateRepository::new(pool.clone()));
    let shift_template_repo = Arc::new(PgShiftTemplateRepository::new(pool.clone()));
    let shift_instance_repo = Arc::new(PgShiftInstanceRepository::new(pool.clone()));
    let schedule_exception_repo = Arc::new(PgScheduleExceptionRepository::new(pool.clone()));
    let domain_event_outbox_repo = Arc::new(PgDomainEventOutboxRepository::new(pool.clone()));

    SharedRepos {
        pool,
        flight_repo,
        label_repo,
        flight_archive_repo,
        user_repo,
        role_repo,
        permission_repo,
        todo_repo,
        todo_agent_context_repo,
        dispatch_order_repo,
        dispatch_alert_repo,
        workflow_dispatch_repo,
        dispatch_collaboration_repo,
        mobile_device_repo,
        mobile_upload_repo,
        notification_repo,
        online_history_repo,
        operator_identity_repo,
        permission_template_repo,
        ai_copilot_business_case_batch_repo,
        auth_user_repo,
        auth_role_repo,
        session_runtime_repo,
        anomaly_repo,
        ai_entity_config_repo,
        department_repo,
        team_type_repo,
        team_repo,
        team_member_repo,
        equipment_type_repo,
        equipment_repo,
        stand_repo,
        task_type_repo,
        business_case_repo,
        flight_runtime_projection_repo,
        business_case_type_repo,
        business_case_workflow_run_repo,
        shift_handover_repo,
        workflow_form_repo,
        dispatch_member_repo,
        dispatch_travel_stats_repo,
        dispatch_checklist_repo,
        qualification_repo,
        qualification_grant_repo,
        task_type_requirement_repo,
        generation_rule_repo,
        adjustment_rule_repo,
        temporary_task_template_repo,
        shift_template_repo,
        shift_instance_repo,
        schedule_exception_repo,
        domain_event_outbox_repo,
    }
}

/// 跨领域共享的基础设施：SSE hub、消息队列网关、Flowable 客户端/服务、
/// 性能指标、各类 SSE publisher / metrics recorder，以及业务案例事件发布器。
pub(crate) struct SharedInfra {
    pub sse_hub: Arc<SseHub>,
    pub message_queue: Arc<MessageQueueGatewayClient>,
    pub domain_events_topic: String,
    pub flowable_client: Arc<dyn FlowableGateway>,
    pub flowable_svc: Arc<FlowableService>,
    pub performance_metrics: Arc<PerformanceMetricsService>,
    pub business_case_event_publisher: Option<Arc<dyn BusinessCaseEventPublisher>>,
    pub business_case_event_notifier: Arc<SseBusinessCaseEventPublisher>,
    pub dispatch_chat_event_publisher: Arc<SseDispatchChatEventPublisher>,
    pub notification_metrics_recorder: Arc<PerformanceNotificationMetricsRecorder>,
    pub mobile_realtime_metrics_recorder: Arc<PerformanceMobileRealtimeMetricsRecorder>,
    pub notification_delivery_publisher: Arc<SseNotificationDeliveryPublisher>,
    pub todo_scheduler_sse_publisher: Arc<SseTodoSchedulerPublisher>,
    pub kpi_aggregation_sse_publisher: Arc<SseKpiAggregationPublisher>,
    pub workflow_dispatch_sse_publisher: Arc<SseWorkflowDispatchPublisher>,
}

pub(crate) fn build_shared_infra(repos: &SharedRepos) -> SharedInfra {
    let pool = &repos.pool;

    let sse_hub_capacity = std::env::var("SSE_HUB_CAPACITY")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&v| v > 0)
        .unwrap_or(1024);
    let sse_hub = SseHub::new(sse_hub_capacity);
    let mq_gateway_url = env_string("MQ_GATEWAY_URL", "http://mq-gateway:8097");
    let domain_events_topic = env_string("EVENTS_DOMAIN_TOPIC", "fms.domain-events");
    let message_queue = Arc::new(MessageQueueGatewayClient::new(mq_gateway_url.clone()));
    let business_case_event_publisher =
        Some(Arc::new(OutboxBusinessCaseEventPublisher::new(pool.clone())) as Arc<dyn BusinessCaseEventPublisher>);

    let flowable_client: Arc<dyn FlowableGateway> =
        match std::env::var("FLOWABLE_ENGINE_MODE")
            .unwrap_or_else(|_| "embedded".to_string())
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            // remote = 过渡回退路径（HTTP 调 tomcat flowable-rest），稳定后计划删除
            "remote" => {
                let flowable_admin_pass = std::env::var("FLOWABLE_ADMIN_PASSWORD")
                    .or_else(|_| std::env::var("FLOWABLE_PASSWORD"))
                    .unwrap_or_else(|_| "test".to_string());
                Arc::new(
                    FlowableClient::try_new(
                        std::env::var("FLOWABLE_BASE_URL")
                            .unwrap_or_else(|_| "http://localhost:8082/flowable-rest/service".to_string()),
                        std::env::var("FLOWABLE_USERNAME").unwrap_or_else(|_| "rest-admin".to_string()),
                        flowable_admin_pass,
                    )
                    .expect("FLOWABLE_BASE_URL must be a valid absolute URL with a host"),
                )
            }
            _ => {
                let engine = EmbeddedFlowableEngine::try_new_from_env()
                    .expect("FLOWABLE_ENGINE_MODE=embedded requires a bootable flowable engine");
                Arc::new(engine)
            }
        };
    let flowable_svc = Arc::new(FlowableService::new(flowable_client.clone()));

    let performance_metrics = PerformanceMetricsService::new();
    let business_case_event_notifier = Arc::new(SseBusinessCaseEventPublisher::new(sse_hub.clone()));
    let dispatch_chat_event_publisher = Arc::new(SseDispatchChatEventPublisher::new(sse_hub.clone()));
    let notification_metrics_recorder =
        Arc::new(PerformanceNotificationMetricsRecorder::new(performance_metrics.clone()));
    let mobile_realtime_metrics_recorder = Arc::new(PerformanceMobileRealtimeMetricsRecorder::new(
        performance_metrics.clone(),
    ));
    let notification_delivery_publisher = Arc::new(SseNotificationDeliveryPublisher::new(
        sse_hub.clone(),
        performance_metrics.clone(),
    ));
    let todo_scheduler_sse_publisher = Arc::new(SseTodoSchedulerPublisher::new(sse_hub.clone()));
    let kpi_aggregation_sse_publisher = Arc::new(SseKpiAggregationPublisher::new(sse_hub.clone()));
    let workflow_dispatch_sse_publisher = Arc::new(SseWorkflowDispatchPublisher::new(sse_hub.clone()));

    SharedInfra {
        sse_hub,
        message_queue,
        domain_events_topic,
        flowable_client,
        flowable_svc,
        performance_metrics,
        business_case_event_publisher,
        business_case_event_notifier,
        dispatch_chat_event_publisher,
        notification_metrics_recorder,
        mobile_realtime_metrics_recorder,
        notification_delivery_publisher,
        todo_scheduler_sse_publisher,
        kpi_aggregation_sse_publisher,
        workflow_dispatch_sse_publisher,
    }
}

/// 跨领域共享的应用服务：通知、待办调度、待办。
pub(crate) struct SharedServices {
    pub notification_svc: Arc<ConcreteNotificationService>,
    pub todo_scheduler_svc: Arc<TodoSchedulerService>,
    pub todo_svc: Arc<ConcreteTodoService>,
}

pub(crate) fn build_shared_services(repos: &SharedRepos, infra: &SharedInfra) -> SharedServices {
    let notification_repo_for_service: Arc<dyn NotificationRepository + Send + Sync> = repos.notification_repo.clone();
    let notification_tx_repo: Arc<dyn SqlxNotificationTransactionalRepository> = repos.notification_repo.clone();
    let notification_pref_repo_for_service: Arc<dyn NotificationPreferenceRepository + Send + Sync> =
        repos.notification_repo.clone();
    let notification_collaboration_repo: Arc<dyn DispatchCollaborationRepository + Send + Sync> =
        repos.dispatch_collaboration_repo.clone();
    let notification_svc: Arc<ConcreteNotificationService> = Arc::new(
        NotificationService::new(notification_repo_for_service, notification_pref_repo_for_service)
            .with_transactional_repository(notification_tx_repo)
            .with_collaboration_repo(notification_collaboration_repo)
            .with_metrics_recorder(infra.notification_metrics_recorder.clone() as Arc<dyn NotificationMetricsRecorder>)
            .with_delivery_publisher(infra.notification_delivery_publisher.clone()
                as Arc<dyn fms_application::services::notification_service::NotificationDeliveryPublisher>)
            .with_receipt_group_sync(
                Arc::new(NoopNotificationReceiptGroupSync) as Arc<dyn NotificationReceiptGroupSync>
            ),
    );
    let todo_scheduler_svc = Arc::new(
        TodoSchedulerService::new(repos.todo_repo.clone())
            .with_notification_service(notification_svc.clone())
            .with_sse_publisher(infra.todo_scheduler_sse_publisher.clone()),
    );
    let todo_tx_repo: Arc<dyn SqlxTodoTransactionalRepository> = repos.todo_repo.clone();
    let todo_svc = Arc::new(
        TodoService::new(repos.todo_repo.clone())
            .with_transactional_repository(todo_tx_repo)
            .with_agent_context_repository(repos.todo_agent_context_repo.clone()),
    );

    SharedServices {
        notification_svc,
        todo_scheduler_svc,
        todo_svc,
    }
}

/// 构建 Redis 连接池 Data 与 anti-replay nonce store Data。
///
/// 当 `redis_required` 为真时使用 Redis 桶式 nonce store；否则回退到本地
/// TTL store（容量与桶周期可由环境变量配置）。返回 `(redis_pool_data,
/// anti_replay_store_data)`，二者均为 `Option<web::Data<_>>`，可直接填入
/// `DiContainer` 对应字段。
pub(crate) fn build_redis_security_stores(
    redis_manager: &Option<Arc<fms_infrastructure::cache::RedisPool>>,
    redis_required: bool,
) -> (
    Option<actix_web::web::Data<fms_infrastructure::cache::RedisPool>>,
    Option<actix_web::web::Data<Arc<dyn fms_domain::ports::nonce_replay_store::NonceReplayStore>>>,
) {
    use tracing::info;

    let redis_pool_data = redis_manager
        .as_ref()
        .map(|mgr| actix_web::web::Data::new(mgr.as_ref().clone()));

    let anti_replay_store_data: Option<
        actix_web::web::Data<Arc<dyn fms_domain::ports::nonce_replay_store::NonceReplayStore>>,
    > = if redis_required {
        redis_manager.as_ref().map(|mgr| {
            let pool = mgr.as_ref().clone();
            let store = fms_infrastructure::security::RedisBucketNonceStore::new(pool);
            info!("anti-replay nonce store using redis backend");
            actix_web::web::Data::new(
                Arc::new(store) as Arc<dyn fms_domain::ports::nonce_replay_store::NonceReplayStore>
            )
        })
    } else {
        let max_entries = std::env::var("ANTI_REPLAY_LOCAL_MAX_ENTRIES_PER_BUCKET")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(1_000_000);
        let bucket_secs = std::env::var("ANTI_REPLAY_BUCKET_SECS")
            .ok()
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(10);
        info!(
            max_entries_per_bucket = max_entries,
            bucket_secs, "anti-replay nonce store using local ttl backend"
        );
        Some(actix_web::web::Data::new(
            Arc::new(fms_infrastructure::security::LocalTtlNonceStore::new(
                max_entries,
                bucket_secs,
            )) as Arc<dyn fms_domain::ports::nonce_replay_store::NonceReplayStore>,
        ))
    };

    (redis_pool_data, anti_replay_store_data)
}
