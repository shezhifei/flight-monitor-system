//! 日志初始化

use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// 初始化 tracing 日志系统
///
/// - 开发环境: 美观的 pretty 格式
/// - 生产环境: JSON 格式
pub fn init_logging(json_format: bool) {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn,hyper=warn"));

    if json_format {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt::layer().json())
            .init();
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt::layer().pretty())
            .init();
    }

    tracing::info!("日志系统已初始化 (json={})", json_format);
}
