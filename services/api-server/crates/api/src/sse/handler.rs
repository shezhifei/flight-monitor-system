//! Internal SSE HTTP helpers.
//!
//! Kept for internal reuse, but intentionally not mounted on the public
//! parity route surface.

use actix_web::{web, HttpRequest, HttpResponse};
use futures_core::Stream;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};

use super::hub::{SseHub, SseMessage, DEFAULT_CONNECTION_QUEUE_SIZE};
use crate::middleware::jwt::JwtAuth;
use crate::services::runtime_error_monitor::record_runtime_error_background;
use fms_runtime::spawn_tracked::spawn_tracked;

pub(crate) const SSE_CONNECTION_QUEUE_CAPACITY: usize = DEFAULT_CONNECTION_QUEUE_SIZE;

#[derive(Clone)]
struct StreamTracker {
    hub: Arc<SseHub>,
    client_id: String,
}

impl StreamTracker {
    fn new(hub: Arc<SseHub>, client_id: String) -> Self {
        Self { hub, client_id }
    }

    fn record_enqueue(&self, is_heartbeat: bool) {
        self.hub.record_connection_enqueue(&self.client_id, is_heartbeat);
    }

    fn rollback_enqueue(&self) {
        self.hub.rollback_connection_enqueue(&self.client_id);
    }

    fn record_delivery(&self) {
        self.hub.record_connection_delivery(&self.client_id);
    }

    fn record_lagged(&self, count: u64) {
        self.hub.record_connection_lagged(&self.client_id, count);
    }

    fn record_send_failure(&self) {
        self.hub.record_connection_send_failure(&self.client_id);
    }
}

/// SSE 流 wrapper — 实现 Stream<Item=Bytes>.
///
/// Uses an async relay task that does `recv()` on broadcast receivers
/// and a tracked mpsc relay queue to expose Python-compatible
/// per-connection runtime metrics.
pub(crate) struct SseStream {
    relay: mpsc::Receiver<actix_web::web::Bytes>,
    tracker: Option<StreamTracker>,
    on_drop: Option<Box<dyn Send + Sync + Fn()>>,
}

impl SseStream {
    #[allow(dead_code)]
    async fn new(hub: Arc<SseHub>, topics: Vec<String>, heartbeat_secs: u64) -> Self {
        let (tx, rx) = mpsc::channel::<actix_web::web::Bytes>(SSE_CONNECTION_QUEUE_CAPACITY);
        let receivers: Vec<(String, broadcast::Receiver<SseMessage>)> =
            futures::future::join_all(topics.into_iter().map(|topic| {
                let hub = hub.clone();
                async move { (topic.clone(), hub.subscribe(&topic).await) }
            }))
            .await;
        Self::spawn_tasks(receivers, tx, heartbeat_secs, None, hub);
        Self {
            relay: rx,
            tracker: None,
            on_drop: None,
        }
    }

    pub(crate) async fn new_with_initial(
        hub: Arc<SseHub>,
        topics: Vec<String>,
        heartbeat_secs: u64,
        initial_payload: String,
        tracker: Option<(Arc<SseHub>, String)>,
        on_drop: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        let (tx, rx) = mpsc::channel::<actix_web::web::Bytes>(SSE_CONNECTION_QUEUE_CAPACITY);
        let tracker = tracker.map(|(hub, client_id)| StreamTracker::new(hub, client_id));

        let tx_initial = tx.clone();
        let tracker_initial = tracker.clone();
        spawn_tracked("sse:initial_payload", async move {
            send_payload(
                &tx_initial,
                actix_web::web::Bytes::from(initial_payload),
                tracker_initial,
                false,
            )
            .await;
        });

        let receivers: Vec<(String, broadcast::Receiver<SseMessage>)> =
            futures::future::join_all(topics.into_iter().map(|topic| {
                let hub = hub.clone();
                async move { (topic.clone(), hub.subscribe(&topic).await) }
            }))
            .await;

        Self::spawn_tasks(receivers, tx, heartbeat_secs, tracker.clone(), hub);
        Self {
            relay: rx,
            tracker,
            on_drop: Some(Box::new(on_drop)),
        }
    }

    /// Spawns a SINGLE aggregated task instead of N+M tasks (1 heartbeat + M receivers).
    /// Uses tokio::select! to multiplex heartbeat timer and all broadcast receivers
    /// into one async loop, reducing coroutine overhead from O(N*M) to O(N).
    fn spawn_tasks(
        receivers: Vec<(String, broadcast::Receiver<SseMessage>)>,
        tx: mpsc::Sender<actix_web::web::Bytes>,
        heartbeat_secs: u64,
        tracker: Option<StreamTracker>,
        hub: Arc<SseHub>,
    ) {
        let tracker_forward = tracker.clone();
        spawn_tracked("sse:heartbeat_and_forward", async move {
            let mut heartbeat = tokio::time::interval(Duration::from_secs(heartbeat_secs));
            let _ = heartbeat.tick().await; // consume the immediate first tick

            // Keep topic alongside each stream so we can resubscribe on Lagged.
            let mut streams: Vec<(String, tokio_stream::wrappers::BroadcastStream<SseMessage>)> = receivers
                .into_iter()
                .map(|(topic, rx)| (topic, tokio_stream::wrappers::BroadcastStream::new(rx)))
                .collect();

            loop {
                tokio::select! {
                    // 1. Heartbeat tick
                    _ = heartbeat.tick() => {
                        let payload = format!(
                            "event: heartbeat\ndata: {{\"timestamp\":{}}}\n\n",
                            chrono::Utc::now().timestamp_millis() as f64 / 1000.0
                        );
                        if !send_payload(
                            &tx,
                            actix_web::web::Bytes::from(payload),
                            tracker_forward.clone(),
                            true,
                        )
                        .await
                        {
                            break;
                        }
                    }

                    // 2. Poll all broadcast receivers via select_all
                    result = futures::future::select_all(
                        streams.iter_mut().map(|(_topic, s)| Box::pin(tokio_stream::StreamExt::next(s)))
                    ) => {
                        let (item, index, _) = result;
                        match item {
                            Some(Ok(msg)) => {
                                let data: &str = msg.serialized_data.as_ref();
                                let event_name = msg.event.unwrap_or(msg.topic);
                                let payload = format!("event: {}\ndata: {}\n\n", event_name, data);
                                if !send_payload(
                                    &tx,
                                    actix_web::web::Bytes::from(payload),
                                    tracker_forward.clone(),
                                    false,
                                )
                                .await
                                {
                                    break;
                                }
                            }
                            Some(Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(count))) => {
                                if let Some(tracker) = &tracker_forward {
                                    tracker.record_lagged(count);
                                }
                                // Resubscribe to the same topic instead of permanently
                                // dropping it, so slow clients continue to receive new
                                // messages on high-frequency topics (e.g. anomaly_alerts).
                                let topic = &streams[index].0;
                                let new_rx = hub.subscribe(topic).await;
                                streams[index].1 = tokio_stream::wrappers::BroadcastStream::new(new_rx);
                            }
                            _ => {
                                // Stream closed (EOF or error) - remove it
                                streams.remove(index);
                                if streams.is_empty() {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        });
    }
}

async fn send_payload(
    tx: &mpsc::Sender<actix_web::web::Bytes>,
    payload: actix_web::web::Bytes,
    tracker: Option<StreamTracker>,
    is_heartbeat: bool,
) -> bool {
    if let Some(tracker) = &tracker {
        tracker.record_enqueue(is_heartbeat);
    }
    if tx.send(payload).await.is_ok() {
        return true;
    }
    if let Some(tracker) = &tracker {
        tracker.rollback_enqueue();
        tracker.record_send_failure();
    }
    false
}

impl Drop for SseStream {
    fn drop(&mut self) {
        if let Some(on_drop) = self.on_drop.take() {
            on_drop();
        }
    }
}

impl Stream for SseStream {
    type Item = Result<actix_web::web::Bytes, actix_web::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.relay.poll_recv(cx) {
            Poll::Ready(Some(bytes)) => {
                if let Some(tracker) = &self.tracker {
                    tracker.record_delivery();
                }
                Poll::Ready(Some(Ok(bytes)))
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Query 参数
#[derive(serde::Deserialize)]
pub struct SseQuery {
    /// 逗号分隔的主题列表
    topics: Option<String>,
}

/// GET /api/v2/sse/stream
async fn sse_stream(
    hub: web::Data<Arc<SseHub>>,
    query: web::Query<SseQuery>,
    auth: JwtAuth,
    _req: HttpRequest,
) -> HttpResponse {
    if !hub.try_acquire_connection() {
        record_runtime_error_background(crate::services::runtime_error_monitor::RuntimeErrorInput {
            error_type: crate::services::runtime_error_types::RuntimeErrorKind::SseConnectionLimitReached,
            message: "SSE connection limit reached".to_string(),
            severity: crate::services::runtime_error_types::Severity::Medium,
            category: crate::services::runtime_error_types::ErrorCategory::System,
            operation: Some("sse_stream".to_string()),
            details: None,
        });
        return HttpResponse::ServiceUnavailable().json(serde_json::json!({
            "success": false,
            "message": "SSE connection limit reached"
        }));
    }

    let mut topics = match normalize_topics(query.topics.as_deref()) {
        Ok(topics) => topics,
        Err(message) => {
            hub.release_connection();
            return HttpResponse::BadRequest().json(serde_json::json!({ "error": message }));
        }
    };

    let user_id = auth.0.sub.clone().unwrap_or_else(|| "anonymous".to_string());

    // Per-user topics are private: a caller may only subscribe to topics
    // whose suffix matches their own user id. Reject cross-user
    // subscriptions instead of silently filtering them.
    if let Some(offending) = find_foreign_user_topic(&topics, &user_id) {
        hub.release_connection();
        return HttpResponse::Forbidden().json(serde_json::json!({
            "error": format!("cannot subscribe to another user's topic: {offending}")
        }));
    }

    if user_id != "anonymous" {
        let notif_topic = format!("user_notifications_{user_id}");
        let chat_topic = format!("user_dispatch_chat_{user_id}");
        if !topics.contains(&notif_topic) {
            topics.push(notif_topic);
        }
        if !topics.contains(&chat_topic) {
            topics.push(chat_topic);
        }
    }

    static NEXT_CLIENT_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = NEXT_CLIENT_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let client_id = format!(
        "sse_universal_{}_{}_{}",
        user_id,
        chrono::Utc::now().timestamp_millis(),
        seq
    );

    let queue_capacity = hub.connection_queue_size();
    let heartbeat_interval = hub.heartbeat_interval();

    hub.register_connection(
        &client_id,
        (user_id != "anonymous").then_some(user_id.as_str()),
        &topics,
        queue_capacity,
    );

    let connected_payload = format!(
        "event: connected\nretry: 15000\ndata: {}\n\n",
        serde_json::json!({
            "client_id": client_id,
            "topics": topics,
            "heartbeat_interval": heartbeat_interval,
        })
    );

    let hub_arc = hub.get_ref().clone();
    let tracked_client_id = client_id.clone();
    let stream = SseStream::new_with_initial(
        hub_arc.clone(),
        topics,
        heartbeat_interval,
        connected_payload,
        Some((hub_arc.clone(), tracked_client_id.clone())),
        move || {
            hub_arc.unregister_connection(&tracked_client_id);
            hub_arc.release_connection();
        },
    )
    .await;

    HttpResponse::Ok()
        .content_type("text/event-stream; charset=utf-8")
        .insert_header(("Cache-Control", "no-cache"))
        .insert_header(("Connection", "keep-alive"))
        .insert_header(("X-Accel-Buffering", "no"))
        .streaming(stream)
}

/// GET /api/v2/sse/stats
#[allow(dead_code)]
async fn sse_stats(hub: web::Data<Arc<SseHub>>) -> HttpResponse {
    let stats = hub.stats().await;
    HttpResponse::Ok().json(stats)
}

/// 注册 SSE 路由
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(web::scope("/api/v2/sse").route("/stream", web::get().to(sse_stream)));
}

fn normalize_topics(raw: Option<&str>) -> Result<Vec<String>, String> {
    use super::hub::{PREFIX_TOPICS, STATIC_TOPICS};

    let Some(raw) = raw else {
        return Ok(STATIC_TOPICS.iter().map(|s| s.to_string()).collect());
    };

    let mut topics = Vec::new();
    for part in raw.split(',') {
        let topic = part.trim();
        if topic.is_empty() {
            continue;
        }

        let is_static = STATIC_TOPICS.iter().any(|t| *t == topic);
        let is_prefix = PREFIX_TOPICS.iter().any(|prefix| topic.starts_with(*prefix));

        if !is_static && !is_prefix {
            return Err(format!("unknown SSE topic: {topic}"));
        }

        if !topics.iter().any(|existing| existing == topic) {
            topics.push(topic.to_string());
        }
    }

    if topics.is_empty() {
        return Ok(STATIC_TOPICS.iter().map(|s| s.to_string()).collect());
    }

    Ok(topics)
}

/// Returns the first per-user topic whose owner suffix does not match
/// `user_id`, or `None` when all requested topics belong to the caller.
fn find_foreign_user_topic<'a>(topics: &'a [String], user_id: &str) -> Option<&'a String> {
    topics.iter().find(|topic| {
        super::hub::PREFIX_TOPICS
            .iter()
            .filter_map(|prefix| topic.strip_prefix(prefix))
            .any(|suffix| suffix != user_id)
    })
}

#[cfg(test)]
mod tests {
    use super::normalize_topics;

    #[test]
    fn accepts_all_static_topics() {
        let all = "flights,flight_status_changes,system_alerts,ai_execution,smart_monitor,anomaly_alerts,global_status,error_events,kpi_updated,business_cases";
        let result = normalize_topics(Some(all));
        assert!(result.is_ok(), "should accept all static topics: {:?}", result);
        assert_eq!(result.unwrap().len(), 10);
    }

    #[test]
    fn accepts_per_user_topic() {
        let result = normalize_topics(Some("user_notifications_42,user_dispatch_chat_42,user_ai_v2_99"));
        assert!(result.is_ok(), "should accept per-user topics: {:?}", result);
        assert_eq!(result.unwrap().len(), 3);
    }

    #[test]
    fn rejects_unknown_topic() {
        let result = normalize_topics(Some("flights,totally_bogus"));
        assert!(result.is_err());
    }

    #[test]
    fn empty_param_defaults_to_all_static() {
        let result = normalize_topics(None);
        assert_eq!(result.unwrap().len(), super::super::hub::STATIC_TOPICS.len());
    }

    #[test]
    fn deduplicates_topics() {
        let result = normalize_topics(Some("flights,flights,system_alerts"));
        assert_eq!(result.unwrap().len(), 2);
    }

    #[test]
    fn rejects_topics_owned_by_other_users() {
        use super::find_foreign_user_topic;

        let own = vec![
            "flights".to_string(),
            "user_notifications_42".to_string(),
            "user_dispatch_chat_42".to_string(),
            "user_ai_v2_42".to_string(),
        ];
        assert!(find_foreign_user_topic(&own, "42").is_none());

        let foreign = vec!["flights".to_string(), "user_notifications_77".to_string()];
        assert_eq!(
            find_foreign_user_topic(&foreign, "42").map(String::as_str),
            Some("user_notifications_77")
        );

        // Anonymous callers may not subscribe to any per-user topic.
        assert!(find_foreign_user_topic(&foreign, "anonymous").is_some());
    }
}
