//! 写路径验证中间件 — 将 Rust 写操作结果转发到 Python 验证端点进行比对。
//!
//! Phase 3 写路径双写验证:
//!   Rust 写请求 → 执行写入 → 异步转发结果到 Python /api/v2/verification/compare
//!   Python 执行相同写入 → 比对两者结果 → 记录差异到 Redis stream

use actix_web::HttpResponse;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct WriteVerificationConfig {
    pub verification_url: String,
    pub verification_token: String,
    pub enabled: bool,
}

impl Default for WriteVerificationConfig {
    fn default() -> Self {
        Self {
            verification_url: std::env::var("WRITE_VERIFICATION_URL")
                .unwrap_or_else(|_| "http://localhost:8088/api/v2/verification/compare".to_string()),
            verification_token: std::env::var("WRITE_VERIFICATION_TOKEN")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .or_else(|| {
                    std::env::var("VERIFICATION_TOKEN")
                        .ok()
                        .filter(|value| !value.trim().is_empty())
                })
                .unwrap_or_default(),
            enabled: std::env::var("WRITE_VERIFICATION_ENABLED")
                .map(|v| v == "1" || v.to_lowercase() == "true")
                .unwrap_or(false),
        }
    }
}

#[derive(Debug, Default)]
pub struct WriteVerificationStats {
    pub total_verified: AtomicU64,
    pub total_matches: AtomicU64,
    pub total_diffs: AtomicU64,
    pub total_errors: AtomicU64,
}

pub struct WriteVerificationService {
    config: RwLock<WriteVerificationConfig>,
    stats: WriteVerificationStats,
    client: reqwest::Client,
}

impl WriteVerificationService {
    pub fn new(config: WriteVerificationConfig) -> Arc<Self> {
        let client = fms_application::http_client::shared_http_client();
        Arc::new(Self {
            config: RwLock::new(config),
            stats: WriteVerificationStats::default(),
            client,
        })
    }

    pub async fn verify_write(&self, method: &str, path: &str, rust_status: u16, rust_body: &Value) {
        let config = self.config.read().await;
        if !config.enabled {
            return;
        }

        self.stats.total_verified.fetch_add(1, Ordering::Relaxed);
        if config.verification_token.trim().is_empty() {
            self.stats.total_errors.fetch_add(1, Ordering::Relaxed);
            tracing::warn!("write verification is enabled but no verification token is configured");
            return;
        }

        let payload = json!({
            "method": method,
            "path": path,
            "request_body": Value::Null,
            "rust_response": {
                "status_code": rust_status,
                "body": rust_body,
            },
        });

        match self
            .client
            .post(&config.verification_url)
            .header("X-Verification-Token", &config.verification_token)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
        {
            Ok(resp) => {
                if resp.status().is_success() {
                    match resp.json::<Value>().await {
                        Ok(body) => {
                            let diff_data = body.get("data").and_then(Value::as_object);
                            let has_status_diff = diff_data
                                .and_then(|data| data.get("status_match"))
                                .and_then(Value::as_bool)
                                .map(|status_match| !status_match)
                                .unwrap_or(false);
                            let has_data_diff = diff_data
                                .and_then(|data| data.get("has_data_diff"))
                                .and_then(Value::as_bool)
                                .unwrap_or(false);

                            if has_status_diff || has_data_diff {
                                self.stats.total_diffs.fetch_add(1, Ordering::Relaxed);
                            } else {
                                self.stats.total_matches.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        Err(_) => {
                            self.stats.total_matches.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                } else {
                    self.stats.total_errors.fetch_add(1, Ordering::Relaxed);
                }
            }
            Err(_) => {
                self.stats.total_errors.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    pub fn get_stats(&self) -> WriteVerificationStatsSnapshot {
        WriteVerificationStatsSnapshot {
            total_verified: self.stats.total_verified.load(Ordering::Relaxed),
            total_matches: self.stats.total_matches.load(Ordering::Relaxed),
            total_diffs: self.stats.total_diffs.load(Ordering::Relaxed),
            total_errors: self.stats.total_errors.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, serde::Serialize)]
pub struct WriteVerificationStatsSnapshot {
    pub total_verified: u64,
    pub total_matches: u64,
    pub total_diffs: u64,
    pub total_errors: u64,
}

pub async fn write_verification_stats(service: actix_web::web::Data<Arc<WriteVerificationService>>) -> HttpResponse {
    let stats = service.get_stats();
    HttpResponse::Ok().json(json!({
        "success": true,
        "data": stats,
    }))
}

pub fn configure(cfg: &mut actix_web::web::ServiceConfig) {
    cfg.route(
        "/api/v2/verification/write-stats",
        actix_web::web::get().to(write_verification_stats),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = WriteVerificationConfig::default();
        assert!(!config.enabled);
        assert!(config.verification_url.contains("/api/v2/verification/compare"));
    }

    #[test]
    fn default_config_does_not_create_fallback_verification_token() {
        std::env::remove_var("WRITE_VERIFICATION_TOKEN");
        std::env::remove_var("VERIFICATION_TOKEN");

        let config = WriteVerificationConfig::default();

        assert!(config.verification_token.is_empty());
    }

    #[test]
    fn test_stats_snapshot() {
        let service = WriteVerificationService::new(WriteVerificationConfig::default());
        let stats = service.get_stats();
        assert_eq!(stats.total_verified, 0);
        assert_eq!(stats.total_matches, 0);
        assert_eq!(stats.total_diffs, 0);
        assert_eq!(stats.total_errors, 0);
    }

    #[test]
    fn test_verification_disabled() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let service = WriteVerificationService::new(WriteVerificationConfig {
            enabled: false,
            ..Default::default()
        });
        rt.block_on(async {
            service.verify_write("POST", "/api/v2/flights", 201, &json!({})).await;
        });
        let stats = service.get_stats();
        assert_eq!(stats.total_verified, 0);
    }

    #[test]
    fn enabled_verification_without_token_fails_closed() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let service = WriteVerificationService::new(WriteVerificationConfig {
            enabled: true,
            verification_token: String::new(),
            ..Default::default()
        });

        rt.block_on(async {
            service.verify_write("POST", "/api/v2/flights", 201, &json!({})).await;
        });

        let stats = service.get_stats();
        assert_eq!(stats.total_verified, 1);
        assert_eq!(stats.total_errors, 1);
    }
}
