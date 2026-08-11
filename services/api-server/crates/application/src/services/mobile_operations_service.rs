//! 移动端运营事件聚合服务。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde_json::json;

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::DispatchOrderStatus;

use crate::services::anomaly_service::AnomalyResponse;
use crate::services::notification_service::NotificationResponse;
use crate::types::{ConcreteAnomalyService, ConcreteDispatchQueryService, ConcreteNotificationService};

pub struct MobileOperationsService {
    dispatch_query_service: Arc<ConcreteDispatchQueryService>,
    anomaly_service: Option<Arc<ConcreteAnomalyService>>,
    notification_service: Option<Arc<ConcreteNotificationService>>,
}

impl MobileOperationsService {
    pub fn new(
        dispatch_query_service: Arc<ConcreteDispatchQueryService>,
        anomaly_service: Option<Arc<ConcreteAnomalyService>>,
        notification_service: Option<Arc<ConcreteNotificationService>>,
    ) -> Self {
        Self {
            dispatch_query_service,
            anomaly_service,
            notification_service,
        }
    }

    pub async fn build_event_feed(
        &self,
        user_id: &str,
        is_admin: bool,
        limit: i64,
    ) -> Result<serde_json::Value, DomainError> {
        let normalized_user_id = user_id.trim();
        if normalized_user_id.is_empty() {
            return Err(DomainError::ValidationError("user_id is required".into()));
        }

        let my_orders = self
            .dispatch_query_service
            .list_my_orders(normalized_user_id, None)
            .await?;

        let mut events = my_orders.iter().map(dispatch_order_to_event).collect::<Vec<_>>();

        if let Some(anomaly_service) = &self.anomaly_service {
            let anomalies = anomaly_service
                .list_anomalies(Some("open"), None, None, None, limit.max(50), 0)
                .await?;
            if is_admin {
                events.extend(anomalies.iter().map(anomaly_to_event));
            } else {
                let my_flights = my_orders
                    .iter()
                    .filter_map(|order| {
                        let flight_id = order.flight_id.trim();
                        if flight_id.is_empty() {
                            None
                        } else {
                            Some(flight_id.to_string())
                        }
                    })
                    .collect::<HashSet<_>>();
                events.extend(anomalies.iter().filter_map(|item| {
                    let flight_id = item.flight_id.trim();
                    if flight_id.is_empty() || my_flights.contains(flight_id) {
                        Some(anomaly_to_event(item))
                    } else {
                        None
                    }
                }));
            }
        }

        if let Some(notification_service) = &self.notification_service {
            let notifications = notification_service
                .list_notifications(normalized_user_id, true, limit.max(20), 0)
                .await?;
            events.extend(notifications.iter().map(notification_to_event));
        }

        events.sort_by_key(|item| {
            std::cmp::Reverse(
                parse_timestamp(item.get("occurred_at").and_then(|value| value.as_str())).unwrap_or_else(Utc::now),
            )
        });
        events.truncate(limit.max(1) as usize);

        Ok(json!({
            "user_id": normalized_user_id,
            "generated_at": Utc::now(),
            "total": events.len(),
            "event_type_counts": count_by_key(&events, "event_type"),
            "severity_counts": count_by_key(&events, "severity"),
            "events": events,
        }))
    }
}

fn dispatch_order_to_event(order: &fms_domain::models::dispatch::DispatchOrder) -> serde_json::Value {
    let status = dispatch_order_status_value(order.status);
    let severity = match status.as_str() {
        "pending" | "assigned" => "critical",
        "in_progress" => "warning",
        _ => "info",
    };
    let occurred_at = order
        .actual_start_time
        .or(order.planned_start_time)
        .or(order.created_at)
        .unwrap_or_else(Utc::now);
    json!({
        "event_id": format!("dispatch:{}", order.id),
        "event_type": "dispatch_order",
        "severity": severity,
        "status": status,
        "title": format!("工单 {}", order.task_type),
        "flight_id": order.flight_id,
        "occurred_at": occurred_at,
        "source": "dispatch_orders",
        "payload": {
            "order_id": order.id,
            "task_type": order.task_type,
            "planned_start_time": order.planned_start_time,
            "planned_end_time": order.planned_end_time,
            "assignment_deadline": order.assignment_deadline,
        }
    })
}

fn anomaly_to_event(item: &AnomalyResponse) -> serde_json::Value {
    json!({
        "event_id": format!("anomaly:{}", item.anomaly_id),
        "event_type": "anomaly",
        "severity": normalize_text_value(&item.severity, "info"),
        "status": normalize_text_value(&item.status, "open"),
        "title": item.title,
        "flight_id": item.flight_id,
        "occurred_at": item.detected_at,
        "source": "anomalies",
        "payload": {
            "anomaly_id": item.anomaly_id,
            "anomaly_type": normalize_text_value(&item.anomaly_type, "unknown"),
            "description": item.description,
            "linked_todo_id": item.linked_todo_id,
        }
    })
}

fn notification_to_event(item: &NotificationResponse) -> serde_json::Value {
    let related_flight_id = if normalize_optional_text(item.related_entity_type.as_deref()).as_deref() == Some("flight")
    {
        item.related_entity_id.clone()
    } else {
        None
    };
    json!({
        "event_id": format!("notification:{}", item.notification_id),
        "event_type": "notification",
        "severity": normalize_text_value(&item.severity, "info"),
        "status": if item.is_read { "read" } else { "unread" },
        "title": item.title,
        "flight_id": related_flight_id,
        "occurred_at": item.created_at,
        "source": "notifications",
        "payload": {
            "notification_id": item.notification_id,
            "category": item.category,
            "ack_status": item.ack_status,
            "delivery_status": item.delivery_status,
            "related_entity_type": item.related_entity_type,
            "related_entity_id": item.related_entity_id,
        }
    })
}

fn count_by_key(items: &[serde_json::Value], key: &str) -> HashMap<String, i64> {
    let mut counters = HashMap::new();
    for item in items {
        let value = item
            .get(key)
            .and_then(|value| value.as_str())
            .unwrap_or("unknown")
            .trim()
            .to_ascii_lowercase();
        *counters.entry(value).or_insert(0) += 1;
    }
    counters
}

fn parse_timestamp(value: Option<&str>) -> Option<DateTime<Utc>> {
    value
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc))
}

fn normalize_text_value(value: &str, fallback: &str) -> String {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        fallback.to_string()
    } else {
        normalized
    }
}

fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    let normalized = value.unwrap_or("").trim().to_ascii_lowercase();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
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
