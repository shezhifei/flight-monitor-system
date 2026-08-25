//! 本体 V1 应用服务（ONTOLOGY_V1.md §3–§7）
//!
//! 编排飞机中心资源对象与 ReassignAircraft / 建议 / draft 确认 / 双视图。

mod error;
mod service;
mod writer;

#[cfg(test)]
mod tests;

pub use error::OntologyError;
pub use service::OntologyService;
pub use writer::{OntologyTransactions, OntologyWriter};
