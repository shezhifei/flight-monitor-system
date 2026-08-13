//! Tool authorization decision types.
//!
//! The `ToolAuthorizationService` (application layer) is the **single
//! authorization decision point** for protected tool calls. It evaluates a
//! [`ToolAuthorizationContext`] produced by the Rust edge from the
//! authenticated session/token, and returns a [`ToolAuthorizationDecision`]
//! that the consumer publishes to `ai_runtime_commands`.
//!
//! The decision is deterministic for a given context. `lease_id` is
//! generated at decision time, but the decision logic itself does not
//! branch on clocks or random values.
//!
//! Failure codes mirror the names documented in the
//! "AI Agent Resilient Tool Architecture" plan and are stable strings
//! that may be returned through API surfaces and persistence.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::tool_governance::ResolvedToolGovernance;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ToolAuthorizationDenialCode {
    ToolNotInEntityCapability,
    ToolRequiredPermissionUndeclared,
    ToolActorPermissionDenied,
    ToolObjectPolicyDenied,
    ToolAuthContextRequired,
    ToolGovernanceRequiresProposal,
}

impl ToolAuthorizationDenialCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ToolNotInEntityCapability => "TOOL_NOT_IN_ENTITY_CAPABILITY",
            Self::ToolRequiredPermissionUndeclared => "TOOL_REQUIRED_PERMISSION_UNDECLARED",
            Self::ToolActorPermissionDenied => "TOOL_ACTOR_PERMISSION_DENIED",
            Self::ToolObjectPolicyDenied => "TOOL_OBJECT_POLICY_DENIED",
            Self::ToolAuthContextRequired => "TOOL_AUTH_CONTEXT_REQUIRED",
            Self::ToolGovernanceRequiresProposal => "TOOL_GOVERNANCE_REQUIRES_PROPOSAL",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum ToolAuthorizationDecision {
    AllowDirect {
        lease_id: String,
        max_retries: u32,
        timeout_seconds: u32,
    },
    ProposalOnly {
        reason: String,
    },
    Deny {
        code: ToolAuthorizationDenialCode,
        message: String,
    },
}

impl ToolAuthorizationDecision {
    pub fn is_allow_direct(&self) -> bool {
        matches!(self, Self::AllowDirect { .. })
    }

    pub fn is_proposal_only(&self) -> bool {
        matches!(self, Self::ProposalOnly { .. })
    }

    pub fn is_deny(&self) -> bool {
        matches!(self, Self::Deny { .. })
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AllowDirect { .. } => "allow_direct",
            Self::ProposalOnly { .. } => "proposal_only",
            Self::Deny { .. } => "deny",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObjectPolicyDecision {
    pub object_type: String,
    pub object_id: String,
    pub permission: String,
    pub allowed: bool,
}

impl ObjectPolicyDecision {
    pub fn allow(object_type: impl Into<String>, object_id: impl Into<String>, permission: impl Into<String>) -> Self {
        Self {
            object_type: object_type.into(),
            object_id: object_id.into(),
            permission: permission.into(),
            allowed: true,
        }
    }

    pub fn deny(object_type: impl Into<String>, object_id: impl Into<String>, permission: impl Into<String>) -> Self {
        Self {
            object_type: object_type.into(),
            object_id: object_id.into(),
            permission: permission.into(),
            allowed: false,
        }
    }

    pub fn matches(&self, object_type: &str, object_id: &str, permission: &str) -> bool {
        self.object_type == object_type && self.object_id == object_id && self.permission == permission
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolAuthorizationContext {
    pub requester_user_id: String,
    pub requester_user_roles: Vec<String>,
    pub requester_permissions: Vec<String>,
    pub requester_object_policies: Vec<ObjectPolicyDecision>,
    pub entity_tool_allowlist: Vec<String>,
    pub tool_governance: ResolvedToolGovernance,
    pub tool_call_pk: String,
    pub tool_args: Value,
    pub feature_flags: HashMap<String, bool>,
}

impl ToolAuthorizationContext {
    pub fn has_object_policy(&self, object_type: &str, object_id: &str, permission: &str) -> Option<bool> {
        self.requester_object_policies
            .iter()
            .find(|decision| decision.matches(object_type, object_id, permission))
            .map(|decision| decision.allowed)
    }

    pub fn feature_flag_enabled(&self, name: &str) -> bool {
        self.feature_flags.get(name).copied().unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::tool_governance::ToolGovernancePreset;

    fn sample_governance() -> ResolvedToolGovernance {
        ToolGovernancePreset::ReadOnlyQuery.default_governance("weather_at_airport")
    }

    #[test]
    fn denial_code_strings_match_plan() {
        assert_eq!(
            ToolAuthorizationDenialCode::ToolNotInEntityCapability.as_str(),
            "TOOL_NOT_IN_ENTITY_CAPABILITY"
        );
        assert_eq!(
            ToolAuthorizationDenialCode::ToolRequiredPermissionUndeclared.as_str(),
            "TOOL_REQUIRED_PERMISSION_UNDECLARED"
        );
        assert_eq!(
            ToolAuthorizationDenialCode::ToolActorPermissionDenied.as_str(),
            "TOOL_ACTOR_PERMISSION_DENIED"
        );
        assert_eq!(
            ToolAuthorizationDenialCode::ToolObjectPolicyDenied.as_str(),
            "TOOL_OBJECT_POLICY_DENIED"
        );
        assert_eq!(
            ToolAuthorizationDenialCode::ToolAuthContextRequired.as_str(),
            "TOOL_AUTH_CONTEXT_REQUIRED"
        );
        assert_eq!(
            ToolAuthorizationDenialCode::ToolGovernanceRequiresProposal.as_str(),
            "TOOL_GOVERNANCE_REQUIRES_PROPOSAL"
        );
    }

    #[test]
    fn decision_serializes_with_snake_case_tag() {
        let allow = ToolAuthorizationDecision::AllowDirect {
            lease_id: "lease-1".into(),
            max_retries: 2,
            timeout_seconds: 30,
        };
        let s = serde_json::to_string(&allow).unwrap();
        assert!(s.contains("\"decision\":\"allow_direct\""), "got: {s}");

        let deny = ToolAuthorizationDecision::Deny {
            code: ToolAuthorizationDenialCode::ToolActorPermissionDenied,
            message: "missing permission".into(),
        };
        let s = serde_json::to_string(&deny).unwrap();
        assert!(s.contains("\"decision\":\"deny\""), "got: {s}");
        assert!(s.contains("\"code\":\"TOOL_ACTOR_PERMISSION_DENIED\""), "got: {s}");
    }

    #[test]
    fn object_policy_decision_matches_on_triple() {
        let d = ObjectPolicyDecision::allow("Flight", "CA1234", "flight:read");
        assert!(d.matches("Flight", "CA1234", "flight:read"));
        assert!(!d.matches("Flight", "CA5678", "flight:read"));
        assert!(!d.matches("Todo", "CA1234", "flight:read"));
        assert!(!d.matches("Flight", "CA1234", "flight:write"));
    }

    #[test]
    fn context_feature_flag_defaults_to_false() {
        let ctx = ToolAuthorizationContext {
            requester_user_id: "u-1".into(),
            requester_user_roles: vec![],
            requester_permissions: vec![],
            requester_object_policies: vec![],
            entity_tool_allowlist: vec![],
            tool_governance: sample_governance(),
            tool_call_pk: "tpc-1".into(),
            tool_args: Value::Null,
            feature_flags: HashMap::new(),
        };
        assert!(!ctx.feature_flag_enabled("disable_direct_l0_read"));
    }
}
