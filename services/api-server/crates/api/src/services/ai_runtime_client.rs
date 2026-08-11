use actix_web::{HttpRequest, HttpResponse};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::services::python_sidecar_proxy::{
    ai_sidecar_sse_connect_timeout, ai_sidecar_timeout, ai_sidecar_url, degraded_response, forward_json_request,
    forward_request, forward_sse_json_request, forward_sse_json_request_raw, forward_sse_request,
    probe_ai_sidecar_health, PythonSidecarHealth, SidecarAuth,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

struct CircuitBreakerInner {
    consecutive_failures: AtomicU32,
    last_failure_epoch: AtomicU64,
    total_requests: AtomicU64,
    total_failures: AtomicU64,
    threshold: u32,
    reset_secs: u64,
    state: RwLock<CircuitState>,
}

impl CircuitBreakerInner {
    fn new(threshold: u32, reset_secs: u64) -> Self {
        Self {
            consecutive_failures: AtomicU32::new(0),
            last_failure_epoch: AtomicU64::new(0),
            total_requests: AtomicU64::new(0),
            total_failures: AtomicU64::new(0),
            threshold,
            reset_secs,
            state: RwLock::new(CircuitState::Closed),
        }
    }

    async fn is_available(&self) -> bool {
        let state = self.state.read().await;
        match *state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                let last = self.last_failure_epoch.load(Ordering::Relaxed);
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                if now.saturating_sub(last) >= self.reset_secs {
                    drop(state);
                    *self.state.write().await = CircuitState::HalfOpen;
                    true
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => true,
        }
    }

    async fn record_success(&self) {
        self.consecutive_failures.store(0, Ordering::Relaxed);
        *self.state.write().await = CircuitState::Closed;
    }

    async fn record_failure(&self) {
        let prev = self.consecutive_failures.fetch_add(1, Ordering::Relaxed);
        self.total_failures.fetch_add(1, Ordering::Relaxed);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.last_failure_epoch.store(now, Ordering::Relaxed);
        if prev + 1 >= self.threshold {
            *self.state.write().await = CircuitState::Open;
        }
    }
}

pub struct AiRuntimeClient {
    breaker: Arc<CircuitBreakerInner>,
    base_url: String,
}

impl AiRuntimeClient {
    pub fn new() -> Self {
        Self {
            breaker: Arc::new(CircuitBreakerInner::new(5, 60)),
            base_url: ai_sidecar_url(),
        }
    }

    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
            breaker: Arc::new(CircuitBreakerInner::new(5, 60)),
            base_url: base_url.into(),
        }
    }

    pub fn new_with_config(threshold: u32, reset_secs: u64) -> Self {
        Self {
            breaker: Arc::new(CircuitBreakerInner::new(threshold, reset_secs)),
            base_url: ai_sidecar_url(),
        }
    }

    fn auth_for_path(path: &str) -> SidecarAuth {
        SidecarAuth::ServiceIdentity {
            audience: "python-ai-runtime",
            path: path.to_string(),
        }
    }

    pub async fn start_run(&self, req: &HttpRequest, envelope: &Value) -> HttpResponse {
        self.breaker.total_requests.fetch_add(1, Ordering::Relaxed);
        if !self.breaker.is_available().await {
            return degraded_response("AI Runtime 不可用：熔断器开启");
        }
        let url = format!("{}/internal/ai/v1/runs", self.base_url);
        let resp = forward_json_request(
            req,
            reqwest::Method::POST,
            &url,
            envelope,
            Self::auth_for_path("/internal/ai/v1/runs"),
            ai_sidecar_timeout(),
        )
        .await;
        if resp.status().is_success() {
            self.breaker.record_success().await;
        } else {
            self.breaker.record_failure().await;
        }
        resp
    }

    pub async fn stream_run(&self, req: &HttpRequest, envelope: &Value) -> HttpResponse {
        self.breaker.total_requests.fetch_add(1, Ordering::Relaxed);
        if !self.breaker.is_available().await {
            return degraded_response("AI Runtime 不可用：熔断器开启");
        }
        let url = format!("{}/internal/ai/v1/runs/stream", self.base_url);
        let resp = forward_sse_json_request(
            req,
            reqwest::Method::POST,
            &url,
            envelope,
            Self::auth_for_path("/internal/ai/v1/runs/stream"),
            ai_sidecar_sse_connect_timeout(),
        )
        .await;
        if resp.status().is_success() {
            self.breaker.record_success().await;
        } else {
            self.breaker.record_failure().await;
        }
        resp
    }

    pub async fn stream_run_raw(&self, req: &HttpRequest, envelope: &Value) -> Result<reqwest::Response, HttpResponse> {
        self.breaker.total_requests.fetch_add(1, Ordering::Relaxed);
        if !self.breaker.is_available().await {
            return Err(degraded_response("AI Runtime 不可用：熔断器开启"));
        }
        let url = format!("{}/internal/ai/v1/runs/stream", self.base_url);
        let resp_res = forward_sse_json_request_raw(
            req,
            reqwest::Method::POST,
            &url,
            envelope,
            Self::auth_for_path("/internal/ai/v1/runs/stream"),
            ai_sidecar_sse_connect_timeout(),
        )
        .await;

        match resp_res {
            Ok(resp) => {
                self.breaker.record_success().await;
                Ok(resp)
            }
            Err(e) => {
                self.breaker.record_failure().await;
                Err(e)
            }
        }
    }

    pub async fn stream_run_with_tools_raw(
        &self,
        req: &HttpRequest,
        envelope: &Value,
    ) -> Result<reqwest::Response, HttpResponse> {
        self.breaker.total_requests.fetch_add(1, Ordering::Relaxed);
        if !self.breaker.is_available().await {
            return Err(degraded_response("AI Runtime 不可用：熔断器开启"));
        }
        let url = format!("{}/internal/ai/v1/runs/stream-with-tools", self.base_url);
        let resp_res = forward_sse_json_request_raw(
            req,
            reqwest::Method::POST,
            &url,
            envelope,
            Self::auth_for_path("/internal/ai/v1/runs/stream-with-tools"),
            ai_sidecar_sse_connect_timeout(),
        )
        .await;

        match resp_res {
            Ok(resp) => {
                self.breaker.record_success().await;
                Ok(resp)
            }
            Err(e) => {
                self.breaker.record_failure().await;
                Err(e)
            }
        }
    }

    pub async fn cancel_run(&self, req: &HttpRequest, run_id: &str) -> HttpResponse {
        self.breaker.total_requests.fetch_add(1, Ordering::Relaxed);
        if !self.breaker.is_available().await {
            return degraded_response("AI Runtime 不可用：熔断器开启");
        }
        let path = format!("/internal/ai/v1/runs/{}/cancel", run_id);
        let url = format!("{}{}", self.base_url, path);
        let resp = forward_json_request(
            req,
            reqwest::Method::POST,
            &url,
            &json!({}),
            Self::auth_for_path(&path),
            ai_sidecar_timeout(),
        )
        .await;
        if resp.status().is_success() {
            self.breaker.record_success().await;
        } else {
            self.breaker.record_failure().await;
        }
        resp
    }

    pub async fn forward_json(
        &self,
        req: &HttpRequest,
        method: reqwest::Method,
        internal_path: &str,
        body: &Value,
    ) -> HttpResponse {
        self.breaker.total_requests.fetch_add(1, Ordering::Relaxed);
        if !self.breaker.is_available().await {
            return degraded_response("AI Runtime 不可用：熔断器开启");
        }
        let url = format!("{}{}", self.base_url, internal_path);
        let resp = forward_json_request(
            req,
            method,
            &url,
            body,
            Self::auth_for_path(internal_path),
            ai_sidecar_timeout(),
        )
        .await;
        if resp.status().is_success() {
            self.breaker.record_success().await;
        } else {
            self.breaker.record_failure().await;
        }
        resp
    }

    pub async fn forward_request(
        &self,
        req: &HttpRequest,
        method: reqwest::Method,
        internal_path: &str,
    ) -> HttpResponse {
        self.breaker.total_requests.fetch_add(1, Ordering::Relaxed);
        if !self.breaker.is_available().await {
            return degraded_response("AI Runtime 不可用：熔断器开启");
        }
        let url = format!("{}{}", self.base_url, internal_path);
        let resp = forward_request(
            req,
            method,
            &url,
            Self::auth_for_path(internal_path),
            ai_sidecar_timeout(),
        )
        .await;
        if resp.status().is_success() {
            self.breaker.record_success().await;
        } else {
            self.breaker.record_failure().await;
        }
        resp
    }

    pub async fn forward_sse(&self, req: &HttpRequest, internal_path: &str) -> HttpResponse {
        self.breaker.total_requests.fetch_add(1, Ordering::Relaxed);
        if !self.breaker.is_available().await {
            return degraded_response("AI Runtime 不可用：熔断器开启");
        }
        let url = format!("{}{}", self.base_url, internal_path);
        let resp = forward_sse_request(
            req,
            &url,
            Self::auth_for_path(internal_path),
            ai_sidecar_sse_connect_timeout(),
        )
        .await;
        if resp.status().is_success() {
            self.breaker.record_success().await;
        } else {
            self.breaker.record_failure().await;
        }
        resp
    }

    pub async fn health(&self) -> PythonSidecarHealth {
        probe_ai_sidecar_health().await
    }

    pub async fn circuit_state(&self) -> CircuitState {
        self.breaker.state.read().await.clone()
    }

    pub fn metrics(&self) -> Value {
        json!({
            "total_requests": self.breaker.total_requests.load(Ordering::Relaxed),
            "total_failures": self.breaker.total_failures.load(Ordering::Relaxed),
            "consecutive_failures": self.breaker.consecutive_failures.load(Ordering::Relaxed),
            "circuit_threshold": self.breaker.threshold,
            "circuit_reset_secs": self.breaker.reset_secs,
        })
    }
}
