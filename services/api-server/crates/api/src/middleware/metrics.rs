//! HTTP 请求指标中间件
//!
//! 记录 `fms_http_requests_total` (Counter) 与 `fms_http_request_duration_seconds`
//! (Histogram)，标签含 method/path/status。path 使用 Actix 匹配模式做归一化。

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::Error;
use futures_util::future::{ok, Ready};

/// HTTP 指标采集中间件
pub struct MetricsMiddleware;

impl<S, B> Transform<S, ServiceRequest> for MetricsMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = MetricsMiddlewareService<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(MetricsMiddlewareService { service })
    }
}

pub struct MetricsMiddlewareService<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for MetricsMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(&self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let method = req.method().to_string();
        let path = req.match_pattern().unwrap_or_else(|| req.path().to_string());
        let started_at = Instant::now();
        let fut = self.service.call(req);

        Box::pin(async move {
            let result = fut.await;
            let duration = started_at.elapsed().as_secs_f64();
            let status = match &result {
                Ok(response) => response.status().as_u16().to_string(),
                Err(_) => "500".to_string(),
            };

            metrics::counter!(
                "fms_http_requests_total",
                "method" => method.clone(),
                "path" => path.clone(),
                "status" => status
            )
            .increment(1);

            metrics::histogram!(
                "fms_http_request_duration_seconds",
                "method" => method,
                "path" => path
            )
            .record(duration);

            result
        })
    }
}
