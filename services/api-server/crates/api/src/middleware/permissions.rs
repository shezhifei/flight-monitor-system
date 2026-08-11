//! 共享权限检查模块
//!
//! 提供 `PermissionCheck` trait，为 `JwtAuth` 统一实现权限判断逻辑。
//! 路由 handler 通过 `claims.ensure_permission("ai:view")?` 调用，
//! 无需在每个路由文件中重复定义 `ensure_permission` / `has_permission`。

use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use fms_application::services::authorization_service::AuthorizationService;

pub trait PermissionCheck {
    fn has_permission(&self, permission: &str) -> bool;
    fn ensure_permission(&self, permission: &str) -> Result<(), ApiError>;
    fn ensure_authenticated(&self) -> Result<(), ApiError>;
    fn has_resource_wildcard(&self, permission: &str) -> bool;
    fn has_grant(&self, permission: &str) -> bool;
    fn ensure_grant(&self, permission: &str) -> Result<(), ApiError>;
    fn viewer_department_id(&self) -> Option<&str>;
    fn viewer_department_name(&self) -> Option<&str>;
}

impl PermissionCheck for JwtAuth {
    fn has_permission(&self, permission: &str) -> bool {
        if AuthorizationService::has_grant(&self.0, permission) {
            return true;
        }

        if let Some((resource, _)) = permission.split_once(':') {
            let wildcard = format!("{resource}:*");
            if AuthorizationService::has_grant(&self.0, &wildcard) {
                return true;
            }
        }

        false
    }

    fn ensure_permission(&self, permission: &str) -> Result<(), ApiError> {
        if self.has_permission(permission) {
            Ok(())
        } else {
            Err(ApiError::Forbidden(format!("缺少权限: {permission}")))
        }
    }

    fn ensure_authenticated(&self) -> Result<(), ApiError> {
        if AuthorizationService::is_authenticated(&self.0) {
            Ok(())
        } else {
            Err(ApiError::Unauthorized("未认证".into()))
        }
    }

    fn has_resource_wildcard(&self, permission: &str) -> bool {
        permission
            .split_once('.')
            .map(|(resource, _)| format!("{resource}.*"))
            .map(|wildcard| self.0.permissions.iter().any(|item| item == &wildcard))
            .unwrap_or(false)
    }

    fn has_grant(&self, permission: &str) -> bool {
        AuthorizationService::has_grant(&self.0, permission) || self.has_resource_wildcard(permission)
    }

    fn ensure_grant(&self, permission: &str) -> Result<(), ApiError> {
        if self.has_grant(permission) {
            Ok(())
        } else {
            Err(ApiError::Forbidden(format!("缺少权限: {permission}")))
        }
    }

    fn viewer_department_id(&self) -> Option<&str> {
        AuthorizationService::department_id(&self.0)
    }

    fn viewer_department_name(&self) -> Option<&str> {
        AuthorizationService::department_name(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fms_application::schemas::auth_schemas::TokenData;

    fn make_claims(permissions: Vec<&str>, is_admin: bool) -> JwtAuth {
        JwtAuth(TokenData {
            sub: Some("test-user".to_string()),
            email: None,
            username: Some("test".to_string()),
            token_kind: None,
            is_admin: Some(is_admin),
            permissions: permissions.into_iter().map(String::from).collect(),
            department: None,
            department_id: None,
            pv: None,
            iat: None,
            exp: None,
            iss: None,
            aud: None,
            ua_hash: None,
            ip_subnet_hash: None,
        })
    }

    #[test]
    fn test_has_permission_matches_exact() {
        let claims = make_claims(vec!["ai:view"], false);
        assert!(claims.has_permission("ai:view"));
        assert!(!claims.has_permission("ai:execute"));
    }

    #[test]
    fn test_has_permission_wildcard() {
        let claims = make_claims(vec!["*"], false);
        assert!(claims.has_permission("ai:view"));
        assert!(claims.has_permission("flights:edit"));
    }

    #[test]
    fn test_has_permission_admin_bypass() {
        let claims = make_claims(vec![], true);
        assert!(claims.has_permission("anything:atall"));
    }

    #[test]
    fn test_ensure_permission_ok() {
        let claims = make_claims(vec!["ai:view"], false);
        assert!(claims.ensure_permission("ai:view").is_ok());
    }

    #[test]
    fn test_ensure_permission_forbidden() {
        let claims = make_claims(vec![], false);
        let result = claims.ensure_permission("ai:view");
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("缺少权限"));
    }

    #[test]
    fn test_has_resource_wildcard_dot_permission() {
        let claims = make_claims(vec!["business_case.*"], false);
        assert!(claims.has_resource_wildcard("business_case.create"));
        assert!(!claims.has_grant("ai:media"));
        assert!(claims.has_grant("business_case.create"));
    }
}
