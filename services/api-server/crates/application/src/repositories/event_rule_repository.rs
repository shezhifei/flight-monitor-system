//! 事件驱动的派工规则 Repository
//!
//! 提供调整规则和生成规则的持久化接口。
//! 注意：具体类型已移至 fms-domain 以避免循环依赖；此处仅作 re-export。

pub use fms_domain::ports::event_rule_repository::{
    AdjustmentRuleRecord, EventRuleRepository, GenerationRuleRecord, ListAdjustmentRulesParams,
    ListGenerationRulesParams,
};
