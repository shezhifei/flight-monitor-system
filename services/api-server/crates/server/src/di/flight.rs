//! 航班领域服务装配：flight / label / flight_import / flight_archive /
//! flight_cache，以及需要 business_case 与 ai runtime 的 flight_runtime 服务。

use std::sync::Arc;

use crate::di::types::*;

use fms_application::services::flight_archive_service::FlightArchiveService;
use fms_application::services::flight_batch_cell_update_service::{
    FlightBatchCellUpdate, FlightBatchCellUpdateService,
};
use fms_application::services::flight_cache_service::FlightCacheService;
use fms_application::services::flight_import_service::FlightImportService;
use fms_application::services::flight_runtime_service::{
    DispatchTimelineWriter, FlightRuntimeService, FlightTimelineWriter,
};
use fms_application::services::flight_service::FlightService;
use fms_application::services::flight_writer::{FlightTransactionalWrites, FlightWriter, UowFlightWriter};
use fms_application::services::label_service::LabelService;
use fms_application::services::ontology_actions::OntologyActionServices;
use fms_application::services::ontology_service::OntologyService;
use fms_application::sqlx_transactional_repositories::{
    SqlxDomainEventOutboxTransactionalRepository, SqlxFlightTimelineTransactionalRepository,
    SqlxFlightTransactionalRepository, SqlxOntologyTransactionalRepository,
};
use fms_domain::ports::anomaly_repository::AnomalyRepository;
use fms_domain::ports::audit_log_repository::AuditLogRepository;
use fms_domain::ports::business_case_repository::BusinessCaseRepository;
use fms_domain::ports::dispatch_repository::{DispatchOrderRepository, StandRepository, TeamRepository};
use fms_domain::ports::domain_event_outbox_repository::DomainEventOutboxTransactionalRepository;
use fms_domain::ports::flight_repository::{FlightRepository, FlightTransactionalRepository};
use fms_domain::ports::flight_timeline_event_repository::FlightTimelineEventRepository;
use fms_domain::ports::ontology_repository::{
    AircraftRepository, GateAssignmentRepository, ResourceAdjustmentSuggestionRepository, StandOccupationRepository,
    TurnaroundLinkRepository,
};
use fms_infrastructure::repositories::pg_ontology_repository::{
    PgAircraftRepository, PgGateAssignmentRepository, PgResourceAdjustmentSuggestionRepository,
    PgStandOccupationRepository, PgTurnaroundLinkRepository,
};

use fms_infrastructure::cache::flight_cache_backend::RedisFlightCacheBackend;
use fms_infrastructure::repositories::pg_audit_log_repository::PgAuditLogRepository;
use fms_infrastructure::repositories::pg_flight_timeline_event_repository::PgFlightTimelineEventRepository;

use crate::di::ai::AiServices;
use crate::di::business_case::BusinessCaseServices;
use crate::di::shared::{SharedInfra, SharedRepos};

pub(crate) struct FlightServices {
    pub flight_svc: Arc<ConcreteFlightService>,
    pub flight_writer: Arc<FlightWriter<sqlx::Transaction<'static, sqlx::Postgres>>>,
    pub label_svc: Arc<ConcreteLabelService>,
    pub flight_import_svc: Arc<FlightImportService>,
    pub flight_archive_svc: Arc<FlightArchiveService>,
    pub flight_cache_svc: Arc<FlightCacheService>,
    pub flight_batch_cell_svc: Arc<dyn FlightBatchCellUpdate>,
    pub ontology_svc: Arc<OntologyService>,
    pub ontology_actions: Arc<OntologyActionServices>,
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
    let outbox_tx_repo: Arc<dyn SqlxDomainEventOutboxTransactionalRepository> = repos.domain_event_outbox_repo.clone();
    let timeline_pg = Arc::new(PgFlightTimelineEventRepository::new(repos.pool.clone()));
    let timeline_read: Arc<dyn FlightTimelineEventRepository + Send + Sync> = timeline_pg.clone();
    let timeline_tx_repo: Arc<dyn SqlxFlightTimelineTransactionalRepository> = timeline_pg;
    let flight_writer: Arc<FlightWriter<sqlx::Transaction<'static, sqlx::Postgres>>> = Arc::new(FlightWriter::new(
        repos.flight_repo.clone() as Arc<dyn FlightRepository + Send + Sync>,
        flight_tx_repo.clone()
            as Arc<dyn FlightTransactionalRepository<sqlx::Transaction<'static, sqlx::Postgres>> + Send + Sync>,
        outbox_tx_repo.clone()
            as Arc<
                dyn DomainEventOutboxTransactionalRepository<sqlx::Transaction<'static, sqlx::Postgres>> + Send + Sync,
            >,
    ));
    let flight_uow_writer: Arc<UowFlightWriter<_>> = Arc::new(UowFlightWriter::new(
        FlightWriter::new(
            repos.flight_repo.clone() as Arc<dyn FlightRepository + Send + Sync>,
            flight_tx_repo.clone()
                as Arc<dyn FlightTransactionalRepository<sqlx::Transaction<'static, sqlx::Postgres>> + Send + Sync>,
            outbox_tx_repo.clone()
                as Arc<
                    dyn DomainEventOutboxTransactionalRepository<sqlx::Transaction<'static, sqlx::Postgres>>
                        + Send
                        + Sync,
                >,
        ),
        repos.unit_of_work.clone(),
    ));
    let flight_svc = Arc::new(
        FlightService::new(repos.flight_repo.clone())
            .with_transactional_writer(flight_uow_writer as Arc<dyn FlightTransactionalWrites>),
    );
    let flight_batch_cell_svc: Arc<dyn FlightBatchCellUpdate> = Arc::new(
        FlightBatchCellUpdateService::new(
            repos.flight_repo.clone(),
            flight_tx_repo.clone(),
            timeline_tx_repo,
            timeline_read,
            outbox_tx_repo.clone(),
            repos.unit_of_work.clone(),
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
    let dispatch_order_port: Arc<dyn DispatchOrderRepository + Send + Sync> = repos.dispatch_order_repo.clone();
    let anomaly_port: Arc<dyn AnomalyRepository + Send + Sync> = repos.anomaly_repo.clone();
    let team_port: Arc<dyn TeamRepository + Send + Sync> = repos.team_repo.clone();
    let stand_port: Arc<dyn StandRepository + Send + Sync> = repos.stand_repo.clone();
    let business_case_port: Arc<dyn BusinessCaseRepository + Send + Sync> = repos.business_case_repo.clone();
    let aircraft_port: Arc<dyn AircraftRepository + Send + Sync> = aircraft_repo.clone();
    let occupation_port: Arc<dyn StandOccupationRepository + Send + Sync> = occupation_repo;
    let assignment_port: Arc<dyn GateAssignmentRepository + Send + Sync> = assignment_repo;
    let link_port: Arc<dyn TurnaroundLinkRepository + Send + Sync> = link_repo;
    let suggestion_port: Arc<dyn ResourceAdjustmentSuggestionRepository + Send + Sync> = suggestion_repo;

    let ontology_svc = Arc::new(
        OntologyService::new(
            repos.pool.clone(),
            flight_repo_port.clone(),
            flight_tx_repo,
            aircraft_port,
            occupation_port.clone(),
            assignment_port,
            link_port,
            suggestion_port,
            ontology_tx,
            outbox_tx_repo,
        )
        .with_flight_service(flight_svc.clone()),
    );

    let ontology_actions = Arc::new(OntologyActionServices::new(
        flight_repo_port,
        dispatch_order_port,
        anomaly_port,
        team_port,
        stand_port,
        occupation_port,
        business_case_port,
    ));

    FlightServices {
        flight_svc,
        flight_writer,
        label_svc,
        flight_import_svc,
        flight_archive_svc,
        flight_cache_svc,
        flight_batch_cell_svc,
        ontology_svc,
        ontology_actions,
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
    let timeline_writer: Arc<dyn DispatchTimelineWriter> = Arc::new(FlightTimelineWriter::new(
        timeline_pg,
        repos.domain_event_outbox_repo.clone(),
        repos.unit_of_work.clone(),
    ));
    Arc::new(
        FlightRuntimeService::new(flight.flight_svc.clone())
            .with_business_case_service(business_case.business_case_svc.clone())
            .with_projection_repository(repos.flight_runtime_projection_repo.clone())
            .with_audit_log_repository(audit_log_repo)
            .with_timeline_repository(timeline_repo, timeline_writer)
            .with_ai_runtime_service(ai.ai_runtime_svc.clone()),
    )
}
