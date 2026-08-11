use fms_mq_gateway::offset_store::{ConsumerOffset, OffsetStore};
use fms_mq_gateway::offset_store_memory::MemoryOffsetStore;

#[tokio::test]
async fn memory_store_round_trips_offsets() {
    let store = MemoryOffsetStore::new();
    let key = ConsumerOffset {
        topic: "fms.domain-events".to_string(),
        consumer_group: "domain_event_processors".to_string(),
        queue_id: 3,
        broker_name: "broker-a".to_string(),
    };
    store.save(&key, 42).await.unwrap();
    let restored = store.load(&key).await.unwrap();
    assert_eq!(restored, Some(42));
}

#[tokio::test]
async fn memory_store_returns_none_for_missing_key() {
    let store = MemoryOffsetStore::new();
    let key = ConsumerOffset {
        topic: "fms.domain-events".to_string(),
        consumer_group: "domain_event_processors".to_string(),
        queue_id: 0,
        broker_name: "broker-a".to_string(),
    };
    assert_eq!(store.load(&key).await.unwrap(), None);
}
