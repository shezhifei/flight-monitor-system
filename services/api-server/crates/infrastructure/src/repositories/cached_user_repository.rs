//! Redis 缓存装饰器 —— CachedUserRepository
//!
//! 包装 `PgUserRepository`，为 `find_by_id` 添加 Redis 缓存能力。
//! 缓存 key: `user:auth:{user_id}`，TTL 300 秒。
//! 写操作通过 cache-aside 模式使缓存失效，保证角色/权限变更的最终一致性。
//! 缓存失败时静默降级到数据库查询，不阻断请求。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tracing::warn;

use fms_domain::error::DomainError;
use fms_domain::models::user::{Role, User};
use fms_domain::ports::user_repository::{RoleRepository, UserRepository};

use crate::cache::cache_service::CacheLookup;
use crate::cache::{CacheService, LocalCacheService, RedisPool};
use crate::repositories::pg_role_repository::PgRoleRepository;
use crate::repositories::pg_user_repository::PgUserRepository;

/// 缓存 key 前缀
const CACHE_KEY_PREFIX: &str = "user:auth:";
const ALL_USER_AUTH_CACHE_KEYS_PATTERN: &str = "user:auth:*";

/// 缓存 TTL（秒）
const CACHE_TTL: u64 = 300;
const NEGATIVE_CACHE_TTL: u64 = 30;
const NEGATIVE_CACHE_MAX_ENTRIES: usize = 10_000;

/// User + Roles + Permissions 缓存聚合体
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuthBundle {
    user: User,
}

/// Redis 缓存装饰器
///
/// 将 `PgUserRepository` 与 `RedisPool` 组合，对 `find_by_id` 提供
/// 缓存加速，写操作自动失效缓存。
pub struct CachedUserRepository {
    inner: Arc<dyn UserRepository + Send + Sync>,
    redis: RedisPool,
    local_negative_cache: LocalCacheService,
}

impl CachedUserRepository {
    pub fn new(inner: PgUserRepository, redis: RedisPool) -> Self {
        Self::with_inner(Arc::new(inner), redis)
    }

    fn with_inner(inner: Arc<dyn UserRepository + Send + Sync>, redis: RedisPool) -> Self {
        Self {
            inner,
            redis,
            local_negative_cache: LocalCacheService::new(
                NEGATIVE_CACHE_MAX_ENTRIES,
                Duration::from_secs(NEGATIVE_CACHE_TTL),
            ),
        }
    }

    /// 构建缓存 key
    fn cache_key(user_id: &str) -> String {
        format!("{}{}", CACHE_KEY_PREFIX, user_id)
    }

    /// 尝试从 Redis 读取用户缓存
    ///
    /// 返回 `Ok(None)` 表示缓存未命中或发生可降级错误。
    async fn get_from_cache(&self, user_id: &str) -> Result<Option<User>, DomainError> {
        let key = Self::cache_key(user_id);

        let mut conn = match self.redis.get().await {
            Ok(c) => c,
            Err(e) => {
                warn!(error = %e, "获取 Redis 连接失败，跳过缓存读取");
                return Ok(None);
            }
        };

        let result = redis::cmd("GET")
            .arg(&key)
            .query_async::<Option<String>>(&mut *conn)
            .await;

        match result {
            Ok(Some(json_str)) => match serde_json::from_str::<AuthBundle>(&json_str) {
                Ok(bundle) => Ok(Some(bundle.user)),
                Err(e) => {
                    warn!(error = %e, key = %key, "缓存数据反序列化失败，降级到数据库");
                    Ok(None)
                }
            },
            Ok(None) => Ok(None),
            Err(e) => {
                warn!(error = %e, key = %key, "Redis GET 操作失败，降级到数据库");
                Ok(None)
            }
        }
    }

    /// 将用户数据写入 Redis 缓存
    async fn set_cache(&self, user_id: &str, user: &User) {
        let key = Self::cache_key(user_id);

        let mut conn = match self.redis.get().await {
            Ok(c) => c,
            Err(e) => {
                warn!(error = %e, "获取 Redis 连接失败，跳过缓存写入");
                return;
            }
        };

        let bundle = AuthBundle { user: user.clone() };
        let json_str = match serde_json::to_string(&bundle) {
            Ok(s) => s,
            Err(e) => {
                warn!(error = %e, "缓存序列化失败");
                return;
            }
        };

        if let Err(e) = redis::cmd("SETEX")
            .arg(&key)
            .arg(CACHE_TTL)
            .arg(&json_str)
            .query_async::<()>(&mut *conn)
            .await
        {
            warn!(error = %e, key = %key, "Redis SETEX 操作失败");
        }
    }

    /// 使指定用户的缓存失效
    async fn invalidate_cache(&self, user_id: &str) {
        let key = Self::cache_key(user_id);
        self.local_negative_cache.delete(&key).await;

        let mut conn = match self.redis.get().await {
            Ok(c) => c,
            Err(e) => {
                warn!(error = %e, "获取 Redis 连接失败，跳过缓存失效");
                return;
            }
        };

        if let Err(e) = redis::cmd("DEL").arg(&key).query_async::<()>(&mut *conn).await {
            warn!(error = %e, key = %key, "Redis DEL 操作失败");
        }
    }
}

/// Redis 缓存装饰器 —— 角色仓储
///
/// `User` 鉴权缓存包含角色和权限聚合数据，因此角色关系或角色权限发生变化时，
/// 需要同步失效 `CachedUserRepository` 写入的用户鉴权缓存。
pub struct CachedRoleRepository {
    inner: PgRoleRepository,
    redis: RedisPool,
}

impl CachedRoleRepository {
    pub fn new(inner: PgRoleRepository, redis: RedisPool) -> Self {
        Self { inner, redis }
    }

    fn affected_user_cache_key(user_id: &str) -> String {
        CachedUserRepository::cache_key(user_id)
    }

    fn user_auth_cache_key_patterns() -> [&'static str; 1] {
        [ALL_USER_AUTH_CACHE_KEYS_PATTERN]
    }

    /// 使用 SCAN/MATCH 删除匹配 key，避免阻塞 Redis。
    async fn delete_matching_keys<C>(conn: &mut C, pattern: &str)
    where
        C: redis::aio::ConnectionLike + Send,
    {
        let mut cursor = 0_u64;

        loop {
            let scan_result = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(pattern)
                .arg("COUNT")
                .arg(100)
                .query_async::<(u64, Vec<String>)>(&mut *conn)
                .await;

            let (next_cursor, keys) = match scan_result {
                Ok(result) => result,
                Err(e) => {
                    warn!(error = %e, pattern = %pattern, "Redis SCAN 操作失败，跳过角色缓存失效");
                    break;
                }
            };

            if !keys.is_empty() {
                if let Err(e) = redis::cmd("DEL").arg(&keys).query_async::<()>(&mut *conn).await {
                    warn!(error = %e, pattern = %pattern, "Redis DEL 批量删除用户鉴权缓存失败");
                }
            }

            cursor = next_cursor;
            if cursor == 0 {
                break;
            }
        }
    }

    async fn invalidate_user_auth_cache(&self, user_id: &str) {
        let key = Self::affected_user_cache_key(user_id);

        let mut conn = match self.redis.get().await {
            Ok(c) => c,
            Err(e) => {
                warn!(error = %e, "获取 Redis 连接失败，跳过用户角色缓存失效");
                return;
            }
        };

        if let Err(e) = redis::cmd("DEL").arg(&key).query_async::<()>(&mut *conn).await {
            warn!(error = %e, key = %key, "Redis DEL 用户鉴权缓存失败");
        }
    }

    async fn invalidate_all_user_auth_caches(&self) {
        let mut conn = match self.redis.get().await {
            Ok(c) => c,
            Err(e) => {
                warn!(error = %e, "获取 Redis 连接失败，跳过角色权限缓存失效");
                return;
            }
        };

        for pattern in Self::user_auth_cache_key_patterns() {
            Self::delete_matching_keys(&mut *conn, pattern).await;
        }
    }
}

#[async_trait]
impl UserRepository for CachedUserRepository {
    async fn find_by_id(&self, user_id: &str) -> Result<Option<User>, DomainError> {
        // 尝试从缓存读取，缓存 miss 或错误时静默降级
        if let Ok(Some(user)) = self.get_from_cache(user_id).await {
            self.local_negative_cache.delete(&Self::cache_key(user_id)).await;
            return Ok(Some(user));
        }

        let cache_key = Self::cache_key(user_id);
        if let CacheLookup::NegativeHit = self.local_negative_cache.get_lookup::<AuthBundle>(&cache_key).await {
            return Ok(None);
        }

        // 缓存未命中 → 回源数据库
        let user = self.inner.find_by_id(user_id).await?;

        // 异步写入缓存（fire-and-forget，失败仅记录日志）
        if let Some(ref user) = user {
            self.local_negative_cache.delete(&cache_key).await;
            self.set_cache(user_id, user).await;
        } else {
            self.local_negative_cache
                .set_negative(&cache_key, Some(Duration::from_secs(NEGATIVE_CACHE_TTL)))
                .await;
        }

        Ok(user)
    }

    async fn find_permission_version_by_id(&self, id: &str) -> Result<Option<i32>, DomainError> {
        self.inner.find_permission_version_by_id(id).await
    }

    async fn find_by_username(&self, username: &str) -> Result<Option<User>, DomainError> {
        self.inner.find_by_username(username).await
    }

    async fn find_by_email(&self, email: &str) -> Result<Option<User>, DomainError> {
        self.inner.find_by_email(email).await
    }

    async fn find_all(&self, limit: i64, offset: i64) -> Result<Vec<User>, DomainError> {
        self.inner.find_all(limit, offset).await
    }

    async fn list_distinct_departments_in_use(&self) -> Result<Vec<String>, DomainError> {
        self.inner.list_distinct_departments_in_use().await
    }

    async fn has_any_user_with_department_id(&self, department_id: &str) -> Result<bool, DomainError> {
        self.inner.has_any_user_with_department_id(department_id).await
    }

    async fn save(&self, user: &User) -> Result<(), DomainError> {
        self.inner.save(user).await?;
        // 写入后使缓存失效，下次读取时重新填充
        self.invalidate_cache(&user.id).await;
        Ok(())
    }

    async fn update(&self, user: &User) -> Result<bool, DomainError> {
        let updated = self.inner.update(user).await?;
        if updated {
            self.invalidate_cache(&user.id).await;
        }
        Ok(updated)
    }

    async fn delete(&self, id: &str) -> Result<bool, DomainError> {
        let deleted = self.inner.delete(id).await?;
        if deleted {
            self.invalidate_cache(id).await;
        }
        Ok(deleted)
    }

    async fn update_password(&self, user_id: &str, password_hash: &str) -> Result<bool, DomainError> {
        let updated = self.inner.update_password(user_id, password_hash).await?;
        if updated {
            self.invalidate_cache(user_id).await;
        }
        Ok(updated)
    }

    async fn update_last_login(&self, user_id: &str) -> Result<bool, DomainError> {
        let updated = self.inner.update_last_login(user_id).await?;
        if updated {
            self.invalidate_cache(user_id).await;
        }
        Ok(updated)
    }
}

#[async_trait]
impl RoleRepository for CachedRoleRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<Role>, DomainError> {
        self.inner.find_by_id(id).await
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<Role>, DomainError> {
        self.inner.find_by_name(name).await
    }

    async fn find_all(&self) -> Result<Vec<Role>, DomainError> {
        self.inner.find_all().await
    }

    async fn save(&self, role: &Role) -> Result<(), DomainError> {
        self.inner.save(role).await?;
        self.invalidate_all_user_auth_caches().await;
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<bool, DomainError> {
        let deleted = self.inner.delete(id).await?;
        if deleted {
            self.invalidate_all_user_auth_caches().await;
        }
        Ok(deleted)
    }

    async fn find_by_user_id(&self, user_id: &str) -> Result<Vec<Role>, DomainError> {
        self.inner.find_by_user_id(user_id).await
    }

    async fn count_users(&self, role_id: &str) -> Result<i64, DomainError> {
        self.inner.count_users(role_id).await
    }

    async fn set_permissions(&self, role_id: &str, permission_names: &[String]) -> Result<(), DomainError> {
        self.inner.set_permissions(role_id, permission_names).await?;
        self.invalidate_all_user_auth_caches().await;
        Ok(())
    }

    async fn assign_role_to_user(&self, user_id: &str, role_id: &str) -> Result<(), DomainError> {
        self.inner.assign_role_to_user(user_id, role_id).await?;
        self.invalidate_user_auth_cache(user_id).await;
        Ok(())
    }

    async fn remove_user_from_role(&self, user_id: &str, role_id: &str) -> Result<(), DomainError> {
        self.inner.remove_user_from_role(user_id, role_id).await?;
        self.invalidate_user_auth_cache(user_id).await;
        Ok(())
    }

    async fn add_permission(&self, role_id: &str, permission_name: &str) -> Result<bool, DomainError> {
        let added = self.inner.add_permission(role_id, permission_name).await?;
        if added {
            self.invalidate_all_user_auth_caches().await;
        }
        Ok(added)
    }

    async fn remove_permission(&self, role_id: &str, permission_name: &str) -> Result<bool, DomainError> {
        let removed = self.inner.remove_permission(role_id, permission_name).await?;
        if removed {
            self.invalidate_all_user_auth_caches().await;
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bb8::Pool;
    use bb8_redis::RedisConnectionManager;
    use chrono::Utc;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    struct MockUserRepository {
        users: Mutex<HashMap<String, User>>,
        find_by_id_calls: AtomicUsize,
    }

    impl MockUserRepository {
        fn new() -> Self {
            Self {
                users: Mutex::new(HashMap::new()),
                find_by_id_calls: AtomicUsize::new(0),
            }
        }

        fn find_by_id_calls(&self) -> usize {
            self.find_by_id_calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl UserRepository for MockUserRepository {
        async fn find_by_id(&self, id: &str) -> Result<Option<User>, DomainError> {
            self.find_by_id_calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.users.lock().expect("lock users").get(id).cloned())
        }

        async fn find_permission_version_by_id(&self, _id: &str) -> Result<Option<i32>, DomainError> {
            Ok(None)
        }

        async fn find_by_username(&self, _username: &str) -> Result<Option<User>, DomainError> {
            Ok(None)
        }

        async fn find_by_email(&self, _email: &str) -> Result<Option<User>, DomainError> {
            Ok(None)
        }

        async fn find_all(&self, _limit: i64, _offset: i64) -> Result<Vec<User>, DomainError> {
            Ok(Vec::new())
        }

        async fn list_distinct_departments_in_use(&self) -> Result<Vec<String>, DomainError> {
            Ok(Vec::new())
        }

        async fn has_any_user_with_department_id(&self, _department_id: &str) -> Result<bool, DomainError> {
            Ok(false)
        }

        async fn save(&self, user: &User) -> Result<(), DomainError> {
            self.users
                .lock()
                .expect("lock users")
                .insert(user.id.clone(), user.clone());
            Ok(())
        }

        async fn update(&self, user: &User) -> Result<bool, DomainError> {
            self.users
                .lock()
                .expect("lock users")
                .insert(user.id.clone(), user.clone());
            Ok(true)
        }

        async fn delete(&self, id: &str) -> Result<bool, DomainError> {
            Ok(self.users.lock().expect("lock users").remove(id).is_some())
        }

        async fn update_password(&self, _id: &str, _password_hash: &str) -> Result<bool, DomainError> {
            Ok(false)
        }

        async fn update_last_login(&self, _id: &str) -> Result<bool, DomainError> {
            Ok(false)
        }
    }

    fn pattern_matches_key(pattern: &str, key: &str) -> bool {
        pattern.strip_suffix('*').is_some_and(|prefix| key.starts_with(prefix))
    }

    async fn test_redis_pool() -> RedisPool {
        let manager = RedisConnectionManager::new("redis://127.0.0.1:1/").expect("valid redis test URL");
        Pool::builder()
            .max_size(1)
            .connection_timeout(Duration::from_millis(5))
            .build(manager)
            .await
            .expect("build redis test pool")
    }

    fn test_user(id: &str) -> User {
        let now = Utc::now();
        User {
            id: id.to_string(),
            email: format!("{id}@example.test"),
            password_hash: "hash".to_string(),
            username: id.to_string(),
            display_name: Some(id.to_string()),
            roles: Vec::new(),
            created_at: now,
            updated_at: now,
            last_login_at: None,
            is_active: true,
            is_verified: true,
            is_admin: false,
            verification_token: None,
            verification_token_expires: None,
            verified_at: None,
            password_reset_token: None,
            password_reset_token_expires: None,
            password_changed_at: None,
            department: None,
            department_id: None,
            job_level: Some(1),
            job_title: None,
            permission_version: 1,
            account_type: "personal".into(),
            login_enabled: true,
            current_occupant_user_id: None,
        }
    }

    #[test]
    fn user_role_assignment_invalidates_target_user_auth_cache() {
        assert_eq!(
            CachedRoleRepository::affected_user_cache_key("user-42"),
            CachedUserRepository::cache_key("user-42")
        );
    }

    #[test]
    fn role_permission_mutation_invalidates_all_cached_user_auth_bundles() {
        let cached_user_key = CachedUserRepository::cache_key("user-42");

        assert!(
            CachedRoleRepository::user_auth_cache_key_patterns()
                .iter()
                .any(|pattern| pattern_matches_key(pattern, &cached_user_key)),
            "role permission changes must invalidate cached user auth bundles"
        );
    }

    #[tokio::test]
    async fn find_by_id_negative_cache_prevents_repeated_origin_lookup_after_absence() {
        let inner = Arc::new(MockUserRepository::new());
        let repo = CachedUserRepository::with_inner(inner.clone(), test_redis_pool().await);

        assert!(repo.find_by_id("missing-user").await.expect("first lookup").is_none());
        assert!(repo.find_by_id("missing-user").await.expect("second lookup").is_none());

        assert_eq!(inner.find_by_id_calls(), 1);
    }

    #[tokio::test]
    async fn save_invalidates_negative_cache_for_same_user_id() {
        let inner = Arc::new(MockUserRepository::new());
        let repo = CachedUserRepository::with_inner(inner.clone(), test_redis_pool().await);
        let user = test_user("new-user");

        assert!(repo.find_by_id(&user.id).await.expect("first lookup").is_none());
        assert_eq!(inner.find_by_id_calls(), 1);

        repo.save(&user).await.expect("save user");

        assert!(repo.find_by_id(&user.id).await.expect("lookup after save").is_some());
        assert_eq!(inner.find_by_id_calls(), 2);
    }
}
