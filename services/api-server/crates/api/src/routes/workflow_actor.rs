use crate::error::ApiError;
use crate::middleware::jwt::JwtAuth;
use fms_application::schemas::auth_schemas::UserResponse;
use fms_application::services::auth_service::AuthService;
use fms_application::services::business_case_workflow_service::WorkflowActor;
use fms_application::services::operator_identity_service::OperatorIdentityService;

pub(crate) async fn resolve_workflow_actor(
    claims: &JwtAuth,
    auth_svc: Option<&AuthService>,
    operator_identity_svc: Option<&OperatorIdentityService>,
    context_type: Option<String>,
    context_id: Option<String>,
) -> Result<WorkflowActor, ApiError> {
    let enriched_user = match (
        auth_svc,
        operator_identity_svc,
        claims.0.sub.as_deref().map(str::trim).filter(|value| !value.is_empty()),
    ) {
        (Some(auth_svc), Some(operator_identity_svc), Some(user_id)) => {
            if let Some(user) = auth_svc.find_user_by_id(user_id).await? {
                Some(
                    operator_identity_svc
                        .enrich_user_response(user, context_type.as_deref(), context_id.as_deref())
                        .await
                        .map_err(ApiError::from)?,
                )
            } else {
                None
            }
        }
        _ => None,
    };

    Ok(build_workflow_actor(
        claims,
        enriched_user.as_ref(),
        context_type,
        context_id,
    ))
}

pub(crate) fn build_workflow_actor(
    claims: &JwtAuth,
    enriched_user: Option<&UserResponse>,
    context_type: Option<String>,
    context_id: Option<String>,
) -> WorkflowActor {
    let user_id = claims
        .0
        .sub
        .as_deref()
        .and_then(non_empty)
        .map(ToOwned::to_owned)
        .or_else(|| enriched_user.and_then(|user| non_empty_owned(Some(user.id.clone()))));
    let username = claims
        .0
        .username
        .as_deref()
        .and_then(non_empty)
        .map(ToOwned::to_owned)
        .or_else(|| enriched_user.and_then(|user| non_empty_owned(Some(user.username.clone()))));
    let name_snapshot = enriched_user
        .and_then(preferred_operator_name)
        .or_else(|| username.clone());
    let actor = enriched_user
        .and_then(|user| non_empty_owned(user.effective_operator_label.clone()))
        .or_else(|| name_snapshot.clone())
        .or_else(|| username.clone())
        .or_else(|| user_id.clone())
        .unwrap_or_else(|| "system".to_string());

    WorkflowActor {
        actor,
        user_id,
        username,
        name_snapshot,
        context_type,
        context_id,
    }
}

fn preferred_operator_name(user: &UserResponse) -> Option<String> {
    non_empty_owned(user.effective_operator_name.clone())
        .or_else(|| non_empty_owned(user.display_name.clone()))
        .or_else(|| non_empty_owned(Some(user.username.clone())))
}

fn non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn non_empty_owned(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::build_workflow_actor;
    use crate::middleware::jwt::JwtAuth;
    use chrono::Utc;
    use fms_application::schemas::auth_schemas::TokenData;
    use fms_application::schemas::auth_schemas::UserResponse;

    fn claims(username: Option<&str>) -> JwtAuth {
        JwtAuth(TokenData {
            sub: Some("user-1".to_string()),
            email: None,
            username: username.map(ToOwned::to_owned),
            token_kind: Some("access".to_string()),
            is_admin: Some(false),
            permissions: vec![],
            department: None,
            department_id: None,
            pv: Some(1),
            iat: None,
            exp: None,
            iss: None,
            aud: None,
            ua_hash: None,
            ip_subnet_hash: None,
        })
    }

    fn user_response() -> UserResponse {
        UserResponse {
            id: "user-1".to_string(),
            username: "dispatcher".to_string(),
            email: "dispatcher@example.com".to_string(),
            is_active: true,
            is_verified: true,
            is_admin: false,
            created_at: Utc::now(),
            last_login_at: None,
            roles: vec!["dispatcher".to_string()],
            permissions: vec![],
            display_name: Some("调度员甲".to_string()),
            effective_operator_name: Some("当前值班调度".to_string()),
            effective_operator_label: Some("当前值班调度-dispatcher".to_string()),
            operator_context_type: Some("web_client".to_string()),
            operator_context_id: Some("console-1".to_string()),
            department: Some("ops".to_string()),
            job_level: Some(1),
            job_title: Some("dispatcher".to_string()),
            permission_version: 1,
            account_type: "personal".to_string(),
            login_enabled: true,
            current_occupant_user_id: None,
        }
    }

    #[test]
    fn workflow_actor_prefers_effective_operator_identity() {
        let actor = build_workflow_actor(
            &claims(Some("dispatcher")),
            Some(&user_response()),
            Some("web_client".to_string()),
            Some("console-1".to_string()),
        );

        assert_eq!(actor.user_id.as_deref(), Some("user-1"));
        assert_eq!(actor.username.as_deref(), Some("dispatcher"));
        assert_eq!(actor.name_snapshot.as_deref(), Some("当前值班调度"));
        assert_eq!(actor.actor, "当前值班调度-dispatcher");
        assert_eq!(actor.context_type.as_deref(), Some("web_client"));
        assert_eq!(actor.context_id.as_deref(), Some("console-1"));
    }

    #[test]
    fn workflow_actor_falls_back_to_claims_when_no_enriched_user() {
        let actor = build_workflow_actor(&claims(Some("dispatcher")), None, None, None);

        assert_eq!(actor.user_id.as_deref(), Some("user-1"));
        assert_eq!(actor.username.as_deref(), Some("dispatcher"));
        assert_eq!(actor.name_snapshot.as_deref(), Some("dispatcher"));
        assert_eq!(actor.actor, "dispatcher");
    }
}
