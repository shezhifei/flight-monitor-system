//! 用户领域模型
//!
//! 对应 Python `src/domain/models/user.py`。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// 权限模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permission {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default = "default_true")]
    pub is_active: bool,
}

/// 角色模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Role {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default = "default_true")]
    pub is_active: bool,
    #[serde(default)]
    pub is_system: bool,
}

impl Role {
    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.iter().any(|p| p == permission || p == "*")
    }
}

/// 用户实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub email: String,
    pub password_hash: String,
    pub username: String,
    pub display_name: Option<String>,
    #[serde(default)]
    pub roles: Vec<Role>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_login_at: Option<DateTime<Utc>>,
    #[serde(default = "default_true")]
    pub is_active: bool,
    #[serde(default)]
    pub is_verified: bool,
    #[serde(default)]
    pub is_admin: bool,

    // 验证相关
    pub verification_token: Option<String>,
    pub verification_token_expires: Option<DateTime<Utc>>,
    pub verified_at: Option<DateTime<Utc>>,

    // 密码重置
    pub password_reset_token: Option<String>,
    pub password_reset_token_expires: Option<DateTime<Utc>>,
    pub password_changed_at: Option<DateTime<Utc>>,

    // 组织架构
    pub department: Option<String>,
    pub department_id: Option<String>,
    #[serde(default = "default_job_level")]
    pub job_level: Option<i16>,
    pub job_title: Option<String>,

    // 权限版本控制
    #[serde(default = "default_permission_version")]
    pub permission_version: i32,
}

fn default_true() -> bool {
    true
}
fn default_job_level() -> Option<i16> {
    Some(1)
}
fn default_permission_version() -> i32 {
    1
}

impl User {
    pub fn has_role(&self, role_name: &str) -> bool {
        self.roles.iter().any(|r| r.name == role_name)
    }

    pub fn has_permission(&self, permission: &str) -> bool {
        if self.is_admin {
            return true;
        }
        self.get_all_permissions().contains(permission)
    }

    pub fn get_all_permissions(&self) -> HashSet<String> {
        let mut perms = HashSet::new();
        for role in &self.roles {
            for p in &role.permissions {
                let trimmed = p.trim();
                if !trimmed.is_empty() {
                    perms.insert(trimmed.to_string());
                }
            }
        }
        perms
    }

    pub fn is_verification_token_valid(&self) -> bool {
        match (&self.verification_token, &self.verification_token_expires) {
            (Some(_), Some(expires)) => Utc::now() <= *expires,
            _ => false,
        }
    }

    pub fn is_password_reset_token_valid(&self) -> bool {
        match (&self.password_reset_token, &self.password_reset_token_expires) {
            (Some(_), Some(expires)) => Utc::now() <= *expires,
            _ => false,
        }
    }
}
