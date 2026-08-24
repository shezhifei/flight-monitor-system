//! `BusinessCaseMentionAudience` 的生产实现：适配派工协作仓储。

use std::collections::HashSet;
use std::sync::Arc;

use fms_domain::ports::dispatch_collaboration_repository::DispatchCollaborationRepository;

use super::schemas::BusinessCaseMentionAudience;

/// 用航班协作群的**活跃**成员作为可提及范围。
///
/// 注意这里是 `find_active_members`，与群聊消息路径的 `find_group_members`
/// （含只读/已停用成员）刻意不同：业务事项的候选人接口
/// `/flights/{id}/stakeholders` 也只返回活跃成员，两端保持一致。
/// 这条策略以前散在 `BusinessCaseService` 里重复两遍，现在只有这一处。
pub struct CollaborationMentionAudience {
    repo: Arc<dyn DispatchCollaborationRepository + Send + Sync>,
}

impl CollaborationMentionAudience {
    pub fn new(repo: Arc<dyn DispatchCollaborationRepository + Send + Sync>) -> Self {
        Self { repo }
    }
}

#[async_trait::async_trait]
impl BusinessCaseMentionAudience for CollaborationMentionAudience {
    async fn mentionable_user_ids(&self, flight_id: &str) -> Vec<String> {
        // @提及是追加内容的附属信息，取不到群或成员时不该让追加失败——
        // 与收窄前的行为一致：安静地退化成「没有人可被提及」。
        let Ok(Some(group)) = self.repo.get_group_by_flight(flight_id).await else {
            return Vec::new();
        };
        let Ok(members) = self.repo.find_active_members(&group.group_id).await else {
            return Vec::new();
        };
        members
            .into_iter()
            .map(|m| m.user_id.trim().to_string())
            .collect::<HashSet<String>>()
            .into_iter()
            .collect()
    }
}

/// 不允许任何 @提及。给不关心提及行为的测试用，
/// 语义上等同于收窄前「依赖没接线」的那条分支。
pub struct NoMentionAudience;

#[async_trait::async_trait]
impl BusinessCaseMentionAudience for NoMentionAudience {
    async fn mentionable_user_ids(&self, _flight_id: &str) -> Vec<String> {
        Vec::new()
    }
}
