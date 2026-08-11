use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConsumerOffset {
    pub topic: String,
    pub consumer_group: String,
    pub queue_id: i32,
    pub broker_name: String,
}

#[cfg(feature = "rocketmq-backend")]
impl ConsumerOffset {
    pub fn from_message_queue(
        topic: &str,
        consumer_group: &str,
        queue: &rocketmq_common::common::message::message_queue::MessageQueue,
    ) -> Self {
        Self {
            topic: topic.to_string(),
            consumer_group: consumer_group.to_string(),
            queue_id: queue.queue_id(),
            broker_name: queue.broker_name().to_string(),
        }
    }
}

impl fmt::Display for ConsumerOffset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}:{}:{}",
            self.topic, self.consumer_group, self.broker_name, self.queue_id
        )
    }
}

#[async_trait]
pub trait OffsetStore: Send + Sync {
    async fn load(&self, key: &ConsumerOffset) -> Result<Option<i64>, OffsetStoreError>;
    async fn save(&self, key: &ConsumerOffset, offset: i64) -> Result<(), OffsetStoreError>;
}

#[derive(Debug, thiserror::Error)]
pub enum OffsetStoreError {
    #[error("offset store unavailable: {0}")]
    Unavailable(String),
}
