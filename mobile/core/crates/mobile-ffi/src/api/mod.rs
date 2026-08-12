//! Domain-level async exports visible to Dart (plan §4).
//!
//! Runtime assembly lives here (`init_core`); domain exports are split into
//! `session` / `auth` / `dispatch` submodules. Forwarding only — no business
//! logic lives in this crate.

pub mod auth;
pub mod business_case;
pub mod chat;
pub mod dispatch;
pub mod handover;
pub mod notification;
pub mod operations;
pub mod session;

use std::sync::{Arc, Mutex, OnceLock};

use anyhow::Context;
use mobile_core::offline::OfflineQueue;
use mobile_core::session::SessionManager;
use mobile_core::sse;
use mobile_core::signing;
use mobile_core::{ApiClient, ApiConfig};

use crate::frb_generated::StreamSink;

/// frb entry point required by `RustLib.init()` on the Dart side.
#[flutter_rust_bridge::frb(init)]
pub fn init_app() {
    flutter_rust_bridge::setup_default_user_utils();
}

/// Mirror of `mobile_core::signing::SignatureHeaders` for frb codegen.
/// Kept as a local struct so `mobile-core` stays frb-free.
pub struct SignatureHeaders {
    pub timestamp: String,
    pub nonce: String,
    pub body_sha256: String,
    pub signature: String,
}

impl From<signing::SignatureHeaders> for SignatureHeaders {
    fn from(h: signing::SignatureHeaders) -> Self {
        Self {
            timestamp: h.timestamp,
            nonce: h.nonce,
            body_sha256: h.body_sha256,
            signature: h.signature,
        }
    }
}

/// Assembled core runtime shared by every export. `client` bundles the
/// validated config, the shared HTTP pipeline, the session state machine and
/// the operator-context (device) id; `offline` is the sqlite action queue.
pub(crate) struct CoreRuntime {
    pub(crate) client: ApiClient,
    pub(crate) offline: OfflineQueue,
}

static RUNTIME: OnceLock<Mutex<Option<Arc<CoreRuntime>>>> = OnceLock::new();

fn runtime_slot() -> &'static Mutex<Option<Arc<CoreRuntime>>> {
    RUNTIME.get_or_init(|| Mutex::new(None))
}

/// Clone the assembled runtime handle. Fails until [`init_core`] has run.
pub(crate) fn runtime() -> anyhow::Result<Arc<CoreRuntime>> {
    runtime_slot()
        .lock()
        .expect("runtime mutex poisoned")
        .clone()
        .context("init_core must be called before any other export")
}

/// Initialize the core. Must be called once before any other export.
/// Re-initialization is allowed (replaces the previous runtime).
///
/// `operator_context_id` is the stable device id sent as
/// `X-Operator-Context-Id` on every request (§0.3; the legacy app uses
/// ANDROID_ID). It doubles as the `device_id` for device register/heartbeat.
/// `db_path` is the sqlite offline-queue file (Dart:
/// `getApplicationSupportDirectory`).
pub async fn init_core(
    base_url: String,
    allow_cleartext: bool,
    db_path: String,
    operator_context_id: String,
) -> anyhow::Result<()> {
    let config = ApiConfig::new(base_url, allow_cleartext).context("init_core")?;
    let session = SessionManager::new();
    let client = ApiClient::new(config, session, operator_context_id);
    let offline = OfflineQueue::open(&db_path).context("init_core: offline queue")?;
    let mut slot = runtime_slot().lock().expect("runtime mutex poisoned");
    *slot = Some(Arc::new(CoreRuntime { client, offline }));
    Ok(())
}

/// P0 FFI round-trip demo: sign a request with a fresh timestamp and nonce
/// and return the four anti-replay header values.
pub async fn ping_sign_demo(
    method: String,
    uri: String,
    body: Vec<u8>,
    secret: String,
) -> anyhow::Result<SignatureHeaders> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .context("system clock before UNIX_EPOCH")?
        .as_secs() as i64;
    let nonce = signing::fresh_nonce();
    Ok(signing::sign_request(&method, &uri, &body, &secret, timestamp, &nonce).into())
}

/// One SSE event; `data` is the raw JSON payload (parsed on the Dart side).
pub struct SseEvent {
    pub event: String,
    pub data: String,
}

/// Connection lifecycle state, surfaced for UI indicators.
pub enum SseConnectionState {
    Connecting,
    Connected,
    Disconnected { reason: String },
}

/// Item pushed to the Dart side of an SSE stream.
pub enum SseUpdate {
    State(SseConnectionState),
    Event(SseEvent),
}

impl From<sse::SseEvent> for SseEvent {
    fn from(e: sse::SseEvent) -> Self {
        Self {
            event: e.event,
            data: e.data,
        }
    }
}

impl From<sse::SseConnectionState> for SseConnectionState {
    fn from(s: sse::SseConnectionState) -> Self {
        match s {
            sse::SseConnectionState::Connecting => Self::Connecting,
            sse::SseConnectionState::Connected => Self::Connected,
            sse::SseConnectionState::Disconnected { reason } => {
                Self::Disconnected { reason }
            }
        }
    }
}

impl From<sse::SseUpdate> for SseUpdate {
    fn from(u: sse::SseUpdate) -> Self {
        match u {
            sse::SseUpdate::State(s) => Self::State(s.into()),
            sse::SseUpdate::Event(e) => Self::Event(e.into()),
        }
    }
}

/// Connect to the universal SSE stream (`GET /api/v2/sse/stream`, see
/// `sse.rs` docs) and forward every event / connection-state change to Dart
/// until the sink is dropped. The token comes from the restored session.
pub async fn notifications_stream(sink: StreamSink<SseUpdate>) -> anyhow::Result<()> {
    let rt = runtime()?;
    let bundle = rt
        .client
        .session()
        .current_token_bundle()
        .await
        .context("not logged in")?;
    let base_url = rt.client.config().base_url.clone();
    let mut rx = sse::connect_notifications_stream(&base_url, bundle.access_token);
    while let Some(update) = rx.recv().await {
        sink.add(update.into())
            .map_err(|e| anyhow::anyhow!("sse sink closed: {e:?}"))?;
    }
    Ok(())
}
