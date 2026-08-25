//! 通知路由
//!
//! 对齐 Python `notification_routes.py`。

pub(crate) use actix_web::{web, HttpResponse};
pub(crate) use futures_core::Stream;
pub(crate) use futures_util::StreamExt;
pub(crate) use serde::Deserialize;
pub(crate) use serde_json::json;
pub(crate) use std::pin::Pin;
pub(crate) use std::sync::Arc;
pub(crate) use std::task::{Context, Poll};
pub(crate) use std::time::Duration;
pub(crate) use tokio_stream::wrappers::{errors::BroadcastStreamRecvError, BroadcastStream};

pub(crate) static NULL_VALUE: serde_json::Value = serde_json::Value::Null;
pub(crate) use crate::error::ApiError;
pub(crate) use crate::middleware::jwt::JwtAuth;
pub(crate) use crate::middleware::permissions::PermissionCheck;
pub(crate) use crate::sse::hub::{normalize_event_source_message, SseHub, SseMessage};
pub(crate) use crate::types::ConcreteNotificationService;
pub(crate) use fms_application::services::authorization_service::PermissionCatalog;
pub(crate) use fms_application::services::notification_service::{
    CollaborationEventRecorder, DispatchBatchNotificationCreate, NoCollaborationEvents,
    NotificationCollaborationEvents, NotificationDeliveryPublisher, NotificationMetricsRecorder,
    NotificationPreferenceUpdate, NotificationReceiptGroupSync, NotificationService,
};
pub(crate) use fms_application::services::online_status_service::OnlineStatusService;
#[allow(dead_code)]
pub(crate) struct NotificationSseStream {
    pub(crate) receiver: BroadcastStream<SseMessage>,
    pub(crate) heartbeat: tokio::time::Interval,
}

#[derive(Deserialize)]
pub(crate) struct NotifListQuery {
    pub(crate) limit: Option<i64>,
    pub(crate) offset: Option<i64>,
    pub(crate) unread_only: Option<bool>,
}

#[derive(Deserialize)]
pub(crate) struct NotificationAckRequest {
    pub(crate) action: String,
    pub(crate) note: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct DispatchOnlineUsersQuery {
    pub(crate) keyword: Option<String>,
    pub(crate) department: Option<String>,
    pub(crate) job_title: Option<String>,
    pub(crate) limit: Option<i64>,
}

#[derive(Deserialize)]
pub(crate) struct SentReceiptGroupsQuery {
    pub(crate) limit: Option<i64>,
    pub(crate) offset: Option<i64>,
}

#[derive(Deserialize)]
pub(crate) struct DispatchManualNotificationRequest {
    pub(crate) recipient_user_ids: Vec<String>,
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) severity: Option<String>,
    pub(crate) flight_id: Option<String>,
    pub(crate) flight_no: Option<String>,
    pub(crate) receipt_required: Option<bool>,
}

pub(crate) async fn ack_notification_inner<NR, PR, CE, DP, MR, RS>(
    svc: &NotificationService<NR, PR, CE, DP, MR, RS>,
    path: web::Path<String>,
    body: web::Json<NotificationAckRequest>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError>
where
    NR: fms_domain::ports::notification_repository::NotificationRepository + Send + Sync + ?Sized,
    PR: fms_domain::ports::notification_repository::NotificationPreferenceRepository + Send + Sync + ?Sized,
    CE: fms_application::services::notification_service::NotificationCollaborationEvents + Send + Sync + ?Sized,
    DP: NotificationDeliveryPublisher + Send + Sync + ?Sized,
    MR: NotificationMetricsRecorder + Send + Sync + ?Sized,
    RS: NotificationReceiptGroupSync + Send + Sync + ?Sized,
{
    let user_id = current_user_id(&claims)?;
    let id = path.into_inner();
    let payload = body.into_inner();
    let action = payload.action.trim().to_ascii_lowercase();
    if action != "acknowledged" && action != "rejected" {
        return Err(ApiError::BadRequest(
            "invalid notification acknowledgement request".into(),
        ));
    }
    let note = payload.note.as_deref().map(str::trim);
    if action == "rejected" && note.unwrap_or("").is_empty() {
        return Err(ApiError::BadRequest(
            "invalid notification acknowledgement request".into(),
        ));
    }
    let existing = svc
        .get_notification(&id, user_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Notification not found".into()))?;
    if existing.ack_status != "pending" {
        return Err(ApiError::Conflict("Notification already acknowledged".into()));
    }
    let updated = svc
        .acknowledge(&id, user_id, &action, note, Some(current_username(&claims)))
        .await?
        .ok_or_else(|| ApiError::Internal("Failed to acknowledge notification".into()))?;
    Ok(ok_resp(
        if action == "acknowledged" {
            "Notification acknowledged"
        } else {
            "Notification rejected"
        },
        notification_receipt_value(&updated),
    ))
}

pub(crate) async fn get_receipt_group_inner<NR, PR, CE, DP, MR, RS>(
    svc: &NotificationService<NR, PR, CE, DP, MR, RS>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError>
where
    NR: fms_domain::ports::notification_repository::NotificationRepository + Send + Sync + ?Sized,
    PR: fms_domain::ports::notification_repository::NotificationPreferenceRepository + Send + Sync + ?Sized,
    CE: fms_application::services::notification_service::NotificationCollaborationEvents + Send + Sync + ?Sized,
    DP: NotificationDeliveryPublisher + Send + Sync + ?Sized,
    MR: NotificationMetricsRecorder + Send + Sync + ?Sized,
    RS: NotificationReceiptGroupSync + Send + Sync + ?Sized,
{
    let current_user_id = current_user_id(&claims)?;
    let can_view = claims.has_permission(PermissionCatalog::NOTIFICATION_RECEIPT_READ)
        || claims.has_permission(PermissionCatalog::NOTIFICATION_READ);
    let receipt_group_id = path.into_inner();
    let payload = svc
        .get_receipt_group(&receipt_group_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Notification receipt group not found".into()))?;
    let sender_user_id = payload
        .get("sender_user_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .unwrap_or_default();
    if !can_view && sender_user_id != current_user_id {
        return Err(ApiError::Forbidden(format!(
            "缺少权限: {}",
            PermissionCatalog::NOTIFICATION_RECEIPT_READ
        )));
    }
    Ok(HttpResponse::Ok().json(payload))
}

pub(crate) fn ok_resp(message: impl Into<String>, data: impl serde::Serialize) -> HttpResponse {
    HttpResponse::Ok().json(json!({ "success": true, "data": data, "message": message.into() }))
}

pub(crate) fn ensure_dispatch_view_permission(claims: &JwtAuth) -> Result<(), ApiError> {
    if claims.has_permission(PermissionCatalog::NOTIFICATION_READ)
        || claims.has_permission(PermissionCatalog::NOTIFICATION_RECEIPT_READ)
    {
        Ok(())
    } else {
        Err(ApiError::Forbidden(format!(
            "缺少权限: {}",
            PermissionCatalog::NOTIFICATION_READ
        )))
    }
}

pub(crate) fn current_user_id(claims: &JwtAuth) -> Result<&str, ApiError> {
    claims
        .0
        .sub
        .as_deref()
        .ok_or_else(|| ApiError::Unauthorized("未认证".into()))
}

pub(crate) fn current_username(claims: &JwtAuth) -> &str {
    claims
        .0
        .username
        .as_deref()
        .or(claims.0.sub.as_deref())
        .unwrap_or("unknown")
}

pub(crate) fn notification_receipt_value(item: &fms_domain::models::notification::Notification) -> serde_json::Value {
    let recipient_username = item
        .recipient_username_snapshot
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&item.user_id);
    json!({
        "receipt_id": item.notification_id,
        "notification_id": item.notification_id,
        "user_id": item.user_id,
        "recipient_user_id": item.user_id,
        "recipient_username": recipient_username,
        "recipient_display_name": item.recipient_display_name_snapshot,
        "recipient_department": item.recipient_department_snapshot,
        "recipient_job_title": item.recipient_job_title_snapshot,
        "title": item.title,
        "severity": item.severity,
        "origin_type": item.origin_type,
        "origin_label": if item.origin_type.eq_ignore_ascii_case("workflow") { "流程" } else { "人工" },
        "receipt_group_id": item.receipt_group_id,
        "delivery_status": item.delivery_status,
        "delivered_at": item.delivered_at,
        "read_status": if item.is_read { "read" } else { "unread" },
        "read_at": item.read_at,
        "ack_status": item.ack_status,
        "ack_at": item.ack_at,
        "ack_note": item.ack_note,
        "sender_user_id": item.sender_user_id,
        "sender_username": item.sender_username_snapshot,
        "updated_at": item.ack_at.or(item.read_at).or(item.delivered_at).unwrap_or(item.created_at),
    })
}

impl Stream for NotificationSseStream {
    type Item = Result<actix_web::web::Bytes, actix_web::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.heartbeat.poll_tick(cx).is_ready() {
            let payload = format!(
                "event: heartbeat\ndata: {{\"timestamp\":\"{}\"}}\n\n",
                chrono::Utc::now().to_rfc3339()
            );
            return Poll::Ready(Some(Ok(actix_web::web::Bytes::from(payload))));
        }

        loop {
            match self.receiver.poll_next_unpin(cx) {
                Poll::Ready(Some(Ok(message))) => {
                    let Some(normalized) = normalize_event_source_message(&message, &message.topic) else {
                        continue;
                    };
                    let payload = format!("event: {}\ndata: {}\n\n", normalized.event, normalized.data);
                    return Poll::Ready(Some(Ok(actix_web::web::Bytes::from(payload))));
                }
                Poll::Ready(Some(Err(BroadcastStreamRecvError::Lagged(_)))) => continue,
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

/// GET /api/v2/notifications/stream
#[allow(dead_code)]
pub(crate) async fn notification_stream(
    svc: web::Data<Arc<ConcreteNotificationService>>,
    hub: web::Data<Arc<SseHub>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let user_id = current_user_id(&claims)?;
    let initial_items = svc.list_notifications(user_id, false, 50, 0).await?;
    let unread_count = svc.get_unread_count(user_id).await?;
    let initial_payload = format!(
        "event: initial\ndata: {}\n\n",
        json!({
            "type": "initial_data",
            "items": initial_items,
            "unread_count": unread_count,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        })
    );
    let stream = futures_util::stream::iter(vec![Ok::<actix_web::web::Bytes, actix_web::Error>(
        actix_web::web::Bytes::from(initial_payload),
    )])
    .chain(NotificationSseStream {
        receiver: BroadcastStream::new(hub.subscribe(&format!("user_notifications_{user_id}")).await),
        heartbeat: tokio::time::interval(Duration::from_secs(15)),
    });

    Ok(HttpResponse::Ok()
        .content_type("text/event-stream")
        .insert_header(("Cache-Control", "no-cache"))
        .insert_header(("Connection", "keep-alive"))
        .insert_header(("X-Accel-Buffering", "no"))
        .streaming(stream))
}
