use async_trait::async_trait;
use fms_domain::ports::flight_cache_backend::FlightCacheBackend;
use redis::AsyncCommands;
use tracing::warn;

use super::RedisPool;

const FLIGHT_DETAIL_HASH_KEY: &str = "flights:detail:hash";
const FLIGHT_LIST_HASH_KEY: &str = "flights:list:hash";
const FLIGHTS_VERSION_KEY: &str = "flights:cache:version";
const FLIGHT_DETAIL_HASH_TTL_SECONDS: i64 = 300;
const FLIGHT_LIST_HASH_TTL_SECONDS: i64 = 120;

pub struct RedisFlightCacheBackend {
    redis: RedisPool,
}

impl RedisFlightCacheBackend {
    pub fn new(redis: RedisPool) -> Self {
        Self { redis }
    }
}

#[async_trait]
impl FlightCacheBackend for RedisFlightCacheBackend {
    async fn invalidate_single_flight_cache(&self, flight_id: &str) {
        let mut conn = match self.redis.get().await {
            Ok(conn) => conn,
            Err(error) => {
                warn!(flight_id, error = %error, "failed to get redis connection from pool");
                return;
            }
        };

        if let Err(error) = conn.hdel::<_, _, ()>(FLIGHT_DETAIL_HASH_KEY, flight_id).await {
            warn!(flight_id, error = %error, "failed to invalidate single flight detail cache");
        }
        if let Err(error) = conn.hdel::<_, _, ()>(FLIGHT_LIST_HASH_KEY, flight_id).await {
            warn!(flight_id, error = %error, "failed to invalidate single flight list cache entry");
        }
    }

    async fn refresh_single_flight_cache(&self, flight_id: &str, payload: &str) {
        let mut conn = match self.redis.get().await {
            Ok(conn) => conn,
            Err(error) => {
                warn!(flight_id, error = %error, "failed to get redis connection from pool");
                return;
            }
        };

        if let Err(error) = conn
            .hset::<_, _, _, ()>(FLIGHT_DETAIL_HASH_KEY, flight_id, payload)
            .await
        {
            warn!(flight_id, error = %error, "failed to refresh single flight detail cache");
        }
        if let Err(error) = conn
            .expire::<_, ()>(FLIGHT_DETAIL_HASH_KEY, FLIGHT_DETAIL_HASH_TTL_SECONDS)
            .await
        {
            warn!(flight_id, error = %error, "failed to extend single flight detail cache ttl");
        }
        if let Err(error) = conn.hset::<_, _, _, ()>(FLIGHT_LIST_HASH_KEY, flight_id, payload).await {
            warn!(flight_id, error = %error, "failed to refresh single flight list cache entry");
        }
        if let Err(error) = conn
            .expire::<_, ()>(FLIGHT_LIST_HASH_KEY, FLIGHT_LIST_HASH_TTL_SECONDS)
            .await
        {
            warn!(flight_id, error = %error, "failed to extend single flight list cache ttl");
        }
    }

    async fn invalidate_flights_cache(&self) {
        let mut conn = match self.redis.get().await {
            Ok(conn) => conn,
            Err(error) => {
                warn!(error = %error, "failed to get redis connection from pool");
                return;
            }
        };
        if let Err(error) = conn.incr::<_, _, i64>(FLIGHTS_VERSION_KEY, 1).await {
            warn!(error = %error, "failed to invalidate flights cache version");
        }
    }
}
