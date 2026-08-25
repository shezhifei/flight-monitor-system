//! 业务事项应用服务。

pub mod mention_audience;
pub mod schemas;

mod service;

#[cfg(test)]
mod tests;

pub use mention_audience::{CollaborationMentionAudience, NoMentionAudience};
pub use schemas::{
    BusinessCaseAppendResult, BusinessCaseEventPublisher, BusinessCaseMentionAudience, BusinessCaseStatusMetadata,
    BusinessCaseTerminalUpdatePayload, BusinessCaseUpdatePayload, BUSINESS_CASE_ALLOWED_STATUSES,
    BUSINESS_CASE_STATUS_METADATA,
};
pub use service::{BusinessCaseService, BusinessCaseServiceOps};
