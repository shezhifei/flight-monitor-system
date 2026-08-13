//! Auth + device API wrappers.
//!
//! Endpoints (backend `routes/auth/mod.rs`, `routes/mobile.rs`):
//! - `POST /api/v2/auth/login` / `POST /api/v2/auth/refresh` → raw
//!   `TokenResponse` (NOT enveloped); native surface is mandatory so the JSON
//!   carries `session_secret`;
//! - `POST /api/v2/auth/logout` / `POST /api/v2/auth/heartbeat` →
//!   `{success, message}` ack at the TOP LEVEL (the live response is
//!   `{success, message, data: null}` — envelope-shaped but with null data,
//!   so it must be parsed raw, not via `call_with_envelope`);
//! - `GET /api/v2/auth/me` → raw `UserProfile` (backend returns
//!   `UserResponse` verbatim);
//! - `POST /api/v2/mobile/devices/register` /
//!   `POST /api/v2/mobile/devices/{id}/heartbeat` → enveloped
//!   `MobileDeviceResponse`.

use crate::client::ApiClient;
use crate::dto::auth::{
    AuthAckResponse, LoginRequest, TokenResponse, UserProfile,
};
use crate::dto::mobile::{
    MobileDeviceHeartbeatRequest, MobileDeviceRegisterRequest, MobileDeviceResponse,
};
use crate::error::CoreError;
use crate::session::TokenBundle;

/// Login and activate the session. The request goes through the standard
/// pipeline: `X-Client-Surface: native` is a fixed header, so the backend
/// returns the full native `TokenResponse` including `session_secret`
/// (web surfaces get it nulled out — verified on a live backend).
///
/// Fail-closed: a response without `refresh_token`/`session_secret` is an
/// auth error and leaves the session anonymous.
pub async fn login(
    client: &ApiClient,
    username: &str,
    password: &str,
) -> Result<TokenBundle, CoreError> {
    let token: TokenResponse = client
        .call_raw(
            "POST",
            "/api/v2/auth/login",
            Some(&LoginRequest {
                username: username.to_string(),
                password: password.to_string(),
            }),
        )
        .await?;
    client.session().activate(&token).await
}

/// Logout: best-effort server call, local state always cleared afterwards.
pub async fn logout(client: &ApiClient) -> Result<(), CoreError> {
    let result: Result<AuthAckResponse, CoreError> = client
        .call_raw("POST", "/api/v2/auth/logout", Option::<&()>::None)
        .await;
    client.session().clear().await;
    result.map(|_| ())
}

/// Force a token refresh through the session's single-flight path.
pub async fn refresh(client: &ApiClient) -> Result<TokenBundle, CoreError> {
    let current = client
        .session()
        .current_token_bundle()
        .await
        .ok_or_else(|| CoreError::Auth("not logged in".into()))?;
    client
        .session()
        .refresh_single_flight(
            crate::client::shared_http_client(),
            &client.config().base_url,
            Some(&current.access_token),
        )
        .await
}

/// `GET /api/v2/auth/me` — raw (non-enveloped) user profile.
pub async fn me(client: &ApiClient) -> Result<UserProfile, CoreError> {
    client.call_raw("GET", "/api/v2/auth/me", Option::<&()>::None).await
}

/// `POST /api/v2/auth/heartbeat` — keep-alive ack.
pub async fn auth_heartbeat(client: &ApiClient) -> Result<(), CoreError> {
    let ack: AuthAckResponse = client
        .call_raw("POST", "/api/v2/auth/heartbeat", Option::<&()>::None)
        .await?;
    if ack.success {
        Ok(())
    } else {
        Err(CoreError::Api {
            message: ack.message.unwrap_or_else(|| "heartbeat failed".to_string()),
            request_id: None,
        })
    }
}

/// `POST /api/v2/mobile/devices/register` — enveloped `MobileDeviceResponse`.
pub async fn register_device(
    client: &ApiClient,
    request: &MobileDeviceRegisterRequest,
) -> Result<MobileDeviceResponse, CoreError> {
    client
        .call_with_envelope("POST", "/api/v2/mobile/devices/register", Some(request))
        .await
}

/// `POST /api/v2/mobile/devices/{device_id}/heartbeat`.
pub async fn device_heartbeat(
    client: &ApiClient,
    device_id: &str,
    request: &MobileDeviceHeartbeatRequest,
) -> Result<MobileDeviceResponse, CoreError> {
    client
        .call_with_envelope(
            "POST",
            &format!("/api/v2/mobile/devices/{device_id}/heartbeat"),
            Some(request),
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::ApiClient;
    use crate::config::ApiConfig;
    use crate::session::SessionManager;
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;

    /// One-shot mock server: answers every request with the given
    /// status/body and records the first request's headers.
    async fn spawn_one_shot(status: u16, body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let (read_half, mut write_half) = socket.split();
            let mut reader = BufReader::new(read_half);
            let mut content_length = 0usize;
            let mut line = String::new();
            loop {
                line.clear();
                if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
                    return;
                }
                let trimmed = line.trim_end();
                if trimmed.is_empty() {
                    break;
                }
                if let Some((name, value)) = trimmed.split_once(':') {
                    if name.trim().eq_ignore_ascii_case("content-length") {
                        content_length = value.trim().parse().unwrap_or(0);
                    }
                }
            }
            let mut buf = vec![0u8; content_length];
            if content_length > 0 {
                let _ = reader.read_exact(&mut buf).await;
            }
            let reason = if status == 200 { "OK" } else { "Error" };
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = write_half.write_all(response.as_bytes()).await;
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn login_activates_session_with_native_secret() {
        let base = spawn_one_shot(
            200,
            r#"{"access_token":"a1","token_type":"bearer","expires_in":3600,"refresh_token":"r1","sse_token":null,"sse_expires_in":null,"session_secret":"s1"}"#,
        )
        .await;
        let session = SessionManager::new();
        let client = ApiClient::new(ApiConfig::new(base, true).unwrap(), session.clone(), "dev");
        let bundle = login(&client, "user", "pass").await.unwrap();
        assert_eq!(bundle.access_token, "a1");
        assert_eq!(bundle.session_secret, "s1");
        assert_eq!(session.current_token_bundle().await, Some(bundle));
    }

    #[tokio::test]
    async fn login_without_secret_fails_closed() {
        // Web-shaped response (secrets nulled): must NOT silently degrade.
        let base = spawn_one_shot(
            200,
            r#"{"access_token":"a1","token_type":"bearer","expires_in":3600,"refresh_token":null,"sse_token":null,"sse_expires_in":null,"session_secret":null}"#,
        )
        .await;
        let session = SessionManager::new();
        let client = ApiClient::new(ApiConfig::new(base, true).unwrap(), session.clone(), "dev");
        let err = login(&client, "user", "pass").await.unwrap_err();
        assert!(matches!(err, CoreError::Auth(_)));
        assert_eq!(session.current_token_bundle().await, None);
    }
}
