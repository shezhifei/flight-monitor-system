//! 广播器 trait，用于解耦应用层与 SSE 广播实现。

use async_trait::async_trait;

#[async_trait]
pub trait Broadcaster {
    async fn broadcast_event(&self, topic: &str, event_name: Option<&str>, payload: serde_json::Value);
}
