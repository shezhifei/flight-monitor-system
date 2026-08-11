//! 工单内容调整器 Trait 定义
//!
//! 提供基于事件的工单内容动态调整能力。

use crate::services::domain_event_subscriber_service::DomainEventEnvelope;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

pub trait DispatchOrderAdjuster: Send + Sync {
    /// 返回此调整器监听的事件类型列表
    fn event_patterns(&self) -> &[&'static str];

    /// 返回调整器的唯一标识符
    fn id(&self) -> &str;

    /// 返回调整器的显示名称
    fn name(&self) -> &str;

    /// 返回执行优先级，数值越小越先执行
    fn priority(&self) -> i32;

    /// 判断此调整器是否应该处理给定的订单和事件
    fn should_apply(&self, order: &fms_domain::models::dispatch::DispatchOrder, event: &DomainEventEnvelope) -> bool;

    /// 调整工单内容
    fn adjust(
        &self,
        order: &mut fms_domain::models::dispatch::DispatchOrder,
        event: &DomainEventEnvelope,
    ) -> Result<AdjustmentResult, fms_domain::error::DomainError>;

    /// 返回调整器的配置参数（可选）
    fn config(&self) -> Option<&Value> {
        None
    }
}

/// 调整结果
#[derive(Debug, Clone)]
pub struct AdjustmentResult {
    pub modified: bool,
    pub reason: String,
    pub modified_fields: Vec<String>,
}

impl AdjustmentResult {
    pub fn unchanged(reason: &str) -> Self {
        Self {
            modified: false,
            reason: reason.to_string(),
            modified_fields: Vec::new(),
        }
    }

    pub fn modified(reason: &str, fields: Vec<&str>) -> Self {
        Self {
            modified: true,
            reason: reason.to_string(),
            modified_fields: fields.into_iter().map(String::from).collect(),
        }
    }
}

pub type AdjusterFuture =
    Pin<Box<dyn Future<Output = Result<AdjustmentResult, fms_domain::error::DomainError>> + Send>>;

pub trait AsyncDispatchOrderAdjuster: Send + Sync {
    fn event_patterns(&self) -> &[&'static str];
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn priority(&self) -> i32;
    fn should_apply(&self, order: &fms_domain::models::dispatch::DispatchOrder, event: &DomainEventEnvelope) -> bool;
    fn adjust<'a>(
        &'a self,
        order: &'a mut fms_domain::models::dispatch::DispatchOrder,
        event: &'a DomainEventEnvelope,
    ) -> AdjusterFuture;
    fn config(&self) -> Option<&Value> {
        None
    }
}

pub struct NoOpDispatchOrderAdjuster;

impl DispatchOrderAdjuster for NoOpDispatchOrderAdjuster {
    fn event_patterns(&self) -> &[&'static str] {
        &[]
    }

    fn id(&self) -> &str {
        "no_op"
    }

    fn name(&self) -> &str {
        "No-Op Adjuster"
    }

    fn priority(&self) -> i32 {
        i32::MAX
    }

    fn should_apply(&self, _order: &fms_domain::models::dispatch::DispatchOrder, _event: &DomainEventEnvelope) -> bool {
        false
    }

    fn adjust(
        &self,
        _order: &mut fms_domain::models::dispatch::DispatchOrder,
        _event: &DomainEventEnvelope,
    ) -> Result<AdjustmentResult, fms_domain::error::DomainError> {
        Ok(AdjustmentResult::unchanged("No-Op adjuster"))
    }
}
