//! SSE (Server-Sent Events) 广播中心
//!
//! 对应 Python `AsyncSSEHub`；使用 tokio broadcast channel 实现
//! 主题级广播，并补充 Python 兼容的连接运行态快照。

use async_trait::async_trait;
use dashmap::DashMap;
use fms_domain::broadcaster::Broadcaster;
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::broadcast;

pub const STATIC_TOPICS: &[&str] = &[
    "flights",
    "flight_status_changes",
    "anomaly_alerts",
    "global_status",
    "kpi_updated",
    "error_events",
    "system_alerts",
    "ai_execution",
    "smart_monitor",
    "business_cases",
    "dispatch_alerts",
];
pub const PREFIX_TOPICS: &[&str] = &["user_dispatch_chat_", "user_notifications_", "user_ai_v2_"];
pub const DEFAULT_HEARTBEAT_INTERVAL_SECS: u64 = 15;
pub const DEFAULT_CONNECTION_QUEUE_SIZE: usize = 64;
pub const DEFAULT_CLEANUP_INTERVAL_SECS: u64 = 30;
pub const DEFAULT_HEARTBEAT_TIMEOUT_SECS: u64 = 45;
pub const DEFAULT_QUEUE_FULL_DISCONNECT_SECS: u64 = 10;
pub const DEFAULT_MAX_CONNECTIONS: usize = 1024;

#[derive(Debug, Clone)]
pub struct NormalizedSsePayload {
    pub event: String,
    pub data: String,
}

/// 单条 SSE 消息 — 携带预序列化的 JSON 文本，避免广播风暴中 N 次重复序列化。
#[derive(Debug, Clone, Serialize)]
pub struct SseMessage {
    pub topic: String,
    pub event: Option<String>,
    /// 预序列化的 data JSON 字符串；广播方只序列化一次，所有消费者零拷贝共享。
    #[serde(skip)]
    pub serialized_data: Arc<String>,
}

#[derive(Debug)]
struct RuntimeConnectionState {
    client_id: String,
    user_id: Option<String>,
    subscriptions: Vec<String>,
    connected_at_ms: i64,
    last_heartbeat_ms: AtomicI64,
    last_message_ms: AtomicI64,
    queue_size: AtomicUsize,
    queue_maxsize: usize,
    is_active: AtomicBool,
    dropped_messages: AtomicU64,
}

impl RuntimeConnectionState {
    fn new(
        client_id: impl Into<String>,
        user_id: Option<&str>,
        subscriptions: &[String],
        queue_maxsize: usize,
    ) -> Self {
        let now_ms = now_ms();
        let mut normalized_subscriptions = subscriptions.to_vec();
        normalized_subscriptions.sort();
        normalized_subscriptions.dedup();
        Self {
            client_id: client_id.into(),
            user_id: user_id.map(str::to_string),
            subscriptions: normalized_subscriptions,
            connected_at_ms: now_ms,
            last_heartbeat_ms: AtomicI64::new(now_ms),
            last_message_ms: AtomicI64::new(now_ms),
            queue_size: AtomicUsize::new(0),
            queue_maxsize: queue_maxsize.max(1),
            is_active: AtomicBool::new(true),
            dropped_messages: AtomicU64::new(0),
        }
    }

    fn snapshot(&self, current_time_ms: i64) -> SseConnectionDetail {
        let last_heartbeat_ms = self.last_heartbeat_ms.load(Ordering::Relaxed);
        let last_message_ms = self.last_message_ms.load(Ordering::Relaxed);
        let queue_size = self.queue_size.load(Ordering::Relaxed);
        SseConnectionDetail {
            client_id: self.client_id.clone(),
            user_id: self.user_id.clone(),
            is_active: self.is_active.load(Ordering::Relaxed),
            connected_at: ms_to_unix_seconds(self.connected_at_ms),
            last_message_at: ms_to_unix_seconds(last_message_ms),
            last_heartbeat: ms_to_unix_seconds(last_heartbeat_ms),
            time_since_heartbeat: ((current_time_ms - last_heartbeat_ms).max(0) as f64) / 1000.0,
            queue_size,
            queue_maxsize: self.queue_maxsize,
            queue_full: queue_size >= self.queue_maxsize,
            dropped_messages: self.dropped_messages.load(Ordering::Relaxed),
            subscriptions: self.subscriptions.clone(),
        }
    }

    fn mark_enqueued(&self, is_heartbeat: bool) {
        self.queue_size.fetch_add(1, Ordering::Relaxed);
        let now = now_ms();
        self.last_message_ms.store(now, Ordering::Relaxed);
        if is_heartbeat {
            self.last_heartbeat_ms.store(now, Ordering::Relaxed);
        }
    }

    fn rollback_enqueue(&self) {
        let _ = self
            .queue_size
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                Some(value.saturating_sub(1))
            });
    }

    fn mark_delivered(&self) {
        let _ = self
            .queue_size
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                Some(value.saturating_sub(1))
            });
    }

    fn mark_lagged(&self, count: u64) {
        self.dropped_messages.fetch_add(count, Ordering::Relaxed);
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct SseConnectionDetail {
    pub client_id: String,
    pub user_id: Option<String>,
    pub is_active: bool,
    pub connected_at: f64,
    pub last_message_at: f64,
    pub last_heartbeat: f64,
    pub time_since_heartbeat: f64,
    pub queue_size: usize,
    pub queue_maxsize: usize,
    pub queue_full: bool,
    pub dropped_messages: u64,
    pub subscriptions: Vec<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct SseConnectionBreakdown {
    pub connected: usize,
    pub inactive: usize,
}

/// SSE 广播中心 — 管理主题订阅与 message 分发
pub struct SseHub {
    topics: DashMap<String, broadcast::Sender<SseMessage>>,
    topic_subscriber_counts: DashMap<String, AtomicUsize>,
    runtime_connections: DashMap<String, Arc<RuntimeConnectionState>>,
    capacity: usize,
    max_connections: usize,
    heartbeat_interval: u64,
    connection_queue_size: usize,
    cleanup_interval_seconds: u64,
    heartbeat_timeout_seconds: u64,
    queue_full_disconnect_seconds: u64,
    last_cleanup_ms: AtomicI64,
    active_connections: AtomicUsize,
    total_messages_sent: AtomicU64,
    messages_failed: AtomicU64,
    lagged_total: AtomicU64,
    lifetime_connections: AtomicU64,
}

impl SseHub {
    pub fn new(capacity: usize) -> Arc<Self> {
        let max_connections = std::env::var("SSE_MAX_CONNECTIONS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_MAX_CONNECTIONS);
        let heartbeat_interval = std::env::var("SSE_HEARTBEAT_INTERVAL_SECONDS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_HEARTBEAT_INTERVAL_SECS);
        let connection_queue_size = std::env::var("SSE_CONNECTION_QUEUE_SIZE")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_CONNECTION_QUEUE_SIZE);
        let cleanup_interval_seconds = std::env::var("SSE_CLEANUP_INTERVAL_SECONDS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_CLEANUP_INTERVAL_SECS);
        let heartbeat_timeout_seconds = std::env::var("SSE_HEARTBEAT_TIMEOUT_SECONDS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_HEARTBEAT_TIMEOUT_SECS);
        let queue_full_disconnect_seconds = std::env::var("SSE_QUEUE_FULL_DISCONNECT_SECONDS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_QUEUE_FULL_DISCONNECT_SECS);
        Arc::new(Self {
            topics: DashMap::new(),
            topic_subscriber_counts: DashMap::new(),
            runtime_connections: DashMap::new(),
            capacity,
            max_connections,
            heartbeat_interval,
            connection_queue_size,
            cleanup_interval_seconds,
            heartbeat_timeout_seconds,
            queue_full_disconnect_seconds,
            last_cleanup_ms: AtomicI64::new(now_ms()),
            active_connections: AtomicUsize::new(0),
            total_messages_sent: AtomicU64::new(0),
            messages_failed: AtomicU64::new(0),
            lagged_total: AtomicU64::new(0),
            lifetime_connections: AtomicU64::new(0),
        })
    }

    pub fn try_acquire_connection(&self) -> bool {
        let acquired = if self.max_connections == 0 {
            self.active_connections.fetch_add(1, Ordering::Release);
            true
        } else {
            loop {
                let current = self.active_connections.load(Ordering::Acquire);
                if current >= self.max_connections {
                    return false;
                }
                match self.active_connections.compare_exchange_weak(
                    current,
                    current + 1,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => break true,
                    Err(_) => continue,
                }
            }
        };
        if acquired {
            metrics::gauge!("fms_sse_connections").set(self.active_connections.load(Ordering::Relaxed) as f64);
        }
        acquired
    }

    pub fn release_connection(&self) {
        self.active_connections.fetch_sub(1, Ordering::Release);
        metrics::gauge!("fms_sse_connections").set(self.active_connections.load(Ordering::Relaxed) as f64);
    }

    pub fn connection_queue_size(&self) -> usize {
        self.connection_queue_size
    }

    pub fn heartbeat_interval(&self) -> u64 {
        self.heartbeat_interval
    }

    pub fn register_connection(
        &self,
        client_id: &str,
        user_id: Option<&str>,
        subscriptions: &[String],
        queue_maxsize: usize,
    ) {
        let replaced = self.runtime_connections.insert(
            client_id.to_string(),
            Arc::new(RuntimeConnectionState::new(
                client_id,
                user_id,
                subscriptions,
                queue_maxsize,
            )),
        );
        if replaced.is_none() {
            self.lifetime_connections.fetch_add(1, Ordering::Relaxed);
        }
        for topic in subscriptions {
            let counter = self
                .topic_subscriber_counts
                .entry(topic.clone())
                .or_insert_with(|| AtomicUsize::new(0));
            counter.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn unregister_connection(&self, client_id: &str) {
        if let Some((_, connection)) = self.runtime_connections.remove(client_id) {
            for topic in &connection.subscriptions {
                if let Some(counter) = self.topic_subscriber_counts.get(topic) {
                    counter.fetch_sub(1, Ordering::Relaxed);
                }
            }
            connection.is_active.store(false, Ordering::Relaxed);
            connection.queue_size.store(0, Ordering::Relaxed);
        }
    }

    pub fn record_connection_enqueue(&self, client_id: &str, is_heartbeat: bool) {
        if let Some(connection) = self.runtime_connection(client_id) {
            connection.mark_enqueued(is_heartbeat);
        }
    }

    pub fn rollback_connection_enqueue(&self, client_id: &str) {
        if let Some(connection) = self.runtime_connection(client_id) {
            connection.rollback_enqueue();
        }
    }

    pub fn record_connection_delivery(&self, client_id: &str) {
        if let Some(connection) = self.runtime_connection(client_id) {
            connection.mark_delivered();
        }
    }

    pub fn record_connection_lagged(&self, client_id: &str, count: u64) {
        if let Some(connection) = self.runtime_connection(client_id) {
            connection.mark_lagged(count);
        }
        self.lagged_total.fetch_add(count, Ordering::Relaxed);
        self.messages_failed.fetch_add(count, Ordering::Relaxed);
    }

    pub fn record_connection_send_failure(&self, client_id: &str) {
        if let Some(connection) = self.runtime_connection(client_id) {
            connection.mark_lagged(1);
        }
        self.messages_failed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn get_topic_subscriber_count(&self, topic: &str) -> usize {
        let topic = topic.trim();
        if topic.is_empty() {
            return 0;
        }
        self.topic_subscriber_counts
            .get(topic)
            .map(|counter| counter.load(Ordering::Relaxed))
            .unwrap_or(0)
    }

    fn runtime_connection(&self, client_id: &str) -> Option<Arc<RuntimeConnectionState>> {
        self.runtime_connections
            .get(client_id)
            .map(|connection| connection.value().clone())
    }

    /// 订阅一个主题，返回 Receiver
    pub async fn subscribe(&self, topic: &str) -> broadcast::Receiver<SseMessage> {
        self.prune_idle_topics_if_due().await;
        if !Self::is_allowed_topic(topic) {
            return closed_receiver(self.capacity);
        }

        if let Some(sender) = self.topics.get(topic) {
            return sender.subscribe();
        }

        let sender = self
            .topics
            .entry(topic.to_string())
            .or_insert_with(|| broadcast::channel(self.capacity).0);
        sender.subscribe()
    }

    /// 向主题广播消息
    pub async fn broadcast(&self, topic: &str, data: serde_json::Value) -> usize {
        self.broadcast_event(topic, None, data).await
    }

    /// 向主题广播指定 event 的消息
    pub async fn broadcast_event(&self, topic: &str, event: Option<&str>, data: serde_json::Value) -> usize {
        let delivered = {
            if let Some(sender) = self.topics.get(topic) {
                let serialized = serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string());
                let msg = SseMessage {
                    topic: topic.to_string(),
                    event: event.map(ToOwned::to_owned),
                    serialized_data: Arc::new(serialized),
                };
                match sender.send(msg) {
                    Ok(n) => {
                        self.total_messages_sent.fetch_add(n as u64, Ordering::Relaxed);
                        n
                    }
                    Err(_) => 0,
                }
            } else {
                0
            }
        };
        self.prune_idle_topics_if_due().await;
        delivered
    }

    async fn prune_idle_topics_if_due(&self) {
        let now = now_ms();
        let interval_ms = (self.cleanup_interval_seconds.max(1) as i64) * 1000;

        loop {
            let last_cleanup_ms = self.last_cleanup_ms.load(Ordering::Acquire);
            if now.saturating_sub(last_cleanup_ms) < interval_ms {
                return;
            }

            if self
                .last_cleanup_ms
                .compare_exchange_weak(last_cleanup_ms, now, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                self.prune_idle_topics().await;
                return;
            }
        }
    }

    pub async fn prune_idle_topics(&self) {
        self.topics.retain(|_, sender| sender.receiver_count() > 0);
    }

    #[cfg(test)]
    fn remove_topic_if_idle(&self, topic: &str) -> bool {
        self.topics
            .remove_if(topic, |_topic, sender| sender.receiver_count() == 0)
            .is_some()
    }

    /// 向所有活跃主题广播 (heartbeat)
    pub async fn broadcast_all(&self, data: serde_json::Value) -> usize {
        let serialized = serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string());
        let shared_serialized = Arc::new(serialized);
        let mut total = 0;
        for entry in self.topics.iter() {
            let topic = entry.key();
            let sender = entry.value();
            let msg = SseMessage {
                topic: topic.clone(),
                event: None,
                serialized_data: Arc::clone(&shared_serialized),
            };
            let delivered = sender.send(msg).unwrap_or(0);
            total += delivered;
        }
        self.total_messages_sent.fetch_add(total as u64, Ordering::Relaxed);
        self.prune_idle_topics_if_due().await;
        total
    }

    /// 获取统计
    pub async fn stats(&self) -> SseStats {
        self.prune_idle_topics_if_due().await;
        let current_time_ms = now_ms();
        let mut connection_details = self
            .runtime_connections
            .iter()
            .map(|connection| connection.value().snapshot(current_time_ms))
            .collect::<Vec<_>>();
        connection_details.sort_by(|left, right| left.client_id.cmp(&right.client_id));

        let connected = connection_details.iter().filter(|detail| detail.is_active).count();
        let inactive = connection_details.len().saturating_sub(connected);
        let messages_dropped = connection_details
            .iter()
            .map(|detail| detail.dropped_messages)
            .sum::<u64>();
        let topics = topic_counts_from_details(&connection_details);
        let topic_count = topics.len();

        SseStats {
            active_connections: connection_details.len(),
            total_connections: connection_details.len(),
            active_connections_gauge: connection_details.len(),
            lifetime_connections: self.lifetime_connections.load(Ordering::Relaxed),
            lifetime_connections_counter: self.lifetime_connections.load(Ordering::Relaxed),
            messages_sent: self.total_messages_sent.load(Ordering::Relaxed),
            messages_failed: self.messages_failed.load(Ordering::Relaxed),
            messages_dropped,
            topics,
            connection_breakdown: SseConnectionBreakdown { connected, inactive },
            connection_details,
            heartbeat_interval: self.heartbeat_interval,
            max_connections: self.max_connections,
            connection_queue_size: self.connection_queue_size,
            cleanup_interval_seconds: self.cleanup_interval_seconds,
            heartbeat_timeout_seconds: self.heartbeat_timeout_seconds,
            queue_full_disconnect_seconds: self.queue_full_disconnect_seconds,
            topic_count,
            total_messages_sent: self.total_messages_sent.load(Ordering::Relaxed),
            lagged_total: self.lagged_total.load(Ordering::Relaxed),
        }
    }

    pub fn is_allowed_topic(topic: &str) -> bool {
        let topic = topic.trim();
        if topic.is_empty() || topic.len() > 128 {
            return false;
        }

        if STATIC_TOPICS.iter().any(|candidate| *candidate == topic) {
            return true;
        }

        PREFIX_TOPICS
            .iter()
            .any(|prefix| topic.strip_prefix(prefix).map(is_valid_topic_suffix).unwrap_or(false))
    }
}

#[derive(Default)]
#[allow(dead_code)]
struct Dummy; // Just a separator if needed, but not required

#[async_trait]
impl Broadcaster for SseHub {
    async fn broadcast_event(&self, topic: &str, event_name: Option<&str>, payload: serde_json::Value) {
        let _ = SseHub::broadcast_event(self, topic, event_name, payload).await;
    }
}

fn topic_counts_from_details(details: &[SseConnectionDetail]) -> BTreeMap<String, usize> {
    let mut topics = BTreeMap::<String, usize>::new();
    for detail in details {
        for topic in &detail.subscriptions {
            *topics.entry(topic.clone()).or_insert(0) += 1;
        }
    }
    topics
}

fn closed_receiver(capacity: usize) -> broadcast::Receiver<SseMessage> {
    let (sender, receiver) = broadcast::channel(capacity.max(1));
    drop(sender);
    receiver
}

fn is_valid_topic_suffix(suffix: &str) -> bool {
    !suffix.is_empty()
        && suffix.len() <= 96
        && suffix
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | ':' | '.' | '@'))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as i64)
        .unwrap_or_default()
}

fn ms_to_unix_seconds(value: i64) -> f64 {
    (value as f64) / 1000.0
}

pub fn normalize_event_source_message(message: &SseMessage, fallback_event: &str) -> Option<NormalizedSsePayload> {
    let event = message
        .event
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback_event)
        .to_string();

    let serialized = message.serialized_data.as_str();
    let trimmed = serialized.trim();
    if is_empty_json_payload(trimmed) {
        return None;
    }

    if !needs_payload_parse(trimmed) {
        return Some(NormalizedSsePayload {
            event,
            data: serialized.to_owned(),
        });
    }

    let Ok(payload) = serde_json::from_str::<serde_json::Value>(trimmed) else {
        return Some(NormalizedSsePayload {
            event,
            data: serialized.to_owned(),
        });
    };

    if let Some(object) = payload.as_object() {
        let wrapped_event = object
            .get("event")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if let Some(wrapped_event) = wrapped_event {
            let wrapped_payload = object.get("data").unwrap_or(&payload);
            let data = match wrapped_payload {
                serde_json::Value::String(text) => text.trim().to_string(),
                // 仅对嵌套的非字符串 payload 才需要重新序列化（罕见路径）
                value => serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string()),
            };
            if data.is_empty() {
                return None;
            }
            return Some(NormalizedSsePayload {
                event: wrapped_event.to_string(),
                data,
            });
        }

        // 直接使用预序列化数据，消除重复 serde_json::to_string() 调用
        if is_empty_json_payload(trimmed) {
            return None;
        }
        return Some(NormalizedSsePayload {
            event,
            data: serialized.to_owned(),
        });
    }

    if let Some(text) = payload.as_str() {
        let data = text.trim().to_string();
        if data.is_empty() {
            return None;
        }
        if data.starts_with("data:") || data.starts_with("event:") {
            let mut parsed_event = event.clone();
            let mut data_lines = Vec::new();
            for line in data.lines().map(str::trim).filter(|line| !line.is_empty()) {
                if let Some(value) = line.strip_prefix("event:") {
                    let value = value.trim();
                    if !value.is_empty() {
                        parsed_event = value.to_string();
                    }
                    continue;
                }
                if let Some(value) = line.strip_prefix("data:") {
                    data_lines.push(value.trim_start().to_string());
                }
            }
            if data_lines.is_empty() {
                return None;
            }
            return Some(NormalizedSsePayload {
                event: parsed_event,
                data: data_lines.join("\n"),
            });
        }
        return Some(NormalizedSsePayload { event, data });
    }

    // 使用预序列化数据，消除最后的 serde_json::to_string() 回退调用
    if is_empty_json_payload(trimmed) {
        return None;
    }

    Some(NormalizedSsePayload {
        event,
        data: serialized.to_owned(),
    })
}

fn is_empty_json_payload(value: &str) -> bool {
    matches!(value, "{}" | "[]" | "null")
}

fn needs_payload_parse(value: &str) -> bool {
    value.starts_with('"')
        || (value.starts_with('{')
            && (value.contains("\"event\"") || value.contains("\"data\"") || value.contains("\\u")))
}

#[derive(Debug, Serialize, Clone)]
pub struct SseStats {
    pub active_connections: usize,
    pub total_connections: usize,
    pub active_connections_gauge: usize,
    pub lifetime_connections: u64,
    pub lifetime_connections_counter: u64,
    pub messages_sent: u64,
    pub messages_failed: u64,
    pub messages_dropped: u64,
    pub topics: BTreeMap<String, usize>,
    pub connection_breakdown: SseConnectionBreakdown,
    pub connection_details: Vec<SseConnectionDetail>,
    pub heartbeat_interval: u64,
    pub max_connections: usize,
    pub connection_queue_size: usize,
    pub cleanup_interval_seconds: u64,
    pub heartbeat_timeout_seconds: u64,
    pub queue_full_disconnect_seconds: u64,
    pub topic_count: usize,
    pub total_messages_sent: u64,
    pub lagged_total: u64,
}

#[cfg(test)]
mod tests {
    use super::{normalize_event_source_message, SseHub, SseMessage, DEFAULT_MAX_CONNECTIONS};
    use serde_json::json;
    use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

    #[test]
    fn ai_v2_user_topics_are_allowed() {
        assert!(SseHub::is_allowed_topic("user_ai_v2_demo-user"));
    }

    #[test]
    fn kpi_updated_topic_is_allowed() {
        assert!(SseHub::is_allowed_topic("kpi_updated"));
    }

    #[actix_web::test]
    async fn runtime_stats_track_connection_details_and_topics() {
        let hub = SseHub::new(16);
        assert!(hub.try_acquire_connection());
        let topics = vec!["flights".to_string(), "system_alerts".to_string()];
        hub.register_connection("client-1", Some("user-1"), &topics, 64);
        hub.record_connection_enqueue("client-1", false);

        let stats = hub.stats().await;
        assert_eq!(stats.active_connections, 1);
        assert_eq!(stats.total_connections, 1);
        assert_eq!(stats.connection_breakdown.connected, 1);
        assert_eq!(stats.topics.get("flights"), Some(&1));
        assert_eq!(stats.topics.get("system_alerts"), Some(&1));
        assert_eq!(stats.connection_details.len(), 1);
        assert_eq!(stats.connection_details[0].client_id, "client-1");
        assert_eq!(stats.connection_details[0].queue_size, 1);
        assert_eq!(
            stats.connection_details[0].subscriptions,
            vec!["flights".to_string(), "system_alerts".to_string()]
        );
        assert_eq!(hub.get_topic_subscriber_count("flights"), 1);
        assert_eq!(hub.get_topic_subscriber_count("system_alerts"), 1);

        hub.unregister_connection("client-1");
        hub.release_connection();
    }

    #[actix_web::test]
    async fn lagged_messages_are_reported_as_dropped() {
        let hub = SseHub::new(16);
        assert!(hub.try_acquire_connection());
        let topics = vec!["global_status".to_string()];
        hub.register_connection("client-lagged", Some("ops"), &topics, 64);
        hub.record_connection_lagged("client-lagged", 3);

        let stats = hub.stats().await;
        assert_eq!(stats.messages_dropped, 3);
        assert_eq!(stats.lagged_total, 3);
        assert_eq!(stats.messages_failed, 3);
        assert_eq!(stats.connection_details[0].dropped_messages, 3);

        hub.unregister_connection("client-lagged");
        hub.release_connection();
    }

    #[actix_web::test]
    async fn broadcast_hot_path_defers_idle_topic_pruning_until_cleanup_is_due() {
        let hub = SseHub::new(16);
        let receiver = hub.subscribe("flights").await;
        drop(receiver);

        assert!(hub.topics.contains_key("flights"));
        assert_eq!(hub.broadcast("flights", json!({"status": "ok"})).await, 0);
        assert!(
            hub.topics.contains_key("flights"),
            "broadcast should not scan and prune idle topics on every hot-path call"
        );

        hub.prune_idle_topics().await;
        assert!(!hub.topics.contains_key("flights"));
    }

    #[actix_web::test]
    async fn default_max_connections_is_bounded_when_env_is_unset() {
        let _guard = lock_env();
        let _env = EnvVarGuard::unset("SSE_MAX_CONNECTIONS");

        let hub = SseHub::new(16);
        let stats = hub.stats().await;
        assert_eq!(stats.max_connections, DEFAULT_MAX_CONNECTIONS);
    }

    #[actix_web::test]
    async fn max_connections_env_override_is_enforced() {
        let _guard = lock_env();
        let _env = EnvVarGuard::set("SSE_MAX_CONNECTIONS", "2");

        let hub = SseHub::new(16);
        let stats = hub.stats().await;
        assert_eq!(stats.max_connections, 2);
        assert!(hub.try_acquire_connection());
        assert!(hub.try_acquire_connection());
        assert!(!hub.try_acquire_connection());

        hub.release_connection();
        hub.release_connection();
    }

    #[actix_web::test]
    async fn idle_pruning_does_not_remove_topic_with_new_receiver() {
        let hub = SseHub::new(16);
        let stale_receiver = hub.subscribe("flights").await;
        drop(stale_receiver);
        assert!(hub.topics.contains_key("flights"));

        let stale_candidate = "flights".to_string();
        let mut new_receiver = hub.subscribe("flights").await;
        hub.remove_topic_if_idle(&stale_candidate);

        assert!(
            hub.topics.contains_key("flights"),
            "stale cleanup candidate must not remove a topic that has a new receiver"
        );
        assert_eq!(hub.broadcast("flights", json!({"status": "ok"})).await, 1);
        match new_receiver.try_recv() {
            Ok(message) => assert_eq!(message.topic, "flights"),
            Err(error) => panic!("new receiver should get broadcast after pruning: {error}"),
        }
    }

    #[test]
    fn normalize_plain_json_object_uses_fallback_event_and_original_data() {
        let message = test_message("flights", None, r#"{"status":"ok"}"#);

        let normalized = normalize_event_source_message(&message, "fallback").expect("message should normalize");

        assert_eq!(normalized.event, "fallback");
        assert_eq!(normalized.data, r#"{"status":"ok"}"#);
    }

    #[test]
    fn normalize_wrapped_event_uses_nested_event_and_data() {
        let message = test_message(
            "flights",
            Some("ignored"),
            r#"{"event":"flight_status_changes","data":{"flight_id":"CA123","status":"delayed"}}"#,
        );

        let normalized = normalize_event_source_message(&message, "fallback").expect("message should normalize");

        assert_eq!(normalized.event, "flight_status_changes");
        assert_eq!(normalized.data, r#"{"flight_id":"CA123","status":"delayed"}"#);
    }

    #[test]
    fn normalize_data_object_without_event_preserves_original_payload() {
        let message = test_message("flights", None, r#"{"data":{"flight_id":"CA123","status":"delayed"}}"#);

        let normalized = normalize_event_source_message(&message, "fallback").expect("message should normalize");

        assert_eq!(normalized.event, "fallback");
        assert_eq!(normalized.data, r#"{"data":{"flight_id":"CA123","status":"delayed"}}"#);
    }

    #[test]
    fn normalize_json_string_sse_lines_preserves_multiline_data() {
        let message = test_message("notifications", None, r#""event: notify\ndata: first\ndata: second\n""#);

        let normalized = normalize_event_source_message(&message, "fallback").expect("message should normalize");

        assert_eq!(normalized.event, "notify");
        assert_eq!(normalized.data, "first\nsecond");
    }

    #[test]
    fn normalize_exact_empty_json_payloads_are_ignored() {
        for payload in ["{}", "[]", "null", " {}", " [] ", " null\n"] {
            let message = test_message("flights", None, payload);
            assert!(normalize_event_source_message(&message, "fallback").is_none());
        }
    }

    fn test_message(topic: &str, event: Option<&str>, serialized_data: &str) -> SseMessage {
        SseMessage {
            topic: topic.to_string(),
            event: event.map(str::to_string),
            serialized_data: Arc::new(serialized_data.to_string()),
        }
    }

    fn env_lock() -> &'static Mutex<()> {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        ENV_LOCK.get_or_init(|| Mutex::new(()))
    }

    fn lock_env() -> MutexGuard<'static, ()> {
        match env_lock().lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvVarGuard {
        fn unset(key: &'static str) -> Self {
            let previous = std::env::var(key).ok();
            std::env::remove_var(key);
            Self { key, previous }
        }

        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            std::env::set_var(key, value);
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            if let Some(value) = &self.previous {
                std::env::set_var(self.key, value);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }
}
