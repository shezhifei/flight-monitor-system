use fms_mq_gateway::offset_store::{ConsumerOffset, OffsetStore};
use fms_mq_gateway::offset_store_redis::RedisOffsetStore;

#[tokio::test]
#[ignore = "requires redis"]
async fn redis_store_round_trips_offsets() {
    let redis_url =
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    let store = RedisOffsetStore::new(&redis_url)
        .await
        .expect("connect redis");
    let key = ConsumerOffset {
        topic: "fms.domain-events".to_string(),
        consumer_group: "domain_event_processors".to_string(),
        queue_id: 7,
        broker_name: "broker-a".to_string(),
    };
    store.save(&key, 12345).await.unwrap();
    let restored = store.load(&key).await.unwrap();
    assert_eq!(restored, Some(12345));
}
