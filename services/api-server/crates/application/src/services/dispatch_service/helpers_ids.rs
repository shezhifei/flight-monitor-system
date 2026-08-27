use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use tracing::warn;

use crate::schemas::dispatch_schemas::*;
use crate::services::notification_service::DispatchBatchNotificationCreate;
use fms_domain::error::DomainError;
use fms_domain::models::anomaly::{AnomalySeverity, AnomalyType};
use fms_domain::models::dispatch::*;
use fms_domain::models::dispatch_collaboration::DispatchCollaborationEvent;

use super::{DispatchService, NULL_VALUE};

impl DispatchService {
    pub(super) const ACTIVE_CONFLICT_STATUSES: [&'static str; 3] = ["pending", "assigned", "in_progress"];

    pub(super) fn new_dispatch_id() -> String {
        ulid::Ulid::new().to_string()
    }

    pub(super) fn deterministic_issue_anomaly_id(order_id: &str, client_action_id: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"dispatch_issue_anomaly:v1");
        hasher.update(order_id.as_bytes());
        hasher.update(b":");
        hasher.update(client_action_id.as_bytes());
        let digest = hex::encode(hasher.finalize());
        format!("AI{}", &digest[..24])
    }

    pub(super) fn ensure_actor(actor_id: &str) -> Result<(), DomainError> {
        if actor_id.trim().is_empty() {
            return Err(DomainError::Unauthorized("未登录".to_string()));
        }
        Ok(())
    }

    pub(super) fn ensure_order_execution_published(
        order: &DispatchOrder,
        action_label: &str,
    ) -> Result<(), DomainError> {
        if order.publication_state.trim() == "prepublished" {
            return Err(DomainError::BusinessRuleViolation(format!(
                "预发布工单尚未正式发布，不能{action_label}"
            )));
        }
        Ok(())
    }

    pub(super) fn normalize_checklist_result(result: Option<&str>) -> Result<&str, DomainError> {
        match result.map(str::trim).filter(|value| !value.is_empty()) {
            Some(normalized @ ("pass" | "fail" | "na")) => Ok(normalized),
            _ => Err(DomainError::BusinessRuleViolation(
                "invalid safety checklist item result".to_string(),
            )),
        }
    }

    pub(super) fn increment_metric(&self, key: &str) {
        *self.analytics.metrics_counters.entry(key.to_string()).or_insert(0) += 1;
    }

    pub(super) fn snapshot_metrics(&self) -> std::collections::BTreeMap<String, i64> {
        let mut map: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
        for entry in self.analytics.metrics_counters.iter() {
            map.insert(entry.key().clone(), *entry.value());
        }
        map
    }

    pub(super) fn normalize_checklist_level(level: Option<&str>, required: bool) -> Result<String, DomainError> {
        match level.map(str::trim).filter(|value| !value.is_empty()) {
            Some("critical") | None if required => Ok("critical".to_string()),
            Some("routine") => Ok("routine".to_string()),
            None => Ok("routine".to_string()),
            _ => Err(Self::invalid_safety_template_request()),
        }
    }

    pub(super) fn invalid_safety_template_request() -> DomainError {
        DomainError::BusinessRuleViolation("invalid safety checklist template request".to_string())
    }

    pub(super) fn normalize_safety_template_version(value: &str) -> Result<String, DomainError> {
        let normalized = value.trim();
        if normalized.is_empty() {
            return Err(Self::invalid_safety_template_request());
        }
        Ok(normalized.to_string())
    }

    pub(super) fn already_started_response(
        actual_start_time: Option<DateTime<Utc>>,
        fallback: DateTime<Utc>,
    ) -> serde_json::Value {
        serde_json::json!({
            "success": true,
            "message": "派工单已在执行中",
            "actual_start_time": actual_start_time.map(|t| t.to_rfc3339()).unwrap_or_else(|| fallback.to_rfc3339()),
            "compat_alias": true,
        })
    }

    pub(super) fn already_completed_response(
        actual_end_time: Option<DateTime<Utc>>,
        fallback: DateTime<Utc>,
    ) -> serde_json::Value {
        serde_json::json!({
            "message": "派工单已完成",
            "actual_end_time": actual_end_time.map(|t| t.to_rfc3339()).unwrap_or_else(|| fallback.to_rfc3339()),
            "completion_mode": "already_completed",
            "followup_required": false,
            "followup_owner_role": Value::Null,
            "followup_todo_id": Value::Null,
            "compat_alias": true,
        })
    }

    pub(super) fn normalize_safety_template_items(
        items: &[serde_json::Value],
    ) -> Result<Vec<serde_json::Value>, DomainError> {
        if items.is_empty() {
            return Err(Self::invalid_safety_template_request());
        }

        let mut normalized = Vec::with_capacity(items.len());
        let mut seen_codes = HashSet::new();

        for (index, raw_item) in items.iter().enumerate() {
            let Some(item) = raw_item.as_object() else {
                return Err(Self::invalid_safety_template_request());
            };

            let item_code = item
                .get("item_code")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(Self::invalid_safety_template_request)?;
            if !seen_codes.insert(item_code.to_string()) {
                return Err(Self::invalid_safety_template_request());
            }

            let title = item
                .get("title")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(item_code)
                .to_string();
            let required = item.get("required").and_then(Value::as_bool).unwrap_or(true);
            let allow_na = item.get("allow_na").and_then(Value::as_bool).unwrap_or(false);
            let order = match item.get("order") {
                None | Some(Value::Null) => (index + 1) as i64,
                Some(value) => value
                    .as_i64()
                    .filter(|value| *value >= 0)
                    .ok_or_else(Self::invalid_safety_template_request)?,
            };
            let level = item.get("level").and_then(Value::as_str).map(str::trim);
            let level = Self::normalize_checklist_level(level, required)?;

            normalized.push(json!({
                "item_code": item_code,
                "title": title,
                "required": required,
                "allow_na": allow_na,
                "order": order,
                "level": level,
            }));
        }

        normalized.sort_by(|left, right| {
            let left_order = left.get("order").and_then(Value::as_i64).unwrap_or_default();
            let right_order = right.get("order").and_then(Value::as_i64).unwrap_or_default();
            left_order.cmp(&right_order).then_with(|| {
                left.get("item_code")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .cmp(right.get("item_code").and_then(Value::as_str).unwrap_or_default())
            })
        });

        Ok(normalized)
    }

    pub(super) fn severity_rank(severity: &str) -> i32 {
        match severity {
            "critical" => 4,
            "high" => 3,
            "medium" => 2,
            _ => 1,
        }
    }

    pub(super) fn effective_interval(
        order: &DispatchOrder,
        fallback_now: DateTime<Utc>,
    ) -> (DateTime<Utc>, DateTime<Utc>) {
        let start = order
            .actual_start_time
            .or(order.planned_start_time)
            .or(order.dispatched_at)
            .or(order.created_at)
            .unwrap_or(fallback_now);
        let mut end = order
            .actual_end_time
            .or(order.planned_end_time)
            .unwrap_or(start + Duration::minutes(15));
        if end < start {
            end = start;
        }
        (start, end)
    }

    pub(super) fn order_member_user_ids(order: &DispatchOrder) -> Vec<String> {
        let mut user_ids = Vec::new();
        let mut seen = HashSet::new();

        if let Some(user_id) = order
            .individual_user_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if seen.insert(user_id) {
                user_ids.push(user_id.to_string());
            }
        }

        for member in order.members.iter().filter(|member| member.is_active) {
            let normalized = member.user_id.trim();
            if !normalized.is_empty() && seen.insert(normalized) {
                user_ids.push(normalized.to_string());
            }
        }

        if let Some(members) = order.task_crew.get("members").and_then(Value::as_array) {
            for member in members {
                let normalized = member
                    .get("user_id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty());
                if let Some(normalized) = normalized {
                    if seen.insert(normalized) {
                        user_ids.push(normalized.to_string());
                    }
                }
            }
        }

        user_ids
    }

    pub(super) fn equipment_ids_from_order(order: &DispatchOrder) -> Vec<String> {
        let mut equipment_ids = Vec::new();

        for equipment in &order.equipment_list {
            let normalized = equipment.id.trim();
            if !normalized.is_empty() && !equipment_ids.iter().any(|existing| existing == normalized) {
                equipment_ids.push(normalized.to_string());
            }
        }

        for equipment in &order.equipment_assignment {
            let normalized = equipment
                .get("equipment_id")
                .or_else(|| equipment.get("id"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            if let Some(normalized) = normalized {
                if !equipment_ids.iter().any(|existing| existing == normalized) {
                    equipment_ids.push(normalized.to_string());
                }
            }
        }

        equipment_ids
    }

    pub(super) async fn resolve_followup_owner(&self, order: &DispatchOrder) -> Result<Option<String>, DomainError> {
        for member in order.members.iter().filter(|member| member.is_active) {
            if matches!(member.role, MemberRole::Leader) {
                let user_id = member.user_id.trim();
                if !user_id.is_empty() {
                    return Ok(Some(user_id.to_string()));
                }
            }
        }

        Ok(order
            .dispatched_by
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string))
    }

    pub(super) async fn find_open_followup_todo(
        &self,
        source_type: &str,
        source_id: &str,
    ) -> Result<Option<fms_domain::models::todo::Todo>, DomainError> {
        let todo_repo = self.order.todo_repo.as_ref();
        for todo in todo_repo.find_by_source(source_type, source_id).await? {
            if !todo.status.is_terminal() {
                return Ok(Some(todo));
            }
        }
        Ok(None)
    }

    pub(super) fn parse_todo_priority(value: &str) -> fms_domain::models::todo::TodoPriority {
        match value.trim() {
            "关键" | "critical" => fms_domain::models::todo::TodoPriority::Critical,
            "高" | "high" => fms_domain::models::todo::TodoPriority::High,
            "低" | "low" => fms_domain::models::todo::TodoPriority::Low,
            "后台" | "background" => fms_domain::models::todo::TodoPriority::Background,
            _ => fms_domain::models::todo::TodoPriority::Medium,
        }
    }

    pub(super) async fn ensure_followup_todo(
        &self,
        order: &DispatchOrder,
        actor_id: &str,
        source_type: &str,
        title: String,
        description: String,
        priority: &str,
        due_date: Option<DateTime<Utc>>,
        tags: Vec<String>,
    ) -> Result<Option<Value>, DomainError> {
        let todo_repo = self.order.todo_repo.as_ref();

        let owner_user_id = self.resolve_followup_owner(order).await?;
        let mut todo = if let Some(existing) = self.find_open_followup_todo(source_type, &order.id).await? {
            existing
        } else {
            fms_domain::models::todo::Todo {
                todo_id: ulid::Ulid::new().to_string(),
                title,
                description: Some(description),
                priority: Self::parse_todo_priority(priority),
                status: fms_domain::models::todo::TodoStatus::Pending,
                category: Some(fms_domain::models::todo::TodoCategory::Work),
                due_date,
                assigned_to: owner_user_id.clone(),
                tags,
                estimated_duration: Some(10),
                actual_duration: None,
                progress: 0,
                is_recurring: false,
                recurring_pattern: None,
                parent_todo_id: None,
                execution_order: 0,
                depends_on: vec![],
                source_type: source_type.to_string(),
                source_id: Some(order.id.clone()),
                is_deleted: false,
                deleted_at: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                created_by: if actor_id.trim().is_empty() {
                    "system".to_string()
                } else {
                    actor_id.to_string()
                },
                updated_by: if actor_id.trim().is_empty() {
                    "system".to_string()
                } else {
                    actor_id.to_string()
                },
                version: 1,
            }
        };

        if let Some(owner_user_id) = owner_user_id {
            if todo.assigned_to.as_deref() != Some(owner_user_id.as_str()) {
                todo.assigned_to = Some(owner_user_id.clone());
                if matches!(todo.status, fms_domain::models::todo::TodoStatus::Pending) {
                    todo.status = fms_domain::models::todo::TodoStatus::InProgress;
                }
                todo.updated_at = Utc::now();
                todo.updated_by = if actor_id.trim().is_empty() {
                    "system".to_string()
                } else {
                    actor_id.to_string()
                };
                todo.version += 1;
            }
        }

        todo_repo.save(&todo).await?;
        Ok(Some(json!({
            "todo_id": todo.todo_id,
            "created": true,
            "assigned_to": todo.assigned_to,
        })))
    }

    pub(super) async fn count_open_followups(&self, source_type: &str) -> Result<i64, DomainError> {
        let todo_repo = self.order.todo_repo.as_ref();
        Ok(todo_repo
            .find_all(None, None, None, None, Some(source_type), None, 10_000, 0)
            .await?
            .into_iter()
            .filter(|todo| !todo.status.is_terminal())
            .count() as i64)
    }
}
