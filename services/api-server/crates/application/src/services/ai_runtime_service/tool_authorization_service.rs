//! Tool authorization service (Phase 1a).
//!
//! Single authorization decision point for protected tool calls. Given a
//! [`ToolAuthorizationContext`] derived from the Rust edge (so that
//! `requester_user_id`, `requester_user_roles`, `requester_permissions`
//! and `requester_object_policies` are trusted claims — not values
//! pushed up by the Python sidecar), the service returns a
//! [`ToolAuthorizationDecision`] that the MQ consumer publishes as
//! `tool_lease` / `tool_proposal_only` / `tool_denied` via the
//! `ai_runtime_commands` queue.
//!
//! The decision algorithm is deterministic for a given context: the
//! same `(context)` always produces the same decision variant.
//! `lease_id` is the only non-deterministic value and is only present
//! on the `AllowDirect` variant.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
#[cfg(test)]
use fms_domain::models::tool_authorization::ObjectPolicyDecision;
use fms_domain::models::tool_authorization::{
    ToolAuthorizationContext, ToolAuthorizationDecision, ToolAuthorizationDenialCode,
};
use fms_domain::models::tool_governance::{ExecutionMode, GovernanceTier};
use serde_json::Value;
use thiserror::Error;
use ulid::Ulid;

const PLACEHOLDER_PERMISSION_MARKER: &str = "__PLACEHOLDER__";
const BASE_CHAT_PERMISSION: &str = "ai:chat";

pub struct ToolAuthorizationService {
    feature_flags: Arc<dyn FeatureFlagSource>,
}

impl std::fmt::Debug for ToolAuthorizationService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolAuthorizationService").finish_non_exhaustive()
    }
}

impl ToolAuthorizationService {
    pub fn new(feature_flags: Arc<dyn FeatureFlagSource>) -> Self {
        Self { feature_flags }
    }

    pub async fn authorize(
        &self,
        context: ToolAuthorizationContext,
    ) -> Result<ToolAuthorizationDecision, ToolAuthorizationError> {
        let governance = &context.tool_governance;
        let tool_name = governance.tool_name.clone();

        if self.is_public_direct_fast_path(&context) {
            return Ok(ToolAuthorizationDecision::AllowDirect {
                lease_id: generate_lease_id(),
                max_retries: governance.retry_policy.max_retries,
                timeout_seconds: governance.timeout_seconds,
            });
        }

        if !context.entity_tool_allowlist.iter().any(|name| name == &tool_name) {
            return Ok(ToolAuthorizationDecision::Deny {
                code: ToolAuthorizationDenialCode::ToolNotInEntityCapability,
                message: format!(
                    "tool '{}' is not in the resolved entity capability allowlist",
                    tool_name
                ),
            });
        }

        let requester_present = !context.requester_user_id.trim().is_empty();
        let has_permissions = !context.requester_permissions.is_empty();
        if !requester_present || !has_permissions {
            return Ok(ToolAuthorizationDecision::Deny {
                code: ToolAuthorizationDenialCode::ToolAuthContextRequired,
                message: "protected tool requires an authenticated requester context with permissions".to_string(),
            });
        }

        if !governance.required_account_permissions.is_empty()
            && has_undeclared_permissions(&governance.required_account_permissions)
        {
            return Ok(ToolAuthorizationDecision::Deny {
                code: ToolAuthorizationDenialCode::ToolRequiredPermissionUndeclared,
                message: format!(
                    "tool '{}' declares required_account_permissions that the resolver could not bind",
                    tool_name
                ),
            });
        }

        if governance.execution_mode == ExecutionMode::ProposalOnly {
            return Ok(ToolAuthorizationDecision::ProposalOnly {
                reason: format!(
                    "tool '{}' is governed as proposal-only; direct execution is not permitted in this context",
                    tool_name
                ),
            });
        }

        if !actor_has_all_permissions(&context.requester_permissions, &governance.required_account_permissions) {
            return Ok(ToolAuthorizationDecision::Deny {
                code: ToolAuthorizationDenialCode::ToolActorPermissionDenied,
                message: format!(
                    "requester '{}' is missing one of required_account_permissions {:?} for tool '{}'",
                    context.requester_user_id, governance.required_account_permissions, tool_name
                ),
            });
        }

        if governance.object_policy.permission.is_some() {
            match self.evaluate_object_policy(&context) {
                ObjectPolicyCheck::Allow => {}
                ObjectPolicyCheck::NoDecision | ObjectPolicyCheck::Denied => {
                    return Ok(ToolAuthorizationDecision::Deny {
                        code: ToolAuthorizationDenialCode::ToolObjectPolicyDenied,
                        message: format!(
                            "object policy decision for tool '{}' is missing or denies the target object",
                            tool_name
                        ),
                    });
                }
            }
        }

        if governance.execution_mode == ExecutionMode::Direct && self.direct_tier_disabled(governance.tier).await? {
            return Ok(ToolAuthorizationDecision::ProposalOnly {
                reason: format!(
                    "tool '{}' is demoted to proposal-only because direct execution is disabled for tier {}",
                    tool_name,
                    governance.tier.as_str()
                ),
            });
        }

        Ok(ToolAuthorizationDecision::AllowDirect {
            lease_id: generate_lease_id(),
            max_retries: governance.retry_policy.max_retries,
            timeout_seconds: governance.timeout_seconds,
        })
    }

    fn is_public_direct_fast_path(&self, context: &ToolAuthorizationContext) -> bool {
        let governance = &context.tool_governance;
        if !governance.is_public_direct() {
            return false;
        }
        if !governance.public {
            return false;
        }
        if governance.side_effect {
            return false;
        }
        if governance.object_policy.permission.is_some() {
            let Some(object_type_arg) = governance.object_policy.object_type_arg.as_deref() else {
                return false;
            };
            let Some(object_id_arg) = governance.object_policy.object_id_arg.as_deref() else {
                return false;
            };
            let Some(permission) = governance.object_policy.permission.as_deref() else {
                return false;
            };
            let (Some(object_type), Some(object_id)) = (
                extract_string_arg(&context.tool_args, object_type_arg),
                extract_string_arg(&context.tool_args, object_id_arg),
            ) else {
                return false;
            };
            match context.has_object_policy(&object_type, &object_id, permission) {
                Some(true) => {}
                Some(false) => return false,
                None => return false,
            }
        }
        let has_base_chat = context
            .requester_permissions
            .iter()
            .any(|p| p == "*" || p == BASE_CHAT_PERMISSION);
        let requester_present = !context.requester_user_id.trim().is_empty();
        requester_present && has_base_chat
    }

    fn evaluate_object_policy(&self, context: &ToolAuthorizationContext) -> ObjectPolicyCheck {
        let governance = &context.tool_governance;
        let Some(permission) = governance.object_policy.permission.as_deref() else {
            return ObjectPolicyCheck::Allow;
        };
        let (Some(object_type_arg), Some(object_id_arg)) = (
            governance.object_policy.object_type_arg.as_deref(),
            governance.object_policy.object_id_arg.as_deref(),
        ) else {
            return ObjectPolicyCheck::NoDecision;
        };
        let (Some(object_type), Some(object_id)) = (
            extract_string_arg(&context.tool_args, object_type_arg),
            extract_string_arg(&context.tool_args, object_id_arg),
        ) else {
            return ObjectPolicyCheck::NoDecision;
        };
        match context.has_object_policy(&object_type, &object_id, permission) {
            Some(true) => ObjectPolicyCheck::Allow,
            Some(false) => ObjectPolicyCheck::Denied,
            None => ObjectPolicyCheck::NoDecision,
        }
    }

    async fn direct_tier_disabled(&self, tier: GovernanceTier) -> Result<bool, ToolAuthorizationError> {
        let flag = format!("disable_direct_{}", tier.as_str());
        match self.feature_flags.is_enabled(&flag).await {
            Ok(enabled) => Ok(enabled),
            Err(err) => Err(ToolAuthorizationError::FeatureFlagSource(err)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ObjectPolicyCheck {
    Allow,
    Denied,
    NoDecision,
}

fn actor_has_all_permissions(actor_permissions: &[String], required: &[String]) -> bool {
    if required.is_empty() {
        return true;
    }
    required
        .iter()
        .all(|needed| actor_permissions.iter().any(|p| p == needed || p == "*"))
}

fn has_undeclared_permissions(required: &[String]) -> bool {
    required
        .iter()
        .all(|permission| permission.trim().is_empty() || permission == PLACEHOLDER_PERMISSION_MARKER)
}

fn extract_string_arg(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(Value::as_str).map(str::to_string)
}

fn generate_lease_id() -> String {
    Ulid::new().to_string()
}

#[async_trait]
pub trait FeatureFlagSource: Send + Sync {
    async fn is_enabled(&self, name: &str) -> Result<bool, String>;
}

pub struct StaticFeatureFlagSource {
    flags: HashMap<String, bool>,
}

impl StaticFeatureFlagSource {
    pub fn new(flags: HashMap<String, bool>) -> Self {
        Self { flags }
    }

    pub fn empty() -> Self {
        Self { flags: HashMap::new() }
    }
}

impl std::fmt::Debug for StaticFeatureFlagSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StaticFeatureFlagSource")
            .field("flags", &self.flags)
            .finish()
    }
}

#[async_trait]
impl FeatureFlagSource for StaticFeatureFlagSource {
    async fn is_enabled(&self, name: &str) -> Result<bool, String> {
        Ok(self.flags.get(name).copied().unwrap_or(false))
    }
}

#[derive(Debug, Error)]
pub enum ToolAuthorizationError {
    #[error("feature flag source error: {0}")]
    FeatureFlagSource(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::ai_runtime_service::tool_authorization_service::FeatureFlagSource as _;
    use fms_domain::models::tool_governance::{ObjectPolicy, ResolvedToolGovernance, ToolGovernancePreset};
    use serde_json::json;
    use std::collections::HashMap;

    fn preset_governance(preset: ToolGovernancePreset, name: &str) -> ResolvedToolGovernance {
        let mut g = preset.default_governance(name);
        g.object_policy = ObjectPolicy {
            object_type_arg: None,
            object_id_arg: None,
            permission: None,
        };
        g
    }

    fn protected_governance(
        tool_name: &str,
        required: Vec<String>,
        object_policy: ObjectPolicy,
    ) -> ResolvedToolGovernance {
        let mut g = ToolGovernancePreset::InternalWorkspaceWrite.default_governance(tool_name);
        g.required_account_permissions = required;
        g.object_policy = object_policy;
        g
    }

    fn base_context(governance: ResolvedToolGovernance) -> ToolAuthorizationContext {
        ToolAuthorizationContext {
            requester_user_id: "user-1".into(),
            requester_user_roles: vec!["dispatcher".into()],
            requester_permissions: vec!["ai:chat".into(), "flight:read".into()],
            requester_object_policies: vec![],
            entity_tool_allowlist: vec![governance.tool_name.clone()],
            tool_governance: governance,
            tool_call_pk: "tpc-1".into(),
            tool_args: json!({}),
            feature_flags: HashMap::new(),
        }
    }

    fn flags_with(pairs: &[(&str, bool)]) -> Arc<dyn FeatureFlagSource> {
        let mut map = HashMap::new();
        for (name, value) in pairs {
            map.insert((*name).to_string(), *value);
        }
        Arc::new(StaticFeatureFlagSource::new(map))
    }

    #[tokio::test]
    async fn public_l0_read_returns_allow_direct_without_actor_check() {
        let governance = preset_governance(ToolGovernancePreset::ReadOnlyQuery, "weather_at_airport");
        let mut ctx = base_context(governance);
        ctx.requester_permissions = vec!["ai:chat".into()];
        ctx.entity_tool_allowlist = vec![];
        let svc = ToolAuthorizationService::new(Arc::new(StaticFeatureFlagSource::empty()));

        let decision = svc.authorize(ctx).await.unwrap();
        match decision {
            ToolAuthorizationDecision::AllowDirect {
                max_retries,
                timeout_seconds,
                ..
            } => {
                assert_eq!(max_retries, 2);
                assert_eq!(timeout_seconds, 30);
            }
            other => panic!("expected AllowDirect, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn tool_not_in_entity_capability_denied() {
        let governance = preset_governance(ToolGovernancePreset::ReadOnlyQuery, "weather_at_airport");
        let mut ctx = base_context(governance.clone());
        ctx.entity_tool_allowlist = vec!["other_tool".into()];
        ctx.tool_governance.public = false;
        ctx.tool_governance.authorization_mode = fms_domain::models::tool_governance::AuthorizationMode::RustPdp;
        let svc = ToolAuthorizationService::new(Arc::new(StaticFeatureFlagSource::empty()));

        let decision = svc.authorize(ctx).await.unwrap();
        match decision {
            ToolAuthorizationDecision::Deny { code, .. } => {
                assert_eq!(code, ToolAuthorizationDenialCode::ToolNotInEntityCapability);
            }
            other => panic!("expected Deny, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn protected_tool_denied_when_requester_missing() {
        let governance = protected_governance(
            "flight_update_status".into(),
            vec!["flight:write".into()],
            ObjectPolicy {
                object_type_arg: None,
                object_id_arg: None,
                permission: None,
            },
        );
        let mut ctx = base_context(governance);
        ctx.requester_user_id.clear();
        ctx.requester_permissions = vec!["flight:write".into()];
        let svc = ToolAuthorizationService::new(Arc::new(StaticFeatureFlagSource::empty()));

        let decision = svc.authorize(ctx).await.unwrap();
        match decision {
            ToolAuthorizationDecision::Deny { code, .. } => {
                assert_eq!(code, ToolAuthorizationDenialCode::ToolAuthContextRequired);
            }
            other => panic!("expected Deny, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn protected_tool_denied_when_permission_undeclared() {
        let governance = protected_governance(
            "todo_create".into(),
            vec!["__PLACEHOLDER__".into()],
            ObjectPolicy {
                object_type_arg: None,
                object_id_arg: None,
                permission: None,
            },
        );
        let mut ctx = base_context(governance);
        ctx.requester_permissions = vec!["flight:write".into()];
        let svc = ToolAuthorizationService::new(Arc::new(StaticFeatureFlagSource::empty()));

        let decision = svc.authorize(ctx).await.unwrap();
        match decision {
            ToolAuthorizationDecision::Deny { code, .. } => {
                assert_eq!(code, ToolAuthorizationDenialCode::ToolRequiredPermissionUndeclared);
            }
            other => panic!("expected Deny, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn protected_tool_returns_proposal_only_for_standard_requester() {
        let governance = protected_governance(
            "todo_create".into(),
            vec!["todo:write".into()],
            ObjectPolicy {
                object_type_arg: None,
                object_id_arg: None,
                permission: None,
            },
        );
        let mut ctx = base_context(governance);
        ctx.requester_permissions = vec!["todo:write".into()];
        let svc = ToolAuthorizationService::new(Arc::new(StaticFeatureFlagSource::empty()));

        let decision = svc.authorize(ctx).await.unwrap();
        match decision {
            ToolAuthorizationDecision::ProposalOnly { reason } => {
                assert!(
                    reason.contains("todo_create"),
                    "reason should mention tool name: {reason}"
                );
            }
            other => panic!("expected ProposalOnly, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn protected_tool_allows_direct_when_requester_has_all_permissions() {
        let mut governance = protected_governance(
            "flight_update_status".into(),
            vec!["flight:write".into()],
            ObjectPolicy {
                object_type_arg: None,
                object_id_arg: None,
                permission: None,
            },
        );
        governance.execution_mode = ExecutionMode::Direct;
        let mut ctx = base_context(governance);
        ctx.requester_permissions = vec!["flight:write".into()];
        let svc = ToolAuthorizationService::new(Arc::new(StaticFeatureFlagSource::empty()));

        let decision = svc.authorize(ctx).await.unwrap();
        match decision {
            ToolAuthorizationDecision::AllowDirect {
                max_retries,
                timeout_seconds,
                lease_id,
            } => {
                assert!(!lease_id.is_empty());
                assert_eq!(max_retries, 0);
                assert_eq!(timeout_seconds, 60);
            }
            other => panic!("expected AllowDirect, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn protected_tool_denied_when_object_policy_rejects_target() {
        let governance = protected_governance(
            "flight_update_status".into(),
            vec!["flight:write".into()],
            ObjectPolicy {
                object_type_arg: Some("flight_id".into()),
                object_id_arg: Some("flight_id".into()),
                permission: Some("flight:write".into()),
            },
        );
        let mut governance = governance;
        governance.execution_mode = ExecutionMode::Direct;
        let mut ctx = base_context(governance);
        ctx.requester_permissions = vec!["flight:write".into()];
        ctx.tool_args = json!({ "flight_id": "CA1234" });
        ctx.requester_object_policies = vec![ObjectPolicyDecision::deny("Flight", "CA1234", "flight:write")];
        let svc = ToolAuthorizationService::new(Arc::new(StaticFeatureFlagSource::empty()));

        let decision = svc.authorize(ctx).await.unwrap();
        match decision {
            ToolAuthorizationDecision::Deny { code, .. } => {
                assert_eq!(code, ToolAuthorizationDenialCode::ToolObjectPolicyDenied);
            }
            other => panic!("expected Deny, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn feature_flag_can_demote_direct_to_proposal_only() {
        let mut governance = protected_governance(
            "flight_update_status".into(),
            vec!["flight:write".into()],
            ObjectPolicy {
                object_type_arg: None,
                object_id_arg: None,
                permission: None,
            },
        );
        governance.execution_mode = ExecutionMode::Direct;
        let mut ctx = base_context(governance);
        ctx.requester_permissions = vec!["flight:write".into()];
        let svc = ToolAuthorizationService::new(flags_with(&[("disable_direct_L1_WORKSPACE_WRITE", true)]));

        let decision = svc.authorize(ctx).await.unwrap();
        match decision {
            ToolAuthorizationDecision::ProposalOnly { reason } => {
                assert!(
                    reason.contains("L1_WORKSPACE_WRITE"),
                    "reason should mention tier: {reason}"
                );
            }
            other => panic!("expected ProposalOnly, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn public_l0_read_with_protected_object_policy_is_not_a_fast_path() {
        let mut governance = preset_governance(ToolGovernancePreset::ReadOnlyQuery, "flight_status_lookup");
        governance.object_policy = ObjectPolicy {
            object_type_arg: Some("flight_id".into()),
            object_id_arg: Some("flight_id".into()),
            permission: Some("flight:read".into()),
        };
        let mut ctx = base_context(governance);
        ctx.requester_permissions = vec!["ai:chat".into()];
        ctx.requester_object_policies = vec![ObjectPolicyDecision::deny("Flight", "CA1234", "flight:read")];
        ctx.tool_args = json!({ "flight_id": "CA1234" });
        let svc = ToolAuthorizationService::new(Arc::new(StaticFeatureFlagSource::empty()));

        let decision = svc.authorize(ctx).await.unwrap();
        match decision {
            ToolAuthorizationDecision::Deny { code, .. } => {
                assert_eq!(code, ToolAuthorizationDenialCode::ToolObjectPolicyDenied);
            }
            other => panic!("expected Deny, got {other:?}"),
        }
    }
}
