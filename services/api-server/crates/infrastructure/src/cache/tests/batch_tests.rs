//! Tests of the shipped Redis PIPELINE/MGET batch transform and RedisCacheService.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::cache::cache_service::{assemble_batch_get_results, CacheService, RedisCacheService};
use crate::cache::{create_redis_pool, redis_pipeline_enabled};
use crate::config::RedisConfig;
use crate::observability::shadow_mode_enabled;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct TestCacheValue {
    id: String,
    data: String,
}

fn sample(id: &str, data: &str) -> TestCacheValue {
    TestCacheValue {
        id: id.to_string(),
        data: data.to_string(),
    }
}

#[test]
fn assemble_empty_keys_returns_empty_map() {
    let map = assemble_batch_get_results::<TestCacheValue>(&[], Vec::new());
    assert!(map.is_empty());
}

#[test]
fn assemble_omits_missing_and_invalid_values() {
    let present = sample("1", "alpha");
    let encoded = serde_json::to_string(&present).expect("encode present value");
    let keys = ["keep", "missing", "broken", "also-keep"];
    let values = vec![
        Some(encoded),
        None,
        Some("not-json".to_string()),
        Some(serde_json::to_string(&sample("2", "beta")).expect("encode beta")),
    ];

    let map = assemble_batch_get_results::<TestCacheValue>(&keys, values);
    assert_eq!(map.len(), 2);
    assert_eq!(map.get("keep").map(|v| v.data.as_str()), Some("alpha"));
    assert_eq!(map.get("also-keep").map(|v| v.data.as_str()), Some("beta"));
    assert!(!map.contains_key("missing"));
    assert!(!map.contains_key("broken"));
}

#[test]
fn assemble_one_hundred_plus_keys_completes_without_error() {
    let keys: Vec<String> = (0..128).map(|i| format!("k{i}")).collect();
    let key_refs: Vec<&str> = keys.iter().map(String::as_str).collect();
    let values: Vec<Option<String>> = (0..128)
        .map(|i| {
            if i % 3 == 0 {
                None
            } else {
                Some(serde_json::to_string(&sample(&i.to_string(), "x")).expect("encode"))
            }
        })
        .collect();

    let map = assemble_batch_get_results::<TestCacheValue>(&key_refs, values);
    let expected = (0..128).filter(|i| i % 3 != 0).count();
    assert_eq!(map.len(), expected);
    assert!(!map.contains_key("k0"));
    assert_eq!(map.get("k1").map(|v| v.id.as_str()), Some("1"));
}

#[tokio::test]
async fn get_batch_empty_keys_does_not_need_redis() {
    // Pool is unused for the empty-key fast path of the shipped get_batch.
    if let Some(service) = try_redis_service().await {
        let map = service.get_batch::<TestCacheValue>(&[]).await;
        assert!(map.is_empty());
        assert!(service.set_batch::<TestCacheValue>(&[], None).await);
    } else {
        let map = assemble_batch_get_results::<TestCacheValue>(&[], Vec::new());
        assert!(map.is_empty());
    }
}

#[tokio::test]
async fn redis_roundtrip_skips_missing_keys_when_available() {
    let Some(service) = try_redis_service().await else {
        eprintln!("REDIS_URL not reachable; empty/zip/deserialize unit checks still ran");
        return;
    };

    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let present_key = format!("present-{suffix}");
    let missing_key = format!("missing-{suffix}");
    let value = sample("p", "payload");
    assert!(
        service
            .set(&present_key, value.clone(), Some(Duration::from_secs(30)))
            .await
    );

    let keys = [present_key.as_str(), missing_key.as_str()];
    let map = service.get_batch::<TestCacheValue>(&keys).await;
    assert_eq!(map.get(present_key.as_str()), Some(&value));
    assert!(!map.contains_key(missing_key.as_str()));

    let many: Vec<String> = (0..120).map(|i| format!("bulk-{suffix}-{i}")).collect();
    let pairs: Vec<(&str, TestCacheValue)> = many
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 2 == 0)
        .map(|(i, key)| (key.as_str(), sample(&i.to_string(), "bulk")))
        .collect();
    assert!(service.set_batch(&pairs, Some(Duration::from_secs(30))).await);
    let key_refs: Vec<&str> = many.iter().map(String::as_str).collect();
    let bulk = service.get_batch::<TestCacheValue>(&key_refs).await;
    assert_eq!(bulk.len(), 60);
}

#[test]
fn pipeline_and_shadow_flags_default_off() {
    let pipeline = std::env::var("REDIS_PIPELINE_ENABLED").ok();
    let shadow = std::env::var("ENABLE_SHADOW_MODE").ok();
    if pipeline.is_none() {
        assert!(!redis_pipeline_enabled());
    }
    if shadow.is_none() {
        assert!(!shadow_mode_enabled());
    }
}

async fn try_redis_service() -> Option<RedisCacheService> {
    let url = std::env::var("REDIS_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "redis://127.0.0.1:6379/".to_string());
    let config = RedisConfig {
        url,
        sentinel_urls: None,
        sentinel_master_name: "mymaster".to_string(),
    };
    match tokio::time::timeout(Duration::from_secs(2), create_redis_pool(&config)).await {
        Ok(Ok(pool)) => Some(RedisCacheService::new(
            pool,
            Duration::from_secs(30),
            format!("fms-pipeline-test:{}:", std::process::id()),
        )),
        _ => None,
    }
}
