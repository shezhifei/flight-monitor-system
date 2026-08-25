//! 派工协作读侧查询服务。

use std::sync::Arc;

use serde_json::{json, Value};

use fms_domain::error::DomainError;
use fms_domain::models::dispatch_collaboration::{
    DispatchChatGroupSummary, DispatchChatMessageCursor, DispatchCollaborationEvent, DispatchFlightCollaborationView,
    DispatchOrderCollaborationView,
};
use fms_domain::models::notification::Notification;
use fms_domain::ports::dispatch_collaboration_repository::DispatchCollaborationRepository;
use fms_domain::ports::dispatch_repository::DispatchOrderRepository;

use crate::services::dispatch_query_service::dispatch_order_to_value_with_summary;

const MESSAGE_EVENT_TYPE: &str = "message_sent";

pub struct DispatchCollaborationQueryService {
    collaboration_repo: Arc<dyn DispatchCollaborationRepository + Send + Sync>,
    order_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
}

impl DispatchCollaborationQueryService {
    pub fn new(
        collaboration_repo: Arc<dyn DispatchCollaborationRepository + Send + Sync>,
        order_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
    ) -> Self {
        Self {
            collaboration_repo,
            order_repo,
        }
    }

    pub async fn get_flight_view(
        &self,
        flight_id: &str,
        user_id: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<DispatchFlightCollaborationView, DomainError> {
        let orders = self.order_repo.find_by_flight(flight_id).await?;
        let group = self.resolve_group_for_flight(flight_id, user_id).await?;
        let recent_messages = match group.as_ref() {
            Some(group) => {
                self.collaboration_repo
                    .list_group_messages(&group.group_id, limit.clamp(1, 20), DispatchChatMessageCursor::Latest)
                    .await?
                    .items
            }
            None => Vec::new(),
        };

        let order_payload = self.serialize_orders_with_receipt_summaries(&orders).await?;
        let notification_receipt_summary = self.collaboration_repo.summarize_receipts_for_flight(flight_id).await?;

        Ok(DispatchFlightCollaborationView {
            flight_id: flight_id.to_string(),
            orders: order_payload,
            group,
            recent_messages,
            recent_notifications: self
                .collaboration_repo
                .find_recent_notifications_by_flight(flight_id, 10)
                .await?
                .iter()
                .map(notification_to_value)
                .collect(),
            notification_receipt_summary,
            events: self
                .collaboration_repo
                .list_events_by_flight(flight_id, limit.clamp(1, 200), offset.max(0))
                .await?,
            total_orders: orders.len() as i64,
        })
    }

    pub async fn get_group_summary_for_flight(
        &self,
        flight_id: &str,
        user_id: Option<&str>,
    ) -> Result<Option<DispatchChatGroupSummary>, DomainError> {
        self.resolve_group_for_flight(flight_id, user_id).await
    }

    pub async fn list_flight_events(&self, flight_id: &str, limit: i64, offset: i64) -> Result<Value, DomainError> {
        Ok(json!({
            "flight_id": flight_id,
            "items": self.collaboration_repo.list_events_by_flight(flight_id, limit.clamp(1, 200), offset.max(0)).await?,
            "limit": limit.clamp(1, 200),
            "offset": offset.max(0),
        }))
    }

    pub async fn get_order_view(
        &self,
        order_id: &str,
        user_id: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Option<DispatchOrderCollaborationView>, DomainError> {
        let Some(order) = self.order_repo.find_by_id(order_id, true, None).await? else {
            return Ok(None);
        };

        let group = match user_id {
            Some(user_id) if !user_id.trim().is_empty() => {
                self.collaboration_repo
                    .get_group_for_user_by_flight(&order.flight_id, user_id)
                    .await?
            }
            _ => None,
        };

        let events = self
            .collaboration_repo
            .list_events_by_order(order_id, limit.clamp(1, 200), offset.max(0))
            .await?;

        let notification_receipt_summary = self.collaboration_repo.summarize_receipts_for_order(order_id).await?;

        Ok(Some(DispatchOrderCollaborationView {
            order: dispatch_order_to_value_with_summary(&order, Some(&notification_receipt_summary)),
            group,
            recent_messages: filter_message_events(&events),
            recent_notifications: self
                .collaboration_repo
                .find_recent_notifications_by_order(order_id, 10)
                .await?
                .iter()
                .map(notification_to_value)
                .collect(),
            notification_receipt_summary,
            events,
        }))
    }

    pub async fn list_order_events(&self, order_id: &str, limit: i64, offset: i64) -> Result<Value, DomainError> {
        Ok(json!({
            "dispatch_order_id": order_id,
            "items": self.collaboration_repo.list_events_by_order(order_id, limit.clamp(1, 200), offset.max(0)).await?,
            "limit": limit.clamp(1, 200),
            "offset": offset.max(0),
        }))
    }

    pub async fn get_order_record(&self, order_id: &str) -> Result<Option<Value>, DomainError> {
        let Some(order) = self.order_repo.find_by_id(order_id, true, None).await? else {
            return Ok(None);
        };

        let notification_receipt_summary = self.collaboration_repo.summarize_receipts_for_order(order_id).await?;
        let payload = dispatch_order_to_value_with_summary(&order, Some(&notification_receipt_summary));

        Ok(Some(payload))
    }

    pub async fn get_order_timeline(&self, order_id: &str, limit: i64) -> Result<Option<Value>, DomainError> {
        let Some(_) = self.order_repo.find_by_id(order_id, false, None).await? else {
            return Ok(None);
        };

        let items = self
            .collaboration_repo
            .list_events_by_order(order_id, limit.max(1), 0)
            .await?
            .into_iter()
            .filter_map(dispatch_event_to_timeline_item)
            .collect::<Vec<_>>();

        Ok(Some(json!({
            "dispatch_order_id": order_id,
            "items": items,
            "total": items.len(),
        })))
    }

    async fn resolve_group_for_flight(
        &self,
        flight_id: &str,
        user_id: Option<&str>,
    ) -> Result<Option<DispatchChatGroupSummary>, DomainError> {
        match user_id {
            Some(user_id) if !user_id.trim().is_empty() => {
                self.collaboration_repo
                    .get_group_for_user_by_flight(flight_id, user_id)
                    .await
            }
            _ => Ok(None),
        }
    }
}

impl DispatchCollaborationQueryService {
    async fn serialize_orders_with_receipt_summaries(
        &self,
        orders: &[fms_domain::models::dispatch::DispatchOrder],
    ) -> Result<Vec<Value>, DomainError> {
        let mut payload = Vec::with_capacity(orders.len());
        for order in orders {
            let summary = self.collaboration_repo.summarize_receipts_for_order(&order.id).await?;
            payload.push(dispatch_order_to_value_with_summary(order, Some(&summary)));
        }
        Ok(payload)
    }
}

fn filter_message_events(events: &[DispatchCollaborationEvent]) -> Vec<DispatchCollaborationEvent> {
    events
        .iter()
        .filter(|event| event.event_type == MESSAGE_EVENT_TYPE)
        .take(10)
        .cloned()
        .collect()
}

fn dispatch_event_to_timeline_item(event: DispatchCollaborationEvent) -> Option<Value> {
    let action = match event.event_type.as_str() {
        "order_created" => "created",
        "order_accepted" => "accepted",
        "order_started" => "started",
        "order_completed" => "completed",
        "order_cancelled" => "cancelled",
        "order_checked_in" => "checked_in",
        "order_issue_reported" => "issue_reported",
        "order_replanned" => "replanned",
        _ => return None,
    };

    let payload = event.payload.as_object().cloned().unwrap_or_default();
    let actor_username = payload
        .get("actor_username")
        .or_else(|| payload.get("username"))
        .and_then(Value::as_str)
        .map(str::to_string);

    let mut details = serde_json::Map::new();
    for (key, value) in payload {
        if key != "actor_username" && key != "username" {
            details.insert(key, value);
        }
    }

    Some(json!({
        "id": if event.event_id.trim().is_empty() {
            event.source_record_id
        } else {
            Some(event.event_id)
        },
        "action": action,
        "actor_id": event.actor_user_id,
        "actor_username": actor_username,
        "details": Value::Object(details),
        "created_at": event.occurred_at,
    }))
}

fn notification_to_value(notification: &Notification) -> Value {
    json!({
        "notification_id": notification.notification_id,
        "user_id": notification.user_id,
        "title": notification.title,
        "body": notification.body,
        "category": notification.category,
        "severity": notification.severity,
        "is_read": notification.is_read,
        "read_status": if notification.is_read { "read" } else { "unread" },
        "delivery_status": notification.delivery_status,
        "delivered_at": notification.delivered_at,
        "origin_type": notification.origin_type,
        "origin_label": if notification.origin_type.eq_ignore_ascii_case("workflow") { "流程" } else { "人工" },
        "receipt_required": notification.receipt_required,
        "receipt_group_id": notification.receipt_group_id,
        "ack_status": notification.ack_status,
        "ack_at": notification.ack_at,
        "ack_note": notification.ack_note,
        "related_entity_type": notification.related_entity_type,
        "related_entity_id": notification.related_entity_id,
        "created_at": notification.created_at,
        "read_at": notification.read_at,
    })
}
