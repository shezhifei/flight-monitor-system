//! 依赖注入 (DI) 与组装容器模块。
//!
//! `DiContainer` 结构体定义与本模块聚合；具体的仓库 / 基础设施 / 各领域服务
//! 装配逻辑拆分到 `adapters`、`shared`、`auth`、`flight`、`dispatch`、
//! `business_case`、`ai`、`observability`、`mobile` 子模块中，由
//! [`build_di_container`] 按依赖顺序编排并组装出最终的 `DiContainer`。

use std::sync::Arc;

use actix_web::web;

use crate::di::types::*;
use fms_api::middleware::jwt::{JwtAudience, JwtSecret, WorkflowInternalToken};
use fms_api::services::performance_metrics::PerformanceMetricsService;
use fms_api::services::runtime_error_monitor::RuntimeErrorMonitor;
use fms_api::services::scheduler_runtime_service::SchedulerRuntimeService;
use fms_api::sse::hub::SseHub;

use fms_application::services::ai_admin_service::AiAdminService;
use fms_application::services::ai_context_service::AiContextService;
use fms_application::services::ai_job_service::AiJobService;
use fms_application::services::ai_job_timeout_reaper_service::AiJobTimeoutReaperService;
use fms_application::services::ai_media_service::AiMediaService;
use fms_application::services::ai_output_validator::AiOutputValidator;
use fms_application::services::ai_proposal_ingest_service::AiProposalIngestService;
use fms_application::services::ai_realtime_audio_service::RealtimeAudioSessionService;
use fms_application::services::ai_runtime_service::ai_event_consumer::AiEventConsumer;
use fms_application::services::ai_runtime_service::ai_execution_control_service::AiExecutionControlService;
use fms_application::services::ai_runtime_service::recovery_orchestrator::RecoveryOrchestrator;
use fms_application::services::ai_runtime_service::rollback_service::RollbackService;
use fms_application::services::ai_runtime_service::AiRuntimeService;
use fms_application::services::auth_admin_service::{AuthAdminCommandService, AuthAdminQueryService};
use fms_application::services::auth_service::JwtConfig;
use fms_application::services::business_case_type_service::BusinessCaseTypeService;
use fms_application::services::business_case_workflow_service::BusinessCaseWorkflowService;
use fms_application::services::cache_invalidation_service::CacheInvalidationService;
use fms_application::services::dispatch_analytics_service::DispatchAnalyticsService;
use fms_application::services::dispatch_collaboration_query_service::DispatchCollaborationQueryService;
use fms_application::services::dispatch_frontend_replan_service::DispatchFrontendReplanService;
use fms_application::services::dispatch_rule_service::DispatchRuleService;
use fms_application::services::dispatch_scenario_service::DispatchScenarioService;
use fms_application::services::dispatch_service::dispatch_overrun_warning_service::DispatchOverrunWarningService;
use fms_application::services::domain_event_cdc_relay_service::DomainEventCdcRelayService;
use fms_application::services::flight_archive_service::FlightArchiveService;
use fms_application::services::flight_batch_cell_update_service::FlightBatchCellUpdate;
use fms_application::services::flight_cache_service::FlightCacheService;
use fms_application::services::flight_import_service::FlightImportService;
use fms_application::services::flight_runtime_service::FlightRuntimeService;
use fms_application::services::flowable_draft_service::FlowableDraftService;
use fms_application::services::flowable_service::FlowableService;
use fms_application::services::kpi_aggregation_service::KpiAggregationService;
use fms_application::services::llm_eval_service::LLMEvalService;
use fms_application::services::mobile_upload_service::MobileUploadService;
use fms_application::services::online_history_service::OnlineHistoryService;
use fms_application::services::online_status_service::OnlineStatusService;
use fms_application::services::ontology_actions::OntologyActionServices;
use fms_application::services::ontology_service::OntologyService;
use fms_application::services::operator_identity_service::OperatorIdentityService;
use fms_application::services::resource_utilization_service::ResourceUtilizationService;
use fms_application::services::shift_handover_service::ShiftHandoverService;
use fms_application::services::system_flags_service::SystemFlagsService;
use fms_application::services::system_ops_service::SystemOpsService;
use fms_application::services::workflow_form_service::WorkflowFormService;
use fms_infrastructure::db::transaction::PgUnitOfWork;

use fms_domain::ports::ai_auth_context_loader::RunAuthorizationContextLoader;
use fms_domain::ports::ai_ontology_repository::AiOntologyRepository;
use fms_domain::ports::dispatch_collaboration_repository::DispatchCollaborationRepository;
use fms_domain::ports::nonce_replay_store::NonceReplayStore;

use crate::config::DatabaseUrlDefaults;

// ─── Real Adapters: 见 di/adapters.rs（publisher / metrics recorder 适配器）──
mod adapters;
pub mod ai;
pub mod auth;
pub mod business_case;
pub mod dispatch;
pub mod flight;
pub mod mobile;
pub mod observability;
pub mod shared;
mod types;

// ─── DI Container ────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct DiContainer {
    pub pool: sqlx::PgPool,
    pub jwt_secret: JwtSecret,
    pub jwt_audience: JwtAudience,
    pub workflow_internal_token: WorkflowInternalToken,
    pub sse_hub: Arc<SseHub>,
    pub performance_metrics: Arc<PerformanceMetricsService>,
    pub runtime_error_monitor: Arc<RuntimeErrorMonitor>,
    pub scheduler_runtime_svc: Arc<SchedulerRuntimeService>,

    pub flight_svc: Arc<ConcreteFlightService>,
    pub flight_batch_cell_svc: Arc<dyn FlightBatchCellUpdate>,
    pub cache_invalidation_svc: Arc<CacheInvalidationService>,
    pub label_svc: Arc<ConcreteLabelService>,
    pub flight_import_svc: Arc<FlightImportService>,
    pub flight_archive_svc: Arc<FlightArchiveService>,
    pub ontology_svc: Arc<OntologyService>,
    pub ontology_actions: Arc<OntologyActionServices>,
    pub auth_svc: Arc<ConcreteAuthService>,
    pub login_failure_limiter: Arc<fms_api::routes::auth::LoginFailureRateLimiter>,
    pub auth_validation_cache: Arc<fms_application::services::auth_validation_cache::AuthValidationCache>,
    pub todo_svc: Arc<ConcreteTodoService>,
    pub auth_admin_query_svc: Arc<AuthAdminQueryService>,
    pub auth_admin_command_svc: Arc<AuthAdminCommandService>,
    pub online_history_svc: Arc<OnlineHistoryService>,
    pub online_status_svc: Arc<OnlineStatusService>,
    pub operator_identity_svc: Arc<OperatorIdentityService>,
    pub dispatch_svc: Arc<ConcreteDispatchService>,
    pub dispatch_query_svc: Arc<ConcreteDispatchQueryService>,
    pub dispatch_overrun_warning_svc: Arc<DispatchOverrunWarningService>,
    pub dispatch_frontend_replan_svc: Arc<DispatchFrontendReplanService>,
    pub llm_eval_svc: Arc<LLMEvalService>,
    pub dispatch_collaboration_repo: Arc<dyn DispatchCollaborationRepository + Send + Sync>,
    pub dispatch_collaboration_query_svc: Arc<DispatchCollaborationQueryService>,
    pub dispatch_chat_svc: Arc<ConcreteDispatchChatService>,
    pub dispatch_rule_svc: Arc<DispatchRuleService>,
    pub event_rule_admin_svc: Arc<ConcreteEventRuleAdminService>,
    pub dispatch_schedule_svc: Arc<ConcreteDispatchScheduleService>,
    pub dispatch_analytics_svc: Arc<DispatchAnalyticsService>,
    pub dispatch_scenario_svc: Arc<DispatchScenarioService>,
    pub mobile_device_svc: Arc<ConcreteMobileDeviceService>,
    pub mobile_upload_svc: Arc<MobileUploadService>,
    pub mobile_workbench_svc: Arc<ConcreteMobileWorkbenchService>,
    pub dashboard_workbench_svc: Arc<ConcreteDashboardWorkbenchService>,
    pub mobile_operations_svc: Arc<ConcreteMobileOperationsService>,
    pub nl_query_svc: Arc<ConcreteNLQueryService>,
    pub notification_svc: Arc<ConcreteNotificationService>,
    pub anomaly_svc: Arc<ConcreteAnomalyService>,

    pub ai_admin_svc: Arc<AiAdminService>,
    pub ai_route_svc: Arc<fms_application::services::ai_route_service::AiRouteService>,
    pub ai_media_svc: Arc<AiMediaService>,
    pub ai_business_case_copilot_svc: Arc<ConcreteAiBusinessCaseCopilotService>,
    pub ai_realtime_audio_svc: Arc<RealtimeAudioSessionService>,
    pub ai_runtime_svc: Arc<AiRuntimeService>,
    pub ai_runtime_client: Arc<fms_api::services::ai_runtime_client::AiRuntimeClient>,
    pub ai_action_proposal_svc: Arc<ConcreteAiActionProposalService>,
    pub micro_model_registry: Arc<fms_domain::models::micro_model::MicroModelRegistry>,
    pub ai_job_svc: Arc<AiJobService>,
    pub ai_ontology_repo: Arc<dyn AiOntologyRepository + Send + Sync>,
    pub ai_output_validator: Arc<AiOutputValidator>,
    pub ai_proposal_ingest_svc: Arc<AiProposalIngestService>,
    pub ai_execution_readiness_svc:
        Arc<fms_application::services::ai_execution_readiness_service::AiExecutionReadinessService>,
    pub ai_execution_metrics_svc:
        Arc<fms_application::services::ai_execution_metrics_service::AiExecutionMetricsService>,
    pub ai_rollout_status_svc: Arc<fms_application::services::ai_rollout_status_service::AiRolloutStatusService>,
    pub ai_context_svc: Arc<AiContextService>,
    pub ai_control_svc: Arc<AiExecutionControlService>,
    pub ai_rollback_svc: Arc<RollbackService>,
    pub ai_recovery_orchestrator: Arc<RecoveryOrchestrator>,
    pub ai_event_consumer: Arc<AiEventConsumer>,
    pub ai_job_timeout_reaper: Arc<AiJobTimeoutReaperService>,
    pub ai_run_auth_loader: Arc<dyn RunAuthorizationContextLoader + Send + Sync>,

    // Missing service registrations
    pub business_case_svc: Arc<ConcreteBusinessCaseService>,
    pub business_case_type_svc: Arc<BusinessCaseTypeService>,
    pub business_case_workflow_svc: Arc<BusinessCaseWorkflowService>,
    pub dispatch_resource_svc: Arc<ConcreteDispatchResourceService>,
    pub flight_cache_svc: Arc<FlightCacheService>,
    pub flight_runtime_svc: Arc<FlightRuntimeService>,
    pub flowable_draft_svc: Arc<FlowableDraftService>,
    pub flowable_svc: Arc<FlowableService>,
    pub kpi_aggregation_svc: Arc<KpiAggregationService>,
    pub resource_utilization_svc: Arc<ResourceUtilizationService>,
    pub shift_handover_svc: Arc<ShiftHandoverService>,
    pub system_flags_svc: Arc<SystemFlagsService>,
    pub system_ops_svc: Arc<SystemOpsService>,
    pub workflow_dispatch_svc: Arc<ConcreteWorkflowDispatchService>,
    pub workflow_form_svc: Arc<WorkflowFormService>,

    pub redis_pool: Option<web::Data<fms_infrastructure::cache::RedisPool>>,
    pub anti_replay_store: Option<web::Data<std::sync::Arc<dyn NonceReplayStore>>>,
    pub background_jobs_enabled: bool,
    pub cdc_relay_svc: Arc<DomainEventCdcRelayService<PgUnitOfWork>>,
}

/// 按依赖顺序编排各子模块 builder，组装出完整的 [`DiContainer`]。
///
/// 组装顺序：repos → infra → shared(notification/todo) → auth → flight →
/// dispatch → business_case → ai → flight_runtime → observability → mobile。
#[allow(clippy::too_many_arguments)]
pub async fn build_di_container(
    pool: sqlx::PgPool,
    redis_manager: Option<Arc<fms_infrastructure::cache::RedisPool>>,
    jwt_config: JwtConfig,
    jwt_secret: String,
    jwt_audiences: Vec<String>,
    workflow_internal_token: Option<String>,
    runtime_role: &str,
    redis_required: bool,
    database_url_defaults: &DatabaseUrlDefaults,
) -> Result<DiContainer, std::io::Error> {
    let repos = shared::build_shared_repos(pool, &redis_manager);
    let infra = shared::build_shared_infra(&repos);
    let shared_svc = shared::build_shared_services(&repos, &infra);
    let auth = auth::build_auth_services(&repos, &infra, jwt_config);
    let flight = flight::build_flight_services(&repos, &infra, &redis_manager);
    let dispatch = dispatch::build_dispatch_services(&repos, &infra, &shared_svc, &auth, &flight);
    let business_case =
        business_case::build_business_case_services(&repos, &infra, &shared_svc, &flight, &dispatch).await;
    let ai = ai::build_ai_services(&repos, &shared_svc, &flight, &dispatch, &business_case);
    let flight_runtime_svc = flight::build_flight_runtime_service(&repos, &flight, &business_case, &ai);
    let observability = observability::build_observability_services(
        &repos,
        &infra,
        &shared_svc,
        &auth,
        &flight,
        &dispatch,
        &business_case,
        &ai,
        &flight_runtime_svc,
        database_url_defaults,
        runtime_role,
    )
    .await?;
    let mobile = mobile::build_mobile_services(&repos, &infra, &shared_svc, &dispatch, &observability);

    let jwt_secret_val = JwtSecret(jwt_secret);
    let jwt_audience_val = JwtAudience(jwt_audiences);

    let (redis_pool, anti_replay_store) = shared::build_redis_security_stores(&redis_manager, redis_required);

    Ok(DiContainer {
        pool: repos.pool.clone(),
        jwt_secret: jwt_secret_val,
        jwt_audience: jwt_audience_val,
        workflow_internal_token: WorkflowInternalToken(workflow_internal_token),
        sse_hub: infra.sse_hub.clone(),
        performance_metrics: infra.performance_metrics.clone(),
        runtime_error_monitor: observability.runtime_error_monitor.clone(),
        scheduler_runtime_svc: observability.scheduler_runtime_svc.clone(),

        flight_svc: flight.flight_svc.clone(),
        flight_batch_cell_svc: flight.flight_batch_cell_svc.clone(),
        cache_invalidation_svc: observability.cache_invalidation_svc.clone(),
        label_svc: flight.label_svc.clone(),
        flight_import_svc: flight.flight_import_svc.clone(),
        flight_archive_svc: flight.flight_archive_svc.clone(),
        ontology_svc: flight.ontology_svc.clone(),
        ontology_actions: flight.ontology_actions.clone(),
        auth_svc: auth.auth_svc.clone(),
        login_failure_limiter: auth.login_failure_limiter.clone(),
        auth_validation_cache: auth.auth_validation_cache.clone(),
        todo_svc: shared_svc.todo_svc.clone(),
        auth_admin_query_svc: auth.auth_admin_query_svc.clone(),
        auth_admin_command_svc: auth.auth_admin_command_svc.clone(),
        online_history_svc: auth.online_history_svc.clone(),
        online_status_svc: auth.online_status_svc.clone(),
        operator_identity_svc: auth.operator_identity_svc.clone(),
        dispatch_svc: dispatch.dispatch_svc.clone(),
        dispatch_query_svc: dispatch.dispatch_query_svc.clone(),
        dispatch_overrun_warning_svc: dispatch.dispatch_overrun_warning_svc.clone(),
        dispatch_frontend_replan_svc: dispatch.dispatch_frontend_replan_svc.clone(),
        llm_eval_svc: dispatch.llm_eval_svc.clone(),
        dispatch_collaboration_repo: repos.dispatch_collaboration_repo.clone(),
        dispatch_collaboration_query_svc: dispatch.dispatch_collaboration_query_svc.clone(),
        dispatch_chat_svc: dispatch.dispatch_chat_svc.clone(),
        dispatch_rule_svc: dispatch.dispatch_rule_svc.clone(),
        event_rule_admin_svc: dispatch.event_rule_admin_svc.clone(),
        dispatch_schedule_svc: dispatch.dispatch_schedule_svc.clone(),
        dispatch_analytics_svc: dispatch.dispatch_analytics_svc.clone(),
        dispatch_scenario_svc: dispatch.dispatch_scenario_svc.clone(),
        mobile_device_svc: mobile.mobile_device_svc.clone(),
        mobile_upload_svc: mobile.mobile_upload_svc.clone(),
        mobile_workbench_svc: mobile.mobile_workbench_svc.clone(),
        dashboard_workbench_svc: observability.dashboard_workbench_svc.clone(),
        mobile_operations_svc: mobile.mobile_operations_svc.clone(),
        nl_query_svc: observability.nl_query_svc.clone(),
        notification_svc: shared_svc.notification_svc.clone(),
        anomaly_svc: dispatch.anomaly_svc.clone(),

        ai_admin_svc: ai.ai_admin_svc.clone(),
        ai_route_svc: ai.ai_route_svc.clone(),
        ai_media_svc: ai.ai_media_svc.clone(),
        ai_business_case_copilot_svc: ai.ai_business_case_copilot_svc.clone(),
        ai_realtime_audio_svc: ai.ai_realtime_audio_svc.clone(),
        ai_runtime_svc: ai.ai_runtime_svc.clone(),
        ai_runtime_client: ai.ai_runtime_client.clone(),
        ai_action_proposal_svc: ai.ai_action_proposal_svc.clone(),
        micro_model_registry: ai.micro_model_registry.clone(),
        ai_job_svc: ai.ai_job_svc.clone(),
        ai_ontology_repo: ai.ai_ontology_repo.clone(),
        ai_output_validator: ai.ai_output_validator.clone(),
        ai_proposal_ingest_svc: ai.ai_proposal_ingest_svc.clone(),
        ai_execution_readiness_svc: ai.ai_execution_readiness_svc.clone(),
        ai_execution_metrics_svc: ai.ai_execution_metrics_svc.clone(),
        ai_rollout_status_svc: ai.ai_rollout_status_svc.clone(),
        ai_context_svc: ai.ai_context_svc.clone(),
        ai_control_svc: ai.ai_control_svc.clone(),
        ai_rollback_svc: ai.ai_rollback_svc.clone(),
        ai_recovery_orchestrator: ai.ai_recovery_orchestrator.clone(),
        ai_event_consumer: ai.ai_event_consumer.clone(),
        ai_job_timeout_reaper: ai.ai_job_timeout_reaper.clone(),
        ai_run_auth_loader: ai.ai_run_auth_loader.clone(),

        business_case_svc: business_case.business_case_svc.clone(),
        business_case_type_svc: business_case.business_case_type_svc.clone(),
        business_case_workflow_svc: business_case.business_case_workflow_svc.clone(),
        dispatch_resource_svc: dispatch.dispatch_resource_svc.clone(),
        flight_cache_svc: flight.flight_cache_svc.clone(),
        flight_runtime_svc,
        flowable_draft_svc: observability.flowable_draft_svc.clone(),
        flowable_svc: infra.flowable_svc.clone(),
        kpi_aggregation_svc: observability.kpi_aggregation_svc.clone(),
        resource_utilization_svc: dispatch.resource_utilization_svc.clone(),
        shift_handover_svc: observability.shift_handover_svc.clone(),
        system_flags_svc: observability.system_flags_svc.clone(),
        system_ops_svc: observability.system_ops_svc.clone(),
        workflow_dispatch_svc: dispatch.workflow_dispatch_svc.clone(),
        workflow_form_svc: observability.workflow_form_svc.clone(),

        redis_pool,
        anti_replay_store,
        background_jobs_enabled: observability.background_jobs_enabled,
        cdc_relay_svc: observability.domain_event_cdc_svc.clone(),
    })
}
