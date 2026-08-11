//! 航班监控系统 — 领域层
//!
//! 包含所有业务实体、值对象、领域事件和仓储 trait 定义。
//! 此 crate 不包含任何 I/O 操作，仅定义纯业务逻辑。

pub mod ai_runtime_event;
pub mod broadcaster;
pub mod canonical_args;
pub mod error;
pub mod events;
pub mod models;
pub mod ontology;
pub mod pgoutput_decoder;
pub mod ports;
pub mod validation;
