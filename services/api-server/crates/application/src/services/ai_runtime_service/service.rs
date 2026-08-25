use chrono::{DateTime, Utc};
use metrics::{counter, histogram};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use ulid::Ulid;

use fms_domain::error::DomainError;
use fms_domain::ports::todo_agent_context_repository::TodoAgentContextRepository;
use fms_domain::ports::todo_repository::TodoRepository;
use fms_runtime::spawn_tracked::spawn_tracked;

use crate::services::notification_service::{NotificationCreate, NotificationService};

pub(crate) use super::helpers::*;
pub(crate) use super::types::*;

#[derive(Debug, Clone)]
pub struct AiToolExecutionSpec {
    pub tool_name: String,
    pub category: String,
    pub operation_level: String,
    pub side_effect: bool,
    pub query_intent: Option<String>,
    pub query_dataset: Option<String>,
}

#[derive(Debug)]
pub enum AiRuntimeError {
    NotFound(String),
    Validation(String),
    Conflict {
        code: String,
        message: String,
        blocked_reason: Option<String>,
    },
}

impl AiRuntimeError {
    pub fn not_found(id: impl Into<String>) -> Self {
        Self::NotFound(id.into())
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }

    pub fn conflict(code: impl Into<String>, message: impl Into<String>, blocked_reason: Option<String>) -> Self {
        Self::Conflict {
            code: code.into(),
            message: message.into(),
            blocked_reason,
        }
    }
}

pub(super) const PENDING_ACTION_RETENTION_MINUTES: i64 = 60;
pub(super) const MAX_PENDING_ACTIONS: usize = 128;
pub(super) const EXECUTION_RETENTION_HOURS: i64 = 12;
pub(super) const MAX_EXECUTIONS: usize = 512;
pub(super) const CHAIN_RETENTION_HOURS: i64 = 12;
pub(super) const MAX_CHAINS: usize = 64;
pub(super) const METRIC_SAMPLE_WINDOW: usize = 512;
pub(super) const DEFAULT_TODO_GRAPH_PILOT_ENTITY_ID: &str = "todo_graph_pilot";
pub(super) const READY_GRAPH_REQUESTED_TOTAL_MIN: i64 = 30;
pub(super) const READY_COMPLETION_RATE_MIN: f64 = 0.95;
pub(super) const READY_GRAPH_FALLBACK_RATE_MAX: f64 = 0.05;
pub(super) const READY_GRAPH_RESUME_TOTAL_MIN: i64 = 5;
pub(super) const READY_GRAPH_RESUME_SUCCESS_RATE_MIN: f64 = 0.95;
pub(super) const READY_DUPLICATE_TOOL_EXECUTION_TOTAL_MAX: i64 = 0;
pub(super) const READY_DUPLICATE_TOOL_EXECUTION_BLOCKED_TOTAL_MAX: i64 = 0;
pub(super) const READY_STALE_PENDING_TOTAL_MAX: i64 = 0;
pub(super) const ROLLBACK_GRAPH_REQUESTED_TOTAL_MIN: i64 = 10;
pub(super) const ROLLBACK_GRAPH_FALLBACK_RATE_GT: f64 = 0.20;
pub(super) const ROLLBACK_GRAPH_RESUME_TOTAL_MIN: i64 = 5;
pub(super) const ROLLBACK_GRAPH_RESUME_SUCCESS_RATE_LT: f64 = 0.90;
pub(super) const AI_QUERY_ROUTE_TOTAL_METRIC: &str = "ai_query_route_total";
pub(super) const AI_QUERY_MISROUTE_TOTAL_METRIC: &str = "ai_query_misroute_total";
pub(super) const AI_QUERY_SELECTION_TOTAL_METRIC: &str = "ai_query_selection_total";
pub(super) const AI_QUERY_MISSELECTION_TOTAL_METRIC: &str = "ai_query_misselection_total";
pub(super) const AI_REPORT_SCHEMA_VALIDATION_TOTAL_METRIC: &str = "ai_report_schema_validation_total";
pub(super) const AI_REPORT_SCHEMA_VALIDATION_ERROR_COUNT_METRIC: &str = "ai_report_schema_validation_error_count";

pub trait AiRuntimeNotificationSender: Send + Sync {
    fn send_ai_runtime_notification<'a>(
        &'a self,
        notification: NotificationCreate,
    ) -> Pin<Box<dyn Future<Output = Result<(), DomainError>> + Send + 'a>>;
}

impl<NR, PR, CE, DP, MR, RS> AiRuntimeNotificationSender for NotificationService<NR, PR, CE, DP, MR, RS>
where
    NR: fms_domain::ports::notification_repository::NotificationRepository + Send + Sync + ?Sized,
    PR: fms_domain::ports::notification_repository::NotificationPreferenceRepository + Send + Sync + ?Sized,
    CE: crate::services::notification_service::NotificationCollaborationEvents + Send + Sync + ?Sized,
    DP: crate::services::notification_service::NotificationDeliveryPublisher + Send + Sync + ?Sized,
    MR: crate::services::notification_service::NotificationMetricsRecorder + Send + Sync + ?Sized,
    RS: crate::services::notification_service::NotificationReceiptGroupSync + Send + Sync + ?Sized,
{
    fn send_ai_runtime_notification<'a>(
        &'a self,
        notification: NotificationCreate,
    ) -> Pin<Box<dyn Future<Output = Result<(), DomainError>> + Send + 'a>> {
        Box::pin(async move {
            self.send_notification(notification).await?;
            Ok(())
        })
    }
}

pub struct AiRuntimeService {
    pub(super) state: Arc<RwLock<AiRuntimeState>>,
    prune_scheduled: Arc<AtomicBool>,
    notification_service: Option<Arc<dyn AiRuntimeNotificationSender>>,
    todo_repository: Option<Arc<dyn TodoRepository + Send + Sync>>,
    todo_agent_context_repository: Option<Arc<dyn TodoAgentContextRepository + Send + Sync>>,
    ai_job_service: Option<Arc<crate::services::ai_job_service::AiJobService>>,
}

impl Default for AiRuntimeService {
    fn default() -> Self {
        Self::new()
    }
}

impl AiRuntimeService {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(AiRuntimeState::default())),
            prune_scheduled: Arc::new(AtomicBool::new(false)),
            notification_service: None,
            todo_repository: None,
            todo_agent_context_repository: None,
            ai_job_service: None,
        }
    }

    pub fn with_notification_service(mut self, notification_service: Arc<dyn AiRuntimeNotificationSender>) -> Self {
        self.notification_service = Some(notification_service);
        self
    }

    pub(super) fn schedule_prune(&self) {
        if self
            .prune_scheduled
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }

        let state = Arc::clone(&self.state);
        let prune_scheduled = Arc::clone(&self.prune_scheduled);
        spawn_tracked("ai_runtime:prune", async move {
            let mut state = state.write().await;
            state.prune(Utc::now());
            prune_scheduled.store(false, Ordering::Release);
        });
    }

    #[cfg(test)]
    pub(super) fn is_prune_scheduled(&self) -> bool {
        self.prune_scheduled.load(Ordering::Acquire)
    }
}

impl AiRuntimeService {
    pub fn with_todo_repository(mut self, todo_repository: Arc<dyn TodoRepository + Send + Sync>) -> Self {
        self.todo_repository = Some(todo_repository);
        self
    }

    pub fn with_todo_agent_context_repository(
        mut self,
        todo_agent_context_repository: Arc<dyn TodoAgentContextRepository + Send + Sync>,
    ) -> Self {
        self.todo_agent_context_repository = Some(todo_agent_context_repository);
        self
    }

    pub fn with_ai_job_service(mut self, ai_job_service: Arc<crate::services::ai_job_service::AiJobService>) -> Self {
        self.ai_job_service = Some(ai_job_service);
        self
    }

    pub async fn execute_tool(
        &self,
        spec: AiToolExecutionSpec,
        tool_args: Value,
        requester_user_id: Option<String>,
        requester_user_roles: Vec<String>,
    ) -> Value {
        let now = Utc::now();
        let decision = if spec.side_effect {
            "pending_approval"
        } else {
            "executed"
        };
        counter!(
            "fms_ai_tool_executions_total",
            "tool" => spec.tool_name.clone(),
            "decision" => decision
        )
        .increment(1);
        self.schedule_prune();
        let mut state = self.state.write().await;
        let execution_id = next_id("exec");
        let tool_call_id = next_id("call");

        if spec.side_effect {
            let action_id = next_id("pending_action");
            let action = PendingActionRecord::new(
                action_id.clone(),
                spec.clone(),
                tool_args.clone(),
                tool_call_id.clone(),
                requester_user_id.clone(),
                requester_user_roles.clone(),
                execution_id.clone(),
                now,
            );
            state.pending_order.push(action_id.clone());
            state.pending_actions.insert(action_id.clone(), action.clone());
            state.execution_order.push(execution_id.clone());
            state.executions.insert(
                execution_id.clone(),
                ExecutionRecord::pending_approval(
                    execution_id.clone(),
                    spec.tool_name.clone(),
                    tool_args.clone(),
                    requester_user_id.clone(),
                    requester_user_roles,
                    Some(action_id.clone()),
                    now,
                ),
            );
            state.evict_old_pending_actions();
            state.evict_old_executions();
            drop(state);

            self.notify_requester_best_effort(requester_user_id.as_deref(), &action.to_value(), "pending")
                .await;

            let pending_message = format!(
                "工具 '{}' 已进入人工审批队列 (operation_level={})",
                spec.tool_name, spec.operation_level
            );

            return json!({
                "success": false,
                "status": "pending_approval",
                "code": "TOOL_PENDING_APPROVAL",
                "message": format!("tool '{}' is queued for human approval", spec.tool_name),
                "recoverable": true,
                "retryable": false,
                "severity": "warning",
                "execution_id": execution_id,
                "tool_name": spec.tool_name,
                "approval_required": true,
                "approval_id": action_id,
                "data": action.to_value(),
                "error": pending_message,
                "meta": {
                    "duration_ms": 0,
                    "contract_version": "2.0"
                },
            });
        }

        let finished_at = now + chrono::Duration::milliseconds(28);
        let output = build_read_tool_output(&spec, &tool_args, &execution_id, &tool_call_id, now, finished_at);
        state.execution_order.push(execution_id.clone());
        state.executions.insert(
            execution_id.clone(),
            ExecutionRecord::success(
                execution_id.clone(),
                spec.tool_name.clone(),
                output.clone(),
                requester_user_id,
                requester_user_roles,
                now,
            )
            .with_input(tool_args.clone()),
        );
        state.evict_old_executions();
        state.record_visibility_sample(80.0, 320.0);
        drop(state);

        if let Some(metric) = resolve_query_route_metric(&spec, &tool_args) {
            self.record_query_route(
                &metric.intent,
                &metric.dataset,
                &metric.adapter,
                metric.status,
                metric.misroute,
                metric.reason,
            )
            .await;
        }

        json!({
            "success": true,
            "status": "success",
            "code": "TOOL_SUCCESS",
            "message": "tool executed successfully",
            "recoverable": false,
            "retryable": false,
            "execution_id": execution_id,
            "tool_name": spec.tool_name,
            "severity": "success",
            "approval_required": false,
            "approval_id": Value::Null,
            "data": output,
            "error": Value::Null,
            "meta": {
                "duration_ms": 28,
                "contract_version": "2.0",
                "tool_call_id": tool_call_id,
            },
        })
    }

    pub async fn list_pending_actions(
        &self,
        status: Option<&str>,
        tool_name: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Value {
        let state = self.state.read().await;
        let items = state.filter_pending_actions(status, tool_name);
        let total_count = items.len();
        let bounded_limit = limit.clamp(1, 200);
        let bounded_offset = offset.min(total_count);
        let window = items
            .into_iter()
            .skip(bounded_offset)
            .take(bounded_limit)
            .collect::<Vec<_>>();
        let has_more = bounded_offset + window.len() < total_count;
        let next_offset = has_more.then_some(bounded_offset + window.len());
        let visible_count = window.len();
        json!({
            "items": window.into_iter().map(|item| item.to_value()).collect::<Vec<_>>(),
            "total": visible_count,
            "total_count": total_count,
            "pagination": {
                "limit": bounded_limit,
                "offset": bounded_offset,
                "next_offset": next_offset,
                "has_more": has_more,
            },
        })
    }

    pub async fn get_pending_action_diff(&self, action_id: &str) -> Result<Value, AiRuntimeError> {
        let state = self.state.read().await;
        let action = state
            .pending_actions
            .get(action_id)
            .ok_or_else(|| AiRuntimeError::not_found(action_id))?;
        Ok(json!({
            "action_id": action.action_id,
            "tool_name": action.tool_name,
            "before_snapshot": action.before_snapshot,
            "after_snapshot": action.after_snapshot,
            "json_patch": action.json_patch,
            "diff_summary": action.diff_summary,
            "diff_source": action.diff_source,
            "ui_hints": action.ui_hints,
        }))
    }

    pub async fn get_pending_action_result(&self, action_id: &str) -> Result<Value, AiRuntimeError> {
        let state = self.state.read().await;
        let action = state
            .pending_actions
            .get(action_id)
            .ok_or_else(|| AiRuntimeError::not_found(action_id))?;
        Ok(json!({
            "action_id": action.action_id,
            "status": action.status,
            "status_code": action.status_code,
            "execution_result": action.execution_result,
            "execution_error": action.execution_error,
            "execution_receipt": action.execution_receipt,
            "error_payload": action.error_payload,
        }))
    }

    pub async fn approve_pending_action(
        &self,
        action_id: &str,
        approver_id: &str,
        modified_arguments: Option<Value>,
    ) -> Result<Value, AiRuntimeError> {
        let now = Utc::now();
        self.schedule_prune();
        let mut state = self.state.write().await;
        let (
            pending_action,
            execution_result,
            execution_output,
            run_id,
            tool_name,
            requester_user_id,
            user_roles,
            approval_id,
        ) = {
            let action = state
                .pending_actions
                .get_mut(action_id)
                .ok_or_else(|| AiRuntimeError::not_found(action_id))?;
            ensure_action_open(action, now)?;

            let final_arguments = merge_json_objects(action.arguments.clone(), modified_arguments.clone());
            action.status = "executed".to_string();
            action.approved_by = Some(approver_id.to_string());
            action.approved_at = Some(now);
            action.updated_at = now;
            let finished_at = now + chrono::Duration::milliseconds(64);
            let execution_output = build_write_tool_output(
                &action.tool_name,
                &action.operation_level,
                &final_arguments,
                action.correlation_id.as_deref(),
                &action.tool_call_id,
                approver_id,
                now,
                finished_at,
            );
            action.execution_result = Some(execution_output.clone());
            action.execution_receipt = Some(build_execution_receipt(
                action.correlation_id.as_deref(),
                &action.tool_call_id,
                "success",
                action.approved_by.as_deref(),
                now,
                finished_at,
            ));
            action.status_code = Some("EXECUTED".to_string());
            action.error_payload = None;
            action.decision_blocked_reason = None;

            (
                action.to_value(),
                json!({
                    "status": "success",
                    "code": "EXECUTED",
                    "message": "approved action executed successfully",
                    "recoverable": false,
                    "retryable": false,
                    "severity": "info",
                    "execution_id": action.correlation_id,
                    "data": {
                        "tool_name": action.tool_name,
                        "result": execution_output,
                        "error": Value::Null,
                    },
                    "result_data": {
                        "execution_id": action.correlation_id,
                        "tool_call_id": action.tool_call_id,
                        "status": "success",
                    },
                }),
                execution_output,
                action.correlation_id.clone(),
                action.tool_name.clone(),
                action.requester_user_id.clone(),
                action.requester_user_roles.clone(),
                action.action_id.clone(),
            )
        };

        if let Some(run_id) = run_id {
            state.executions.insert(
                run_id.clone(),
                ExecutionRecord::success(
                    run_id.clone(),
                    tool_name,
                    execution_output,
                    requester_user_id.clone(),
                    user_roles,
                    now,
                )
                .with_approval(Some(approval_id)),
            );
            state.execution_order.retain(|id| id != &run_id);
            state.execution_order.push(run_id);
            state.evict_old_executions();
        }
        state.record_visibility_sample(120.0, 480.0);
        drop(state);

        self.notify_requester_best_effort(requester_user_id.as_deref(), &pending_action, "approved")
            .await;

        Ok(json!({
            "pending_action": pending_action,
            "execution_result": execution_result,
            "modification": modified_arguments.map(|value| json!({ "modified_arguments": value })),
        }))
    }

    pub async fn reject_pending_action(
        &self,
        action_id: &str,
        approver_id: &str,
        reason: Option<&str>,
    ) -> Result<Value, AiRuntimeError> {
        let now = Utc::now();
        self.schedule_prune();
        let mut state = self.state.write().await;
        let (pending_action, run_id, tool_name, rejection_reason, requester_user_id, user_roles, approval_id) = {
            let action = state
                .pending_actions
                .get_mut(action_id)
                .ok_or_else(|| AiRuntimeError::not_found(action_id))?;
            ensure_action_open(action, now)?;

            action.status = "rejected".to_string();
            action.rejected_by = Some(approver_id.to_string());
            action.rejected_reason = Some(
                reason
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("rejected_by_human")
                    .to_string(),
            );
            action.rejected_at = Some(now);
            action.updated_at = now;
            action.status_code = Some("APPROVAL_REJECTED".to_string());
            action.execution_error = Some("approval request rejected by human reviewer".to_string());
            action.execution_receipt = Some(build_execution_receipt(
                action.correlation_id.as_deref(),
                &action.tool_call_id,
                "rejected",
                Some(approver_id),
                now,
                now,
            ));
            action.error_payload = Some(json!({
                "reason": action.rejected_reason,
                "tool_name": action.tool_name,
                "tool_call_id": action.tool_call_id,
            }));

            (
                action.to_value(),
                action.correlation_id.clone(),
                action.tool_name.clone(),
                action.rejected_reason.clone(),
                action.requester_user_id.clone(),
                action.requester_user_roles.clone(),
                action.action_id.clone(),
            )
        };

        if let Some(run_id) = run_id {
            state.executions.insert(
                run_id.clone(),
                ExecutionRecord::rejected(
                    run_id.clone(),
                    tool_name,
                    rejection_reason,
                    requester_user_id.clone(),
                    user_roles,
                    Some(approval_id),
                    now,
                ),
            );
            state.execution_order.retain(|id| id != &run_id);
            state.execution_order.push(run_id);
            state.evict_old_executions();
        }
        drop(state);

        self.notify_requester_best_effort(requester_user_id.as_deref(), &pending_action, "rejected")
            .await;

        Ok(json!({ "pending_action": pending_action }))
    }

    pub async fn list_executions(
        &self,
        todo_id: Option<&str>,
        entity_id: Option<&str>,
        status: Option<&str>,
        limit: usize,
    ) -> Vec<Value> {
        let state = self.state.read().await;
        state
            .execution_order
            .iter()
            .rev()
            .filter_map(|id| state.executions.get(id))
            .filter(|item| matches_optional(&item.todo_id, todo_id))
            .filter(|item| matches_optional(&item.entity_id, entity_id))
            .filter(|item| status.map(|value| item.status == value).unwrap_or(true))
            .take(limit.clamp(1, 200))
            .map(ExecutionRecord::to_value)
            .collect()
    }

    pub async fn get_execution(&self, run_id: &str) -> Option<Value> {
        let state = self.state.read().await;
        if let Some(execution) = state.executions.get(run_id) {
            return Some(execution.to_value());
        }

        state
            .pending_actions
            .values()
            .find(|action| action.correlation_id.as_deref() == Some(run_id))
            .map(PendingActionRecord::to_execution_value)
    }

    async fn notify_requester_best_effort(
        &self,
        requester_user_id: Option<&str>,
        pending_action: &Value,
        decision: &str,
    ) {
        let Some(notification_service) = self.notification_service.as_ref() else {
            return;
        };
        let Some(user_id) = requester_user_id.map(str::trim).filter(|value| !value.is_empty()) else {
            return;
        };

        let tool_name = pending_action
            .get("tool_name")
            .and_then(Value::as_str)
            .unwrap_or("unknown_tool");
        let action_id = pending_action
            .get("action_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let reason = pending_action.get("reason").and_then(Value::as_str).unwrap_or_default();

        let (title, body) = if decision == "approved" {
            (
                format!("AI 工具 '{}' 审批已通过", tool_name),
                format!("动作 {} 状态: approved", action_id),
            )
        } else if decision == "pending" {
            let body = if reason.is_empty() {
                format!("动作 {} 状态: pending", action_id)
            } else {
                format!("动作 {} 状态: pending；原因: {}", action_id, reason)
            };
            (format!("AI 工具 '{}' 已进入审批队列", tool_name), body)
        } else {
            let body = if reason.is_empty() {
                format!("动作 {} 状态: rejected", action_id)
            } else {
                format!("动作 {} 状态: rejected；原因: {}", action_id, reason)
            };
            (format!("AI 工具 '{}' 审批已被拒绝", tool_name), body)
        };

        let _ = notification_service
            .send_ai_runtime_notification(NotificationCreate {
                user_id: user_id.to_string(),
                title,
                body,
                category: Some("ai_approval".to_string()),
                severity: Some(match decision {
                    "approved" => "info".to_string(),
                    _ => "warning".to_string(),
                }),
                flight_id: None,
                related_entity_type: Some("pending_action".to_string()),
                related_entity_id: pending_action
                    .get("action_id")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                dispatch_order_id: None,
                group_id: None,
                sender_user_id: None,
                sender_username_snapshot: None,
                origin_type: Some("ai".to_string()),
                receipt_required: false,
                receipt_group_id: None,
            })
            .await;
    }

    pub async fn cancel_execution(&self, run_id: &str) -> bool {
        let now = Utc::now();
        self.schedule_prune();
        let mut state = self.state.write().await;
        let Some(item) = state.executions.get_mut(run_id) else {
            return false;
        };
        if item.status == "success" || item.status == "failed" || item.status == "cancelled" {
            return false;
        }
        item.status = "cancelled".to_string();
        item.finished_at = Some(now);
        item.updated_at = now;
        item.error_message = Some("execution cancelled by user".to_string());
        true
    }

    pub async fn rate_limit_status(&self) -> Value {
        let state = self.state.read().await;
        let executions = state.executions.len() as i64;
        let rpm_limit = 60_i64;
        let tpm_limit = 100_000_i64;
        let rpm_used = executions.min(rpm_limit);
        let tpm_used = (executions * 256).min(tpm_limit);
        json!({
            "rpm_used": rpm_used,
            "rpm_limit": rpm_limit,
            "tpm_used": tpm_used,
            "tpm_limit": tpm_limit,
            "rpm_remaining": (rpm_limit - rpm_used).max(0),
            "tpm_remaining": (tpm_limit - tpm_used).max(0),
            "rpm_percentage": percentage(rpm_used, rpm_limit),
            "tpm_percentage": percentage(tpm_used, tpm_limit),
        })
    }

    pub async fn execute_todo(
        &self,
        todo_id: &str,
        entity_id: Option<String>,
        max_iterations: usize,
        system_prompt_override: Option<String>,
        user_id: Option<String>,
        user_roles: Vec<String>,
    ) -> Value {
        let now = Utc::now();
        self.schedule_prune();
        let mut state = self.state.write().await;
        let execution_id = next_id("exec");
        let result = json!({
            "execution_id": execution_id,
            "todo_id": todo_id,
            "entity_id": entity_id,
            "status": "success",
            "completed_iterations": 1,
            "max_iterations": max_iterations,
            "system_prompt_override": system_prompt_override,
            "output": {
                "summary": format!("todo {todo_id} executed by Rust runtime"),
            }
        });
        let mut execution = ExecutionRecord::success(
            execution_id.clone(),
            "todo_agent".to_string(),
            result.clone(),
            user_id,
            user_roles,
            now,
        );
        execution.todo_id = Some(todo_id.to_string());
        execution.entity_id = entity_id;
        execution.input = json!({ "todo_id": todo_id, "max_iterations": max_iterations });
        state.execution_order.push(execution_id);
        state.executions.insert(execution.execution_id.clone(), execution);
        state.evict_old_executions();
        state.record_visibility_sample(100.0, 420.0);
        result
    }

    pub async fn execute_todo_tree(
        &self,
        root_todo_id: &str,
        max_iterations_per_todo: usize,
        fail_fast: bool,
        user_id: Option<String>,
        user_roles: Vec<String>,
    ) -> Value {
        let todo_ids = {
            self.schedule_prune();
            let state = self.state.read().await;
            state
                .chains
                .get(root_todo_id)
                .map(|item| item.todo_ids.clone())
                .unwrap_or_else(|| vec![root_todo_id.to_string()])
        };

        let mut results = serde_json::Map::new();
        for todo_id in &todo_ids {
            let result = self
                .execute_todo(
                    todo_id,
                    None,
                    max_iterations_per_todo,
                    None,
                    user_id.clone(),
                    user_roles.clone(),
                )
                .await;
            results.insert(todo_id.clone(), result);
        }
        json!({
            "total": todo_ids.len(),
            "executed": todo_ids.len(),
            "completed": todo_ids.len(),
            "failed": 0,
            "cancelled": 0,
            "skipped": 0,
            "fail_fast": fail_fast,
            "results": results,
        })
    }

    pub async fn create_chain_from_template(&self, template_id: &str, context: Value) -> Result<Value, AiRuntimeError> {
        let template = chain_templates()
            .into_iter()
            .find(|item| item.template_id == template_id)
            .ok_or_else(|| AiRuntimeError::validation("invalid todo chain template request"))?;
        let now = Utc::now();
        let mut todo_ids = Vec::new();
        for _title in &template.todos {
            todo_ids.push(next_id("todo"));
        }
        let root_todo_id = todo_ids.first().cloned().unwrap_or_else(|| next_id("todo"));
        self.schedule_prune();
        let mut state = self.state.write().await;
        state.chains.insert(
            root_todo_id.clone(),
            ChainRecord {
                root_todo_id: root_todo_id.clone(),
                template_id: template_id.to_string(),
                todo_ids: todo_ids.clone(),
                context,
                created_at: now,
            },
        );
        state.evict_old_chains();
        Ok(json!({
            "template_id": template_id,
            "todo_ids": todo_ids,
            "root_todo_id": root_todo_id,
            "total": template.todos.len(),
        }))
    }

    pub async fn get_chain_status(&self, root_todo_id: &str) -> Result<Value, AiRuntimeError> {
        let state = self.state.read().await;
        let chain = state
            .chains
            .get(root_todo_id)
            .ok_or_else(|| AiRuntimeError::not_found(root_todo_id))?;
        Ok(json!({
            "root_todo_id": chain.root_todo_id,
            "template_id": chain.template_id,
            "total": chain.todo_ids.len(),
            "completed": 0,
            "in_progress": 0,
            "pending": chain.todo_ids.len(),
            "created_at": chain.created_at.to_rfc3339(),
            "context": chain.context,
            "nodes": chain.todo_ids.iter().enumerate().map(|(idx, todo_id)| {
                json!({
                    "todo_id": todo_id,
                    "status": "pending",
                    "execution_order": idx,
                })
            }).collect::<Vec<_>>(),
        }))
    }

    pub async fn list_chain_templates(&self) -> Value {
        let items = chain_templates()
            .into_iter()
            .map(|item| {
                json!({
                    "template_id": item.template_id,
                    "name": item.name,
                    "description": item.description,
                    "todo_titles": item.todos,
                })
            })
            .collect::<Vec<_>>();
        json!({
            "items": items,
            "total": items.len(),
        })
    }

    pub async fn generate_plan(&self, prompt: &str, entity_id: Option<&str>) -> Value {
        let normalized = prompt.trim();
        let mut task_types = normalized
            .split(['\n', '。', '.', ';', '；'])
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .take(6)
            .enumerate()
            .map(|(idx, item)| {
                json!({
                    "id": format!("step_{}", idx + 1),
                    "title": item,
                    "status": "pending",
                })
            })
            .collect::<Vec<_>>();
        if task_types.is_empty() {
            task_types.push(json!({
                "id": "step_1",
                "title": normalized,
                "status": "pending",
            }));
        }
        json!({
            "title": normalized.lines().next().unwrap_or("任务计划"),
            "entity_id": entity_id.unwrap_or("default"),
            "task_types": task_types,
        })
    }

    pub async fn record_query_route(
        &self,
        intent: &str,
        dataset: &str,
        adapter: &str,
        status: &str,
        misroute: bool,
        reason: &str,
    ) {
        let labels = QueryRouteMetricLabels::new(intent, dataset, adapter, status, misroute, reason);
        {
            let mut state = self.state.write().await;
            state.record_query_route(labels.clone());
        }

        counter!(
            AI_QUERY_ROUTE_TOTAL_METRIC,
            "intent" => labels.intent.clone(),
            "dataset" => labels.dataset.clone(),
            "adapter" => labels.adapter.clone(),
            "status" => labels.status.clone(),
            "misroute" => labels.misroute.clone(),
            "reason" => labels.reason.clone()
        )
        .increment(1);

        if labels.misroute == "true" {
            counter!(
                AI_QUERY_MISROUTE_TOTAL_METRIC,
                "intent" => labels.intent,
                "dataset" => labels.dataset,
                "reason" => labels.reason
            )
            .increment(1);
        }
    }

    pub async fn record_query_tool_selection(&self, status: &str, mismatch: bool, tool_name: &str, reason: &str) {
        let labels = QuerySelectionMetricLabels::new(status, mismatch, tool_name, reason);
        {
            let mut state = self.state.write().await;
            state.record_query_tool_selection(labels.clone());
        }

        counter!(
            AI_QUERY_SELECTION_TOTAL_METRIC,
            "tool" => labels.tool.clone(),
            "status" => labels.status.clone(),
            "mismatch" => labels.mismatch.clone(),
            "reason" => labels.reason.clone()
        )
        .increment(1);

        if labels.mismatch == "true" {
            counter!(
                AI_QUERY_MISSELECTION_TOTAL_METRIC,
                "tool" => labels.tool,
                "status" => labels.status,
                "reason" => labels.reason
            )
            .increment(1);
        }
    }

    pub async fn record_report_schema_validation(
        &self,
        schema_valid: bool,
        mode: &str,
        report_type: &str,
        error_count: usize,
    ) {
        let labels = ReportSchemaValidationMetricLabels::new(schema_valid, mode, report_type);
        {
            let mut state = self.state.write().await;
            state.record_report_schema_validation(labels.clone());
        }

        counter!(
            AI_REPORT_SCHEMA_VALIDATION_TOTAL_METRIC,
            "schema_valid" => labels.schema_valid.clone(),
            "mode" => labels.mode.clone(),
            "report_type" => labels.report_type.clone()
        )
        .increment(1);
        histogram!(
            AI_REPORT_SCHEMA_VALIDATION_ERROR_COUNT_METRIC,
            "schema_valid" => labels.schema_valid,
            "mode" => labels.mode,
            "report_type" => labels.report_type
        )
        .record(error_count.max(0) as f64);
    }

    pub async fn query_routing_metrics(&self) -> Value {
        let state = self.state.read().await;
        let route_total: usize = state.query_route_totals.values().sum();
        let misroute_total: usize = state.query_misroute_totals.values().sum();
        let selection_total: usize = state.query_selection_totals.values().sum();
        let misselection_total: usize = state.query_misselection_totals.values().sum();
        let mut reason_buckets: HashMap<String, usize> = HashMap::new();

        for (labels, count) in &state.query_route_totals {
            *reason_buckets.entry(labels.reason.clone()).or_insert(0) += *count;
        }

        let mut top_reasons = reason_buckets
            .into_iter()
            .map(|(reason, count)| json!({ "reason": reason, "count": count }))
            .collect::<Vec<_>>();
        top_reasons.sort_by(|left, right| {
            let right_count = right.get("count").and_then(Value::as_u64).unwrap_or(0);
            let left_count = left.get("count").and_then(Value::as_u64).unwrap_or(0);
            right_count.cmp(&left_count).then_with(|| {
                left.get("reason")
                    .and_then(Value::as_str)
                    .cmp(&right.get("reason").and_then(Value::as_str))
            })
        });
        top_reasons.truncate(10);

        json!({
            "query_route_total": route_total,
            "query_misroute_total": misroute_total,
            "query_misroute_rate": rate(misroute_total, route_total),
            "query_selection_total": selection_total,
            "query_misselection_total": misselection_total,
            "query_misselection_rate": rate(misselection_total, selection_total),
            "top_reasons": top_reasons,
        })
    }

    pub async fn report_schema_metrics(&self) -> Value {
        let state = self.state.read().await;
        let total: usize = state.report_schema_validation_totals.values().sum();
        let invalid_total: usize = state
            .report_schema_validation_totals
            .iter()
            .filter(|(labels, _)| labels.schema_valid == "false")
            .map(|(_, count)| *count)
            .sum();
        let mut mode_buckets: HashMap<String, usize> = HashMap::new();
        for (labels, count) in &state.report_schema_validation_totals {
            *mode_buckets.entry(labels.mode.clone()).or_insert(0) += *count;
        }
        let mut mode_breakdown = mode_buckets
            .into_iter()
            .map(|(mode, count)| json!({ "mode": mode, "count": count }))
            .collect::<Vec<_>>();
        mode_breakdown.sort_by(|left, right| {
            let right_count = right.get("count").and_then(Value::as_u64).unwrap_or(0);
            let left_count = left.get("count").and_then(Value::as_u64).unwrap_or(0);
            right_count.cmp(&left_count).then_with(|| {
                left.get("mode")
                    .and_then(Value::as_str)
                    .cmp(&right.get("mode").and_then(Value::as_str))
            })
        });

        json!({
            "schema_validation_total": total,
            "schema_validation_invalid_total": invalid_total,
            "schema_validation_invalid_rate": rate(invalid_total, total),
            "mode_breakdown": mode_breakdown,
        })
    }

    pub async fn execution_visibility_metrics(&self) -> Value {
        let state = self.state.read().await;
        json!({
            "execution_event_total": state.execution_event_total,
            "first_progress_latency_ms": first_progress_metric_summary(&state.first_progress_latency_ms, 1500.0, state.first_progress_violation_total),
            "event_interval_ms": event_interval_metric_summary(&state.event_interval_ms, 3000.0, state.event_interval_violation_total),
            "coverage": {
                "first_progress_samples": state.first_progress_latency_ms.len(),
                "event_interval_samples": state.event_interval_ms.len(),
            },
        })
    }

    pub async fn todo_graph_pilot_metrics(
        &self,
        entity_id: Option<String>,
        window_hours: i32,
        sample_limit: i32,
        pending_stale_after_minutes: i32,
    ) -> Value {
        let normalized_entity_id = entity_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let bounded_window_hours = window_hours.max(1) as i64;
        let bounded_sample_limit = sample_limit.clamp(1, 1000) as usize;
        let bounded_pending_stale_after_minutes = pending_stale_after_minutes.max(1) as i64;
        let window_ended_at = Utc::now();
        let window_started_at = window_ended_at - chrono::Duration::hours(bounded_window_hours.max(1));
        let stale_cutoff = window_ended_at - chrono::Duration::minutes(bounded_pending_stale_after_minutes);

        let (execution_candidates, action_candidates) = {
            let state = self.state.read().await;

            let executions = state
                .execution_order
                .iter()
                .rev()
                .filter_map(|id| state.executions.get(id).cloned())
                .collect::<Vec<_>>();
            let actions = state
                .pending_order
                .iter()
                .rev()
                .filter_map(|id| state.pending_actions.get(id).cloned())
                .collect::<Vec<_>>();
            (executions, actions)
        };

        let scoped_todo_ids = self
            .collect_recent_scoped_todo_ids(normalized_entity_id.as_deref(), window_started_at, bounded_sample_limit)
            .await;

        let execution_candidates = execution_candidates
            .into_iter()
            .filter(|execution| execution.started_at >= window_started_at)
            .filter(|execution| execution_matches_scope(execution, normalized_entity_id.as_deref(), &scoped_todo_ids))
            .take(bounded_sample_limit)
            .collect::<Vec<_>>();

        let execution_index = execution_candidates
            .iter()
            .map(|execution| (execution.execution_id.clone(), execution.clone()))
            .collect::<HashMap<_, _>>();

        let recent_actions = action_candidates
            .iter()
            .filter(|action| action.created_at >= window_started_at)
            .filter(|action| {
                pending_action_matches_scope(
                    action,
                    normalized_entity_id.as_deref(),
                    &scoped_todo_ids,
                    &execution_index,
                )
            })
            .take(bounded_sample_limit)
            .cloned()
            .collect::<Vec<_>>();

        let pending_actions = action_candidates
            .iter()
            .filter(|action| action.status == "pending")
            .filter(|action| {
                pending_action_matches_scope(
                    action,
                    normalized_entity_id.as_deref(),
                    &scoped_todo_ids,
                    &execution_index,
                )
            })
            .take(bounded_sample_limit)
            .cloned()
            .collect::<Vec<_>>();

        let mut action_by_id = HashMap::new();
        for action in recent_actions.into_iter().chain(pending_actions.iter().cloned()) {
            action_by_id.insert(action.action_id.clone(), action);
        }
        let normalized_actions = action_by_id.into_values().collect::<Vec<_>>();

        let execution_total = execution_candidates.len() as i64;
        let completed_total = execution_candidates
            .iter()
            .filter(|execution| is_completed_status(&execution.status))
            .count() as i64;
        let failed_total = execution_candidates
            .iter()
            .filter(|execution| is_failed_status(&execution.status))
            .count() as i64;
        let cancelled_total = execution_candidates
            .iter()
            .filter(|execution| execution.status == "cancelled")
            .count() as i64;
        let pending_execution_total = execution_candidates
            .iter()
            .filter(|execution| is_pending_status(&execution.status))
            .count() as i64;

        let mut graph_requested_total = 0_i64;
        let mut graph_actual_total = 0_i64;
        let mut graph_fallback_total = 0_i64;
        let mut fallback_reasons = HashMap::<String, i64>::new();
        let mut execution_duration_samples = Vec::<f64>::new();
        let mut guardrail_metrics = GuardrailMetrics::default();
        let mut graph_requested_run_ids = HashSet::new();

        for execution in &execution_candidates {
            let requested_path = normalized_runtime_text(Some(&execution.runtime_path_requested));
            let runtime_path = normalized_runtime_text(Some(&execution.runtime_path));

            if requested_path == "graph" {
                graph_requested_total += 1;
                graph_requested_run_ids.insert(execution.execution_id.clone());
            }
            if runtime_path == "graph" {
                graph_actual_total += 1;
            }
            if requested_path == "graph" && runtime_path == "legacy" {
                graph_fallback_total += 1;
                let reason = execution
                    .runtime_fallback_reason
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("unknown")
                    .to_string();
                *fallback_reasons.entry(reason).or_insert(0) += 1;
            }

            if let Some(duration_ms) = execution_duration_ms(execution) {
                execution_duration_samples.push(duration_ms);
            }

            guardrail_metrics.merge(extract_guardrail_metrics(execution));
            let backstop = scan_tool_call_duplicates(&execution.output);
            guardrail_metrics.duplicate_tool_execution_backstop_total += backstop.total;
            guardrail_metrics.duplicate_tool_execution_backstop_runs += backstop.runs;
        }

        guardrail_metrics.duplicate_tool_execution_total = guardrail_metrics
            .duplicate_tool_execution_total
            .max(guardrail_metrics.duplicate_tool_execution_backstop_total);
        guardrail_metrics.duplicate_tool_execution_runs = guardrail_metrics
            .duplicate_tool_execution_runs
            .max(guardrail_metrics.duplicate_tool_execution_backstop_runs);

        let pending_total = pending_actions.len() as i64;
        let stale_pending_total = pending_actions
            .iter()
            .filter(|action| action.created_at <= stale_cutoff)
            .count() as i64;
        let graph_resume_actions = normalized_actions
            .iter()
            .filter(|action| execution_receipt_resume_mode(action) == Some("graph"))
            .collect::<Vec<_>>();
        let graph_resume_total = graph_resume_actions.len() as i64;
        let graph_resume_success_total = graph_resume_actions
            .iter()
            .filter(|action| execution_receipt_status(action) == Some("applied"))
            .count() as i64;

        let mut approval_response_time_samples = Vec::<f64>::new();
        let mut graph_approval_run_ids = HashSet::new();
        for action in &normalized_actions {
            if let Some(duration_ms) = approval_response_time_ms(action) {
                approval_response_time_samples.push(duration_ms);
            }
            if execution_receipt_resume_mode(action) == Some("graph") {
                if let Some(run_id) = action
                    .correlation_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    graph_approval_run_ids.insert(run_id.to_string());
                }
            }
        }

        let completion_rate = ratio(completed_total, execution_total);
        let graph_fallback_rate = ratio(graph_fallback_total, graph_requested_total);
        let graph_resume_success_rate = ratio(graph_resume_success_total, graph_resume_total);

        let mut top_fallback_reasons = fallback_reasons
            .into_iter()
            .map(|(reason, count)| json!({ "reason": reason, "count": count }))
            .collect::<Vec<_>>();
        top_fallback_reasons.sort_by(|left, right| {
            let right_count = right.get("count").and_then(Value::as_i64).unwrap_or(0);
            let left_count = left.get("count").and_then(Value::as_i64).unwrap_or(0);
            right_count.cmp(&left_count).then_with(|| {
                left.get("reason")
                    .and_then(Value::as_str)
                    .cmp(&right.get("reason").and_then(Value::as_str))
            })
        });
        top_fallback_reasons.truncate(10);

        let verdict = build_todo_graph_pilot_verdict(
            normalized_entity_id.as_deref(),
            graph_requested_total,
            graph_fallback_rate,
            graph_resume_total,
            graph_resume_success_rate,
            completion_rate,
            stale_pending_total,
            guardrail_metrics.duplicate_tool_execution_total,
            guardrail_metrics.duplicate_tool_execution_blocked_total,
        );

        json!({
            "scope": {
                "entity_id": normalized_entity_id,
                "cohort_mode": if entity_id.as_deref().map(str::trim).filter(|value| !value.is_empty()).is_some() {
                    "entity"
                } else {
                    "global_snapshot"
                },
                "window_hours": bounded_window_hours,
                "window_started_at": window_started_at.to_rfc3339(),
                "window_ended_at": window_ended_at.to_rfc3339(),
                "default_pilot_entity_id": DEFAULT_TODO_GRAPH_PILOT_ENTITY_ID,
            },
            "thresholds": todo_graph_pilot_thresholds(),
            "verdict": verdict,
            "window": {
                "execution_sample_size": execution_total,
                "approval_sample_size": normalized_actions.len(),
                "pending_stale_after_minutes": bounded_pending_stale_after_minutes,
            },
            "executions": {
                "total": execution_total,
                "completed_total": completed_total,
                "failed_total": failed_total,
                "cancelled_total": cancelled_total,
                "pending_total": pending_execution_total,
                "completion_rate": completion_rate,
                "graph_requested_total": graph_requested_total,
                "graph_actual_total": graph_actual_total,
                "graph_fallback_total": graph_fallback_total,
                "graph_fallback_rate": graph_fallback_rate,
                "top_fallback_reasons": top_fallback_reasons,
            },
            "approvals": {
                "pending_total": pending_total,
                "stale_pending_total": stale_pending_total,
                "graph_resume_total": graph_resume_total,
                "graph_resume_success_total": graph_resume_success_total,
                "graph_resume_success_rate": graph_resume_success_rate,
            },
            "guardrails": {
                "duplicate_tool_execution_total": guardrail_metrics.duplicate_tool_execution_total,
                "duplicate_tool_execution_runs": guardrail_metrics.duplicate_tool_execution_runs,
                "duplicate_tool_execution_blocked_total": guardrail_metrics.duplicate_tool_execution_blocked_total,
                "duplicate_tool_execution_blocked_runs": guardrail_metrics.duplicate_tool_execution_blocked_runs,
                "duplicate_tool_execution_backstop_total": guardrail_metrics.duplicate_tool_execution_backstop_total,
                "duplicate_tool_execution_backstop_runs": guardrail_metrics.duplicate_tool_execution_backstop_runs,
                "duplicate_tool_execution_instrumented": true,
            },
            "value_metrics": {
                "execution_duration_ms": percentile_summary(&execution_duration_samples),
                "approval_response_time_ms": percentile_summary(&approval_response_time_samples),
                "human_approval_rate": ratio(graph_approval_run_ids.len() as i64, execution_total),
                "graph_approval_run_total": graph_approval_run_ids.len() as i64,
            },
        })
    }

    async fn collect_recent_scoped_todo_ids(
        &self,
        entity_id: Option<&str>,
        window_started_at: DateTime<Utc>,
        sample_limit: usize,
    ) -> HashSet<String> {
        let fetch_limit = (sample_limit.max(200) as i64).min(5_000);
        let Some(todo_repository) = self.todo_repository.as_ref() else {
            return HashSet::new();
        };

        if let Some(entity_id) = entity_id {
            if let Some(context_repository) = self.todo_agent_context_repository.as_ref() {
                if let Ok(todo_ids) = context_repository
                    .find_todo_ids(None, Some(entity_id), None, fetch_limit, 0)
                    .await
                {
                    if let Ok(todos) = todo_repository.find_by_ids(&todo_ids).await {
                        return todos
                            .into_iter()
                            .filter(|todo| todo.created_at >= window_started_at || todo.updated_at >= window_started_at)
                            .map(|todo| todo.todo_id)
                            .collect();
                    }
                }
            }
            return HashSet::new();
        }

        todo_repository
            .find_all(None, None, None, None, None, None, fetch_limit, 0)
            .await
            .map(|todos| {
                todos
                    .into_iter()
                    .filter(|todo| todo.created_at >= window_started_at || todo.updated_at >= window_started_at)
                    .map(|todo| todo.todo_id)
                    .collect()
            })
            .unwrap_or_default()
    }
}
