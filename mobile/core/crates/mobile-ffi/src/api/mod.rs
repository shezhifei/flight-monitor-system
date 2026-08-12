//! Domain-level async exports visible to Dart (plan §4).
//!
//! P0 scope: `init_core` + `ping_sign_demo` (FFI round-trip smoke test)
//! + `notifications_stream` (SSE connector smoke test, plan P0 task 5).

use std::sync::{Mutex, OnceLock};

use anyhow::Context;
use crate::frb_generated::StreamSink;
use mobile_core::sse;
use mobile_core::signing;
use mobile_core::ApiConfig;

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

/// Holds the validated config + offline DB path. Fields are consumed by the
/// HTTP client / offline queue once those land in P1.
#[allow(dead_code)]
struct CoreRuntime {
    config: ApiConfig,
    db_path: String,
}

static RUNTIME: OnceLock<Mutex<Option<CoreRuntime>>> = OnceLock::new();

fn runtime_slot() -> &'static Mutex<Option<CoreRuntime>> {
    RUNTIME.get_or_init(|| Mutex::new(None))
}

/// Initialize the core. Must be called once before any other export.
/// Re-initialization is allowed (replaces the previous config).
pub async fn init_core(
    base_url: String,
    allow_cleartext: bool,
    db_path: String,
) -> anyhow::Result<()> {
    let config = ApiConfig::new(base_url, allow_cleartext).context("init_core")?;
    let mut slot = runtime_slot().lock().expect("runtime mutex poisoned");
    *slot = Some(CoreRuntime { config, db_path });
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

/// P0 demo: connect to `GET /api/v2/notifications/stream` and forward every
/// event / connection-state change to Dart until the sink is dropped.
/// The token is supplied manually for now; the session state machine and the
/// signed chat stream arrive in P1.
pub async fn notifications_stream(
    sink: StreamSink<SseUpdate>,
    base_url: String,
    access_token: String,
) -> anyhow::Result<()> {
    let mut rx = sse::connect_notifications_stream(&base_url, access_token);
    while let Some(update) = rx.recv().await {
        sink.add(update.into())
            .map_err(|e| anyhow::anyhow!("sse sink closed: {e:?}"))?;
    }
    Ok(())
}
