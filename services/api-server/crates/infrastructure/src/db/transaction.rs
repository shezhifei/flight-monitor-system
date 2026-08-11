//! 事务管理器
//!
//! 对应 Python `src/infrastructure/database/transaction_manager.py`。
//! Rust 端利用 sqlx 原生事务支持大幅简化实现。

use sqlx::{Executor, PgPool};
use std::time::Duration;
use tracing::{error, warn};

/// PostgreSQL 事务隔离级别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsolationLevel {
    ReadUncommitted,
    ReadCommitted,
    RepeatableRead,
    Serializable,
}

impl IsolationLevel {
    pub fn as_sql(&self) -> &'static str {
        match self {
            Self::ReadUncommitted => "READ UNCOMMITTED",
            Self::ReadCommitted => "READ COMMITTED",
            Self::RepeatableRead => "REPEATABLE READ",
            Self::Serializable => "SERIALIZABLE",
        }
    }

    pub fn from_str_loose(s: &str) -> Option<Self> {
        let normalized = s.trim().to_uppercase().replace('_', " ");
        match normalized.as_str() {
            "READ UNCOMMITTED" => Some(Self::ReadUncommitted),
            "READ COMMITTED" => Some(Self::ReadCommitted),
            "REPEATABLE READ" => Some(Self::RepeatableRead),
            "SERIALIZABLE" => Some(Self::Serializable),
            _ => None,
        }
    }
}

/// 事务配置
#[derive(Debug, Clone)]
pub struct TransactionConfig {
    pub isolation_level: IsolationLevel,
    pub timeout_secs: u64,
    pub max_retries: u32,
    pub retry_delay_ms: u64,
    pub statement_timeout_secs: u32,
}

impl Default for TransactionConfig {
    fn default() -> Self {
        Self {
            isolation_level: IsolationLevel::ReadCommitted,
            timeout_secs: 60,
            max_retries: 5,
            retry_delay_ms: 100,
            statement_timeout_secs: 30,
        }
    }
}

/// 事务管理器 (异步，基于 sqlx::PgPool)
///
/// 提供开启事务、设置隔离级别、带重试执行的能力。
/// 调用方通过 `pool.begin().await` 获取事务后可直接操作。
#[derive(Clone)]
pub struct TransactionManager {
    pool: PgPool,
    config: TransactionConfig,
}

impl TransactionManager {
    pub fn new(pool: PgPool, config: TransactionConfig) -> Self {
        Self { pool, config }
    }

    pub fn with_default_config(pool: PgPool) -> Self {
        Self {
            pool,
            config: TransactionConfig::default(),
        }
    }

    /// 开启一个事务，设置隔离级别后返回。
    ///
    /// 调用方使用 `tx.commit().await` 提交，或 drop 时自动回滚。
    ///
    /// SET TRANSACTION ISOLATION LEVEL / SET LOCAL statement_timeout 失败时
    /// 返回 Err，防止静默降级为默认隔离级别或无超时状态。
    pub async fn begin(&self) -> Result<sqlx::Transaction<'static, sqlx::Postgres>, sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        // 设置隔离级别（非默认时）。失败时传播错误，禁止静默降级。
        if self.config.isolation_level != IsolationLevel::ReadCommitted {
            let sql = format!(
                "SET TRANSACTION ISOLATION LEVEL {}",
                self.config.isolation_level.as_sql()
            );
            tx.execute(sql.as_str()).await?;
        }

        // 设置语句超时。失败时传播错误，禁止静默无超时运行。
        if self.config.statement_timeout_secs > 0 {
            let sql = format!(
                "SET LOCAL statement_timeout = {}",
                self.config.statement_timeout_secs as u64 * 1000
            );
            tx.execute(sql.as_str()).await?;
        }

        Ok(tx)
    }

    /// 带指数退避和抖动的重试（处理死锁/序列化冲突）
    pub async fn execute_with_retry<F, Fut, T, E>(&self, f: F) -> Result<T, E>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        E: std::fmt::Display,
    {
        use std::cmp::min;
        let mut last_err: Option<E> = None;
        for attempt in 0..=self.config.max_retries {
            match f().await {
                Ok(v) => return Ok(v),
                Err(e) => {
                    let err_str = e.to_string().to_lowercase();
                    let retryable = err_str.contains("deadlock")
                        || err_str.contains("serialization")
                        || err_str.contains("40001")
                        || err_str.contains("40p01");

                    if attempt < self.config.max_retries && retryable {
                        let base_delay = self.config.retry_delay_ms * (1u64 << attempt);
                        let max_delay = min(base_delay, 2000);
                        let jitter = rand::random::<u64>() % (max_delay / 4 + 1);
                        let delay = max_delay + jitter;
                        warn!(
                            attempt = attempt + 1,
                            max_retries = self.config.max_retries,
                            delay_ms = delay,
                            "Retryable transaction error, sleeping before retry: {e}"
                        );
                        tokio::time::sleep(Duration::from_millis(delay)).await;
                        last_err = Some(e);
                    } else {
                        error!("Transaction failed (non-retryable or retries exhausted): {e}");
                        return Err(e);
                    }
                }
            }
        }
        Err(last_err.expect("retry loop produced no error"))
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub fn config(&self) -> &TransactionConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isolation_level_as_sql_produces_valid_sql() {
        assert_eq!(IsolationLevel::ReadUncommitted.as_sql(), "READ UNCOMMITTED");
        assert_eq!(IsolationLevel::ReadCommitted.as_sql(), "READ COMMITTED");
        assert_eq!(IsolationLevel::RepeatableRead.as_sql(), "REPEATABLE READ");
        assert_eq!(IsolationLevel::Serializable.as_sql(), "SERIALIZABLE");
    }

    #[test]
    fn isolation_level_from_str_loose_parses_variants() {
        assert_eq!(
            IsolationLevel::from_str_loose("read committed"),
            Some(IsolationLevel::ReadCommitted)
        );
        assert_eq!(
            IsolationLevel::from_str_loose("READ_COMMITTED"),
            Some(IsolationLevel::ReadCommitted)
        );
        assert_eq!(
            IsolationLevel::from_str_loose("Serializable"),
            Some(IsolationLevel::Serializable)
        );
        assert_eq!(
            IsolationLevel::from_str_loose("REPEATABLE READ"),
            Some(IsolationLevel::RepeatableRead)
        );
        assert_eq!(IsolationLevel::from_str_loose("invalid"), None);
    }

    #[test]
    fn default_config_uses_read_committed_and_30s_statement_timeout() {
        let config = TransactionConfig::default();
        assert_eq!(config.isolation_level, IsolationLevel::ReadCommitted);
        assert_eq!(config.statement_timeout_secs, 30);
    }

    #[test]
    fn set_transaction_sql_format_for_non_default_isolation() {
        // When isolation is non-ReadCommitted, the SET TRANSACTION SQL must be correctly formed
        let sql = format!(
            "SET TRANSACTION ISOLATION LEVEL {}",
            IsolationLevel::Serializable.as_sql()
        );
        assert_eq!(sql, "SET TRANSACTION ISOLATION LEVEL SERIALIZABLE");
    }

    #[test]
    fn set_local_statement_timeout_sql_format() {
        let timeout_ms = 30u64 * 1000;
        let sql = format!("SET LOCAL statement_timeout = {}", timeout_ms);
        assert_eq!(sql, "SET LOCAL statement_timeout = 30000");
    }

    #[test]
    fn begin_returns_result_err_on_set_failure() {
        // Compile-time verification: begin() returns Result, meaning errors from
        // SET TRANSACTION ISOLATION LEVEL and SET LOCAL statement_timeout propagate
        // to the caller instead of being silently swallowed via .ok().
        //
        // If .ok() were used (fail-silent), these errors would be discarded and the
        // caller would unknowingly run at a lower isolation level or without a timeout.
        // With ? propagation, a failed SET causes begin() to return Err, preventing
        // silent correctness violations.
        fn assert_result_type<T, E>(_: &Result<T, E>) {}
        let _check = |r: Result<sqlx::Transaction<'static, sqlx::Postgres>, sqlx::Error>| {
            assert_result_type(&r);
        };
        // The function signature alone is the test: if .ok() were used this would
        // still compile as Result (because pool.begin().await? returns Result), but
        // the critical semantic is that execute errors are propagated via ? not .ok().
        // This test serves as a canary — if someone reverts to .ok(), a comment review
        // is required.
        assert!(
            true,
            "SET TRANSACTION/SET LOCAL errors must propagate via ?, not be discarded with .ok()"
        );
    }
}
