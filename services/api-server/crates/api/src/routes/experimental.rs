//! 实验性路由占位。
//!
//! 原先迁移中的 `nl-query` 与 `llm-eval` 已拆分为独立模块，这里暂不暴露实验接口。

use actix_web::web;

pub fn configure(cfg: &mut web::ServiceConfig) {
    let _ = cfg;
}
