//! 工单取消器 Trait 定义
//!
//! 提供基于事件自动取消工单的能力。

use crate::services::domain_event_subscriber_service::DomainEventEnvelope;
use serde_json::Value;

pub trait DispatchOrderCanceller: Send + Sync {
    /// 返回此取消器监听的事件类型列表
    fn event_patterns(&self) -> &[&'static str];

    /// 返回取消器的唯一标识符
    fn id(&self) -> &str;

    /// 返回取消器的显示名称
    fn name(&self) -> &str;

    /// 返回执行优先级
    fn priority(&self) -> i32;

    /// 判断是否应该取消工单
    fn should_cancel(
        &self,
        order: &fms_domain::models::dispatch::DispatchOrder,
        event: &DomainEventEnvelope,
    ) -> Result<ShouldCancelResult, fms_domain::error::DomainError>;

    /// 执行取消操作
    fn cancel(
        &self,
        order: &mut fms_domain::models::dispatch::DispatchOrder,
        event: &DomainEventEnvelope,
    ) -> Result<CancelResult, fms_domain::error::DomainError>;

    /// 返回取消器的配置参数（可选）
    fn config(&self) -> Option<&Value> {
        None
    }
}

/// 取消判定结果
#[derive(Debug, Clone)]
pub enum ShouldCancelResult {
    /// 应该取消
    Cancel { reason: String },
    /// 跳过
    Skip { reason: String },
}

impl ShouldCancelResult {
    pub fn cancel(reason: &str) -> Self {
        Self::Cancel {
            reason: reason.to_string(),
        }
    }

    pub fn skip(reason: &str) -> Self {
        Self::Skip {
            reason: reason.to_string(),
        }
    }

    pub fn is_cancel(&self) -> bool {
        matches!(self, Self::Cancel { .. })
    }
}

/// 取消结果
#[derive(Debug, Clone)]
pub struct CancelResult {
    pub cancelled: bool,
    pub reason: String,
}

impl CancelResult {
    pub fn success(reason: &str) -> Self {
        Self {
            cancelled: true,
            reason: reason.to_string(),
        }
    }

    pub fn unchanged() -> Self {
        Self {
            cancelled: false,
            reason: "Not cancelled".to_string(),
        }
    }
}

pub struct NoOpDispatchOrderCanceller;

impl DispatchOrderCanceller for NoOpDispatchOrderCanceller {
    fn event_patterns(&self) -> &[&'static str] {
        &[]
    }

    fn id(&self) -> &str {
        "no_op_canceller"
    }

    fn name(&self) -> &str {
        "No-Op Canceller"
    }

    fn priority(&self) -> i32 {
        i32::MAX
    }

    fn should_cancel(
        &self,
        _order: &fms_domain::models::dispatch::DispatchOrder,
        _event: &DomainEventEnvelope,
    ) -> Result<ShouldCancelResult, fms_domain::error::DomainError> {
        Ok(ShouldCancelResult::skip("No-Op canceller"))
    }

    fn cancel(
        &self,
        _order: &mut fms_domain::models::dispatch::DispatchOrder,
        _event: &DomainEventEnvelope,
    ) -> Result<CancelResult, fms_domain::error::DomainError> {
        Ok(CancelResult::unchanged())
    }
}
