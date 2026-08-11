//! 会话运行时仓储实现

use std::collections::HashSet;
use std::sync::atomic::{AtomicI64, Ordering};

use crate::cache::RedisPool;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

use fms_domain::error::DomainError;
use fms_domain::models::session_runtime::{
    OnlineSessionStatus, SessionEstablishResult, SessionKickEvent, SessionRuntimeStatus,
};
use fms_domain::ports::session_runtime_repository::SessionRuntimeRepository;

const KEY_REFRESH_TOKEN: &str = "session:refresh:";
const KEY_ONLINE_STATUS: &str = "online:";
const KEY_ONLINE_USERS_INDEX: &str = "online:index:users";
const KEY_USER_SESSION: &str = "session:active:";
const KEY_SESSION_KICK_EVENT: &str = "session:kick_event:";
const KEY_PERMISSION_VERSION: &str = "auth:permver:";
const MAX_REFRESH_TOKENS_PER_USER: usize = 8;
const FALLBACK_ONLINE_TTL: i64 = 150;

#[derive(Debug, Clone)]
struct SessionRecord {
    session_id: String,
    login_time: DateTime<Utc>,
    last_seen: DateTime<Utc>,
    client_ip: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionPayload {
    session_id: String,
    login_time: DateTime<Utc>,
    last_seen: DateTime<Utc>,
    client_ip: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RefreshTokenPayload {
    tokens: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct RefreshTokenRecord {
    tokens: Vec<String>,
    touched_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
struct KickEventRecord {
    event: SessionKickEvent,
    expires_at: DateTime<Utc>,
}

pub struct InMemorySessionRuntimeRepository {
    idle_threshold_seconds: i64,
    online_ttl_seconds: i64,
    refresh_ttl_seconds: i64,
    redis_pool: Option<RedisPool>,
    fallback_since: AtomicI64,
    sessions: DashMap<String, SessionRecord>,
    refresh_tokens: DashMap<String, RefreshTokenRecord>,
    kick_events: DashMap<String, KickEventRecord>,
}

impl InMemorySessionRuntimeRepository {
    pub fn new(idle_threshold_seconds: i64) -> Self {
        let now = Utc::now();
        Self {
            idle_threshold_seconds: idle_threshold_seconds.max(1),
            online_ttl_seconds: 300,
            refresh_ttl_seconds: 604_800,
            redis_pool: None,
            fallback_since: AtomicI64::new(now.timestamp()),
            sessions: DashMap::new(),
            refresh_tokens: DashMap::new(),
            kick_events: DashMap::new(),
        }
    }

    pub fn with_redis(
        redis_pool: RedisPool,
        idle_threshold_seconds: i64,
        online_ttl_seconds: i64,
        refresh_ttl_seconds: i64,
    ) -> Self {
        Self {
            idle_threshold_seconds: idle_threshold_seconds.max(1),
            online_ttl_seconds: online_ttl_seconds.max(1),
            refresh_ttl_seconds: refresh_ttl_seconds.max(1),
            redis_pool: Some(redis_pool),
            fallback_since: AtomicI64::new(0),
            sessions: DashMap::new(),
            refresh_tokens: DashMap::new(),
            kick_events: DashMap::new(),
        }
    }

    fn refresh_key(user_id: &str) -> String {
        format!("{KEY_REFRESH_TOKEN}{user_id}")
    }

    fn online_key(user_id: &str) -> String {
        format!("{KEY_ONLINE_STATUS}{user_id}")
    }

    fn session_key(user_id: &str) -> String {
        format!("{KEY_USER_SESSION}{user_id}")
    }

    fn kick_event_key(user_id: &str) -> String {
        format!("{KEY_SESSION_KICK_EVENT}{user_id}")
    }

    fn permission_version_key(user_id: &str) -> String {
        format!("{KEY_PERMISSION_VERSION}{user_id}")
    }

    fn fallback_ttl_seconds(&self) -> i64 {
        self.online_ttl_seconds.min(FALLBACK_ONLINE_TTL).max(1)
    }

    async fn mark_redis_success(&self) {
        if self.redis_pool.is_some() {
            self.fallback_since.store(0, Ordering::Relaxed);
        }
    }

    async fn mark_fallback(&self) {
        let now = Utc::now().timestamp();
        let _ = self
            .fallback_since
            .compare_exchange(0, now, Ordering::Relaxed, Ordering::Relaxed);
    }

    fn session_is_stale(&self, last_seen: DateTime<Utc>) -> bool {
        (Utc::now() - last_seen).num_seconds() > self.online_ttl_seconds
    }

    fn refresh_token_is_stale_at(&self, now: DateTime<Utc>, touched_at: DateTime<Utc>) -> bool {
        (now - touched_at).num_seconds() > self.refresh_ttl_seconds
    }

    fn refresh_token_is_stale(&self, touched_at: DateTime<Utc>) -> bool {
        self.refresh_token_is_stale_at(Utc::now(), touched_at)
    }

    async fn store_memory_session(&self, user_id: &str, client_ip: Option<&str>) -> (SessionRecord, bool) {
        self.opportunistic_prune_memory();
        let now = Utc::now();
        if let Some(mut entry) = self.sessions.get_mut(user_id) {
            entry.last_seen = now;
            entry.client_ip = normalize_optional(client_ip);
            return (entry.clone(), false);
        }

        let record = SessionRecord {
            session_id: uuid::Uuid::new_v4().to_string(),
            login_time: now,
            last_seen: now,
            client_ip: normalize_optional(client_ip),
        };
        self.sessions.insert(user_id.to_string(), record.clone());
        (record, true)
    }

    async fn store_memory_refresh_token(&self, user_id: &str, refresh_token: Option<&str>) {
        let Some(refresh_token) = normalize_optional(refresh_token) else {
            return;
        };

        let now = Utc::now();
        self.refresh_tokens
            .retain(|_, record| !self.refresh_token_is_stale_at(now, record.touched_at));

        let mut record = self
            .refresh_tokens
            .entry(user_id.to_string())
            .or_insert_with(|| RefreshTokenRecord {
                tokens: Vec::new(),
                touched_at: now,
            });

        let mut deduped = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();
        for item in record.tokens.iter().chain(std::iter::once(&refresh_token)) {
            if seen.insert(item.as_str()) {
                deduped.push(item.clone());
            }
        }
        if deduped.len() > MAX_REFRESH_TOKENS_PER_USER {
            let drain = deduped.len() - MAX_REFRESH_TOKENS_PER_USER;
            deduped.drain(0..drain);
        }
        record.tokens = deduped;
        record.touched_at = now;
    }

    async fn read_memory_refresh_token(&self, user_id: &str, refresh_token: &str) -> bool {
        let should_remove = self
            .refresh_tokens
            .get(user_id)
            .map(|entry| self.refresh_token_is_stale(entry.value().touched_at))
            .unwrap_or(false);
        if should_remove {
            self.refresh_tokens.remove(user_id);
            return false;
        }

        self.refresh_tokens
            .get(user_id)
            .map(|entry| entry.value().tokens.iter().any(|token| token == refresh_token))
            .unwrap_or(false)
    }

    async fn clear_memory_refresh_tokens(&self, user_id: &str) {
        self.refresh_tokens.remove(user_id);
    }

    fn build_status(
        &self,
        user_id: &str,
        record: Option<&SessionRecord>,
        kick_event: Option<SessionKickEvent>,
    ) -> OnlineSessionStatus {
        if let Some(record) = record {
            let idle_seconds = (Utc::now() - record.last_seen).num_seconds();
            let status = if idle_seconds > self.idle_threshold_seconds {
                "idle"
            } else {
                "active"
            };
            return OnlineSessionStatus {
                user_id: user_id.to_string(),
                session_id: Some(record.session_id.clone()),
                login_time: Some(record.login_time),
                last_seen: Some(record.last_seen),
                status: status.to_string(),
                client_ip: record.client_ip.clone(),
                username: None,
                job_title: None,
                department: None,
                forced_logout: false,
                kick_event: None,
            };
        }

        OnlineSessionStatus {
            user_id: user_id.to_string(),
            session_id: None,
            login_time: None,
            last_seen: None,
            status: "offline".to_string(),
            client_ip: None,
            username: None,
            job_title: None,
            department: None,
            forced_logout: kick_event.is_some(),
            kick_event,
        }
    }

    fn cleanup_memory_user_if_stale(&self, user_id: &str) {
        let should_remove = self
            .sessions
            .get(user_id)
            .map(|entry| self.session_is_stale(entry.last_seen))
            .unwrap_or(false);
        if should_remove {
            self.sessions.remove(user_id);
        }
    }

    fn cleanup_memory_sessions(&self) {
        self.sessions.retain(|_, entry| !self.session_is_stale(entry.last_seen));
    }

    fn kick_event_is_stale(&self, expires_at: DateTime<Utc>) -> bool {
        Utc::now() >= expires_at
    }

    fn cleanup_memory_kick_event_if_stale(&self, user_id: &str) {
        let should_remove = self
            .kick_events
            .get(user_id)
            .map(|entry| self.kick_event_is_stale(entry.value().expires_at))
            .unwrap_or(false);
        if should_remove {
            self.kick_events.remove(user_id);
        }
    }

    fn cleanup_memory_kick_events(&self) {
        self.kick_events
            .retain(|_, entry| !self.kick_event_is_stale(entry.expires_at));
    }

    fn opportunistic_prune_memory(&self) {
        self.cleanup_memory_sessions();
        self.cleanup_memory_kick_events();
    }

    fn store_memory_kick_event(&self, user_id: &str, reason: &str) {
        self.opportunistic_prune_memory();
        let now = Utc::now();
        self.kick_events.insert(
            user_id.to_string(),
            KickEventRecord {
                event: SessionKickEvent {
                    reason: reason.to_string(),
                    at: now,
                },
                expires_at: now + chrono::Duration::seconds(self.fallback_ttl_seconds()),
            },
        );
    }

    fn get_memory_kick_event(&self, user_id: &str) -> Option<SessionKickEvent> {
        self.cleanup_memory_kick_event_if_stale(user_id);
        self.kick_events.get(user_id).map(|entry| entry.value().event.clone())
    }

    async fn store_redis_session(
        &self,
        user_id: &str,
        record: &SessionRecord,
        refresh_token: Option<&str>,
    ) -> Result<(), DomainError> {
        let Some(redis_pool) = &self.redis_pool else {
            return Ok(());
        };

        let session_payload = SessionPayload {
            session_id: record.session_id.clone(),
            login_time: record.login_time,
            last_seen: record.last_seen,
            client_ip: record.client_ip.clone(),
        };

        let mut connection = redis_pool
            .get()
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        let online_key = Self::online_key(user_id);
        let session_key = Self::session_key(user_id);
        let payload =
            serde_json::to_string(&session_payload).map_err(|error| DomainError::Internal(error.to_string()))?;

        let mut pipe = redis::pipe();
        pipe.cmd("SETEX")
            .arg(&online_key)
            .arg(self.online_ttl_seconds)
            .arg(payload);
        pipe.cmd("SET").arg(&session_key).arg(&record.session_id);
        pipe.cmd("ZADD")
            .arg(KEY_ONLINE_USERS_INDEX)
            .arg(record.last_seen.timestamp())
            .arg(user_id);
        pipe.cmd("ZREMRANGEBYSCORE")
            .arg(KEY_ONLINE_USERS_INDEX)
            .arg("-inf")
            .arg(record.last_seen.timestamp() - self.online_ttl_seconds);

        if refresh_token.is_none() {
            pipe.cmd("DEL").arg(Self::kick_event_key(user_id));
            pipe.query_async::<()>(&mut *connection)
                .await
                .map_err(|error| DomainError::Internal(error.to_string()))?;
        } else {
            pipe.query_async::<()>(&mut *connection)
                .await
                .map_err(|error| DomainError::Internal(error.to_string()))?;

            if let Some(refresh_token) = normalize_optional(refresh_token) {
                let refresh_key = Self::refresh_key(user_id);
                let existing_raw = redis::cmd("GET")
                    .arg(&refresh_key)
                    .query_async::<Option<String>>(&mut *connection)
                    .await
                    .map_err(|error| DomainError::Internal(error.to_string()))?;
                let mut bundle = existing_raw
                    .as_deref()
                    .and_then(|value| serde_json::from_str::<RefreshTokenPayload>(value).ok())
                    .unwrap_or(RefreshTokenPayload { tokens: Vec::new() });
                if !bundle.tokens.iter().any(|item| item == &refresh_token) {
                    bundle.tokens.push(refresh_token);
                }
                if bundle.tokens.len() > MAX_REFRESH_TOKENS_PER_USER {
                    let drain = bundle.tokens.len() - MAX_REFRESH_TOKENS_PER_USER;
                    bundle.tokens.drain(0..drain);
                }
                let encoded =
                    serde_json::to_string(&bundle).map_err(|error| DomainError::Internal(error.to_string()))?;

                let mut second_pipe = redis::pipe();
                second_pipe
                    .cmd("SETEX")
                    .arg(&refresh_key)
                    .arg(self.refresh_ttl_seconds)
                    .arg(encoded);
                second_pipe.cmd("DEL").arg(Self::kick_event_key(user_id));
                second_pipe
                    .query_async::<()>(&mut *connection)
                    .await
                    .map_err(|error| DomainError::Internal(error.to_string()))?;
            } else {
                redis::cmd("DEL")
                    .arg(Self::kick_event_key(user_id))
                    .query_async::<()>(&mut *connection)
                    .await
                    .map_err(|error| DomainError::Internal(error.to_string()))?;
            }
        }

        Ok(())
    }

    async fn fetch_redis_status(&self, user_id: &str) -> Result<Option<OnlineSessionStatus>, DomainError> {
        let Some(redis_pool) = &self.redis_pool else {
            return Ok(None);
        };

        let mut connection = redis_pool
            .get()
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        let online_key = Self::online_key(user_id);
        let raw = redis::cmd("GET")
            .arg(&online_key)
            .query_async::<Option<String>>(&mut *connection)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;

        let Some(raw) = raw else {
            let kick_event = Self::fetch_redis_kick_event_with_connection(&mut *connection, user_id).await?;
            return Ok(Some(self.build_status(user_id, None, kick_event)));
        };

        let payload: SessionPayload =
            serde_json::from_str(&raw).map_err(|error| DomainError::Internal(error.to_string()))?;
        if self.session_is_stale(payload.last_seen) {
            return Ok(Some(self.build_status(user_id, None, None)));
        }

        let status = self.build_status(
            user_id,
            Some(&SessionRecord {
                session_id: payload.session_id,
                login_time: payload.login_time,
                last_seen: payload.last_seen,
                client_ip: payload.client_ip,
            }),
            None,
        );
        Ok(Some(status))
    }

    async fn fetch_redis_kick_event_with_connection<C>(
        connection: &mut C,
        user_id: &str,
    ) -> Result<Option<SessionKickEvent>, DomainError>
    where
        C: redis::aio::ConnectionLike + Send,
    {
        let raw = redis::cmd("GET")
            .arg(Self::kick_event_key(user_id))
            .query_async::<Option<String>>(connection)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        raw.map(|value| serde_json::from_str::<SessionKickEvent>(&value))
            .transpose()
            .map_err(|error| DomainError::Internal(error.to_string()))
    }

    async fn revoke_redis_session(
        &self,
        user_id: &str,
        reason: &str,
    ) -> Result<Option<OnlineSessionStatus>, DomainError> {
        let status = self.fetch_redis_status(user_id).await?;
        let Some(redis_pool) = &self.redis_pool else {
            return Ok(status);
        };

        let mut connection = redis_pool
            .get()
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        redis::cmd("DEL")
            .arg(Self::online_key(user_id))
            .arg(Self::session_key(user_id))
            .arg(Self::refresh_key(user_id))
            .query_async::<()>(&mut *connection)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        redis::cmd("ZREM")
            .arg(KEY_ONLINE_USERS_INDEX)
            .arg(user_id)
            .query_async::<()>(&mut *connection)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;

        let normalized_reason = reason.trim();
        if matches!(normalized_reason, "admin_kick" | "admin_force_offline") {
            let payload = SessionKickEvent {
                reason: normalized_reason.to_string(),
                at: Utc::now(),
            };
            let encoded = serde_json::to_string(&payload).map_err(|error| DomainError::Internal(error.to_string()))?;
            redis::cmd("SETEX")
                .arg(Self::kick_event_key(user_id))
                .arg(self.fallback_ttl_seconds())
                .arg(encoded)
                .query_async::<()>(&mut *connection)
                .await
                .map_err(|error| DomainError::Internal(error.to_string()))?;
        } else {
            redis::cmd("DEL")
                .arg(Self::kick_event_key(user_id))
                .query_async::<()>(&mut *connection)
                .await
                .map_err(|error| DomainError::Internal(error.to_string()))?;
        }

        Ok(status)
    }

    async fn heartbeat_redis(&self, user_id: &str) -> Result<(), DomainError> {
        let Some(redis_pool) = &self.redis_pool else {
            return Ok(());
        };

        let mut connection = redis_pool
            .get()
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        let key = Self::online_key(user_id);
        let raw = redis::cmd("GET")
            .arg(&key)
            .query_async::<Option<String>>(&mut *connection)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        let Some(raw) = raw else {
            return Ok(());
        };

        let mut payload: SessionPayload =
            serde_json::from_str(&raw).map_err(|error| DomainError::Internal(error.to_string()))?;
        payload.last_seen = Utc::now();
        let encoded = serde_json::to_string(&payload).map_err(|error| DomainError::Internal(error.to_string()))?;
        redis::cmd("SETEX")
            .arg(&key)
            .arg(self.online_ttl_seconds)
            .arg(encoded)
            .query_async::<()>(&mut *connection)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        redis::cmd("ZADD")
            .arg(KEY_ONLINE_USERS_INDEX)
            .arg(payload.last_seen.timestamp())
            .arg(user_id)
            .query_async::<()>(&mut *connection)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(())
    }

    async fn fetch_redis_online_users(&self) -> Result<Option<Vec<String>>, DomainError> {
        let Some(redis_pool) = &self.redis_pool else {
            return Ok(None);
        };

        let mut connection = redis_pool
            .get()
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        let min_score = Utc::now().timestamp() - self.online_ttl_seconds;
        redis::cmd("ZREMRANGEBYSCORE")
            .arg(KEY_ONLINE_USERS_INDEX)
            .arg("-inf")
            .arg(min_score - 1)
            .query_async::<()>(&mut *connection)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        let users = redis::cmd("ZRANGEBYSCORE")
            .arg(KEY_ONLINE_USERS_INDEX)
            .arg(min_score)
            .arg("+inf")
            .query_async::<Vec<String>>(&mut *connection)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(Some(users))
    }

    async fn validate_redis_refresh_token(
        &self,
        user_id: &str,
        refresh_token: &str,
    ) -> Result<Option<bool>, DomainError> {
        let Some(redis_pool) = &self.redis_pool else {
            return Ok(None);
        };
        let mut connection = redis_pool
            .get()
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        let raw = redis::cmd("GET")
            .arg(Self::refresh_key(user_id))
            .query_async::<Option<String>>(&mut *connection)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        let result = raw
            .as_deref()
            .and_then(|value| serde_json::from_str::<RefreshTokenPayload>(value).ok())
            .map(|payload| payload.tokens.iter().any(|token| token == refresh_token))
            .unwrap_or(false);
        Ok(Some(result))
    }

    async fn revoke_redis_refresh_tokens(&self, user_id: &str) -> Result<(), DomainError> {
        let Some(redis_pool) = &self.redis_pool else {
            return Ok(());
        };
        let mut connection = redis_pool
            .get()
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        redis::cmd("DEL")
            .arg(Self::refresh_key(user_id))
            .query_async::<()>(&mut *connection)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        Ok(())
    }
}

#[async_trait]
impl SessionRuntimeRepository for InMemorySessionRuntimeRepository {
    async fn establish_session(
        &self,
        user_id: &str,
        client_ip: Option<&str>,
        refresh_token: Option<&str>,
    ) -> Result<SessionEstablishResult, DomainError> {
        let normalized_user_id = user_id.trim();
        if normalized_user_id.is_empty() {
            return Err(DomainError::ValidationError("user_id is required".into()));
        }

        let (record, created) = self.store_memory_session(normalized_user_id, client_ip).await;
        self.store_memory_refresh_token(normalized_user_id, refresh_token).await;
        self.cleanup_memory_kick_event_if_stale(normalized_user_id);
        self.kick_events.remove(normalized_user_id);

        if self.redis_pool.is_some() {
            match self
                .store_redis_session(normalized_user_id, &record, refresh_token)
                .await
            {
                Ok(()) => self.mark_redis_success().await,
                Err(_) => self.mark_fallback().await,
            }
        }

        Ok(SessionEstablishResult {
            session: self.build_status(normalized_user_id, Some(&record), None),
            created,
        })
    }

    async fn validate_refresh_token(&self, user_id: &str, refresh_token: &str) -> Result<bool, DomainError> {
        let normalized_user_id = user_id.trim();
        let normalized_token = refresh_token.trim();
        if normalized_user_id.is_empty() || normalized_token.is_empty() {
            return Ok(false);
        }

        if self.redis_pool.is_some() {
            match self
                .validate_redis_refresh_token(normalized_user_id, normalized_token)
                .await
            {
                Ok(Some(result)) => {
                    self.mark_redis_success().await;
                    return Ok(result);
                }
                Ok(None) => {}
                Err(_) => self.mark_fallback().await,
            }
        }

        Ok(self
            .read_memory_refresh_token(normalized_user_id, normalized_token)
            .await)
    }

    async fn revoke_refresh_tokens(&self, user_id: &str) -> Result<(), DomainError> {
        let normalized_user_id = user_id.trim();
        if normalized_user_id.is_empty() {
            return Ok(());
        }

        self.clear_memory_refresh_tokens(normalized_user_id).await;
        if self.redis_pool.is_some() {
            match self.revoke_redis_refresh_tokens(normalized_user_id).await {
                Ok(()) => self.mark_redis_success().await,
                Err(_) => self.mark_fallback().await,
            }
        }
        Ok(())
    }

    async fn revoke_session(&self, user_id: &str, reason: &str) -> Result<Option<OnlineSessionStatus>, DomainError> {
        let normalized_user_id = user_id.trim();
        if normalized_user_id.is_empty() {
            return Ok(None);
        }

        self.cleanup_memory_user_if_stale(normalized_user_id);
        self.cleanup_memory_kick_event_if_stale(normalized_user_id);
        let removed = self.sessions.remove(normalized_user_id).map(|(_, record)| record);
        self.clear_memory_refresh_tokens(normalized_user_id).await;

        let normalized_reason = reason.trim();
        if matches!(normalized_reason, "admin_kick" | "admin_force_offline") {
            self.store_memory_kick_event(normalized_user_id, normalized_reason);
        } else {
            self.kick_events.remove(normalized_user_id);
        }

        let mut redis_status = None;
        if self.redis_pool.is_some() {
            match self.revoke_redis_session(normalized_user_id, normalized_reason).await {
                Ok(status) => {
                    redis_status = status;
                    self.mark_redis_success().await;
                }
                Err(_) => self.mark_fallback().await,
            }
        }

        Ok(redis_status.or_else(|| removed.map(|record| self.build_status(normalized_user_id, Some(&record), None))))
    }

    async fn heartbeat(&self, user_id: &str) -> Result<Option<OnlineSessionStatus>, DomainError> {
        let normalized_user_id = user_id.trim();
        if normalized_user_id.is_empty() {
            return Ok(None);
        }

        self.cleanup_memory_user_if_stale(normalized_user_id);
        if let Some(mut entry) = self.sessions.get_mut(normalized_user_id) {
            entry.last_seen = Utc::now();
        } else {
            let _ = self.store_memory_session(normalized_user_id, None).await;
        }

        if self.redis_pool.is_some() {
            match self.heartbeat_redis(normalized_user_id).await {
                Ok(()) => self.mark_redis_success().await,
                Err(_) => self.mark_fallback().await,
            }
        }

        Ok(Some(self.get_online_status(normalized_user_id).await?))
    }

    async fn get_online_users(&self) -> Result<Vec<String>, DomainError> {
        self.cleanup_memory_sessions();
        self.cleanup_memory_kick_events();
        if self.redis_pool.is_some() {
            match self.fetch_redis_online_users().await {
                Ok(Some(users)) => {
                    self.mark_redis_success().await;
                    return Ok(users);
                }
                Ok(None) => {}
                Err(_) => self.mark_fallback().await,
            }
        }

        Ok(self
            .sessions
            .iter()
            .map(|entry| entry.key().clone())
            .collect::<Vec<_>>())
    }

    async fn get_online_status(&self, user_id: &str) -> Result<OnlineSessionStatus, DomainError> {
        let normalized_user_id = user_id.trim();
        if normalized_user_id.is_empty() {
            return Err(DomainError::ValidationError("user_id is required".into()));
        }

        self.cleanup_memory_user_if_stale(normalized_user_id);
        self.cleanup_memory_kick_event_if_stale(normalized_user_id);
        if self.redis_pool.is_some() {
            match self.fetch_redis_status(normalized_user_id).await {
                Ok(Some(status)) => {
                    self.mark_redis_success().await;
                    return Ok(status);
                }
                Ok(None) => {}
                Err(_) => self.mark_fallback().await,
            }
        }

        let session = self.sessions.get(normalized_user_id).map(|entry| entry.value().clone());
        let kick_event = self.get_memory_kick_event(normalized_user_id);
        Ok(self.build_status(normalized_user_id, session.as_ref(), kick_event))
    }

    async fn get_all_online_status(&self) -> Result<Vec<OnlineSessionStatus>, DomainError> {
        let users = self.get_online_users().await?;
        let mut result = Vec::new();
        for user_id in users {
            let status = self.get_online_status(&user_id).await?;
            if status.status != "offline" {
                result.push(status);
            }
        }
        Ok(result)
    }

    async fn get_runtime_status(&self) -> Result<SessionRuntimeStatus, DomainError> {
        let ts = self.fallback_since.load(Ordering::Relaxed);
        let fallback_since = if ts > 0 { DateTime::from_timestamp(ts, 0) } else { None };
        let redis_available = if let Some(redis_pool) = &self.redis_pool {
            let mut connection = redis_pool
                .get()
                .await
                .map_err(|_| DomainError::Internal("redis pool error".to_string()))?;
            redis::cmd("PING")
                .query_async::<String>(&mut *connection)
                .await
                .map(|response| response.eq_ignore_ascii_case("PONG"))
                .unwrap_or(false)
        } else {
            false
        };

        let mode = if self.redis_pool.is_none() || fallback_since.is_some() {
            "fallback"
        } else {
            "redis"
        };
        let fallback_duration_seconds = fallback_since.map(|since| (Utc::now() - since).num_seconds());
        let circuit_state = if self.redis_pool.is_none() {
            "fallback_memory"
        } else if fallback_since.is_some() {
            "open"
        } else {
            "closed"
        };

        Ok(SessionRuntimeStatus {
            mode: mode.to_string(),
            fallback_since,
            fallback_duration_seconds,
            circuit_state: circuit_state.to_string(),
            redis_available,
        })
    }

    async fn get_permission_version(&self, user_id: &str) -> Result<i64, DomainError> {
        let normalized_user_id = user_id.trim();
        if normalized_user_id.is_empty() {
            return Ok(1);
        }

        let Some(redis_pool) = &self.redis_pool else {
            return Ok(1);
        };

        let mut connection = redis_pool
            .get()
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        let raw: Option<i64> = redis::cmd("GET")
            .arg(Self::permission_version_key(normalized_user_id))
            .query_async(&mut *connection)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;

        Ok(raw.unwrap_or(1).max(1))
    }
}

fn normalize_optional(value: Option<&str>) -> Option<String> {
    let normalized = value.unwrap_or("").trim();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_async<F>(future: F) -> F::Output
    where
        F: std::future::Future,
    {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime")
            .block_on(future)
    }

    #[test]
    fn prunes_stale_refresh_tokens_for_distinct_users_in_fallback_mode() {
        let mut repository = InMemorySessionRuntimeRepository::new(60);
        repository.refresh_ttl_seconds = 1;

        run_async(repository.establish_session("user-a", None, Some("token-a"))).expect("session for user-a");

        run_async(async {
            if let Some(mut entry) = repository.refresh_tokens.get_mut("user-a") {
                entry.touched_at = Utc::now() - chrono::Duration::seconds(3);
            }
        });

        run_async(repository.establish_session("user-b", None, Some("token-b"))).expect("session for user-b");

        let user_a_valid = run_async(repository.validate_refresh_token("user-a", "token-a")).expect("validate user-a");
        let user_b_valid = run_async(repository.validate_refresh_token("user-b", "token-b")).expect("validate user-b");

        assert!(!user_a_valid);
        assert!(user_b_valid);

        assert!(!repository.refresh_tokens.contains_key("user-a"));
        assert!(repository.refresh_tokens.contains_key("user-b"));
        assert_eq!(repository.refresh_tokens.len(), 1);
    }

    #[test]
    fn keeps_existing_per_user_refresh_token_cap() {
        let repository = InMemorySessionRuntimeRepository::new(60);

        for index in 0..=MAX_REFRESH_TOKENS_PER_USER {
            run_async(repository.establish_session("user-a", None, Some(&format!("token-{index}"))))
                .expect("session for user-a");
        }

        let entry = repository.refresh_tokens.get("user-a").expect("tokens for user-a");
        let tokens = &entry.value().tokens;
        assert_eq!(tokens.len(), MAX_REFRESH_TOKENS_PER_USER);
        assert_eq!(tokens.first().map(String::as_str), Some("token-1"));
        assert_eq!(tokens.last().map(String::as_str), Some("token-8"));
    }

    #[test]
    fn prunes_stale_sessions_during_fallback_writes() {
        let mut repository = InMemorySessionRuntimeRepository::new(60);
        repository.online_ttl_seconds = 1;

        run_async(repository.establish_session("stale-user", None, None)).expect("session for stale-user");

        repository.sessions.insert(
            "stale-user".to_string(),
            SessionRecord {
                session_id: "stale-session".to_string(),
                login_time: Utc::now() - chrono::Duration::seconds(3),
                last_seen: Utc::now() - chrono::Duration::seconds(3),
                client_ip: None,
            },
        );

        run_async(repository.establish_session("fresh-user", None, None)).expect("session for fresh-user");

        assert!(!repository.sessions.contains_key("stale-user"));
        assert!(repository.sessions.contains_key("fresh-user"));
    }

    #[test]
    fn prunes_stale_kick_events_during_fallback_writes() {
        let repository = InMemorySessionRuntimeRepository::new(60);

        repository.kick_events.insert(
            "stale-user".to_string(),
            KickEventRecord {
                event: SessionKickEvent {
                    reason: "admin_kick".to_string(),
                    at: Utc::now() - chrono::Duration::seconds(5),
                },
                expires_at: Utc::now() - chrono::Duration::seconds(1),
            },
        );

        run_async(repository.revoke_session("fresh-user", "admin_kick")).expect("revoke fresh-user");

        assert!(!repository.kick_events.contains_key("stale-user"));
        assert!(repository.kick_events.contains_key("fresh-user"));
    }
}
