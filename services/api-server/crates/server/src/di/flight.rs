//! 航班领域服务装配：flight / label / flight_import / flight_archive /
//! flight_cache，以及需要 business_case 与 ai runtime 的 flight_runtime 服务。

use std::sync::Arc;

use crate::di::types::*;

use fms_application::services::flight_archive_service::FlightArchiveService;
use fms_application::services::flight_batch_cell_update_service::FlightBatchCellUpdateService;
use fms_application::services::flight_cache_service::FlightCacheService;
use fms_application::services::flight_import_service::FlightImportService;
use fms_application::services::flight_runtime_service::FlightRuntimeService;
use fms_application::services::flight_service::FlightService;
use fms_application::services::label_service::LabelService;
use fms_application::services::ontology_service::OntologyService;
use fms_application::sqlx_transactional_repositories::{
    SqlxFlightTimelineTransactionalRepository, SqlxFlightTransactionalRepository,
    SqlxOntologyTransactionalRepository,
};
use fms_domain::ports::flight_repository::FlightRepository;
use fms_domain::ports::ontology_repository::{
    AircraftRepository, GateAssignmentRepository, ResourceAdjustmentSuggestionRepository,
    StandOccupationRepository, TurnaroundLinkRepository,
};
use fms_infrastructure::repositories::pg_ontology_repository::{
    PgAircraftRepository, PgGateAssignmentRepository, PgResourceAdjustmentSuggestionRepository,
    PgStandOccupationRepository, PgTurnaroundLinkRepository,
};
use fms_domain::ports::audit_log_repository::AuditLogRepository;
use fms_domain::ports::flight_timeline_event_repository::FlightTimelineEventRepository;

use fms_infrastructure::cache::flight_cache_backend::RedisFlightCacheBackend;
use fms_infrastructure::repositories::pg_audit_log_repository::PgAuditLogRepository;
use fms_infrastructure::repositories::pg_flight_timeline_event_repository::PgFlightTimelineEventRepository;

use crate::di::ai::AiServices;
use crate::di::business_case::BusinessCaseServices;
use crate::di::shared::{SharedInfra, SharedRepos};

pub(crate) struct FlightServices {
    pub flight_svc: Arc<ConcreteFlightService>,
    pub label_svc: Arc<ConcreteLabelService>,
    pub flight_import_svc: Arc<FlightImportService>,
    pub flight_archive_svc: Arc<FlightArchiveService>,
    pub flight_cache_svc: Arc<FlightCacheService>,
    pub flight_batch_cell_svc: Arc<FlightBatchCellUpdateService>,
    pub ontology_svc: Arc<OntologyService>,
}

pub(crate) fn build_flight_services(
    repos: &SharedRepos,
    infra: &SharedInfra,
    redis_manager: &Option<Arc<fms_infrastructure::cache::RedisPool>>,
) -> FlightServices {
    let flight_cache_svc = match redis_manager.clone() {
        Some(redis_manager) => Arc::new(FlightCacheService::with_backend(Arc::new(
            RedisFlightCacheBackend::new(redis_manager.as_ref().clone()),
        ))),
        None => Arc::new(FlightCacheService::disabled()),
    };

    let flight_tx_repo: Arc<dyn SqlxFlightTransactionalRepository> = repos.flight_repo.clone();
    let timeline_pg = Arc::new(PgFlightTimelineEventRepository::new(repos.pool.clone()));
    let timeline_read: Arc<dyn FlightTimelineEventRepository + Send + Sync> = timeline_pg.clone();
    let timeline_tx_repo: Arc<dyn SqlxFlightTimelineTransactionalRepository> = timeline_pg;
    let flight_svc = Arc::new(
        FlightService::new(repos.flight_repo.clone())
            .with_transactional_repository(flight_tx_repo.clone())
            .with_pool(repos.pool.clone()),
    );
    let flight_batch_cell_svc = Arc::new(
        FlightBatchCellUpdateService::new(
            repos.flight_repo.clone(),
            flight_tx_repo.clone(),
            timeline_tx_repo,
            timeline_read,
            repos.pool.clone(),
        )
        .with_projection_repository(repos.flight_runtime_projection_repo.clone()),
    );
    let label_svc = Arc::new(LabelService::new(repos.label_repo.clone(), infra.sse_hub.clone()));
    let flight_import_svc = Arc::new(FlightImportService::new(flight_svc.clone()));
    let flight_archive_svc = Arc::new(FlightArchiveService::new(repos.flight_archive_repo.clone()));

    let aircraft_repo = Arc::new(PgAircraftRepository::new(repos.pool.clone()));
    let occupation_repo = Arc::new(PgStandOccupationRepository::new(repos.pool.clone()));
    let assignment_repo = Arc::new(PgGateAssignmentRepository::new(repos.pool.clone()));
    let link_repo = Arc::new(PgTurnaroundLinkRepository::new(repos.pool.clone()));
    let suggestion_repo = Arc::new(PgResourceAdjustmentSuggestionRepository::new(repos.pool.clone()));
    let ontology_tx: Arc<dyn SqlxOntologyTransactionalRepository> = aircraft_repo.clone();
    let flight_repo_port: Arc<dyn FlightRepository + Send + Sync> = repos.flight_repo.clone();
    let aircraft_port: Arc<dyn AircraftRepository + Send + Sync> = aircraft_repo.clone();
    let occupation_port: Arc<dyn StandOccupationRepository + Send + Sync> = occupation_repo;
    let assignment_port: Arc<dyn GateAssignmentRepository + Send + Sync> = assignment_repo;
    let link_port: Arc<dyn TurnaroundLinkRepository + Send + Sync> = link_repo;
    let suggestion_port: Arc<dyn ResourceAdjustmentSuggestionRepository + Send + Sync> = suggestion_repo;

    let ontology_svc = Arc::new(
        OntologyService::new(
            repos.pool.clone(),
            flight_repo_port,
            flight_tx_repo,
            aircraft_port,
            occupation_port,
            assignment_port,
            link_port,
            suggestion_port,
            ontology_tx,
        )
        .with_flight_service(flight_svc.clone()),
    );

    FlightServices {
        flight_svc,
        label_svc,
        flight_import_svc,
        flight_archive_svc,
        flight_cache_svc,
        flight_batch_cell_svc,
        ontology_svc,
    }
}

pub(crate) fn build_flight_runtime_service(
    repos: &SharedRepos,
    flight: &FlightServices,
    business_case: &BusinessCaseServices,
    ai: &AiServices,
) -> Arc<FlightRuntimeService> {
    let audit_log_repo: Arc<dyn AuditLogRepository + Send + Sync> =
        Arc::new(PgAuditLogRepository::new(repos.pool.clone()));
    let timeline_pg = Arc::new(PgFlightTimelineEventRepository::new(repos.pool.clone()));
    let timeline_repo: Arc<dyn FlightTimelineEventRepository + Send + Sync> = timeline_pg.clone();
    let timeline_tx_repo: Arc<dyn SqlxFlightTimelineTransactionalRepository> = timeline_pg;
    Arc::new(
        FlightRuntimeService::new(repos.pool.clone(), flight.flight_svc.clone())
            .with_business_case_service(business_case.business_case_svc.clone())
            .with_projection_repository(repos.flight_runtime_projection_repo.clone())
            .with_audit_log_repository(audit_log_repo)
            .with_timeline_repository(timeline_repo, timeline_tx_repo)
            .with_ai_runtime_service(ai.ai_runtime_svc.clone()),
    )
}
