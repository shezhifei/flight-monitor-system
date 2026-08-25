//! Server composition-root aliases.
//!
//! API handlers use trait-object aliases from `fms_api::types`; server-only
//! assembly code keeps infrastructure-bound aliases here.

pub use fms_api::types::{ConcreteAiBusinessCaseCopilotService, ConcreteEventRuleAdminService};
pub use fms_application::types::*;

use fms_application::services::dispatch_schedule_service::DispatchScheduleService;
use fms_application::services::dispatch_service::DispatchService;
use fms_application::services::resource_availability_service::ResourceAvailabilityService;

use fms_infrastructure::repositories::pg_dispatch_schedule_repository::{
    PgScheduleExceptionRepository, PgShiftInstanceRepository, PgShiftTemplateRepository,
};
use fms_infrastructure::repositories::pg_equipment_repository::PgEquipmentRepository;
use fms_infrastructure::repositories::pg_team_member_repository::PgTeamMemberRepository;
use fms_infrastructure::repositories::pg_team_repository::PgTeamRepository;

pub type ConcreteDispatchService = DispatchService;

// Keep API handlers and DI on the same monomorphized type.
pub use fms_application::types::ConcreteDispatchResourceService;

pub use fms_application::types::ConcreteDispatchScheduleService;
