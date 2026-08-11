//! 鉴权领域服务装配：auth / auth_admin / online_history / online_status /
//! operator_identity，以及登录限流器与鉴权校验缓存。

use std::sync::Arc;

use crate::di::types::*;

use fms_application::services::auth_admin_service::{AuthAdminCommandService, AuthAdminQueryService};
use fms_application::services::auth_service::{AuthService, JwtConfig};
use fms_application::services::online_history_service::OnlineHistoryService;
use fms_application::services::online_status_service::OnlineStatusService;
use fms_application::services::operator_identity_service::OperatorIdentityService;

use crate::di::shared::{SharedInfra, SharedRepos};

pub(crate) struct AuthServices {
    pub auth_svc: Arc<ConcreteAuthService>,
    pub auth_admin_query_svc: Arc<AuthAdminQueryService>,
    pub auth_admin_command_svc: Arc<AuthAdminCommandService>,
    pub online_history_svc: Arc<OnlineHistoryService>,
    pub online_status_svc: Arc<OnlineStatusService>,
    pub operator_identity_svc: Arc<OperatorIdentityService>,
    pub login_failure_limiter: Arc<fms_api::routes::auth::LoginFailureRateLimiter>,
    pub auth_validation_cache: Arc<fms_application::services::auth_validation_cache::AuthValidationCache>,
}

pub(crate) fn build_auth_services(repos: &SharedRepos, _infra: &SharedInfra, jwt_config: JwtConfig) -> AuthServices {
    let auth_svc = Arc::new(AuthService::new(
        repos.auth_user_repo.clone(),
        repos.auth_role_repo.clone(),
        repos.permission_repo.clone(),
        repos.department_repo.clone(),
        repos.session_runtime_repo.clone(),
        repos.online_history_repo.clone(),
        jwt_config,
    ));
    let auth_admin_query_svc = Arc::new(AuthAdminQueryService::new(
        repos.permission_template_repo.clone(),
        repos.auth_user_repo.clone(),
        repos.department_repo.clone(),
    ));
    let auth_admin_command_svc = Arc::new(AuthAdminCommandService::new(
        repos.permission_template_repo.clone(),
        repos.auth_role_repo.clone(),
    ));
    let online_history_svc = Arc::new(OnlineHistoryService::new(repos.online_history_repo.clone()));
    let online_status_svc = Arc::new(OnlineStatusService::new(
        repos.auth_user_repo.clone(),
        repos.session_runtime_repo.clone(),
        repos.online_history_repo.clone(),
    ));
    let operator_identity_svc = Arc::new(OperatorIdentityService::new(repos.operator_identity_repo.clone()));

    let login_failure_limiter = Arc::new(fms_api::routes::auth::LoginFailureRateLimiter::default());
    let auth_validation_cache = Arc::new(fms_application::services::auth_validation_cache::AuthValidationCache::new());

    AuthServices {
        auth_svc,
        auth_admin_query_svc,
        auth_admin_command_svc,
        online_history_svc,
        online_status_svc,
        operator_identity_svc,
        login_failure_limiter,
        auth_validation_cache,
    }
}
