//! 运行环境判定：跨层共享的唯一口径。
//!
//! 放在 `fms-runtime` 是因为 `infrastructure` 只能依赖 `domain` 与本 crate，
//! 而原先的实现位于 `server/src/config.rs`，适配器层读不到，只能各自复制一份
//! 环境变量解析逻辑——复制出来的版本一旦 fail-open，就会与权威口径分叉。

/// 运行环境分类。
///
/// Fail-closed：缺失、空白、拼写错误、未知取值一律按 `Production` 处理，
/// 这样「忘了设环境变量」永远不会静默关掉安全加固或数据持久化要求。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeEnvironment {
    /// 开发/测试环境，允许放宽的默认值。
    Development,
    /// 生产环境，要求严格配置。
    Production,
}

impl RuntimeEnvironment {
    /// 解析环境变量取值；未知值映射为 `Production`。
    pub fn from_env_value(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            None | Some("") => RuntimeEnvironment::Production,
            Some(v)
                if v.eq_ignore_ascii_case("development")
                    || v.eq_ignore_ascii_case("dev")
                    || v.eq_ignore_ascii_case("test")
                    || v.eq_ignore_ascii_case("testing")
                    || v.eq_ignore_ascii_case("local")
                    || v.eq_ignore_ascii_case("localhost") =>
            {
                RuntimeEnvironment::Development
            }
            Some(_) => RuntimeEnvironment::Production,
        }
    }

    pub fn is_production(&self) -> bool {
        matches!(self, RuntimeEnvironment::Production)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            RuntimeEnvironment::Development => "development",
            RuntimeEnvironment::Production => "production",
        }
    }
}

/// 按 `APP_ENVIRONMENT` → `APP_ENV` → `ENVIRONMENT` 顺序读取当前环境名。
///
/// 返回 `None` 表示三者都未设置或全为空白，交由 [`RuntimeEnvironment::from_env_value`]
/// 按 fail-closed 规则处理。
pub fn runtime_environment() -> Option<String> {
    std::env::var("APP_ENVIRONMENT")
        .or_else(|_| std::env::var("APP_ENV"))
        .or_else(|_| std::env::var("ENVIRONMENT"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// 读取当前环境并按 fail-closed 规则归类。
pub fn current() -> RuntimeEnvironment {
    RuntimeEnvironment::from_env_value(runtime_environment().as_deref())
}

#[cfg(test)]
mod tests {
    use super::RuntimeEnvironment;

    #[test]
    fn runtime_environment_enum_parses_known_development_values() {
        assert_eq!(
            RuntimeEnvironment::from_env_value(Some("development")),
            RuntimeEnvironment::Development
        );
        assert_eq!(
            RuntimeEnvironment::from_env_value(Some("dev")),
            RuntimeEnvironment::Development
        );
        assert_eq!(
            RuntimeEnvironment::from_env_value(Some("test")),
            RuntimeEnvironment::Development
        );
        assert_eq!(
            RuntimeEnvironment::from_env_value(Some("testing")),
            RuntimeEnvironment::Development
        );
        assert_eq!(
            RuntimeEnvironment::from_env_value(Some("local")),
            RuntimeEnvironment::Development
        );
        assert_eq!(
            RuntimeEnvironment::from_env_value(Some("localhost")),
            RuntimeEnvironment::Development
        );
    }

    #[test]
    fn runtime_environment_enum_defaults_to_production_for_unknown_values() {
        assert_eq!(
            RuntimeEnvironment::from_env_value(Some("production")),
            RuntimeEnvironment::Production
        );
        assert_eq!(
            RuntimeEnvironment::from_env_value(Some("prod")),
            RuntimeEnvironment::Production
        );
        assert_eq!(
            RuntimeEnvironment::from_env_value(Some("staging")),
            RuntimeEnvironment::Production
        );
        assert_eq!(
            RuntimeEnvironment::from_env_value(Some("unknown_typo")),
            RuntimeEnvironment::Production
        );
        assert_eq!(RuntimeEnvironment::from_env_value(None), RuntimeEnvironment::Production);
        assert_eq!(
            RuntimeEnvironment::from_env_value(Some("")),
            RuntimeEnvironment::Production
        );
        assert_eq!(
            RuntimeEnvironment::from_env_value(Some("  ")),
            RuntimeEnvironment::Production
        );
    }

    #[test]
    fn runtime_environment_enum_is_case_insensitive() {
        assert_eq!(
            RuntimeEnvironment::from_env_value(Some("DEVELOPMENT")),
            RuntimeEnvironment::Development
        );
        assert_eq!(
            RuntimeEnvironment::from_env_value(Some("Development")),
            RuntimeEnvironment::Development
        );
        assert_eq!(
            RuntimeEnvironment::from_env_value(Some("PRODUCTION")),
            RuntimeEnvironment::Production
        );
    }

    #[test]
    fn runtime_environment_enum_as_str_returns_canonical_name() {
        assert_eq!(RuntimeEnvironment::Development.as_str(), "development");
        assert_eq!(RuntimeEnvironment::Production.as_str(), "production");
    }

    #[test]
    fn runtime_environment_enum_is_production_matches_from_env_value() {
        assert!(RuntimeEnvironment::from_env_value(None).is_production());
        assert!(RuntimeEnvironment::from_env_value(Some("production")).is_production());
        assert!(RuntimeEnvironment::from_env_value(Some("unknown")).is_production());
        assert!(!RuntimeEnvironment::from_env_value(Some("development")).is_production());
        assert!(!RuntimeEnvironment::from_env_value(Some("test")).is_production());
    }
}
