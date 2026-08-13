//! Auth DTOs.
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

/// Simple ack payload (`{success, message}`) used by logout/heartbeat.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AuthAckResponse {
    pub success: bool,
    pub message: Option<String>,
}

/// Current-user profile (`GET /api/v2/auth/me` returns the backend
/// `UserResponse` verbatim).
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: logout/heartbeat return the ack at the top level with
    /// `data: null` (`{"success":true,"message":"...","data":null}`) — it must
    /// parse raw, envelope extraction would fail on the null data.
    #[test]
    fn auth_ack_parses_top_level_ack_with_null_data() {
        let ack: AuthAckResponse = serde_json::from_str(
            r#"{"success":true,"message":"ok","data":null}"#,
        )
        .unwrap();
        assert!(ack.success);
        assert_eq!(ack.message.as_deref(), Some("ok"));
    }
}
