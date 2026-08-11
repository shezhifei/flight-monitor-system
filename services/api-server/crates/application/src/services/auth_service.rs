//! 认证应用服务
//!
//! 编排用户认证、JWT 生成、密码验证、RBAC 管理。

use std::sync::Arc;

use std::collections::HashSet;

use chrono::{Duration, Utc};
use hmac::Mac;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::Department;
use fms_domain::models::session_runtime::{OnlineSessionStatus, SessionRuntimeStatus};
use fms_domain::models::user::{Role, User};
use fms_domain::ports::dispatch_repository::DepartmentRepository;
use fms_domain::ports::online_history_repository::OnlineHistoryRepository;
use fms_domain::ports::session_runtime_repository::SessionRuntimeRepository;
use fms_domain::ports::user_repository::{PermissionRepository, RoleRepository, UserRepository};

use crate::schemas::auth_schemas::{
    ChangePassword, PermissionResponse, RoleCreate, RoleResponse, RoleUpdate, Token, TokenData, UserAdminUpdate,
    UserCreate, UserLogin, UserResponse, UserRoleAssign,
};

/// JWT 配置
#[derive(Debug, Clone)]
pub struct JwtConfig {
    pub secret: String,
    pub access_token_expire_secs: i64,
    pub refresh_token_expire_secs: i64,
    pub sse_token_expire_secs: i64,
    pub issuer: String,
    pub audience: String,
}

impl Default for JwtConfig {
    /// Test-only default. In production, `resolve_jwt_secret()` in main.rs
    /// always overrides `secret` from the JWT_SECRET env var (panics if unset).
    fn default() -> Self {
        Self {
            secret: "change-me-in-production".to_string(),
            access_token_expire_secs: 3600,
            refresh_token_expire_secs: 604800,
            sse_token_expire_secs: 300,
            issuer: "fms".to_string(),
            audience: "flight-monitor-api".to_string(),
        }
    }
}

pub struct AuthService {
    user_repo: Arc<dyn UserRepository + Send + Sync>,
    role_repo: Arc<dyn RoleRepository + Send + Sync>,
    permission_repo: Arc<dyn PermissionRepository + Send + Sync>,
    department_repo: Arc<dyn DepartmentRepository + Send + Sync>,
    session_runtime: Arc<dyn SessionRuntimeRepository + Send + Sync>,
    online_history_repo: Arc<dyn OnlineHistoryRepository + Send + Sync>,
    jwt_config: JwtConfig,
}

pub struct DeleteRoleResult {
    pub found: bool,
    pub is_system: bool,
    pub deleted: bool,
    pub affected_users: i64,
}

impl AuthService {
    fn normalize_department_name(raw: Option<&str>) -> Option<String> {
        raw.map(str::trim).filter(|value| !value.is_empty()).map(str::to_string)
    }

    fn normalize_department_id(raw: Option<&str>) -> Option<String> {
        raw.map(str::trim).filter(|value| !value.is_empty()).map(str::to_string)
    }

    async fn sync_department_directory_for_user_change(
        &self,
        previous_department: Option<&str>,
        current_department: Option<&str>,
    ) -> Result<(), DomainError> {
        let previous_department = Self::normalize_department_name(previous_department);
        let current_department = Self::normalize_department_name(current_department);

        if let Some(current_department) = current_department.as_deref() {
            match self.department_repo.find_by_name(current_department).await? {
                Some(mut existing) => {
                    if !existing.is_active {
                        existing.is_active = true;
                        existing.updated_at = Some(Utc::now());
                        let _ = self.department_repo.save(&existing).await?;
                    }
                }
                None => {
                    let _ = self
                        .department_repo
                        .save(&Department {
                            id: ulid::Ulid::new().to_string(),
                            name: current_department.to_string(),
                            code: None,
                            description: None,
                            manager_id: None,
                            terminal: None,
                            created_at: Some(Utc::now()),
                            updated_at: Some(Utc::now()),
                            is_active: true,
                        })
                        .await?;
                }
            }
        }

        if let Some(previous_department) = previous_department.as_deref() {
            if current_department.as_deref() == Some(previous_department) {
                return Ok(());
            }
            let still_in_use = self
                .user_repo
                .list_distinct_departments_in_use()
                .await?
                .into_iter()
                .any(|value| value == previous_department);
            if still_in_use {
                return Ok(());
            }
            if let Some(existing) = self.department_repo.find_by_name(previous_department).await? {
                if self.user_repo.has_any_user_with_department_id(&existing.id).await? {
                    return Ok(());
                }
                if self.department_repo.has_dependencies(&existing.id).await? {
                    return Ok(());
                }
                let _ = self.department_repo.delete_permanently(&existing.id).await?;
            }
        }

        Ok(())
    }

    pub fn new(
        user_repo: Arc<dyn UserRepository + Send + Sync>,
        role_repo: Arc<dyn RoleRepository + Send + Sync>,
        permission_repo: Arc<dyn PermissionRepository + Send + Sync>,
        department_repo: Arc<dyn DepartmentRepository + Send + Sync>,
        session_runtime: Arc<dyn SessionRuntimeRepository + Send + Sync>,
        online_history_repo: Arc<dyn OnlineHistoryRepository + Send + Sync>,
        jwt_config: JwtConfig,
    ) -> Self {
        Self {
            user_repo,
            role_repo,
            permission_repo,
            department_repo,
            session_runtime,
            online_history_repo,
            jwt_config,
        }
    }

    async fn resolve_roles_by_name(&self, role_names: &[String]) -> Result<Vec<Role>, DomainError> {
        let mut resolved = Vec::with_capacity(role_names.len());
        for role_name in role_names {
            let normalized = role_name.trim();
            if normalized.is_empty() {
                continue;
            }
            let role = self
                .role_repo
                .find_by_name(normalized)
                .await?
                .ok_or_else(|| DomainError::ValidationError(format!("角色 {normalized} 不存在")))?;
            resolved.push(role);
        }
        Ok(resolved)
    }

    fn bump_permission_version(user: &mut User) {
        user.permission_version = user.permission_version.saturating_add(1).max(1);
    }

    /// 登录
    pub async fn login(
        &self,
        dto: UserLogin,
        client_ip: Option<&str>,
        user_agent_hash: Option<&str>,
        ip_subnet_hash: Option<&str>,
    ) -> Result<Token, DomainError> {
        let user = self
            .user_repo
            .find_by_username(&dto.username)
            .await?
            .ok_or_else(|| DomainError::Unauthorized("用户名或密码错误".into()))?;

        let password = dto.password.clone();
        let password_hash = user.password_hash.clone();
        let valid = tokio::task::spawn_blocking(move || bcrypt::verify(&password, &password_hash))
            .await
            .map_err(|_| DomainError::Internal("bcrypt task panicked".into()))?
            .map_err(|_| DomainError::Unauthorized("用户名或密码错误".into()))?;
        if !valid {
            return Err(DomainError::Unauthorized("用户名或密码错误".into()));
        }
        if !user.is_active {
            return Err(DomainError::Unauthorized("账号已停用".into()));
        }

        let _ = self.user_repo.update_last_login(&user.id).await;
        let token = self
            .generate_token_bundle(&user, true, user_agent_hash, ip_subnet_hash)
            .await?;
        if let Some(refresh_token) = token.refresh_token.as_deref() {
            let session = self
                .session_runtime
                .establish_session(&user.id, client_ip, Some(refresh_token))
                .await?;
            if session.created {
                if let Some(session_id) = session.session.session_id.as_deref() {
                    self.online_history_repo
                        .record_login(&user.id, session_id, client_ip, None)
                        .await?;
                }
            }
        }
        Ok(token)
    }

    /// 注册
    pub async fn register(&self, dto: UserCreate) -> Result<UserResponse, DomainError> {
        if dto
            .confirm_password
            .as_deref()
            .is_some_and(|value| value != dto.password)
        {
            return Err(DomainError::ValidationError("密码和确认密码不匹配".into()));
        }

        validate_password_strength(&dto.password)?;

        let normalized_email = dto.email.clone().unwrap_or_else(|| dto.username.clone());

        if self.user_repo.find_by_email(&normalized_email).await?.is_some() {
            return Err(DomainError::ValidationError("邮箱已被注册".into()));
        }
        if self.user_repo.find_by_username(&dto.username).await?.is_some() {
            return Err(DomainError::ValidationError("用户名已被使用".into()));
        }

        let assigned_roles = self.resolve_roles_by_name(dto.roles.as_deref().unwrap_or(&[])).await?;
        let pwd_for_hash = dto.password.clone();
        let password_hash = tokio::task::spawn_blocking(move || bcrypt::hash(&pwd_for_hash, 12))
            .await
            .map_err(|_| DomainError::Internal("bcrypt task panicked".into()))?
            .map_err(|e| DomainError::Internal(format!("密码哈希失败: {e}")))?;

        let now = Utc::now();
        let mut user = User {
            id: ulid::Ulid::new().to_string(),
            username: dto.username,
            email: normalized_email,
            password_hash,
            display_name: dto.display_name,
            roles: assigned_roles,
            created_at: now,
            updated_at: now,
            last_login_at: None,
            is_active: true,
            is_verified: true,
            is_admin: dto.is_admin,
            verification_token: None,
            verification_token_expires: None,
            verified_at: None,
            password_reset_token: None,
            password_reset_token_expires: None,
            password_changed_at: None,
            department: Self::normalize_department_name(dto.department.as_deref()),
            department_id: None,
            job_level: dto.job_level,
            job_title: dto.job_title,
            permission_version: 1,
        };

        self.user_repo.save(&user).await?;
        for role in &user.roles {
            self.role_repo.assign_role_to_user(&user.id, &role.id).await?;
        }
        self.sync_department_directory_for_user_change(None, user.department.as_deref())
            .await?;
        if let Some(department_name) = user.department.as_deref() {
            user.department_id = self
                .department_repo
                .find_by_name(department_name)
                .await?
                .map(|item| item.id);
            self.user_repo.update(&user).await?;
        }
        Ok(user_to_response(&user))
    }

    // ===== 用户管理 =====

    pub async fn list_users(&self) -> Result<Vec<UserResponse>, DomainError> {
        let users = self.user_repo.find_all(200, 0).await?;
        Ok(users.iter().map(user_to_list_response).collect())
    }

    pub async fn list_users_paginated(&self, page: i64, page_size: i64) -> Result<Vec<UserResponse>, DomainError> {
        let offset = (page - 1).max(0) * page_size;
        let users = self.user_repo.find_all(page_size, offset).await?;
        Ok(users.iter().map(user_to_list_response).collect())
    }

    pub async fn find_user_by_id(&self, user_id: &str) -> Result<Option<UserResponse>, DomainError> {
        let user = self.user_repo.find_by_id(user_id).await?;
        Ok(user.map(|u| user_to_response(&u)))
    }

    pub async fn update_user(&self, user_id: &str, dto: UserAdminUpdate) -> Result<Option<UserResponse>, DomainError> {
        let mut user = match self.user_repo.find_by_id(user_id).await? {
            Some(u) => u,
            None => return Ok(None),
        };
        let previous_department = user.department.clone();
        let mut permissions_changed = false;

        if let Some(uname) = dto.username {
            if uname != user.username && self.user_repo.find_by_username(&uname).await?.is_some() {
                return Err(DomainError::ValidationError("用户名已被使用".into()));
            }
            user.username = uname;
        }
        if let Some(email) = dto.email {
            if email != user.email && self.user_repo.find_by_email(&email).await?.is_some() {
                return Err(DomainError::ValidationError("邮箱已被使用".into()));
            }
            user.email = email;
        }
        if let Some(dn) = dto.display_name {
            let normalized = dn.trim();
            user.display_name = if normalized.is_empty() {
                None
            } else {
                Some(normalized.to_string())
            };
        }
        if let Some(active) = dto.is_active {
            if user.is_active != active {
                permissions_changed = true;
            }
            user.is_active = active;
        }
        if let Some(admin) = dto.is_admin {
            if user.is_admin != admin {
                permissions_changed = true;
            }
            user.is_admin = admin;
        }
        if let Some(role_names) = dto.roles {
            let next_roles = self.resolve_roles_by_name(&role_names).await?;
            let current_role_ids: HashSet<String> = user.roles.iter().map(|role| role.id.clone()).collect();
            let next_role_ids: HashSet<String> = next_roles.iter().map(|role| role.id.clone()).collect();

            for role in next_roles.iter().filter(|role| !current_role_ids.contains(&role.id)) {
                self.role_repo.assign_role_to_user(user_id, &role.id).await?;
            }

            for role in user.roles.iter().filter(|role| !next_role_ids.contains(&role.id)) {
                self.role_repo.remove_user_from_role(user_id, &role.id).await?;
            }

            if current_role_ids != next_role_ids {
                permissions_changed = true;
            }

            user.roles = next_roles;
        }
        if let Some(dept) = dto.department {
            let next_department = Self::normalize_department_name(Some(&dept));
            if user.department != next_department {
                permissions_changed = true;
            }
            user.department = next_department;
        }
        if let Some(jl) = dto.job_level {
            user.job_level = Some(jl);
        }
        if let Some(jt) = dto.job_title {
            user.job_title = Some(jt);
        }
        if let Some(pwd) = dto.password {
            validate_password_strength(&pwd)?;
            // Revoke existing sessions BEFORE persisting the new password so a
            // failed revoke cannot leave active sessions on a rotated secret.
            self.logout(user_id).await?;
            let pwd_clone = pwd.clone();
            user.password_hash = tokio::task::spawn_blocking(move || bcrypt::hash(&pwd_clone, 12))
                .await
                .map_err(|_| DomainError::Internal("bcrypt task panicked".into()))?
                .map_err(|e| DomainError::Internal(format!("密码哈希失败: {e}")))?;
            user.password_changed_at = Some(Utc::now());
        }
        if permissions_changed {
            Self::bump_permission_version(&mut user);
        }
        user.updated_at = Utc::now();
        self.user_repo.update(&user).await?;
        self.sync_department_directory_for_user_change(previous_department.as_deref(), user.department.as_deref())
            .await?;
        user.department_id = match user.department.as_deref() {
            Some(department_name) => self
                .department_repo
                .find_by_name(department_name)
                .await?
                .map(|item| item.id),
            None => None,
        };
        self.user_repo.update(&user).await?;
        Ok(Some(user_to_response(&user)))
    }

    pub async fn delete_user(&self, user_id: &str) -> Result<bool, DomainError> {
        let user = match self.user_repo.find_by_id(user_id).await? {
            Some(u) => u,
            None => return Ok(false),
        };
        let previous_department = user.department.clone();
        self.logout(user_id).await?;
        let deleted = self.user_repo.delete(user_id).await?;
        if !deleted {
            return Ok(false);
        }
        self.sync_department_directory_for_user_change(previous_department.as_deref(), None)
            .await?;
        Ok(true)
    }

    // ===== 角色管理 =====

    pub async fn list_roles(&self) -> Result<Vec<RoleResponse>, DomainError> {
        let roles = self.role_repo.find_all().await?;
        let mut result = Vec::with_capacity(roles.len());
        for role in &roles {
            let user_count = self.role_repo.count_users(&role.id).await?;
            result.push(role_to_response(role, user_count));
        }
        Ok(result)
    }

    pub async fn list_roles_paginated(&self, page: i64, page_size: i64) -> Result<Vec<RoleResponse>, DomainError> {
        let roles = self.list_roles().await?;
        let offset = ((page - 1).max(0) * page_size) as usize;
        let limit = page_size.max(1) as usize;
        Ok(roles.into_iter().skip(offset).take(limit).collect())
    }

    pub async fn create_role(&self, dto: RoleCreate) -> Result<RoleResponse, DomainError> {
        let now = Utc::now();
        let role = Role {
            id: ulid::Ulid::new().to_string(),
            name: dto.name,
            description: dto.description,
            permissions: dto.permissions.clone(),
            created_at: now,
            updated_at: now,
            is_active: true,
            is_system: false,
        };
        self.role_repo.save(&role).await?;
        self.role_repo.set_permissions(&role.id, &dto.permissions).await?;
        Ok(role_to_response(&role, 0))
    }

    pub async fn update_role(&self, role_id: &str, dto: RoleUpdate) -> Result<Option<RoleResponse>, DomainError> {
        let mut role = match self.role_repo.find_by_id(role_id).await? {
            Some(r) => r,
            None => return Ok(None),
        };
        if let Some(n) = dto.name {
            role.name = n;
        }
        if let Some(d) = dto.description {
            role.description = Some(d);
        }
        if let Some(is_active) = dto.is_active {
            role.is_active = is_active;
        }
        if let Some(perms) = dto.permissions {
            self.role_repo.set_permissions(role_id, &perms).await?;
            role.permissions = perms;
        }
        role.updated_at = Utc::now();
        self.role_repo.save(&role).await?;
        let user_count = self.role_repo.count_users(&role.id).await?;
        Ok(Some(role_to_response(&role, user_count)))
    }

    pub async fn delete_role(&self, role_id: &str) -> Result<DeleteRoleResult, DomainError> {
        let Some(role) = self.role_repo.find_by_id(role_id).await? else {
            return Ok(DeleteRoleResult {
                found: false,
                is_system: false,
                deleted: false,
                affected_users: 0,
            });
        };

        if role.is_system {
            return Ok(DeleteRoleResult {
                found: true,
                is_system: true,
                deleted: false,
                affected_users: 0,
            });
        }

        let affected_users = self.role_repo.count_users(role_id).await?;
        let deleted = self.role_repo.delete(role_id).await?;
        Ok(DeleteRoleResult {
            found: true,
            is_system: false,
            deleted,
            affected_users,
        })
    }

    // ===== 权限查询 =====

    pub async fn list_permissions(&self) -> Result<Vec<PermissionResponse>, DomainError> {
        let perms = self.permission_repo.find_all().await?;
        Ok(perms
            .iter()
            .map(|p| PermissionResponse {
                id: p.id.clone(),
                name: p.name.clone(),
                description: p.description.clone(),
                is_active: p.is_active,
                created_at: p.created_at,
                updated_at: p.updated_at,
            })
            .collect())
    }

    pub async fn list_permissions_paginated(
        &self,
        page: i64,
        page_size: i64,
    ) -> Result<Vec<PermissionResponse>, DomainError> {
        let perms = self.list_permissions().await?;
        let offset = ((page - 1).max(0) * page_size) as usize;
        let limit = page_size.max(1) as usize;
        Ok(perms.into_iter().skip(offset).take(limit).collect())
    }

    // ===== 用户角色分配 =====

    pub async fn assign_role(&self, dto: UserRoleAssign) -> Result<(), DomainError> {
        // 验证用户和角色存在
        self.user_repo
            .find_by_id(&dto.user_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "User",
                id: dto.user_id.clone(),
            })?;
        self.role_repo
            .find_by_id(&dto.role_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "Role",
                id: dto.role_id.clone(),
            })?;

        self.role_repo.assign_role_to_user(&dto.user_id, &dto.role_id).await
    }

    // ===== 角色权限管理 =====

    pub async fn add_permission_to_role(&self, role_id: &str, permission_name: &str) -> Result<bool, DomainError> {
        self.role_repo
            .find_by_id(role_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "Role",
                id: role_id.to_string(),
            })?;
        self.role_repo.add_permission(role_id, permission_name).await
    }

    pub async fn remove_permission_from_role(&self, role_id: &str, permission_name: &str) -> Result<bool, DomainError> {
        self.role_repo.remove_permission(role_id, permission_name).await
    }

    // ===== 修改密码 =====

    pub async fn change_password(&self, user_id: &str, dto: ChangePassword) -> Result<(), DomainError> {
        let mut user = self
            .user_repo
            .find_by_id(user_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "User",
                id: user_id.to_string(),
            })?;

        let old_password = dto.old_password.clone();
        let stored_hash = user.password_hash.clone();
        let valid = tokio::task::spawn_blocking(move || bcrypt::verify(&old_password, &stored_hash))
            .await
            .map_err(|_| DomainError::Internal("bcrypt task panicked".into()))?
            .map_err(|_| DomainError::Unauthorized("密码验证失败".into()))?;
        if !valid {
            return Err(DomainError::Unauthorized("旧密码不正确".into()));
        }
        if dto.new_password != dto.confirm_new_password {
            return Err(DomainError::ValidationError("新密码与确认密码不一致".into()));
        }

        validate_password_strength(&dto.new_password)?;

        // Revoke sessions first; propagate failure (never swallow).
        self.logout(user_id).await?;

        let new_pwd = dto.new_password.clone();
        user.password_hash = tokio::task::spawn_blocking(move || bcrypt::hash(&new_pwd, 12))
            .await
            .map_err(|_| DomainError::Internal("bcrypt task panicked".into()))?
            .map_err(|e| DomainError::Internal(format!("密码哈希失败: {e}")))?;
        user.password_changed_at = Some(Utc::now());
        user.updated_at = Utc::now();
        self.user_repo.update(&user).await?;
        Ok(())
    }

    // ===== 刷新令牌 =====

    pub async fn refresh_token(
        &self,
        _refresh_token: &str,
        user_agent_hash: Option<&str>,
        ip_subnet_hash: Option<&str>,
    ) -> Result<Token, DomainError> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;
        validation.set_audience(&[self.jwt_config.audience.as_str()]);
        validation.set_issuer(&[self.jwt_config.issuer.as_str()]);

        let decoded = decode::<TokenData>(
            _refresh_token,
            &DecodingKey::from_secret(self.jwt_config.secret.as_bytes()),
            &validation,
        )
        .map_err(|e| DomainError::Unauthorized(format!("刷新令牌无效: {e}")))?;

        if decoded.claims.token_kind.as_deref() != Some("refresh") {
            return Err(DomainError::Unauthorized("令牌类型不是 refresh".into()));
        }

        let user_id = decoded
            .claims
            .sub
            .as_deref()
            .ok_or_else(|| DomainError::Unauthorized("刷新令牌缺少 sub".into()))?;

        if let Some(expected_ua_hash) = decoded.claims.ua_hash.as_deref() {
            if Some(expected_ua_hash) != user_agent_hash {
                return Err(DomainError::Unauthorized("客户端环境已变化，请重新登录".into()));
            }
        }

        if let Some(expected_ip_subnet_hash) = decoded.claims.ip_subnet_hash.as_deref() {
            if Some(expected_ip_subnet_hash) != ip_subnet_hash {
                return Err(DomainError::Unauthorized("客户端网络环境已变化，请重新登录".into()));
            }
        }

        let user = self
            .user_repo
            .find_by_id(user_id)
            .await?
            .ok_or_else(|| DomainError::Unauthorized("用户不存在或已停用".into()))?;

        if !user.is_active {
            return Err(DomainError::Unauthorized("用户不存在或已停用".into()));
        }

        if !self
            .session_runtime
            .validate_refresh_token(user_id, _refresh_token)
            .await?
        {
            return Err(DomainError::Unauthorized("刷新令牌已失效或已被撤销".into()));
        }

        let _ = self.session_runtime.heartbeat(user_id).await?;
        self.generate_token_bundle(&user, false, user_agent_hash, ip_subnet_hash)
            .await
    }

    pub async fn issue_sse_token(
        &self,
        user_id: &str,
        user_agent_hash: Option<&str>,
        ip_subnet_hash: Option<&str>,
    ) -> Result<(String, i64), DomainError> {
        let normalized_user_id = user_id.trim();
        if normalized_user_id.is_empty() {
            return Err(DomainError::ValidationError("user_id is required".into()));
        }

        let user = self
            .user_repo
            .find_by_id(normalized_user_id)
            .await?
            .ok_or_else(|| DomainError::Unauthorized("用户不存在或已停用".into()))?;
        if !user.is_active {
            return Err(DomainError::Unauthorized("用户不存在或已停用".into()));
        }

        let permissions: Vec<String> = user.get_all_permissions().into_iter().collect();
        let permission_version = self.effective_permission_version(&user).await;
        let sse_token = self.sign_token(self.build_claims(
            &user,
            permissions,
            permission_version,
            Utc::now() + Duration::seconds(self.jwt_config.sse_token_expire_secs),
            "sse",
            user_agent_hash,
            ip_subnet_hash,
        ))?;
        Ok((sse_token, self.jwt_config.sse_token_expire_secs))
    }

    pub async fn validate_access_claims_freshness(&self, claims: &TokenData) -> Result<(), DomainError> {
        let user_id = claims
            .sub
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| DomainError::Unauthorized("JWT 缺少用户标识".into()))?;

        let user = self
            .user_repo
            .find_by_id(user_id)
            .await?
            .ok_or_else(|| DomainError::Unauthorized("用户不存在或令牌已失效".into()))?;

        if !user.is_active {
            return Err(DomainError::Unauthorized("账号已停用".into()));
        }

        if let (Some(iat), Some(password_changed_at)) = (claims.iat, user.password_changed_at) {
            if iat < password_changed_at.timestamp() {
                return Err(DomainError::Unauthorized("凭证已失效，请重新登录".into()));
            }
        }

        let token_permissions: HashSet<String> = claims
            .permissions
            .iter()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect();
        let current_permissions = user.get_all_permissions();
        if claims.is_admin.unwrap_or(false) != user.is_admin {
            return Err(DomainError::Unauthorized("权限已变更，请重新登录".into()));
        }
        let token_department = Self::normalize_department_name(claims.department.as_deref());
        let current_department = Self::normalize_department_name(user.department.as_deref());
        if token_department != current_department {
            return Err(DomainError::Unauthorized("部门归属已变更，请重新登录".into()));
        }
        let token_department_id = Self::normalize_department_id(claims.department_id.as_deref());
        let current_department_id = Self::normalize_department_id(user.department_id.as_deref());
        if token_department_id.is_some() && token_department_id != current_department_id {
            return Err(DomainError::Unauthorized("部门归属已变更，请重新登录".into()));
        }
        // Python auth may encode admin as permissions: ["*"]; skip strict set parity in that case.
        let wildcard_admin = token_permissions.len() == 1 && token_permissions.contains("*");
        if !wildcard_admin && token_permissions != current_permissions {
            return Err(DomainError::Unauthorized("权限已变更，请重新登录".into()));
        }

        Ok(())
    }

    /// 检查令牌的权限版本是否仍然有效
    ///
    /// 对应 Python 的 `is_permission_version_current_async`
    /// 用于防止用户权限变更后，旧 token 仍然有效的安全问题
    pub async fn is_permission_version_current_async(&self, user_id: &str, token_permission_version: i64) -> bool {
        if user_id.trim().is_empty() || token_permission_version < 1 {
            return false;
        }

        match self.user_repo.find_permission_version_by_id(user_id).await {
            Ok(Some(stored_permission_version)) => {
                let current = self
                    .effective_permission_version_for_user_id(user_id, stored_permission_version)
                    .await;
                token_permission_version == current
            }
            _ => false,
        }
    }

    pub async fn logout(&self, user_id: &str) -> Result<(), DomainError> {
        if let Some(status) = self.session_runtime.revoke_session(user_id, "manual_logout").await? {
            if let Some(session_id) = status.session_id.as_deref() {
                self.online_history_repo
                    .record_logout(user_id, session_id, false)
                    .await?;
            }
        } else {
            self.session_runtime.revoke_refresh_tokens(user_id).await?;
        }
        Ok(())
    }

    pub async fn heartbeat(&self, user_id: &str) -> Result<(), DomainError> {
        let _ = self.session_runtime.heartbeat(user_id).await?;
        Ok(())
    }

    pub async fn get_online_users(&self) -> Result<Vec<String>, DomainError> {
        self.session_runtime.get_online_users().await
    }

    pub async fn get_user_online_status(&self, user_id: &str) -> Result<OnlineSessionStatus, DomainError> {
        let status = self.session_runtime.get_online_status(user_id).await?;
        self.enrich_online_status(status).await
    }

    pub async fn get_all_online_users_status(&self) -> Result<Vec<OnlineSessionStatus>, DomainError> {
        let statuses = self.session_runtime.get_all_online_status().await?;
        let enriched: Vec<_> = statuses
            .into_iter()
            .map(|status| self.enrich_online_status(status))
            .collect();
        let enriched = futures_util::future::try_join_all(enriched).await?;
        Ok(enriched)
    }

    pub async fn get_session_runtime_status(&self) -> Result<SessionRuntimeStatus, DomainError> {
        self.session_runtime.get_runtime_status().await
    }

    pub async fn kick_user_session(&self, user_id: &str, reason: &str) -> Result<bool, DomainError> {
        let revoked = self.session_runtime.revoke_session(user_id, reason).await?;
        if let Some(status) = revoked {
            if let Some(session_id) = status.session_id.as_deref() {
                self.online_history_repo
                    .record_logout(user_id, session_id, true)
                    .await?;
            }
            return Ok(true);
        }
        Ok(false)
    }

    // -----------------------------------------------------------------------
    // 私有
    // -----------------------------------------------------------------------

    async fn generate_token_bundle(
        &self,
        user: &User,
        include_refresh_token: bool,
        user_agent_hash: Option<&str>,
        ip_subnet_hash: Option<&str>,
    ) -> Result<Token, DomainError> {
        let now = Utc::now();
        let all_perms: Vec<String> = user.get_all_permissions().into_iter().collect();
        let permission_version = self.effective_permission_version(user).await;
        let access_token = self.sign_token(self.build_claims(
            user,
            all_perms.clone(),
            permission_version,
            now + Duration::seconds(self.jwt_config.access_token_expire_secs),
            "access",
            user_agent_hash,
            ip_subnet_hash,
        ))?;
        let refresh_token = if include_refresh_token {
            Some(self.sign_token(self.build_claims(
                user,
                all_perms.clone(),
                permission_version,
                now + Duration::seconds(self.jwt_config.refresh_token_expire_secs),
                "refresh",
                user_agent_hash,
                ip_subnet_hash,
            ))?)
        } else {
            None
        };
        let sse_token = self.sign_token(self.build_claims(
            user,
            all_perms,
            permission_version,
            now + Duration::seconds(self.jwt_config.sse_token_expire_secs),
            "sse",
            user_agent_hash,
            ip_subnet_hash,
        ))?;

        let mut mac = hmac::Hmac::<sha2::Sha256>::new_from_slice(self.jwt_config.secret.as_bytes())
            .map_err(|_| DomainError::Internal("HMAC initialization failed".into()))?;
        hmac::Mac::update(&mut mac, access_token.as_bytes());
        let session_secret_bytes = hmac::Mac::finalize(mac).into_bytes();
        let session_secret = Some(hex::encode(session_secret_bytes));

        Ok(Token {
            access_token,
            token_type: "bearer".to_string(),
            expires_in: self.jwt_config.access_token_expire_secs,
            refresh_token,
            sse_token: Some(sse_token),
            sse_expires_in: Some(self.jwt_config.sse_token_expire_secs),
            session_secret,
        })
    }

    async fn effective_permission_version(&self, user: &User) -> i64 {
        self.effective_permission_version_for_user_id(&user.id, user.permission_version)
            .await
    }

    async fn effective_permission_version_for_user_id(&self, user_id: &str, stored_permission_version: i32) -> i64 {
        let runtime_version = self.session_runtime.get_permission_version(user_id).await.unwrap_or(1);
        runtime_version.max(i64::from(stored_permission_version)).max(1)
    }

    fn build_claims(
        &self,
        user: &User,
        permissions: Vec<String>,
        permission_version: i64,
        expires_at: chrono::DateTime<Utc>,
        token_kind: &str,
        user_agent_hash: Option<&str>,
        ip_subnet_hash: Option<&str>,
    ) -> TokenData {
        let now = Utc::now();
        TokenData {
            sub: Some(user.id.clone()),
            username: Some(user.username.clone()),
            email: Some(user.email.clone()),
            token_kind: Some(token_kind.to_string()),
            is_admin: Some(user.is_admin),
            permissions,
            department: user.department.clone(),
            department_id: user.department_id.clone(),
            pv: Some(permission_version.max(1)),
            iat: Some(now.timestamp()),
            exp: Some(expires_at.timestamp()),
            iss: Some(self.jwt_config.issuer.clone()),
            aud: Some(self.jwt_config.audience.clone()),
            ua_hash: user_agent_hash.map(str::to_string),
            ip_subnet_hash: ip_subnet_hash.map(str::to_string),
        }
    }

    fn sign_token(&self, claims: TokenData) -> Result<String, DomainError> {
        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.jwt_config.secret.as_bytes()),
        )
        .map_err(|e| DomainError::Internal(format!("JWT encode failed: {e}")))
    }

    async fn enrich_online_status(&self, mut status: OnlineSessionStatus) -> Result<OnlineSessionStatus, DomainError> {
        if let Some(user) = self.user_repo.find_by_id(&status.user_id).await? {
            status.username = Some(user.username);
            status.job_title = user.job_title;
            status.department = user.department;
        }
        Ok(status)
    }
}

// ---------------------------------------------------------------------------
// Mapper
// ---------------------------------------------------------------------------

fn user_to_response(u: &User) -> UserResponse {
    let role_names: Vec<String> = u.roles.iter().map(|r| r.name.clone()).collect();
    let all_perms: Vec<String> = u.get_all_permissions().into_iter().collect();

    UserResponse {
        id: u.id.clone(),
        username: u.username.clone(),
        email: u.email.clone(),
        is_active: u.is_active,
        is_verified: u.is_verified,
        is_admin: u.is_admin,
        created_at: u.created_at,
        last_login_at: u.last_login_at,
        roles: role_names,
        permissions: all_perms,
        display_name: u.display_name.clone(),
        effective_operator_name: None,
        effective_operator_label: None,
        operator_context_type: None,
        operator_context_id: None,
        department: u.department.clone(),
        job_level: u.job_level,
        job_title: u.job_title.clone(),
        permission_version: i64::from(u.permission_version),
    }
}

fn user_to_list_response(u: &User) -> UserResponse {
    let mut response = user_to_response(u);
    // Match the current Python /api/v2/auth/users list contract, which returns
    // role names but does not expand per-user permissions in the collection view.
    response.permissions.clear();
    response
}

fn role_to_response(r: &Role, user_count: i64) -> RoleResponse {
    RoleResponse {
        id: r.id.clone(),
        name: r.name.clone(),
        description: r.description.clone(),
        permissions: r.permissions.clone(),
        is_active: r.is_active,
        is_system: r.is_system,
        created_at: r.created_at,
        updated_at: r.updated_at,
        user_count,
    }
}

fn validate_password_strength(password: &str) -> Result<(), DomainError> {
    if password.len() < 8 {
        return Err(DomainError::ValidationError("密码长度必须至少为 8 位".into()));
    }
    let has_uppercase = password.chars().any(|c| c.is_ascii_uppercase());
    let has_lowercase = password.chars().any(|c| c.is_ascii_lowercase());
    let has_digit = password.chars().any(|c| c.is_ascii_digit());

    if !has_uppercase || !has_lowercase || !has_digit {
        return Err(DomainError::ValidationError(
            "密码必须包含大写字母、小写字母和数字".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_password_strength;
    use std::collections::HashMap;

    #[test]
    fn test_validate_password_strength() {
        // Valid passwords
        assert!(validate_password_strength("Abcdef12").is_ok());
        assert!(validate_password_strength("MyP@ssw0rd").is_ok());

        // Invalid: too short
        assert!(validate_password_strength("Ab1").is_err());

        // Invalid: missing uppercase
        assert!(validate_password_strength("abcdef12").is_err());

        // Invalid: missing lowercase
        assert!(validate_password_strength("ABCDEF12").is_err());

        // Invalid: missing digit
        assert!(validate_password_strength("Abcdefgh").is_err());
    }

    #[test]
    fn admin_password_reset_reuses_strength_rules() {
        // Admin resets must reject the same weak passwords as self-service change.
        assert!(validate_password_strength("password").is_err());
        assert!(validate_password_strength("12345678").is_err());
        assert!(validate_password_strength("AdminReset1").is_ok());
    }
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use chrono::Utc;

    use fms_domain::models::online_history::OnlineHistoryRecord;
    use fms_domain::models::session_runtime::{OnlineSessionStatus, SessionEstablishResult, SessionRuntimeStatus};
    use fms_domain::models::user::{Permission, Role};
    use fms_domain::ports::online_history_repository::OnlineHistoryRepository;

    use super::*;

    struct MockUserRepository {
        versions: HashMap<String, i32>,
        find_by_id_calls: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl UserRepository for MockUserRepository {
        async fn find_by_id(&self, _id: &str) -> Result<Option<User>, DomainError> {
            self.find_by_id_calls.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        }

        async fn find_permission_version_by_id(&self, id: &str) -> Result<Option<i32>, DomainError> {
            Ok(self.versions.get(id).copied())
        }

        async fn find_by_username(&self, _username: &str) -> Result<Option<User>, DomainError> {
            Ok(None)
        }

        async fn find_by_email(&self, _email: &str) -> Result<Option<User>, DomainError> {
            Ok(None)
        }

        async fn find_all(&self, _limit: i64, _offset: i64) -> Result<Vec<User>, DomainError> {
            Ok(Vec::new())
        }

        async fn list_distinct_departments_in_use(&self) -> Result<Vec<String>, DomainError> {
            Ok(Vec::new())
        }

        async fn has_any_user_with_department_id(&self, _department_id: &str) -> Result<bool, DomainError> {
            Ok(false)
        }

        async fn save(&self, _user: &User) -> Result<(), DomainError> {
            Ok(())
        }

        async fn update(&self, _user: &User) -> Result<bool, DomainError> {
            Ok(true)
        }

        async fn delete(&self, _id: &str) -> Result<bool, DomainError> {
            Ok(true)
        }

        async fn update_password(&self, _id: &str, _password_hash: &str) -> Result<bool, DomainError> {
            Ok(true)
        }

        async fn update_last_login(&self, _id: &str) -> Result<bool, DomainError> {
            Ok(true)
        }
    }

    struct MockRoleRepository;

    #[async_trait::async_trait]
    impl RoleRepository for MockRoleRepository {
        async fn find_by_id(&self, _id: &str) -> Result<Option<Role>, DomainError> {
            Ok(None)
        }

        async fn find_by_name(&self, _name: &str) -> Result<Option<Role>, DomainError> {
            Ok(None)
        }

        async fn find_all(&self) -> Result<Vec<Role>, DomainError> {
            Ok(Vec::new())
        }

        async fn save(&self, _role: &Role) -> Result<(), DomainError> {
            Ok(())
        }

        async fn delete(&self, _id: &str) -> Result<bool, DomainError> {
            Ok(false)
        }

        async fn find_by_user_id(&self, _user_id: &str) -> Result<Vec<Role>, DomainError> {
            Ok(Vec::new())
        }

        async fn count_users(&self, _role_id: &str) -> Result<i64, DomainError> {
            Ok(0)
        }

        async fn set_permissions(&self, _role_id: &str, _permission_names: &[String]) -> Result<(), DomainError> {
            Ok(())
        }

        async fn assign_role_to_user(&self, _user_id: &str, _role_id: &str) -> Result<(), DomainError> {
            Ok(())
        }

        async fn remove_user_from_role(&self, _user_id: &str, _role_id: &str) -> Result<(), DomainError> {
            Ok(())
        }

        async fn add_permission(&self, _role_id: &str, _permission_name: &str) -> Result<bool, DomainError> {
            Ok(true)
        }

        async fn remove_permission(&self, _role_id: &str, _permission_name: &str) -> Result<bool, DomainError> {
            Ok(true)
        }
    }

    struct MockPermissionRepository;

    #[async_trait::async_trait]
    impl PermissionRepository for MockPermissionRepository {
        async fn find_all(&self) -> Result<Vec<Permission>, DomainError> {
            Ok(Vec::new())
        }

        async fn find_by_role_id(&self, _role_id: &str) -> Result<Vec<Permission>, DomainError> {
            Ok(Vec::new())
        }
    }

    struct MockDepartmentRepository;

    #[async_trait::async_trait]
    impl DepartmentRepository for MockDepartmentRepository {
        async fn save(&self, dept: &Department) -> Result<Department, DomainError> {
            Ok(dept.clone())
        }

        async fn find_by_id(&self, _id: &str) -> Result<Option<Department>, DomainError> {
            Ok(None)
        }

        async fn find_by_name(&self, _name: &str) -> Result<Option<Department>, DomainError> {
            Ok(None)
        }

        async fn find_all(
            &self,
            _include_inactive: bool,
            _limit: i64,
            _offset: i64,
        ) -> Result<Vec<Department>, DomainError> {
            Ok(Vec::new())
        }

        async fn has_dependencies(&self, _department_id: &str) -> Result<bool, DomainError> {
            Ok(false)
        }

        async fn delete_permanently(&self, _department_id: &str) -> Result<bool, DomainError> {
            Ok(true)
        }
    }

    struct MockSessionRuntimeRepository {
        versions: HashMap<String, i64>,
    }

    #[async_trait::async_trait]
    impl SessionRuntimeRepository for MockSessionRuntimeRepository {
        async fn establish_session(
            &self,
            user_id: &str,
            _client_ip: Option<&str>,
            _refresh_token: Option<&str>,
        ) -> Result<SessionEstablishResult, DomainError> {
            Ok(SessionEstablishResult {
                session: offline_status(user_id),
                created: true,
            })
        }

        async fn validate_refresh_token(&self, _user_id: &str, _refresh_token: &str) -> Result<bool, DomainError> {
            Ok(false)
        }

        async fn revoke_refresh_tokens(&self, _user_id: &str) -> Result<(), DomainError> {
            Ok(())
        }

        async fn revoke_session(
            &self,
            _user_id: &str,
            _reason: &str,
        ) -> Result<Option<OnlineSessionStatus>, DomainError> {
            Ok(None)
        }

        async fn heartbeat(&self, _user_id: &str) -> Result<Option<OnlineSessionStatus>, DomainError> {
            Ok(None)
        }

        async fn get_online_users(&self) -> Result<Vec<String>, DomainError> {
            Ok(Vec::new())
        }

        async fn get_online_status(&self, user_id: &str) -> Result<OnlineSessionStatus, DomainError> {
            Ok(offline_status(user_id))
        }

        async fn get_all_online_status(&self) -> Result<Vec<OnlineSessionStatus>, DomainError> {
            Ok(Vec::new())
        }

        async fn get_runtime_status(&self) -> Result<SessionRuntimeStatus, DomainError> {
            Ok(SessionRuntimeStatus {
                mode: "memory".to_string(),
                fallback_since: None,
                fallback_duration_seconds: None,
                circuit_state: "closed".to_string(),
                redis_available: false,
            })
        }

        async fn get_permission_version(&self, user_id: &str) -> Result<i64, DomainError> {
            Ok(self.versions.get(user_id).copied().unwrap_or(1))
        }
    }

    struct MockOnlineHistoryRepository;

    #[async_trait::async_trait]
    impl OnlineHistoryRepository for MockOnlineHistoryRepository {
        async fn record_login(
            &self,
            _user_id: &str,
            _session_id: &str,
            _ip_address: Option<&str>,
            _device_info: Option<&str>,
        ) -> Result<(), DomainError> {
            Ok(())
        }

        async fn record_logout(&self, _user_id: &str, _session_id: &str, _forced: bool) -> Result<(), DomainError> {
            Ok(())
        }

        async fn list_history(
            &self,
            _user_id: Option<&str>,
            _start_date: Option<chrono::DateTime<Utc>>,
            _end_date: Option<chrono::DateTime<Utc>>,
            _limit: i64,
            _offset: i64,
        ) -> Result<Vec<OnlineHistoryRecord>, DomainError> {
            Ok(Vec::new())
        }

        async fn count_history(
            &self,
            _user_id: Option<&str>,
            _start_date: Option<chrono::DateTime<Utc>>,
            _end_date: Option<chrono::DateTime<Utc>>,
        ) -> Result<i64, DomainError> {
            Ok(0)
        }
    }

    fn offline_status(user_id: &str) -> OnlineSessionStatus {
        OnlineSessionStatus {
            user_id: user_id.to_string(),
            session_id: None,
            login_time: None,
            last_seen: None,
            status: "offline".to_string(),
            client_ip: None,
            username: None,
            job_title: None,
            department: None,
            forced_logout: false,
            kick_event: None,
        }
    }

    fn build_service(
        stored_versions: HashMap<String, i32>,
        runtime_versions: HashMap<String, i64>,
    ) -> (AuthService, Arc<AtomicUsize>) {
        let find_by_id_calls = Arc::new(AtomicUsize::new(0));
        let service = AuthService::new(
            Arc::new(MockUserRepository {
                versions: stored_versions,
                find_by_id_calls: Arc::clone(&find_by_id_calls),
            }),
            Arc::new(MockRoleRepository),
            Arc::new(MockPermissionRepository),
            Arc::new(MockDepartmentRepository),
            Arc::new(MockSessionRuntimeRepository {
                versions: runtime_versions,
            }),
            Arc::new(MockOnlineHistoryRepository),
            JwtConfig::default(),
        );
        (service, find_by_id_calls)
    }

    #[tokio::test]
    async fn permission_version_check_uses_lightweight_user_version_lookup() {
        let (service, find_by_id_calls) = build_service(
            HashMap::from([("user-1".to_string(), 3)]),
            HashMap::from([("user-1".to_string(), 2)]),
        );

        assert!(service.is_permission_version_current_async("user-1", 3).await);
        assert_eq!(find_by_id_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn permission_version_check_compares_against_runtime_version() {
        let (service, find_by_id_calls) = build_service(
            HashMap::from([("user-1".to_string(), 3)]),
            HashMap::from([("user-1".to_string(), 5)]),
        );

        assert!(!service.is_permission_version_current_async("user-1", 3).await);
        assert!(service.is_permission_version_current_async("user-1", 5).await);
        assert_eq!(find_by_id_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn permission_version_check_rejects_missing_user_and_invalid_token_version() {
        let (service, find_by_id_calls) = build_service(HashMap::from([("user-1".to_string(), 1)]), HashMap::new());

        assert!(!service.is_permission_version_current_async("missing-user", 1).await);
        assert!(!service.is_permission_version_current_async("user-1", 0).await);
        assert!(!service.is_permission_version_current_async("   ", 1).await);
        assert_eq!(find_by_id_calls.load(Ordering::SeqCst), 0);
    }
    /// In-memory user store used by password-reset revoke tests.
    struct PasswordUserRepo {
        user: std::sync::Mutex<Option<User>>,
        update_calls: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl UserRepository for PasswordUserRepo {
        async fn find_by_id(&self, id: &str) -> Result<Option<User>, DomainError> {
            let guard = self.user.lock().expect("lock");
            Ok(guard.as_ref().filter(|u| u.id == id).cloned())
        }
        async fn find_permission_version_by_id(&self, id: &str) -> Result<Option<i32>, DomainError> {
            Ok(self.find_by_id(id).await?.map(|u| u.permission_version))
        }
        async fn find_by_username(&self, username: &str) -> Result<Option<User>, DomainError> {
            let guard = self.user.lock().expect("lock");
            Ok(guard.as_ref().filter(|u| u.username == username).cloned())
        }
        async fn find_by_email(&self, _email: &str) -> Result<Option<User>, DomainError> {
            Ok(None)
        }
        async fn find_all(&self, _limit: i64, _offset: i64) -> Result<Vec<User>, DomainError> {
            Ok(Vec::new())
        }
        async fn list_distinct_departments_in_use(&self) -> Result<Vec<String>, DomainError> {
            Ok(Vec::new())
        }
        async fn has_any_user_with_department_id(&self, _department_id: &str) -> Result<bool, DomainError> {
            Ok(false)
        }
        async fn save(&self, _user: &User) -> Result<(), DomainError> {
            Ok(())
        }
        async fn update(&self, user: &User) -> Result<bool, DomainError> {
            self.update_calls.fetch_add(1, Ordering::SeqCst);
            let mut guard = self.user.lock().expect("lock");
            *guard = Some(user.clone());
            Ok(true)
        }
        async fn delete(&self, _id: &str) -> Result<bool, DomainError> {
            Ok(true)
        }
        async fn update_password(&self, _id: &str, _password_hash: &str) -> Result<bool, DomainError> {
            Ok(true)
        }
        async fn update_last_login(&self, _id: &str) -> Result<bool, DomainError> {
            Ok(true)
        }
    }

    struct TrackingSessionRuntime {
        revoke_calls: Arc<AtomicUsize>,
        fail_revoke: bool,
    }

    #[async_trait::async_trait]
    impl SessionRuntimeRepository for TrackingSessionRuntime {
        async fn establish_session(
            &self,
            user_id: &str,
            _client_ip: Option<&str>,
            _refresh_token: Option<&str>,
        ) -> Result<SessionEstablishResult, DomainError> {
            Ok(SessionEstablishResult {
                session: offline_status(user_id),
                created: true,
            })
        }
        async fn validate_refresh_token(&self, _user_id: &str, _refresh_token: &str) -> Result<bool, DomainError> {
            Ok(false)
        }
        async fn revoke_refresh_tokens(&self, _user_id: &str) -> Result<(), DomainError> {
            self.revoke_calls.fetch_add(1, Ordering::SeqCst);
            if self.fail_revoke {
                return Err(DomainError::Internal("revoke failed".into()));
            }
            Ok(())
        }
        async fn revoke_session(
            &self,
            _user_id: &str,
            _reason: &str,
        ) -> Result<Option<OnlineSessionStatus>, DomainError> {
            // Returning None forces logout() to call revoke_refresh_tokens.
            Ok(None)
        }
        async fn heartbeat(&self, _user_id: &str) -> Result<Option<OnlineSessionStatus>, DomainError> {
            Ok(None)
        }
        async fn get_online_users(&self) -> Result<Vec<String>, DomainError> {
            Ok(Vec::new())
        }
        async fn get_online_status(&self, user_id: &str) -> Result<OnlineSessionStatus, DomainError> {
            Ok(offline_status(user_id))
        }
        async fn get_all_online_status(&self) -> Result<Vec<OnlineSessionStatus>, DomainError> {
            Ok(Vec::new())
        }
        async fn get_runtime_status(&self) -> Result<SessionRuntimeStatus, DomainError> {
            Ok(SessionRuntimeStatus {
                mode: "memory".to_string(),
                fallback_since: None,
                fallback_duration_seconds: None,
                circuit_state: "closed".to_string(),
                redis_available: false,
            })
        }
        async fn get_permission_version(&self, _user_id: &str) -> Result<i64, DomainError> {
            Ok(1)
        }
    }

    fn sample_user(password_plain: &str) -> User {
        let hash = bcrypt::hash(password_plain, 4).expect("hash");
        User {
            id: "user-1".into(),
            email: "u@example.com".into(),
            password_hash: hash,
            username: "alice".into(),
            display_name: Some("Alice".into()),
            roles: Vec::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_login_at: None,
            is_active: true,
            is_verified: true,
            is_admin: false,
            verification_token: None,
            verification_token_expires: None,
            verified_at: None,
            password_reset_token: None,
            password_reset_token_expires: None,
            password_changed_at: None,
            department: None,
            department_id: None,
            job_level: Some(1),
            job_title: None,
            permission_version: 1,
        }
    }

    fn build_password_service(user: User, fail_revoke: bool) -> (AuthService, Arc<AtomicUsize>, Arc<AtomicUsize>) {
        let revoke_calls = Arc::new(AtomicUsize::new(0));
        let update_calls = Arc::new(AtomicUsize::new(0));
        let user_repo = Arc::new(PasswordUserRepo {
            user: std::sync::Mutex::new(Some(user)),
            update_calls: update_calls.clone(),
        });
        let session = Arc::new(TrackingSessionRuntime {
            revoke_calls: revoke_calls.clone(),
            fail_revoke,
        });
        let service = AuthService::new(
            user_repo,
            Arc::new(MockRoleRepository),
            Arc::new(MockPermissionRepository),
            Arc::new(MockDepartmentRepository),
            session,
            Arc::new(MockOnlineHistoryRepository),
            JwtConfig::default(),
        );
        (service, revoke_calls, update_calls)
    }

    #[tokio::test]
    async fn change_password_revokes_sessions_before_persist() {
        let (service, revoke_calls, update_calls) = build_password_service(sample_user("OldPass12"), false);
        service
            .change_password(
                "user-1",
                ChangePassword {
                    old_password: "OldPass12".into(),
                    new_password: "NewPass34".into(),
                    confirm_new_password: "NewPass34".into(),
                },
            )
            .await
            .expect("change password");
        assert!(revoke_calls.load(Ordering::SeqCst) >= 1, "logout/revoke must be called");
        assert!(
            update_calls.load(Ordering::SeqCst) >= 1,
            "password must be persisted after revoke"
        );
    }

    #[tokio::test]
    async fn change_password_propagates_revoke_failure_and_skips_persist() {
        let (service, revoke_calls, update_calls) = build_password_service(sample_user("OldPass12"), true);
        let err = service
            .change_password(
                "user-1",
                ChangePassword {
                    old_password: "OldPass12".into(),
                    new_password: "NewPass34".into(),
                    confirm_new_password: "NewPass34".into(),
                },
            )
            .await
            .expect_err("revoke failure must surface");
        assert!(matches!(err, DomainError::Internal(_)));
        assert_eq!(revoke_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            update_calls.load(Ordering::SeqCst),
            0,
            "must not persist new password when revoke fails"
        );
    }

    #[tokio::test]
    async fn admin_password_reset_revokes_sessions_and_propagates_failure() {
        let (service, revoke_calls, update_calls) = build_password_service(sample_user("OldPass12"), true);
        let err = service
            .update_user(
                "user-1",
                UserAdminUpdate {
                    password: Some("AdminNew9".into()),
                    ..Default::default()
                },
            )
            .await
            .expect_err("admin reset must fail when revoke fails");
        assert!(matches!(err, DomainError::Internal(_)));
        assert_eq!(revoke_calls.load(Ordering::SeqCst), 1);
        assert_eq!(update_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn admin_password_reset_calls_revoke_on_success() {
        let (service, revoke_calls, update_calls) = build_password_service(sample_user("OldPass12"), false);
        let updated = service
            .update_user(
                "user-1",
                UserAdminUpdate {
                    password: Some("AdminNew9".into()),
                    ..Default::default()
                },
            )
            .await
            .expect("admin reset")
            .expect("user exists");
        assert_eq!(updated.username, "alice");
        assert!(revoke_calls.load(Ordering::SeqCst) >= 1);
        assert!(update_calls.load(Ordering::SeqCst) >= 1);
    }
}
