//! Redis 缓存装饰器 —— CachedFlightRepository
//!
//! 包装 `PgFlightRepository`，为高频查询添加 Redis 缓存能力。
//! 缓存策略：
//! - `find_by_id`: 缓存 key `flight:{flight_id}`，TTL 60 秒
//! - `find_by_date`: 缓存 key `flight:date:{date}`，TTL 30 秒
//! - `find_by_status`: 缓存 key `flight:status:{status}:{limit}:{offset}`，TTL 30 秒
//! - 写操作通过 cache-aside 模式使相关缓存失效

use async_trait::async_trait;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};
use tracing::{debug, warn};

use fms_domain::error::DomainError;
use fms_domain::models::flight::Flight;
use fms_domain::ports::flight_repository::{
    FlightRepository, FlightSearchCriteria, FlightTransactionalRepository, FlightUpdatePatch,
};

use crate::cache::RedisPool;
use crate::repositories::pg_flight_repository::PgFlightRepository;

/// 缓存 TTL 配置（秒）
const CACHE_TTL_BY_ID: u64 = 60;
const CACHE_TTL_BY_DATE: u64 = 30;
const CACHE_TTL_BY_STATUS: u64 = 30;
const CACHE_TTL_SEARCH: u64 = 15;
const CACHE_TTL_FIND_ALL: u64 = 5;

/// 航班缓存聚合体
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FlightCacheEntry {
    flight: Flight,
}

/// 航班列表缓存
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FlightListCacheEntry {
    flights: Vec<Flight>,
}

/// Redis 缓存装饰器
pub struct CachedFlightRepository {
    inner: PgFlightRepository,
    redis: RedisPool,
}

impl CachedFlightRepository {
    pub fn new(inner: PgFlightRepository, redis: RedisPool) -> Self {
        Self { inner, redis }
    }

    /// 构建按 ID 查询的缓存 key
    fn cache_key_by_id(flight_id: &str) -> String {
        format!("flight:{}", flight_id)
    }

    /// 构建按日期查询的缓存 key
    fn cache_key_by_date(date: &NaiveDate) -> String {
        format!("flight:date:{}", date)
    }

    /// 构建按状态查询的缓存 key
    fn cache_key_by_status(status: i32, limit: i64, offset: i64) -> String {
        format!("flight:status:{}:{}:{}", status, limit, offset)
    }

    /// 构建搜索查询的缓存 key
    fn cache_key_search(criteria: &FlightSearchCriteria, limit: i64, offset: i64) -> String {
        let flight_no = criteria.flight_no.as_deref().unwrap_or("");
        let status = criteria.status.as_deref().unwrap_or("");
        let origin = criteria.origin.as_deref().unwrap_or("");
        let destination = criteria.destination.as_deref().unwrap_or("");
        format!(
            "flight:search:{}:{}:{}:{}:{}:{}",
            flight_no, status, origin, destination, limit, offset
        )
    }

    /// 构建 find_all 查询的缓存 key
    fn cache_key_find_all(limit: i64, offset: i64) -> String {
        format!("flight:find_all:{}:{}", limit, offset)
    }

    /// 所有航班列表缓存 key 的 Redis glob 模式。
    fn list_cache_key_patterns() -> &'static [&'static str] {
        &[
            "flight:find_all:*",
            "flight:date:*",
            "flight:status:*",
            "flight:search:*",
        ]
    }

    fn cache_keys_by_flight_ids<'a>(flight_ids: impl IntoIterator<Item = &'a str>) -> Vec<String> {
        flight_ids.into_iter().map(Self::cache_key_by_id).collect()
    }

    /// 尝试从 Redis 读取缓存
    async fn get_from_cache<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Option<T> {
        let mut conn = match self.redis.get().await {
            Ok(c) => c,
            Err(e) => {
                warn!(error = %e, key = key, "获取 Redis 连接失败，跳过缓存读取");
                return None;
            }
        };

        let result = redis::cmd("GET")
            .arg(key)
            .query_async::<Option<String>>(&mut *conn)
            .await;

        match result {
            Ok(Some(json_str)) => match serde_json::from_str::<T>(&json_str) {
                Ok(entry) => Some(entry),
                Err(e) => {
                    warn!(error = %e, key = key, "缓存数据反序列化失败");
                    None
                }
            },
            Ok(None) => None,
            Err(e) => {
                warn!(error = %e, key = key, "Redis GET 操作失败");
                None
            }
        }
    }

    /// 将数据写入 Redis 缓存
    async fn set_cache<T: Serialize>(&self, key: &str, value: &T, ttl: u64) {
        let mut conn = match self.redis.get().await {
            Ok(c) => c,
            Err(e) => {
                warn!(error = %e, key = key, "获取 Redis 连接失败，跳过缓存写入");
                return;
            }
        };

        let json_str = match serde_json::to_string(value) {
            Ok(s) => s,
            Err(e) => {
                warn!(error = %e, key = key, "缓存序列化失败");
                return;
            }
        };

        if let Err(e) = redis::cmd("SETEX")
            .arg(key)
            .arg(ttl)
            .arg(&json_str)
            .query_async::<()>(&mut *conn)
            .await
        {
            warn!(error = %e, key = key, "Redis SETEX 操作失败");
        }
    }

    async fn delete_keys_matching_pattern<C>(conn: &mut C, pattern: &str)
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
                    warn!(error = %e, pattern = pattern, "Redis SCAN 操作失败");
                    return;
                }
            };

            if !keys.is_empty() {
                if let Err(e) = redis::cmd("DEL").arg(&keys).query_async::<()>(&mut *conn).await {
                    warn!(error = %e, pattern = pattern, "Redis DEL 列表缓存操作失败");
                }
            }

            if next_cursor == 0 {
                break;
            }
            cursor = next_cursor;
        }
    }

    /// 使指定航班以及所有航班列表缓存失效
    async fn invalidate_flight_cache(&self, flight_id: &str) {
        let key = Self::cache_key_by_id(flight_id);

        let mut conn = match self.redis.get().await {
            Ok(c) => c,
            Err(e) => {
                warn!(error = %e, flight_id = flight_id, "获取 Redis 连接失败，跳过缓存失效");
                return;
            }
        };

        if let Err(e) = redis::cmd("DEL").arg(&key).query_async::<()>(&mut *conn).await {
            warn!(error = %e, key = key, "Redis DEL 操作失败");
        }

        for pattern in Self::list_cache_key_patterns() {
            Self::delete_keys_matching_pattern(&mut *conn, pattern).await;
        }

        debug!(flight_id = flight_id, "航班及列表缓存已失效");
    }

    /// 批量使指定航班以及所有航班列表缓存失效
    async fn invalidate_flights_cache(&self, flight_ids: &[&str]) {
        let keys = Self::cache_keys_by_flight_ids(flight_ids.iter().copied());
        if keys.is_empty() {
            return;
        }

        let mut conn = match self.redis.get().await {
            Ok(c) => c,
            Err(e) => {
                warn!(error = %e, flight_count = flight_ids.len(), "获取 Redis 连接失败，跳过批量缓存失效");
                return;
            }
        };

        if let Err(e) = redis::cmd("DEL").arg(&keys).query_async::<()>(&mut *conn).await {
            warn!(error = %e, flight_count = flight_ids.len(), "Redis DEL 批量航班缓存操作失败");
        }

        for pattern in Self::list_cache_key_patterns() {
            Self::delete_keys_matching_pattern(&mut *conn, pattern).await;
        }

        debug!(flight_count = flight_ids.len(), "批量航班及列表缓存已失效");
    }
}

#[async_trait]
impl FlightRepository for CachedFlightRepository {
    async fn find_by_id(&self, flight_id: &str) -> Result<Option<Flight>, DomainError> {
        let cache_key = Self::cache_key_by_id(flight_id);

        // 尝试从缓存读取
        if let Some(entry) = self.get_from_cache::<FlightCacheEntry>(&cache_key).await {
            debug!(flight_id = flight_id, "航班缓存命中");
            return Ok(Some(entry.flight));
        }

        // 缓存未命中，回源数据库
        let flight = self.inner.find_by_id(flight_id).await?;

        // 异步写入缓存
        if let Some(ref flight) = flight {
            let entry = FlightCacheEntry { flight: flight.clone() };
            self.set_cache(&cache_key, &entry, CACHE_TTL_BY_ID).await;
        }

        Ok(flight)
    }

    async fn find_all(&self, limit: i64, offset: i64) -> Result<Vec<Flight>, DomainError> {
        let cache_key = Self::cache_key_find_all(limit, offset);

        if let Some(entry) = self.get_from_cache::<FlightListCacheEntry>(&cache_key).await {
            debug!(limit = limit, offset = offset, "航班列表缓存命中");
            return Ok(entry.flights);
        }

        let flights = self.inner.find_all(limit, offset).await?;

        let entry = FlightListCacheEntry {
            flights: flights.clone(),
        };
        self.set_cache(&cache_key, &entry, CACHE_TTL_FIND_ALL).await;

        Ok(flights)
    }

    async fn find_by_date(&self, date: NaiveDate) -> Result<Vec<Flight>, DomainError> {
        let cache_key = Self::cache_key_by_date(&date);

        // 尝试从缓存读取
        if let Some(entry) = self.get_from_cache::<FlightListCacheEntry>(&cache_key).await {
            debug!(date = %date, "航班日期缓存命中");
            return Ok(entry.flights);
        }

        // 缓存未命中，回源数据库
        let flights = self.inner.find_by_date(date).await?;

        // 写入缓存
        let entry = FlightListCacheEntry { flights };
        self.set_cache(&cache_key, &entry, CACHE_TTL_BY_DATE).await;

        Ok(entry.flights.clone())
    }

    async fn find_by_flight_number(&self, flight_no: &str) -> Result<Vec<Flight>, DomainError> {
        // 航班号查询不缓存
        self.inner.find_by_flight_number(flight_no).await
    }

    async fn find_by_status(&self, status: i32, limit: i64, offset: i64) -> Result<Vec<Flight>, DomainError> {
        let cache_key = Self::cache_key_by_status(status, limit, offset);

        // 尝试从缓存读取
        if let Some(entry) = self.get_from_cache::<FlightListCacheEntry>(&cache_key).await {
            debug!(status = status, limit = limit, offset = offset, "航班状态缓存命中");
            return Ok(entry.flights);
        }

        // 缓存未命中，回源数据库
        let flights = self.inner.find_by_status(status, limit, offset).await?;

        // 写入缓存
        let entry = FlightListCacheEntry { flights };
        self.set_cache(&cache_key, &entry, CACHE_TTL_BY_STATUS).await;

        Ok(entry.flights.clone())
    }

    async fn save(&self, flight: &Flight) -> Result<(), DomainError> {
        self.inner.save(flight).await?;

        // 使相关缓存失效
        self.invalidate_flight_cache(flight.flight_id.as_str()).await;

        Ok(())
    }

    async fn update_partial(&self, flight_id: &str, patch: &FlightUpdatePatch) -> Result<Option<Flight>, DomainError> {
        let result = self.inner.update_partial(flight_id, patch).await?;

        // 使缓存失效
        self.invalidate_flight_cache(flight_id).await;

        Ok(result)
    }

    async fn save_batch(&self, flights: &[Flight]) -> Result<usize, DomainError> {
        let result = self.inner.save_batch(flights).await?;

        // 批量使缓存失效
        let flight_ids: Vec<&str> = flights.iter().map(|flight| flight.flight_id.as_str()).collect();
        self.invalidate_flights_cache(&flight_ids).await;

        Ok(result)
    }

    async fn update_status(&self, flight_id: &str, status: i32) -> Result<bool, DomainError> {
        let result = self.inner.update_status(flight_id, status).await?;

        if result {
            self.invalidate_flight_cache(flight_id).await;
        }

        Ok(result)
    }

    async fn delete(&self, flight_id: &str) -> Result<bool, DomainError> {
        let result = self.inner.delete(flight_id).await?;

        if result {
            self.invalidate_flight_cache(flight_id).await;
        }

        Ok(result)
    }

    async fn search(
        &self,
        criteria: &FlightSearchCriteria,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Flight>, DomainError> {
        let cache_key = Self::cache_key_search(criteria, limit, offset);

        // 尝试从缓存读取
        if let Some(entry) = self.get_from_cache::<FlightListCacheEntry>(&cache_key).await {
            debug!(criteria = ?criteria, "航班搜索缓存命中");
            return Ok(entry.flights);
        }

        // 缓存未命中，回源数据库
        let flights = self.inner.search(criteria, limit, offset).await?;

        // 写入缓存（较短的 TTL）
        let entry = FlightListCacheEntry { flights };
        self.set_cache(&cache_key, &entry, CACHE_TTL_SEARCH).await;

        Ok(entry.flights.clone())
    }

    async fn count_by_date(&self, date: NaiveDate) -> Result<i64, DomainError> {
        // 计数不缓存
        self.inner.count_by_date(date).await
    }
}

#[async_trait]
impl<'tx> FlightTransactionalRepository<Transaction<'tx, Postgres>> for CachedFlightRepository {
    async fn save_in_tx(&self, tx: &mut Transaction<'tx, Postgres>, flight: &Flight) -> Result<(), DomainError> {
        self.inner.save_in_tx(tx, flight).await?;
        self.invalidate_flight_cache(flight.flight_id.as_str()).await;
        Ok(())
    }

    async fn update_partial_in_tx(
        &self,
        tx: &mut Transaction<'tx, Postgres>,
        flight_id: &str,
        patch: &FlightUpdatePatch,
    ) -> Result<Option<Flight>, DomainError> {
        let result = self.inner.update_partial_in_tx(tx, flight_id, patch).await?;
        self.invalidate_flight_cache(flight_id).await;
        Ok(result)
    }

    async fn delete_in_tx(&self, tx: &mut Transaction<'tx, Postgres>, flight_id: &str) -> Result<bool, DomainError> {
        let deleted = self.inner.delete_in_tx(tx, flight_id).await?;
        if deleted {
            self.invalidate_flight_cache(flight_id).await;
        }
        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pattern_matches_key(pattern: &str, key: &str) -> bool {
        pattern.strip_suffix('*').is_some_and(|prefix| key.starts_with(prefix))
    }

    #[test]
    fn write_invalidation_patterns_cover_all_flight_list_cache_shapes() {
        let date = NaiveDate::from_ymd_opt(2026, 6, 14).expect("valid test date");
        let criteria = FlightSearchCriteria {
            flight_no: Some("CA123".to_string()),
            status: Some("scheduled".to_string()),
            origin: Some("PEK".to_string()),
            destination: Some("SHA".to_string()),
            has_open_anomaly: None,
        };
        let list_keys = [
            CachedFlightRepository::cache_key_find_all(50, 25),
            CachedFlightRepository::cache_key_by_date(&date),
            CachedFlightRepository::cache_key_by_status(1, 50, 25),
            CachedFlightRepository::cache_key_search(&criteria, 50, 25),
        ];

        for key in list_keys {
            assert!(
                CachedFlightRepository::list_cache_key_patterns()
                    .iter()
                    .any(|pattern| pattern_matches_key(pattern, &key)),
                "list cache key {key} is not covered by write invalidation patterns"
            );
        }
    }

    #[test]
    fn write_invalidation_patterns_do_not_delete_flight_id_cache_by_pattern() {
        let flight_key = CachedFlightRepository::cache_key_by_id("FL-42");

        assert!(
            !CachedFlightRepository::list_cache_key_patterns()
                .iter()
                .any(|pattern| pattern_matches_key(pattern, &flight_key)),
            "flight id cache should be deleted explicitly, not through list cache patterns"
        );
    }

    #[test]
    fn batch_invalidation_builds_all_flight_id_cache_keys() {
        let keys = CachedFlightRepository::cache_keys_by_flight_ids(["FL-1", "FL-2", "FL-3"]);

        assert_eq!(keys, vec!["flight:FL-1", "flight:FL-2", "flight:FL-3"]);
    }
}
