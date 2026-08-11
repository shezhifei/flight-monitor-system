use fms_domain::models::ai_proposal::{ApprovalPolicy, RiskLevel};
use fms_domain::ports::ai_object_policy_repository::AiObjectPolicySubject;
pub(crate) fn object_policy_subject(
    actor_id: &str,
    actor_permissions: &[String],
    actor_department_id: Option<&str>,
) -> AiObjectPolicySubject {
    let mut subject = AiObjectPolicySubject::new(actor_id, actor_permissions.to_vec());
    subject.department_id = actor_department_id.map(str::to_string);
    subject
}

pub(crate) fn feature_enabled(name: &str) -> bool {
    std::env::var(name)
        .map(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

pub(crate) fn normalize_policy_for_risk(risk_level: RiskLevel, requested_policy: ApprovalPolicy) -> ApprovalPolicy {
    match risk_level {
        RiskLevel::Critical => {
            if requested_policy == ApprovalPolicy::RequireFlowableApproval {
                ApprovalPolicy::RequireFlowableApproval
            } else {
                ApprovalPolicy::RequireSupervisorApproval
            }
        }
        RiskLevel::High => {
            if matches!(
                requested_policy,
                ApprovalPolicy::RequireSupervisorApproval | ApprovalPolicy::RequireFlowableApproval
            ) {
                requested_policy
            } else {
                ApprovalPolicy::RequireApproval
            }
        }
        RiskLevel::Medium => {
            if requested_policy == ApprovalPolicy::RequireFlowableApproval {
                ApprovalPolicy::RequireFlowableApproval
            } else {
                ApprovalPolicy::RequireApproval
            }
        }
        RiskLevel::Low => requested_policy,
    }
}
