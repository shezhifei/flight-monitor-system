use std::sync::Arc;

use chrono::Utc;
use fms_domain::error::DomainError;
use fms_domain::ports::message_queue::{MessageQueue, PublishMessage};
use serde_json::{json, Value};
use fms_domain::ports::domain_event_outbox_repository::DomainEventOutboxTransactionalRepository;

pub use fms_domain::events::DomainEventOutboxRow;

const DEFAULT_DOMAIN_TOPIC: &str = "fms.domain-events";
const RETRY_TOTAL_METRIC: &str = "domain_event_relay_retry_total";
const RELAY_LAG_MS_METRIC: &str = "domain_event_relay_lag_ms";
const MAX_BACKOFF_SECONDS: i64 = 300;
const MAX_ERROR_LENGTH: usize = 1000;

/// Outbox publish + mark helpers.
///
/// Holds a transactional outbox repo port for `mark_published` / `mark_failed`
/// operations that require an open transaction.
pub struct DomainEventOutboxDelivery<Tx> {
    topic: String,
    base_backoff_seconds: i64,
    repo: Arc<dyn DomainEventOutboxTransactionalRepository<Tx> + Send + Sync>,
}

// 手写 `Clone`：`derive` 会要求 `Tx: Clone`，但被克隆的是 `Arc`，与 `Tx` 无关。
impl<Tx> Clone for DomainEventOutboxDelivery<Tx> {
    fn clone(&self) -> Self {
        Self {
            topic: self.topic.clone(),
            base_backoff_seconds: self.base_backoff_seconds,
            repo: self.repo.clone(),
        }
    }
}

impl<Tx> DomainEventOutboxDelivery<Tx> {
    pub fn new(
        base_backoff_seconds: i64,
        topic: Option<String>,
        repo: Arc<dyn DomainEventOutboxTransactionalRepository<Tx> + Send + Sync>,
    ) -> Self {
        let resolved_topic = topic
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_DOMAIN_TOPIC.to_string());

        Self {
            topic: resolved_topic,
            base_backoff_seconds: base_backoff_seconds.max(1),
            repo,
        }
    }

    pub fn topic(&self) -> &str {
        &self.topic
    }

    pub async fn publish_row<MQ: MessageQueue + ?Sized>(
        &self,
        message_queue: &MQ,
        row: &DomainEventOutboxRow,
    ) -> Result<(), DomainError> {
        let payload = normalize_payload(&row.payload);
        let event_type = row.event_type.as_deref().unwrap_or_default();
        let message = PublishMessage {
            topic: self.topic.clone(),
            tag: (!event_type.trim().is_empty()).then(|| event_type.to_owned()),
            key: Some(row.event_id.clone()),
            body: json!({
                "event_id": row.event_id,
                "aggregate_type": row.aggregate_type.as_deref().unwrap_or_default(),
                "aggregate_id": row.aggregate_id.as_deref().unwrap_or_default(),
                "event_type": event_type,
                "occurred_at": row.occurred_at.to_rfc3339(),
                "payload": payload,
                "source_change_id": row.source_change_id.as_deref().unwrap_or_default(),
            }),
            properties: Default::default(),
        };

        let started_at = std::time::Instant::now();
        let outcome = message_queue
            .publish(message)
            .await
            .map(|_| ())
            .map_err(|error| DomainError::Internal(error.to_string()));
        metrics::histogram!("fms_outbox_publish_duration_seconds").record(started_at.elapsed().as_secs_f64());
        outcome
    }

    pub async fn claim_pending(
        &self,
        tx: &mut Tx,
        limit: i64,
    ) -> Result<Vec<DomainEventOutboxRow>, DomainError> {
        self.repo.claim_pending_in_tx(tx, limit).await
    }

    pub async fn mark_published(
        &self,
        tx: &mut Tx,
        event_ids: &[String],
    ) -> Result<(), DomainError> {
        self.repo.mark_published_in_tx(tx, event_ids).await
    }

    pub async fn mark_failed(
        &self,
        tx: &mut Tx,
        row: &DomainEventOutboxRow,
        error: &str,
    ) -> Result<i64, DomainError> {
        let next_attempt = row.publish_attempts.saturating_add(1);
        let backoff_seconds = self.compute_backoff_seconds(next_attempt);

        metrics::counter!(RETRY_TOTAL_METRIC, "attempt" => next_attempt.to_string()).increment(1);

        self.repo
            .mark_failed_in_tx(tx, row, &truncate_error(error), backoff_seconds)
            .await?;

        Ok(backoff_seconds)
    }

    pub fn observe_relay_lag(&self, row: &DomainEventOutboxRow) {
        let lag_ms = ((Utc::now() - row.occurred_at).num_milliseconds().max(0)) as f64;
        metrics::histogram!(RELAY_LAG_MS_METRIC).record(lag_ms);
    }

    pub fn compute_backoff_seconds(&self, attempt: i32) -> i64 {
        compute_backoff_seconds(self.base_backoff_seconds, attempt)
    }
}

pub fn event_type_metric_label(row: &DomainEventOutboxRow) -> String {
    row.event_type
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("unknown")
        .to_string()
}

pub fn normalize_payload(payload: &Value) -> Value {
    match payload {
        Value::String(raw) => serde_json::from_str::<Value>(raw).unwrap_or_else(|_| json!({ "raw": raw })),
        Value::Object(_) => payload.clone(),
        _ => json!({}),
    }
}

pub fn compute_backoff_seconds(base_backoff_seconds: i64, attempt: i32) -> i64 {
    let bounded_attempt = attempt.max(1) as u32;
    let multiplier = 2_i64.saturating_pow(bounded_attempt.saturating_sub(1));
    base_backoff_seconds
        .max(1)
        .saturating_mul(multiplier)
        .min(MAX_BACKOFF_SECONDS)
}

pub fn truncate_error(error: &str) -> String {
    error.chars().take(MAX_ERROR_LENGTH).collect()
}

#[cfg(test)]
mod tests {
    use super::{compute_backoff_seconds, normalize_payload};
    use serde_json::json;

    #[test]
    fn compute_backoff_matches_python_and_caps() {
        assert_eq!(compute_backoff_seconds(2, 1), 2);
        assert_eq!(compute_backoff_seconds(2, 2), 4);
        assert_eq!(compute_backoff_seconds(2, 3), 8);
        assert_eq!(compute_backoff_seconds(2, 9), 300);
    }

    #[test]
    fn normalize_payload_keeps_objects() {
        let payload = json!({"hello": "world"});
        assert_eq!(normalize_payload(&payload), payload);
    }

    #[test]
    fn normalize_payload_parses_json_strings_and_wraps_invalid_strings() {
        assert_eq!(
            normalize_payload(&json!("{\"hello\":\"world\"}")),
            json!({"hello": "world"})
        );
        assert_eq!(normalize_payload(&json!("not-json")), json!({"raw": "not-json"}));
    }

    #[test]
    fn normalize_payload_falls_back_to_empty_object_for_non_objects() {
        assert_eq!(normalize_payload(&json!([1, 2, 3])), json!({}));
        assert_eq!(normalize_payload(&json!(null)), json!({}));
    }
}
