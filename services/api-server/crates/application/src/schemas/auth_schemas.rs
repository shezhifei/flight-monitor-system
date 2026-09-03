//! 认证 & 用户 DTO 模式
//!
//! 对应 Python `src/application/schemas/auth_schemas.py`。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

// ---------------------------------------------------------------------------
// 认证请求
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct UserLogin {
    pub username: String,
    pub password: String,
}

/// 占席请求。`proof.kind` 本期仅实现 `password`；`face` / `ext` 预留 501，其它 400。
#[derive(Debug, Clone, Deserialize)]
pub struct SeatOccupyRequest {
    /// 空席第一次上岗 / 换人都走这条路：输入要占用该席的个人用户名。
    pub personal_username: String,
    pub proof: SeatProof,
}

/// 厂商无关的占席证明。密码不入审计。
#[derive(Debug, Clone, Deserialize)]
pub struct SeatProof {
    pub kind: String,
    /// 仅 `kind == "password"` 时使用；其余 kind 忽略此字段。
    #[serde(default)]
    pub password: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserCreate {
    pub username: String,
    pub email: Option<String>,
    pub password: String,
    pub confirm_password: Option<String>,
    #[serde(default)]
    pub is_admin: bool,
    pub roles: Option<Vec<String>>,
    pub department: Option<String>,
    #[serde(default = "default_job_level")]
    pub job_level: Option<i16>,
    pub job_title: Option<String>,
    pub display_name: Option<String>,
    /// 账号类型：`personal` / `position`。用户管理创建必选类型；岗位不可登录。
    #[serde(default = "default_account_type")]
    pub account_type: String,
}

fn default_account_type() -> String {
    "personal".to_string()
}

fn default_job_level() -> Option<i16> {
    Some(1)
}

#[derive(Debug, Clone, Deserialize)]
pub struct RefreshTokenRequest {
    pub refresh_token: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChangePassword {
    pub old_password: String,
    pub new_password: String,
    pub confirm_new_password: String,
}

// ---------------------------------------------------------------------------
// Token
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Token {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub refresh_token: Option<String>,
    pub sse_token: Option<String>,
    pub sse_expires_in: Option<i64>,
    pub session_secret: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TokenData {
    pub sub: Option<String>,
    pub email: Option<String>,
    pub username: Option<String>,
    #[serde(rename = "type")]
    pub token_kind: Option<String>,
    pub is_admin: Option<bool>,
    #[serde(default)]
    pub permissions: Vec<String>,
    pub department: Option<String>,
    pub department_id: Option<String>,
    pub pv: Option<i64>,
    pub iat: Option<i64>,
    pub exp: Option<i64>,
    pub iss: Option<String>,
    pub aud: Option<String>,
    pub ua_hash: Option<String>,
    pub ip_subnet_hash: Option<String>,
}

// ---------------------------------------------------------------------------
// 用户更新
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Deserialize)]
pub struct UserUpdate {
    pub username: Option<String>,
    pub email: Option<String>,
    pub password: Option<String>,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct UserAdminUpdate {
    pub username: Option<String>,
    pub email: Option<String>,
    pub password: Option<String>,
    pub display_name: Option<String>,
    pub is_active: Option<bool>,
    pub is_admin: Option<bool>,
    pub roles: Option<Vec<String>>,
    pub department: Option<String>,
    pub job_level: Option<i16>,
    pub job_title: Option<String>,
    // 注意：不含 account_type —— 创建后不可改类型（PR7）。
}

// ---------------------------------------------------------------------------
// 用户响应
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct UserResponse {
    pub id: String,
    pub username: String,
    pub email: String,
    pub is_active: bool,
    pub is_verified: bool,
    pub is_admin: bool,
    #[ts(type = "string")]
    pub created_at: DateTime<Utc>,
    #[ts(type = "string")]
    pub last_login_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    pub display_name: Option<String>,
    pub effective_operator_name: Option<String>,
    pub effective_operator_label: Option<String>,
    pub operator_context_type: Option<String>,
    pub operator_context_id: Option<String>,
    pub department: Option<String>,
    #[serde(default = "default_job_level")]
    pub job_level: Option<i16>,
    pub job_title: Option<String>,
    pub permission_version: i64,
    #[serde(default = "default_account_type")]
    pub account_type: String,
    #[serde(default = "default_login_enabled")]
    pub login_enabled: bool,
    #[serde(default)]
    pub current_occupant_user_id: Option<String>,
}

// ---------------------------------------------------------------------------
// 角色 & 权限
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct RoleCreate {
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RoleUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub permissions: Option<Vec<String>>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct RoleResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    pub is_active: bool,
    pub is_system: bool,
    #[ts(type = "string")]
    pub created_at: DateTime<Utc>,
    #[ts(type = "string")]
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub user_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PermissionResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserRoleAssign {
    pub user_id: String,
    pub role_id: String,
}
