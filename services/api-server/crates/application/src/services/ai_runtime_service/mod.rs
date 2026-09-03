pub mod ai_event_consumer;
pub mod ai_execution_control_service;
pub mod compensation_planner;
pub mod controlplane_metrics;
mod helpers;
/// 内存测试替身。生产构建不编译。
#[cfg(any(test, feature = "test-support"))]
pub mod in_memory_repos;
pub mod recovery_orchestrator;
pub mod rollback_service;
pub mod service;
pub mod tool_authorization_service;
mod types;

#[cfg(test)]
mod tests;

pub use ai_event_consumer::AiEventConsumer;
pub use ai_execution_control_service::{
    AiExecutionControlService, ControlServiceError, LoggingProposalIngestHook, ProposalIngestHook,
    RunInputCheckpointSummary,
};
pub use compensation_planner::{
    CompensationError, CompensationPlanner, InMemoryObjectVersionLookup, ObjectVersionLookup,
};
pub use recovery_orchestrator::{
    build_recovery_orchestrator, RecoveryOrchestrator, RecoveryOrchestratorConfig, RecoveryOrchestratorCounters,
    RecoveryOrchestratorDeps, RecoveryScanReport,
};
pub use rollback_service::{RollbackError, RollbackService};
pub use service::AiRuntimeError;
pub use service::AiRuntimeService;
pub use service::AiToolExecutionSpec;
