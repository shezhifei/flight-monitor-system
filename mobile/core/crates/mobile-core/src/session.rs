//! Token session state machine.
//!
//! States: `Anonymous | Active { access, refresh, session_secret,
//! access_expire_at }`. Persistence is NOT done in Rust — the FFI/Dart side
//! stores the bundle in flutter_secure_storage and hands it back via
//! [`SessionManager::restore_tokens`] (fail-closed: a failed restore must be
//! treated as logged out; this type simply never writes plaintext anywhere).
//!
//! Refresh policy:
//! - proactive: [`SessionManager::ensure_valid`] refreshes when the access
//!   token has less than [`REFRESH_THRESHOLD_SECS`] left;
//! - reactive + single-flight: concurrent 401s trigger exactly ONE refresh
//!   (`refresh_single_flight`); callers that were waiting on the same stale
//!   token reuse the result instead of firing their own request;
//! - on success the new `session_secret` replaces the old one immediately —
//!   the backend derives it as `HMAC-SHA256(jwt_secret, access_token)` so the
//!   old secret is invalid the moment the refresh lands;
//! - refresh answering 401 → state cleared, [`CoreError::Auth`] returned
//!   (re-login required).

use std::sync::Arc;

use tokio::sync::{watch, Mutex, RwLock};

use crate::client::HEADER_CLIENT_SURFACE;
use crate::dto::auth::{RefreshRequest, TokenResponse};
use crate::error::CoreError;

/// Proactive refresh threshold: refresh when the access token has less than
/// this many seconds of validity left.
pub const REFRESH_THRESHOLD_SECS: i64 = 30;

/// One snapshot of the active session. This is the exact shape the Dart side
/// persists in flutter_secure_storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenBundle {
    pub access_token: String,
    pub refresh_token: String,
    pub session_secret: String,
    /// Access-token expiry as epoch seconds (computed as
    /// `login_time + expires_in`; the backend sends `expires_in`, not an
    /// absolute timestamp).
    pub access_expire_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
enum SessionState {
    #[default]
    Anonymous,
    Active(TokenBundle),
}

/// Public, token-free snapshot of the session for status streams.
/// Token/secret material NEVER leaves the state machine through this channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStateSnapshot {
    Anonymous,
    Active { access_expire_at: i64 },
}

impl From<&SessionState> for SessionStateSnapshot {
    fn from(state: &SessionState) -> Self {
        match state {
            SessionState::Anonymous => SessionStateSnapshot::Anonymous,
            SessionState::Active(bundle) => SessionStateSnapshot::Active {
                access_expire_at: bundle.access_expire_at,
            },
        }
    }
}

/// Token state machine with single-flight refresh. Cheap to clone (shared
/// inner state) so the HTTP client pipeline can hold one reference.
#[derive(Debug, Clone)]
pub struct SessionManager {
    inner: Arc<SessionInner>,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self {
            inner: Arc::new(SessionInner {
                state: RwLock::new(SessionState::Anonymous),
                refresh_lock: Mutex::new(()),
                snapshot_tx: watch::channel(SessionStateSnapshot::Anonymous).0,
            }),
        }
    }
}

#[derive(Debug)]
struct SessionInner {
    state: RwLock<SessionState>,
    /// Single-flight guard: only one in-flight refresh at a time.
    refresh_lock: Mutex<()>,
    /// Token-free state notifications for UI (login guard, indicators).
    snapshot_tx: watch::Sender<SessionStateSnapshot>,
}

fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl SessionManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Restore a persisted bundle (called by FFI at startup). Any previously
    /// held state is replaced.
    pub async fn restore_tokens(&self, bundle: TokenBundle) {
        let mut state = self.inner.state.write().await;
        *state = SessionState::Active(bundle);
        self.inner.snapshot_tx.send_replace((&*state).into());
    }

    /// Build a bundle from a fresh login/refresh response and activate it.
    ///
    /// Fail-closed: a native-surface response MUST carry both `refresh_token`
    /// and `session_secret`; a missing one means the request was not treated
    /// as native (or the contract changed) and must NOT silently degrade.
    pub async fn activate(&self, token: &TokenResponse) -> Result<TokenBundle, CoreError> {
        let bundle = TokenBundle {
            access_token: token.access_token.clone(),
            refresh_token: token.refresh_token.clone().ok_or_else(|| {
                CoreError::Auth("token response missing refresh_token (native surface?)".into())
            })?,
            session_secret: token.session_secret.clone().ok_or_else(|| {
                CoreError::Auth("token response missing session_secret (native surface?)".into())
            })?,
            access_expire_at: now_epoch() + token.expires_in,
        };
        self.restore_tokens(bundle.clone()).await;
        Ok(bundle)
    }

    /// Current bundle, `None` when anonymous.
    pub async fn current_token_bundle(&self) -> Option<TokenBundle> {
        match &*self.inner.state.read().await {
            SessionState::Anonymous => None,
            SessionState::Active(bundle) => Some(bundle.clone()),
        }
    }

    /// Drop all tokens (logout / refresh rejected). Never logs token material.
    pub async fn clear(&self) {
        let mut state = self.inner.state.write().await;
        *state = SessionState::Anonymous;
        self.inner.snapshot_tx.send_replace(SessionStateSnapshot::Anonymous);
    }

    /// Subscribe to token-free session state changes (for the FFI
    /// `session_state()` stream). The receiver immediately observes the
    /// current snapshot via `borrow()`.
    pub fn subscribe_state(&self) -> watch::Receiver<SessionStateSnapshot> {
        self.inner.snapshot_tx.subscribe()
    }

    /// Return a valid bundle, proactively refreshing when the access token is
    /// within [`REFRESH_THRESHOLD_SECS`] of expiry.
    pub async fn ensure_valid(
        &self,
        http: &reqwest::Client,
        base_url: &str,
    ) -> Result<TokenBundle, CoreError> {
        let current = self
            .current_token_bundle()
            .await
            .ok_or_else(|| CoreError::Auth("not logged in".into()))?;
        if current.access_expire_at - now_epoch() >= REFRESH_THRESHOLD_SECS {
            return Ok(current);
        }
        self.refresh_single_flight(http, base_url, Some(&current.access_token))
            .await
    }

    /// Single-flight refresh.
    ///
    /// `stale_access` is the access token the caller just saw fail (or the one
    /// about to expire). While this caller waits on the refresh mutex another
    /// request may already have refreshed; in that case the current bundle no
    /// longer matches `stale_access` and its (fresh) value is returned WITHOUT
    /// firing a second refresh request.
    ///
    /// Never logs token/secret material.
    pub async fn refresh_single_flight(
        &self,
        http: &reqwest::Client,
        base_url: &str,
        stale_access: Option<&str>,
    ) -> Result<TokenBundle, CoreError> {
        let _guard = self.inner.refresh_lock.lock().await;

        let current = self
            .current_token_bundle()
            .await
            .ok_or_else(|| CoreError::Auth("not logged in".into()))?;
        if let Some(stale) = stale_access {
            if current.access_token != stale {
                // Someone else refreshed while we waited — reuse their result.
                return Ok(current);
            }
        }

        tracing::debug!("session: performing single-flight token refresh");
        let url = format!("{}/api/v2/auth/refresh", base_url.trim_end_matches('/'));
        let resp = http
            .post(url)
            // Native surface is required so the JSON body carries the new
            // session_secret (web gets null + a cookie instead).
            .header(HEADER_CLIENT_SURFACE, "native")
            .json(&RefreshRequest {
                refresh_token: current.refresh_token.clone(),
            })
            .send()
            .await
            .map_err(|e| CoreError::Network(format!("refresh request failed: {e}")))?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            self.clear().await;
            tracing::warn!("session: refresh rejected (401), session cleared; re-login required");
            return Err(CoreError::Auth(
                "refresh token rejected; re-login required".into(),
            ));
        }
        if !resp.status().is_success() {
            return Err(CoreError::Network(format!(
                "refresh failed with HTTP {}",
                resp.status()
            )));
        }

        let token: TokenResponse = resp.json().await.map_err(|e| {
            CoreError::Serialization(format!("invalid refresh response: {e}"))
        })?;
        // activate() replaces the session_secret immediately.
        self.activate(&token).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;

    /// Minimal mock HTTP server that counts requests to `/api/v2/auth/refresh`
    /// and answers them with the given status + JSON body.
    struct MockRefreshServer {
        base_url: String,
        refresh_count: Arc<AtomicUsize>,
    }

    async fn spawn_mock_refresh(status: u16, body: &'static str) -> MockRefreshServer {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let refresh_count = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&refresh_count);
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let counter = Arc::clone(&counter);
                tokio::spawn(async move {
                    let (read_half, mut write_half) = socket.split();
                    let mut reader = BufReader::new(read_half);
                    let mut request_line = String::new();
                    reader.read_line(&mut request_line).await.unwrap();
                    if request_line.contains("/api/v2/auth/refresh") {
                        counter.fetch_add(1, Ordering::SeqCst);
                    }
                    // Drain headers, then the body by content-length.
                    let mut content_length = 0usize;
                    let mut line = String::new();
                    loop {
                        line.clear();
                        reader.read_line(&mut line).await.unwrap();
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
                    let mut body_buf = vec![0u8; content_length];
                    reader.read_exact(&mut body_buf).await.unwrap();
                    let reason = if status == 200 { "OK" } else { "Unauthorized" };
                    let response = format!(
                        "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    write_half.write_all(response.as_bytes()).await.unwrap();
                });
            }
        });
        MockRefreshServer {
            base_url: format!("http://{addr}"),
            refresh_count,
        }
    }

    fn http() -> reqwest::Client {
        reqwest::Client::builder().build().unwrap()
    }

    fn bundle(access: &str, expires_in: i64) -> TokenBundle {
        TokenBundle {
            access_token: access.to_string(),
            refresh_token: "refresh-x".to_string(),
            session_secret: "secret-x".to_string(),
            access_expire_at: now_epoch() + expires_in,
        }
    }

    fn token_response_json(access: &str, secret: &str) -> String {
        format!(
            r#"{{"access_token":"{access}","token_type":"bearer","expires_in":3600,"refresh_token":"refresh-new","sse_token":null,"sse_expires_in":null,"session_secret":"{secret}"}}"#
        )
    }

    #[tokio::test]
    async fn ensure_valid_refreshes_when_close_to_expiry() {
        // Leak to get 'static body for the mock.
        let body: &'static str = Box::leak(token_response_json("access-new", "secret-new").into_boxed_str());
        let server = spawn_mock_refresh(200, body).await;
        let session = SessionManager::new();
        session.restore_tokens(bundle("access-old", 10)).await;

        let new_bundle = session
            .ensure_valid(&http(), &server.base_url)
            .await
            .unwrap();
        assert_eq!(new_bundle.access_token, "access-new");
        assert_eq!(new_bundle.refresh_token, "refresh-new");
        // session_secret replaced immediately.
        assert_eq!(new_bundle.session_secret, "secret-new");
        assert_eq!(server.refresh_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn ensure_valid_keeps_fresh_token() {
        let server = spawn_mock_refresh(200, "{}").await;
        let session = SessionManager::new();
        session.restore_tokens(bundle("access-old", 3600)).await;

        let current = session
            .ensure_valid(&http(), &server.base_url)
            .await
            .unwrap();
        assert_eq!(current.access_token, "access-old");
        assert_eq!(server.refresh_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn refresh_401_clears_state_and_requires_relogin() {
        let server = spawn_mock_refresh(401, r#"{"detail":"invalid"}"#).await;
        let session = SessionManager::new();
        session.restore_tokens(bundle("access-old", 3600)).await;

        let err = session
            .refresh_single_flight(&http(), &server.base_url, Some("access-old"))
            .await
            .unwrap_err();
        assert!(matches!(err, CoreError::Auth(_)), "unexpected: {err:?}");
        assert_eq!(session.current_token_bundle().await, None);
    }

    #[tokio::test]
    async fn concurrent_401s_trigger_exactly_one_refresh() {
        let body: &'static str = Box::leak(token_response_json("access-new", "secret-new").into_boxed_str());
        let server = spawn_mock_refresh(200, body).await;
        let session = SessionManager::new();
        session.restore_tokens(bundle("access-stale", 3600)).await;

        let mut handles = Vec::new();
        for _ in 0..8 {
            let session = session.clone();
            let client = http();
            let base = server.base_url.clone();
            handles.push(tokio::spawn(async move {
                session
                    .refresh_single_flight(&client, &base, Some("access-stale"))
                    .await
            }));
        }
        for handle in handles {
            let bundle = handle.await.unwrap().unwrap();
            assert_eq!(bundle.access_token, "access-new");
            assert_eq!(bundle.session_secret, "secret-new");
        }
        // The whole point of the single-flight pattern: 8 concurrent 401s,
        // one refresh request on the wire.
        assert_eq!(server.refresh_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn anonymous_ensure_valid_fails_closed() {
        let session = SessionManager::new();
        let err = session.ensure_valid(&http(), "http://127.0.0.1:1").await.unwrap_err();
        assert!(matches!(err, CoreError::Auth(_)));
    }

    #[tokio::test]
    async fn state_stream_emits_token_free_snapshots() {
        let session = SessionManager::new();
        let mut rx = session.subscribe_state();
        assert_eq!(*rx.borrow(), SessionStateSnapshot::Anonymous);

        session.restore_tokens(bundle("a", 3600)).await;
        rx.changed().await.unwrap();
        match *rx.borrow() {
            SessionStateSnapshot::Active { access_expire_at } => {
                assert!(access_expire_at > now_epoch());
            }
            other => panic!("expected Active, got {other:?}"),
        }

        session.clear().await;
        rx.changed().await.unwrap();
        assert_eq!(*rx.borrow(), SessionStateSnapshot::Anonymous);
    }

    #[tokio::test]
    async fn activate_requires_native_secret() {
        let session = SessionManager::new();
        let web_shaped: TokenResponse = serde_json::from_str(
            r#"{"access_token":"a","token_type":"bearer","expires_in":3600,"refresh_token":null,"sse_token":null,"sse_expires_in":null,"session_secret":null}"#,
        )
        .unwrap();
        let err = session.activate(&web_shaped).await.unwrap_err();
        assert!(matches!(err, CoreError::Auth(_)));
        assert_eq!(session.current_token_bundle().await, None);
    }
}
