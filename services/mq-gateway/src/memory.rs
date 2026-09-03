use std::collections::{BTreeMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::api::{
    build_receipt_handle, ReceivedMessage, ValidatedAckRequest, ValidatedPublishRequest,
    ValidatedReceiveRequest,
};
use crate::transport::{MessageTransport, TransportError};

#[derive(Clone, Default)]
pub struct InMemoryTransport {
    state: Arc<Mutex<MemoryState>>,
    sequence: Arc<AtomicU64>,
}

const MAX_IN_MEMORY_MESSAGES: usize = 10_000;

#[derive(Default)]
struct MemoryState {
    messages: VecDeque<StoredMessage>,
    pending_receipts: HashSet<String>,
}

#[derive(Clone)]
struct StoredMessage {
    message_id: String,
    topic: String,
    tag: Option<String>,
    key: Option<String>,
    body: serde_json::Value,
    properties: BTreeMap<String, String>,
}

#[async_trait]
impl MessageTransport for InMemoryTransport {
    async fn publish(&self, request: ValidatedPublishRequest) -> Result<String, TransportError> {
        let id = format!("mem-{}", self.sequence.fetch_add(1, Ordering::Relaxed) + 1);
        let message = StoredMessage {
            message_id: id.clone(),
            topic: request.topic,
            tag: request.tag,
            key: request.key,
            body: request.body,
            properties: request.properties,
        };

        let mut state = self.state.lock().await;
        if state.messages.len() >= MAX_IN_MEMORY_MESSAGES {
            state.messages.pop_front();
        }
        state.messages.push_back(message);
        Ok(id)
    }

    async fn receive(
        &self,
        request: ValidatedReceiveRequest,
    ) -> Result<Vec<ReceivedMessage>, TransportError> {
        let mut state = self.state.lock().await;
        let drained: VecDeque<StoredMessage> = std::mem::take(&mut state.messages);
        let mut selected = Vec::with_capacity(request.batch_size);
        let mut retained = VecDeque::with_capacity(drained.len());

        for message in drained {
            if selected.len() >= request.batch_size {
                retained.push_back(message);
            } else if message.topic == request.topic
                && tag_matches(message.tag.as_deref(), request.filter_tag.as_deref())
            {
                let receipt = build_receipt_handle(
                    &request.topic,
                    &request.consumer_group,
                    &message.message_id,
                );
                state.pending_receipts.insert(receipt.clone());
                selected.push(ReceivedMessage {
                    receipt_handle: receipt,
                    message_id: message.message_id,
                    topic: message.topic,
                    tag: message.tag,
                    key: message.key,
                    body: message.body,
                    properties: message.properties,
                });
            } else {
                retained.push_back(message);
            }
        }

        state.messages = retained;
        Ok(selected)
    }

    async fn ack(&self, request: ValidatedAckRequest) -> Result<(), TransportError> {
        let receipt =
            build_receipt_handle(&request.topic, &request.consumer_group, &request.message_id);
        let removed = self.state.lock().await.pending_receipts.remove(&receipt);
        if removed {
            Ok(())
        } else {
            Err(TransportError::UnknownReceipt(receipt))
        }
    }

    async fn health(&self) -> Result<(), TransportError> {
        Ok(())
    }
}

fn tag_matches(message_tag: Option<&str>, filter_tag: Option<&str>) -> bool {
    let filter = filter_tag.unwrap_or("*").trim();
    filter == "*" || message_tag.map(|tag| tag == filter).unwrap_or(false)
}
