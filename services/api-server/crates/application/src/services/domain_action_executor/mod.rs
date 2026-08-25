//! 域动作执行器：将审批后的域动作应用到事务中并写出 outbox 事件。
//!
//! 子模块按类型 / 服务 / 辅助函数拆分，保持对外 API 不变。

mod helpers;
mod service;
mod types;

#[cfg(test)]
mod tests;

pub use service::{DomainActionExecution, DomainActionExecutor};
pub use types::{DomainActionError, DomainActionReceipt};
