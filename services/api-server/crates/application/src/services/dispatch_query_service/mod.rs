//! 派工查询应用服务（读侧编排）。
//!
//! 子模块按序列化 / 时间线 / 冲突辅助拆分，保持 `DispatchQueryService` 对外 API 不变。

mod helpers;
mod serialization;
mod service;
mod timeline;
mod types;

#[cfg(test)]
mod tests;

pub use serialization::{dispatch_order_to_value, dispatch_order_to_value_with_summary};
pub use service::DispatchQueryService;
