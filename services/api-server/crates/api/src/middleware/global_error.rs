//! 全局错误处理中间件
//!
//! 对应 Python 的 `GlobalErrorHandlerMiddleware`，捕获所有未处理的异常并返回统一错误格式。

use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::web;
use actix_web::Error;
use futures_util::future::{ok, Ready};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Instant;

use crate::error::record_api_error_with_context;
use crate::error::ApiError;
use crate::services::performance_metrics::PerformanceMetricsService;
use crate::services::runtime_error_monitor::{
    scope_request_error_recording, suppress_next_api_error_recording, RuntimeErrorInput, RuntimeErrorMonitor,
};

/// 全局错误处理中间件
pub struct GlobalErrorMiddleware;

impl<S, B> Transform<S, ServiceRequest> for GlobalErrorMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = GlobalErrorMiddlewareMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(GlobalErrorMiddlewareMiddleware { service })
    }
}

pub struct GlobalErrorMiddlewareMiddleware<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for GlobalErrorMiddlewareMiddleware<S>
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
        let path = req.path().to_string();
        let error_monitor = req
            .app_data::<web::Data<Arc<RuntimeErrorMonitor>>>()
            .map(|value| value.get_ref().clone());
        let performance_metrics = req
            .app_data::<web::Data<Arc<PerformanceMetricsService>>>()
            .map(|value| value.get_ref().clone());
        let fut = self.service.call(req);
        let operation = format!("{method} {path}");
        let started_at = Instant::now();

        Box::pin(async move {
            scope_request_error_recording(Some(operation.clone()), async move {
                if let Some(monitor) = &error_monitor {
                    monitor.record_request().await;
                }
                let result = match fut.await {
                    Ok(res) => Ok(res),
                    Err(err) => {
                        // 尝试将错误转换为 ApiError
                        if let Some(api_error) = err.as_error::<ApiError>() {
                            record_api_error_with_context(api_error);
                            suppress_next_api_error_recording();
                            tracing::error!(
                                error = %api_error,
                                "Unhandled API error caught by global middleware"
                            );
                            // 让 actix-web 的 ResponseError 处理
                            Err(err)
                        } else {
                            // 非 ApiError 的其他错误（如 panic、内部错误）
                            if let Some(monitor) = &error_monitor {
                                monitor
                                    .record_error(RuntimeErrorInput {
                                        error_type:
                                            crate::services::runtime_error_types::RuntimeErrorKind::UnhandledActixError,
                                        message: err.to_string(),
                                        severity: crate::services::runtime_error_types::Severity::Critical,
                                        category: crate::services::runtime_error_types::ErrorCategory::System,
                                        operation: Some(operation.clone()),
                                        details: None,
                                    })
                                    .await;
                            }
                            suppress_next_api_error_recording();
                            tracing::error!(
                                error = %err,
                                "Unhandled non-API error caught by global middleware"
                            );
                            // 转换为 500 内部错误
                            Err(ApiError::Internal("服务器内部错误".to_string()).into())
                        }
                    }
                };

                if let Some(metrics) = &performance_metrics {
                    metrics.record_request_latency(started_at.elapsed().as_secs_f64() * 1000.0);
                }

                result
            })
            .await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, web, App, HttpResponse, ResponseError};
    use std::sync::Arc;
    use tokio::time::{sleep, Duration};

    use crate::services::runtime_error_monitor::set_global_runtime_error_monitor;

    #[actix_web::test]
    async fn middleware_passes_successful_requests() {
        let middleware = GlobalErrorMiddleware;
        let app = test::init_service(
            App::new()
                .wrap(middleware)
                .route("/test", web::get().to(|| async { HttpResponse::Ok().finish() })),
        )
        .await;

        let req = test::TestRequest::get().uri("/test").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn middleware_records_internal_errors_in_runtime_monitor() {
        let monitor = RuntimeErrorMonitor::new(None);
        set_global_runtime_error_monitor(&monitor);
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(Arc::clone(&monitor)))
                .wrap(GlobalErrorMiddleware)
                .route(
                    "/boom",
                    web::get()
                        .to(|| async { Err::<HttpResponse, _>(ApiError::Internal("scheduler exploded".to_string())) }),
                ),
        )
        .await;

        let req = test::TestRequest::get().uri("/boom").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), ApiError::Internal("x".to_string()).status_code());
        sleep(Duration::from_millis(10)).await;

        let errors = monitor.recent_errors(10).await;
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0]["error_type"], "ApiInternalError");
        assert_eq!(errors[0]["operation"], "GET /boom");
    }
}
