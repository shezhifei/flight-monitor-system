use crate::schemas::auth_schemas::TokenData;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::RwLock;
use std::time::{Duration, Instant};

const DEFAULT_FRESHNESS_CACHE_TTL_MS: u64 = 2000;
const DEFAULT_PERMISSION_CACHE_TTL_MS: u64 = 2000;
const DEFAULT_CLAIMS_CACHE_TTL_MS: u64 = 2000;

struct CacheEntry<T: Clone> {
    value: T,
    expires_at: Instant,
}

impl<T: Clone> CacheEntry<T> {
    fn is_valid(&self) -> bool {
        Instant::now() < self.expires_at
    }

    fn is_valid_at(&self, now: Instant) -> bool {
        now < self.expires_at
    }
}

pub struct AuthValidationCache {
    freshness_cache: RwLock<LruCache<String, CacheEntry<bool>>>,
    permission_cache: RwLock<LruCache<String, CacheEntry<bool>>>,
    claims_cache: RwLock<LruCache<String, CacheEntry<TokenData>>>,
    freshness_ttl: Duration,
    permission_ttl: Duration,
    claims_ttl: Duration,
}

impl Default for AuthValidationCache {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthValidationCache {
    pub fn new() -> Self {
        let freshness_ttl_ms: u64 = std::env::var("AUTH_FRESHNESS_CACHE_TTL_MS")
            .unwrap_or_else(|_| DEFAULT_FRESHNESS_CACHE_TTL_MS.to_string())
            .parse()
            .unwrap_or(DEFAULT_FRESHNESS_CACHE_TTL_MS);
        let permission_ttl_ms: u64 = std::env::var("AUTH_PERMISSION_VERSION_CACHE_TTL_MS")
            .unwrap_or_else(|_| DEFAULT_PERMISSION_CACHE_TTL_MS.to_string())
            .parse()
            .unwrap_or(DEFAULT_PERMISSION_CACHE_TTL_MS);
        let claims_ttl_ms: u64 = std::env::var("AUTH_CLAIMS_CACHE_TTL_MS")
            .unwrap_or_else(|_| DEFAULT_CLAIMS_CACHE_TTL_MS.to_string())
            .parse()
            .unwrap_or(DEFAULT_CLAIMS_CACHE_TTL_MS);
        let max_entries: usize = std::env::var("AUTH_CACHE_MAX_ENTRIES")
            .unwrap_or_else(|_| "50000".to_string())
            .parse()
            .unwrap_or(50000);
        let cap = NonZeroUsize::new(max_entries).unwrap_or(NonZeroUsize::new(50000).unwrap());

        Self {
            freshness_cache: RwLock::new(LruCache::new(cap)),
            permission_cache: RwLock::new(LruCache::new(cap)),
            claims_cache: RwLock::new(LruCache::new(cap)),
            freshness_ttl: Duration::from_millis(freshness_ttl_ms),
            permission_ttl: Duration::from_millis(permission_ttl_ms),
            claims_ttl: Duration::from_millis(claims_ttl_ms),
        }
    }

    pub fn get_cached_claims(&self, token_hash: &str) -> Option<TokenData> {
        let cache = self
            .claims_cache
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match cache.peek(token_hash) {
            Some(entry) if entry.is_valid() => Some(entry.value.clone()),
            _ => None,
        }
    }

    pub fn set_cached_claims(&self, token_hash: &str, claims: TokenData) {
        if token_hash.is_empty() {
            return;
        }
        let mut cache = self
            .claims_cache
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        put_with_expiration_guard(
            &mut cache,
            token_hash.to_string(),
            CacheEntry {
                value: claims,
                expires_at: Instant::now() + self.claims_ttl,
            },
        );
    }

    pub async fn get_cached_freshness(&self, user_id: &str, session_key: &str) -> Option<bool> {
        let cache_key = format!("{}:{}", user_id, session_key);
        let cache = self
            .freshness_cache
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match cache.peek(&cache_key) {
            Some(entry) if entry.is_valid() => Some(entry.value),
            _ => None,
        }
    }

    pub async fn set_cached_freshness(&self, user_id: &str, session_key: &str, valid: bool) {
        if !valid {
            return;
        }
        let cache_key = format!("{}:{}", user_id, session_key);
        let mut cache = self
            .freshness_cache
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        put_with_expiration_guard(
            &mut cache,
            cache_key,
            CacheEntry {
                value: valid,
                expires_at: Instant::now() + self.freshness_ttl,
            },
        );
    }

    pub async fn invalidate_freshness(&self, user_id: &str) {
        let mut cache = self
            .freshness_cache
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let prefix = format!("{}:", user_id);
        let keys: Vec<String> = cache
            .iter()
            .filter(|(k, _)| k.starts_with(&prefix))
            .map(|(k, _)| k.clone())
            .collect();
        for key in keys {
            cache.pop(&key);
        }
    }

    pub async fn get_cached_permission(&self, user_id: &str, permission_version: i64) -> Option<bool> {
        let key = format!("{}:{}", user_id, permission_version);
        let cache = self
            .permission_cache
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match cache.peek(&key) {
            Some(entry) if entry.is_valid() => Some(entry.value),
            _ => None,
        }
    }

    pub async fn set_cached_permission(&self, user_id: &str, permission_version: i64, valid: bool) {
        if !valid {
            return;
        }
        let key = format!("{}:{}", user_id, permission_version);
        let mut cache = self
            .permission_cache
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        put_with_expiration_guard(
            &mut cache,
            key,
            CacheEntry {
                value: valid,
                expires_at: Instant::now() + self.permission_ttl,
            },
        );
    }

    pub async fn invalidate_permission(&self, user_id: &str) {
        let mut cache = self
            .permission_cache
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let prefix = format!("{}:", user_id);
        let keys: Vec<String> = cache
            .iter()
            .filter(|(k, _)| k.starts_with(&prefix))
            .map(|(k, _)| k.clone())
            .collect();
        for key in keys {
            cache.pop(&key);
        }
    }
}

fn put_with_expiration_guard<T: Clone>(cache: &mut LruCache<String, CacheEntry<T>>, key: String, entry: CacheEntry<T>) {
    if cache.len() >= cache.cap().get() && !cache.contains(&key) {
        remove_expired_entries(cache, Instant::now());
        if cache.len() >= cache.cap().get() {
            cache.pop_lru();
        }
    }
    cache.put(key, entry);
}

fn remove_expired_entries<T: Clone>(cache: &mut LruCache<String, CacheEntry<T>>, now: Instant) {
    let expired_keys: Vec<String> = cache
        .iter()
        .filter(|(_, entry)| !entry.is_valid_at(now))
        .map(|(key, _)| key.clone())
        .collect();

    for key in expired_keys {
        cache.pop(&key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cache(max_entries: usize, freshness_ttl_ms: u64, permission_ttl_ms: u64) -> AuthValidationCache {
        let cap = NonZeroUsize::new(max_entries).unwrap();
        AuthValidationCache {
            freshness_cache: RwLock::new(LruCache::new(cap)),
            permission_cache: RwLock::new(LruCache::new(cap)),
            claims_cache: RwLock::new(LruCache::new(cap)),
            freshness_ttl: Duration::from_millis(freshness_ttl_ms),
            permission_ttl: Duration::from_millis(permission_ttl_ms),
            claims_ttl: Duration::from_millis(freshness_ttl_ms),
        }
    }

    #[tokio::test]
    async fn freshness_cache_hit_on_same_user_and_session() {
        let cache = make_cache(1000, 5000, 5000);
        cache.set_cached_freshness("user1", "session-a", true).await;
        assert_eq!(cache.get_cached_freshness("user1", "session-a").await, Some(true));
    }

    #[tokio::test]
    async fn freshness_cache_miss_on_different_session() {
        let cache = make_cache(1000, 5000, 5000);
        cache.set_cached_freshness("user1", "session-a", true).await;
        assert_eq!(cache.get_cached_freshness("user1", "session-b").await, None);
    }

    #[tokio::test]
    async fn freshness_cache_does_not_cache_invalid() {
        let cache = make_cache(1000, 5000, 5000);
        cache.set_cached_freshness("user1", "session-a", false).await;
        assert_eq!(cache.get_cached_freshness("user1", "session-a").await, None);
    }

    #[tokio::test]
    async fn freshness_cache_miss_after_ttl_expiry() {
        let cache = make_cache(1000, 1, 5000);
        cache.set_cached_freshness("user1", "session-a", true).await;
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(cache.get_cached_freshness("user1", "session-a").await, None);
    }

    #[tokio::test]
    async fn invalidate_freshness_removes_all_sessions_for_user() {
        let cache = make_cache(1000, 5000, 5000);
        cache.set_cached_freshness("user1", "session-a", true).await;
        cache.set_cached_freshness("user1", "session-b", true).await;
        cache.invalidate_freshness("user1").await;
        assert_eq!(cache.get_cached_freshness("user1", "session-a").await, None);
        assert_eq!(cache.get_cached_freshness("user1", "session-b").await, None);
    }

    #[tokio::test]
    async fn permission_cache_hit_on_same_user_and_version() {
        let cache = make_cache(1000, 5000, 5000);
        cache.set_cached_permission("user1", 3, true).await;
        assert_eq!(cache.get_cached_permission("user1", 3).await, Some(true));
    }

    #[tokio::test]
    async fn permission_cache_miss_on_different_version() {
        let cache = make_cache(1000, 5000, 5000);
        cache.set_cached_permission("user1", 3, true).await;
        assert_eq!(cache.get_cached_permission("user1", 4).await, None);
    }

    #[tokio::test]
    async fn permission_cache_does_not_cache_invalid() {
        let cache = make_cache(1000, 5000, 5000);
        cache.set_cached_permission("user1", 3, false).await;
        assert_eq!(cache.get_cached_permission("user1", 3).await, None);
    }

    #[tokio::test]
    async fn invalidate_permission_removes_all_versions_for_user() {
        let cache = make_cache(1000, 5000, 5000);
        cache.set_cached_permission("user1", 1, true).await;
        cache.set_cached_permission("user1", 2, true).await;
        cache.set_cached_permission("user2", 1, true).await;
        cache.invalidate_permission("user1").await;
        assert_eq!(cache.get_cached_permission("user1", 1).await, None);
        assert_eq!(cache.get_cached_permission("user1", 2).await, None);
        assert_eq!(cache.get_cached_permission("user2", 1).await, Some(true));
    }

    #[tokio::test]
    async fn lru_evicts_least_recently_used() {
        let cache = make_cache(2, 5000, 5000);
        cache.set_cached_freshness("user1", "s1", true).await;
        cache.set_cached_freshness("user2", "s1", true).await;
        // user1/s1 is LRU, inserting user3/s1 should evict it
        cache.set_cached_freshness("user3", "s1", true).await;
        assert_eq!(cache.get_cached_freshness("user1", "s1").await, None);
        assert_eq!(cache.get_cached_freshness("user2", "s1").await, Some(true));
        assert_eq!(cache.get_cached_freshness("user3", "s1").await, Some(true));
    }

    #[tokio::test]
    async fn freshness_cache_removes_expired_entries_before_lru_eviction_when_full() {
        let cache = make_cache(3, 5000, 5000);
        let now = Instant::now();
        {
            let mut freshness_cache = cache.freshness_cache.write().unwrap();
            freshness_cache.put(
                "expired:s1".to_string(),
                CacheEntry {
                    value: true,
                    expires_at: now - Duration::from_millis(1),
                },
            );
            freshness_cache.put(
                "user1:s1".to_string(),
                CacheEntry {
                    value: true,
                    expires_at: now + Duration::from_secs(60),
                },
            );
            freshness_cache.put(
                "user2:s1".to_string(),
                CacheEntry {
                    value: true,
                    expires_at: now + Duration::from_secs(60),
                },
            );
        }

        cache.set_cached_freshness("user3", "s1", true).await;

        assert_eq!(cache.get_cached_freshness("expired", "s1").await, None);
        assert_eq!(cache.get_cached_freshness("user1", "s1").await, Some(true));
        assert_eq!(cache.get_cached_freshness("user2", "s1").await, Some(true));
        assert_eq!(cache.get_cached_freshness("user3", "s1").await, Some(true));
    }

    #[tokio::test]
    async fn permission_cache_uses_bounded_lru_eviction_when_no_expired_entries_exist() {
        let cache = make_cache(2, 5000, 5000);
        cache.set_cached_permission("user1", 1, true).await;
        cache.set_cached_permission("user2", 1, true).await;

        cache.set_cached_permission("user3", 1, true).await;

        assert_eq!(cache.get_cached_permission("user1", 1).await, None);
        assert_eq!(cache.get_cached_permission("user2", 1).await, Some(true));
        assert_eq!(cache.get_cached_permission("user3", 1).await, Some(true));
    }

    #[tokio::test]
    async fn concurrent_hits_share_read_lock() {
        let cache = std::sync::Arc::new(make_cache(1000, 5000, 5000));
        cache.set_cached_freshness("user1", "s1", true).await;
        cache.set_cached_permission("user1", 1, true).await;

        let mut joins = Vec::new();
        for _ in 0..32 {
            let cache = std::sync::Arc::clone(&cache);
            joins.push(tokio::spawn(async move {
                assert_eq!(cache.get_cached_freshness("user1", "s1").await, Some(true));
                assert_eq!(cache.get_cached_permission("user1", 1).await, Some(true));
            }));
        }
        for join in joins {
            join.await.unwrap();
        }
    }

    fn sample_claims(sub: &str) -> TokenData {
        TokenData {
            sub: Some(sub.to_string()),
            email: None,
            username: Some(sub.to_string()),
            token_kind: Some("access".to_string()),
            is_admin: Some(false),
            permissions: vec!["flight:read".to_string()],
            department: None,
            department_id: None,
            pv: Some(1),
            iat: Some(1),
            exp: Some(i64::MAX / 2),
            iss: None,
            aud: None,
            ua_hash: None,
            ip_subnet_hash: None,
        }
    }

    #[test]
    fn claims_cache_hit_on_same_token_hash() {
        let cache = make_cache(1000, 5000, 5000);
        cache.set_cached_claims("hash-a", sample_claims("user1"));
        let cached = cache.get_cached_claims("hash-a").expect("cached claims");
        assert_eq!(cached.sub.as_deref(), Some("user1"));
        assert!(cache.get_cached_claims("hash-b").is_none());
    }
}
