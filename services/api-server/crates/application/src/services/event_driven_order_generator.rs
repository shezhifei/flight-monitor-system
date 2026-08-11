//! 事件驱动的工单生成器 Trait 定义
//!
//! 提供基于事件自动创建新工单的能力。

use crate::services::domain_event_subscriber_service::DomainEventEnvelope;
use serde_json::Value;

pub trait EventDrivenOrderGenerator: Send + Sync {
    /// 返回此生成器监听的事件类型列表
    fn event_patterns(&self) -> &[&'static str];

    /// 返回生成器的唯一标识符
    fn id(&self) -> &str;

    /// 返回生成器的显示名称
    fn name(&self) -> &str;

    /// 返回执行优先级
    fn priority(&self) -> i32;

    /// 判断是否应该为给定事件生成工单
    fn should_generate(
        &self,
        event: &DomainEventEnvelope,
    ) -> Result<ShouldGenerateResult, fms_domain::error::DomainError>;

    /// 生成工单
    fn generate_order(
        &self,
        event: &DomainEventEnvelope,
    ) -> Result<Option<fms_domain::models::dispatch::DispatchOrder>, fms_domain::error::DomainError>;

    /// 返回生成器的配置参数（可选）
    fn config(&self) -> Option<&Value> {
        None
    }
}

/// 生成判定结果
#[derive(Debug, Clone)]
pub enum ShouldGenerateResult {
    /// 应该生成工单
    Generate { reason: String, metadata: Option<Value> },
    /// 跳过生成
    Skip { reason: String },
}

impl ShouldGenerateResult {
    pub fn generate(reason: &str) -> Self {
        Self::Generate {
            reason: reason.to_string(),
            metadata: None,
        }
    }

    pub fn generate_with_metadata(reason: &str, metadata: Value) -> Self {
        Self::Generate {
            reason: reason.to_string(),
            metadata: Some(metadata),
        }
    }

    pub fn skip(reason: &str) -> Self {
        Self::Skip {
            reason: reason.to_string(),
        }
    }

    pub fn is_generate(&self) -> bool {
        matches!(self, Self::Generate { .. })
    }
}

pub struct NoOpEventDrivenOrderGenerator;

impl EventDrivenOrderGenerator for NoOpEventDrivenOrderGenerator {
    fn event_patterns(&self) -> &[&'static str] {
        &[]
    }

    fn id(&self) -> &str {
        "no_op_generator"
    }

    fn name(&self) -> &str {
        "No-Op Generator"
    }

    fn priority(&self) -> i32 {
        i32::MAX
    }

    fn should_generate(
        &self,
        _event: &DomainEventEnvelope,
    ) -> Result<ShouldGenerateResult, fms_domain::error::DomainError> {
        Ok(ShouldGenerateResult::skip("No-Op generator"))
    }

    fn generate_order(
        &self,
        _event: &DomainEventEnvelope,
    ) -> Result<Option<fms_domain::models::dispatch::DispatchOrder>, fms_domain::error::DomainError> {
        Ok(None)
    }
}
