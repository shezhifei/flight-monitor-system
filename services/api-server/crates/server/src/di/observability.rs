//! 可观测性与运行时编排服务装配：scheduler_runtime / domain_event_relay /
//! domain_event_cdc / domain_event_subscriber / cache_invalidation /
//! cache_invalidation_subscriber / kpi / system_ops / shift_handover /
//! system_flags / nl_query / runtime_error_monitor / flowable_draft /
//! workflow_form / dashboard_workbench。同时负责按 runtime_role 启动后台作业。

use std::sync::Arc;

use tracing::info;

use crate::di::types::*;
use fms_api::services::runtime_error_monitor::{set_global_runtime_error_monitor, RuntimeErrorMonitor};
use fms_api::services::scheduler_runtime_service::SchedulerRuntimeService;

use fms_application::services::alert_dispatch_service::AlertDispatchService;
use fms_application::services::cache_invalidation_service::{
    CacheInvalidationService, CacheInvalidationSubscriberService, FlightListResponseCacheInvalidator,
};
use fms_application::services::dashboard_workbench_service::DashboardWorkbenchService;
use fms_application::services::domain_event_cdc_relay_service::{
    DomainEventCdcConfig, DomainEventCdcRelayService, ReplicationDatabaseConfig,
};
use fms_application::services::domain_event_relay_service::DomainEventRelayService;
use fms_application::services::domain_event_subscriber_service::DomainEventSubscriberService;
use fms_application::services::flight_runtime_service::FlightRuntimeService;
use fms_application::services::flowable_draft_service::FlowableDraftService;
use fms_application::services::kpi_aggregation_service::KpiAggregationService;
use fms_application::services::nl_query_service::NLQueryService;
use fms_application::services::shift_handover_service::ShiftHandoverService;
use fms_application::services::system_flags_service::SystemFlagsService;
use fms_application::services::system_ops_service::SystemOpsService;
use fms_application::services::workflow_form_service::WorkflowFormService;
use fms_domain::ports::domain_event_outbox_repository::DomainEventOutboxRepository;
use fms_domain::ports::domain_event_subscription_state_repository::DomainEventSubscriptionStateRepository;
use fms_domain::ports::kpi_port::KpiPort;
use fms_domain::ports::message_queue::{MessageHandler, PushConsumer};
use fms_domain::ports::system_flags_repository::SystemFlagsRepository;

use fms_infrastructure::cdc::PgCdcAdmin;
use fms_infrastructure::messaging::RocketMqPushConsumer;
use fms_infrastructure::repositories::pg_domain_event_subscription_state_repository::PgDomainEventSubscriptionStateRepository;
use fms_infrastructure::repositories::pg_flight_sync_repository::PgFlightSyncRepository;
use fms_infrastructure::repositories::pg_kpi_repository::PgKpiRepository;
use fms_infrastructure::repositories::pg_system_flags_repository::PgSystemFlagsRepository;

use crate::config::{
    env_flag, env_i64, env_optional_string, env_or_value, env_string, io_other, should_start_background_jobs_for_role,
    DatabaseUrlDefaults,
};
use crate::di::adapters::FlightListResponseCacheInvalidatorAdapter;
use crate::di::ai::AiServices;
use crate::di::auth::AuthServices;
use crate::di::business_case::BusinessCaseServices;
use crate::di::dispatch::DispatchServices;
use crate::di::flight::FlightServices;
use crate::di::shared::{SharedInfra, SharedRepos, SharedServices};

pub(crate) struct ObservabilityServices {
    pub scheduler_runtime_svc: Arc<SchedulerRuntimeService>,
    pub domain_event_relay_svc: Arc<DomainEventRelayService>,
    pub domain_event_cdc_svc: Arc<DomainEventCdcRelayService>,
    pub domain_event_subscriber_svc: Arc<DomainEventSubscriberService>,
    pub cache_invalidation_subscriber_svc: Arc<CacheInvalidationSubscriberService>,
    pub cache_invalidation_svc: Arc<CacheInvalidationService>,
    pub kpi_aggregation_svc: Arc<KpiAggregationService>,
    pub shift_handover_svc: Arc<ShiftHandoverService>,
    pub system_flags_svc: Arc<SystemFlagsService>,
    pub system_ops_svc: Arc<SystemOpsService>,
    pub nl_query_svc: Arc<ConcreteNLQueryService>,
    pub runtime_error_monitor: Arc<RuntimeErrorMonitor>,
    pub flowable_draft_svc: Arc<FlowableDraftService>,
    pub workflow_form_svc: Arc<WorkflowFormService>,
    pub dashboard_workbench_svc: Arc<ConcreteDashboardWorkbenchService>,
    pub domain_event_outbox_repo: Arc<dyn DomainEventOutboxRepository + Send + Sync>,
    pub background_jobs_enabled: bool,
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn build_observability_services(
    repos: &SharedRepos,
    infra: &SharedInfra,
    shared: &SharedServices,
    auth: &AuthServices,
    flight: &FlightServices,
    dispatch: &DispatchServices,
    business_case: &BusinessCaseServices,
    ai: &AiServices,
    flight_runtime_svc: &Arc<FlightRuntimeService>,
    database_url_defaults: &DatabaseUrlDefaults,
    runtime_role: &str,
) -> Result<ObservabilityServices, std::io::Error> {
    let pool = &repos.pool;
    let message_queue = &infra.message_queue;
    let domain_events_topic = &infra.domain_events_topic;

    let flight_list_response_cache_invalidator =
        Arc::new(FlightListResponseCacheInvalidatorAdapter) as Arc<dyn FlightListResponseCacheInvalidator>;
    let cache_invalidation_source_instance = env_optional_string("CACHE_INVALIDATION_SOURCE_INSTANCE")
        .unwrap_or_else(|| format!("fms-server-{}", std::process::id()));
    let cache_invalidation_svc = Arc::new(CacheInvalidationService::new(
        Some(message_queue.clone()),
        domain_events_topic.clone(),
        cache_invalidation_source_instance,
        repos.flight_runtime_projection_repo.clone(),
        flight.flight_svc.clone(),
        Some(flight_list_response_cache_invalidator),
    ));
    let shift_handover_svc = Arc::new(
        ShiftHandoverService::new(repos.shift_handover_repo.clone())
            .with_dispatch_query_service(dispatch.dispatch_query_svc.clone())
            .with_anomaly_service(dispatch.anomaly_svc.clone())
            .with_notification_service(shared.notification_svc.clone())
            .with_todo_service(shared.todo_svc.clone()),
    );
    let system_flags_repo: Arc<dyn SystemFlagsRepository + Send + Sync> =
        Arc::new(PgSystemFlagsRepository::new(pool.clone()));
    let system_flags_svc = Arc::new(SystemFlagsService::new(system_flags_repo));
    let alert_dispatch_svc = AlertDispatchService::new();
    let system_ops_svc = Arc::new(SystemOpsService::new(auth.auth_svc.clone(), alert_dispatch_svc));
    let kpi_aggregation_repo: Arc<dyn KpiPort + Send + Sync> = Arc::new(PgKpiRepository::new(pool.clone()));
    let kpi_aggregation_svc = Arc::new(
        KpiAggregationService::new(kpi_aggregation_repo)
            .with_sse_publisher(infra.kpi_aggregation_sse_publisher.clone()),
    );

    let domain_event_outbox_repo = repos.domain_event_outbox_repo.clone();
    let domain_event_outbox_port: Arc<dyn DomainEventOutboxRepository + Send + Sync> = domain_event_outbox_repo.clone();

    let events_outbox_enabled = env_flag("EVENTS_OUTBOX_ENABLED", true);
    let domain_event_recovery_batch_size = env_i64(
        "EVENTS_OUTBOX_RETRY_RECOVERY_BATCH_SIZE",
        env_i64("EVENTS_OUTBOX_BATCH_SIZE", 200),
    );
    let domain_event_relay_svc = Arc::new(DomainEventRelayService::new(
        pool.clone(),
        Some(message_queue.clone()),
        events_outbox_enabled,
        domain_event_recovery_batch_size,
        env_i64("EVENTS_OUTBOX_BASE_BACKOFF_SECONDS", 2),
        Some(domain_events_topic.clone()),
        domain_event_outbox_repo.clone(),
    ));

    let domain_event_cdc_svc = Arc::new(DomainEventCdcRelayService::new(
        pool.clone(),
        Some(message_queue.clone()),
        events_outbox_enabled,
        Some(domain_events_topic.clone()),
        env_i64("EVENTS_OUTBOX_BASE_BACKOFF_SECONDS", 2),
        DomainEventCdcConfig::new(
            env_string("EVENTS_OUTBOX_CDC_PUBLICATION_NAME", "fms_domain_event_outbox_pub"),
            env_string("EVENTS_OUTBOX_CDC_SLOT_NAME", "fms_domain_event_outbox_slot"),
            env_i64("EVENTS_OUTBOX_CDC_STATUS_INTERVAL_SECONDS", 10),
            env_i64("EVENTS_OUTBOX_CDC_RECONNECT_BACKOFF_SECONDS", 5),
        )
        .map_err(io_other)?,
        ReplicationDatabaseConfig {
            host: env_or_value(
                "DB_REPLICATION_HOST",
                Some("DB_HOST"),
                database_url_defaults.host.as_deref(),
                "127.0.0.1",
            ),
            port: env_optional_string("DB_REPLICATION_PORT")
                .and_then(|value| value.parse::<u16>().ok())
                .or_else(|| env_optional_string("DB_PORT").and_then(|value| value.parse::<u16>().ok()))
                .or(database_url_defaults.port)
                .unwrap_or(5432),
            database: env_or_value(
                "DB_REPLICATION_NAME",
                Some("DB_NAME"),
                database_url_defaults.database.as_deref(),
                "postgres",
            ),
            user: env_or_value(
                "DB_REPLICATION_USER",
                Some("DB_USER"),
                database_url_defaults.user.as_deref(),
                "postgres",
            ),
            password: env_or_value(
                "DB_REPLICATION_PASSWORD",
                Some("DB_PASSWORD"),
                database_url_defaults.password.as_deref(),
                "",
            ),
            ssl_mode: env_string("DB_SSL_MODE", "prefer"),
            ssl_root_cert: env_optional_string("DB_SSL_ROOT_CERT"),
            ssl_sni_hostname: env_optional_string("DB_SSL_SNI_HOSTNAME"),
            ssl_client_cert: env_optional_string("DB_SSL_CLIENT_CERT"),
            ssl_client_key: env_optional_string("DB_SSL_CLIENT_KEY"),
        },
        domain_event_outbox_repo.clone(),
        Arc::new(PgCdcAdmin::new(pool.clone())),
    ));

    let cache_invalidation_subscriber_svc = Arc::new(CacheInvalidationSubscriberService::new(
        cache_invalidation_svc.clone(),
        domain_events_topic.clone(),
        env_optional_string("CACHE_INVALIDATION_CONSUMER_GROUP"),
    ));

    let flight_realtime_broadcaster: Arc<dyn fms_domain::broadcaster::Broadcaster + Send + Sync> =
        infra.sse_hub.clone();
    let subscription_state_repo: Arc<dyn DomainEventSubscriptionStateRepository + Send + Sync> =
        Arc::new(PgDomainEventSubscriptionStateRepository::new(pool.clone()));
    let domain_event_subscriber_svc = Arc::new(DomainEventSubscriberService::new(
        subscription_state_repo,
        Some(flight.flight_cache_svc.clone()),
        Some(flight_runtime_svc.clone()),
        Some(dispatch.anomaly_svc.clone()),
        Some(dispatch.dispatch_svc.clone()),
        Some(dispatch.dispatch_overrun_warning_svc.clone()),
        Some(flight.ontology_svc.clone()),
        Some(dispatch.event_rule_repo.clone()),
        Some(business_case.business_case_type_svc.clone()),
        Some(business_case.business_case_workflow_svc.clone()),
        Some(infra.business_case_event_notifier.clone()),
        Some(flight_realtime_broadcaster),
        Some(cache_invalidation_svc.clone()),
        Some(domain_events_topic.clone()),
        std::env::var("EVENTS_SUBSCRIBER_CONSUMER_GROUP").ok(),
        env_i64("EVENTS_SUBSCRIBER_MAX_RETRY", 5) as i32,
    ));

    let flowable_draft_svc =
        Arc::new(FlowableDraftService::new().with_ai_config_repo(repos.ai_entity_config_repo.clone()));
    let workflow_form_svc = Arc::new(
        WorkflowFormService::new(
            repos.workflow_form_repo.clone(),
            repos.business_case_repo.clone(),
            repos.business_case_workflow_run_repo.clone(),
        )
        .with_flowable_service(infra.flowable_svc.clone()),
    );
    let dashboard_workbench_svc = Arc::new(DashboardWorkbenchService::new(
        Some(shared.todo_svc.clone()),
        Some(dispatch.anomaly_svc.clone()),
        Some(dispatch.dispatch_query_svc.clone()),
        Some(system_ops_svc.clone()),
    ));
    let nl_query_svc = Arc::new(NLQueryService::new(
        flight.flight_svc.clone(),
        ai.ai_runtime_svc.clone(),
    ));
    let runtime_error_monitor = RuntimeErrorMonitor::new(Some(infra.sse_hub.clone()));
    set_global_runtime_error_monitor(&runtime_error_monitor);

    // RocketMQ push consumer 是 MQ 消息的唯一消费路径，无条件构造；
    // 订阅/启动失败由 SchedulerRuntimeService::start_push_consumer 致命处理。
    let push_consumer: Option<Arc<dyn PushConsumer + Send + Sync>> = {
        let name_server =
            std::env::var("ROCKETMQ_NAME_SERVER_ADDR").unwrap_or_else(|_| "rocketmq-namesrv:9876".to_string());
        let consumer: Arc<dyn PushConsumer + Send + Sync> =
            Arc::new(RocketMqPushConsumer::new(name_server)) as Arc<dyn PushConsumer + Send + Sync>;
        let ai_event_handler: Arc<dyn MessageHandler> = ai.ai_event_consumer.clone();
        let ai_runtime_topic = env_string("AI_RUNTIME_EVENTS_TOPIC", "ai_runtime_events");
        let ai_runtime_group = env_string("AI_RUNTIME_EVENTS_CONSUMER_GROUP", "fms-ai-runtime");
        if let Err(error) = consumer
            .subscribe(&ai_runtime_topic, &ai_runtime_group, Some("*"), ai_event_handler)
            .await
        {
            info!(
                topic = %ai_runtime_topic,
                error = %error,
                "failed to subscribe ai_runtime_events on push consumer; ai event consumption disabled"
            );
        }
        Some(consumer)
    };

    let scheduler_runtime_svc = SchedulerRuntimeService::new(
        pool.clone(),
        Arc::new(PgFlightSyncRepository::new(pool.clone())),
        infra.sse_hub.clone(),
        runtime_error_monitor.clone(),
        infra.performance_metrics.clone(),
        flight.flight_svc.clone(),
        dispatch.dispatch_chat_svc.clone(),
        domain_event_relay_svc.clone(),
        domain_event_subscriber_svc.clone(),
        cache_invalidation_subscriber_svc.clone(),
        shared.todo_scheduler_svc.clone(),
        ai.ai_business_case_copilot_svc.clone(),
        dispatch.anomaly_svc.clone(),
        kpi_aggregation_svc.clone(),
        ai.ai_admin_svc.clone(),
        system_ops_svc.clone(),
        auth.online_status_svc.clone(),
        push_consumer,
        env_i64("EVENTS_OUTBOX_RETRY_RECOVERY_INTERVAL_SECONDS", 30),
    )
    .await;

    let background_jobs_enabled = should_start_background_jobs_for_role(runtime_role);
    if background_jobs_enabled {
        if !events_outbox_enabled {
            tracing::warn!(
                "Background jobs are enabled (runtime_role={runtime_role}) but EVENTS_OUTBOX_ENABLED=false; \
                 domain event outbox CDC relay and SQL recovery will NOT run. \
                 Set EVENTS_OUTBOX_ENABLED=true to publish domain events."
            );
        }
        domain_event_cdc_svc.start().await.map_err(io_other)?;
        scheduler_runtime_svc.start().await;
        ai.ai_recovery_orchestrator.clone().start();
        ai.ai_job_timeout_reaper.clone().start();
        dispatch.dispatch_overrun_warning_svc.clone().start_scanner();
        flight.ontology_svc.clone().start_autolink_scanner();
    } else {
        if events_outbox_enabled {
            tracing::warn!(
                "EVENTS_OUTBOX_ENABLED=true but runtime_role={runtime_role} does not start background jobs; \
                 domain event outbox CDC relay and SQL recovery will NOT run. \
                 Use runtime_role=all or runtime_role=worker to enable the relay owner."
            );
        }
        info!(
            runtime_role = %runtime_role,
            "Skipping scheduler runtime startup for API-only role"
        );
    }

    Ok(ObservabilityServices {
        scheduler_runtime_svc,
        domain_event_relay_svc,
        domain_event_cdc_svc,
        domain_event_subscriber_svc,
        cache_invalidation_subscriber_svc,
        cache_invalidation_svc,
        kpi_aggregation_svc,
        shift_handover_svc,
        system_flags_svc,
        system_ops_svc,
        nl_query_svc,
        runtime_error_monitor,
        flowable_draft_svc,
        workflow_form_svc,
        dashboard_workbench_svc,
        domain_event_outbox_repo: domain_event_outbox_port,
        background_jobs_enabled,
    })
}
