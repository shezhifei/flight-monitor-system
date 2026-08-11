//! 航班监控系统 — API 层 (actix-web)
//!
//! HTTP 路由、中间件、请求提取器。

pub mod error;
pub mod middleware;
pub mod request_context;
pub mod routes;
pub mod services;
pub mod sse;
#[cfg(test)]
pub(crate) mod test_support;

pub mod types;
