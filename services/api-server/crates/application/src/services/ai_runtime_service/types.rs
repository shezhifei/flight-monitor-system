use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::collections::HashMap;

use super::helpers::{bool_metric_label, normalize_metric_label, trim_sample_window};
use super::service::{
    AiToolExecutionSpec, CHAIN_RETENTION_HOURS, EXECUTION_RETENTION_HOURS, MAX_CHAINS, MAX_EXECUTIONS,
    MAX_PENDING_ACTIONS, METRIC_SAMPLE_WINDOW, PENDING_ACTION_RETENTION_MINUTES,
};

#[derive(Default)]
pub(super) struct AiRuntimeState {
    pub(super) pending_actions: HashMap<String, PendingActionRecord>,
    pub(super) pending_order: Vec<String>,
    pub(super) executions: HashMap<String, ExecutionRecord>,
    pub(super) execution_order: Vec<String>,
    pub(super) query_route_totals: HashMap<QueryRouteMetricLabels, usize>,
    pub(super) query_misroute_totals: HashMap<QueryMisrouteMetricLabels, usize>,
    pub(super) query_selection_totals: HashMap<QuerySelectionMetricLabels, usize>,
    pub(super) query_misselection_totals: HashMap<QueryMisselectionMetricLabels, usize>,
    pub(super) report_schema_validation_totals: HashMap<ReportSchemaValidationMetricLabels, usize>,
    pub(super) execution_event_total: usize,
    pub(super) first_progress_latency_ms: Vec<f64>,
    pub(super) event_interval_ms: Vec<f64>,
    pub(super) first_progress_violation_total: usize,
    pub(super) event_interval_violation_total: usize,
    pub(super) chains: HashMap<String, ChainRecord>,
}

impl AiRuntimeState {
    pub(super) fn prune(&mut self, now: DateTime<Utc>) {
        self.expire_pending_actions(now);
        self.evict_old_pending_actions();
        self.expire_executions(now);
        self.evict_old_executions();
        self.expire_chains(now);
        self.evict_old_chains();
    }

    pub(super) fn filter_pending_actions(
        &self,
        status: Option<&str>,
        tool_name: Option<&str>,
    ) -> Vec<PendingActionRecord> {
        self.pending_order
            .iter()
            .rev()
            .filter_map(|id| self.pending_actions.get(id))
            .filter(|item| status.map(|value| item.status == value).unwrap_or(true))
            .filter(|item| tool_name.map(|value| item.tool_name == value).unwrap_or(true))
            .cloned()
            .collect()
    }

    pub(super) fn record_visibility_sample(&mut self, first_progress_ms: f64, interval_ms: f64) {
        self.execution_event_total += 1;
        self.first_progress_latency_ms.push(first_progress_ms);
        self.event_interval_ms.push(interval_ms);
        trim_sample_window(&mut self.first_progress_latency_ms, METRIC_SAMPLE_WINDOW);
        trim_sample_window(&mut self.event_interval_ms, METRIC_SAMPLE_WINDOW);
        if first_progress_ms > 1500.0 {
            self.first_progress_violation_total += 1;
        }
        if interval_ms > 3000.0 {
            self.event_interval_violation_total += 1;
        }
    }

    pub(super) fn record_query_route(&mut self, labels: QueryRouteMetricLabels) {
        *self.query_route_totals.entry(labels.clone()).or_insert(0) += 1;
        if labels.misroute == "true" {
            *self
                .query_misroute_totals
                .entry(QueryMisrouteMetricLabels {
                    intent: labels.intent,
                    dataset: labels.dataset,
                    reason: labels.reason,
                })
                .or_insert(0) += 1;
        }
    }

    pub(super) fn record_query_tool_selection(&mut self, labels: QuerySelectionMetricLabels) {
        *self.query_selection_totals.entry(labels.clone()).or_insert(0) += 1;
        if labels.mismatch == "true" {
            *self
                .query_misselection_totals
                .entry(QueryMisselectionMetricLabels {
                    tool: labels.tool,
                    status: labels.status,
                    reason: labels.reason,
                })
                .or_insert(0) += 1;
        }
    }

    pub(super) fn record_report_schema_validation(&mut self, labels: ReportSchemaValidationMetricLabels) {
        *self.report_schema_validation_totals.entry(labels).or_insert(0) += 1;
    }

    fn expire_pending_actions(&mut self, now: DateTime<Utc>) {
        for action in self.pending_actions.values_mut() {
            if action.status == "pending" && action.expires_at.map(|value| now >= value).unwrap_or(false) {
                action.mark_expired(now);
            }
        }

        let stale_ids = self
            .pending_actions
            .iter()
            .filter_map(|(id, action)| action.should_remove(now).then_some(id.clone()))
            .collect::<Vec<_>>();
        for id in stale_ids {
            self.pending_actions.remove(&id);
        }
        self.pending_order.retain(|id| self.pending_actions.contains_key(id));
    }

    pub(super) fn evict_old_pending_actions(&mut self) {
        while self.pending_order.len() > MAX_PENDING_ACTIONS {
            let removed_id = self.pending_order.remove(0);
            self.pending_actions.remove(&removed_id);
        }
    }

    fn expire_executions(&mut self, now: DateTime<Utc>) {
        let stale_ids = self
            .executions
            .iter()
            .filter_map(|(id, execution)| execution.should_remove(now).then_some(id.clone()))
            .collect::<Vec<_>>();
        for id in stale_ids {
            self.executions.remove(&id);
        }
        self.execution_order.retain(|id| self.executions.contains_key(id));
    }

    pub(super) fn evict_old_executions(&mut self) {
        while self.execution_order.len() > MAX_EXECUTIONS {
            let removed_id = self.execution_order.remove(0);
            self.executions.remove(&removed_id);
        }
    }

    fn expire_chains(&mut self, now: DateTime<Utc>) {
        let stale_ids = self
            .chains
            .iter()
            .filter_map(|(id, chain)| chain.should_remove(now).then_some(id.clone()))
            .collect::<Vec<_>>();
        for id in stale_ids {
            self.chains.remove(&id);
        }
    }

    pub(super) fn evict_old_chains(&mut self) {
        while self.chains.len() > MAX_CHAINS {
            let Some(oldest_id) = self
                .chains
                .iter()
                .min_by_key(|(_, chain)| chain.created_at)
                .map(|(id, _)| id.clone())
            else {
                break;
            };
            self.chains.remove(&oldest_id);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct QueryRouteMetricLabels {
    pub(super) intent: String,
    pub(super) dataset: String,
    pub(super) adapter: String,
    pub(super) status: String,
    pub(super) misroute: String,
    pub(super) reason: String,
}

impl QueryRouteMetricLabels {
    pub(super) fn new(intent: &str, dataset: &str, adapter: &str, status: &str, misroute: bool, reason: &str) -> Self {
        Self {
            intent: normalize_metric_label(intent, "unknown"),
            dataset: normalize_metric_label(dataset, "unknown"),
            adapter: normalize_metric_label(adapter, "unknown"),
            status: normalize_metric_label(status, "unknown"),
            misroute: bool_metric_label(misroute),
            reason: normalize_metric_label(reason, "none"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct QueryMisrouteMetricLabels {
    pub(super) intent: String,
    pub(super) dataset: String,
    pub(super) reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct QuerySelectionMetricLabels {
    pub(super) tool: String,
    pub(super) status: String,
    pub(super) mismatch: String,
    pub(super) reason: String,
}

impl QuerySelectionMetricLabels {
    pub(super) fn new(status: &str, mismatch: bool, tool_name: &str, reason: &str) -> Self {
        Self {
            tool: normalize_metric_label(tool_name, "QUERY"),
            status: normalize_metric_label(status, "unknown"),
            mismatch: bool_metric_label(mismatch),
            reason: normalize_metric_label(reason, "none"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct QueryMisselectionMetricLabels {
    pub(super) tool: String,
    pub(super) status: String,
    pub(super) reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct ReportSchemaValidationMetricLabels {
    pub(super) schema_valid: String,
    pub(super) mode: String,
    pub(super) report_type: String,
}

impl ReportSchemaValidationMetricLabels {
    pub(super) fn new(schema_valid: bool, mode: &str, report_type: &str) -> Self {
        Self {
            schema_valid: bool_metric_label(schema_valid),
            mode: normalize_metric_label(mode, "unknown"),
            report_type: normalize_metric_label(report_type, "unknown"),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct PendingActionRecord {
    pub(super) action_id: String,
    pub(super) tool_call_id: String,
    pub(super) tool_name: String,
    pub(super) arguments: Value,
    pub(super) operation_level: String,
    pub(super) invocation_mode: String,
    pub(super) requester_user_id: Option<String>,
    pub(super) requester_user_roles: Vec<String>,
    pub(super) reason: String,
    pub(super) status: String,
    pub(super) created_at: DateTime<Utc>,
    pub(super) updated_at: DateTime<Utc>,
    pub(super) approved_by: Option<String>,
    pub(super) approved_at: Option<DateTime<Utc>>,
    pub(super) rejected_by: Option<String>,
    pub(super) rejected_reason: Option<String>,
    pub(super) rejected_at: Option<DateTime<Utc>>,
    pub(super) execution_result: Option<Value>,
    pub(super) execution_error: Option<String>,
    pub(super) risk_level: String,
    pub(super) before_snapshot: Option<Value>,
    pub(super) after_snapshot: Option<Value>,
    pub(super) json_patch: Option<Value>,
    pub(super) diff_summary: Option<Value>,
    pub(super) execution_receipt: Option<Value>,
    pub(super) status_code: Option<String>,
    pub(super) error_payload: Option<Value>,
    pub(super) correlation_id: Option<String>,
    pub(super) ui_hints: Value,
    pub(super) expires_at: Option<DateTime<Utc>>,
    pub(super) diff_source: Option<String>,
    pub(super) decision_blocked_reason: Option<String>,
}

impl PendingActionRecord {
    pub(super) fn new(
        action_id: String,
        spec: AiToolExecutionSpec,
        arguments: Value,
        tool_call_id: String,
        requester_user_id: Option<String>,
        requester_user_roles: Vec<String>,
        correlation_id: String,
        now: DateTime<Utc>,
    ) -> Self {
        let tool_name = spec.tool_name.clone();
        let operation_level = spec.operation_level.clone();
        let category = spec.category.clone();
        let expires_at = now + chrono::Duration::minutes(30);
        Self {
            action_id,
            tool_call_id: tool_call_id.clone(),
            tool_name: tool_name.clone(),
            arguments: arguments.clone(),
            operation_level,
            invocation_mode: "user_requested".to_string(),
            requester_user_id,
            requester_user_roles,
            reason: format!("{category} requires human approval"),
            status: "pending".to_string(),
            created_at: now,
            updated_at: now,
            approved_by: None,
            approved_at: None,
            rejected_by: None,
            rejected_reason: None,
            rejected_at: None,
            execution_result: None,
            execution_error: None,
            risk_level: "NORMAL".to_string(),
            before_snapshot: Some(json!({
                "tool_name": tool_name,
                "arguments": arguments,
                "state": "pending_approval",
            })),
            after_snapshot: Some(json!({
                "tool_name": tool_name,
                "proposed_arguments": arguments,
                "state": "approved_execution",
            })),
            json_patch: Some(json!([
                {"op": "add", "path": "/approval_required", "value": true},
                {"op": "add", "path": "/tool_call_id", "value": tool_call_id},
            ])),
            diff_summary: Some(json!([
                format!("tool={tool_name} queued for approval"),
                format!(
                    "operation_level={} requires reviewer confirmation",
                    spec.operation_level
                ),
            ])),
            execution_receipt: None,
            status_code: None,
            error_payload: None,
            correlation_id: Some(correlation_id),
            ui_hints: json!({
                "diff_source": "runtime_preview",
                "highlight": ["arguments", "approval_required"],
            }),
            expires_at: Some(expires_at),
            diff_source: Some("runtime_preview".to_string()),
            decision_blocked_reason: None,
        }
    }

    pub(super) fn to_value(&self) -> Value {
        json!({
            "action_id": self.action_id,
            "tool_call_id": self.tool_call_id,
            "tool_name": self.tool_name,
            "arguments": self.arguments,
            "operation_level": self.operation_level,
            "invocation_mode": self.invocation_mode,
            "requester_user_id": self.requester_user_id,
            "requester_user_roles": self.requester_user_roles,
            "reason": self.reason,
            "status": self.status,
            "created_at": self.created_at.to_rfc3339(),
            "updated_at": self.updated_at.to_rfc3339(),
            "approved_by": self.approved_by,
            "approved_at": self.approved_at.map(|value| value.to_rfc3339()),
            "rejected_by": self.rejected_by,
            "rejected_reason": self.rejected_reason,
            "rejected_at": self.rejected_at.map(|value| value.to_rfc3339()),
            "execution_result": self.execution_result,
            "execution_error": self.execution_error,
            "risk_level": self.risk_level,
            "before_snapshot": self.before_snapshot,
            "after_snapshot": self.after_snapshot,
            "json_patch": self.json_patch,
            "diff_summary": self.diff_summary,
            "execution_receipt": self.execution_receipt,
            "status_code": self.status_code,
            "error_payload": self.error_payload,
            "correlation_id": self.correlation_id,
            "ui_hints": self.ui_hints,
            "expires_at": self.expires_at.map(|value| value.to_rfc3339()),
            "diff_source": self.diff_source,
            "decision_blocked_reason": self.decision_blocked_reason,
        })
    }

    pub(super) fn to_execution_value(&self) -> Value {
        json!({
            "execution_id": self.correlation_id,
            "run_id": self.correlation_id,
            "status": self.status,
            "tool_name": self.tool_name,
            "approval_id": self.action_id,
            "started_at": self.created_at.to_rfc3339(),
            "updated_at": self.updated_at.to_rfc3339(),
            "finished_at": self.approved_at.or(self.rejected_at).map(|value| value.to_rfc3339()),
            "runtime_path": "legacy",
            "runtime_status": self.status,
            "runtime": {
                "path": "legacy",
                "status": self.status,
                "invocation_mode": self.invocation_mode,
                "resumable": self.status == "pending",
                "requested_path": "legacy",
                "fallback_reason": Value::Null,
                "tool_names": [self.tool_name.clone()],
            },
            "pending_action": self.to_value(),
            "execution_receipt": self.execution_receipt,
            "error_message": self.execution_error,
        })
    }

    pub(super) fn mark_expired(&mut self, now: DateTime<Utc>) {
        self.status = "expired".to_string();
        self.updated_at = now;
        self.status_code = Some("PENDING_ACTION_EXPIRED".to_string());
        self.error_payload = Some(json!({ "decision_blocked_reason": "expired" }));
        self.decision_blocked_reason = Some("expired".to_string());
    }

    fn should_remove(&self, now: DateTime<Utc>) -> bool {
        let retention_deadline = self.updated_at + chrono::Duration::minutes(PENDING_ACTION_RETENTION_MINUTES);
        now >= retention_deadline
    }
}

#[derive(Debug, Clone)]
pub(super) struct ExecutionRecord {
    pub(super) execution_id: String,
    pub(super) todo_id: Option<String>,
    pub(super) entity_id: Option<String>,
    pub(super) status: String,
    pub(super) tool_name: String,
    pub(super) input: Value,
    pub(super) output: Value,
    pub(super) error_message: Option<String>,
    pub(super) approval_id: Option<String>,
    pub(super) user_id: Option<String>,
    pub(super) user_roles: Vec<String>,
    pub(super) started_at: DateTime<Utc>,
    pub(super) updated_at: DateTime<Utc>,
    pub(super) finished_at: Option<DateTime<Utc>>,
    pub(super) runtime_path: String,
    pub(super) runtime_path_requested: String,
    pub(super) runtime_status: String,
    pub(super) runtime_invocation_mode: String,
    pub(super) runtime_resumable: bool,
    pub(super) runtime_fallback_reason: Option<String>,
    pub(super) runtime_tool_names: Vec<String>,
}

#[derive(Debug, Clone)]
pub(super) struct ChainRecord {
    pub(super) root_todo_id: String,
    pub(super) template_id: String,
    pub(super) todo_ids: Vec<String>,
    pub(super) context: Value,
    pub(super) created_at: DateTime<Utc>,
}

pub(super) struct ChainTemplate {
    pub(super) template_id: &'static str,
    pub(super) name: &'static str,
    pub(super) description: &'static str,
    pub(super) todos: Vec<&'static str>,
}

impl ExecutionRecord {
    pub(super) fn pending_approval(
        execution_id: String,
        tool_name: String,
        input: Value,
        user_id: Option<String>,
        user_roles: Vec<String>,
        approval_id: Option<String>,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            execution_id,
            todo_id: None,
            entity_id: None,
            status: "pending_approval".to_string(),
            tool_name: tool_name.clone(),
            input,
            output: Value::Null,
            error_message: None,
            approval_id,
            user_id,
            user_roles,
            started_at: now,
            updated_at: now,
            finished_at: None,
            runtime_path: "legacy".to_string(),
            runtime_path_requested: "legacy".to_string(),
            runtime_status: "pending_approval".to_string(),
            runtime_invocation_mode: "user_requested".to_string(),
            runtime_resumable: true,
            runtime_fallback_reason: None,
            runtime_tool_names: vec![tool_name.clone()],
        }
    }

    pub(super) fn success(
        execution_id: String,
        tool_name: String,
        output: Value,
        user_id: Option<String>,
        user_roles: Vec<String>,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            execution_id,
            todo_id: None,
            entity_id: None,
            status: "success".to_string(),
            tool_name: tool_name.clone(),
            input: Value::Null,
            output,
            error_message: None,
            approval_id: None,
            user_id,
            user_roles,
            started_at: now,
            updated_at: now,
            finished_at: Some(now),
            runtime_path: "legacy".to_string(),
            runtime_path_requested: "legacy".to_string(),
            runtime_status: "success".to_string(),
            runtime_invocation_mode: "user_requested".to_string(),
            runtime_resumable: false,
            runtime_fallback_reason: None,
            runtime_tool_names: vec![tool_name.clone()],
        }
    }

    pub(super) fn rejected(
        execution_id: String,
        tool_name: String,
        reason: Option<String>,
        user_id: Option<String>,
        user_roles: Vec<String>,
        approval_id: Option<String>,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            execution_id,
            todo_id: None,
            entity_id: None,
            status: "rejected".to_string(),
            tool_name: tool_name.clone(),
            input: Value::Null,
            output: Value::Null,
            error_message: reason,
            approval_id,
            user_id,
            user_roles,
            started_at: now,
            updated_at: now,
            finished_at: Some(now),
            runtime_path: "legacy".to_string(),
            runtime_path_requested: "legacy".to_string(),
            runtime_status: "rejected".to_string(),
            runtime_invocation_mode: "user_requested".to_string(),
            runtime_resumable: false,
            runtime_fallback_reason: None,
            runtime_tool_names: vec![tool_name.clone()],
        }
    }

    pub(super) fn with_approval(mut self, approval_id: Option<String>) -> Self {
        self.approval_id = approval_id;
        self
    }

    pub(super) fn with_input(mut self, input: Value) -> Self {
        self.input = input;
        self
    }

    pub(super) fn to_value(&self) -> Value {
        json!({
            "execution_id": self.execution_id,
            "run_id": self.execution_id,
            "todo_id": self.todo_id,
            "entity_id": self.entity_id,
            "status": self.status,
            "tool_name": self.tool_name,
            "input": self.input,
            "result": self.output,
            "error_message": self.error_message,
            "approval_id": self.approval_id,
            "user_id": self.user_id,
            "user_roles": self.user_roles,
            "started_at": self.started_at.to_rfc3339(),
            "updated_at": self.updated_at.to_rfc3339(),
            "finished_at": self.finished_at.map(|value| value.to_rfc3339()),
            "runtime_path": self.runtime_path,
            "runtime_status": self.runtime_status,
            "runtime": {
                "path": self.runtime_path,
                "status": self.runtime_status,
                "invocation_mode": self.runtime_invocation_mode,
                "resumable": self.runtime_resumable,
                "requested_path": self.runtime_path_requested,
                "fallback_reason": self.runtime_fallback_reason,
                "tool_names": self.runtime_tool_names,
            },
        })
    }

    fn should_remove(&self, now: DateTime<Utc>) -> bool {
        let ttl = chrono::Duration::hours(EXECUTION_RETENTION_HOURS);
        if let Some(finished_at) = self.finished_at {
            return now >= finished_at + ttl;
        }
        now >= self.started_at + ttl
    }
}

impl ChainRecord {
    fn should_remove(&self, now: DateTime<Utc>) -> bool {
        now >= self.created_at + chrono::Duration::hours(CHAIN_RETENTION_HOURS)
    }
}

pub(super) fn chain_templates() -> Vec<ChainTemplate> {
    vec![
        ChainTemplate {
            template_id: "flight-event-investigation",
            name: "航班事件排查",
            description: "收集上下文、评估影响并生成处置建议",
            todos: vec!["收集上下文", "评估影响", "生成处置建议"],
        },
        ChainTemplate {
            template_id: "dispatch-recovery",
            name: "派工恢复",
            description: "检查资源、确认阻塞点并形成恢复动作",
            todos: vec!["检查资源", "确认阻塞点", "形成恢复动作"],
        },
    ]
}
