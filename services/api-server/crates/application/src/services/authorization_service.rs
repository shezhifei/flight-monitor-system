use crate::schemas::auth_schemas::TokenData;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeLevel {
    Department,
    Common,
}

pub struct PermissionCatalog;

impl PermissionCatalog {
    pub const BUSINESS_CASE_CREATE: &'static str = "business_case.create";
    pub const BUSINESS_CASE_READ: &'static str = "business_case.read";
    pub const BUSINESS_CASE_APPEND: &'static str = "business_case.append";
    pub const BUSINESS_CASE_UPDATE: &'static str = "business_case.update";
    pub const BUSINESS_CASE_STATUS_TRANSITION: &'static str = "business_case.status_transition";
    pub const BUSINESS_CASE_DELETE: &'static str = "business_case.delete";

    pub const WORKFLOW_RUN_START: &'static str = "workflow_run.start";
    pub const WORKFLOW_RUN_READ: &'static str = "workflow_run.read";
    pub const WORKFLOW_RUN_ACT: &'static str = "workflow_run.act";

    pub const WORKFLOW_DEFINITION_READ: &'static str = "workflow_definition.read";
    pub const WORKFLOW_DEFINITION_EDIT: &'static str = "workflow_definition.edit";
    pub const WORKFLOW_DEFINITION_PUBLISH: &'static str = "workflow_definition.publish";
    pub const WORKFLOW_DEFINITION_DEPRECATE: &'static str = "workflow_definition.deprecate";

    pub const AUTOMATION_NOTIFY_SEND: &'static str = "automation.notify_send";
    pub const AUTOMATION_DISPATCH_CREATE: &'static str = "automation.dispatch_create";
    pub const AUTOMATION_BUSINESS_CASE_COMPLETE: &'static str = "automation.business_case_complete";
    pub const AUTOMATION_BUSINESS_CASE_FAIL: &'static str = "automation.business_case_fail";

    pub const DISPATCH_ORDER_READ: &'static str = "dispatch_order.read";
    pub const DISPATCH_ORDER_CREATE: &'static str = "dispatch_order.create";
    pub const DISPATCH_ORDER_UPDATE: &'static str = "dispatch_order.update";
    pub const DISPATCH_ORDER_PUBLISH: &'static str = "dispatch_order.publish";
    pub const DISPATCH_ORDER_CANCEL: &'static str = "dispatch_order.cancel";

    pub const DISPATCH_CATALOG_READ: &'static str = "dispatch_catalog.read";
    pub const DISPATCH_CATALOG_EDIT: &'static str = "dispatch_catalog.edit";

    pub const SHIFT_HANDOVER_READ: &'static str = "shift_handover.read";
    pub const SHIFT_HANDOVER_CREATE: &'static str = "shift_handover.create";
    pub const SHIFT_HANDOVER_SUBMIT: &'static str = "shift_handover.submit";
    pub const SHIFT_HANDOVER_ACK: &'static str = "shift_handover.ack";

    pub const NOTIFICATION_READ: &'static str = "notification.read";
    pub const NOTIFICATION_SEND: &'static str = "notification.send";
    pub const NOTIFICATION_RECEIPT_READ: &'static str = "notification.receipt_read";
    pub const NOTIFICATION_RECEIPT_MANAGE: &'static str = "notification.receipt_manage";

    pub const FLIGHT_READ: &'static str = "flight.read";
    pub const FLIGHT_UPDATE: &'static str = "flight.update";
    pub const FLIGHT_TIMELINE_EDIT: &'static str = "flight.timeline_edit";
    pub const FLIGHT_IMPORT_COMMIT: &'static str = "flight.import_commit";
    pub const FLIGHT_REPORT_GENERATE: &'static str = "flight.report_generate";

    pub const AUTH_ROLE_READ: &'static str = "auth_role.read";
    pub const AUTH_ROLE_EDIT: &'static str = "auth_role.edit";
    pub const AUTH_PERMISSION_TEMPLATE_READ: &'static str = "auth_permission_template.read";
    pub const AUTH_PERMISSION_TEMPLATE_EDIT: &'static str = "auth_permission_template.edit";
    pub const USER_ADMIN_READ: &'static str = "user_admin.read";
    pub const USER_ADMIN_EDIT: &'static str = "user_admin.edit";

    pub const SYSTEM_CONFIG_READ: &'static str = "system.config_read";
    pub const SYSTEM_CONFIG_WRITE: &'static str = "system.config_write";
    pub const SYSTEM_OPS_ADMIN: &'static str = "system.ops_admin";
}

pub struct AuthorizationService;

impl AuthorizationService {
    pub async fn get_allowed_ai_actions(&self, _user_id: &str, roles: &[String]) -> Result<Vec<String>, String> {
        let mut actions = Vec::new();
        for (action, required_permissions) in AI_ACTION_PERMISSION_MAP {
            if has_any_ai_permission(roles, required_permissions) {
                actions.push((*action).to_string());
            }
        }
        actions.sort();
        actions.dedup();
        Ok(actions)
    }

    pub fn has_ai_action_grants(user_permissions: &[String], required_permissions: &[String]) -> bool {
        if required_permissions.is_empty() {
            return false;
        }

        has_any_ai_permission(
            user_permissions,
            &required_permissions.iter().map(String::as_str).collect::<Vec<_>>(),
        )
    }

    pub fn has_ai_supervisor_approval_grant(user_permissions: &[String]) -> bool {
        user_permissions.iter().any(|permission| {
            matches!(
                permission.as_str(),
                "*" | "ai:approve.supervisor"
                    | "system.ops_admin"
                    | "system:admin"
                    | "flowable:manage"
                    | "dispatch:manage"
            )
        })
    }

    pub fn is_authenticated(claims: &TokenData) -> bool {
        claims
            .sub
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some()
    }

    pub fn is_admin(claims: &TokenData) -> bool {
        claims.is_admin.unwrap_or(false) || Self::has_direct_permission(claims, "*")
    }

    pub fn department_id<'a>(claims: &'a TokenData) -> Option<&'a str> {
        claims
            .department_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    pub fn department_name<'a>(claims: &'a TokenData) -> Option<&'a str> {
        claims
            .department
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    pub fn has_grant(claims: &TokenData, permission: &str) -> bool {
        Self::is_admin(claims)
            || Self::has_direct_permission(claims, permission)
            || Self::legacy_aliases(permission)
                .iter()
                .any(|alias| Self::has_direct_permission(claims, alias))
    }

    pub fn has_any_grant(claims: &TokenData, permissions: &[&str]) -> bool {
        permissions.iter().any(|permission| Self::has_grant(claims, permission))
    }

    pub fn can_manage_common_scope(claims: &TokenData, permission: &str) -> bool {
        Self::is_admin(claims)
            || matches!(
                permission,
                PermissionCatalog::WORKFLOW_DEFINITION_READ
                    | PermissionCatalog::WORKFLOW_DEFINITION_EDIT
                    | PermissionCatalog::WORKFLOW_DEFINITION_PUBLISH
                    | PermissionCatalog::WORKFLOW_DEFINITION_DEPRECATE
                    | PermissionCatalog::AUTOMATION_NOTIFY_SEND
                    | PermissionCatalog::AUTOMATION_DISPATCH_CREATE
                    | PermissionCatalog::AUTOMATION_BUSINESS_CASE_COMPLETE
                    | PermissionCatalog::AUTOMATION_BUSINESS_CASE_FAIL
                    | PermissionCatalog::BUSINESS_CASE_CREATE
                    | PermissionCatalog::BUSINESS_CASE_READ
                    | PermissionCatalog::BUSINESS_CASE_APPEND
                    | PermissionCatalog::BUSINESS_CASE_UPDATE
                    | PermissionCatalog::BUSINESS_CASE_STATUS_TRANSITION
                    | PermissionCatalog::BUSINESS_CASE_DELETE
            ) && Self::has_grant(claims, permission)
    }

    pub fn scope_grant(claims: &TokenData, permission: &str, scope: ScopeLevel) -> bool {
        match scope {
            ScopeLevel::Department => Self::has_grant(claims, permission),
            ScopeLevel::Common => Self::can_manage_common_scope(claims, permission),
        }
    }

    fn has_direct_permission(claims: &TokenData, permission: &str) -> bool {
        claims.permissions.iter().any(|item| item == permission || item == "*")
    }

    /// Legacy permission aliases bridge old permission names to new ones.
    ///
    /// This mapping will be removed after 2026-10-01.
    /// All frontends and API consumers should migrate to new permission names before this date.
    /// Current permission boundaries are documented in `docs/SYSTEM_MANUAL.md`.
    fn legacy_aliases(permission: &str) -> &'static [&'static str] {
        tracing::debug!(permission = permission, "using legacy permission alias");
        match permission {
            PermissionCatalog::BUSINESS_CASE_READ => &["flight:read"],

            PermissionCatalog::WORKFLOW_RUN_START => &["flowable:manage"],
            PermissionCatalog::WORKFLOW_RUN_READ => &["flight:read", "flowable:read", "flowable:manage"],
            PermissionCatalog::WORKFLOW_RUN_ACT => &["flowable:manage"],

            PermissionCatalog::WORKFLOW_DEFINITION_READ => &["flight:read", "flowable:read", "flowable:manage"],
            PermissionCatalog::WORKFLOW_DEFINITION_EDIT => &["flowable:manage"],
            PermissionCatalog::WORKFLOW_DEFINITION_PUBLISH => &["flowable:manage"],
            PermissionCatalog::WORKFLOW_DEFINITION_DEPRECATE => &["flowable:manage"],

            PermissionCatalog::AUTOMATION_NOTIFY_SEND => &["flowable:manage"],
            PermissionCatalog::AUTOMATION_DISPATCH_CREATE => &["flowable:manage", "dispatch:manage"],
            PermissionCatalog::AUTOMATION_BUSINESS_CASE_COMPLETE => &["flowable:manage"],
            PermissionCatalog::AUTOMATION_BUSINESS_CASE_FAIL => &["flowable:manage"],

            PermissionCatalog::DISPATCH_ORDER_READ => &["dispatch:view", "dispatch:manage"],
            PermissionCatalog::DISPATCH_ORDER_CREATE => &["dispatch:manage"],
            PermissionCatalog::DISPATCH_ORDER_UPDATE => &["dispatch:manage"],
            PermissionCatalog::DISPATCH_ORDER_PUBLISH => &["dispatch:manage"],
            PermissionCatalog::DISPATCH_ORDER_CANCEL => &["dispatch:manage"],

            PermissionCatalog::DISPATCH_CATALOG_READ => &[
                "dispatch:view",
                "dispatch:manage",
                "team:view",
                "equipment:view",
                "schedule:view",
            ],
            PermissionCatalog::DISPATCH_CATALOG_EDIT => {
                &["dispatch:manage", "team:manage", "equipment:manage", "schedule:manage"]
            }

            PermissionCatalog::SHIFT_HANDOVER_READ => &["dispatch:view", "dispatch:manage"],
            PermissionCatalog::SHIFT_HANDOVER_CREATE => &["dispatch:manage"],
            PermissionCatalog::SHIFT_HANDOVER_SUBMIT => &["dispatch:manage"],
            PermissionCatalog::SHIFT_HANDOVER_ACK => &["dispatch:manage"],

            PermissionCatalog::NOTIFICATION_READ => &["dispatch:view", "dispatch:manage"],
            PermissionCatalog::NOTIFICATION_SEND => &["dispatch:manage"],
            PermissionCatalog::NOTIFICATION_RECEIPT_READ => &["dispatch:view", "dispatch:manage"],
            PermissionCatalog::NOTIFICATION_RECEIPT_MANAGE => &["dispatch:manage"],

            PermissionCatalog::FLIGHT_READ => &["flight:read"],
            PermissionCatalog::FLIGHT_REPORT_GENERATE => &["flight:read"],

            PermissionCatalog::AUTH_ROLE_READ
            | PermissionCatalog::AUTH_PERMISSION_TEMPLATE_READ
            | PermissionCatalog::USER_ADMIN_READ => &["auth:view"],

            PermissionCatalog::AUTH_ROLE_EDIT
            | PermissionCatalog::AUTH_PERMISSION_TEMPLATE_EDIT
            | PermissionCatalog::USER_ADMIN_EDIT => &["auth:manage"],

            PermissionCatalog::SYSTEM_CONFIG_READ | PermissionCatalog::SYSTEM_CONFIG_WRITE => &["system:config"],
            PermissionCatalog::SYSTEM_OPS_ADMIN => &["system:admin"],
            _ => &[],
        }
    }
}

const AI_ACTION_PERMISSION_MAP: &[(&str, &[&str])] = &[
    ("Flight.get_context", &["flight:read"]),
    ("Flight.update_status", &["flight:write"]),
    ("Flight.add_note", &["flight:write"]),
    ("DispatchOrder.recommend_replan", &["dispatch:write"]),
    ("DispatchOrder.publish", &["dispatch:publish"]),
    ("Anomaly.acknowledge", &["anomaly:write"]),
    ("Anomaly.escalate", &["anomaly:write"]),
    ("BusinessCase.create", &["business_case:create"]),
    ("BusinessCase.close_case", &["business_case:update"]),
];

fn has_any_ai_permission(user_permissions: &[String], required_permissions: &[&str]) -> bool {
    user_permissions.iter().any(|permission| permission == "*")
        || required_permissions.iter().any(|required| {
            user_permissions
                .iter()
                .any(|permission| permission_matches_ai_grant(permission, required))
        })
}

fn permission_matches_ai_grant(permission: &str, required: &str) -> bool {
    permission == required
        || matches!(
            (permission, required),
            ("flight:write", "flight:read")
                | ("dispatch:manage", "dispatch:write")
                | ("dispatch:manage", "dispatch:publish")
                | ("dispatch:manage", "dispatch:admin")
                | ("automation:notify_send", "notification:send")
                | ("automation:dispatch_create", "dispatch:write")
                | ("business_case:update", "business_case:create")
                | ("flight.update", "flight:write")
                | ("flight.timeline_edit", "flight:write")
                | ("dispatch_order.update", "dispatch:write")
                | ("dispatch_order.publish", "dispatch:publish")
                | ("dispatch_order.update", "dispatch:admin")
                | ("business_case.create", "business_case:create")
                | ("business_case.update", "business_case:update")
                | ("notification.send", "notification:send")
                | ("todo.write", "todo:write")
        )
}

#[cfg(test)]
mod tests {
    use super::{AuthorizationService, PermissionCatalog};
    use crate::schemas::auth_schemas::TokenData;

    fn claims(permissions: &[&str]) -> TokenData {
        TokenData {
            sub: Some("user-1".to_string()),
            email: None,
            username: Some("tester".to_string()),
            token_kind: Some("access".to_string()),
            is_admin: Some(false),
            permissions: permissions.iter().map(|item| item.to_string()).collect(),
            department: Some("ops".to_string()),
            department_id: Some("ops-1".to_string()),
            pv: Some(1),
            iat: None,
            exp: None,
            iss: None,
            aud: None,
            ua_hash: None,
            ip_subnet_hash: None,
        }
    }

    #[test]
    fn flight_manage_does_not_expand_to_granular_permissions() {
        let token = claims(&["flight:manage"]);
        assert!(!AuthorizationService::has_grant(
            &token,
            PermissionCatalog::BUSINESS_CASE_UPDATE
        ));
        assert!(!AuthorizationService::has_grant(
            &token,
            PermissionCatalog::WORKFLOW_RUN_START
        ));
        assert!(!AuthorizationService::has_grant(
            &token,
            PermissionCatalog::FLIGHT_UPDATE
        ));
    }

    #[test]
    fn granular_flight_permissions_are_granted_directly() {
        let token = claims(&[
            PermissionCatalog::FLIGHT_UPDATE,
            PermissionCatalog::FLIGHT_TIMELINE_EDIT,
        ]);
        assert!(AuthorizationService::has_grant(
            &token,
            PermissionCatalog::FLIGHT_UPDATE
        ));
        assert!(AuthorizationService::has_grant(
            &token,
            PermissionCatalog::FLIGHT_TIMELINE_EDIT
        ));
    }

    #[test]
    fn dispatch_view_aliases_to_notification_read() {
        let token = claims(&["dispatch:view"]);
        assert!(AuthorizationService::has_grant(
            &token,
            PermissionCatalog::NOTIFICATION_READ
        ));
        assert!(!AuthorizationService::has_grant(
            &token,
            PermissionCatalog::NOTIFICATION_SEND
        ));
    }

    #[tokio::test]
    async fn get_allowed_ai_actions_maps_permissions_to_canonical_actions() {
        let svc = AuthorizationService;
        let flight_read = svc
            .get_allowed_ai_actions("flight_reader", &["flight:read".to_string()])
            .await
            .unwrap();
        let flight_write = svc
            .get_allowed_ai_actions("flight_writer", &["flight:write".to_string()])
            .await
            .unwrap();
        let admin = svc.get_allowed_ai_actions("admin", &["*".to_string()]).await.unwrap();
        let none = svc.get_allowed_ai_actions("none", &[]).await.unwrap();

        assert!(flight_read.contains(&"Flight.get_context".to_string()));
        assert!(!flight_read.contains(&"Flight.change_stand".to_string()));
        assert!(flight_write.contains(&"Flight.add_note".to_string()));
        assert!(admin.contains(&"DispatchOrder.recommend_replan".to_string()));
        assert!(!admin.contains(&"Stand.reserve".to_string()), "Stand.reserve 已废止");
        assert!(!admin.contains(&"Flight.change_stand".to_string()), "Flight.change_stand 已废止");
        assert!(none.is_empty(), "users without grants get no AI actions");
    }

    #[test]
    fn ai_action_grants_default_deny_empty_required_permissions() {
        assert!(!AuthorizationService::has_ai_action_grants(&["*".to_string()], &[]));
        assert!(!AuthorizationService::has_ai_action_grants(
            &["flight:manage".to_string()],
            &["flight:write".to_string()]
        ));
        assert!(AuthorizationService::has_ai_action_grants(
            &[PermissionCatalog::FLIGHT_UPDATE.to_string()],
            &["flight:write".to_string()]
        ));
        assert!(AuthorizationService::has_ai_supervisor_approval_grant(&[
            "system.ops_admin".to_string()
        ]));
    }

    #[test]
    fn legacy_aliases_has_retirement_date() {
        let source = include_str!("authorization_service.rs");
        let test_marker = "#[cfg(test)]";
        let main_code = &source[..source.find(test_marker).unwrap_or(source.len())];
        assert!(
            main_code.contains("2026-10-01") || main_code.contains("retire") || main_code.contains("removal"),
            "legacy_aliases should have a retirement date"
        );
    }
}
