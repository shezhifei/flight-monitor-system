#![recursion_limit = "256"]
//! 航班监控系统 — 应用服务层
//!
//! 编排领域逻辑、事务、DTO 转换等应用级关注点。

pub mod ai;
pub mod http_client;
pub mod repositories;
pub mod schemas;
pub mod services;
pub mod sqlx_transactional_repositories;
pub mod types;

#[cfg(test)]
mod split_assert;
