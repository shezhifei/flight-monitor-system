use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rocketmq_client_rust::consumer::default_mq_push_consumer::DefaultMQPushConsumer;
use rocketmq_client_rust::consumer::listener::consume_concurrently_context::ConsumeConcurrentlyContext;
use rocketmq_client_rust::consumer::listener::consume_concurrently_status::ConsumeConcurrentlyStatus;
use rocketmq_client_rust::consumer::listener::message_listener_concurrently::MessageListenerConcurrently;
use rocketmq_client_rust::consumer::mq_push_consumer::MQPushConsumer;
use rocketmq_common::common::message::message_ext::MessageExt;
use rocketmq_common::common::message::MessageTrait;
use serde_json::Value;

use crate::messaging::{MessageHandler, MessageQueueError, PushConsumer, SubscriberMessage};

pub struct RocketMqPushConsumer {
    name_server_addr: String,
    inner: Mutex<Vec<DefaultMQPushConsumer>>,
    handlers: Mutex<Vec<(String, String, Option<String>, Arc<dyn MessageHandler>)>>,
}

impl RocketMqPushConsumer {
    pub fn new(name_server_addr: impl Into<String>) -> Self {
        Self {
            name_server_addr: name_server_addr.into(),
            inner: Mutex::new(Vec::new()),
            handlers: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl PushConsumer for RocketMqPushConsumer {
    async fn subscribe(
        &self,
        topic: &str,
        consumer_group: &str,
        sub_expression: Option<&str>,
        handler: Arc<dyn MessageHandler>,
    ) -> Result<(), MessageQueueError> {
        let mut handlers = self
            .handlers
            .lock()
            .expect("RocketMqPushConsumer: handlers lock poisoned");
        handlers.push((
            topic.to_string(),
            consumer_group.to_string(),
            sub_expression.map(|s| s.to_string()),
            handler,
        ));
        Ok(())
    }

    async fn start(&self) -> Result<(), MessageQueueError> {
        // Clone handlers data and release lock before any await
        let handlers_snapshot = {
            let handlers = self
                .handlers
                .lock()
                .expect("RocketMqPushConsumer: handlers lock poisoned");
            if handlers.is_empty() {
                return Ok(());
            }
            handlers.clone()
        };

        // group by consumer_group: one DefaultMQPushConsumer per group
        let mut grouped: BTreeMap<String, Vec<(String, Option<String>, Arc<dyn MessageHandler>)>> = BTreeMap::new();
        for (topic, group, sub_expr, handler) in handlers_snapshot.iter() {
            grouped
                .entry(group.clone())
                .or_default()
                .push((topic.clone(), sub_expr.clone(), handler.clone()));
        }

        let mut consumers = Vec::new();
        for (group, subs) in grouped {
            let mut consumer = DefaultMQPushConsumer::builder()
                .name_server_addr(self.name_server_addr.clone())
                .consumer_group(group)
                .build();

            for (topic, sub_expr, handler) in subs {
                let expr = sub_expr.as_deref().unwrap_or("*");
                consumer
                    .subscribe(&topic, expr)
                    .await
                    .map_err(|e| MessageQueueError::Transport(format!("subscribe failed: {e}")))?;
                consumer.register_message_listener_concurrently(ListenerAdapter { handler });
            }

            consumer
                .start()
                .await
                .map_err(|e| MessageQueueError::Transport(format!("consumer start failed: {e}")))?;
            consumers.push(consumer);
        }

        *self.inner.lock().expect("RocketMqPushConsumer: inner lock poisoned") = consumers;
        Ok(())
    }

    async fn shutdown(&self) -> Result<(), MessageQueueError> {
        // Take consumers out of the lock before awaiting shutdown
        let consumers = std::mem::take(&mut *self.inner.lock().expect("RocketMqPushConsumer: inner lock poisoned"));
        for mut consumer in consumers {
            consumer.shutdown().await;
        }
        Ok(())
    }
}

struct ListenerAdapter {
    handler: Arc<dyn MessageHandler>,
}

impl MessageListenerConcurrently for ListenerAdapter {
    fn consume_message(
        &self,
        msgs: &[&MessageExt],
        _ctx: &ConsumeConcurrentlyContext,
    ) -> rocketmq_error::RocketMQResult<ConsumeConcurrentlyStatus> {
        let messages: Vec<SubscriberMessage> = msgs.iter().map(|msg| convert_message(msg)).collect();

        let result =
            tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(self.handler.handle(messages)));

        let topic = msgs
            .first()
            .map(|msg| msg.topic().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let status = if result.is_ok() { "success" } else { "error" };
        metrics::counter!(
            "fms_mq_consume_total",
            "topic" => topic,
            "status" => status
        )
        .increment(1);

        match result {
            Ok(_) => Ok(ConsumeConcurrentlyStatus::ConsumeSuccess),
            Err(e) => {
                tracing::warn!(target: "rocketmq_push_consumer", "message handler failed: {e}");
                Ok(ConsumeConcurrentlyStatus::ReconsumeLater)
            }
        }
    }
}

fn convert_message(msg: &MessageExt) -> SubscriberMessage {
    let body = msg
        .body()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .unwrap_or(Value::Null);

    let properties = msg
        .properties()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    SubscriberMessage {
        message_id: msg.msg_id().to_string(),
        topic: msg.topic().to_string(),
        tag: msg.get_tags().map(|t| t.to_string()),
        key: msg.get_keys().map(|k| k.to_string()),
        body,
        properties,
    }
}
