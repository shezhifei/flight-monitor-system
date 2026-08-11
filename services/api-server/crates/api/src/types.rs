//! API-facing service aliases.
//!
//! These aliases intentionally bind handlers to application/domain traits
//! instead of infrastructure repository implementations. The server
//! composition root owns concrete repository choices.

pub use fms_application::types::*;

use fms_application::services::ai_business_case_copilot_service::AiBusinessCaseCopilotService;
use fms_application::services::event_rule_admin_service::EventRuleAdminService;
use fms_application::services::runtime_diagnostics_service::RuntimeDiagnosticsService;
use fms_domain::ports::ai_copilot_repository::AiCopilotBusinessCaseBatchRepository;
use fms_domain::ports::dispatch_repository::DispatchOrderRepository;
use fms_domain::ports::event_rule_repository::EventRuleRepository;
use fms_domain::ports::runtime_diagnostic_repository::RuntimeDiagnosticRepository;

pub type ConcreteAiCopilotBusinessCaseBatchRepository = dyn AiCopilotBusinessCaseBatchRepository + Send + Sync;

pub type ConcreteAiBusinessCaseCopilotService =
    AiBusinessCaseCopilotService<ConcreteAiCopilotBusinessCaseBatchRepository>;

pub type ConcreteRuntimeDiagnosticsService = RuntimeDiagnosticsService<dyn RuntimeDiagnosticRepository>;

pub type ConcreteEventRuleAdminService =
    EventRuleAdminService<dyn EventRuleRepository + Send + Sync, dyn DispatchOrderRepository + Send + Sync>;
