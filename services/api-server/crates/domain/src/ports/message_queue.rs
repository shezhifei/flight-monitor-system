use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MessageQueueError {
    #[error("message queue gateway unavailable: {0}")]
    Unavailable(String),
    #[error("message queue gateway rejected request: {0}")]
    BadRequest(String),
    #[error("message queue receipt is unknown or already acknowledged: {0}")]
    UnknownReceipt(String),
    #[error("message queue gateway error: {0}")]
    Gateway(String),
    #[error("message queue transport error: {0}")]
    Transport(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PublishMessage {
    pub topic: String,
    pub tag: Option<String>,
    pub key: Option<String>,
    pub body: serde_json::Value,
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReceiveMessages {
    pub topic: String,
    pub consumer_group: String,
    pub filter_tag: Option<String>,
    pub batch_size: Option<usize>,
    pub wait_ms: Option<u64>,
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

impl ReceivedMessage {
    pub fn into_subscriber_message(self) -> SubscriberMessage {
        SubscriberMessage {
            message_id: self.message_id,
            topic: self.topic,
            tag: self.tag,
            key: self.key,
            body: self.body,
            properties: self.properties,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SubscriberMessage {
    pub message_id: String,
    pub topic: String,
    pub tag: Option<String>,
    pub key: Option<String>,
    pub body: serde_json::Value,
    pub properties: BTreeMap<String, String>,
}

#[async_trait]
pub trait MessageHandler: Send + Sync {
    async fn handle(&self, messages: Vec<SubscriberMessage>) -> Result<(), MessageQueueError>;
}

#[async_trait]
pub trait PushConsumer: Send + Sync {
    async fn subscribe(
        &self,
        topic: &str,
        consumer_group: &str,
        sub_expression: Option<&str>,
        handler: Arc<dyn MessageHandler>,
    ) -> Result<(), MessageQueueError>;

    async fn start(&self) -> Result<(), MessageQueueError>;
    async fn shutdown(&self) -> Result<(), MessageQueueError>;
}

#[async_trait]
pub trait MessageQueue {
    async fn publish(&self, message: PublishMessage) -> Result<String, MessageQueueError>;

    async fn receive(&self, request: ReceiveMessages) -> Result<Vec<ReceivedMessage>, MessageQueueError>;

    async fn ack(&self, receipt_handle: &str) -> Result<(), MessageQueueError>;
}
