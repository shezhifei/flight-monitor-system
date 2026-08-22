//! 数据库连接池初始化
pub mod query_builder;
pub mod transaction;
pub mod type_mappings;

use std::path::PathBuf;
use std::str::FromStr;

use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgSslMode};
use sqlx::PgPool;
use tracing::info;

use crate::config::DatabaseConfig;
use crate::error::InfraError;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PostgresTlsConfig {
    pub ssl_mode: Option<String>,
    pub ssl_root_cert: Option<String>,
    pub ssl_client_cert: Option<String>,
    pub ssl_client_key: Option<String>,
}

enum TlsAssetInput {
    Path(PathBuf),
    Pem(Vec<u8>),
}

impl From<&DatabaseConfig> for PostgresTlsConfig {
    fn from(value: &DatabaseConfig) -> Self {
        Self {
            ssl_mode: value.ssl_mode.clone(),
            ssl_root_cert: value.ssl_root_cert.clone(),
            ssl_client_cert: value.ssl_client_cert.clone(),
            ssl_client_key: value.ssl_client_key.clone(),
        }
    }
}

pub fn build_connect_options(
    database_url: &str,
    tls_config: &PostgresTlsConfig,
) -> Result<PgConnectOptions, InfraError> {
    let mut options = PgConnectOptions::from_str(database_url).map_err(InfraError::Database)?;

    if let Some(ssl_mode) = tls_config
        .ssl_mode
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        options = options.ssl_mode(parse_ssl_mode(ssl_mode)?);
    }

    if let Some(root_cert) = tls_config
        .ssl_root_cert
        .as_deref()
        .map(parse_tls_asset_input)
        .transpose()?
    {
        options = match root_cert {
            TlsAssetInput::Path(path) => options.ssl_root_cert(path),
            TlsAssetInput::Pem(pem) => options.ssl_root_cert_from_pem(pem),
        };
    }

    let client_cert = tls_config
        .ssl_client_cert
        .as_deref()
        .map(parse_tls_asset_input)
        .transpose()?;
    let client_key = tls_config
        .ssl_client_key
        .as_deref()
        .map(parse_tls_asset_input)
        .transpose()?;

    match (client_cert, client_key) {
        (None, None) => {}
        (Some(_), None) => {
            return Err(InfraError::Config(
                "DB SSL client cert is set but DB SSL client key is missing".to_string(),
            ));
        }
        (None, Some(_)) => {
            return Err(InfraError::Config(
                "DB SSL client key is set but DB SSL client cert is missing".to_string(),
            ));
        }
        (Some(cert), Some(key)) => {
            options = match cert {
                TlsAssetInput::Path(path) => options.ssl_client_cert(path),
                TlsAssetInput::Pem(pem) => options.ssl_client_cert_from_pem(pem),
            };
            options = match key {
                TlsAssetInput::Path(path) => options.ssl_client_key(path),
                TlsAssetInput::Pem(pem) => options.ssl_client_key_from_pem(pem),
            };
        }
    }

    Ok(options)
}

/// 创建 PostgreSQL 连接池
pub async fn create_pool(config: &DatabaseConfig) -> Result<PgPool, InfraError> {
    let conn_str = config.connection_string();
    let options = build_connect_options(&conn_str, &PostgresTlsConfig::from(config))?;

    let min_conn = if config.min_connections > 0 {
        config.min_connections
    } else {
        2 // Default minimum connections
    };

    let acquire_timeout = if config.acquire_timeout_secs > 0 {
        std::time::Duration::from_secs(config.acquire_timeout_secs)
    } else {
        std::time::Duration::from_secs(5) // Default 5 second acquire timeout
    };

    let idle_timeout = if config.idle_timeout_secs > 0 {
        std::time::Duration::from_secs(config.idle_timeout_secs)
    } else {
        std::time::Duration::from_secs(600) // Default 10 minutes
    };

    let max_lifetime = if config.max_lifetime_secs > 0 {
        std::time::Duration::from_secs(config.max_lifetime_secs)
    } else {
        std::time::Duration::from_secs(1800) // Default 30 minutes
    };

    info!(
        host = %config.host,
        port = %config.port,
        database = %config.database,
        max_connections = %config.max_connections,
        min_connections = %min_conn,
        acquire_timeout_secs = %acquire_timeout.as_secs(),
        idle_timeout_secs = %idle_timeout.as_secs(),
        max_lifetime_secs = %max_lifetime.as_secs(),
        test_before_acquire = %config.test_before_acquire,
        "正在创建数据库连接池"
    );

    let pool = PgPoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(min_conn)
        .acquire_timeout(acquire_timeout)
        .idle_timeout(idle_timeout)
        .max_lifetime(max_lifetime)
        .test_before_acquire(config.test_before_acquire)
        .connect_with(options)
        .await
        .map_err(InfraError::Database)?;

    info!("数据库连接池创建成功");
    record_db_pool_connections(&pool);
    Ok(pool)
}

/// 将 `PgPool` 的当前连接数写入 Prometheus 指标
/// `fms_db_pool_connections{state="active"|"idle"}` (Gauge)。
pub fn record_db_pool_connections(pool: &PgPool) {
    let total = pool.size();
    let idle = pool.num_idle();
    let active = total.saturating_sub(idle as u32);
    metrics::gauge!("fms_db_pool_connections", "state" => "active").set(active as f64);
    metrics::gauge!("fms_db_pool_connections", "state" => "idle").set(idle as f64);
    metrics::gauge!("fms_db_pool_connections_max").set(pool.options().get_max_connections() as f64);
}

fn parse_ssl_mode(raw: &str) -> Result<PgSslMode, InfraError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "disable" => Ok(PgSslMode::Disable),
        "prefer" => Ok(PgSslMode::Prefer),
        "require" => Ok(PgSslMode::Require),
        "verify-ca" => Ok(PgSslMode::VerifyCa),
        "verify-full" => Ok(PgSslMode::VerifyFull),
        other => Err(InfraError::Config(format!(
            "Unsupported DB_SSL_MODE '{other}'. Expected one of: disable, prefer, require, verify-ca, verify-full"
        ))),
    }
}

fn parse_tls_asset_input(raw: &str) -> Result<TlsAssetInput, InfraError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(InfraError::Config(
            "DB SSL certificate/key value must not be empty".to_string(),
        ));
    }

    if is_pem_block(trimmed) {
        return Ok(TlsAssetInput::Pem(trimmed.as_bytes().to_vec()));
    }

    Ok(TlsAssetInput::Path(PathBuf::from(trimmed)))
}

fn is_pem_block(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.starts_with("-----BEGIN ") && trimmed.contains("-----END ")
}

#[cfg(test)]
mod tests {
    use super::{build_connect_options, PostgresTlsConfig};
    use crate::error::InfraError;
    use sqlx::postgres::PgSslMode;

    fn test_database_url() -> String {
        std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://postgres@localhost:5432/fms".to_string())
    }

    #[test]
    fn build_connect_options_applies_ssl_mode_override() {
        let options = build_connect_options(
            &test_database_url(),
            &PostgresTlsConfig {
                ssl_mode: Some("verify-full".to_string()),
                ..PostgresTlsConfig::default()
            },
        )
        .expect("options should build");

        assert!(matches!(options.get_ssl_mode(), PgSslMode::VerifyFull));
    }

    #[test]
    fn build_connect_options_rejects_unsupported_ssl_mode() {
        let error = build_connect_options(
            &test_database_url(),
            &PostgresTlsConfig {
                ssl_mode: Some("allow".to_string()),
                ..PostgresTlsConfig::default()
            },
        )
        .expect_err("invalid mode should fail");

        assert!(matches!(error, InfraError::Config(_)));
    }

    #[test]
    fn build_connect_options_requires_client_cert_and_key_as_pair() {
        let error = build_connect_options(
            &test_database_url(),
            &PostgresTlsConfig {
                ssl_client_cert: Some("certs/client.crt".to_string()),
                ..PostgresTlsConfig::default()
            },
        )
        .expect_err("cert without key should fail");

        assert!(matches!(error, InfraError::Config(_)));
    }

    #[test]
    fn build_connect_options_accepts_pem_inputs_for_rustls() {
        let options = build_connect_options(
            &test_database_url(),
            &PostgresTlsConfig {
                ssl_root_cert: Some("-----BEGIN CERTIFICATE-----\nZm9v\n-----END CERTIFICATE-----".to_string()),
                ssl_client_cert: Some("-----BEGIN CERTIFICATE-----\nYmFy\n-----END CERTIFICATE-----".to_string()),
                ssl_client_key: Some("-----BEGIN PRIVATE KEY-----\nYmF6\n-----END PRIVATE KEY-----".to_string()),
                ..PostgresTlsConfig::default()
            },
        )
        .expect("pem inputs should be accepted");

        assert!(matches!(options.get_ssl_mode(), PgSslMode::Prefer));
    }
}
