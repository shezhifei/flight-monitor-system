//! Redis 连接池初始化和缓存服务

pub mod cache_service;
pub mod flight_cache_backend;

#[cfg(test)]
mod tests;

use bb8::{ManageConnection, Pool};
use bb8_redis::RedisConnectionManager;
use redis::sentinel::Sentinel;
use tracing::info;

use crate::config::RedisConfig;
use crate::error::InfraError;

// 重新导出缓存服务
pub use cache_service::{
    assemble_batch_get_results, redis_pipeline_enabled, CacheService, LocalCacheService, MultiLevelCacheService,
    RedisCacheService,
};
pub use crate::observability::{shadow_mode_enabled, serialize_json, serialize_json_pretty};

/// Redis 连接池类型别名
pub type RedisPool = Pool<RedisConnectionManager>;

/// Redis 连接池配置
#[derive(Debug, Clone)]
pub struct RedisPoolConfig {
    pub max_size: u32,
    pub min_idle: u32,
    pub test_on_check_out: bool,
    pub connection_timeout_secs: u64,
}

impl Default for RedisPoolConfig {
    fn default() -> Self {
        Self {
            max_size: std::env::var("REDIS_POOL_MAX_SIZE")
                .ok()
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(32),
            min_idle: std::env::var("REDIS_POOL_MIN_IDLE")
                .ok()
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(4),
            test_on_check_out: false,
            connection_timeout_secs: std::env::var("REDIS_POOL_CONNECT_TIMEOUT_SECS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(5),
        }
    }
}

/// 创建 Redis 连接池（使用默认配置）
pub async fn create_redis_pool(config: &RedisConfig) -> Result<RedisPool, InfraError> {
    create_redis_pool_with_config(config, &RedisPoolConfig::default()).await
}

/// 创建 Redis 连接池（使用自定义配置）
pub async fn create_redis_pool_with_config(
    config: &RedisConfig,
    pool_config: &RedisPoolConfig,
) -> Result<RedisPool, InfraError> {
    info!(
        url = %config.url,
        max_size = %pool_config.max_size,
        min_idle = %pool_config.min_idle,
        connection_timeout_secs = %pool_config.connection_timeout_secs,
        "正在创建 Redis 连接池"
    );

    let manager = build_redis_manager(config).await?;

    let pool = Pool::builder()
        .max_size(pool_config.max_size)
        .min_idle(Some(pool_config.min_idle))
        .test_on_check_out(pool_config.test_on_check_out)
        .connection_timeout(std::time::Duration::from_secs(pool_config.connection_timeout_secs))
        .build(manager)
        .await?;

    info!("Redis 连接池创建成功");
    Ok(pool)
}

async fn build_redis_manager(config: &RedisConfig) -> Result<RedisConnectionManager, InfraError> {
    let sentinel_urls = resolve_sentinel_urls(config);
    if sentinel_urls.is_empty() {
        return Ok(RedisConnectionManager::new(config.url.as_str())?);
    }

    info!(
        sentinel_count = sentinel_urls.len(),
        master = %config.sentinel_master_name,
        "正在通过 Redis Sentinel 解析主节点"
    );

    let master_name = std::env::var("REDIS_MASTER_NAME")
        .ok()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| config.sentinel_master_name.clone());
    let mut sentinel = Sentinel::build(sentinel_urls)?;
    let client = sentinel.master_for(&master_name, None)?;
    let manager = RedisConnectionManager::new(client.get_connection_info().clone())?;
    // Validate once during startup so a bad Sentinel topology fails before serving traffic.
    let mut conn = manager.connect().await?;
    manager.is_valid(&mut conn).await?;
    Ok(manager)
}

fn resolve_sentinel_urls(config: &RedisConfig) -> Vec<String> {
    let configured = config
        .sentinel_urls
        .clone()
        .unwrap_or_default()
        .into_iter()
        .map(|url| url.trim().to_string())
        .filter(|url| !url.is_empty());

    let env_urls = std::env::var("REDIS_SENTINEL_URLS")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

    configured.chain(env_urls).collect()
}

/// 测试 Redis 连接池可用性
pub async fn ping_pool(pool: &RedisPool) -> Result<bool, InfraError> {
    let mut conn = pool.get().await?;
    let pong: String = redis::cmd("PING").query_async(&mut *conn).await?;
    Ok(pong.eq_ignore_ascii_case("PONG"))
}

/// 获取 Redis 连接池状态信息
pub async fn get_pool_status(pool: &RedisPool) -> PoolStatus {
    PoolStatus {
        connections: pool.state().connections,
        idle_connections: pool.state().idle_connections,
    }
}

/// Redis 连接池状态
#[derive(Debug, Clone)]
pub struct PoolStatus {
    pub connections: u32,
    pub idle_connections: u32,
}

/// 记录一次 Redis 命令执行（成功/失败）到 `fms_redis_commands_total` (Counter)。
pub fn record_redis_command(command: &str, status: &str) {
    record_redis_command_with_latency(command, status, None);
}

pub fn record_redis_command_with_latency(command: &str, status: &str, latency: Option<std::time::Duration>) {
    metrics::counter!(
        "fms_redis_commands_total",
        "command" => command.to_string(),
        "status" => status.to_string()
    )
    .increment(1);
    if let Some(duration) = latency {
        metrics::histogram!(
            "fms_redis_command_latency_seconds",
            "command" => command.to_string()
        )
        .record(duration.as_secs_f64());
    }
}
