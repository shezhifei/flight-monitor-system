//! 移动端工作台聚合服务。

use std::collections::HashSet;
use std::sync::Arc;

use chrono::Utc;
use serde_json::{json, Value};

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::DispatchOrderStatus;

use crate::types::{
    ConcreteDispatchChatService, ConcreteDispatchQueryService, ConcreteMobileDeviceService,
    ConcreteNotificationService, ConcreteShiftHandoverService, ConcreteTodoService,
};

pub struct MobileWorkbenchService {
    dispatch_query_service: Arc<ConcreteDispatchQueryService>,
    notification_service: Option<Arc<ConcreteNotificationService>>,
    dispatch_chat_service: Option<Arc<ConcreteDispatchChatService>>,
    shift_handover_service: Option<Arc<ConcreteShiftHandoverService>>,
    mobile_device_service: Option<Arc<ConcreteMobileDeviceService>>,
    todo_service: Option<Arc<ConcreteTodoService>>,
}

impl MobileWorkbenchService {
    pub fn new(
        dispatch_query_service: Arc<ConcreteDispatchQueryService>,
        notification_service: Option<Arc<ConcreteNotificationService>>,
        dispatch_chat_service: Option<Arc<ConcreteDispatchChatService>>,
        shift_handover_service: Option<Arc<ConcreteShiftHandoverService>>,
        mobile_device_service: Option<Arc<ConcreteMobileDeviceService>>,
        todo_service: Option<Arc<ConcreteTodoService>>,
    ) -> Self {
        Self {
            dispatch_query_service,
            notification_service,
            dispatch_chat_service,
            shift_handover_service,
            mobile_device_service,
            todo_service,
        }
    }

    async fn load_open_todo_source_ids(
        &self,
        order_ids: &HashSet<String>,
        source_type: &str,
    ) -> Result<HashSet<String>, DomainError> {
        if order_ids.is_empty() {
            return Ok(HashSet::new());
        }

        if let Some(todo_service) = &self.todo_service {
            let todos = todo_service
                .list_open_todos_by_source(source_type, (order_ids.len() as i64 * 4).max(20))
                .await?;
            return Ok(todos
                .into_iter()
                .filter_map(|todo| todo.source_id)
                .map(|source_id| source_id.trim().to_string())
                .filter(|source_id| order_ids.contains(source_id))
                .collect());
        }

        if source_type != "dispatch_soft_followup" {
            return Ok(HashSet::new());
        }

        let mut matched = HashSet::new();
        for order_id in order_ids {
            let Some(timeline) = self.dispatch_query_service.get_order_timeline(order_id, 50).await? else {
                continue;
            };
            if timeline
                .get("items")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .any(|entry| {
                    entry
                        .get("action")
                        .and_then(Value::as_str)
                        .map(|action| action == "soft_completion_followup_created")
                        .unwrap_or(false)
                })
            {
                matched.insert(order_id.clone());
            }
        }
        Ok(matched)
    }

    async fn annotate_order_states(&self, order_items: &mut [Value]) -> Result<(), DomainError> {
        let order_ids = order_items
            .iter()
            .filter_map(|item| item.get("order_id").and_then(Value::as_str))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .collect::<HashSet<_>>();
        let soft_followup_ids = self
            .load_open_todo_source_ids(&order_ids, "dispatch_soft_followup")
            .await?;
        let arrival_verification_ids = self
            .load_open_todo_source_ids(&order_ids, "dispatch_arrival_verification")
            .await?;

        for item in order_items.iter_mut() {
            let order_id = item
                .get("order_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            let status = item
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .trim()
                .to_ascii_lowercase();
            let soft_followup_required = soft_followup_ids.contains(&order_id);
            let arrival_verification_needed =
                matches!(status.as_str(), "pending" | "assigned") || arrival_verification_ids.contains(&order_id);
            let verification_status = if arrival_verification_ids.contains(&order_id) {
                Some("pending_verification")
            } else if matches!(status.as_str(), "in_progress" | "completed") {
                Some("verified")
            } else {
                None
            };
            let next_primary_action = resolve_primary_action(&status, soft_followup_required);

            if let Some(object) = item.as_object_mut() {
                object.insert(
                    "next_primary_action".to_string(),
                    Value::String(next_primary_action.to_string()),
                );
                object.insert(
                    "arrival_verification_needed".to_string(),
                    Value::Bool(arrival_verification_needed),
                );
                object.insert(
                    "verification_status".to_string(),
                    verification_status
                        .map(|value| Value::String(value.to_string()))
                        .unwrap_or(Value::Null),
                );
                object.insert(
                    "soft_followup_required".to_string(),
                    Value::Bool(soft_followup_required),
                );
            }
        }

        Ok(())
    }

    pub async fn build_workbench(
        &self,
        user_id: &str,
        pending_sync_action_count: i64,
        max_orders: i64,
    ) -> Result<serde_json::Value, DomainError> {
        let normalized_user_id = user_id.trim();
        if normalized_user_id.is_empty() {
            return Err(DomainError::ValidationError("user_id is required".into()));
        }

        let orders = self
            .dispatch_query_service
            .list_my_orders(normalized_user_id, None)
            .await?;
        let mut order_items = orders
            .into_iter()
            .map(|order| {
                json!({
                    "order_id": order.id,
                    "flight_id": order.flight_id,
                    "task_type": order.task_type,
                    "status": dispatch_order_status_value(order.status),
                    "terminal": order.terminal,
                    "stand_id": order.stand_id,
                    "gate": order.gate,
                    "planned_start_time": order.planned_start_time,
                    "planned_end_time": order.planned_end_time,
                    "actual_start_time": order.actual_start_time,
                    "assignment_deadline": order.assignment_deadline,
                    "supervisor_notified": order.supervisor_notified,
                    "next_primary_action": "view",
                    "arrival_verification_needed": false,
                    "verification_status": Value::Null,
                    "soft_followup_required": false,
                })
            })
            .collect::<Vec<_>>();
        self.annotate_order_states(&mut order_items).await?;
        order_items.sort_by(compare_order_items);

        let order_counts = order_items.iter().fold(
            json!({
                "pending": 0,
                "assigned": 0,
                "in_progress": 0,
                "completed": 0,
                "cancelled": 0,
                "total": order_items.len(),
            }),
            |mut counts, item| {
                if let Some(status) = item.get("status").and_then(|value| value.as_str()) {
                    if let Some(value) = counts.get_mut(status) {
                        *value = json!(value.as_i64().unwrap_or(0) + 1);
                    }
                }
                counts
            },
        );

        let notification_unread_count = match &self.notification_service {
            Some(service) => service.get_unread_count(normalized_user_id).await?,
            None => 0,
        };
        let critical_alerts = match &self.notification_service {
            Some(service) => service
                .list_notifications(normalized_user_id, true, 20, 0)
                .await?
                .into_iter()
                .filter(|item| item.severity.trim().eq_ignore_ascii_case("critical"))
                .take(5)
                .map(|item| {
                    json!({
                        "notification_id": item.notification_id,
                        "title": item.title,
                        "severity": item.severity,
                        "category": item.category,
                        "related_entity_type": item.related_entity_type,
                        "related_entity_id": item.related_entity_id,
                        "created_at": item.created_at,
                    })
                })
                .collect::<Vec<_>>(),
            None => Vec::new(),
        };

        let chat_unread_total = match &self.dispatch_chat_service {
            Some(service) => service
                .list_user_groups(normalized_user_id, "active", 1, 0)
                .await
                .map(|payload| payload.unread_total)
                .map_err(|error| DomainError::Internal(error.to_string()))?,
            None => 0,
        };

        let pending_shift_handover_count = match &self.shift_handover_service {
            Some(service) => service
                .list(None, None, Some("pending"), None, Some(normalized_user_id), 200, 0)
                .await?
                .len() as i64,
            None => 0,
        };
        let handover_draft_summary = match &self.shift_handover_service {
            Some(service) => match service.preview_system_draft(normalized_user_id, None).await {
                Ok(summary) => summary,
                Err(_) => json!({}),
            },
            None => json!({}),
        };

        let channel_recommendation = match &self.mobile_device_service {
            Some(service) => service.resolve_delivery_channels(normalized_user_id).await?,
            None => std::collections::HashMap::from([
                ("push".to_string(), false),
                ("sse".to_string(), true),
                ("in_app".to_string(), true),
            ]),
        };

        let soft_followups_count = order_items
            .iter()
            .filter(|item| {
                item.get("soft_followup_required")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            })
            .count() as i64;
        let arrival_verification_needed = order_items.iter().any(|item| {
            item.get("arrival_verification_needed")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        });
        let mut next_primary_action = "view".to_string();
        for item in &order_items {
            let action = item
                .get("next_primary_action")
                .and_then(Value::as_str)
                .unwrap_or("view");
            if action != "view" {
                next_primary_action = action.to_string();
                break;
            }
        }
        if next_primary_action == "view" && pending_shift_handover_count > 0 {
            next_primary_action = "review_handover".to_string();
        }

        Ok(json!({
            "user_id": normalized_user_id,
            "generated_at": Utc::now(),
            "my_orders": order_items.into_iter().take(max_orders.max(1) as usize).collect::<Vec<_>>(),
            "order_counts": order_counts,
            "notification_unread_count": notification_unread_count,
            "chat_unread_total": chat_unread_total,
            "pending_shift_handover_count": pending_shift_handover_count,
            "pending_sync_action_count": pending_sync_action_count.max(0),
            "channel_recommendation": channel_recommendation,
            "next_primary_action": next_primary_action,
            "arrival_verification_needed": arrival_verification_needed,
            "soft_followups_count": soft_followups_count,
            "critical_alerts": critical_alerts,
            "handover_draft_summary": handover_draft_summary,
        }))
    }
}

fn resolve_primary_action(status: &str, soft_followup_required: bool) -> &'static str {
    match status {
        "pending" | "assigned" => "arrive",
        "in_progress" => "complete",
        "completed" if soft_followup_required => "review_followup",
        _ => "view",
    }
}

fn compare_order_items(left: &Value, right: &Value) -> std::cmp::Ordering {
    action_priority(left)
        .cmp(&action_priority(right))
        .then_with(|| order_sort_key(left).cmp(&order_sort_key(right)))
}

fn action_priority(item: &Value) -> i32 {
    match item
        .get("next_primary_action")
        .and_then(Value::as_str)
        .unwrap_or("view")
    {
        "arrive" => 0,
        "complete" => 1,
        "review_followup" => 2,
        "review_handover" => 3,
        "view" => 4,
        _ => 9,
    }
}

fn order_sort_key(item: &Value) -> String {
    for key in [
        "assignment_deadline",
        "planned_start_time",
        "actual_start_time",
        "created_at",
    ] {
        if let Some(value) = item.get(key).and_then(Value::as_str) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }
    Utc::now().to_rfc3339()
}

fn dispatch_order_status_value(status: DispatchOrderStatus) -> String {
    match status {
        DispatchOrderStatus::Pending => "pending",
        DispatchOrderStatus::Assigned => "assigned",
        DispatchOrderStatus::InProgress => "in_progress",
        DispatchOrderStatus::Completed => "completed",
        DispatchOrderStatus::Cancelled => "cancelled",
    }
    .to_string()
}
