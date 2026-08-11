use async_trait::async_trait;
use redis::{AsyncCommands, Client};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::offset_store::{ConsumerOffset, OffsetStore, OffsetStoreError};

pub struct RedisOffsetStore {
    conn: Arc<Mutex<redis::aio::MultiplexedConnection>>,
    key_prefix: String,
}

impl RedisOffsetStore {
    pub async fn new(redis_url: &str) -> Result<Self, OffsetStoreError> {
        let client = Client::open(redis_url)
            .map_err(|e| OffsetStoreError::Unavailable(format!("open redis: {e}")))?;
        let conn = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| OffsetStoreError::Unavailable(format!("connect redis: {e}")))?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            key_prefix: std::env::var("MQ_GATEWAY_REDIS_KEY_PREFIX")
                .unwrap_or_else(|_| "mq_gateway:offset".to_string()),
        })
    }

    fn build_key(&self, key: &ConsumerOffset) -> String {
        format!(
            "{}:{}:{}:{}:{}",
            self.key_prefix, key.topic, key.consumer_group, key.broker_name, key.queue_id
        )
    }
}

#[async_trait]
impl OffsetStore for RedisOffsetStore {
    async fn load(&self, key: &ConsumerOffset) -> Result<Option<i64>, OffsetStoreError> {
        let mut conn = self.conn.lock().await;
        let value: Option<i64> = conn
            .get(self.build_key(key))
            .await
            .map_err(|e| OffsetStoreError::Unavailable(format!("redis load failed: {e}")))?;
        Ok(value)
    }

    async fn save(&self, key: &ConsumerOffset, offset: i64) -> Result<(), OffsetStoreError> {
        let mut conn = self.conn.lock().await;
        conn.set::<_, _, ()>(self.build_key(key), offset)
            .await
            .map_err(|e| OffsetStoreError::Unavailable(format!("redis save failed: {e}")))?;
        Ok(())
    }
}
