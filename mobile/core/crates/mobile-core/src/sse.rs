//! SSE connector (plan §0.4 / §3.4).
//!
//! Rust handles transport and heartbeat only — domain parsing happens on the
//! Dart side, so event payloads stay raw JSON strings here.
//!
//! Reconnect policy: exponential backoff 1s → 30s with ±20% jitter, reset on
//! a successful connect; a heartbeat timeout (>90s without any data) forces a
//! reconnect. Connection state transitions are emitted alongside events.

use std::time::Duration;

use eventsource_stream::Eventsource;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

use crate::signing::{self, SignatureHeaders};

/// Heartbeat timeout: reconnect when no data arrives for this long.
pub const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(90);
/// Initial reconnect backoff.
pub const BACKOFF_INITIAL: Duration = Duration::from_secs(1);
/// Backoff cap.
pub const BACKOFF_MAX: Duration = Duration::from_secs(30);

/// One SSE event. `data` is the raw JSON payload (parsed on the Dart side).
#[derive(Debug, Clone, PartialEq)]
pub struct SseEvent {
    pub event: String,
    pub data: String,
}

/// Connection lifecycle state, surfaced for UI indicators.
#[derive(Debug, Clone, PartialEq)]
pub enum SseConnectionState {
    Connecting,
    Connected,
    Disconnected { reason: String },
}

/// Item pushed to consumers of an SSE stream.
#[derive(Debug, Clone, PartialEq)]
pub enum SseUpdate {
    State(SseConnectionState),
    Event(SseEvent),
}

/// Reconnecting SSE connector. Use [`connect_notifications_stream`] or
/// [`connect_chat_stream`].
pub struct SseConnector {
    client: reqwest::Client,
    url: String,
    access_token: String,
    /// Session secret for signed connects. Reserved for endpoints that are
    /// NOT in the backend anti-replay skip-list; the current universal SSE
    /// endpoint is a skip path and does not need it.
    session_secret: Option<String>,
}

impl SseConnector {
    pub fn new(
        client: reqwest::Client,
        url: String,
        access_token: String,
        session_secret: Option<String>,
    ) -> Self {
        Self {
            client,
            url,
            access_token,
            session_secret,
        }
    }

    /// Spawn the connect/read/reconnect loop on the current tokio runtime and
    /// return the receiving end. Dropping the receiver stops the loop at the
    /// next send.
    pub fn start(self) -> mpsc::Receiver<SseUpdate> {
        let (tx, rx) = mpsc::channel(64);
        tokio::spawn(async move {
            let mut backoff = BACKOFF_INITIAL;
            loop {
                if tx
                    .send(SseUpdate::State(SseConnectionState::Connecting))
                    .await
                    .is_err()
                {
                    return;
                }
                match self.connect_once(&tx, &mut backoff).await {
                    Ok(()) => {
                        if tx
                            .send(SseUpdate::State(SseConnectionState::Disconnected {
                                reason: "stream ended".to_string(),
                            }))
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                    Err(reason) => {
                        if tx
                            .send(SseUpdate::State(SseConnectionState::Disconnected {
                                reason,
                            }))
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                }
                let sleep = Duration::from_secs_f64(backoff.as_secs_f64() * jitter_factor());
                tokio::time::sleep(sleep.min(BACKOFF_MAX)).await;
                backoff = (backoff * 2).min(BACKOFF_MAX);
            }
        });
        rx
    }

    /// One connect attempt: returns `Ok` when the stream ends cleanly, `Err`
    /// with a human-readable reason otherwise. Resets `backoff` once the
    /// connection is established.
    async fn connect_once(
        &self,
        tx: &mpsc::Sender<SseUpdate>,
        backoff: &mut Duration,
    ) -> Result<(), String> {
        let mut req = self
            .client
            .get(&self.url)
            .header("Accept", "text/event-stream")
            .header("Authorization", format!("Bearer {}", self.access_token));
        if let Some(secret) = &self.session_secret {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_secs() as i64;
            let nonce = signing::fresh_nonce();
            let path_and_query = path_and_query_of(&self.url);
            let SignatureHeaders {
                timestamp,
                nonce,
                body_sha256,
                signature,
            } = signing::sign_request("GET", &path_and_query, b"", secret, timestamp, &nonce);
            req = req
                .header("X-Request-Timestamp", timestamp)
                .header("X-Request-Nonce", nonce)
                .header("X-Request-Body-SHA256", body_sha256)
                .header("X-Request-Signature", signature);
        }

        let resp = self
            .client
            .execute(req.build().map_err(|e| e.to_string())?)
            .await
            .map_err(|e| format!("connect failed: {e}"))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(format!("HTTP {status}"));
        }

        if tx
            .send(SseUpdate::State(SseConnectionState::Connected))
            .await
            .is_err()
        {
            return Ok(());
        }
        *backoff = BACKOFF_INITIAL;

        let mut stream = resp.bytes_stream().eventsource();
        loop {
            match tokio::time::timeout(HEARTBEAT_TIMEOUT, stream.next()).await {
                Err(_) => return Err("heartbeat timeout".to_string()),
                Ok(None) => return Ok(()),
                Ok(Some(Err(e))) => return Err(format!("sse decode: {e}")),
                Ok(Some(Ok(ev))) => {
                    let update = SseUpdate::Event(SseEvent {
                        event: ev.event,
                        data: ev.data,
                    });
                    if tx.send(update).await.is_err() {
                        return Ok(());
                    }
                }
            }
        }
    }
}

/// Extract `path[?query]` from an absolute URL for signing.
fn path_and_query_of(url: &str) -> String {
    // The client controls base_url, so a manual split is sufficient and
    // avoids pulling in a URL parser.
    let without_scheme = url
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(url);
    match without_scheme.find('/') {
        Some(idx) => without_scheme[idx..].to_string(),
        None => "/".to_string(),
    }
}

/// Jitter factor in [0.8, 1.2], derived from a random UUID (keeps `rand` out
/// of the dependency tree).
fn jitter_factor() -> f64 {
    let byte = uuid::Uuid::new_v4().as_bytes()[0];
    // Clamp: f64 rounding can push the max to 1.2000000000000002.
    (0.8 + (f64::from(byte) / 255.0) * 0.4).min(1.2)
}

/// Stable client User-Agent lives in `client.rs` (the backend binds a
/// `ua_hash` claim into access tokens at login, so login, REST and SSE MUST
/// all send the same UA). Re-exported here for existing callers.
pub use crate::client::CLIENT_USER_AGENT;

fn default_client() -> reqwest::Client {
    reqwest::Client::builder()
        .use_rustls_tls()
        .user_agent(CLIENT_USER_AGENT)
        .build()
        .expect("default reqwest client must build")
}

/// Notifications realtime (plan §0.4, corrected against backend source).
///
/// The dedicated `/api/v2/notifications/stream` route is NOT mounted in the
/// current backend (`notification_stream` is dead code in
/// `routes/notifications/shared.rs`), and `/api/v2/dispatch-chat/stream` was
/// removed outright (`middleware/jwt.rs` comment: "已移除以与 Python 一致").
/// The live transport is the universal `GET /api/v2/sse/stream`
/// (`sse/handler.rs`), which automatically subscribes the caller to both
/// `user_notifications_{uid}` and `user_dispatch_chat_{uid}` — so one
/// connection carries chat + notification events. Bearer only, anti-replay
/// skip path, no signature headers.
pub fn connect_notifications_stream(
    base_url: &str,
    access_token: String,
) -> mpsc::Receiver<SseUpdate> {
    SseConnector::new(
        default_client(),
        format!("{}/api/v2/sse/stream", base_url.trim_end_matches('/')),
        access_token,
        None,
    )
    .start()
}

/// Chat realtime: same universal stream as [`connect_notifications_stream`]
/// (see its doc comment for why the dedicated chat path is gone). Kept as a
/// separate entry point so the P2 chat feature can demux by event name.
pub fn connect_chat_stream(base_url: &str, access_token: String) -> mpsc::Receiver<SseUpdate> {
    SseConnector::new(
        default_client(),
        format!("{}/api/v2/sse/stream", base_url.trim_end_matches('/')),
        access_token,
        None,
    )
    .start()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Minimal mock SSE server: replies to one HTTP request with the given
    /// SSE body, then closes the connection.
    async fn spawn_mock_sse(body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            // Drain the request headers (single read is enough for a test).
            let mut buf = [0u8; 4096];
            let _ = socket.read(&mut buf).await;
            let resp = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n{body}"
            );
            socket.write_all(resp.as_bytes()).await.unwrap();
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn receives_events_and_state_transitions() {
        let base = spawn_mock_sse(
            "event: initial\ndata: {\"seq\":1}\n\nevent: user_notification\ndata: {\"id\":7}\n\n",
        )
        .await;
        let connector = SseConnector::new(
            default_client(),
            format!("{base}/api/v2/sse/stream"),
            "token".to_string(),
            None,
        );
        let mut rx = connector.start();

        assert_eq!(
            rx.recv().await,
            Some(SseUpdate::State(SseConnectionState::Connecting))
        );
        assert_eq!(
            rx.recv().await,
            Some(SseUpdate::State(SseConnectionState::Connected))
        );
        assert_eq!(
            rx.recv().await,
            Some(SseUpdate::Event(SseEvent {
                event: "initial".to_string(),
                data: "{\"seq\":1}".to_string(),
            }))
        );
        assert_eq!(
            rx.recv().await,
            Some(SseUpdate::Event(SseEvent {
                event: "user_notification".to_string(),
                data: "{\"id\":7}".to_string(),
            }))
        );
        // Mock closes the connection → clean end → Disconnected.
        match rx.recv().await {
            Some(SseUpdate::State(SseConnectionState::Disconnected { .. })) => {}
            other => panic!("expected Disconnected, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn non_success_status_disconnects() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buf = [0u8; 4096];
                let _ = socket.read(&mut buf).await;
                socket
                    .write_all(b"HTTP/1.1 401 Unauthorized\r\ncontent-length: 0\r\n\r\n")
                    .await
                    .unwrap();
            }
        });
        let connector = SseConnector::new(
            default_client(),
            format!("http://{addr}/api/v2/sse/stream"),
            "bad-token".to_string(),
            None,
        );
        let mut rx = connector.start();
        assert_eq!(
            rx.recv().await,
            Some(SseUpdate::State(SseConnectionState::Connecting))
        );
        match rx.recv().await {
            Some(SseUpdate::State(SseConnectionState::Disconnected { reason })) => {
                assert!(reason.contains("401"), "unexpected reason: {reason}");
            }
            other => panic!("expected Disconnected, got {other:?}"),
        }
    }

    #[test]
    fn path_and_query_extraction() {
        assert_eq!(
            path_and_query_of("http://10.0.2.2:8000/api/v2/sse/stream?topics=flights"),
            "/api/v2/sse/stream?topics=flights"
        );
        assert_eq!(
            path_and_query_of("https://example.com/p?q=1"),
            "/p?q=1"
        );
        assert_eq!(path_and_query_of("https://example.com"), "/");
    }

    #[test]
    fn jitter_stays_within_bounds() {
        for _ in 0..100 {
            let f = jitter_factor();
            assert!((0.8..=1.2).contains(&f), "jitter out of bounds: {f}");
        }
    }

    /// Plan §6 P2: after the server closes, the connector must reconnect
    /// (Connecting → Connected again) and deliver a later event.
    #[tokio::test]
    async fn reconnects_after_server_closes() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            for n in 0..2 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buf = [0u8; 4096];
                let _ = socket.read(&mut buf).await;
                let body = format!("event: ping\ndata: {{\"n\":{n}}}\n\n");
                let resp = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n{body}"
                );
                let _ = socket.write_all(resp.as_bytes()).await;
            }
        });

        let connector = SseConnector::new(
            default_client(),
            format!("http://{addr}/api/v2/sse/stream"),
            "token".to_string(),
            None,
        );
        let mut rx = connector.start();

        let mut connected = 0u32;
        let mut events = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
        while tokio::time::Instant::now() < deadline && (connected < 2 || events.len() < 2) {
            match tokio::time::timeout(
                deadline.saturating_duration_since(tokio::time::Instant::now()),
                rx.recv(),
            )
            .await
            {
                Ok(Some(SseUpdate::State(SseConnectionState::Connected))) => connected += 1,
                Ok(Some(SseUpdate::Event(ev))) => events.push(ev.data),
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => break,
            }
        }

        assert!(
            connected >= 2,
            "expected reconnect to Connected twice, got {connected}; events={events:?}"
        );
        assert!(
            events.iter().any(|d| d.contains("\"n\":0"))
                && events.iter().any(|d| d.contains("\"n\":1")),
            "expected both ping payloads after reconnect, got {events:?}"
        );
    }
}
