use std::sync::Arc;

use fms_mq_gateway::api::{
    build_queue_receipt_handle, AckRequest, PublishRequest, ReceiveRequest, ReceivedMessage,
    ValidationError,
};
use fms_mq_gateway::memory::InMemoryTransport;
use fms_mq_gateway::transport::MessageTransport;
use tokio::sync::Barrier;

#[test]
fn publish_request_rejects_blank_topic() {
    let request = PublishRequest {
        topic: "  ".to_string(),
        tag: None,
        key: Some("evt-1".to_string()),
        body: serde_json::json!({"event_id": "evt-1"}),
        properties: Default::default(),
    };

    assert_eq!(request.validate(), Err(ValidationError::BlankTopic));
}

#[test]
fn receive_request_requires_topic_and_consumer_group() {
    let request = ReceiveRequest {
        topic: "fms.domain-events".to_string(),
        consumer_group: "".to_string(),
        filter_tag: Some("*".to_string()),
        batch_size: Some(5000),
        wait_ms: Some(0),
    };

    let normalized = request.validate().expect("topic is valid");
    assert_eq!(normalized.consumer_group, "domain_event_processors");
    assert_eq!(normalized.batch_size, 1024);
    assert_eq!(normalized.wait_ms, 1);
}

#[test]
fn receipt_round_trip_preserves_message_identity() {
    let message = ReceivedMessage {
        receipt_handle: "fms.domain-events|domain_event_processors|msg-123".to_string(),
        message_id: "msg-123".to_string(),
        topic: "fms.domain-events".to_string(),
        tag: Some("flight.status_updated_v2".to_string()),
        key: Some("evt-1".to_string()),
        body: serde_json::json!({"event_id": "evt-1"}),
        properties: Default::default(),
    };

    let ack = AckRequest {
        receipt_handle: message.receipt_handle.clone(),
    }
    .validate()
    .expect("receipt should parse");

    assert_eq!(ack.topic, "fms.domain-events");
    assert_eq!(ack.consumer_group, "domain_event_processors");
    assert_eq!(ack.message_id, "msg-123");
    assert_eq!(ack.queue_id, None);
    assert_eq!(ack.next_offset, None);
}

#[test]
fn queue_receipt_round_trip_preserves_offset_identity() {
    let receipt = build_queue_receipt_handle(
        "fms.domain-events",
        "domain_event_processors",
        3,
        42,
        "msg-123",
    );

    let ack = AckRequest {
        receipt_handle: receipt.clone(),
    }
    .validate()
    .expect("queue receipt should parse");

    assert_eq!(
        receipt,
        "v2|fms.domain-events|domain_event_processors|3|42|msg-123"
    );
    assert_eq!(ack.topic, "fms.domain-events");
    assert_eq!(ack.consumer_group, "domain_event_processors");
    assert_eq!(ack.queue_id, Some(3));
    assert_eq!(ack.next_offset, Some(42));
    assert_eq!(ack.message_id, "msg-123");
}

#[actix_rt::test]
async fn in_memory_receive_preserves_remaining_queue_order_after_batch_is_full() {
    let transport = InMemoryTransport::default();
    for index in 1..=5 {
        transport
            .publish(
                PublishRequest {
                    topic: "fms.domain-events".to_string(),
                    tag: Some("flight.status_updated_v2".to_string()),
                    key: Some(format!("evt-{index}")),
                    body: serde_json::json!({ "index": index }),
                    properties: Default::default(),
                }
                .validate()
                .expect("publish request should validate"),
            )
            .await
            .expect("publish should succeed");
    }

    let first_batch = transport
        .receive(
            ReceiveRequest {
                topic: "fms.domain-events".to_string(),
                consumer_group: "domain_event_processors".to_string(),
                filter_tag: Some("flight.status_updated_v2".to_string()),
                batch_size: Some(2),
                wait_ms: Some(1),
            }
            .validate()
            .expect("receive request should validate"),
        )
        .await
        .expect("first receive should succeed");
    assert_eq!(
        first_batch
            .iter()
            .map(|message| message.key.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("evt-1"), Some("evt-2")]
    );

    let second_batch = transport
        .receive(
            ReceiveRequest {
                topic: "fms.domain-events".to_string(),
                consumer_group: "domain_event_processors".to_string(),
                filter_tag: Some("flight.status_updated_v2".to_string()),
                batch_size: Some(3),
                wait_ms: Some(1),
            }
            .validate()
            .expect("receive request should validate"),
        )
        .await
        .expect("second receive should succeed");
    assert_eq!(
        second_batch
            .iter()
            .map(|message| message.key.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("evt-3"), Some("evt-4"), Some("evt-5")]
    );
}

#[actix_rt::test]
async fn concurrent_receives_never_duplicate_messages() {
    let transport = Arc::new(InMemoryTransport::default());
    let total_messages = 100usize;

    for index in 0..total_messages {
        transport
            .publish(
                PublishRequest {
                    topic: "fms.domain-events".to_string(),
                    tag: Some("flight.status_updated_v2".to_string()),
                    key: Some(format!("msg-{index}")),
                    body: serde_json::json!({ "index": index }),
                    properties: Default::default(),
                }
                .validate()
                .expect("publish request should validate"),
            )
            .await
            .expect("publish should succeed");
    }

    let num_consumers = 10;
    let barrier = Arc::new(Barrier::new(num_consumers));
    let mut handles = Vec::with_capacity(num_consumers);

    for _ in 0..num_consumers {
        let transport = transport.clone();
        let barrier = barrier.clone();
        handles.push(tokio::spawn(async move {
            barrier.wait().await;
            transport
                .receive(
                    ReceiveRequest {
                        topic: "fms.domain-events".to_string(),
                        consumer_group: "domain_event_processors".to_string(),
                        filter_tag: Some("flight.status_updated_v2".to_string()),
                        batch_size: Some(20),
                        wait_ms: Some(1),
                    }
                    .validate()
                    .expect("receive request should validate"),
                )
                .await
                .expect("receive should succeed")
        }));
    }

    let mut all_keys: Vec<String> = Vec::new();
    for handle in handles {
        let batch = handle.await.expect("task should not panic");
        for msg in batch {
            all_keys.push(msg.key.unwrap_or_default());
        }
    }

    all_keys.sort();
    all_keys.dedup();
    assert_eq!(
        all_keys.len(),
        total_messages,
        "each message must be received exactly once across all consumers"
    );
}
