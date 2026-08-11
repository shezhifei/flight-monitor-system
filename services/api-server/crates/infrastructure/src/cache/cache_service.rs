//! 通用缓存服务

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, warn};

use super::RedisPool;

/// 缓存条目
#[derive(Debug, Clone)]
struct CacheEntry<T: Clone> {
    value: T,
    expires_at: Instant,
    last_accessed: u64,
}

impl<T: Clone> CacheEntry<T> {
    fn new(value: T, ttl: Duration, last_accessed: u64) -> Self {
        Self {
            value,
            expires_at: Instant::now() + ttl,
            last_accessed,
        }
    }

    fn is_expired(&self) -> bool {
        Instant::now() > self.expires_at
    }

    fn touch(&mut self, last_accessed: u64) {
        self.last_accessed = last_accessed;
    }
}

/// 缓存服务接口
#[async_trait]
pub trait CacheService: Send + Sync {
    /// 获取缓存值
    async fn get<T: for<'de> Deserialize<'de> + Serialize + Clone + Send + 'static>(&self, key: &str) -> Option<T>;

    /// 设置缓存值
    async fn set<T: Serialize + Clone + Send + 'static>(&self, key: &str, value: T, ttl: Option<Duration>) -> bool;

    /// 删除缓存
    async fn delete(&self, key: &str) -> bool;

    /// 检查缓存是否存在
    async fn exists(&self, key: &str) -> bool;

    /// 清空所有缓存
    async fn clear(&self) -> bool;
}

/// 缓存查询结果，可区分未命中和已缓存的空结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheLookup<T> {
    Hit(T),
    NegativeHit,
    Miss,
}

/// 本地内存缓存服务实现
pub struct LocalCacheService {
    cache: Arc<RwLock<HashMap<String, CacheEntry<String>>>>,
    access_counter: Arc<AtomicU64>,
    max_entries: usize,
    default_ttl: Duration,
}

impl LocalCacheService {
    /// 创建新的本地缓存服务
    pub fn new(max_entries: usize, default_ttl: Duration) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            access_counter: Arc::new(AtomicU64::new(0)),
            max_entries,
            default_ttl,
        }
    }

    fn next_access_tick(&self) -> u64 {
        self.access_counter.fetch_add(1, Ordering::Relaxed) + 1
    }

    fn least_recently_used_key(cache: &HashMap<String, CacheEntry<String>>) -> Option<String> {
        cache
            .iter()
            .min_by_key(|(_, entry)| entry.last_accessed)
            .map(|(key, _)| key.clone())
    }

    async fn set_json_value(&self, key: &str, json_value: String, ttl: Option<Duration>) -> bool {
        self.maybe_cleanup().await;

        if self.max_entries == 0 {
            warn!(key = key, "本地缓存容量为 0，跳过写入");
            return false;
        }

        let ttl = ttl.unwrap_or(self.default_ttl);
        let entry = CacheEntry::new(json_value, ttl, self.next_access_tick());
        let mut cache = self.cache.write().await;

        if cache.len() >= self.max_entries && !cache.contains_key(key) {
            if let Some(lru_key) = Self::least_recently_used_key(&cache) {
                cache.remove(&lru_key);
            }
        }

        cache.insert(key.to_string(), entry);
        true
    }

    /// 获取缓存值，并区分普通未命中和已缓存的空结果
    pub async fn get_lookup<T: for<'de> Deserialize<'de> + Serialize + Clone + Send + 'static>(
        &self,
        key: &str,
    ) -> CacheLookup<T> {
        let mut cache = self.cache.write().await;

        let Some(entry) = cache.get_mut(key) else {
            return CacheLookup::Miss;
        };

        if entry.is_expired() {
            cache.remove(key);
            return CacheLookup::Miss;
        }

        let raw_value = entry.value.clone();
        entry.touch(self.next_access_tick());
        drop(cache);

        if raw_value == "null" {
            return CacheLookup::NegativeHit;
        }

        match serde_json::from_str::<T>(&raw_value) {
            Ok(value) => CacheLookup::Hit(value),
            Err(e) => {
                warn!(key = key, error = %e, "缓存值反序列化失败");
                CacheLookup::Miss
            }
        }
    }

    /// 写入负缓存条目，用于短 TTL 缓存底层确认不存在的结果
    pub async fn set_negative(&self, key: &str, ttl: Option<Duration>) -> bool {
        self.set_json_value(key, "null".to_string(), ttl).await
    }

    /// 清理过期条目
    async fn cleanup_expired(&self) {
        let mut cache = self.cache.write().await;
        let before_count = cache.len();
        cache.retain(|_, entry| !entry.is_expired());
        let after_count = cache.len();

        if before_count != after_count {
            debug!(before = before_count, after = after_count, "清理过期缓存条目");
        }
    }

    /// 检查是否需要清理
    async fn maybe_cleanup(&self) {
        let cache = self.cache.read().await;
        if cache.len() > self.max_entries * 3 / 4 {
            drop(cache);
            self.cleanup_expired().await;
        }
    }
}

#[async_trait]
impl CacheService for LocalCacheService {
    async fn get<T: for<'de> Deserialize<'de> + Serialize + Clone + Send + 'static>(&self, key: &str) -> Option<T> {
        let mut cache = self.cache.write().await;

        if let Some(entry) = cache.get_mut(key) {
            if entry.is_expired() {
                cache.remove(key);
                return None;
            }

            let raw_value = entry.value.clone();
            entry.touch(self.next_access_tick());
            drop(cache);

            match serde_json::from_str::<T>(&raw_value) {
                Ok(value) => Some(value),
                Err(e) => {
                    warn!(key = key, error = %e, "缓存值反序列化失败");
                    None
                }
            }
        } else {
            None
        }
    }

    async fn set<T: Serialize + Clone + Send + 'static>(&self, key: &str, value: T, ttl: Option<Duration>) -> bool {
        let json_value = match serde_json::to_string(&value) {
            Ok(v) => v,
            Err(e) => {
                warn!(key = key, error = %e, "缓存值序列化失败");
                return false;
            }
        };

        self.set_json_value(key, json_value, ttl).await
    }

    async fn delete(&self, key: &str) -> bool {
        let mut cache = self.cache.write().await;
        cache.remove(key).is_some()
    }

    async fn exists(&self, key: &str) -> bool {
        let cache = self.cache.read().await;

        if let Some(entry) = cache.get(key) {
            !entry.is_expired()
        } else {
            false
        }
    }

    async fn clear(&self) -> bool {
        let mut cache = self.cache.write().await;
        cache.clear();
        true
    }
}

/// Redis 缓存服务实现
pub struct RedisCacheService {
    pool: RedisPool,
    default_ttl: Duration,
    key_prefix: String,
}

impl RedisCacheService {
    /// 创建新的 Redis 缓存服务
    pub fn new(pool: RedisPool, default_ttl: Duration, key_prefix: String) -> Self {
        Self {
            pool,
            default_ttl,
            key_prefix,
        }
    }

    /// 构建完整的缓存键
    fn full_key(&self, key: &str) -> String {
        format!("{}{}", self.key_prefix, key)
    }
}

#[async_trait]
impl CacheService for RedisCacheService {
    async fn get<T: for<'de> Deserialize<'de> + Serialize + Clone + Send + 'static>(&self, key: &str) -> Option<T> {
        let full_key = self.full_key(key);

        let mut conn = match self.pool.get().await {
            Ok(c) => c,
            Err(e) => {
                warn!(key = key, error = %e, "获取 Redis 连接失败");
                return None;
            }
        };

        let result = redis::cmd("GET")
            .arg(&full_key)
            .query_async::<Option<String>>(&mut *conn)
            .await;

        match &result {
            Ok(_) => super::record_redis_command("GET", "success"),
            Err(_) => super::record_redis_command("GET", "error"),
        }

        match result {
            Ok(Some(json_str)) => match serde_json::from_str::<T>(&json_str) {
                Ok(value) => Some(value),
                Err(e) => {
                    warn!(key = key, error = %e, "Redis 缓存值反序列化失败");
                    None
                }
            },
            Ok(None) => None,
            Err(e) => {
                warn!(key = key, error = %e, "Redis GET 操作失败");
                None
            }
        }
    }

    async fn set<T: Serialize + Clone + Send + 'static>(&self, key: &str, value: T, ttl: Option<Duration>) -> bool {
        let full_key = self.full_key(key);
        let ttl_secs = ttl.unwrap_or(self.default_ttl).as_secs();

        let mut conn = match self.pool.get().await {
            Ok(c) => c,
            Err(e) => {
                warn!(key = key, error = %e, "获取 Redis 连接失败");
                return false;
            }
        };

        let json_value = match serde_json::to_string(&value) {
            Ok(v) => v,
            Err(e) => {
                warn!(key = key, error = %e, "缓存值序列化失败");
                return false;
            }
        };

        let result = redis::cmd("SETEX")
            .arg(&full_key)
            .arg(ttl_secs)
            .arg(&json_value)
            .query_async::<()>(&mut *conn)
            .await;

        match &result {
            Ok(_) => super::record_redis_command("SETEX", "success"),
            Err(_) => super::record_redis_command("SETEX", "error"),
        }

        match result {
            Ok(_) => true,
            Err(e) => {
                warn!(key = key, error = %e, "Redis SETEX 操作失败");
                false
            }
        }
    }

    async fn delete(&self, key: &str) -> bool {
        let full_key = self.full_key(key);

        let mut conn = match self.pool.get().await {
            Ok(c) => c,
            Err(e) => {
                warn!(key = key, error = %e, "获取 Redis 连接失败");
                return false;
            }
        };

        let result = redis::cmd("DEL").arg(&full_key).query_async::<()>(&mut *conn).await;

        match &result {
            Ok(_) => super::record_redis_command("DEL", "success"),
            Err(_) => super::record_redis_command("DEL", "error"),
        }

        match result {
            Ok(_) => true,
            Err(e) => {
                warn!(key = key, error = %e, "Redis DEL 操作失败");
                false
            }
        }
    }

    async fn exists(&self, key: &str) -> bool {
        let full_key = self.full_key(key);

        let mut conn = match self.pool.get().await {
            Ok(c) => c,
            Err(e) => {
                warn!(key = key, error = %e, "获取 Redis 连接失败");
                return false;
            }
        };

        let result = redis::cmd("EXISTS")
            .arg(&full_key)
            .query_async::<bool>(&mut *conn)
            .await;

        match &result {
            Ok(_) => super::record_redis_command("EXISTS", "success"),
            Err(_) => super::record_redis_command("EXISTS", "error"),
        }

        match result {
            Ok(exists) => exists,
            Err(e) => {
                warn!(key = key, error = %e, "Redis EXISTS 操作失败");
                false
            }
        }
    }

    async fn clear(&self) -> bool {
        // Redis 不支持通配符删除，需要在业务层处理
        warn!("Redis 缓存服务不支持 clear 操作，请使用带前缀的键管理");
        false
    }
}

/// 多级缓存服务（本地缓存 + Redis 缓存）
pub struct MultiLevelCacheService {
    local: LocalCacheService,
    redis: Option<RedisCacheService>,
}

impl MultiLevelCacheService {
    /// 创建多级缓存服务
    pub fn new(
        local_max_entries: usize,
        local_ttl: Duration,
        redis_pool: Option<RedisPool>,
        redis_ttl: Duration,
        redis_key_prefix: String,
    ) -> Self {
        let local = LocalCacheService::new(local_max_entries, local_ttl);
        let redis = redis_pool.map(|pool| RedisCacheService::new(pool, redis_ttl, redis_key_prefix));

        Self { local, redis }
    }
}

#[async_trait]
impl CacheService for MultiLevelCacheService {
    async fn get<T: for<'de> Deserialize<'de> + Serialize + Clone + Send + 'static>(&self, key: &str) -> Option<T> {
        // 先尝试本地缓存
        if let Some(value) = self.local.get::<T>(key).await {
            debug!(key = key, "本地缓存命中");
            return Some(value);
        }

        // 再尝试 Redis 缓存
        if let Some(redis) = &self.redis {
            if let Some(value) = redis.get::<T>(key).await {
                debug!(key = key, "Redis 缓存命中，回填本地缓存");
                // 回填本地缓存
                self.local.set(key, value.clone(), None).await;
                return Some(value);
            }
        }

        None
    }

    async fn set<T: Serialize + Clone + Send + 'static>(&self, key: &str, value: T, ttl: Option<Duration>) -> bool {
        // 同时写入本地缓存和 Redis 缓存
        let local_result = self.local.set(key, value.clone(), ttl).await;

        let redis_result = if let Some(redis) = &self.redis {
            redis.set(key, value, ttl).await
        } else {
            true
        };

        local_result && redis_result
    }

    async fn delete(&self, key: &str) -> bool {
        let local_result = self.local.delete(key).await;

        let redis_result = if let Some(redis) = &self.redis {
            redis.delete(key).await
        } else {
            true
        };

        local_result || redis_result
    }

    async fn exists(&self, key: &str) -> bool {
        if self.local.exists(key).await {
            return true;
        }

        if let Some(redis) = &self.redis {
            redis.exists(key).await
        } else {
            false
        }
    }

    async fn clear(&self) -> bool {
        self.local.clear().await;

        if let Some(redis) = &self.redis {
            redis.clear().await;
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::{CacheLookup, CacheService, LocalCacheService};
    use std::time::Duration;

    #[tokio::test]
    async fn local_cache_eviction_keeps_recently_accessed_entry() {
        let cache = LocalCacheService::new(2, Duration::from_secs(60));

        assert!(cache.set("alpha", "alpha-value", None).await);
        assert!(cache.set("bravo", "bravo-value", None).await);

        let protected_key = {
            let cache_entries = cache.cache.read().await;
            cache_entries
                .keys()
                .next()
                .expect("test cache should contain entries")
                .to_string()
        };
        let evictable_key = if protected_key == "alpha" { "bravo" } else { "alpha" };
        let protected_value = format!("{protected_key}-value");

        assert_eq!(cache.get::<String>(&protected_key).await, Some(protected_value.clone()));

        assert!(cache.set("charlie", "charlie-value", None).await);

        assert_eq!(cache.get::<String>(&protected_key).await, Some(protected_value));
        assert_eq!(cache.get::<String>(evictable_key).await, None);
        assert_eq!(cache.get::<String>("charlie").await, Some("charlie-value".to_string()));
    }

    #[tokio::test]
    async fn local_cache_negative_entry_distinguishes_cached_absence_from_miss() {
        let cache = LocalCacheService::new(2, Duration::from_secs(60));

        assert_eq!(cache.get_lookup::<String>("missing").await, CacheLookup::Miss);

        assert!(cache.set_negative("missing", Some(Duration::from_secs(5))).await);

        assert_eq!(cache.get_lookup::<String>("missing").await, CacheLookup::NegativeHit);
        assert!(cache.exists("missing").await);
    }
}
