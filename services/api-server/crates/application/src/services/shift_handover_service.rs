//! Shift handover application service.

use std::sync::Arc;

use chrono::{NaiveDate, Utc};
use serde_json::{json, Value};
use std::collections::HashSet;

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::DispatchOrderStatus;
use fms_domain::models::shift_handover::{ShiftHandover, ShiftHandoverItem};
use fms_domain::ports::shift_handover_repository::ShiftHandoverRepository;

use crate::types::{
    ConcreteAnomalyService, ConcreteDispatchQueryService, ConcreteNotificationService, ConcreteTodoService,
};

pub struct ShiftHandoverService {
    repo: Arc<dyn ShiftHandoverRepository + Send + Sync>,
    dispatch_query_service: Option<Arc<ConcreteDispatchQueryService>>,
    anomaly_service: Option<Arc<ConcreteAnomalyService>>,
    notification_service: Option<Arc<ConcreteNotificationService>>,
    todo_service: Option<Arc<ConcreteTodoService>>,
}

impl ShiftHandoverService {
    pub fn new(repo: Arc<dyn ShiftHandoverRepository + Send + Sync>) -> Self {
        Self {
            repo,
            dispatch_query_service: None,
            anomaly_service: None,
            notification_service: None,
            todo_service: None,
        }
    }

    pub fn with_dispatch_query_service(mut self, dispatch_query_service: Arc<ConcreteDispatchQueryService>) -> Self {
        self.dispatch_query_service = Some(dispatch_query_service);
        self
    }

    pub fn with_anomaly_service(mut self, anomaly_service: Arc<ConcreteAnomalyService>) -> Self {
        self.anomaly_service = Some(anomaly_service);
        self
    }

    pub fn with_notification_service(mut self, notification_service: Arc<ConcreteNotificationService>) -> Self {
        self.notification_service = Some(notification_service);
        self
    }

    pub fn with_todo_service(mut self, todo_service: Arc<ConcreteTodoService>) -> Self {
        self.todo_service = Some(todo_service);
        self
    }

    pub async fn create(
        &self,
        shift_date: NaiveDate,
        shift_code: &str,
        from_user_id: Option<&str>,
        to_user_id: &str,
        summary: Option<String>,
        risk_level: &str,
        item_payloads: Vec<ShiftHandoverItemCreateInput>,
        actor_user_id: &str,
        from_operator_name: Option<String>,
        from_operator_job_title: Option<String>,
    ) -> Result<ShiftHandover, DomainError> {
        let normalized_shift_code = shift_code.trim();
        if normalized_shift_code.is_empty() {
            return Err(DomainError::ValidationError("shift_code is required".into()));
        }

        let normalized_from = from_user_id.unwrap_or(actor_user_id).trim();
        let normalized_to = to_user_id.trim();
        if normalized_from.is_empty() {
            return Err(DomainError::ValidationError("from_user_id is required".into()));
        }
        if normalized_to.is_empty() {
            return Err(DomainError::ValidationError("to_user_id is required".into()));
        }
        if normalized_from == normalized_to {
            return Err(DomainError::ValidationError(
                "from_user_id and to_user_id must be different".into(),
            ));
        }

        let normalized_risk_level = normalize_risk_level(risk_level)?;
        let now = Utc::now();
        let handover_id = ulid::Ulid::new().to_string();
        let mut items = Vec::new();
        let mut seen_keys = HashSet::new();

        for generated in self.build_system_generated_items(normalized_from, normalized_to).await {
            let item = ShiftHandoverItemCreateInput::from_generated_item(generated).into_item(&handover_id, now)?;
            let key = generated_item_key(&item.item_type, &item.title);
            if seen_keys.insert(key) {
                items.push(item);
            }
        }

        for item in item_payloads {
            let item = item.into_item(&handover_id, now)?;
            let key = generated_item_key(&item.item_type, &item.title);
            if seen_keys.insert(key) {
                items.push(item);
            }
        }

        let handover = ShiftHandover {
            handover_id: handover_id.clone(),
            shift_date,
            shift_code: normalized_shift_code.to_string(),
            from_user_id: normalized_from.to_string(),
            to_user_id: normalized_to.to_string(),
            from_operator_name: normalize_optional_string(from_operator_name),
            from_operator_job_title: normalize_optional_string(from_operator_job_title),
            from_operator_label: None,
            to_operator_name: None,
            to_operator_job_title: None,
            to_operator_label: None,
            status: "draft".to_string(),
            summary: normalize_optional_string(summary),
            risk_level: normalized_risk_level.to_string(),
            signed_at: None,
            submitted_at: None,
            created_at: now,
            updated_at: now,
            items,
        };

        self.repo.create(&handover).await
    }

    pub async fn list(
        &self,
        shift_date: Option<NaiveDate>,
        shift_code: Option<&str>,
        status: Option<&str>,
        from_user_id: Option<&str>,
        to_user_id: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ShiftHandover>, DomainError> {
        let normalized_status = normalize_status_opt(status)?;
        self.repo
            .find_all(
                shift_date,
                normalize_optional_ref(shift_code),
                normalized_status.as_deref(),
                normalize_optional_ref(from_user_id),
                normalize_optional_ref(to_user_id),
                limit.max(1),
                offset.max(0),
            )
            .await
    }

    pub async fn get(&self, handover_id: &str) -> Result<Option<ShiftHandover>, DomainError> {
        self.repo.find_by_id(handover_id).await
    }

    pub async fn preview_system_draft(&self, user_id: &str, to_user_id: Option<&str>) -> Result<Value, DomainError> {
        let normalized_user_id = user_id.trim();
        let normalized_to_user_id = to_user_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(normalized_user_id);
        let items = self
            .build_system_generated_items(normalized_user_id, normalized_to_user_id)
            .await;

        Ok(json!({
            "generated_item_count": items.len(),
            "mandatory_count": items
                .iter()
                .filter(|item| item.get("is_mandatory").and_then(Value::as_bool).unwrap_or(true))
                .count(),
            "titles": items
                .iter()
                .take(5)
                .map(|item| item.get("title").and_then(Value::as_str).unwrap_or("").to_string())
                .collect::<Vec<_>>(),
            "items": items.into_iter().take(20).collect::<Vec<_>>(),
        }))
    }

    pub async fn submit(
        &self,
        handover_id: &str,
        actor_user_id: &str,
        is_admin: bool,
    ) -> Result<Option<ShiftHandover>, DomainError> {
        let Some(handover) = self.repo.find_by_id(handover_id).await? else {
            return Ok(None);
        };

        if !is_admin && handover.from_user_id != actor_user_id {
            return Err(DomainError::PermissionDenied(
                "only from_user can submit this handover".into(),
            ));
        }
        if handover.status != "draft" {
            return Err(DomainError::Conflict(format!(
                "handover status is {}, cannot submit",
                handover.status
            )));
        }

        self.repo.submit(handover_id).await
    }

    pub async fn acknowledge_item(
        &self,
        handover_id: &str,
        item_id: &str,
        actor_user_id: &str,
        acknowledged: bool,
        is_admin: bool,
    ) -> Result<Option<ShiftHandoverItem>, DomainError> {
        let Some(handover) = self.repo.find_by_id(handover_id).await? else {
            return Ok(None);
        };

        if !is_admin && handover.to_user_id != actor_user_id {
            return Err(DomainError::PermissionDenied(
                "only to_user can acknowledge handover items".into(),
            ));
        }
        if handover.status != "pending" && handover.status != "sign_off" {
            return Err(DomainError::Conflict(format!(
                "handover status is {}, cannot acknowledge items",
                handover.status
            )));
        }

        self.repo
            .acknowledge_item(handover_id, item_id, actor_user_id, acknowledged)
            .await
    }

    pub async fn complete(
        &self,
        handover_id: &str,
        actor_user_id: &str,
        is_admin: bool,
        to_operator_name: Option<String>,
        to_operator_job_title: Option<String>,
    ) -> Result<Option<ShiftHandover>, DomainError> {
        let Some(handover) = self.repo.find_by_id(handover_id).await? else {
            return Ok(None);
        };

        if !is_admin && handover.to_user_id != actor_user_id {
            return Err(DomainError::PermissionDenied(
                "only to_user can sign off this handover".into(),
            ));
        }
        if handover.status != "pending" && handover.status != "sign_off" {
            return Err(DomainError::Conflict(format!(
                "handover status is {}, cannot complete",
                handover.status
            )));
        }

        let pending_titles = self.repo.list_unacked_mandatory_titles(handover_id).await?;
        if !pending_titles.is_empty() {
            return Err(DomainError::Conflict(format!(
                "mandatory items pending acknowledgment: {}",
                pending_titles.join(", ")
            )));
        }

        self.repo
            .complete(
                handover_id,
                normalize_optional_ref_owned(&to_operator_name),
                normalize_optional_ref_owned(&to_operator_job_title),
            )
            .await
    }

    async fn build_system_generated_items(&self, from_user_id: &str, to_user_id: &str) -> Vec<Value> {
        let normalized_from = from_user_id.trim();
        if normalized_from.is_empty() {
            return Vec::new();
        }

        let mut items = Vec::new();

        if let Some(service) = &self.dispatch_query_service {
            if let Ok(orders) = service.list_my_orders(normalized_from, None).await {
                for order in orders {
                    let status = normalize_dispatch_status(order.status);
                    if matches!(status, "completed" | "cancelled") {
                        continue;
                    }
                    items.push(json!({
                        "item_type": "pending_task",
                        "title": format!("跟进工单 {} {}", order.id, order.task_type).trim().to_string(),
                        "detail": format!("当前状态: {status}"),
                        "owner_user_id": non_empty_owned(to_user_id),
                        "due_at": order.planned_end_time,
                        "is_mandatory": status == "in_progress",
                    }));
                }
            }
        }

        if let Some(service) = &self.anomaly_service {
            if let Ok(anomalies) = service.list_anomalies(Some("open"), None, None, None, 50, 0).await {
                for anomaly in anomalies {
                    let reported_by = anomaly
                        .context_data
                        .get("reported_by")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .trim();
                    if reported_by != normalized_from {
                        continue;
                    }

                    items.push(json!({
                        "item_type": "open_anomaly",
                        "title": format!("跟进异常 {}", anomaly.title).trim().to_string(),
                        "detail": format!("严重级别: {}", anomaly.severity),
                        "owner_user_id": non_empty_owned(to_user_id),
                        "due_at": Value::Null,
                        "is_mandatory": true,
                    }));
                }
            }
        }

        if let Some(service) = &self.notification_service {
            if let Ok(notifications) = service.list_notifications(normalized_from, true, 20, 0).await {
                for notification in notifications {
                    if notification.severity.trim().eq_ignore_ascii_case("critical") {
                        items.push(json!({
                            "item_type": "risk_note",
                            "title": format!("高风险通知待确认 {}", notification.title).trim().to_string(),
                            "detail": non_empty_owned(&notification.body),
                            "owner_user_id": non_empty_owned(to_user_id),
                            "due_at": Value::Null,
                            "is_mandatory": true,
                        }));
                    }
                }
            }
        }

        if let Some(service) = &self.todo_service {
            for source_type in ["dispatch_soft_followup", "dispatch_arrival_verification"] {
                if let Ok(todos) = service
                    .list_open_todos_by_source_for_assignee(source_type, normalized_from, 50)
                    .await
                {
                    for todo in todos {
                        items.push(json!({
                            "item_type": "pending_task",
                            "title": todo.title.trim().to_string(),
                            "detail": trim_to_owned(todo.description.as_deref()),
                            "owner_user_id": non_empty_owned(to_user_id),
                            "due_at": todo.due_date,
                            "is_mandatory": true,
                        }));
                    }
                }
            }
        }

        dedupe_generated_items(items)
    }
}

#[derive(Debug, Clone)]
pub struct ShiftHandoverItemCreateInput {
    pub item_type: String,
    pub title: String,
    pub detail: Option<String>,
    pub owner_user_id: Option<String>,
    pub due_at: Option<chrono::DateTime<Utc>>,
    pub is_mandatory: bool,
}

impl ShiftHandoverItemCreateInput {
    fn from_generated_item(item: Value) -> Self {
        Self {
            item_type: item
                .get("item_type")
                .and_then(Value::as_str)
                .unwrap_or("other")
                .to_string(),
            title: item
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            detail: trim_to_owned(item.get("detail").and_then(Value::as_str)),
            owner_user_id: item
                .get("owner_user_id")
                .and_then(Value::as_str)
                .map(ToString::to_string),
            due_at: item
                .get("due_at")
                .cloned()
                .and_then(|value| serde_json::from_value(value).ok()),
            is_mandatory: item.get("is_mandatory").and_then(Value::as_bool).unwrap_or(true),
        }
    }

    fn into_item(self, handover_id: &str, now: chrono::DateTime<Utc>) -> Result<ShiftHandoverItem, DomainError> {
        let normalized_title = self.title.trim();
        if normalized_title.is_empty() {
            return Err(DomainError::ValidationError("handover item title is required".into()));
        }

        Ok(ShiftHandoverItem {
            item_id: ulid::Ulid::new().to_string(),
            handover_id: handover_id.to_string(),
            item_type: normalize_item_type(&self.item_type),
            title: normalized_title.to_string(),
            detail: normalize_optional_string(self.detail),
            owner_user_id: normalize_optional_string(self.owner_user_id),
            due_at: self.due_at,
            is_mandatory: self.is_mandatory,
            acknowledged: false,
            acknowledged_at: None,
            acknowledged_by: None,
            created_at: now,
            updated_at: now,
        })
    }
}

fn normalize_status_opt(status: Option<&str>) -> Result<Option<String>, DomainError> {
    let Some(value) = normalize_optional_ref(status) else {
        return Ok(None);
    };
    match value {
        "draft" | "pending" | "sign_off" | "completed" => Ok(Some(value.to_string())),
        _ => Err(DomainError::ValidationError("invalid status".into())),
    }
}

fn normalize_risk_level(risk_level: &str) -> Result<&str, DomainError> {
    match risk_level.trim().to_ascii_lowercase().as_str() {
        "low" => Ok("low"),
        "medium" | "" => Ok("medium"),
        "high" => Ok("high"),
        "critical" => Ok("critical"),
        _ => Err(DomainError::ValidationError("invalid risk_level".into())),
    }
}

fn normalize_item_type(item_type: &str) -> String {
    match item_type.trim().to_ascii_lowercase().as_str() {
        "pending_task" => "pending_task".to_string(),
        "open_anomaly" => "open_anomaly".to_string(),
        "risk_note" => "risk_note".to_string(),
        _ => "other".to_string(),
    }
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|text| {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_optional_ref(value: Option<&str>) -> Option<&str> {
    value.and_then(|text| {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn normalize_optional_ref_owned(value: &Option<String>) -> Option<&str> {
    value.as_deref().and_then(|text| {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn normalize_dispatch_status(status: DispatchOrderStatus) -> &'static str {
    match status {
        DispatchOrderStatus::Pending => "pending",
        DispatchOrderStatus::Assigned => "assigned",
        DispatchOrderStatus::InProgress => "in_progress",
        DispatchOrderStatus::Completed => "completed",
        DispatchOrderStatus::Cancelled => "cancelled",
    }
}

fn trim_to_owned(value: Option<&str>) -> Option<String> {
    value.and_then(|text| {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn non_empty_owned(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn dedupe_generated_items(items: Vec<Value>) -> Vec<Value> {
    let mut deduped = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for item in items {
        let key = (
            item.get("item_type").and_then(Value::as_str).unwrap_or("").to_string(),
            item.get("title").and_then(Value::as_str).unwrap_or("").to_string(),
        );
        if seen.insert(key) {
            deduped.push(item);
        }
    }
    deduped
}

fn generated_item_key(item_type: &str, title: &str) -> (String, String) {
    (normalize_item_type(item_type), title.trim().to_string())
}
