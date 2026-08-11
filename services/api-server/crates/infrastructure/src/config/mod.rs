//! 应用配置

use serde::Deserialize;

/// 应用配置结构
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub api: ApiConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub cache: CacheConfig,
    pub app: AppMeta,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    #[serde(default = "default_db_host")]
    pub host: String,
    #[serde(default = "default_db_port")]
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: String,
    #[serde(default = "default_pool_size")]
    pub max_connections: u32,
    /// Minimum connections to keep alive (default: 2)
    #[serde(default = "default_min_connections")]
    pub min_connections: u32,
    /// Maximum time to wait for a connection from pool (default: 5s)
    #[serde(default = "default_acquire_timeout_secs")]
    pub acquire_timeout_secs: u64,
    /// Test connections before use (default: true)
    #[serde(default = "default_test_before_acquire")]
    pub test_before_acquire: bool,
    /// Idle timeout for connections in seconds (default: 600)
    #[serde(default = "default_idle_timeout_secs")]
    pub idle_timeout_secs: u64,
    /// Maximum lifetime for connections in seconds (default: 1800)
    #[serde(default = "default_max_lifetime_secs")]
    pub max_lifetime_secs: u64,
    #[serde(default)]
    pub ssl_mode: Option<String>,
    #[serde(default)]
    pub ssl_root_cert: Option<String>,
    #[serde(default)]
    pub ssl_client_cert: Option<String>,
    #[serde(default)]
    pub ssl_client_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RedisConfig {
    #[serde(default = "default_redis_url")]
    pub url: String,
    #[serde(default)]
    pub sentinel_urls: Option<Vec<String>>,
    #[serde(default = "default_redis_sentinel_master_name")]
    pub sentinel_master_name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CacheConfig {
    /// Default TTL for cache entries in seconds (default: 300)
    #[serde(default = "default_cache_ttl_secs")]
    pub default_ttl_secs: u64,
    /// Maximum number of entries in local cache (default: 1000)
    #[serde(default = "default_local_cache_max_entries")]
    pub local_cache_max_entries: usize,
    /// Enable/disable caching globally (default: true)
    #[serde(default = "default_cache_enabled")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppMeta {
    #[serde(default = "default_app_name")]
    pub name: String,
    #[serde(default = "default_app_version")]
    pub version: String,
    #[serde(default = "default_environment")]
    pub environment: String,
}

impl DatabaseConfig {
    /// 构造 PostgreSQL 连接字符串
    pub fn connection_string(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}",
            self.username, self.password, self.host, self.port, self.database
        )
    }
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}
fn default_port() -> u16 {
    8080
}
fn default_db_host() -> String {
    "localhost".to_string()
}
fn default_db_port() -> u16 {
    5432
}
fn default_pool_size() -> u32 {
    10
}
fn default_min_connections() -> u32 {
    2
}
fn default_acquire_timeout_secs() -> u64 {
    5
}
fn default_test_before_acquire() -> bool {
    false
}
fn default_idle_timeout_secs() -> u64 {
    600
}
fn default_max_lifetime_secs() -> u64 {
    1800
}
fn default_redis_url() -> String {
    "redis://127.0.0.1:6379".to_string()
}
fn default_redis_sentinel_master_name() -> String {
    "mymaster".to_string()
}
fn default_cache_ttl_secs() -> u64 {
    300
}
fn default_local_cache_max_entries() -> usize {
    1000
}
fn default_cache_enabled() -> bool {
    true
}
fn default_app_name() -> String {
    "Flight Monitor".to_string()
}
fn default_app_version() -> String {
    "0.1.0".to_string()
}
fn default_environment() -> String {
    "development".to_string()
}
