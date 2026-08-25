use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use fms_domain::error::DomainError;
use fms_domain::models::business_case::{BusinessCaseAppendEntry, FlightBusinessCase};

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct BusinessCaseStatusMetadata {
    pub value: &'static str,
    pub label: &'static str,
    pub color: &'static str,
    pub category: &'static str,
    pub is_terminal: bool,
    pub manual_transition_enabled: bool,
    pub workflow_target_enabled: bool,
    pub default_for_actions: &'static [&'static str],
}

pub const BUSINESS_CASE_ALLOWED_STATUSES: [&str; 6] =
    ["INITIAL", "PENDING", "PROCESSING", "SUCCESS", "COMPLETED", "FAILED"];

pub const BUSINESS_CASE_STATUS_METADATA: [BusinessCaseStatusMetadata; 6] = [
    BusinessCaseStatusMetadata {
        value: "INITIAL",
        label: "初始",
        color: "#8E8E93",
        category: "active",
        is_terminal: false,
        manual_transition_enabled: true,
        workflow_target_enabled: true,
        default_for_actions: &[],
    },
    BusinessCaseStatusMetadata {
        value: "PENDING",
        label: "待处理",
        color: "#FF9500",
        category: "active",
        is_terminal: false,
        manual_transition_enabled: true,
        workflow_target_enabled: true,
        default_for_actions: &[],
    },
    BusinessCaseStatusMetadata {
        value: "PROCESSING",
        label: "处理中",
        color: "#5856D6",
        category: "active",
        is_terminal: false,
        manual_transition_enabled: true,
        workflow_target_enabled: true,
        default_for_actions: &[],
    },
    BusinessCaseStatusMetadata {
        value: "SUCCESS",
        label: "成功",
        color: "#34C759",
        category: "terminal",
        is_terminal: true,
        manual_transition_enabled: true,
        workflow_target_enabled: true,
        default_for_actions: &[],
    },
    BusinessCaseStatusMetadata {
        value: "COMPLETED",
        label: "已完成",
        color: "#34C759",
        category: "terminal",
        is_terminal: true,
        manual_transition_enabled: true,
        workflow_target_enabled: true,
        default_for_actions: &["complete_case"],
    },
    BusinessCaseStatusMetadata {
        value: "FAILED",
        label: "失败",
        color: "#FF3B30",
        category: "terminal",
        is_terminal: true,
        manual_transition_enabled: true,
        workflow_target_enabled: true,
        default_for_actions: &["fail_case"],
    },
];

#[derive(Debug, Clone, Default)]
pub struct BusinessCaseUpdatePayload {
    pub case_type: Option<String>,
    pub description: Option<String>,
    pub context: Option<HashMap<String, serde_json::Value>>,
    pub status: Option<String>,
    pub stand: Option<String>,
    pub gate: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BusinessCaseTerminalUpdatePayload {
    pub action: String,
    pub target_status: String,
    pub actor: String,
    pub reason: Option<String>,
    pub write_finished_at: bool,
    pub workflow_run_id: Option<String>,
    pub workflow_outcome: Option<String>,
    pub receipt_group_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BusinessCaseAppendResult {
    pub case: FlightBusinessCase,
    pub append: BusinessCaseAppendEntry,
    pub inserted: bool,
}

pub trait BusinessCaseEventPublisher: Send + Sync {
    fn publish_appended<'a>(
        &'a self,
        business_case: &'a FlightBusinessCase,
        append_entry_id: &'a str,
        operator: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), DomainError>> + Send + 'a>>;

    fn publish_updated<'a>(
        &'a self,
        _business_case: &'a FlightBusinessCase,
        _event_name: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), DomainError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
}

/// 业务事项校验 @提及时，只需要知道「这条航班的协作群里有哪些人可以被提及」。
///
/// 此前这里直接依赖整个 `DispatchCollaborationRepository`（33 个方法），
/// 只为了调用其中 2 个；代价是任何想构造 `BusinessCaseService` 的地方
/// 都得提供 33 个方法的实现，于是只能靠可选依赖 + 空实现桩绕过去。
/// 收窄成 1 个方法后，这个依赖可以是必填的。
#[async_trait::async_trait]
pub trait BusinessCaseMentionAudience: Send + Sync {
    /// 返回该航班协作群中允许被 @ 的用户 ID；无群或查询失败时返回空集，
    /// 调用方据此把所有 @提及过滤掉。
    async fn mentionable_user_ids(&self, flight_id: &str) -> Vec<String>;
}
