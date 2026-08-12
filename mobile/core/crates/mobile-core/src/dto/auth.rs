//! Auth DTOs (plan §3.6).
//!
//! Field authority: legacy `AuthModels.kt`; `TokenResponse` additionally
//! cross-checked against the backend `Token` schema
//! (`services/api-server/crates/application/src/schemas/auth_schemas.rs`),
//! which is what login/refresh actually serialize.

use serde::{Deserialize, Serialize};

/// `POST /api/v2/auth/login` body.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// `POST /api/v2/auth/refresh` body (native clients use the JSON body; web
/// uses the HttpOnly cookie — see backend `routes/auth/login/session.rs`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RefreshRequest {
    pub refresh_token: String,
}

/// Login/refresh response body for the `native` client surface.
///
/// The backend returns the `Token` schema verbatim for native clients; web
/// clients get `refresh_token`/`session_secret` nulled out (delivered via
/// cookies instead). `session_secret` is `None` for web — native callers must
/// treat a missing secret as an error (fail-closed).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub refresh_token: Option<String>,
    pub sse_token: Option<String>,
    pub sse_expires_in: Option<i64>,
    pub session_secret: Option<String>,
}

/// `POST /api/v2/auth/sse-token` response.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SseTokenResponse {
    pub sse_token: String,
    pub sse_expires_in: i64,
}

/// Simple ack payload (`{success, message}`) used by logout/heartbeat.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AuthAckResponse {
    pub success: bool,
    pub message: Option<String>,
}

/// Current-user profile (`GET /api/v2/auth/me` returns the backend
/// `UserResponse` verbatim; this is the legacy app's projection of it).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UserProfile {
    pub id: String,
    pub username: String,
    pub is_admin: bool,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    pub display_name: Option<String>,
    pub job_title: Option<String>,
    pub effective_operator_name: Option<String>,
    pub effective_operator_label: Option<String>,
    pub operator_context_type: Option<String>,
    pub operator_context_id: Option<String>,
}

/// `PUT /api/v2/auth/me/operator-context` body.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct OperatorContextUpdateRequest {
    pub operator_name: Option<String>,
}
