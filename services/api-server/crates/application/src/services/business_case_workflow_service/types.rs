use std::collections::HashMap;

use fms_domain::models::business_case_workflow::BusinessCaseWorkflowRun;

use super::service::BusinessCaseWorkflowBatchItem;

#[derive(Debug, Clone)]
pub(super) struct FlowableStartSnapshot {
    pub(super) process_instance_id: String,
    pub(super) process_definition_id: Option<String>,
    pub(super) waiting_task_id: Option<String>,
    pub(super) status: String,
}

#[derive(Debug, Clone)]
pub(super) struct FlowableRunSnapshot {
    pub(super) process_instance: serde_json::Value,
    pub(super) active_tasks: Vec<serde_json::Value>,
    pub(super) historic_tasks: Vec<serde_json::Value>,
    pub(super) variables: serde_json::Map<String, serde_json::Value>,
    pub(super) wait_task_id: Option<String>,
    pub(super) receipt_group_id: Option<String>,
    pub(super) status: String,
}

#[derive(Debug, Clone)]
pub(super) struct RuntimeSnapshot {
    pub(super) process_instance: Option<serde_json::Value>,
    pub(super) active_tasks: Vec<serde_json::Value>,
    pub(super) historic_tasks: Vec<serde_json::Value>,
    pub(super) receipt_group: Option<serde_json::Value>,
    pub(super) flowable: Option<FlowableRunSnapshot>,
}

#[derive(Debug, Clone)]
pub(super) struct WorkflowStartOrchestration {
    pub(super) waiting_task_id: Option<String>,
    pub(super) receipt_group_id: Option<String>,
    pub(super) recipient_snapshot: Vec<HashMap<String, serde_json::Value>>,
    pub(super) status: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct WorkflowBatchPlanItem {
    pub(super) item: BusinessCaseWorkflowBatchItem,
    pub(super) business_case: fms_domain::models::business_case::FlightBusinessCase,
    pub(super) run: BusinessCaseWorkflowRun,
    pub(super) start_snapshot: FlowableStartSnapshot,
    pub(super) definition: WorkflowRuntimeDefinition,
    pub(super) recipients: Vec<HashMap<String, serde_json::Value>>,
    pub(super) notification_title: String,
    pub(super) notification_body: String,
    pub(super) receipt_required: bool,
    pub(super) notification_severity: String,
    pub(super) extra_info: HashMap<String, serde_json::Value>,
    pub(super) start_payload: HashMap<String, serde_json::Value>,
    pub(super) batch_policy: WorkflowBatchPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct WorkflowNotificationGroupKey {
    pub(super) template_code: String,
    pub(super) case_type: String,
    pub(super) notification_task_id: String,
    pub(super) recipient_set_hash: String,
    pub(super) receipt_required: bool,
    pub(super) severity: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WorkflowBatchNotificationIdempotencyContext {
    pub(super) receipt_group_id_override: Option<String>,
    pub(super) notification_id_seed: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WorkflowBatchPolicy {
    pub(super) notification_enabled: bool,
    pub(super) receipt_mode: WorkflowBatchReceiptMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum WorkflowBatchReceiptMode {
    SharedGroup,
    PerCase,
}

impl Default for WorkflowBatchPolicy {
    fn default() -> Self {
        Self {
            notification_enabled: false,
            receipt_mode: WorkflowBatchReceiptMode::PerCase,
        }
    }
}

impl WorkflowBatchPolicy {
    pub(super) fn should_group(&self, receipt_required: bool) -> bool {
        if !self.notification_enabled {
            return false;
        }
        !receipt_required || self.receipt_mode == WorkflowBatchReceiptMode::SharedGroup
    }
}

#[derive(Debug, Clone)]
pub(super) struct WorkflowRuntimeDefinition {
    pub(super) case_type: String,
    pub(super) notification_task_id: String,
    pub(super) wait_task_id: String,
    pub(super) notification_title: String,
    pub(super) notification_body: String,
    pub(super) notification_severity: String,
    pub(super) append_extra_info: bool,
    pub(super) notification_targets: Vec<WorkflowNotificationTarget>,
    pub(super) recipient_resolver: WorkflowRecipientResolverConfig,
    pub(super) receipt_required: bool,
    pub(super) completion_policy: String,
    pub(super) reject_policy: String,
    pub(super) success_action: WorkflowBusinessCaseAction,
    pub(super) failure_action: WorkflowBusinessCaseAction,
    pub(super) dispatch_tasks: HashMap<String, WorkflowDispatchTaskConfig>,
}

#[derive(Debug, Clone)]
pub(super) struct WorkflowNotificationTarget {
    pub(super) department: String,
    pub(super) roles: Vec<String>,
}

#[derive(Debug, Clone)]
pub(super) struct WorkflowRecipientResolverConfig {
    pub(super) source: String,
    pub(super) empty_policy: String,
    pub(super) deduplicate: bool,
}

#[derive(Debug, Clone)]
pub(super) struct WorkflowBusinessCaseAction {
    pub(super) node_id: String,
    pub(super) action: String,
    pub(super) target_status: String,
    pub(super) reason_template: Option<String>,
    pub(super) write_finished_at: bool,
    pub(super) require_case_id: bool,
}

#[derive(Debug, Clone)]
pub(super) struct WorkflowDispatchTaskConfig {
    pub(super) node_id: String,
    pub(super) node_name: String,
    pub(super) task_type: String,
    pub(super) target_department: String,
    pub(super) target_job_title: Option<String>,
    pub(super) required_people: i32,
    pub(super) priority: String,
    pub(super) description_template: Option<String>,
    pub(super) assignment_deadline_minutes: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReceiptWorkflowOutcome {
    Confirmed,
    Rejected,
}

impl ReceiptWorkflowOutcome {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Confirmed => "confirmed",
            Self::Rejected => "rejected",
        }
    }
}
