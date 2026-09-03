use async_trait::async_trait;
use dashmap::DashMap;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::timeout;

use super::super::cache::RedisPool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NonceReplayDecision {
    FirstSeen,
    Replay,
}

#[derive(Debug, thiserror::Error)]
pub enum NonceReplayStoreError {
    #[error("Redis error: {0}")]
    Redis(String),
    #[error("Timeout")]
    Timeout,
    #[error("Internal error: {0}")]
    Internal(String),
}

#[async_trait]
pub trait NonceReplayStore: Send + Sync {
    async fn check_and_record(
        &self,
        session_hash: &str,
        timestamp: i64,
        nonce: &str,
    ) -> Result<NonceReplayDecision, NonceReplayStoreError>;
}

fn compute_time_bucket(timestamp: i64, bucket_secs: i64) -> i64 {
    timestamp / bucket_secs
}

fn default_ttl(max_timestamp_skew_secs: i64, bucket_secs: i64) -> i64 {
    max_timestamp_skew_secs * 2 + bucket_secs
}

// One Redis key per session+time-bucket. Members are the nonces seen in that
// window. This keeps GET anti-replay (one write per request) but avoids the
// previous "one key per nonce" expire-heap tax.
const LUA_SADD_EXPIRE: &str = r#"
local added = redis.call("SADD", KEYS[1], ARGV[1])
if added == 0 then
  return 0
end
redis.call("EXPIRE", KEYS[1], ARGV[2])
return 1
"#;

pub struct RedisBucketNonceStore {
    pool: RedisPool,
    bucket_secs: i64,
    timeout_ms: u64,
    policy: TimeoutPolicy,
    lua_sha: RwLock<Option<String>>,
}

pub enum TimeoutPolicy {
    FailClosed,
    FailOpen,
}

impl TimeoutPolicy {
    pub fn from_env() -> Self {
        match std::env::var("ANTI_REPLAY_REDIS_TIMEOUT_POLICY")
            .unwrap_or_else(|_| "fail_closed".to_string())
            .as_str()
        {
            "fail_open" => TimeoutPolicy::FailOpen,
            _ => TimeoutPolicy::FailClosed,
        }
    }
}

impl RedisBucketNonceStore {
    pub fn new(pool: RedisPool) -> Self {
        let bucket_secs: i64 = std::env::var("ANTI_REPLAY_BUCKET_SECS")
            .unwrap_or_else(|_| "10".to_string())
            .parse()
            .unwrap_or(10);
        let timeout_ms: u64 = std::env::var("ANTI_REPLAY_REDIS_TIMEOUT_MS")
            .unwrap_or_else(|_| "30".to_string())
            .parse()
            .unwrap_or(30);
        Self {
            pool,
            bucket_secs,
            timeout_ms,
            policy: TimeoutPolicy::from_env(),
            lua_sha: RwLock::new(None),
        }
    }

    fn build_bucket_key(session_hash: &str, time_bucket: i64) -> String {
        let mut hasher = Sha256::new();
        hasher.update(session_hash.as_bytes());
        hasher.update(b":");
        hasher.update(time_bucket.to_string().as_bytes());
        format!("fms:anti_replay:bucket:{}", hex::encode(hasher.finalize()))
    }

    async fn load_or_refresh_script(&self) -> Result<String, NonceReplayStoreError> {
        {
            let sha = self.lua_sha.read().await;
            if let Some(sha) = sha.as_ref() {
                return Ok(sha.clone());
            }
        }

        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| NonceReplayStoreError::Redis(e.to_string()))?;

        let sha: String = redis::cmd("SCRIPT")
            .arg("LOAD")
            .arg(LUA_SADD_EXPIRE)
            .query_async(&mut *conn)
            .await
            .map_err(|e| NonceReplayStoreError::Redis(e.to_string()))?;

        {
            let mut sha_cache = self.lua_sha.write().await;
            *sha_cache = Some(sha.clone());
        }

        Ok(sha)
    }

    async fn exec_lua_cmd(
        &self,
        conn: &mut redis::aio::MultiplexedConnection,
        sha: &str,
        key: &str,
        nonce: &str,
        ttl: i64,
    ) -> Result<i32, NonceReplayStoreError> {
        let result: Result<i32, redis::RedisError> = redis::cmd("EVALSHA")
            .arg(sha)
            .arg(1)
            .arg(key)
            .arg(nonce)
            .arg(ttl)
            .query_async(conn)
            .await;

        match result {
            Ok(val) => Ok(val),
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("NOSCRIPT") {
                    let mut sha_cache = self.lua_sha.write().await;
                    *sha_cache = None;
                }
                Err(NonceReplayStoreError::Redis(err_str))
            }
        }
    }
}

#[async_trait]
impl NonceReplayStore for RedisBucketNonceStore {
    async fn check_and_record(
        &self,
        session_hash: &str,
        timestamp: i64,
        nonce: &str,
    ) -> Result<NonceReplayDecision, NonceReplayStoreError> {
        const MAX_NOSCRIPT_RETRIES: u32 = 3;

        let time_bucket = compute_time_bucket(timestamp, self.bucket_secs);
        let key = Self::build_bucket_key(session_hash, time_bucket);
        let ttl = default_ttl(120, self.bucket_secs);

        let mut noscript_retries: u32 = 0;
        loop {
            let mut conn = match self.pool.get().await {
                Ok(c) => c,
                Err(e) => {
                    return Err(NonceReplayStoreError::Redis(e.to_string()));
                }
            };

            let sha = match self.load_or_refresh_script().await {
                Ok(s) => s,
                Err(e) => return Err(e),
            };

            let lua_result = timeout(
                Duration::from_millis(self.timeout_ms),
                self.exec_lua_cmd(&mut conn, &sha, &key, nonce, ttl),
            )
            .await;

            match lua_result {
                Ok(Ok(1)) => return Ok(NonceReplayDecision::FirstSeen),
                Ok(Ok(0)) => return Ok(NonceReplayDecision::Replay),
                Ok(Ok(other)) => {
                    tracing::warn!("unexpected EVALSHA return value: {}", other);
                    return Ok(NonceReplayDecision::FirstSeen);
                }
                Ok(Err(NonceReplayStoreError::Redis(ref msg)))
                    if msg.contains("NOSCRIPT") && noscript_retries < MAX_NOSCRIPT_RETRIES =>
                {
                    noscript_retries += 1;
                    let mut sha_cache = self.lua_sha.write().await;
                    *sha_cache = None;
                    drop(sha_cache);
                    tracing::debug!("NOSCRIPT retry {}/{}", noscript_retries, MAX_NOSCRIPT_RETRIES);
                    continue;
                }
                Ok(Err(e)) => match self.policy {
                    TimeoutPolicy::FailClosed => return Err(e),
                    TimeoutPolicy::FailOpen => {
                        tracing::warn!(error = %e, "anti-replay store error, fail open");
                        return Ok(NonceReplayDecision::FirstSeen);
                    }
                },
                Err(_) => match self.policy {
                    TimeoutPolicy::FailClosed => return Err(NonceReplayStoreError::Timeout),
                    TimeoutPolicy::FailOpen => {
                        tracing::warn!("anti-replay store timeout, fail open");
                        return Ok(NonceReplayDecision::FirstSeen);
                    }
                },
            }
        }
    }
}

#[async_trait]
impl fms_domain::ports::nonce_replay_store::NonceReplayStore for RedisBucketNonceStore {
    async fn check_and_record(
        &self,
        session_hash: &str,
        timestamp: i64,
        nonce: &str,
    ) -> Result<
        fms_domain::ports::nonce_replay_store::NonceReplayDecision,
        fms_domain::ports::nonce_replay_store::NonceReplayStoreError,
    > {
        match <Self as NonceReplayStore>::check_and_record(self, session_hash, timestamp, nonce).await {
            Ok(NonceReplayDecision::FirstSeen) => {
                Ok(fms_domain::ports::nonce_replay_store::NonceReplayDecision::FirstSeen)
            }
            Ok(NonceReplayDecision::Replay) => Ok(fms_domain::ports::nonce_replay_store::NonceReplayDecision::Replay),
            Err(NonceReplayStoreError::Timeout) => {
                Err(fms_domain::ports::nonce_replay_store::NonceReplayStoreError::Timeout)
            }
            Err(error) => {
                Err(fms_domain::ports::nonce_replay_store::NonceReplayStoreError::Unavailable(error.to_string()))
            }
        }
    }
}

struct LocalNonceBucket {
    nonces: HashSet<String>,
    expires_at: Instant,
}

pub struct LocalTtlNonceStore {
    cache: DashMap<String, LocalNonceBucket>,
    max_entries_per_bucket: usize,
    bucket_secs: i64,
    inserts_since_cleanup: AtomicU64,
}

impl LocalTtlNonceStore {
    pub fn new(max_entries_per_bucket: usize, bucket_secs: i64) -> Self {
        Self {
            cache: DashMap::new(),
            max_entries_per_bucket,
            bucket_secs,
            inserts_since_cleanup: AtomicU64::new(0),
        }
    }

    fn bucket_ttl_secs(&self, max_skew: i64) -> i64 {
        default_ttl(max_skew, self.bucket_secs)
    }

    fn maybe_cleanup(&self, now: Instant) {
        if self
            .inserts_since_cleanup
            .fetch_add(1, Ordering::Relaxed)
            .is_multiple_of(4096)
        {
            self.cache.retain(|_, bucket| now < bucket.expires_at);
        }
    }
}

#[async_trait]
impl NonceReplayStore for LocalTtlNonceStore {
    async fn check_and_record(
        &self,
        session_hash: &str,
        timestamp: i64,
        nonce: &str,
    ) -> Result<NonceReplayDecision, NonceReplayStoreError> {
        let time_bucket = compute_time_bucket(timestamp, self.bucket_secs);
        let key = format!("{}:{}", session_hash, time_bucket);
        let ttl = Duration::from_secs(self.bucket_ttl_secs(120) as u64);
        let now = Instant::now();
        self.maybe_cleanup(now);

        let mut bucket = self.cache.entry(key).or_insert_with(|| LocalNonceBucket {
            nonces: HashSet::new(),
            expires_at: now + ttl,
        });
        if now >= bucket.expires_at {
            bucket.nonces.clear();
            bucket.expires_at = now + ttl;
        }
        if bucket.nonces.len() >= self.max_entries_per_bucket {
            return Err(NonceReplayStoreError::Internal("local nonce store bucket full".into()));
        }
        if !bucket.nonces.insert(nonce.to_string()) {
            return Ok(NonceReplayDecision::Replay);
        }
        Ok(NonceReplayDecision::FirstSeen)
    }
}

#[async_trait]
impl fms_domain::ports::nonce_replay_store::NonceReplayStore for LocalTtlNonceStore {
    async fn check_and_record(
        &self,
        session_hash: &str,
        timestamp: i64,
        nonce: &str,
    ) -> Result<
        fms_domain::ports::nonce_replay_store::NonceReplayDecision,
        fms_domain::ports::nonce_replay_store::NonceReplayStoreError,
    > {
        match <Self as NonceReplayStore>::check_and_record(self, session_hash, timestamp, nonce).await {
            Ok(NonceReplayDecision::FirstSeen) => {
                Ok(fms_domain::ports::nonce_replay_store::NonceReplayDecision::FirstSeen)
            }
            Ok(NonceReplayDecision::Replay) => Ok(fms_domain::ports::nonce_replay_store::NonceReplayDecision::Replay),
            Err(NonceReplayStoreError::Timeout) => {
                Err(fms_domain::ports::nonce_replay_store::NonceReplayStoreError::Timeout)
            }
            Err(error) => {
                Err(fms_domain::ports::nonce_replay_store::NonceReplayStoreError::Unavailable(error.to_string()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_store_first_seen_returns_first_seen() {
        let store = LocalTtlNonceStore::new(1000, 10);
        let result = store.check_and_record("session1", 1000, "nonce-abc").await.unwrap();
        assert_eq!(result, NonceReplayDecision::FirstSeen);
    }

    #[tokio::test]
    async fn local_store_duplicate_nonce_returns_replay() {
        let store = LocalTtlNonceStore::new(1000, 10);
        store.check_and_record("session1", 1000, "nonce-abc").await.unwrap();
        let result = store.check_and_record("session1", 1000, "nonce-abc").await.unwrap();
        assert_eq!(result, NonceReplayDecision::Replay);
    }

    #[tokio::test]
    async fn local_store_different_nonces_in_same_bucket_are_independent() {
        let store = LocalTtlNonceStore::new(1000, 10);
        store.check_and_record("session1", 1000, "nonce-a").await.unwrap();
        let result = store.check_and_record("session1", 1000, "nonce-b").await.unwrap();
        assert_eq!(result, NonceReplayDecision::FirstSeen);
    }

    #[tokio::test]
    async fn local_store_different_sessions_same_nonce_are_independent() {
        let store = LocalTtlNonceStore::new(1000, 10);
        store.check_and_record("session1", 1000, "nonce-abc").await.unwrap();
        let result = store.check_and_record("session2", 1000, "nonce-abc").await.unwrap();
        assert_eq!(result, NonceReplayDecision::FirstSeen);
    }

    #[tokio::test]
    async fn local_store_different_buckets_same_nonce_are_independent() {
        let store = LocalTtlNonceStore::new(1000, 10);
        store.check_and_record("session1", 1000, "nonce-abc").await.unwrap();
        // Different time bucket (bucket_secs=10, so bucket = timestamp/10)
        let result = store.check_and_record("session1", 1010, "nonce-abc").await.unwrap();
        assert_eq!(result, NonceReplayDecision::FirstSeen);
    }

    #[test]
    fn redis_bucket_key_is_stable_per_session_and_bucket() {
        let first = RedisBucketNonceStore::build_bucket_key("session1", 100);
        let second = RedisBucketNonceStore::build_bucket_key("session1", 100);
        assert_eq!(first, second);
        assert!(first.starts_with("fms:anti_replay:bucket:"));
    }

    #[test]
    fn redis_bucket_key_is_shared_for_nonces_in_the_same_bucket() {
        let first = RedisBucketNonceStore::build_bucket_key("session1", 100);
        let second = RedisBucketNonceStore::build_bucket_key("session1", 100);
        assert_eq!(first, second);
    }

    #[test]
    fn redis_bucket_key_differs_across_sessions_or_buckets() {
        let base = RedisBucketNonceStore::build_bucket_key("session1", 100);
        let other_session = RedisBucketNonceStore::build_bucket_key("session2", 100);
        let other_bucket = RedisBucketNonceStore::build_bucket_key("session1", 101);
        assert_ne!(base, other_session);
        assert_ne!(base, other_bucket);
    }

    #[test]
    fn lua_script_uses_sadd_not_per_nonce_set() {
        assert!(LUA_SADD_EXPIRE.contains("SADD"));
        assert!(LUA_SADD_EXPIRE.contains("EXPIRE"));
        assert!(!LUA_SADD_EXPIRE.contains("SET"));
    }

    #[tokio::test]
    async fn local_store_records_unique_nonces_under_concurrency() {
        let store = std::sync::Arc::new(LocalTtlNonceStore::new(100_000, 10));
        let mut tasks = Vec::new();
        for index in 0..256 {
            let store = std::sync::Arc::clone(&store);
            tasks.push(tokio::spawn(async move {
                store
                    .check_and_record("session-concurrent", 1_000, &format!("nonce-{index}"))
                    .await
                    .unwrap()
            }));
        }
        let mut first_seen = 0;
        for task in tasks {
            match task.await.unwrap() {
                NonceReplayDecision::FirstSeen => first_seen += 1,
                NonceReplayDecision::Replay => panic!("unique nonce marked replay"),
            }
        }
        assert_eq!(first_seen, 256);
        let replay = store
            .check_and_record("session-concurrent", 1_000, "nonce-0")
            .await
            .unwrap();
        assert_eq!(replay, NonceReplayDecision::Replay);
    }
}
