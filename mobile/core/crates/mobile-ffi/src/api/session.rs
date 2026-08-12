//! Session exports (plan §4 初始化与状态).
//!
//! Token material crosses the bridge ONLY through [`TokenBundle`] (so Dart
//! can persist it in flutter_secure_storage); the `session_state` stream is
//! deliberately token-free.

use anyhow::Context;
use mobile_core::session::{SessionStateSnapshot, TokenBundle as CoreTokenBundle};

use super::runtime;
use crate::frb_generated::StreamSink;

/// Mirror of `mobile_core::session::TokenBundle`. This exact shape is what
/// Dart stores in flutter_secure_storage after login and hands back via
/// [`restore_tokens`] at startup.
pub struct TokenBundle {
    pub access_token: String,
    pub refresh_token: String,
    pub session_secret: String,
    pub access_expire_at: i64,
}

impl From<CoreTokenBundle> for TokenBundle {
    fn from(b: CoreTokenBundle) -> Self {
        Self {
            access_token: b.access_token,
            refresh_token: b.refresh_token,
            session_secret: b.session_secret,
            access_expire_at: b.access_expire_at,
        }
    }
}

impl From<TokenBundle> for CoreTokenBundle {
    fn from(b: TokenBundle) -> Self {
        Self {
            access_token: b.access_token,
            refresh_token: b.refresh_token,
            session_secret: b.session_secret,
            access_expire_at: b.access_expire_at,
        }
    }
}

/// Token-free session snapshot for the UI (login guard / indicators).
/// Mirrors `mobile_core::session::SessionStateSnapshot`.
pub enum SessionState {
    Anonymous,
    Active { access_expire_at: i64 },
}

impl From<SessionStateSnapshot> for SessionState {
    fn from(s: SessionStateSnapshot) -> Self {
        match s {
            SessionStateSnapshot::Anonymous => Self::Anonymous,
            SessionStateSnapshot::Active { access_expire_at } => {
                Self::Active { access_expire_at }
            }
        }
    }
}

/// Restore a persisted bundle at startup. Fail-closed: an obviously invalid
/// bundle (empty token/secret fields) clears the session to anonymous and
/// returns an error instead of silently degrading.
pub async fn restore_tokens(bundle: TokenBundle) -> anyhow::Result<()> {
    let rt = runtime()?;
    if bundle.access_token.is_empty()
        || bundle.refresh_token.is_empty()
        || bundle.session_secret.is_empty()
    {
        rt.client.session().clear().await;
        anyhow::bail!("invalid persisted token bundle; session cleared to anonymous");
    }
    rt.client.session().restore_tokens(bundle.into()).await;
    Ok(())
}

/// Current bundle for Dart-side persistence (store after login, clear on
/// logout). `None` when anonymous.
pub async fn current_token_bundle() -> anyhow::Result<Option<TokenBundle>> {
    Ok(runtime()?
        .client
        .session()
        .current_token_bundle()
        .await
        .map(Into::into))
}

/// Stream of token-free session state. Emits the current snapshot
/// immediately, then every transition until the sink is dropped.
pub async fn session_state(sink: StreamSink<SessionState>) -> anyhow::Result<()> {
    let rt = runtime()?;
    let mut rx = rt.client.session().subscribe_state();
    sink.add((*rx.borrow()).into())
        .map_err(|e| anyhow::anyhow!("session_state sink closed: {e:?}"))?;
    loop {
        rx.changed().await.context("session state channel closed")?;
        sink.add((*rx.borrow_and_update()).into())
            .map_err(|e| anyhow::anyhow!("session_state sink closed: {e:?}"))?;
    }
}

/// Logout: best-effort server call, local session always cleared (Dart must
/// also delete the persisted bundle).
pub async fn logout() -> anyhow::Result<()> {
    let rt = runtime()?;
    mobile_core::api::auth::logout(&rt.client).await?;
    Ok(())
}
