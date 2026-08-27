//! Ontology application services for flight-ops actions.
//!
//! Each action is a named service. HTTP (`/api/v2/ai/ontology/actions/*`)
//! is the protocol adapter that selects the service. Approved writes go
//! through `DomainActionExecutor` onto existing domain services.
//!
//! Resource mutations for the ops desk live in `OntologyService`
//! (`/api/v2/ontology`).

mod anomaly_escalation_advisor_service;
mod anomaly_open_list_service;
mod briefing_service;
mod delay_advisor_service;
mod dispatch_replan_advisor_service;
mod dispatch_status_service;
mod equipment_context_service;
mod error;
mod flight_context_service;
mod flight_search_service;
mod notification_broadcast_advisor_service;
mod permissions;
mod personnel_context_service;
mod stand_availability_service;
mod stand_recommendation_service;
mod support;
mod team_context_service;

use std::sync::Arc;

use fms_domain::ports::anomaly_repository::AnomalyRepository;
use fms_domain::ports::business_case_repository::BusinessCaseRepository;
use fms_domain::ports::dispatch_repository::{
    DispatchOrderRepository, EquipmentRepository, PersonnelRuntimeRepository, QualificationGrantRepository,
    StandRepository, TeamRepository,
};
use fms_domain::ports::flight_repository::FlightRepository;
use fms_domain::ports::ontology_repository::StandOccupationRepository;
use fms_domain::ports::user_repository::UserRepository;

pub use anomaly_escalation_advisor_service::AnomalyEscalationAdvisorService;
pub use anomaly_open_list_service::AnomalyOpenListService;
pub use briefing_service::BriefingService;
pub use delay_advisor_service::DelayAdvisorService;
pub use dispatch_replan_advisor_service::DispatchReplanAdvisorService;
pub use dispatch_status_service::DispatchStatusService;
pub use equipment_context_service::EquipmentContextService;
pub use error::OntologyActionError;
pub use flight_context_service::FlightContextService;
pub use flight_search_service::FlightSearchService;
pub use notification_broadcast_advisor_service::NotificationBroadcastAdvisorService;
pub use permissions::{advisory_action_permission, read_action_permission};
pub use personnel_context_service::PersonnelContextService;
pub use stand_availability_service::StandAvailabilityService;
pub use stand_recommendation_service::StandRecommendationService;
pub use team_context_service::TeamContextService;

/// Composition of ontology action services. This is a wiring bundle, not a dispatcher.
pub struct OntologyActionServices {
    pub flight_context: FlightContextService,
    pub flight_search: FlightSearchService,
    pub dispatch_status: DispatchStatusService,
    pub anomaly_open_list: AnomalyOpenListService,
    pub stand_availability: StandAvailabilityService,
    pub briefing: BriefingService,
    pub stand_recommendation: StandRecommendationService,
    pub dispatch_replan: DispatchReplanAdvisorService,
    pub anomaly_escalation: AnomalyEscalationAdvisorService,
    pub delay: DelayAdvisorService,
    pub notification_broadcast: NotificationBroadcastAdvisorService,
    pub personnel_context: PersonnelContextService,
    pub team_context: TeamContextService,
    pub equipment_context: EquipmentContextService,
}

impl OntologyActionServices {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        flight_repo: Arc<dyn FlightRepository + Send + Sync>,
        dispatch_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
        anomaly_repo: Arc<dyn AnomalyRepository + Send + Sync>,
        team_repo: Arc<dyn TeamRepository + Send + Sync>,
        stand_repo: Arc<dyn StandRepository + Send + Sync>,
        stand_occupation_repo: Arc<dyn StandOccupationRepository + Send + Sync>,
        business_case_repo: Arc<dyn BusinessCaseRepository + Send + Sync>,
        user_repo: Arc<dyn UserRepository + Send + Sync>,
        personnel_runtime_repo: Arc<dyn PersonnelRuntimeRepository + Send + Sync>,
        qualification_grant_repo: Arc<dyn QualificationGrantRepository + Send + Sync>,
        equipment_repo: Arc<dyn EquipmentRepository + Send + Sync>,
    ) -> Self {
        Self {
            flight_context: FlightContextService::new(
                flight_repo.clone(),
                dispatch_repo.clone(),
                anomaly_repo.clone(),
                business_case_repo,
            ),
            flight_search: FlightSearchService::new(flight_repo.clone()),
            dispatch_status: DispatchStatusService::new(dispatch_repo.clone()),
            anomaly_open_list: AnomalyOpenListService::new(anomaly_repo.clone()),
            stand_availability: StandAvailabilityService::new(stand_repo.clone(), stand_occupation_repo.clone()),
            briefing: BriefingService::new(flight_repo.clone(), dispatch_repo.clone(), anomaly_repo.clone()),
            stand_recommendation: StandRecommendationService::new(
                flight_repo.clone(),
                stand_repo,
                stand_occupation_repo,
            ),
            dispatch_replan: DispatchReplanAdvisorService::new(dispatch_repo.clone()),
            anomaly_escalation: AnomalyEscalationAdvisorService::new(anomaly_repo.clone()),
            delay: DelayAdvisorService::new(flight_repo, dispatch_repo, anomaly_repo),
            notification_broadcast: NotificationBroadcastAdvisorService::new(),
            personnel_context: PersonnelContextService::new(user_repo, personnel_runtime_repo, qualification_grant_repo),
            team_context: TeamContextService::new(team_repo),
            equipment_context: EquipmentContextService::new(equipment_repo),
        }
    }
}

#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod tests;
