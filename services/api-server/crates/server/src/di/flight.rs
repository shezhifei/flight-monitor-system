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
use fms_application::sqlx_transactional_repositories::{
    SqlxFlightTimelineTransactionalRepository, SqlxFlightTransactionalRepository,
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
            flight_tx_repo,
            timeline_tx_repo,
            timeline_read,
            repos.pool.clone(),
        )
        .with_projection_repository(repos.flight_runtime_projection_repo.clone()),
    );
    let label_svc = Arc::new(LabelService::new(repos.label_repo.clone(), infra.sse_hub.clone()));
    let flight_import_svc = Arc::new(FlightImportService::new(flight_svc.clone()));
    let flight_archive_svc = Arc::new(FlightArchiveService::new(repos.flight_archive_repo.clone()));

    FlightServices {
        flight_svc,
        label_svc,
        flight_import_svc,
        flight_archive_svc,
        flight_cache_svc,
        flight_batch_cell_svc,
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
