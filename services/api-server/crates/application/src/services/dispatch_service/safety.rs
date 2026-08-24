use chrono::Utc;
use serde_json::{json, Value};
use std::collections::HashSet;

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::*;

use crate::schemas::dispatch_schemas::*;

use super::{DispatchService, NULL_VALUE};

impl DispatchService {
    /// 提交安全检查清单项
    pub async fn submit_safety_checklist_item(
        &self,
        order_id: &str,
        item_code: &str,
        dto: SafetyChecklistItemRequest,
        actor_id: &str,
    ) -> Result<serde_json::Value, DomainError> {
        Self::ensure_actor(actor_id)?;
        let order = self
            .order
            .order_repo
            .find_by_id(order_id, false, None)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "DispatchOrder",
                id: order_id.to_string(),
            })?;
        Self::ensure_order_execution_published(&order, "提交安全检查清单")?;
        if matches!(
            order.status,
            DispatchOrderStatus::Completed | DispatchOrderStatus::Cancelled
        ) {
            return Err(DomainError::BusinessRuleViolation(
                "当前状态不可提交安全检查清单".to_string(),
            ));
        }
        if !matches!(
            order.status,
            DispatchOrderStatus::Assigned | DispatchOrderStatus::InProgress
        ) {
            return Err(DomainError::BusinessRuleViolation(format!(
                "当前状态不可提交清单，当前状态: {:?}",
                order.status
            )));
        }
        self.ensure_actor_can_complete_order(&order, order_id, actor_id, "无权操作此派工单安全检查清单")
            .await?;

        let checklist_repo = self.resources.checklist_repo.as_ref();
        let template = checklist_repo
            .get_template(&order.task_type)
            .await?
            .ok_or_else(|| DomainError::BusinessRuleViolation("invalid safety checklist item result".to_string()))?;
        let normalized_result = Self::normalize_checklist_result(dto.result.as_deref())?;
        let checklist_items = template
            .get("checklist_items")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let template_item = checklist_items
            .into_iter()
            .find(|item| {
                item.get("item_code")
                    .and_then(|v| v.as_str())
                    .map(|code| code == item_code)
                    .unwrap_or(false)
            })
            .ok_or_else(|| DomainError::BusinessRuleViolation("invalid safety checklist item result".to_string()))?;
        if normalized_result == "na" && !template_item.get("allow_na").and_then(|v| v.as_bool()).unwrap_or(false) {
            return Err(DomainError::BusinessRuleViolation(
                "invalid safety checklist item result".to_string(),
            ));
        }
        if template_item
            .get("level")
            .and_then(|v| v.as_str())
            .unwrap_or("critical")
            == "critical"
            && normalized_result == "fail"
            && !dto.handled_on_site
            && dto
                .note
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_none()
        {
            return Err(DomainError::BusinessRuleViolation(
                "critical fail item requires note or handled_on_site flag".to_string(),
            ));
        }

        let record = checklist_repo
            .submit_item_result(
                order_id,
                &order.task_type,
                item_code,
                Some(normalized_result),
                dto.note.as_deref(),
                actor_id,
            )
            .await?;

        self.order
            .order_repo
            .append_log(
                order_id,
                "safety_checklist_item",
                Some(actor_id),
                Some(serde_json::json!({
                    "item_code": item_code,
                    "result": normalized_result,
                    "note": dto.note,
                    "handled_on_site": dto.handled_on_site,
                    "template_version": record.get("template_version").unwrap_or(&NULL_VALUE),
                })),
            )
            .await?;

        Ok(record)
    }

    pub async fn submit_safety_checklist_batch(
        &self,
        order_id: &str,
        dto: DispatchSafetyChecklistBatchSubmitRequest,
        actor_id: &str,
    ) -> Result<Value, DomainError> {
        Self::ensure_actor(actor_id)?;
        let order = self
            .order
            .order_repo
            .find_by_id(order_id, false, None)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "DispatchOrder",
                id: order_id.to_string(),
            })?;
        Self::ensure_order_execution_published(&order, "批量安全检查")?;
        if matches!(
            order.status,
            DispatchOrderStatus::Completed | DispatchOrderStatus::Cancelled
        ) {
            return Err(DomainError::BusinessRuleViolation(
                "当前状态不可提交安全检查清单".to_string(),
            ));
        }
        if !matches!(
            order.status,
            DispatchOrderStatus::Assigned | DispatchOrderStatus::InProgress
        ) {
            return Err(DomainError::BusinessRuleViolation(format!(
                "当前状态不可提交清单，当前状态: {:?}",
                order.status
            )));
        }
        self.ensure_actor_can_complete_order(&order, order_id, actor_id, "无权操作此派工单安全检查清单")
            .await?;

        let checklist_repo = self.resources.checklist_repo.as_ref();
        if dto.items.is_empty() {
            return Err(DomainError::BusinessRuleViolation(
                "invalid safety checklist batch submission".to_string(),
            ));
        }

        let mut seen_codes = HashSet::new();
        let mut records = Vec::with_capacity(dto.items.len());
        for item in dto.items {
            let item_code = item.item_code.trim();
            if item_code.is_empty() || !seen_codes.insert(item_code.to_string()) {
                return Err(DomainError::BusinessRuleViolation(
                    "invalid safety checklist batch submission".to_string(),
                ));
            }
            records.push(
                self.submit_safety_checklist_item(
                    order_id,
                    item_code,
                    SafetyChecklistItemRequest {
                        result: Some(item.result),
                        note: item.note,
                        handled_on_site: item.handled_on_site,
                    },
                    actor_id,
                )
                .await?,
            );
        }

        let template = checklist_repo.get_template(&order.task_type).await?;
        let all_records = checklist_repo.list_records(order_id).await?;
        let gate = Self::build_checklist_status(order_id, &order.task_type, template.as_ref(), &all_records)?;

        let empty_array = serde_json::Value::Array(vec![]);
        let zero = serde_json::Value::from(0);
        let true_val = serde_json::Value::Bool(true);
        self.order
            .order_repo
            .append_log(
                order_id,
                "safety_checklist_batch",
                Some(actor_id),
                Some(json!({
                    "submitted_count": records.len(),
                    "blocking_issues": gate.get("blocking_issues").unwrap_or(&empty_array),
                    "soft_missing_count": gate.get("soft_missing_count").unwrap_or(&zero),
                    "template_version": gate.get("template_version").unwrap_or(&NULL_VALUE),
                })),
            )
            .await?;

        Ok(json!({
            "dispatch_order_id": order_id,
            "task_type": order.task_type,
            "submitted_count": records.len(),
            "records": records,
            "gate": {
                "blocking_issues": gate.get("blocking_issues").unwrap_or(&empty_array),
                "soft_missing_count": gate.get("soft_missing_count").unwrap_or(&zero),
                "can_soft_complete": gate.get("can_soft_complete").unwrap_or(&true_val),
                "required_total": gate.get("required_total").unwrap_or(&zero),
                "completed_required": gate.get("completed_required").unwrap_or(&zero),
                "template_version": gate.get("template_version").unwrap_or(&NULL_VALUE),
            }
        }))
    }

    pub async fn get_followup_queue(
        &self,
        assignee: Option<&str>,
        source_type: Option<&str>,
        limit: i64,
    ) -> Result<Value, DomainError> {
        let allowed_source_types = [
            ("dispatch_soft_followup", "soft_followup"),
            ("dispatch_arrival_verification", "arrival_verification"),
        ];
        let normalized_source_type = source_type.map(str::trim).filter(|value| !value.is_empty());
        if let Some(source_type) = normalized_source_type {
            if !allowed_source_types
                .iter()
                .any(|(candidate, _)| *candidate == source_type)
            {
                return Err(DomainError::ValidationError("invalid source_type".to_string()));
            }
        }
        let todo_repo = self.order.todo_repo.as_ref();

        let mut items = Vec::new();
        let fetch_limit = 10_000;
        for (source_type_key, followup_kind) in allowed_source_types {
            if normalized_source_type.is_some() && normalized_source_type != Some(source_type_key) {
                continue;
            }
            for todo in todo_repo
                .find_all(
                    None,
                    None,
                    None,
                    assignee.map(str::trim).filter(|value| !value.is_empty()),
                    Some(source_type_key),
                    None,
                    fetch_limit,
                    0,
                )
                .await?
            {
                if todo.status.is_terminal() {
                    continue;
                }
                items.push(json!({
                    "todo_id": todo.todo_id,
                    "title": todo.title,
                    "description": todo.description,
                    "status": todo.status.label(),
                    "priority": todo.priority.label(),
                    "due_date": todo.due_date,
                    "assigned_to": todo.assigned_to,
                    "source_type": todo.source_type,
                    "source_id": todo.source_id,
                    "tags": todo.tags,
                    "followup_kind": followup_kind,
                }));
            }
        }

        items.sort_by(|left, right| {
            let left_due = left.get("due_date").and_then(Value::as_str).unwrap_or("9999");
            let right_due = right.get("due_date").and_then(Value::as_str).unwrap_or("9999");
            left_due.cmp(right_due).then_with(|| {
                left.get("title")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .cmp(right.get("title").and_then(Value::as_str).unwrap_or_default())
            })
        });

        let total = items.len();
        let pending_verification_count = items
            .iter()
            .filter(|item| item.get("source_type").and_then(Value::as_str) == Some("dispatch_arrival_verification"))
            .count();
        let soft_followup_count = items
            .iter()
            .filter(|item| item.get("source_type").and_then(Value::as_str) == Some("dispatch_soft_followup"))
            .count();
        let items = items
            .into_iter()
            .take(limit.max(1).min(200) as usize)
            .collect::<Vec<_>>();
        Ok(json!({
            "generated_at": Utc::now(),
            "assignee": assignee.map(str::trim).filter(|value| !value.is_empty()),
            "total": total,
            "pending_verification_count": pending_verification_count,
            "soft_followup_count": soft_followup_count,
            "items": items,
        }))
    }

    pub async fn get_burden_metrics(&self) -> Result<Value, DomainError> {
        let counters = self.snapshot_metrics();
        let blocked_completion_count = *counters.get("dispatch.order.complete.blocked").unwrap_or(&0);
        let soft_completion_count = *counters.get("dispatch.order.complete.soft").unwrap_or(&0);
        let pending_arrival_verification_count = *counters
            .get("dispatch.order.arrival.pending_verification")
            .unwrap_or(&0);
        let issue_reported_text = *counters.get("dispatch.issue_reported.text").unwrap_or(&0);
        let issue_reported_photo = *counters.get("dispatch.issue_reported.photo").unwrap_or(&0);
        let issue_reported_voice = *counters.get("dispatch.issue_reported.voice").unwrap_or(&0);

        let open_soft_followups = self.count_open_followups("dispatch_soft_followup").await?;
        let open_arrival_verifications = self.count_open_followups("dispatch_arrival_verification").await?;
        let raw_counters = json!(counters);

        Ok(json!({
            "generated_at": Utc::now(),
            "blocked_completion_count": blocked_completion_count,
            "soft_completion_count": soft_completion_count,
            "pending_arrival_verification_count": pending_arrival_verification_count,
            "issue_reported_counts": {
                "text": issue_reported_text,
                "photo": issue_reported_photo,
                "voice": issue_reported_voice,
            },
            "open_soft_followups": open_soft_followups,
            "open_arrival_verifications": open_arrival_verifications,
            "raw_counters": raw_counters,
        }))
    }

    /// 获取安全检查清单模板
    pub async fn get_safety_template(&self, task_type: &str) -> Result<Option<serde_json::Value>, DomainError> {
        let checklist_repo = self.resources.checklist_repo.as_ref();
        checklist_repo.get_template(task_type).await
    }

    /// 获取派工单安全检查清单
    pub async fn get_order_safety_checklist(&self, order_id: &str) -> Result<serde_json::Value, DomainError> {
        let order = self
            .order
            .order_repo
            .find_by_id(order_id, false, None)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "DispatchOrder",
                id: order_id.to_string(),
            })?;

        {
            let checklist_repo = self.resources.checklist_repo.as_ref();
            let template = checklist_repo.get_template(&order.task_type).await?;
            let records = checklist_repo.list_records(order_id).await?;
            return Self::build_checklist_status(order_id, &order.task_type, template.as_ref(), &records);
        }

        let logs = self.order.order_repo.list_logs(order_id, 200).await?;
        let checklist_items: Vec<&serde_json::Value> = logs
            .iter()
            .filter(|log| {
                log.get("action")
                    .and_then(|v| v.as_str())
                    .map(|a| a == "safety_checklist_item")
                    .unwrap_or(false)
            })
            .collect();

        let total = checklist_items.len() as f64;
        let checked = checklist_items
            .iter()
            .filter(|log| {
                log.get("details")
                    .and_then(|d| d.get("result"))
                    .and_then(|r| r.as_str())
                    .map(|r| r == "pass" || r == "ok")
                    .unwrap_or(false)
            })
            .count() as f64;
        let progress = if total > 0.0 { checked / total } else { 0.0 };

        Ok(serde_json::json!({
            "order_id": order_id,
            "items": checklist_items,
            "progress": progress,
            "total": total as i64,
            "checked": checked as i64,
        }))
    }

    /// 更新安全检查清单模板
    pub async fn upsert_safety_template(
        &self,
        task_type: &str,
        dto: SafetyTemplateUpsertRequest,
        actor_id: &str,
    ) -> Result<serde_json::Value, DomainError> {
        Self::ensure_actor(actor_id)?;
        let normalized_task_type = task_type.trim();
        if normalized_task_type.is_empty() {
            return Err(Self::invalid_safety_template_request());
        }

        let checklist_repo = self.resources.checklist_repo.as_ref();
        let normalized_version = Self::normalize_safety_template_version(&dto.checklist_version)?;
        let normalized_items = Self::normalize_safety_template_items(&dto.checklist_items)?;

        checklist_repo
            .upsert_template(
                &ulid::Ulid::new().to_string(),
                normalized_task_type,
                &normalized_version,
                &normalized_items,
                dto.is_active,
                Some(actor_id),
            )
            .await
    }

    /// 批量查询安全检查清单进度
    pub async fn evaluate_checklist_progress(
        &self,
        items: Vec<SafetyChecklistProgressOrderItem>,
    ) -> Result<serde_json::Value, DomainError> {
        let mut normalized_items = Vec::new();
        let mut seen = HashSet::new();
        for item in &items {
            let order_id = item.dispatch_order_id.trim();
            let task_type = item.task_type.trim();
            if order_id.is_empty() || task_type.is_empty() {
                continue;
            }
            let key = (order_id.to_string(), task_type.to_string());
            if seen.insert(key.clone()) {
                normalized_items.push(key);
            }
        }

        {
            let checklist_repo = self.resources.checklist_repo.as_ref();
            let empty_array = serde_json::Value::Array(vec![]);
            let zero = serde_json::Value::from(0);
            let true_val = serde_json::Value::Bool(true);
            let false_val = serde_json::Value::Bool(false);
            let mut results = Vec::new();
            for (order_id, task_type) in &normalized_items {
                let template = checklist_repo.get_template(task_type).await?;
                let records = checklist_repo.list_records(order_id).await?;
                let status = Self::build_checklist_status(order_id, task_type, template.as_ref(), &records)?;
                let pending_required_count = status
                    .get("pending_required_items")
                    .and_then(|v| v.as_array())
                    .map(|items| items.len())
                    .unwrap_or(0);
                let failed_required_count = status
                    .get("failed_required_items")
                    .and_then(|v| v.as_array())
                    .map(|items| items.len())
                    .unwrap_or(0);

                results.push(serde_json::json!({
                    "dispatch_order_id": order_id,
                    "task_type": task_type,
                    "enforced": status.get("enforced").unwrap_or(&false_val),
                    "ready": status.get("ready").unwrap_or(&true_val),
                    "required_total": status.get("required_total").unwrap_or(&zero),
                    "completed_required": status.get("completed_required").unwrap_or(&zero),
                    "pending_required_count": pending_required_count,
                    "failed_required_count": failed_required_count,
                    "template_version": status.get("template_version").unwrap_or(&NULL_VALUE),
                    "blocking_issues": status.get("blocking_issues").unwrap_or(&empty_array),
                    "soft_missing_count": status.get("soft_missing_count").unwrap_or(&zero),
                    "can_soft_complete": status.get("can_soft_complete").unwrap_or(&true_val),
                    "routine_total": status.get("routine_total").unwrap_or(&zero),
                    "completed_routine": status.get("completed_routine").unwrap_or(&zero),
                }));
            }

            return Ok(serde_json::json!({
                "items": results,
                "total": normalized_items.len(),
            }));
        }

        let mut results = Vec::new();
        for (order_id, task_type) in &normalized_items {
            match self.get_order_safety_checklist(order_id).await {
                Ok(checklist) => {
                    let total = checklist.get("total").and_then(|v| v.as_i64()).unwrap_or(0);
                    let checked = checklist.get("checked").and_then(|v| v.as_i64()).unwrap_or(0);
                    let progress = checklist.get("progress").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    results.push(serde_json::json!({
                        "dispatch_order_id": order_id,
                        "task_type": task_type,
                        "progress": progress,
                        "total_items": total,
                        "checked_items": checked,
                        "can_complete": total > 0 && checked >= total,
                    }));
                }
                Err(_) => {
                    results.push(serde_json::json!({
                        "dispatch_order_id": order_id,
                        "task_type": task_type,
                        "progress": 0.0,
                        "total_items": 0,
                        "checked_items": 0,
                        "can_complete": false,
                    }));
                }
            }
        }

        Ok(serde_json::json!({
            "items": results,
            "total": normalized_items.len(),
        }))
    }
}
