use async_trait::async_trait;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
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

const LUA_SET_NX_EX: &str = r#"
local added = redis.call("SET", KEYS[1], "1", "NX", "EX", ARGV[1])
if added then
  return 1
end
return 0
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

    fn build_nonce_key(session_hash: &str, time_bucket: i64, nonce: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(session_hash.as_bytes());
        hasher.update(b":");
        hasher.update(time_bucket.to_string().as_bytes());
        hasher.update(b":");
        hasher.update(nonce.as_bytes());
        format!("fms:anti_replay:nonce:{}", hex::encode(hasher.finalize()))
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
            .arg(LUA_SET_NX_EX)
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
        ttl: i64,
    ) -> Result<i32, NonceReplayStoreError> {
        let result: Result<i32, redis::RedisError> = redis::cmd("EVALSHA")
            .arg(sha)
            .arg(1)
            .arg(key)
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
        let key = Self::build_nonce_key(session_hash, time_bucket, nonce);
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
                self.exec_lua_cmd(&mut *conn, &sha, &key, ttl),
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

#[derive(Clone)]
struct LocalNonceBucket {
    nonces: HashSet<String>,
    expires_at: Instant,
}

pub struct LocalTtlNonceStore {
    cache: Arc<RwLock<HashMap<String, LocalNonceBucket>>>,
    max_entries_per_bucket: usize,
    bucket_secs: i64,
}

impl LocalTtlNonceStore {
    pub fn new(max_entries_per_bucket: usize, bucket_secs: i64) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            max_entries_per_bucket,
            bucket_secs,
        }
    }

    fn bucket_ttl_secs(&self, max_skew: i64) -> i64 {
        default_ttl(max_skew, self.bucket_secs)
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

        let mut cache = self.cache.write().await;
        let now = Instant::now();
        cache.retain(|_, bucket| now < bucket.expires_at);

        let bucket = cache.entry(key).or_insert_with(|| LocalNonceBucket {
            nonces: HashSet::new(),
            expires_at: now + ttl,
        });

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
    fn redis_nonce_key_is_stable_per_session_bucket_nonce() {
        let first = RedisBucketNonceStore::build_nonce_key("session1", 100, "nonce-abc");
        let second = RedisBucketNonceStore::build_nonce_key("session1", 100, "nonce-abc");
        assert_eq!(first, second);
    }

    #[test]
    fn redis_nonce_key_is_distributed_by_nonce() {
        let first = RedisBucketNonceStore::build_nonce_key("session1", 100, "nonce-a");
        let second = RedisBucketNonceStore::build_nonce_key("session1", 100, "nonce-b");
        assert_ne!(first, second);
    }
}
