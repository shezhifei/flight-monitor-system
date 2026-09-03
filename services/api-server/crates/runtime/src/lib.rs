//! 航班监控系统 — 跨层运行时原语
//!
//! 此 crate 提供被 `domain` / `application` / `infrastructure` / `api`
//! 共同依赖的底层运行时工具，避免 application 反向依赖 infrastructure。

pub mod environment;
pub mod spawn_tracked;
