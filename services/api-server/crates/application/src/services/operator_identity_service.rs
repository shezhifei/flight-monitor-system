//! 操作员身份解析服务

use std::sync::Arc;

use chrono::Utc;

use fms_domain::error::DomainError;
use fms_domain::models::operator_identity::OperatorIdentityContext;
use fms_domain::ports::operator_identity_repository::OperatorIdentityRepository;

use crate::schemas::auth_schemas::UserResponse;

pub struct OperatorIdentityService {
    repo: Arc<dyn OperatorIdentityRepository + Send + Sync>,
}

impl OperatorIdentityService {
    pub fn new(repo: Arc<dyn OperatorIdentityRepository + Send + Sync>) -> Self {
        Self { repo }
    }

    pub fn normalize_context(
        &self,
        context_type: Option<&str>,
        context_id: Option<&str>,
    ) -> Result<(Option<String>, Option<String>), DomainError> {
        let normalized_type = normalize_optional_lower(context_type);
        let normalized_id = normalize_optional(context_id);

        if normalized_type.is_none() && normalized_id.is_none() {
            return Ok((None, None));
        }
        if normalized_type.is_none() || normalized_id.is_none() {
            return Err(DomainError::ValidationError(
                "operator context headers must include both type and id".into(),
            ));
        }

        match normalized_type.as_deref() {
            Some("mobile_device") | Some("web_client") => Ok((normalized_type, normalized_id)),
            _ => Err(DomainError::ValidationError("unsupported operator context type".into())),
        }
    }

    pub async fn enrich_user_response(
        &self,
        mut user: UserResponse,
        context_type: Option<&str>,
        context_id: Option<&str>,
    ) -> Result<UserResponse, DomainError> {
        let (context_type, context_id) = self.normalize_context(context_type, context_id)?;
        let mut operator_name = user
            .display_name
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| user.username.clone());

        if let (Some(context_type), Some(context_id)) = (context_type.as_deref(), context_id.as_deref()) {
            if let Some(context) = self.repo.find_by_scope(&user.id, context_type, context_id).await? {
                let scoped_name = context.operator_name.trim();
                if !scoped_name.is_empty() {
                    operator_name = scoped_name.to_string();
                }
            }
        }

        let job_title = resolve_job_title(&user);
        user.effective_operator_name = Some(operator_name.clone());
        user.effective_operator_label = compose_operator_label(&operator_name, &job_title);
        user.operator_context_type = context_type;
        user.operator_context_id = context_id;
        Ok(user)
    }

    pub async fn update_operator_context(
        &self,
        user: UserResponse,
        context_type: &str,
        context_id: &str,
        operator_name: Option<&str>,
    ) -> Result<UserResponse, DomainError> {
        let (context_type, context_id) = self.normalize_context(Some(context_type), Some(context_id))?;
        let context_type = context_type.expect("validated context_type");
        let context_id = context_id.expect("validated context_id");

        if let Some(operator_name) = normalize_optional(operator_name) {
            self.repo
                .upsert(&OperatorIdentityContext {
                    user_id: user.id.clone(),
                    context_type: context_type.clone(),
                    context_id: context_id.clone(),
                    operator_name,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                })
                .await?;
        } else {
            self.repo.delete(&user.id, &context_type, &context_id).await?;
        }

        self.enrich_user_response(user, Some(&context_type), Some(&context_id))
            .await
    }
}

fn normalize_optional(value: Option<&str>) -> Option<String> {
    let normalized = value.unwrap_or("").trim();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized.to_string())
    }
}

fn normalize_optional_lower(value: Option<&str>) -> Option<String> {
    normalize_optional(value).map(|value| value.to_ascii_lowercase())
}

fn resolve_job_title(user: &UserResponse) -> String {
    if let Some(job_title) = user.job_title.as_deref() {
        let normalized = job_title.trim();
        if !normalized.is_empty() {
            return normalized.to_string();
        }
    }
    if let Some(role) = user.roles.first() {
        let normalized = role.trim();
        if !normalized.is_empty() {
            return normalized.to_string();
        }
    }
    if user.is_admin {
        return "admin".to_string();
    }
    "用户".to_string()
}

fn compose_operator_label(operator_name: &str, job_title: &str) -> Option<String> {
    let normalized_name = operator_name.trim();
    let normalized_job_title = job_title.trim();
    if normalized_name.is_empty() && normalized_job_title.is_empty() {
        return None;
    }
    if normalized_name.is_empty() {
        return Some(normalized_job_title.to_string());
    }
    if normalized_job_title.is_empty() {
        return Some(normalized_name.to_string());
    }
    Some(format!("{normalized_name}-{normalized_job_title}"))
}
