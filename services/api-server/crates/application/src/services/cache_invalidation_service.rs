use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use fms_domain::error::DomainError;
use fms_domain::ports::flight_runtime_projection_repository::FlightRuntimeProjectionRepository;
use fms_domain::ports::message_queue::{
    MessageHandler, MessageQueue, MessageQueueError, PublishMessage, SubscriberMessage,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::warn;

use crate::types::ConcreteFlightService;

pub const CACHE_INVALIDATION_EVENT_TYPE: &str = "cache.invalidation";
pub const DEFAULT_CACHE_INVALIDATION_TOPIC: &str = "fms_domain_events";
const DEFAULT_CONSUMER_GROUP_PREFIX: &str = "cache_invalidation";
const PUBLISH_FAILED_TOTAL_METRIC: &str = "cache_invalidation_publish_failed_total";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheInvalidationKey {
    FlightRuntimeProjection,
    FlightListHot,
    FlightListResponse,
}

impl CacheInvalidationKey {
    fn as_str(self) -> &'static str {
        match self {
            Self::FlightRuntimeProjection => "flight_runtime_projection",
            Self::FlightListHot => "flight_list_hot",
            Self::FlightListResponse => "flight_list_response",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheInvalidationEvent {
    pub event_id: String,
    pub event_type: String,
    pub scope: String,
    pub flight_id: Option<String>,
    pub cache_keys: Vec<String>,
    pub occurred_at: String,
    pub source_instance: String,
}

impl CacheInvalidationEvent {
    pub fn for_flight(
        flight_id: impl Into<String>,
        cache_keys: impl IntoIterator<Item = CacheInvalidationKey>,
        source_instance: impl Into<String>,
    ) -> Self {
        Self::new("flight", Some(flight_id.into()), cache_keys, source_instance)
    }

    pub fn for_flight_list(
        cache_keys: impl IntoIterator<Item = CacheInvalidationKey>,
        source_instance: impl Into<String>,
    ) -> Self {
        Self::new("flight_list", None, cache_keys, source_instance)
    }

    fn new(
        scope: impl Into<String>,
        flight_id: Option<String>,
        cache_keys: impl IntoIterator<Item = CacheInvalidationKey>,
        source_instance: impl Into<String>,
    ) -> Self {
        Self {
            event_id: ulid::Ulid::new().to_string(),
            event_type: CACHE_INVALIDATION_EVENT_TYPE.to_string(),
            scope: scope.into(),
            flight_id: flight_id
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            cache_keys: cache_keys.into_iter().map(|key| key.as_str().to_string()).collect(),
            occurred_at: Utc::now().to_rfc3339(),
            source_instance: source_instance.into(),
        }
    }

    fn recognized_keys(&self) -> Vec<CacheInvalidationKey> {
        self.cache_keys
            .iter()
            .filter_map(|key| match key.trim() {
                "flight_runtime_projection" => Some(CacheInvalidationKey::FlightRuntimeProjection),
                "flight_list_hot" => Some(CacheInvalidationKey::FlightListHot),
                "flight_list_response" => Some(CacheInvalidationKey::FlightListResponse),
                other => {
                    warn!(
                        cache_key = other,
                        event_id = %self.event_id,
                        "ignoring unknown cache invalidation key"
                    );
                    None
                }
            })
            .collect()
    }
}

#[async_trait]
pub trait FlightListResponseCacheInvalidator: Send + Sync {
    async fn invalidate_flight_list_response_cache(&self);
}

pub struct CacheInvalidationService {
    message_queue: Option<Arc<dyn MessageQueue + Send + Sync>>,
    topic: String,
    source_instance: String,
    projection_repo: Arc<dyn FlightRuntimeProjectionRepository>,
    flight_service: Arc<ConcreteFlightService>,
    response_cache: Option<Arc<dyn FlightListResponseCacheInvalidator>>,
}

impl CacheInvalidationService {
    pub fn new(
        message_queue: Option<Arc<dyn MessageQueue + Send + Sync>>,
        topic: impl Into<String>,
        source_instance: impl Into<String>,
        projection_repo: Arc<dyn FlightRuntimeProjectionRepository>,
        flight_service: Arc<ConcreteFlightService>,
        response_cache: Option<Arc<dyn FlightListResponseCacheInvalidator>>,
    ) -> Self {
        Self {
            message_queue,
            topic: trim_or_default(topic.into(), DEFAULT_CACHE_INVALIDATION_TOPIC),
            source_instance: source_instance.into(),
            projection_repo,
            flight_service,
            response_cache,
        }
    }

    pub fn source_instance(&self) -> &str {
        &self.source_instance
    }

    pub fn flight_event(
        &self,
        flight_id: impl Into<String>,
        cache_keys: impl IntoIterator<Item = CacheInvalidationKey>,
    ) -> CacheInvalidationEvent {
        CacheInvalidationEvent::for_flight(flight_id, cache_keys, self.source_instance.clone())
    }

    pub fn flight_list_event(
        &self,
        cache_keys: impl IntoIterator<Item = CacheInvalidationKey>,
    ) -> CacheInvalidationEvent {
        CacheInvalidationEvent::for_flight_list(cache_keys, self.source_instance.clone())
    }

    pub async fn invalidate_local(&self, event: &CacheInvalidationEvent) {
        for key in event.recognized_keys() {
            match key {
                CacheInvalidationKey::FlightRuntimeProjection => {
                    if let Some(flight_id) = event.flight_id.as_deref() {
                        self.projection_repo.invalidate_flight(flight_id).await;
                    }
                }
                CacheInvalidationKey::FlightListHot => {
                    self.flight_service.invalidate_hot_list().await;
                }
                CacheInvalidationKey::FlightListResponse => {
                    if let Some(response_cache) = self.response_cache.as_ref() {
                        response_cache.invalidate_flight_list_response_cache().await;
                    }
                }
            }
        }
    }

    pub async fn publish(&self, event: &CacheInvalidationEvent) -> Result<String, DomainError> {
        let Some(message_queue) = self.message_queue.as_ref() else {
            return Ok(String::new());
        };

        let aggregate_id = event.flight_id.clone().unwrap_or_else(|| "flight_list".to_string());
        let body = json!({
            "event_id": event.event_id,
            "aggregate_type": "cache",
            "aggregate_id": aggregate_id,
            "event_type": CACHE_INVALIDATION_EVENT_TYPE,
            "occurred_at": event.occurred_at,
            "payload": event,
            "source_change_id": "",
        });

        message_queue
            .publish(PublishMessage {
                topic: self.topic.clone(),
                tag: Some(CACHE_INVALIDATION_EVENT_TYPE.to_string()),
                key: Some(event.event_id.clone()),
                body,
                properties: Default::default(),
            })
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))
    }

    pub async fn invalidate_and_publish(&self, event: CacheInvalidationEvent) {
        self.invalidate_local(&event).await;
        if let Err(error) = self.publish(&event).await {
            metrics::counter!(
                PUBLISH_FAILED_TOTAL_METRIC,
                "event_id" => event.event_id.clone(),
                "scope" => event.scope.clone()
            )
            .increment(1);
            warn!(
                event_id = %event.event_id,
                flight_id = ?event.flight_id,
                error = %error,
                "failed to publish cache invalidation event"
            );
        }
    }
}

pub struct CacheInvalidationSubscriberService {
    invalidation_service: Arc<CacheInvalidationService>,
    topic: String,
    consumer_group: String,
}

impl CacheInvalidationSubscriberService {
    pub fn new(
        invalidation_service: Arc<CacheInvalidationService>,
        topic: impl Into<String>,
        consumer_group: Option<String>,
    ) -> Self {
        let source_instance = invalidation_service.source_instance().to_string();
        Self {
            invalidation_service,
            topic: trim_or_default(topic.into(), DEFAULT_CACHE_INVALIDATION_TOPIC),
            consumer_group: consumer_group
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| format!("{DEFAULT_CONSUMER_GROUP_PREFIX}_{source_instance}")),
        }
    }

    pub fn topic(&self) -> &str {
        &self.topic
    }

    pub fn consumer_group(&self) -> &str {
        &self.consumer_group
    }
}

/// Handle messages delivered via push-consumer callback.
/// Acknowledgement is handled by the RocketMQ push consumer framework
/// through the return status, so we only need to process the events.
impl CacheInvalidationSubscriberService {
    pub async fn handle_messages(&self, messages: Vec<SubscriberMessage>) -> Result<(), DomainError> {
        for msg in &messages {
            match decode_cache_invalidation_event(&msg.body) {
                Some(event) => {
                    self.invalidation_service.invalidate_local(&event).await;
                }
                None => {
                    warn!(
                        message_id = %msg.message_id,
                        "received malformed cache invalidation message"
                    );
                }
            }
        }
        Ok(())
    }
}

#[async_trait]
impl MessageHandler for CacheInvalidationSubscriberService {
    async fn handle(&self, messages: Vec<SubscriberMessage>) -> Result<(), MessageQueueError> {
        self.handle_messages(messages)
            .await
            .map_err(|e| MessageQueueError::Transport(format!("cache invalidation handler failed: {e}")))
    }
}

fn decode_cache_invalidation_event(body: &Value) -> Option<CacheInvalidationEvent> {
    let payload = body.get("payload").unwrap_or(body);
    serde_json::from_value::<CacheInvalidationEvent>(payload.clone()).ok()
}

fn trim_or_default(value: String, default: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{CacheInvalidationEvent, CacheInvalidationKey, CACHE_INVALIDATION_EVENT_TYPE};

    #[test]
    fn cache_invalidation_event_round_trips() {
        let event = CacheInvalidationEvent::for_flight(
            "flight-1",
            [
                CacheInvalidationKey::FlightRuntimeProjection,
                CacheInvalidationKey::FlightListResponse,
            ],
            "node-1",
        );
        let value = serde_json::to_value(&event).expect("serialize event");
        let decoded: CacheInvalidationEvent = serde_json::from_value(value).expect("deserialize event");

        assert_eq!(decoded.event_type, CACHE_INVALIDATION_EVENT_TYPE);
        assert_eq!(decoded.scope, "flight");
        assert_eq!(decoded.flight_id.as_deref(), Some("flight-1"));
        assert_eq!(
            decoded.cache_keys,
            vec!["flight_runtime_projection", "flight_list_response"]
        );
        assert_eq!(decoded.source_instance, "node-1");
    }

    #[test]
    fn unknown_cache_keys_are_ignored() {
        let mut event =
            CacheInvalidationEvent::for_flight("flight-1", [CacheInvalidationKey::FlightListResponse], "node-1");
        event.cache_keys.push("unknown".to_string());

        assert_eq!(event.recognized_keys(), vec![CacheInvalidationKey::FlightListResponse]);
    }

    #[test]
    fn publish_failed_metric_name_is_defined() {
        assert_eq!(
            super::PUBLISH_FAILED_TOTAL_METRIC,
            "cache_invalidation_publish_failed_total"
        );
    }
}
