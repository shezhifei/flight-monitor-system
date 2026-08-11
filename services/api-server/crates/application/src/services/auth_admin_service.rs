//! 认证管理应用服务

use std::sync::Arc;

use chrono::Utc;

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::Department;
use fms_domain::models::permission_template::PermissionTemplate;
use fms_domain::ports::dispatch_repository::DepartmentRepository;
use fms_domain::ports::permission_template_repository::PermissionTemplateRepository;
use fms_domain::ports::user_repository::{RoleRepository, UserRepository};

use crate::schemas::permission_template_schemas::{
    PermissionTemplateCreate, PermissionTemplateResponse, PermissionTemplateUpdate,
};

pub struct AuthAdminQueryService {
    permission_template_repo: Arc<dyn PermissionTemplateRepository + Send + Sync>,
    user_repo: Arc<dyn UserRepository + Send + Sync>,
    department_repo: Arc<dyn DepartmentRepository + Send + Sync>,
}

impl AuthAdminQueryService {
    pub fn new(
        permission_template_repo: Arc<dyn PermissionTemplateRepository + Send + Sync>,
        user_repo: Arc<dyn UserRepository + Send + Sync>,
        department_repo: Arc<dyn DepartmentRepository + Send + Sync>,
    ) -> Self {
        Self {
            permission_template_repo,
            user_repo,
            department_repo,
        }
    }

    pub async fn list_permission_templates(
        &self,
        category: Option<&str>,
        include_inactive: bool,
        page: i64,
        page_size: i64,
    ) -> Result<Vec<PermissionTemplateResponse>, DomainError> {
        let safe_page = page.max(1);
        let safe_page_size = page_size.clamp(1, 500);
        let offset = (safe_page - 1) * safe_page_size;
        let templates = self
            .permission_template_repo
            .find_all(category, include_inactive, safe_page_size, offset)
            .await?;
        Ok(templates.iter().map(to_response).collect())
    }

    pub async fn get_permission_template(
        &self,
        template_id: &str,
    ) -> Result<Option<PermissionTemplateResponse>, DomainError> {
        Ok(self
            .permission_template_repo
            .find_by_id(template_id)
            .await?
            .as_ref()
            .map(to_response))
    }

    pub async fn list_departments_in_use(&self) -> Result<Vec<String>, DomainError> {
        let departments_in_use = self.user_repo.list_distinct_departments_in_use().await?;
        let existing = self.department_repo.find_all(true, 5000, 0).await?;
        let mut existing_by_name = existing
            .into_iter()
            .filter_map(|item| {
                let key = item.name.trim().to_string();
                if key.is_empty() {
                    None
                } else {
                    Some((key, item))
                }
            })
            .collect::<std::collections::HashMap<_, _>>();
        let in_use = departments_in_use
            .iter()
            .cloned()
            .collect::<std::collections::HashSet<_>>();

        for department_name in &departments_in_use {
            if let Some(existing) = existing_by_name.get_mut(department_name) {
                if !existing.is_active {
                    existing.is_active = true;
                    existing.updated_at = Some(Utc::now());
                    let saved = self.department_repo.save(existing).await?;
                    *existing = saved;
                }
                continue;
            }
            let department = Department {
                id: ulid::Ulid::new().to_string(),
                name: department_name.clone(),
                code: None,
                description: None,
                manager_id: None,
                terminal: None,
                created_at: Some(Utc::now()),
                updated_at: Some(Utc::now()),
                is_active: true,
            };
            let saved = self.department_repo.save(&department).await?;
            existing_by_name.insert(saved.name.trim().to_string(), saved);
        }

        for (name, existing) in existing_by_name {
            if in_use.contains(&name) {
                continue;
            }
            if self.user_repo.has_any_user_with_department_id(&existing.id).await? {
                continue;
            }
            if self.department_repo.has_dependencies(&existing.id).await? {
                continue;
            }
            let _ = self.department_repo.delete_permanently(&existing.id).await?;
        }

        Ok(departments_in_use)
    }
}

pub struct AuthAdminCommandService {
    permission_template_repo: Arc<dyn PermissionTemplateRepository + Send + Sync>,
    role_repo: Arc<dyn RoleRepository + Send + Sync>,
}

impl AuthAdminCommandService {
    pub fn new(
        permission_template_repo: Arc<dyn PermissionTemplateRepository + Send + Sync>,
        role_repo: Arc<dyn RoleRepository + Send + Sync>,
    ) -> Self {
        Self {
            permission_template_repo,
            role_repo,
        }
    }

    pub async fn create_permission_template(
        &self,
        dto: PermissionTemplateCreate,
    ) -> Result<PermissionTemplateResponse, DomainError> {
        let name = normalize_required(&dto.name, "name")?;
        if self.permission_template_repo.exists_by_name(&name).await? {
            return Err(DomainError::Conflict("模板名称已存在".into()));
        }

        let code = normalize_optional(dto.code.as_deref());
        if let Some(code) = code.as_deref() {
            if self.permission_template_repo.exists_by_code(code).await? {
                return Err(DomainError::Conflict("模板代码已存在".into()));
            }
        }

        let now = Utc::now();
        let saved = self
            .permission_template_repo
            .save(&PermissionTemplate {
                id: ulid::Ulid::new().to_string(),
                name,
                code,
                description: normalize_optional(dto.description.as_deref()),
                permissions: normalize_permissions(dto.permissions),
                is_system: false,
                category: normalize_optional(dto.category.as_deref()),
                display_order: dto.display_order,
                is_active: true,
                created_at: now,
                updated_at: now,
            })
            .await?;
        Ok(to_response(&saved))
    }

    pub async fn update_permission_template(
        &self,
        template_id: &str,
        dto: PermissionTemplateUpdate,
    ) -> Result<PermissionTemplateResponse, DomainError> {
        let mut template = self
            .permission_template_repo
            .find_by_id(template_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "PermissionTemplate",
                id: template_id.to_string(),
            })?;

        if let Some(name) = dto.name.as_deref() {
            let normalized = normalize_required(name, "name")?;
            if normalized != template.name && self.permission_template_repo.exists_by_name(&normalized).await? {
                return Err(DomainError::Conflict("模板名称已存在".into()));
            }
            template.name = normalized;
        }
        if let Some(code) = dto.code.as_deref() {
            let normalized = normalize_required(code, "code")?;
            if template.code.as_deref() != Some(normalized.as_str())
                && self.permission_template_repo.exists_by_code(&normalized).await?
            {
                return Err(DomainError::Conflict("模板代码已存在".into()));
            }
            template.code = Some(normalized);
        }
        if dto.code.is_some() && dto.code.as_deref().is_some_and(|value| value.trim().is_empty()) {
            template.code = None;
        }
        if dto.description.is_some() {
            template.description = normalize_optional(dto.description.as_deref());
        }
        if let Some(permissions) = dto.permissions {
            template.permissions = normalize_permissions(permissions);
        }
        if dto.category.is_some() {
            template.category = normalize_optional(dto.category.as_deref());
        }
        if let Some(display_order) = dto.display_order {
            template.display_order = display_order;
        }
        if let Some(is_active) = dto.is_active {
            template.is_active = is_active;
        }
        template.updated_at = Utc::now();

        let saved = self.permission_template_repo.save(&template).await?;
        Ok(to_response(&saved))
    }

    pub async fn delete_permission_template(&self, template_id: &str) -> Result<(), DomainError> {
        let template = self
            .permission_template_repo
            .find_by_id(template_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "PermissionTemplate",
                id: template_id.to_string(),
            })?;
        if template.is_system {
            return Err(DomainError::BusinessRuleViolation("系统预设模板不可删除".into()));
        }
        let deleted = self.permission_template_repo.delete(template_id).await?;
        if !deleted {
            return Err(DomainError::Internal("模板删除失败".into()));
        }
        Ok(())
    }

    pub async fn apply_template_to_role(
        &self,
        role_id: &str,
        template_id: &str,
        mode: &str,
    ) -> Result<(String, String), DomainError> {
        let template = self
            .permission_template_repo
            .find_by_id(template_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "PermissionTemplate",
                id: template_id.to_string(),
            })?;
        let mut role = self
            .role_repo
            .find_by_id(role_id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "Role",
                id: role_id.to_string(),
            })?;

        match mode.trim().to_ascii_lowercase().as_str() {
            "replace" => {
                role.permissions = template.permissions.clone();
            }
            "append" => {
                for permission in &template.permissions {
                    if !role.permissions.iter().any(|item| item == permission) {
                        role.permissions.push(permission.clone());
                    }
                }
            }
            _ => {
                return Err(DomainError::ValidationError("无效的应用模式".into()));
            }
        }

        role.updated_at = Utc::now();
        self.role_repo.save(&role).await?;
        self.role_repo.set_permissions(&role.id, &role.permissions).await?;
        Ok((template.name, role.name))
    }
}

fn to_response(template: &PermissionTemplate) -> PermissionTemplateResponse {
    PermissionTemplateResponse {
        id: template.id.clone(),
        name: template.name.clone(),
        code: template.code.clone(),
        description: template.description.clone(),
        permissions: template.permissions.clone(),
        is_system: template.is_system,
        category: template.category.clone(),
        display_order: template.display_order,
        is_active: template.is_active,
        created_at: template.created_at,
        updated_at: template.updated_at,
    }
}

fn normalize_permissions(values: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    for value in values {
        let item = value.trim();
        if item.is_empty() || normalized.iter().any(|existing| existing == item) {
            continue;
        }
        normalized.push(item.to_string());
    }
    normalized
}

fn normalize_optional(value: Option<&str>) -> Option<String> {
    let text = value.unwrap_or("").trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

fn normalize_required(value: &str, field_name: &str) -> Result<String, DomainError> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(DomainError::ValidationError(format!("{field_name} is required")));
    }
    Ok(normalized.to_string())
}
