//! AI 领域服务装配：ai_admin / ai_media / ai_realtime_audio / ai_runtime /
//! ai_business_case_copilot / domain_action_executor / ai_action_proposal /
//! ai_proposal_ingest / ai_job / ai_ontology / ai_output_validator /
//! ai_execution_readiness / ai_execution_metrics / ai_rollout_status /
//! ai_context / ai_runtime_client / micro_model_registry。
//!
//! Also wires the AI execution control plane
//! (`AiExecutionControlService`, `RollbackService`,
//! `RecoveryOrchestrator`, `AiEventConsumer`): the command queue
//! and worker leases are the canonical run lifecycle path. Production
//! uses Postgres-backed repositories and a Postgres-backed authorization
//! context loader so Rust is the authority boundary for tool permissions.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::di::types::*;
use fms_api::services::ai_runtime_client::AiRuntimeClient;

use fms_application::services::ai_action_proposal_service::AiActionProposalService;
use fms_application::services::ai_admin_service::AiAdminService;
use fms_application::services::ai_business_case_copilot_service::AiBusinessCaseCopilotService;
use fms_application::services::ai_context_service::AiContextService;
use fms_application::services::ai_execution_metrics_service::AiExecutionMetricsService;
use fms_application::services::ai_execution_readiness_service::AiExecutionReadinessService;
use fms_application::services::ai_job_service::AiJobService;
use fms_application::services::ai_job_timeout_reaper_service::{AiJobTimeoutReaperService, ReaperConfig};
use fms_application::services::ai_media_service::AiMediaService;
use fms_application::services::ai_output_validator::AiOutputValidator;
use fms_application::services::ai_proposal_audit_recorder::{
    AiProposalAuditEventRecorder, PgAiProposalAuditEventRecorder,
};
use fms_application::services::ai_proposal_ingest_service::AiProposalIngestService;
use fms_application::services::ai_realtime_audio_service::RealtimeAudioSessionService;
use fms_application::services::ai_rollout_status_service::AiRolloutStatusService;
use fms_application::services::ai_route_service::AiRouteService;
use fms_application::services::ai_runtime_service::ai_event_consumer::AiEventConsumer;
use fms_application::services::ai_runtime_service::ai_execution_control_service::{
    AiExecutionControlService, ControlServiceError, RunLifecycleHook,
};
use fms_application::services::ai_runtime_service::compensation_planner::CompensationPlanner;
use fms_application::services::ai_runtime_service::recovery_orchestrator::{
    RecoveryOrchestrator, RecoveryOrchestratorConfig, RecoveryOrchestratorDeps,
};
use fms_application::services::ai_runtime_service::rollback_service::RollbackService;
use fms_application::services::ai_runtime_service::tool_authorization_service::{
    StaticFeatureFlagSource, ToolAuthorizationService,
};
use fms_application::services::ai_runtime_service::AiRuntimeService;
use fms_application::services::authorization_service::AuthorizationService;
use fms_application::services::domain_action_executor::DomainActionExecutor;

use fms_domain::models::ai_job::{AiJobStatus, AiRunStatus};
use fms_domain::models::micro_model::MicroModelRegistry;
use fms_domain::ontology::flight_ops_v1;
use fms_domain::ports::ai_auth_context_loader::RunAuthorizationContextLoader;
use fms_domain::ports::ai_copilot_repository::AiCopilotBusinessCaseBatchRepository;
use fms_domain::ports::ai_execution_repository::{
    AiActionReceiptRepository, AiCompensationPlanRepository, AiRunCheckpointRepository, AiRuntimeCommandRepository,
    AiToolCallRepository,
};
use fms_domain::ports::ai_object_policy_repository::AiObjectPolicyRepository;
use fms_domain::ports::ai_ontology_repository::AiOntologyRepository;
use fms_domain::ports::ai_proposal_repository::AiProposalRepository;
use fms_domain::ports::dispatch_repository::StandRepository;

use fms_domain::ports::ai_job_repository::AiJobRepository;
use fms_domain::ports::ai_run_event_repository::AiRunEventRepository;
use fms_domain::ports::ai_run_repository::AiRunRepository;
use fms_infrastructure::ai_context_snapshot::PgAiContextSnapshotRepository;
use fms_infrastructure::repositories::pg_ai_action_receipt_repository::PgAiActionReceiptRepository;
use fms_infrastructure::repositories::pg_ai_auth_context_loader::PgRunAuthorizationContextLoader;
use fms_infrastructure::repositories::pg_ai_compensation_plan_repository::PgAiCompensationPlanRepository;
use fms_infrastructure::repositories::pg_ai_job_repository::PgAiJobRepository;
use fms_infrastructure::repositories::pg_ai_object_policy_repository::PgAiObjectPolicyRepository;
use fms_infrastructure::repositories::pg_ai_ontology_repository::PgAiOntologyRepository;
use fms_infrastructure::repositories::pg_ai_proposal_repository::PgAiProposalRepository;
use fms_infrastructure::repositories::pg_ai_run_checkpoint_repository::PgAiRunCheckpointRepository;
use fms_infrastructure::repositories::pg_ai_run_event_repository::PgAiRunEventRepository;
use fms_infrastructure::repositories::pg_ai_run_repository::PgAiRunRepository;
use fms_infrastructure::repositories::pg_ai_runtime_command_repository::PgAiRuntimeCommandRepository;
use fms_infrastructure::repositories::pg_ai_tool_call_repository::PgAiToolCallRepository;

use crate::di::business_case::BusinessCaseServices;
use crate::di::dispatch::DispatchServices;
use crate::di::flight::FlightServices;
use crate::di::shared::{SharedRepos, SharedServices};

pub(crate) struct AiServices {
    pub ai_admin_svc: Arc<AiAdminService>,
    pub ai_route_svc: Arc<AiRouteService>,
    pub ai_media_svc: Arc<AiMediaService>,
    pub ai_business_case_copilot_svc: Arc<ConcreteAiBusinessCaseCopilotService>,
    pub ai_realtime_audio_svc: Arc<RealtimeAudioSessionService>,
    pub ai_runtime_svc: Arc<AiRuntimeService>,
    pub ai_runtime_client: Arc<AiRuntimeClient>,
    pub ai_action_proposal_svc: Arc<ConcreteAiActionProposalService>,
    pub micro_model_registry: Arc<MicroModelRegistry>,
    pub ai_job_svc: Arc<AiJobService>,
    pub ai_ontology_repo: Arc<dyn AiOntologyRepository + Send + Sync>,
    pub ai_output_validator: Arc<AiOutputValidator>,
    pub ai_proposal_ingest_svc: Arc<AiProposalIngestService>,
    pub ai_execution_readiness_svc: Arc<AiExecutionReadinessService>,
    pub ai_execution_metrics_svc: Arc<AiExecutionMetricsService>,
    pub ai_rollout_status_svc: Arc<AiRolloutStatusService>,
    pub ai_context_svc: Arc<AiContextService>,
    pub ai_control_svc: Arc<AiExecutionControlService>,
    pub ai_rollback_svc: Arc<RollbackService>,
    pub ai_recovery_orchestrator: Arc<RecoveryOrchestrator>,
    pub ai_event_consumer: Arc<AiEventConsumer>,
    pub ai_job_timeout_reaper: Arc<AiJobTimeoutReaperService>,
}

pub(crate) fn build_ai_services(
    repos: &SharedRepos,
    shared: &SharedServices,
    flight: &FlightServices,
    dispatch: &DispatchServices,
    business_case: &BusinessCaseServices,
) -> AiServices {
    let pool = &repos.pool;

    let ai_admin_svc = Arc::new(AiAdminService::new(repos.ai_entity_config_repo.clone()));
    let ai_media_svc = Arc::new(AiMediaService::new(ai_admin_svc.clone()));
    let ai_realtime_audio_svc = Arc::new(RealtimeAudioSessionService::new(ai_admin_svc.clone()));
    let ai_runtime_svc = Arc::new(
        AiRuntimeService::new()
            .with_notification_service(shared.notification_svc.clone())
            .with_todo_repository(repos.todo_repo.clone())
            .with_todo_agent_context_repository(repos.todo_agent_context_repo.clone()),
    );
    let ai_route_svc = Arc::new(fms_application::services::ai_route_service::AiRouteService::new(
        ai_admin_svc.clone(),
    ));

    let ai_copilot_business_case_batch_repo: Arc<dyn AiCopilotBusinessCaseBatchRepository + Send + Sync> =
        repos.ai_copilot_business_case_batch_repo.clone();
    let ai_business_case_copilot_svc: Arc<ConcreteAiBusinessCaseCopilotService> = Arc::new(
        AiBusinessCaseCopilotService::new(
            ai_copilot_business_case_batch_repo,
            ai_admin_svc.clone(),
            repos.flight_repo.clone(),
            flight.flight_svc.clone(),
            business_case.business_case_svc.clone(),
        )
        .with_workflow_service(business_case.business_case_workflow_svc.clone())
        .with_business_case_type_service(business_case.business_case_type_svc.clone()),
    );

    let domain_action_executor = Arc::new(DomainActionExecutor::new(
        flight.flight_svc.clone(),
        dispatch.dispatch_svc.clone(),
        shared.notification_svc.clone(),
        dispatch.anomaly_svc.clone(),
        flight.label_svc.clone(),
        shared.todo_svc.clone(),
        business_case.business_case_svc.clone(),
        repos.domain_event_outbox_repo.clone(),
        pool.clone(),
    ));

    let ai_proposal_repo: Arc<dyn AiProposalRepository + Send + Sync> =
        Arc::new(PgAiProposalRepository::new(pool.clone()));
    let ai_object_policy_repo: Arc<dyn AiObjectPolicyRepository + Send + Sync> =
        Arc::new(PgAiObjectPolicyRepository::new(pool.clone()));
    let ai_ontology_repo: Arc<dyn AiOntologyRepository + Send + Sync> =
        Arc::new(PgAiOntologyRepository::new(pool.clone()));
    let domain_event_outbox_port: Arc<
        dyn fms_domain::ports::domain_event_outbox_repository::DomainEventOutboxRepository + Send + Sync,
    > = repos.domain_event_outbox_repo.clone();
    let db_metadata_port: Arc<dyn fms_domain::ports::database_metadata_port::DatabaseMetadataPort + Send + Sync> =
        Arc::new(
            fms_infrastructure::repositories::pg_database_metadata_adapter::PgDatabaseMetadataAdapter::new(
                pool.clone(),
            ),
        );
    let ai_execution_readiness_svc = Arc::new(
        AiExecutionReadinessService::new(Some(db_metadata_port.clone()), Some(domain_event_outbox_port.clone()))
            .with_proposal_repo(ai_proposal_repo.clone())
            .with_ontology_repo(ai_ontology_repo.clone()),
    );
    let ai_job_repo: Arc<dyn AiJobRepository + Send + Sync> = Arc::new(PgAiJobRepository::new(pool.clone()));
    let ai_run_repo: Arc<dyn AiRunRepository + Send + Sync> = Arc::new(PgAiRunRepository::new(pool.clone()));
    let ai_run_event_repo: Arc<dyn AiRunEventRepository + Send + Sync> =
        Arc::new(PgAiRunEventRepository::new(pool.clone()));

    let ai_proposal_audit_recorder: Arc<dyn AiProposalAuditEventRecorder> =
        Arc::new(PgAiProposalAuditEventRecorder::new(ai_run_event_repo.clone()));

    let stand_repo: Arc<dyn StandRepository + Send + Sync> = repos.stand_repo.clone();
    let ai_action_proposal_svc = Arc::new(
        AiActionProposalService::new()
            .with_repository(ai_proposal_repo.clone())
            .with_ai_runtime_service(ai_runtime_svc.clone())
            .with_notification_service(shared.notification_svc.clone())
            .with_domain_action_executor(domain_action_executor.clone())
            .with_object_policy_repository(ai_object_policy_repo.clone())
            .with_ontology_repository(ai_ontology_repo.clone())
            .with_pool(pool.clone())
            .with_flight_repository(repos.flight_repo.clone())
            .with_anomaly_repository(repos.anomaly_repo.clone())
            .with_stand_repository(stand_repo)
            .with_readiness_service(ai_execution_readiness_svc.clone())
            .with_audit_recorder(ai_proposal_audit_recorder),
    );
    let micro_model_registry = Arc::new(MicroModelRegistry::with_default_models());
    let ai_job_svc = Arc::new(AiJobService::new(
        ai_job_repo.clone(),
        ai_run_repo.clone(),
        ai_run_event_repo.clone(),
    ));

    let ai_ontology_repo_pg: Arc<dyn AiOntologyRepository + Send + Sync> =
        Arc::new(PgAiOntologyRepository::new(pool.clone()));
    let ai_output_validator = Arc::new(AiOutputValidator::new(flight_ops_v1::build_flight_ops_v1_schema()));

    let ai_proposal_ingest_svc = Arc::new(
        AiProposalIngestService::new(
            ai_output_validator.clone(),
            ai_action_proposal_svc.clone(),
            ai_job_svc.clone(),
        )
        .with_ontology_repository(ai_ontology_repo),
    );

    let ai_execution_metrics_svc = Arc::new(AiExecutionMetricsService::new(
        ai_proposal_repo.clone(),
        domain_event_outbox_port.clone(),
    ));

    let ai_rollout_status_svc = Arc::new(AiRolloutStatusService::new(
        ai_execution_readiness_svc.clone(),
        ai_execution_metrics_svc.clone(),
        ai_proposal_repo.clone(),
        repos.todo_repo.clone(),
        repos.todo_repo.clone(),
        db_metadata_port,
        pool.clone(),
        domain_event_outbox_port,
        ai_run_event_repo.clone(),
    ));

    let authorization_svc = Arc::new(AuthorizationService);
    let ai_context_svc = Arc::new(
        AiContextService::new(flight.flight_svc.clone(), authorization_svc)
            .with_dispatch_query_service(dispatch.dispatch_query_svc.clone())
            .with_anomaly_service(dispatch.anomaly_svc.clone())
            .with_business_case_service(business_case.business_case_svc.clone())
            .with_notification_service(shared.notification_svc.clone())
            .with_todo_service(shared.todo_svc.clone())
            .with_object_policy_repository(ai_object_policy_repo)
            .with_snapshot_repository(Arc::new(PgAiContextSnapshotRepository::new(pool.clone()))),
    );
    let ai_runtime_client = Arc::new(AiRuntimeClient::new());

    let ai_control_svc = Arc::new(build_ai_execution_control_service(
        pool.clone(),
        repos.ai_entity_config_repo.clone(),
        ai_job_svc.clone(),
    ));
    let ai_rollback_svc = Arc::new(build_ai_rollback_service(
        ai_action_proposal_svc.clone(),
        domain_action_executor.clone(),
        ai_control_svc.clone(),
        pool.clone(),
    ));
    let ai_recovery_orchestrator = build_ai_recovery_orchestrator(ai_control_svc.clone(), ai_rollback_svc.clone());
    let ai_event_consumer = Arc::new(AiEventConsumer::new(ai_control_svc.clone()));
    // Wire the transactional outbox into AiJobService so that terminal
    // transitions (complete_run / fail_run / cancel_job / timeout_job)
    // emit `ai_job.*` domain events for SSE fan-out (CDC → MQ → SSE).
    let outbox_tx_repo: Arc<
        dyn fms_application::sqlx_transactional_repositories::SqlxDomainEventOutboxTransactionalRepository,
    > = repos.domain_event_outbox_repo.clone();
    let ai_job_svc = Arc::new(
        AiJobService::new(ai_job_repo, ai_run_repo, ai_run_event_repo)
            .with_control_service(ai_control_svc.clone())
            .with_outbox_repository(outbox_tx_repo, pool.clone()),
    );
    let ai_job_timeout_reaper = Arc::new(AiJobTimeoutReaperService::new(
        ai_job_svc.clone(),
        ReaperConfig::default(),
    ));

    AiServices {
        ai_admin_svc,
        ai_route_svc,
        ai_media_svc,
        ai_business_case_copilot_svc,
        ai_realtime_audio_svc,
        ai_runtime_svc,
        ai_runtime_client,
        ai_action_proposal_svc,
        micro_model_registry,
        ai_job_svc,
        ai_ontology_repo: ai_ontology_repo_pg,
        ai_output_validator,
        ai_proposal_ingest_svc,
        ai_execution_readiness_svc,
        ai_execution_metrics_svc,
        ai_rollout_status_svc,
        ai_context_svc,
        ai_control_svc,
        ai_rollback_svc,
        ai_recovery_orchestrator,
        ai_event_consumer,
        ai_job_timeout_reaper,
    }
}

struct PgRunLifecycleHook {
    job_svc: Arc<AiJobService>,
}

#[async_trait]
impl RunLifecycleHook for PgRunLifecycleHook {
    async fn on_run_complete(
        &self,
        run_id: &str,
        output_raw: Value,
        token_usage: Option<Value>,
        _proposal_ids: &[String],
        _terminal_event_id: Option<&str>,
    ) -> Result<(), ControlServiceError> {
        let run = self
            .job_svc
            .get_run(run_id)
            .await
            .map_err(|e| ControlServiceError::InvalidState(format!("get_run failed: {e}")))?;

        // Propagate DB errors to the MQ consumer so transient failures
        // trigger a retry instead of silently acking the event.
        self.job_svc
            .complete_run(run_id, Some(output_raw), None, token_usage)
            .await
            .map_err(|e| ControlServiceError::InvalidState(format!("complete_run failed: {e}")))?;

        self.job_svc
            .transition_job(&run.job_id, AiJobStatus::Succeeded)
            .await
            .map_err(|e| ControlServiceError::InvalidState(format!("transition_job failed: {e}")))?;

        tracing::info!(
            target: "ai_execution_control",
            run_id = %run_id,
            job_id = %run.job_id,
            "run.complete: persisted run output and transitioned job to Succeeded"
        );
        Ok(())
    }

    async fn on_run_fail(
        &self,
        run_id: &str,
        error_code: &str,
        error_message: &str,
        _terminal_event_id: Option<&str>,
    ) -> Result<(), ControlServiceError> {
        let run = self
            .job_svc
            .get_run(run_id)
            .await
            .map_err(|e| ControlServiceError::InvalidState(format!("get_run failed: {e}")))?;

        self.job_svc
            .fail_run(run_id, Some(error_code), Some(error_message), None)
            .await
            .map_err(|e| ControlServiceError::InvalidState(format!("fail_run failed: {e}")))?;

        self.job_svc
            .transition_job(&run.job_id, AiJobStatus::FailedTerminal)
            .await
            .map_err(|e| ControlServiceError::InvalidState(format!("transition_job failed: {e}")))?;

        tracing::info!(
            target: "ai_execution_control",
            run_id = %run_id,
            job_id = %run.job_id,
            error_code = %error_code,
            "run.fail: persisted run error and transitioned job to FailedTerminal"
        );
        Ok(())
    }
}

fn build_ai_execution_control_service(
    pool: sqlx::PgPool,
    entity_config_repo: Arc<
        fms_infrastructure::repositories::pg_ai_entity_config_repository::PgAiEntityConfigRepository,
    >,
    job_svc: Arc<AiJobService>,
) -> AiExecutionControlService {
    let tool_call_repo: Arc<dyn AiToolCallRepository + Send + Sync> =
        Arc::new(PgAiToolCallRepository::new(pool.clone()));
    let command_repo: Arc<dyn AiRuntimeCommandRepository + Send + Sync> =
        Arc::new(PgAiRuntimeCommandRepository::new(pool.clone()));
    let checkpoint_repo: Arc<dyn AiRunCheckpointRepository + Send + Sync> =
        Arc::new(PgAiRunCheckpointRepository::new(pool.clone()));
    let feature_flags: Arc<
        dyn fms_application::services::ai_runtime_service::tool_authorization_service::FeatureFlagSource,
    > = Arc::new(StaticFeatureFlagSource::empty());
    let authorization = Arc::new(ToolAuthorizationService::new(feature_flags));
    let auth_loader: Arc<dyn RunAuthorizationContextLoader + Send + Sync> =
        Arc::new(PgRunAuthorizationContextLoader::new(pool, entity_config_repo));
    let run_lifecycle: Arc<dyn RunLifecycleHook> = Arc::new(PgRunLifecycleHook { job_svc });
    AiExecutionControlService::new(tool_call_repo, command_repo, authorization)
        .with_checkpoint_repo(checkpoint_repo)
        .with_auth_context_loader(auth_loader)
        .with_run_lifecycle_hook(run_lifecycle)
}

fn build_ai_rollback_service(
    proposal_service: Arc<AiActionProposalService>,
    domain_executor: Arc<DomainActionExecutor>,
    control_service: Arc<AiExecutionControlService>,
    pool: sqlx::PgPool,
) -> RollbackService {
    let receipt_repo: Arc<dyn AiActionReceiptRepository + Send + Sync> =
        Arc::new(PgAiActionReceiptRepository::new(pool.clone()));
    let checkpoint_repo: Arc<dyn AiRunCheckpointRepository + Send + Sync> =
        Arc::new(PgAiRunCheckpointRepository::new(pool.clone()));
    let plan_repo: Arc<dyn AiCompensationPlanRepository + Send + Sync> =
        Arc::new(PgAiCompensationPlanRepository::new(pool));
    let version_lookup = Arc::new(
        fms_application::services::ai_runtime_service::compensation_planner::InMemoryObjectVersionLookup::new(),
    )
        as Arc<dyn fms_application::services::ai_runtime_service::compensation_planner::ObjectVersionLookup>;
    let planner = Arc::new(CompensationPlanner::new(version_lookup));
    RollbackService::new(proposal_service, receipt_repo, plan_repo, planner)
        .with_checkpoint_repo(checkpoint_repo)
        .with_control_service(control_service)
        .with_domain_executor(domain_executor)
}

fn build_ai_recovery_orchestrator(
    control_service: Arc<AiExecutionControlService>,
    rollback_service: Arc<RollbackService>,
) -> Arc<RecoveryOrchestrator> {
    let tool_call_repo = control_service
        .tool_call_repo()
        .expect("control service must expose tool_call_repo for recovery orchestrator");
    let command_repo = control_service
        .command_repo()
        .expect("control service must expose command_repo for recovery orchestrator");
    let checkpoint_repo = control_service.checkpoint_repo();
    let deps = RecoveryOrchestratorDeps {
        tool_call_repo,
        command_repo,
        checkpoint_repo,
        rollback_service: Some(rollback_service),
        compensation_executing_timeout_seconds:
            fms_application::services::ai_runtime_service::recovery_orchestrator::DEFAULT_COMPENSATION_EXECUTING_TIMEOUT_SECONDS,
        compensation_auto_execute_grace_seconds:
            fms_application::services::ai_runtime_service::recovery_orchestrator::DEFAULT_COMPENSATION_AUTO_EXECUTE_GRACE_SECONDS,
    };
    Arc::new(RecoveryOrchestrator::new(deps, RecoveryOrchestratorConfig::default()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fms_application::services::ai_runtime_service::in_memory_repos::{
        InMemoryCheckpointRepository, InMemoryRuntimeCommandRepository, InMemoryToolCallRepository,
    };
    use fms_application::services::ai_runtime_service::recovery_orchestrator::{
        DEFAULT_COMPENSATION_AUTO_EXECUTE_GRACE_SECONDS, DEFAULT_COMPENSATION_EXECUTING_TIMEOUT_SECONDS,
        DEFAULT_RECOVERY_TICK_SECONDS,
    };

    fn build_test_control_service() -> AiExecutionControlService {
        let tool_call_repo: Arc<dyn AiToolCallRepository + Send + Sync> = Arc::new(InMemoryToolCallRepository::new());
        let command_repo: Arc<dyn AiRuntimeCommandRepository + Send + Sync> =
            Arc::new(InMemoryRuntimeCommandRepository::new());
        let checkpoint_repo: Arc<dyn AiRunCheckpointRepository + Send + Sync> =
            Arc::new(InMemoryCheckpointRepository::new());
        let feature_flags: Arc<
            dyn fms_application::services::ai_runtime_service::tool_authorization_service::FeatureFlagSource,
        > = Arc::new(StaticFeatureFlagSource::empty());
        let authorization = Arc::new(ToolAuthorizationService::new(feature_flags));
        AiExecutionControlService::new(tool_call_repo, command_repo, authorization)
            .with_checkpoint_repo(checkpoint_repo)
    }

    #[test]
    fn composition_root_wires_all_dependencies() {
        let control = build_test_control_service();
        assert!(control.tool_call_repo().is_some(), "tool_call_repo must be wired");
        assert!(control.command_repo().is_some(), "command_repo must be wired");
        assert!(control.checkpoint_repo().is_some(), "checkpoint_repo must be wired");

        let control_arc = Arc::new(control);
        let _event_consumer = AiEventConsumer::new(control_arc.clone());

        let tool_call_repo = control_arc.tool_call_repo().unwrap();
        let command_repo = control_arc.command_repo().unwrap();
        let checkpoint_repo = control_arc.checkpoint_repo();
        let deps = RecoveryOrchestratorDeps {
            tool_call_repo,
            command_repo,
            checkpoint_repo,
            rollback_service: None,
            compensation_executing_timeout_seconds: DEFAULT_COMPENSATION_EXECUTING_TIMEOUT_SECONDS,
            compensation_auto_execute_grace_seconds: DEFAULT_COMPENSATION_AUTO_EXECUTE_GRACE_SECONDS,
        };
        let orchestrator = Arc::new(RecoveryOrchestrator::new(deps, RecoveryOrchestratorConfig::default()));
        assert!(!orchestrator.is_running(), "orchestrator must start stopped");
        assert_eq!(
            orchestrator.config().tick_interval,
            std::time::Duration::from_secs(DEFAULT_RECOVERY_TICK_SECONDS)
        );
    }

    #[test]
    fn control_service_exposes_all_repositories() {
        let control = build_test_control_service();
        assert!(control.checkpoint_repo().is_some());
        assert!(control.command_repo().is_some());
        assert!(control.tool_call_repo().is_some());
    }
}
