use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use fms_domain::error::DomainError;
use fms_domain::models::dispatch_collaboration::DispatchCollaborationEvent;
use fms_domain::ports::dispatch_collaboration_repository::DispatchCollaborationRepository;

use super::traits::NotificationCollaborationEvents;

/// 把 33 方法的 `DispatchCollaborationRepository` 适配到 1 方法的窄端口。
///
/// 通知服务只需要往协作流里追加一条事件，`create_event` 的返回值它也不看。
pub struct CollaborationEventRecorder {
    repo: Arc<dyn DispatchCollaborationRepository + Send + Sync>,
}

impl CollaborationEventRecorder {
    pub fn new(repo: Arc<dyn DispatchCollaborationRepository + Send + Sync>) -> Self {
        Self { repo }
    }
}

impl NotificationCollaborationEvents for CollaborationEventRecorder {
    fn create_event<'a>(
        &'a self,
        event: &'a DispatchCollaborationEvent,
    ) -> Pin<Box<dyn Future<Output = Result<(), DomainError>> + Send + 'a>> {
        Box::pin(async move { self.repo.create_event(event).await.map(|_| ()) })
    }
}

/// 明确表示「此处不记录协作事件」。测试与不需要协作流的装配用它。
///
/// 它与 `CollaborationEventRecorder` 的区别在类型上可见——以前两者都是「字段是
/// `None`」，忘记接线和故意不接无法区分。
pub struct NoCollaborationEvents;

impl NotificationCollaborationEvents for NoCollaborationEvents {
    fn create_event<'a>(
        &'a self,
        _event: &'a DispatchCollaborationEvent,
    ) -> Pin<Box<dyn Future<Output = Result<(), DomainError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
}
