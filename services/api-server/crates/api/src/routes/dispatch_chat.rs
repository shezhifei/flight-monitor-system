//! 派工聊天路由。

use actix_web::{web, Error as ActixError, HttpResponse};
use futures_core::Stream;
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::Value;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio_stream::wrappers::{errors::BroadcastStreamRecvError, BroadcastStream};

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use crate::sse::hub::{normalize_event_source_message, NormalizedSsePayload, SseHub, SseMessage};
use fms_application::services::dispatch_chat_service::{
    DispatchChatError, DispatchChatLifecycleChange, DispatchChatService,
};

#[derive(Debug, Deserialize)]
struct GroupListQuery {
    status: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct MessageListQuery {
    limit: Option<i64>,
    before_seq: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct SendMessageRequest {
    content: String,
    #[serde(default)]
    at_all: bool,
}

#[derive(Debug, Deserialize)]
struct MarkReadRequest {
    read_seq: Option<i64>,
}

/// GET /api/v2/dispatch/collaboration/groups
async fn list_groups(
    svc: web::Data<Arc<DispatchChatService>>,
    query: web::Query<GroupListQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let user_id = claims
        .0
        .sub
        .as_deref()
        .ok_or_else(|| ApiError::Unauthorized("未认证".into()))?;
    let status = query.status.as_deref().unwrap_or("active").trim().to_ascii_lowercase();
    if !matches!(status.as_str(), "active" | "archived" | "all") {
        return Err(ApiError::BadRequest("status must be active|archived|all".into()));
    }
    let payload = svc
        .list_user_groups(user_id, &status, query.limit.unwrap_or(50), query.offset.unwrap_or(0))
        .await
        .map_err(map_chat_error)?;
    Ok(HttpResponse::Ok().json(payload))
}

/// GET /api/v2/dispatch/collaboration/groups/by-flight/{flight_id}
async fn get_group_by_flight(
    svc: web::Data<Arc<DispatchChatService>>,
    hub: web::Data<Arc<SseHub>>,
    path: web::Path<String>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let flight_id = path.into_inner();
    let user_id = claims
        .0
        .sub
        .as_deref()
        .ok_or_else(|| ApiError::Unauthorized("未认证".into()))?;
    // Ops/admin may open any flight chat even without current assignment membership.
    let force_join = claims.0.is_admin.unwrap_or(false)
        || claims.0.permissions.iter().any(|p| {
            matches!(
                p.as_str(),
                "*" | "system:admin" | "system.ops_admin" | "dispatch:manage" | "dispatch.manage"
            )
        });

    let payload = svc
        .open_group_for_user_by_flight(&flight_id, user_id, force_join)
        .await
        .map_err(map_chat_error)?;
    match payload {
        Some(payload) => {
            // Best-effort lifecycle broadcast when group was just (re)synced.
            if let Some(change) = svc
                .refresh_group_lifecycle_for_flight(&flight_id)
                .await
                .map_err(map_chat_error)?
            {
                let _ = broadcast_lifecycle_change(hub.get_ref().clone(), svc.get_ref().clone(), change).await;
            }
            Ok(HttpResponse::Ok().json(payload))
        }
        None => Err(ApiError::NotFound("Group not found".into())),
    }
}

/// GET /api/v2/dispatch/collaboration/groups/{group_id}/messages
async fn list_messages(
    svc: web::Data<Arc<DispatchChatService>>,
    path: web::Path<String>,
    query: web::Query<MessageListQuery>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let user_id = claims
        .0
        .sub
        .as_deref()
        .ok_or_else(|| ApiError::Unauthorized("未认证".into()))?;
    let payload = svc
        .list_group_messages(&path.into_inner(), user_id, query.limit.unwrap_or(50), query.before_seq)
        .await
        .map_err(map_chat_error)?;
    Ok(HttpResponse::Ok().json(payload))
}

/// POST /api/v2/dispatch/collaboration/groups/{group_id}/messages
async fn send_message(
    svc: web::Data<Arc<DispatchChatService>>,
    hub: web::Data<Arc<SseHub>>,
    path: web::Path<String>,
    claims: JwtAuth,
    body: web::Json<SendMessageRequest>,
) -> Result<HttpResponse, ApiError> {
    let group_id = path.into_inner();
    let user_id = claims
        .0
        .sub
        .as_deref()
        .ok_or_else(|| ApiError::Unauthorized("未认证".into()))?;
    let payload = svc
        .send_message(&group_id, user_id, &body.content, body.at_all)
        .await
        .map_err(map_chat_error)?;
    broadcast_message_event(hub.get_ref().clone(), svc.get_ref().clone(), &group_id, &payload)
        .await
        .map_err(map_chat_error)?;
    Ok(HttpResponse::Ok().json(payload))
}

/// POST /api/v2/dispatch/collaboration/groups/{group_id}/read
async fn mark_read(
    svc: web::Data<Arc<DispatchChatService>>,
    hub: web::Data<Arc<SseHub>>,
    path: web::Path<String>,
    claims: JwtAuth,
    body: web::Json<MarkReadRequest>,
) -> Result<HttpResponse, ApiError> {
    let group_id = path.into_inner();
    let user_id = claims
        .0
        .sub
        .as_deref()
        .ok_or_else(|| ApiError::Unauthorized("未认证".into()))?;
    let payload = svc
        .mark_group_read(&group_id, user_id, body.read_seq)
        .await
        .map_err(map_chat_error)?;
    let last_read_seq = payload.get("last_read_seq").and_then(Value::as_i64).unwrap_or(0);
    broadcast_read_synced_event(
        hub.get_ref().clone(),
        svc.get_ref().clone(),
        &group_id,
        user_id,
        last_read_seq,
    )
    .await
    .map_err(map_chat_error)?;
    Ok(HttpResponse::Ok().json(payload))
}

/// GET /api/v2/dispatch/collaboration/stream
#[allow(dead_code)]
async fn chat_stream(
    hub: web::Data<Arc<SseHub>>,
    svc: web::Data<Arc<DispatchChatService>>,
    claims: JwtAuth,
) -> Result<HttpResponse, ApiError> {
    let user_id = claims
        .0
        .sub
        .as_deref()
        .ok_or_else(|| ApiError::Unauthorized("未认证".into()))?
        .to_string();
    let topic = user_dispatch_chat_topic(&user_id);
    let receiver = hub.subscribe(&topic).await;
    let initial = svc
        .build_initial_stream_payload(&user_id, 200)
        .await
        .map_err(map_chat_error)?;

    let stream = DispatchChatSseStream {
        receiver: BroadcastStream::new(receiver),
        heartbeat: tokio::time::interval(Duration::from_secs(15)),
        initial_payload: Some(initial),
    };

    Ok(HttpResponse::Ok()
        .content_type("text/event-stream")
        .insert_header(("Cache-Control", "no-cache"))
        .insert_header(("Connection", "keep-alive"))
        .insert_header(("X-Accel-Buffering", "no"))
        .streaming(stream))
}

pub(crate) fn configure_under_collaboration_scope(cfg: &mut web::ServiceConfig) {
    cfg.route("/groups", web::get().to(list_groups))
        .route("/groups/by-flight/{flight_id}", web::get().to(get_group_by_flight))
        .route("/groups/{group_id}/messages", web::get().to(list_messages))
        .route("/groups/{group_id}/messages", web::post().to(send_message))
        .route("/groups/{group_id}/read", web::post().to(mark_read));
}

/// 注册派工聊天路由 (6 endpoints)
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(web::scope("/api/v2/dispatch/collaboration").configure(configure_under_collaboration_scope));
}

fn map_chat_error(error: DispatchChatError) -> ApiError {
    match error {
        DispatchChatError::Forbidden(message) => ApiError::Forbidden(message),
        DispatchChatError::NotFound(message) => ApiError::NotFound(message),
        DispatchChatError::Archived(message) => ApiError::Conflict(message),
        DispatchChatError::Validation(message) => ApiError::BadRequest(message),
        DispatchChatError::Domain(error) => error.into(),
    }
}

pub(crate) async fn broadcast_message_event(
    hub: Arc<SseHub>,
    svc: Arc<DispatchChatService>,
    group_id: &str,
    message: &fms_domain::models::dispatch_collaboration::DispatchChatMessage,
) -> Result<(), DispatchChatError> {
    let events = svc.build_message_stream_events(group_id, message).await?;
    for (user_id, payload) in events {
        let topic = user_dispatch_chat_topic(&user_id);
        let _ = hub.broadcast_event(&topic, Some("chat_message"), payload).await;
    }
    Ok(())
}

pub(crate) async fn broadcast_read_synced_event(
    hub: Arc<SseHub>,
    svc: Arc<DispatchChatService>,
    group_id: &str,
    user_id: &str,
    last_read_seq: i64,
) -> Result<(), DispatchChatError> {
    let payload = svc
        .build_read_synced_stream_event(group_id, user_id, last_read_seq)
        .await?;
    let topic = user_dispatch_chat_topic(user_id);
    let _ = hub.broadcast_event(&topic, Some("chat_read_synced"), payload).await;
    Ok(())
}

pub(crate) async fn broadcast_group_upserted_event(
    hub: Arc<SseHub>,
    svc: Arc<DispatchChatService>,
    group_id: &str,
) -> Result<(), DispatchChatError> {
    let events = svc.build_group_upserted_stream_events(group_id).await?;
    for (user_id, payload) in events {
        let topic = user_dispatch_chat_topic(&user_id);
        let _ = hub.broadcast_event(&topic, Some("chat_group_upserted"), payload).await;
    }
    Ok(())
}

pub(crate) async fn broadcast_group_archived_event(
    hub: Arc<SseHub>,
    svc: Arc<DispatchChatService>,
    group_id: &str,
    archived_at: chrono::DateTime<chrono::Utc>,
) -> Result<(), DispatchChatError> {
    let events = svc.build_group_archived_stream_events(group_id, archived_at).await?;
    for (user_id, payload) in events {
        let topic = user_dispatch_chat_topic(&user_id);
        let _ = hub.broadcast_event(&topic, Some("chat_group_archived"), payload).await;
    }
    Ok(())
}

async fn broadcast_lifecycle_change(
    hub: Arc<SseHub>,
    svc: Arc<DispatchChatService>,
    change: DispatchChatLifecycleChange,
) -> Result<(), DispatchChatError> {
    match change {
        DispatchChatLifecycleChange::Upserted { group_id } => broadcast_group_upserted_event(hub, svc, &group_id).await,
        DispatchChatLifecycleChange::Archived { group_id, archived_at } => {
            broadcast_group_archived_event(hub, svc, &group_id, archived_at).await
        }
    }
}

fn user_dispatch_chat_topic(user_id: &str) -> String {
    format!("user_dispatch_chat_{}", user_id.trim())
}

#[allow(dead_code)]
fn normalize_dispatch_chat_message(message: &SseMessage) -> Option<NormalizedSsePayload> {
    let mut normalized = normalize_event_source_message(message, &message.topic)?;

    let Ok(parsed) = serde_json::from_str::<Value>(&normalized.data) else {
        return Some(normalized);
    };
    let Some(payload) = parsed.as_object() else {
        return Some(normalized);
    };

    let mapped_event = payload
        .get("type")
        .and_then(Value::as_str)
        .map(str::trim)
        .map(|value| match value {
            "dispatch_chat_message" => Some("chat_message"),
            "dispatch_chat_group_upserted" => Some("chat_group_upserted"),
            "dispatch_chat_group_archived" => Some("chat_group_archived"),
            "dispatch_chat_read_synced" => Some("chat_read_synced"),
            _ => None,
        })
        .flatten();

    if let Some(event) = mapped_event {
        normalized.event = event.to_string();
    }

    Some(normalized)
}

#[allow(dead_code)]
struct DispatchChatSseStream {
    receiver: BroadcastStream<SseMessage>,
    heartbeat: tokio::time::Interval,
    initial_payload: Option<Value>,
}

impl Stream for DispatchChatSseStream {
    type Item = Result<actix_web::web::Bytes, ActixError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(initial_payload) = self.initial_payload.take() {
            let data = serde_json::to_string(&initial_payload).unwrap_or_else(|_| "{}".to_string());
            let payload = format!("event: initial\ndata: {}\n\n", data);
            return Poll::Ready(Some(Ok(actix_web::web::Bytes::from(payload))));
        }

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
                    let Some(normalized) = normalize_dispatch_chat_message(&message) else {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};
    use tokio::sync::broadcast;

    struct CountWake {
        count: AtomicUsize,
    }

    impl Wake for CountWake {
        fn wake(self: Arc<Self>) {
            self.count.fetch_add(1, Ordering::SeqCst);
        }

        fn wake_by_ref(self: &Arc<Self>) {
            self.count.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[actix_web::test]
    async fn dispatch_chat_stream_registers_broadcast_waker_for_realtime_delivery() {
        let (sender, receiver) = broadcast::channel(8);
        let mut heartbeat = tokio::time::interval(Duration::from_secs(60));
        heartbeat.tick().await;
        let mut stream = DispatchChatSseStream {
            receiver: BroadcastStream::new(receiver),
            heartbeat,
            initial_payload: None,
        };
        let wake = Arc::new(CountWake {
            count: AtomicUsize::new(0),
        });
        let waker = Waker::from(wake.clone());
        let mut cx = Context::from_waker(&waker);

        assert!(matches!(Pin::new(&mut stream).poll_next(&mut cx), Poll::Pending));

        sender
            .send(SseMessage {
                topic: "user_dispatch_chat_user-1".to_string(),
                event: Some("chat_message".to_string()),
                serialized_data: Arc::new(json!({"type": "dispatch_chat_message"}).to_string()),
            })
            .expect("message should send");
        tokio::task::yield_now().await;

        assert!(
            wake.count.load(Ordering::SeqCst) > 0,
            "broadcast send should wake a pending dispatch chat stream immediately"
        );
    }
}
