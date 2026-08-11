use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use fms_runtime::spawn_tracked::spawn_tracked;

use crate::messaging::{MessageHandler, MessageQueueError, PushConsumer, SubscriberMessage};

type HandlerRegistration = (Option<String>, Arc<dyn MessageHandler>);

pub struct MemoryPushConsumer {
    handlers: Mutex<HashMap<String, Vec<HandlerRegistration>>>,
}

impl MemoryPushConsumer {
    pub fn new() -> Self {
        Self {
            handlers: Mutex::new(HashMap::new()),
        }
    }

    pub fn inject(&self, topic: &str, tag: Option<&str>, messages: Vec<SubscriberMessage>) {
        let Some(handlers) = self.matching_handlers(topic, tag) else {
            return;
        };

        for handler in handlers {
            let msgs = messages.clone();
            // Note: back-pressure is intentionally relaxed here because this is
            // an in-memory test consumer; panics are no longer silently dropped.
            spawn_tracked("messaging:memory_push_handler", async move {
                let _ = handler.handle(msgs).await;
            });
        }
    }

    fn matching_handlers(&self, topic: &str, tag: Option<&str>) -> Option<Vec<Arc<dyn MessageHandler>>> {
        let handlers = self
            .handlers
            .lock()
            .expect("MemoryPushConsumer: handlers lock poisoned");
        let topic_handlers = handlers.get(topic)?;
        Some(
            topic_handlers
                .iter()
                .filter(|(registered_tag, _handler)| tag_matches(registered_tag.as_deref(), tag))
                .map(|(_registered_tag, handler)| handler.clone())
                .collect(),
        )
    }
}

fn tag_matches(registered_tag: Option<&str>, message_tag: Option<&str>) -> bool {
    match (registered_tag, message_tag) {
        (None, _) => true,
        (Some(r), Some(t)) => r == t || r == "*",
        (Some(_), None) => false,
    }
}

impl Default for MemoryPushConsumer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PushConsumer for MemoryPushConsumer {
    async fn subscribe(
        &self,
        topic: &str,
        _consumer_group: &str,
        sub_expression: Option<&str>,
        handler: Arc<dyn MessageHandler>,
    ) -> Result<(), MessageQueueError> {
        let mut handlers = self
            .handlers
            .lock()
            .expect("MemoryPushConsumer: handlers lock poisoned");
        handlers
            .entry(topic.to_string())
            .or_default()
            .push((sub_expression.map(|s| s.to_string()), handler));
        Ok(())
    }

    async fn start(&self) -> Result<(), MessageQueueError> {
        Ok(())
    }

    async fn shutdown(&self) -> Result<(), MessageQueueError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    struct CountingHandler {
        counter: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl MessageHandler for CountingHandler {
        async fn handle(&self, messages: Vec<SubscriberMessage>) -> Result<(), MessageQueueError> {
            self.counter.fetch_add(messages.len(), Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn memory_push_consumer_routes_matching_messages() {
        let consumer = MemoryPushConsumer::new();
        let counter = Arc::new(AtomicUsize::new(0));
        consumer
            .subscribe(
                "test-topic",
                "cg",
                Some("matched"),
                Arc::new(CountingHandler {
                    counter: counter.clone(),
                }),
            )
            .await
            .unwrap();
        consumer.start().await.unwrap();

        consumer.inject(
            "test-topic",
            Some("matched"),
            vec![SubscriberMessage {
                message_id: "1".to_string(),
                topic: "test-topic".to_string(),
                tag: Some("matched".to_string()),
                key: None,
                body: json!({}),
                properties: Default::default(),
            }],
        );

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn memory_push_consumer_does_not_clone_messages_while_holding_handlers_lock() {
        let consumer = MemoryPushConsumer::new();
        let counter = Arc::new(AtomicUsize::new(0));
        consumer
            .subscribe(
                "test-topic",
                "cg",
                Some("*"),
                Arc::new(CountingHandler {
                    counter: counter.clone(),
                }),
            )
            .await
            .expect("test subscription should be registered");

        let handlers = consumer
            .matching_handlers("test-topic", Some("matched"))
            .expect("matching handlers should be collected");

        let subscription = consumer.subscribe(
            "test-topic",
            "cg",
            Some("late"),
            Arc::new(CountingHandler {
                counter: counter.clone(),
            }),
        );

        tokio::time::timeout(Duration::from_millis(50), subscription)
            .await
            .expect("subscribe should not wait for cloned messages after handler matching")
            .expect("late subscription should be registered");

        assert_eq!(handlers.len(), 1);
    }
}
