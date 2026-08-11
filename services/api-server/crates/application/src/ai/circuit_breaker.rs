//! 熔断器 — 保护 Rust AI 代理层免受 Python Sidecar 故障的级联影响。
//!
//! 三态模型: Closed → Open → HalfOpen → Closed
//! - Closed: 正常转发请求
//! - Open: 连续失败达到阈值后熔断，直接返回降级响应
//! - HalfOpen: 熔断恢复期后放行一个请求探测

use serde::Serialize;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub reset_timeout_secs: u64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            reset_timeout_secs: 60,
        }
    }
}

#[derive(Debug)]
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    consecutive_failures: AtomicU32,
    last_failure_time: AtomicU64,
    total_requests: AtomicU64,
    total_failures: AtomicU64,
    state: RwLock<CircuitState>,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            consecutive_failures: AtomicU32::new(0),
            last_failure_time: AtomicU64::new(0),
            total_requests: AtomicU64::new(0),
            total_failures: AtomicU64::new(0),
            state: RwLock::new(CircuitState::Closed),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(CircuitBreakerConfig::default())
    }

    pub async fn is_available(&self) -> bool {
        let state = self.state.read().await;
        match *state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                let last_failure = self.last_failure_time.load(Ordering::Relaxed);
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                if now.saturating_sub(last_failure) >= self.config.reset_timeout_secs {
                    drop(state);
                    // Re-acquire write lock and re-check state (TOCTOU guard)
                    let mut state = self.state.write().await;
                    if matches!(*state, CircuitState::Open) {
                        *state = CircuitState::HalfOpen;
                    }
                    true
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => true,
        }
    }

    pub async fn record_success(&self) {
        self.consecutive_failures.store(0, Ordering::Relaxed);
        let mut state = self.state.write().await;
        *state = CircuitState::Closed;
    }

    pub async fn record_failure(&self) {
        let failures = self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;
        self.total_failures.fetch_add(1, Ordering::Relaxed);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.last_failure_time.store(now, Ordering::Relaxed);

        if failures >= self.config.failure_threshold {
            let mut state = self.state.write().await;
            *state = CircuitState::Open;
        }
    }

    pub fn increment_requests(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
    }

    pub async fn current_state(&self) -> CircuitState {
        self.state.read().await.clone()
    }

    pub fn total_requests(&self) -> u64 {
        self.total_requests.load(Ordering::Relaxed)
    }

    pub fn total_failures(&self) -> u64 {
        self.total_failures.load(Ordering::Relaxed)
    }

    pub fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Serialize)]
pub struct CircuitBreakerStats {
    pub state: CircuitState,
    pub total_requests: u64,
    pub total_failures: u64,
    pub consecutive_failures: u32,
    pub failure_threshold: u32,
    pub reset_timeout_secs: u64,
}

impl CircuitBreaker {
    pub async fn stats(&self) -> CircuitBreakerStats {
        CircuitBreakerStats {
            state: self.current_state().await,
            total_requests: self.total_requests(),
            total_failures: self.total_failures(),
            consecutive_failures: self.consecutive_failures(),
            failure_threshold: self.config.failure_threshold,
            reset_timeout_secs: self.config.reset_timeout_secs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state_is_closed() {
        let cb = CircuitBreaker::with_defaults();
        let rt = tokio::runtime::Runtime::new().unwrap();
        assert_eq!(rt.block_on(cb.current_state()), CircuitState::Closed);
        assert!(rt.block_on(cb.is_available()));
    }

    #[test]
    fn test_opens_after_threshold() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 3,
            reset_timeout_secs: 60,
        });
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(cb.record_failure());
        rt.block_on(cb.record_failure());
        assert_eq!(rt.block_on(cb.current_state()), CircuitState::Closed);
        rt.block_on(cb.record_failure());
        assert_eq!(rt.block_on(cb.current_state()), CircuitState::Open);
        assert!(!rt.block_on(cb.is_available()));
    }

    #[test]
    fn test_success_resets_to_closed() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 2,
            reset_timeout_secs: 60,
        });
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(cb.record_failure());
        rt.block_on(cb.record_failure());
        assert_eq!(rt.block_on(cb.current_state()), CircuitState::Open);
        rt.block_on(cb.record_success());
        assert_eq!(rt.block_on(cb.current_state()), CircuitState::Closed);
        assert_eq!(cb.consecutive_failures(), 0);
    }

    #[test]
    fn test_stats() {
        let cb = CircuitBreaker::with_defaults();
        cb.increment_requests();
        cb.increment_requests();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(cb.record_failure());
        let stats = rt.block_on(cb.stats());
        assert_eq!(stats.total_requests, 2);
        assert_eq!(stats.total_failures, 1);
        assert_eq!(stats.consecutive_failures, 1);
    }
}
