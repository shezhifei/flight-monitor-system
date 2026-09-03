use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

const OFFSET_PERSIST_FAILED_TOTAL_METRIC: &str = "mq_gateway_offset_persist_failed_total";

use async_trait::async_trait;
use rocketmq_client_rust::admin::default_mq_admin_ext_impl::DefaultMQAdminExtImpl;
use rocketmq_client_rust::admin::mq_admin_ext_async::MQAdminExt;
use rocketmq_client_rust::base::client_config::ClientConfig;
use rocketmq_client_rust::consumer::pull_status::PullStatus;
use rocketmq_client_rust::producer::default_mq_producer::DefaultMQProducer;
use rocketmq_client_rust::producer::mq_producer::MQProducer;
use rocketmq_common::common::config::TopicConfig;
use rocketmq_common::common::message::message_builder::MessageBuilder;
use rocketmq_common::common::message::message_ext::MessageExt;
use rocketmq_common::common::message::message_queue::MessageQueue;
use rocketmq_common::common::message::MessageConst;
use rocketmq_rust::ArcMut;
use tokio::sync::{Mutex, RwLock};

use crate::api::{
    build_queue_receipt_handle, ReceivedMessage, ValidatedAckRequest, ValidatedPublishRequest,
    ValidatedReceiveRequest,
};
use crate::offset_store::{ConsumerOffset, OffsetStore};
use crate::offset_store_memory::MemoryOffsetStore;
use crate::transport::{MessageTransport, TransportError};

const DEFAULT_NAME_SERVER_ADDR: &str = "rocketmq-namesrv:9876";
const DEFAULT_PRODUCER_GROUP: &str = "fms_mq_gateway";
const DEFAULT_BROKER_ADDR: &str = "127.0.0.1:10911";
const DEFAULT_BOOTSTRAP_TOPICS: &[&str] = &["fms_domain_events", "fms_realtime", "fms_diagnostics"];

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ConsumerKey {
    topic: String,
    consumer_group: String,
    filter_tag: String,
}

#[derive(Debug, Clone)]
struct PendingReceipt {
    key: ConsumerKey,
    queue: MessageQueue,
    next_offset: i64,
}

/// 合并的消费者状态，减少锁竞争
struct ConsumerState {
    offsets: HashMap<ConsumerKey, HashMap<MessageQueue, i64>>,
    pending_receipts: HashMap<String, PendingReceipt>,
    in_flight_queues: HashSet<(ConsumerKey, MessageQueue)>,
}

impl ConsumerState {
    fn claim_pending_receipt(&mut self, receipt_handle: &str) -> Option<PendingReceipt> {
        self.pending_receipts.remove(receipt_handle)
    }

    fn restore_pending_receipt(&mut self, receipt_handle: String, pending: PendingReceipt) {
        self.pending_receipts
            .entry(receipt_handle)
            .or_insert(pending);
    }

    fn complete_ack(&mut self, pending: &PendingReceipt) {
        let group_offsets = self.offsets.entry(pending.key.clone()).or_default();
        let offset = group_offsets.entry(pending.queue.clone()).or_insert(0);
        *offset = (*offset).max(pending.next_offset);
    }
}

pub struct RocketMqTransport {
    broker_addr: String,
    admin: RwLock<ArcMut<DefaultMQAdminExtImpl>>,
    producer: RwLock<DefaultMQProducer>,
    consumer_state: Mutex<ConsumerState>,
    offset_store: Arc<dyn OffsetStore>,
}

impl RocketMqTransport {
    pub async fn from_env() -> Result<Self, TransportError> {
        let namesrv_addr = std::env::var("ROCKETMQ_NAME_SERVER_ADDR")
            .unwrap_or_else(|_| DEFAULT_NAME_SERVER_ADDR.to_string());
        let producer_group = std::env::var("MQ_GATEWAY_PRODUCER_GROUP")
            .unwrap_or_else(|_| DEFAULT_PRODUCER_GROUP.to_string());
        let offset_store: Arc<dyn OffsetStore> =
            match std::env::var("MQ_GATEWAY_OFFSET_STORE").as_deref() {
                Ok("redis") => {
                    let redis_url = std::env::var("REDIS_URL")
                        .unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
                    let store = crate::offset_store_redis::RedisOffsetStore::new(&redis_url)
                        .await
                        .map_err(|e| TransportError::Unavailable(e.to_string()))?;
                    Arc::new(store)
                }
                _ => Arc::new(MemoryOffsetStore::new()),
            };
        Self::new(namesrv_addr, producer_group, offset_store).await
    }

    pub async fn new(
        namesrv_addr: String,
        producer_group: String,
        offset_store: Arc<dyn OffsetStore>,
    ) -> Result<Self, TransportError> {
        ensure_topics(&namesrv_addr).await?;

        let mut producer = DefaultMQProducer::builder()
            .producer_group(producer_group)
            .name_server_addr(namesrv_addr.clone())
            .build();
        producer.start().await.map_err(map_unavailable)?;

        let broker_addr = std::env::var("MQ_GATEWAY_BROKER_ADDR")
            .unwrap_or_else(|_| DEFAULT_BROKER_ADDR.to_string());
        let mut client_config = ClientConfig::new();
        client_config.set_namesrv_addr(namesrv_addr.into());
        let mut admin = ArcMut::new(DefaultMQAdminExtImpl::new(
            None,
            Duration::from_secs(10),
            ArcMut::new(client_config),
            "fms_mq_gateway_runtime_admin".into(),
        ));
        let admin_inner = admin.clone();
        admin.set_inner(admin_inner);
        admin.start().await.map_err(map_unavailable)?;

        Ok(Self {
            broker_addr,
            admin: RwLock::new(admin),
            producer: RwLock::new(producer),
            consumer_state: Mutex::new(ConsumerState {
                offsets: HashMap::new(),
                pending_receipts: HashMap::new(),
                in_flight_queues: HashSet::new(),
            }),
            offset_store,
        })
    }
}

async fn ensure_topics(namesrv_addr: &str) -> Result<(), TransportError> {
    let broker_addr =
        std::env::var("MQ_GATEWAY_BROKER_ADDR").unwrap_or_else(|_| DEFAULT_BROKER_ADDR.to_string());
    let topics = std::env::var("MQ_GATEWAY_BOOTSTRAP_TOPICS")
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .filter(|topics| !topics.is_empty())
        .unwrap_or_else(|| {
            DEFAULT_BOOTSTRAP_TOPICS
                .iter()
                .map(|value| value.to_string())
                .collect()
        });

    let mut client_config = ClientConfig::new();
    client_config.set_namesrv_addr(namesrv_addr.into());
    let mut admin = ArcMut::new(DefaultMQAdminExtImpl::new(
        None,
        Duration::from_secs(10),
        ArcMut::new(client_config),
        "fms_mq_gateway_admin".into(),
    ));
    let admin_inner = admin.clone();
    admin.set_inner(admin_inner);
    admin.start().await.map_err(map_unavailable)?;
    for topic in &topics {
        let topic_config = TopicConfig {
            topic_name: Some(topic.as_str().into()),
            read_queue_nums: 8,
            write_queue_nums: 8,
            ..TopicConfig::default()
        };
        admin
            .create_and_update_topic_config(broker_addr.as_str().into(), topic_config)
            .await
            .map_err(map_backend)?;
    }
    wait_for_topic_routes(&admin, &broker_addr, &topics).await?;
    admin.shutdown().await;
    Ok(())
}

async fn wait_for_topic_routes(
    admin: &ArcMut<DefaultMQAdminExtImpl>,
    broker_addr: &str,
    topics: &[String],
) -> Result<(), TransportError> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(45);
    let mut last_error = None;

    loop {
        let mut ready = true;
        for topic in topics {
            match admin
                .examine_topic_stats(topic.as_str().into(), Some(broker_addr.into()))
                .await
            {
                Ok(_) => {}
                Err(error) => {
                    ready = false;
                    last_error = Some(error.to_string());
                    break;
                }
            }
        }

        if ready {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(TransportError::Unavailable(format!(
                "RocketMQ topics were not ready before timeout: {}",
                last_error.unwrap_or_else(|| "unknown error".to_string())
            )));
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

#[async_trait]
impl MessageTransport for RocketMqTransport {
    async fn publish(&self, request: ValidatedPublishRequest) -> Result<String, TransportError> {
        let body = serde_json::to_vec(&request.body)
            .map_err(|error| TransportError::Backend(format!("serialize message body: {error}")))?;
        let mut builder = MessageBuilder::new().topic(request.topic).body(body);
        if let Some(tag) = request.tag {
            builder = builder.tags(tag);
        }
        if let Some(key) = request.key {
            builder = builder.key(key);
        }
        for (key, value) in request.properties {
            builder = builder.raw_property(key, value).map_err(map_backend)?;
        }

        let message = builder.build().map_err(map_backend)?;
        let mut producer = self.producer.write().await;
        let result = producer.send(message).await.map_err(map_backend)?;
        let message_id = result
            .and_then(|result| result.msg_id.map(|value| value.to_string()))
            .ok_or_else(|| {
                TransportError::Backend("RocketMQ send returned no message id".to_string())
            })?;
        Ok(message_id)
    }

    async fn receive(
        &self,
        request: ValidatedReceiveRequest,
    ) -> Result<Vec<ReceivedMessage>, TransportError> {
        let key = ConsumerKey {
            topic: request.topic.clone(),
            consumer_group: request.consumer_group.clone(),
            filter_tag: request
                .filter_tag
                .clone()
                .unwrap_or_else(|| "*".to_string()),
        };
        let filter_tag = key.filter_tag.as_str();
        let deadline = tokio::time::Instant::now() + Duration::from_millis(request.wait_ms);
        let mut messages = Vec::new();

        while messages.len() < request.batch_size {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }

            let stats = {
                let admin = self.admin.read().await;
                admin
                    .examine_topic_stats(
                        request.topic.as_str().into(),
                        Some(self.broker_addr.as_str().into()),
                    )
                    .await
                    .map_err(map_backend)?
            };
            let mut queues = stats.into_offset_table().into_iter().collect::<Vec<_>>();
            queues.sort_by_key(|(queue, _)| queue.queue_id());

            let mut made_progress = false;
            for (queue, topic_offset) in queues {
                if messages.len() >= request.batch_size {
                    break;
                }

                let consumer_offset_key = ConsumerOffset::from_message_queue(
                    &request.topic,
                    &request.consumer_group,
                    &queue,
                );
                let stored = self
                    .offset_store
                    .load(&consumer_offset_key)
                    .await
                    .ok()
                    .flatten();
                let offset = {
                    let mut state = self.consumer_state.lock().await;
                    let in_flight_key = (key.clone(), queue.clone());
                    if state.in_flight_queues.contains(&in_flight_key) {
                        continue;
                    }

                    let group_offsets = state.offsets.entry(key.clone()).or_default();
                    let offset = *group_offsets
                        .entry(queue.clone())
                        .or_insert_with(|| stored.unwrap_or_else(|| topic_offset.get_min_offset()));
                    if offset < topic_offset.get_max_offset() {
                        state.in_flight_queues.insert(in_flight_key);
                    }
                    offset
                };
                if offset >= topic_offset.get_max_offset() {
                    continue;
                }

                let pull_result = {
                    let admin = self.admin.read().await;
                    admin
                        .pull_message_from_queue_for_group(
                            &self.broker_addr,
                            &request.consumer_group,
                            &queue,
                            filter_tag,
                            offset,
                            (request.batch_size - messages.len()) as i32,
                            remaining.as_millis().min(u64::MAX as u128) as u64,
                        )
                        .await
                };
                let pull_result = match pull_result {
                    Ok(pull_result) => pull_result,
                    Err(error) => {
                        let mut state = self.consumer_state.lock().await;
                        state.in_flight_queues.remove(&(key.clone(), queue.clone()));
                        return Err(map_backend(error));
                    }
                };

                if pull_result.pull_status() != &PullStatus::Found {
                    let mut state = self.consumer_state.lock().await;
                    state.in_flight_queues.remove(&(key.clone(), queue.clone()));
                    continue;
                }

                if let Some(raw_messages) = pull_result.msg_found_list() {
                    let mut state = self.consumer_state.lock().await;
                    let mut max_next_offset = offset;
                    for message in raw_messages
                        .iter()
                        .take(request.batch_size - messages.len())
                    {
                        let next_offset = message.queue_offset() + 1;
                        max_next_offset = max_next_offset.max(next_offset);
                        let received = received_message_from_ext(
                            &request,
                            &queue,
                            next_offset,
                            message.as_ref().clone(),
                        );
                        state.pending_receipts.insert(
                            received.receipt_handle.clone(),
                            PendingReceipt {
                                key: key.clone(),
                                queue: queue.clone(),
                                next_offset,
                            },
                        );
                        messages.push(received);
                        made_progress = true;
                    }
                    if max_next_offset > offset {
                        let group_offsets = state.offsets.entry(key.clone()).or_default();
                        let offset = group_offsets
                            .entry(queue.clone())
                            .or_insert(max_next_offset);
                        *offset = (*offset).max(max_next_offset);
                    }
                    state.in_flight_queues.remove(&(key.clone(), queue.clone()));
                } else {
                    let mut state = self.consumer_state.lock().await;
                    state.in_flight_queues.remove(&(key.clone(), queue.clone()));
                }
            }

            if !made_progress {
                break;
            }
        }

        Ok(messages)
    }

    async fn ack(&self, request: ValidatedAckRequest) -> Result<(), TransportError> {
        let receipt_handle = request.receipt_handle.clone();
        let pending = {
            let mut state = self.consumer_state.lock().await;
            state
                .claim_pending_receipt(&receipt_handle)
                .ok_or_else(|| TransportError::UnknownReceipt(receipt_handle.clone()))?
        };

        let offset_key = ConsumerOffset::from_message_queue(
            &pending.key.topic,
            &pending.key.consumer_group,
            &pending.queue,
        );
        if let Err(e) = self
            .offset_store
            .save(&offset_key, pending.next_offset)
            .await
        {
            metrics::counter!(OFFSET_PERSIST_FAILED_TOTAL_METRIC).increment(1);
            log::warn!(target: "mq_gateway", "failed to persist consumer offset: {e}");
            let mut state = self.consumer_state.lock().await;
            state.restore_pending_receipt(receipt_handle, pending);
            return Err(TransportError::Backend(format!(
                "failed to persist consumer offset: {e}"
            )));
        }

        let admin = self.admin.read().await;
        if let Err(e) = admin
            .update_consume_offset(
                self.broker_addr.as_str().into(),
                request.consumer_group.as_str().into(),
                pending.queue.clone(),
                pending.next_offset as u64,
            )
            .await
        {
            let mut state = self.consumer_state.lock().await;
            state.restore_pending_receipt(receipt_handle, pending);
            return Err(map_backend(e));
        }

        let mut state = self.consumer_state.lock().await;
        state.complete_ack(&pending);
        Ok(())
    }

    async fn health(&self) -> Result<(), TransportError> {
        let _guard = self.producer.read().await;
        Ok(())
    }
}

fn received_message_from_ext(
    request: &ValidatedReceiveRequest,
    queue: &MessageQueue,
    next_offset: i64,
    message: MessageExt,
) -> ReceivedMessage {
    let message_id = message.msg_id().to_string();
    let topic = message.topic().to_string();
    let tag = message.get_tags().map(|value| value.to_string());
    let properties = message
        .properties()
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect::<BTreeMap<_, _>>();
    let key = properties.get(MessageConst::PROPERTY_KEYS).cloned();
    let body = message
        .body()
        .and_then(|body| serde_json::from_slice(body.as_ref()).ok())
        .unwrap_or(serde_json::Value::Null);
    let receipt_handle = build_queue_receipt_handle(
        &request.topic,
        &request.consumer_group,
        queue.queue_id(),
        next_offset,
        &message_id,
    );

    ReceivedMessage {
        receipt_handle,
        message_id,
        topic,
        tag,
        key,
        body,
        properties,
    }
}

fn map_unavailable(error: impl std::fmt::Display) -> TransportError {
    TransportError::Unavailable(error.to_string())
}

fn map_backend(error: impl std::fmt::Display) -> TransportError {
    TransportError::Backend(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{ConsumerKey, ConsumerState, PendingReceipt};
    use rocketmq_common::common::message::message_queue::MessageQueue;
    use std::collections::{HashMap, HashSet};

    fn consumer_key() -> ConsumerKey {
        ConsumerKey {
            topic: "fms_domain_events".to_string(),
            consumer_group: "domain_event_processors".to_string(),
            filter_tag: "*".to_string(),
        }
    }

    fn message_queue() -> MessageQueue {
        MessageQueue::from_parts("fms_domain_events", "broker-a", 3)
    }

    fn pending_receipt() -> PendingReceipt {
        PendingReceipt {
            key: consumer_key(),
            queue: message_queue(),
            next_offset: 42,
        }
    }

    fn consumer_state_with_pending(receipt_handle: &str) -> ConsumerState {
        let mut pending_receipts = HashMap::new();
        pending_receipts.insert(receipt_handle.to_string(), pending_receipt());
        ConsumerState {
            offsets: HashMap::new(),
            pending_receipts,
            in_flight_queues: HashSet::new(),
        }
    }

    #[test]
    fn claiming_pending_receipt_removes_it_before_external_ack_work() {
        let mut state = consumer_state_with_pending("receipt-1");

        let first = state.claim_pending_receipt("receipt-1");
        let second = state.claim_pending_receipt("receipt-1");

        assert!(first.is_some());
        assert!(second.is_none());
        assert!(!state.pending_receipts.contains_key("receipt-1"));
    }

    #[test]
    fn failed_ack_can_restore_claimed_receipt_for_retry() {
        let mut state = consumer_state_with_pending("receipt-1");
        let pending = state
            .claim_pending_receipt("receipt-1")
            .expect("receipt should be claimable");

        state.restore_pending_receipt("receipt-1".to_string(), pending.clone());
        let retried = state.claim_pending_receipt("receipt-1");

        assert_eq!(retried.map(|pending| pending.next_offset), Some(42));
    }

    #[test]
    fn completing_ack_advances_local_offset_after_external_ack_work() {
        let mut state = ConsumerState {
            offsets: HashMap::new(),
            pending_receipts: HashMap::new(),
            in_flight_queues: HashSet::new(),
        };
        let pending = pending_receipt();

        state.complete_ack(&pending);

        assert_eq!(
            state
                .offsets
                .get(&pending.key)
                .and_then(|queues| queues.get(&pending.queue))
                .copied(),
            Some(42)
        );
    }
}
