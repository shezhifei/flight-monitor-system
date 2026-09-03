//! Server composition-root aliases.
//!
//! API handlers use trait-object aliases from `fms_api::types`; server-only
//! assembly code keeps infrastructure-bound aliases here.

pub use fms_api::types::{ConcreteAiBusinessCaseCopilotService, ConcreteEventRuleAdminService};
pub use fms_application::types::*;

use fms_application::services::dispatch_service::DispatchService;

pub type ConcreteDispatchService = DispatchService;

// Keep API handlers and DI on the same monomorphized type.
pub use fms_application::types::ConcreteDispatchResourceService;

pub use fms_application::types::ConcreteDispatchScheduleService;
