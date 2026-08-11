use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

const DEFAULT_DOMAIN_CONSUMER_GROUP: &str = "domain_event_processors";
const MAX_BATCH_SIZE: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ValidationError {
    #[error("topic must not be blank")]
    BlankTopic,
    #[error("receipt_handle must not be blank")]
    BlankReceiptHandle,
    #[error("receipt_handle must contain topic, consumer_group, and message_id")]
    InvalidReceiptHandle,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PublishRequest {
    pub topic: String,
    pub tag: Option<String>,
    pub key: Option<String>,
    pub body: serde_json::Value,
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
}

impl PublishRequest {
    pub fn validate(self) -> Result<ValidatedPublishRequest, ValidationError> {
        let topic = normalize_required(&self.topic).ok_or(ValidationError::BlankTopic)?;
        Ok(ValidatedPublishRequest {
            topic,
            tag: normalize_optional(self.tag.as_deref()),
            key: normalize_optional(self.key.as_deref()),
            body: self.body,
            properties: self.properties,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ValidatedPublishRequest {
    pub topic: String,
    pub tag: Option<String>,
    pub key: Option<String>,
    pub body: serde_json::Value,
    pub properties: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PublishResponse {
    pub message_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReceiveRequest {
    pub topic: String,
    pub consumer_group: String,
    pub filter_tag: Option<String>,
    pub batch_size: Option<usize>,
    pub wait_ms: Option<u64>,
}

impl ReceiveRequest {
    pub fn validate(self) -> Result<ValidatedReceiveRequest, ValidationError> {
        let topic = normalize_required(&self.topic).ok_or(ValidationError::BlankTopic)?;
        let consumer_group = normalize_optional(Some(&self.consumer_group))
            .unwrap_or_else(|| DEFAULT_DOMAIN_CONSUMER_GROUP.to_string());
        let batch_size = self.batch_size.unwrap_or(100).clamp(1, MAX_BATCH_SIZE);
        let wait_ms = self.wait_ms.unwrap_or(200).max(1);

        Ok(ValidatedReceiveRequest {
            topic,
            consumer_group,
            filter_tag: normalize_optional(self.filter_tag.as_deref())
                .or_else(|| Some("*".to_string())),
            batch_size,
            wait_ms,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ValidatedReceiveRequest {
    pub topic: String,
    pub consumer_group: String,
    pub filter_tag: Option<String>,
    pub batch_size: usize,
    pub wait_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReceivedMessage {
    pub receipt_handle: String,
    pub message_id: String,
    pub topic: String,
    pub tag: Option<String>,
    pub key: Option<String>,
    pub body: serde_json::Value,
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReceiveResponse {
    pub messages: Vec<ReceivedMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AckRequest {
    pub receipt_handle: String,
}

impl AckRequest {
    pub fn validate(self) -> Result<ValidatedAckRequest, ValidationError> {
        let handle = self.receipt_handle.trim();
        if handle.is_empty() {
            return Err(ValidationError::BlankReceiptHandle);
        }

        if let Some(rest) = handle.strip_prefix("v2|") {
            let mut parts = rest.splitn(5, '|');
            let topic = parts.next().and_then(normalize_required);
            let consumer_group = parts.next().and_then(normalize_required);
            let queue_id = parts
                .next()
                .and_then(normalize_required)
                .and_then(|value| value.parse::<i32>().ok());
            let next_offset = parts
                .next()
                .and_then(normalize_required)
                .and_then(|value| value.parse::<i64>().ok())
                .filter(|value| *value >= 0);
            let message_id = parts.next().and_then(normalize_required);

            return match (topic, consumer_group, queue_id, next_offset, message_id) {
                (
                    Some(topic),
                    Some(consumer_group),
                    Some(queue_id),
                    Some(next_offset),
                    Some(message_id),
                ) => Ok(ValidatedAckRequest {
                    receipt_handle: handle.to_string(),
                    topic,
                    consumer_group,
                    message_id,
                    queue_id: Some(queue_id),
                    next_offset: Some(next_offset),
                }),
                _ => Err(ValidationError::InvalidReceiptHandle),
            };
        }

        let mut parts = handle.splitn(3, '|');
        let topic = parts.next().and_then(normalize_required);
        let consumer_group = parts.next().and_then(normalize_required);
        let message_id = parts.next().and_then(normalize_required);
        match (topic, consumer_group, message_id) {
            (Some(topic), Some(consumer_group), Some(message_id)) => Ok(ValidatedAckRequest {
                receipt_handle: handle.to_string(),
                topic,
                consumer_group,
                message_id,
                queue_id: None,
                next_offset: None,
            }),
            _ => Err(ValidationError::InvalidReceiptHandle),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedAckRequest {
    pub receipt_handle: String,
    pub topic: String,
    pub consumer_group: String,
    pub message_id: String,
    pub queue_id: Option<i32>,
    pub next_offset: Option<i64>,
}

pub fn build_receipt_handle(topic: &str, consumer_group: &str, message_id: &str) -> String {
    format!(
        "{}|{}|{}",
        topic.trim(),
        consumer_group.trim(),
        message_id.trim()
    )
}

pub fn build_queue_receipt_handle(
    topic: &str,
    consumer_group: &str,
    queue_id: i32,
    next_offset: i64,
    message_id: &str,
) -> String {
    format!(
        "v2|{}|{}|{}|{}|{}",
        topic.trim(),
        consumer_group.trim(),
        queue_id,
        next_offset,
        message_id.trim()
    )
}

fn normalize_required(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn normalize_optional(value: Option<&str>) -> Option<String> {
    value.and_then(normalize_required)
}
