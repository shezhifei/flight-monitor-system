//! 航班监控系统 — Rust 后端入口 (组合根)
//!
//! 负责：服务启动配置读取、基础设施初始化、依赖组装分发、Actix-Web 服务端启动。

#[cfg(target_os = "linux")]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

use actix_cors::Cors;
use actix_web::dev::Service;
use actix_web::{middleware::Logger, App, HttpMessage, HttpServer};
use chrono::Utc;
use fms_infrastructure::config::RedisConfig;
use fms_infrastructure::db::{build_connect_options, PostgresTlsConfig};
use futures_util::future::Either;
use metrics_exporter_prometheus::PrometheusBuilder;
use std::sync::Arc;
use tracing::{info, warn};

mod config;
mod di;
mod profiling;
mod web;

use crate::config::{
    build_request_size_error_response, env_optional_string, insert_standard_security_headers,
    install_rustls_crypto_provider, is_production_environment, is_redis_required_for_role, load_cors_allowed_origins,
    load_required_vault_rendered_env, load_rustls_server_config, max_request_size_bytes, redact_url_credentials,
    request_uses_https, resolve_http_tls_binding_config, resolve_http_tls_performance_config, resolve_jwt_audiences, resolve_jwt_secret,
    resolve_workflow_internal_token, runtime_environment, runtime_role, should_start_http_server_for_role,
    DatabaseUrlDefaults,
};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    install_rustls_crypto_provider()?;

    // 1. 加载 .env 与 Vault 渲染的环境变量
    dotenvy::dotenv().ok();
    let _rendered_env_file = load_required_vault_rendered_env(&[
        "DB_PASSWORD",
        "DB_REPLICATION_PASSWORD",
        "REDIS_PASSWORD",
        "JWT_SECRET_KEY",
        "AI_CONFIG_ENCRYPTION_KEY",
        "FLOWABLE_DB_PASSWORD",
    ])?;

    // 2. 初始化日志与全局 Panic Hook
    let json_log = std::env::var("LOG_FORMAT")
        .map(|v| v.eq_ignore_ascii_case("json"))
        .unwrap_or(false);
    fms_infrastructure::logging::init_logging(json_log);

    std::panic::set_hook(Box::new(|panic_info| {
        let (filename, line) = panic_info
            .location()
            .map(|loc| (loc.file(), loc.line()))
            .unwrap_or(("<unknown>", 0));
        let message = format!("{panic_info}");
        tracing::error!(panic = true, file = filename, line = line, "panic: {message}");
    }));

    // 3. 基本绑定地址与运行角色配置
    // 安装 Prometheus 指标 recorder（全进程仅一次）。渲染句柄注入到 app_data，
    // 供 `/metrics` 路由使用。
    let prom_handle = PrometheusBuilder::new()
        .install_recorder()
        .expect("failed to install prometheus recorder");
    let host = std::env::var("API_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port: u16 = std::env::var("API_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);

    let tls_binding_config = resolve_http_tls_binding_config(
        env_optional_string("API_TLS_ENABLED").as_deref(),
        env_optional_string("API_TLS_CERT_FILE").as_deref(),
        env_optional_string("API_TLS_KEY_FILE").as_deref(),
    );
    let runtime_role = runtime_role();
    let redis_required = is_redis_required_for_role(&runtime_role);
    let max_request_size = max_request_size_bytes();

    let runtime_env_value = runtime_environment();
    let runtime_env = crate::config::RuntimeEnvironment::from_env_value(runtime_env_value.as_deref());

    info!(
        host = %host,
        port = %port,
        runtime_role = %runtime_role,
        runtime_environment = %runtime_env.as_str(),
        runtime_environment_raw = ?runtime_env_value,
        redis_required = redis_required,
        max_request_size_bytes = max_request_size,
        version = %env!("CARGO_PKG_VERSION"),
        "航班监控系统 Rust 后端启动中"
    );

    if !runtime_env.is_production() && runtime_env_value.is_none() {
        tracing::warn!(
            "APP_ENVIRONMENT is not set; defaulting to 'production' security profile. \
             Set APP_ENVIRONMENT=development to relax security for local development."
        );
    }

    // 4. 解析 JWT 签名配置
    let jwt_secret_key = resolve_jwt_secret()?;
    let jwt_audiences = resolve_jwt_audiences()?;
    if jwt_audiences.is_empty() {
        tracing::warn!("JWT_AUDIENCE is not set. JWT audience validation is disabled only in development.");
    }
    let workflow_internal_token = resolve_workflow_internal_token()?;
    if workflow_internal_token.is_none() {
        tracing::warn!(
            "WORKFLOW_INTERNAL_TOKEN is not set. Internal workflow token auth is disabled (dev/test mode only)."
        );
    }
    let jwt_config = fms_application::services::auth_service::JwtConfig {
        secret: jwt_secret_key.clone(),
        issuer: std::env::var("JWT_ISSUER").unwrap_or_else(|_| "flight-monitor".to_string()),
        audience: jwt_audiences
            .first()
            .cloned()
            .unwrap_or_else(|| "flight-monitor-api".to_string()),
        // JwtConfig::default() is cfg(test)-gated; production must set every
        // field explicitly. Values match the previous Default impl.
        access_token_expire_secs: 3600,
        refresh_token_expire_secs: 604800,
        sse_token_expire_secs: 300,
    };

    // 5. 数据库连接池配置与建立
    let database_url = std::env::var("DATABASE_URL").map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "DATABASE_URL must be set via Vault rendered env",
        )
    })?;
    let database_url_defaults = DatabaseUrlDefaults::from_database_url(&database_url);
    let pool_max = std::env::var("DB_POOL_MAX_CONNECTIONS")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(32);
    let pool_min = std::env::var("DB_POOL_MIN_CONNECTIONS")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(4);
    let pool_acquire_timeout = std::env::var("DB_POOL_ACQUIRE_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(30);
    let pool_idle_timeout = std::env::var("DB_POOL_IDLE_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(600);
    let pool_max_lifetime = std::env::var("DB_POOL_MAX_LIFETIME_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(1800);
    let database_tls_config = PostgresTlsConfig {
        ssl_mode: env_optional_string("DB_SSL_MODE"),
        ssl_root_cert: env_optional_string("DB_SSL_ROOT_CERT"),
        ssl_client_cert: env_optional_string("DB_SSL_CLIENT_CERT"),
        ssl_client_key: env_optional_string("DB_SSL_CLIENT_KEY"),
    };
    let database_connect_options =
        build_connect_options(&database_url, &database_tls_config).map_err(crate::config::io_other)?;

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(pool_max)
        .min_connections(pool_min)
        .acquire_timeout(std::time::Duration::from_secs(pool_acquire_timeout))
        .idle_timeout(std::time::Duration::from_secs(pool_idle_timeout))
        .max_lifetime(std::time::Duration::from_secs(pool_max_lifetime))
        .connect_with(database_connect_options)
        .await
        .map_err(|error| std::io::Error::other(format!("无法连接数据库，请检查 DATABASE_URL: {error}")))?;
    info!(db_pool_max = pool_max, "数据库连接池已成功创建");
    fms_infrastructure::db::record_db_pool_connections(&pool);
    {
        let pool_for_metrics = pool.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(std::time::Duration::from_secs(15));
            loop {
                ticker.tick().await;
                fms_infrastructure::db::record_db_pool_connections(&pool_for_metrics);
            }
        });
    }

    // 6. Redis 客户端连接池配置与建立
    let redis_url = crate::config::resolve_redis_url(redis_required)?;
    let redis_config = RedisConfig {
        url: redis_url.clone(),
        sentinel_urls: std::env::var("REDIS_SENTINEL_URLS").ok().map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect()
        }),
        sentinel_master_name: std::env::var("REDIS_MASTER_NAME")
            .ok()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| "mymaster".to_string()),
    };
    let redis_manager = match fms_infrastructure::cache::create_redis_pool(&redis_config)
        .await
        .map(Arc::new)
    {
        Ok(mgr) => Some(mgr),
        Err(error) => {
            warn!(
                error = %error,
                redis_url = %redact_url_credentials(&redis_url),
                "Redis connection failed, falling back to memory mode"
            );
            if redis_required {
                return Err(std::io::Error::other(format!(
                    "Redis is required in distributed mode: connection failed: {error}"
                )));
            }
            None
        }
    };

    // 7. 执行全局依赖注入组装
    let di_container = di::build_di_container(
        pool,
        redis_manager,
        jwt_config,
        jwt_secret_key,
        jwt_audiences,
        workflow_internal_token,
        &runtime_role,
        redis_required,
        &database_url_defaults,
    )
    .await?;

    // 8. 若角色属于后台 jobs 则保持后台运行；若非 HTTP 服务则直接等待关闭
    if !should_start_http_server_for_role(&runtime_role) {
        info!(runtime_role = %runtime_role, "Worker runtime started without HTTP server; waiting for shutdown signal");
        tokio::signal::ctrl_c()
            .await
            .map_err(|error| std::io::Error::other(format!("failed to listen for shutdown signal: {error}")))?;
        info!(runtime_role = %runtime_role, "Shutdown signal received");
        if di_container.background_jobs_enabled {
            di_container.scheduler_runtime_svc.stop().await;
            di_container
                .cdc_relay_svc
                .stop()
                .await
                .map_err(crate::config::io_other)?;
        }
        return Ok(());
    }

    let cors_allowed_origins = load_cors_allowed_origins()?;
    if std::env::var("CORS_ALLOWED_ORIGINS").is_err() {
        tracing::warn!("CORS_ALLOWED_ORIGINS is not set. Using development default CORS origins.");
    }

    let is_production = is_production_environment(runtime_environment().as_deref());

    // 9. 启动 actix-web HTTP 服务层
    let di_capture = di_container.clone();
    let server = HttpServer::new(move || {
        let max_request_size_bytes = max_request_size;
        let cors_allowed_origins = cors_allowed_origins.clone();

        // CORS 设置
        let mut cors = Cors::default()
            .allowed_methods(vec!["GET", "POST", "PUT", "DELETE", "PATCH", "OPTIONS"])
            .allowed_headers(vec![
                actix_web::http::header::AUTHORIZATION,
                actix_web::http::header::ACCEPT,
                actix_web::http::header::CONTENT_TYPE,
                actix_web::http::header::ORIGIN,
                actix_web::http::header::HeaderName::from_static("x-request-id"),
                actix_web::http::header::HeaderName::from_static("x-request-timestamp"),
                actix_web::http::header::HeaderName::from_static("x-request-nonce"),
                actix_web::http::header::HeaderName::from_static("x-request-body-sha256"),
                actix_web::http::header::HeaderName::from_static("x-request-signature"),
            ])
            .expose_headers(vec![
                actix_web::http::header::CONTENT_TYPE,
                actix_web::http::header::HeaderName::from_static("x-request-id"),
            ])
            .supports_credentials()
            .max_age(600);
        for origin in &cors_allowed_origins {
            cors = cors.allowed_origin(origin);
        }

        App::new()
            .wrap(fms_api::middleware::global_error::GlobalErrorMiddleware)
            .wrap(fms_api::middleware::metrics::MetricsMiddleware)
            .wrap(Logger::default().exclude("/api/v2/flights"))
            .wrap(fms_api::middleware::anti_replay::AntiReplay::new())
            // 自定义安全防御与请求头填充中间件
            .wrap_fn(move |mut req, srv| {
                let secure_request = request_uses_https(&req);
                let request_size = req
                    .headers()
                    .get("content-length")
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse::<usize>().ok());
                let inbound_request_id = req
                    .headers()
                    .get("x-request-id")
                    .and_then(|value| value.to_str().ok())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string);
                let request_id = inbound_request_id
                    .clone()
                    .unwrap_or_else(|| format!("req_{}", Utc::now().timestamp_micros()));
                let response_request_id = inbound_request_id
                    .map(|_| format!("req_{}", Utc::now().timestamp_micros()))
                    .unwrap_or_else(|| request_id.clone());
                let header_name = actix_web::http::header::HeaderName::from_static("x-request-id");

                let oversized_response = request_size
                    .filter(|size| *size > max_request_size_bytes)
                    .map(|size| {
                        let mut response = build_request_size_error_response(
                            &response_request_id,
                            max_request_size_bytes,
                            size,
                        );
                        insert_standard_security_headers(response.headers_mut(), secure_request, is_production);
                        response
                    });

                if let Some(response) = oversized_response {
                    return Either::Left(async move {
                        Ok(req.into_response(response).map_into_right_body())
                    });
                }

                req.extensions_mut().insert(request_id.clone());
                if !req.headers().contains_key(&header_name) {
                    if let Ok(header_value) = actix_web::http::header::HeaderValue::from_str(&request_id) {
                        req.headers_mut().insert(header_name.clone(), header_value);
                    }
                }
                let fut = srv.call(req);

                Either::Right(async move {
                    let mut res = fut.await?.map_into_left_body();
                    if let Ok(header_value) = actix_web::http::header::HeaderValue::from_str(&response_request_id) {
                        res.headers_mut().insert(header_name, header_value);
                    }
                    insert_standard_security_headers(res.headers_mut(), secure_request, is_production);
                    Ok(res)
                })
            })
            .wrap(cors)
            // 暴露 Prometheus 渲染句柄给 /metrics 路由
            .app_data(actix_web::web::Data::new(prom_handle.clone()))
            // 注册路由与依赖容器数据
            .configure(|cfg| web::configure_app(cfg, &di_capture))
    });

    // 10. 服务端口侦听配置与生命周期处理
    let default_workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .max(2);
    let actix_workers = std::env::var("ACTIX_WORKERS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(default_workers);
    let server = server.workers(actix_workers);
    info!(actix_workers, "Actix-web worker threads configured");

    let bind_addr = format!("{host}:{port}");
    
    // 加载 TLS 性能优化配置
    let tls_performance_config = resolve_http_tls_performance_config();
    
    let server = if let Some(tls_binding_config) = tls_binding_config.as_ref() {
        info!(
            bind_addr = %bind_addr,
            cert_file = %tls_binding_config.cert_file,
            key_file = %tls_binding_config.key_file,
            session_timeout = tls_performance_config.session_timeout,
            enable_session_tickets = tls_performance_config.enable_session_tickets,
            "Starting HTTPS server with HTTP/2 and TLS session optimization enabled"
        );
        
        // 配置 HTTP/2 window sizes for better performance
        let mut server = server.bind_rustls_0_23(bind_addr.clone(), load_rustls_server_config(tls_binding_config, &tls_performance_config)?)?;
        
        // Apply HTTP/2 window size configurations
        server = server.h2_initial_window_size(tls_performance_config.http2_initial_stream_window_size);
        server = server.h2_initial_connection_window_size(tls_performance_config.http2_initial_connection_window_size);
        
        server
    } else {
        info!(bind_addr = %bind_addr, "Starting HTTP server");
        server.bind(bind_addr)?
    }
    .run();

    let server_result = server.await;

    // 清理后台常驻任务
    if di_container.background_jobs_enabled {
        di_container.scheduler_runtime_svc.stop().await;
        di_container
            .cdc_relay_svc
            .stop()
            .await
            .map_err(crate::config::io_other)?;
    }

    server_result.map_err(Into::into)
}
