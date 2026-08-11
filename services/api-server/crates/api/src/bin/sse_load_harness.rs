use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use actix_web::{web, App, HttpResponse, HttpServer};
use fms_api::sse::hub::{SseHub, SseMessage};
use futures_core::Stream;
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::json;
use tokio_stream::wrappers::{errors::BroadcastStreamRecvError, BroadcastStream};

static NEXT_CLIENT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Deserialize)]
struct SseQuery {
    topic: Option<String>,
    user_id: Option<String>,
}

#[derive(Deserialize)]
struct BroadcastQuery {
    topic: Option<String>,
    event: Option<String>,
    count: Option<usize>,
    payload_bytes: Option<usize>,
}

struct HarnessSseStream {
    hub: Arc<SseHub>,
    client_id: String,
    receiver: BroadcastStream<SseMessage>,
    heartbeat: tokio::time::Interval,
    initial_payload: Option<String>,
}

impl Drop for HarnessSseStream {
    fn drop(&mut self) {
        self.hub.unregister_connection(&self.client_id);
        self.hub.release_connection();
    }
}

impl Stream for HarnessSseStream {
    type Item = Result<web::Bytes, actix_web::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(initial_payload) = self.initial_payload.take() {
            return Poll::Ready(Some(Ok(web::Bytes::from(initial_payload))));
        }

        if self.heartbeat.poll_tick(cx).is_ready() {
            let payload = format!(
                "event: heartbeat\ndata: {{\"timestamp_ms\":{}}}\n\n",
                chrono::Utc::now().timestamp_millis()
            );
            return Poll::Ready(Some(Ok(web::Bytes::from(payload))));
        }

        loop {
            match self.receiver.poll_next_unpin(cx) {
                Poll::Ready(Some(Ok(message))) => {
                    let event = message.event.as_deref().unwrap_or(message.topic.as_str());
                    let payload = format!("event: {event}\ndata: {}\n\n", message.serialized_data);
                    return Poll::Ready(Some(Ok(web::Bytes::from(payload))));
                }
                Poll::Ready(Some(Err(BroadcastStreamRecvError::Lagged(_)))) => continue,
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

async fn health() -> HttpResponse {
    HttpResponse::Ok().json(json!({"ok": true}))
}

async fn stats(hub: web::Data<Arc<SseHub>>) -> HttpResponse {
    HttpResponse::Ok().json(hub.stats().await)
}

async fn summary(hub: web::Data<Arc<SseHub>>) -> HttpResponse {
    let stats = hub.stats().await;
    HttpResponse::Ok().json(json!({
        "active_connections": stats.active_connections,
        "lifetime_connections": stats.lifetime_connections,
        "messages_sent": stats.messages_sent,
        "messages_failed": stats.messages_failed,
        "messages_dropped": stats.messages_dropped,
        "topics": stats.topics,
        "topic_count": stats.topic_count,
        "heartbeat_interval": stats.heartbeat_interval,
        "max_connections": stats.max_connections,
        "connection_queue_size": stats.connection_queue_size,
        "lagged_total": stats.lagged_total
    }))
}

async fn sse(hub: web::Data<Arc<SseHub>>, query: web::Query<SseQuery>) -> HttpResponse {
    if !hub.try_acquire_connection() {
        return HttpResponse::ServiceUnavailable().json(json!({
            "success": false,
            "message": "SSE connection limit reached"
        }));
    }

    let topic = query.topic.as_deref().unwrap_or("flights").trim();
    if !SseHub::is_allowed_topic(topic) {
        hub.release_connection();
        return HttpResponse::BadRequest().json(json!({
            "success": false,
            "message": format!("unknown SSE topic: {topic}")
        }));
    }

    let user_id = query.user_id.as_deref().filter(|value| !value.is_empty());
    let topic_string = topic.to_string();
    let topics = vec![topic_string.clone()];
    let client_id = format!("load-{}", NEXT_CLIENT_ID.fetch_add(1, Ordering::Relaxed));
    hub.register_connection(&client_id, user_id, &topics, hub.connection_queue_size());
    let receiver = hub.subscribe(&topic_string).await;
    let heartbeat_secs = hub.heartbeat_interval();
    let initial_payload = format!(
        "event: connected\ndata: {}\n\n",
        json!({
            "client_id": client_id,
            "topic": topic_string,
            "heartbeat_interval": heartbeat_secs
        })
    );

    let stream = HarnessSseStream {
        hub: hub.get_ref().clone(),
        client_id,
        receiver: BroadcastStream::new(receiver),
        heartbeat: tokio::time::interval(Duration::from_secs(heartbeat_secs)),
        initial_payload: Some(initial_payload),
    };

    HttpResponse::Ok()
        .content_type("text/event-stream; charset=utf-8")
        .insert_header(("Cache-Control", "no-cache"))
        .insert_header(("Connection", "keep-alive"))
        .insert_header(("X-Accel-Buffering", "no"))
        .streaming(stream)
}

async fn broadcast(hub: web::Data<Arc<SseHub>>, query: web::Query<BroadcastQuery>) -> HttpResponse {
    let topic = query.topic.as_deref().unwrap_or("flights");
    let event = query.event.as_deref().unwrap_or("load_event");
    let count = query.count.unwrap_or(1).max(1);
    let payload_bytes = query.payload_bytes.unwrap_or(64).max(1);
    let payload = "x".repeat(payload_bytes);

    let mut delivered = 0usize;
    for index in 0..count {
        delivered += hub
            .broadcast_event(
                topic,
                Some(event),
                json!({
                    "index": index,
                    "payload": payload,
                    "timestamp_ms": chrono::Utc::now().timestamp_millis()
                }),
            )
            .await;
    }

    HttpResponse::Ok().json(json!({
        "topic": topic,
        "event": event,
        "count": count,
        "delivered": delivered
    }))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let host = std::env::var("SSE_LOAD_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = std::env::var("SSE_LOAD_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(19080);
    let hub_capacity = std::env::var("SSE_HUB_CAPACITY")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(1024);
    let workers = std::env::var("SSE_LOAD_WORKERS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or_else(|| std::thread::available_parallelism().map(usize::from).unwrap_or(4));

    let hub = SseHub::new(hub_capacity);
    eprintln!("sse-load-harness listening on http://{host}:{port} workers={workers} hub_capacity={hub_capacity}");

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(hub.clone()))
            .route("/health", web::get().to(health))
            .route("/stats", web::get().to(stats))
            .route("/summary", web::get().to(summary))
            .route("/sse", web::get().to(sse))
            .route("/broadcast", web::post().to(broadcast))
            .route("/broadcast", web::get().to(broadcast))
    })
    .workers(workers)
    .bind((host, port))?
    .run()
    .await
}
