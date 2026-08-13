//! HTTP client with the full request pipeline (auth, signing, retries).
//!
//! Per-request pipeline:
//! 1. fixed headers: `X-Client-Surface: native`,
//!    `X-Operator-Context-Type: mobile_device`,
//!    `X-Operator-Context-Id: {device_id}`;
//! 2. `Authorization: Bearer` from the session state machine;
//! 3. anti-replay signature headers — skipped for public/stream paths
//!    ([`is_public_path`] / [`is_stream_path`], mirrored byte-for-byte from
//!    `services/api-server/crates/api/src/middleware/anti_replay.rs`);
//! 4. send; on 401 run the session's single-flight refresh and retry ONCE.
//!
//! User-Agent is fixed ([`CLIENT_USER_AGENT`]): the backend binds a `ua_hash`
//! claim into access tokens at login, so login and every subsequent request
//! MUST send the same UA or the token is rejected (verified on a live
//! backend). `sse.rs` references this constant.
//!
//! Body hashes use the streaming `sha2` hasher; for multipart uploads the
//! hash is computed incrementally while the body is assembled.

use std::sync::OnceLock;

use serde::de::DeserializeOwned;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::config::ApiConfig;
use crate::dto::GenericApiResponse;
use crate::error::CoreError;
use crate::session::SessionManager;
use crate::signing::{self, SignatureHeaders};

/// Stable client User-Agent. The backend binds a `ua_hash` claim into access
/// tokens at login (`middleware/jwt.rs::validate_client_context`), so login
/// and every subsequent request MUST send the same UA.
pub const CLIENT_USER_AGENT: &str = "FlightMonitorMobile/0.1 (Android; flutter)";

// Fixed header names.
pub const HEADER_CLIENT_SURFACE: &str = "X-Client-Surface";
pub const HEADER_OPERATOR_CONTEXT_TYPE: &str = "X-Operator-Context-Type";
pub const HEADER_OPERATOR_CONTEXT_ID: &str = "X-Operator-Context-Id";

const CLIENT_SURFACE_NATIVE: &str = "native";
const OPERATOR_CONTEXT_TYPE_MOBILE: &str = "mobile_device";

/// Process-wide `reqwest::Client` singleton (rustls, connection pooling,
/// fixed UA). One global instance.
pub fn shared_http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .use_rustls_tls()
            .user_agent(CLIENT_USER_AGENT)
            .build()
            .expect("shared reqwest client must build")
    })
}

/// Mirror of the backend `is_public_path` (`anti_replay.rs` L71-101). Must
/// stay byte-for-byte identical: these paths skip anti-replay signing.
pub fn is_public_path(path: &str) -> bool {
    let exact_skips = [
        "/api/auth/login",
        "/api/auth/register",
        "/api/auth/refresh",
        "/api/v2/auth/login",
        "/api/v2/auth/register",
        "/api/v2/auth/refresh",
        "/api/ping",
        "/api/v2/ping",
        "/api/v2/health/ping",
        "/api/v2/system/runtime/health/ping",
        "/api/v2/system/health",
        "/",
        "/favicon.ico",
    ];
    if exact_skips.contains(&path) {
        return true;
    }
    let prefixes = [
        "/docs",
        "/openapi.json",
        "/redoc",
        "/frontend",
        "/static",
        "/pics",
        "/css",
        "/js",
    ];
    prefixes.iter().any(|prefix| path.starts_with(prefix))
}

/// Mirror of the backend `is_stream_path` (`anti_replay.rs` L103-114). Note:
/// `/api/v2/dispatch-chat/stream` is deliberately NOT in this list.
pub fn is_stream_path(path: &str) -> bool {
    let prefixes = [
        "/api/v2/sse/stream",
        "/api/v2/flights/stream",
        "/api/v2/ai/events/stream",
        "/api/v2/notifications/stream",
        "/api/v2/dispatch/collaboration/stream",
        "/api/v2/kpi/stream",
        "/api/v2/anomalies/stream",
    ];
    prefixes.iter().any(|prefix| path.starts_with(prefix))
}

/// Strip the query string before path classification (the backend compares
/// the path component only).
fn path_only(path_and_query: &str) -> &str {
    path_and_query.split('?').next().unwrap_or(path_and_query)
}

/// Whether anti-replay signing applies to this path.
fn signing_required(path_and_query: &str) -> bool {
    let path = path_only(path_and_query);
    !is_public_path(path) && !is_stream_path(path)
}

/// Authenticated API client. One instance per app runtime; holds the session
/// state machine and the device id used for the operator-context header.
#[derive(Clone)]
pub struct ApiClient {
    config: ApiConfig,
    http: reqwest::Client,
    session: SessionManager,
    device_id: String,
}

impl ApiClient {
    pub fn new(config: ApiConfig, session: SessionManager, device_id: impl Into<String>) -> Self {
        Self {
            config,
            http: shared_http_client().clone(),
            session,
            device_id: device_id.into(),
        }
    }

    pub fn config(&self) -> &ApiConfig {
        &self.config
    }

    pub fn session(&self) -> &SessionManager {
        &self.session
    }

    /// The stable device id sent as `X-Operator-Context-Id`; also used
    /// as the `device_id` for device register/heartbeat endpoints.
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    fn url(&self, path_and_query: &str) -> String {
        format!("{}{}", self.config.base_url, path_and_query)
    }

    /// Call an endpoint that returns the `GenericApiResponse<T>` envelope.
    /// `success=false` maps to [`CoreError::Api`] carrying the
    /// server message and request_id; a missing `data` on success is a
    /// serialization error.
    pub async fn call_with_envelope<T, B>(
        &self,
        method: &str,
        path_and_query: &str,
        body: Option<&B>,
    ) -> Result<T, CoreError>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        let bytes = self.execute(method, path_and_query, body, None).await?;
        let envelope: GenericApiResponse<T> = serde_json::from_slice(&bytes)?;
        if !envelope.success {
            return Err(CoreError::Api {
                message: envelope
                    .message
                    .or_else(|| envelope.error.map(|e| e.to_string()))
                    .unwrap_or_else(|| "request failed".to_string()),
                request_id: envelope.request_id,
            });
        }
        envelope.data.ok_or_else(|| {
            CoreError::Serialization("envelope success but data is null".to_string())
        })
    }

    /// Call an endpoint that returns a bare JSON object (no envelope).
    pub async fn call_raw<T, B>(
        &self,
        method: &str,
        path_and_query: &str,
        body: Option<&B>,
    ) -> Result<T, CoreError>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        let bytes = self.execute(method, path_and_query, body, None).await?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// Multipart upload `POST /api/v2/mobile/uploads` (fields `file` +
    /// `category`). The multipart body is assembled once (reqwest
    /// sends it from memory) and its SHA-256 is computed INCREMENTALLY while
    /// assembling — never hashed as one big buffer afterwards.
    pub async fn upload(
        &self,
        category: &str,
        filename: &str,
        content_type: &str,
        bytes: &[u8],
    ) -> Result<crate::dto::mobile::MobileUploadAsset, CoreError> {
        let boundary = format!("fms{}", uuid::Uuid::new_v4().simple());
        let mut hasher = Sha256::new();
        let mut body: Vec<u8> = Vec::with_capacity(bytes.len() + 512);
        let mut push = |chunk: &[u8]| {
            hasher.update(chunk);
            body.extend_from_slice(chunk);
        };
        push(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"category\"\r\n\r\n{category}\r\n"
            )
            .as_bytes(),
        );
        push(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: {content_type}\r\n\r\n"
            )
            .as_bytes(),
        );
        push(bytes);
        push(format!("\r\n--{boundary}--\r\n").as_bytes());

        let response_bytes = self
            .execute_bytes(
                "POST",
                "/api/v2/mobile/uploads",
                body,
                Some(hex::encode(hasher.finalize())),
                Some(format!("multipart/form-data; boundary={boundary}")),
            )
            .await?;
        let envelope: GenericApiResponse<crate::dto::mobile::MobileUploadAsset> =
            serde_json::from_slice(&response_bytes)?;
        if !envelope.success {
            return Err(CoreError::Api {
                message: envelope
                    .message
                    .unwrap_or_else(|| "upload failed".to_string()),
                request_id: envelope.request_id,
            });
        }
        envelope.data.ok_or_else(|| {
            CoreError::Serialization("upload envelope success but data is null".to_string())
        })
    }

    /// Serialize a JSON body and run the pipeline.
    async fn execute<B>(
        &self,
        method: &str,
        path_and_query: &str,
        body: Option<&B>,
        body_hash_override: Option<String>,
    ) -> Result<Vec<u8>, CoreError>
    where
        B: Serialize + ?Sized,
    {
        let (bytes, content_type) = match body {
            Some(b) => (
                serde_json::to_vec(b)?,
                Some("application/json".to_string()),
            ),
            None => (Vec::new(), None),
        };
        self.execute_bytes(method, path_and_query, bytes, body_hash_override, content_type)
            .await
    }

    /// Full pipeline: fixed headers → Bearer → sign → send → on 401 refresh
    /// (single-flight) and retry once.
    async fn execute_bytes(
        &self,
        method: &str,
        path_and_query: &str,
        body: Vec<u8>,
        body_hash_override: Option<String>,
        content_type: Option<String>,
    ) -> Result<Vec<u8>, CoreError> {
        if (method.eq_ignore_ascii_case("GET") || method.eq_ignore_ascii_case("HEAD"))
            && !body.is_empty()
        {
            return Err(CoreError::BodyNotAllowed);
        }

        // Proactive refresh when we hold tokens; public paths may run
        // anonymous, anything else requires a session.
        let mut bundle = match self.session.current_token_bundle().await {
            Some(_) => Some(
                self.session
                    .ensure_valid(&self.http, &self.config.base_url)
                    .await?,
            ),
            None if !signing_required(path_and_query) => None,
            None => return Err(CoreError::Auth("not logged in".into())),
        };

        let mut attempt = 0u8;
        loop {
            attempt += 1;
            let request = self.build_request(
                method,
                path_and_query,
                &body,
                body_hash_override.as_deref(),
                content_type.as_deref(),
                bundle.as_ref(),
            )?;
            let response = self
                .http
                .execute(request)
                .await
                .map_err(|e| CoreError::Network(format!("{method} {path_and_query}: {e}")))?;

            if response.status() != reqwest::StatusCode::UNAUTHORIZED {
                return self.decode_response(response).await;
            }

            // 401: exactly one single-flight refresh + retry.
            let Some(stale) = bundle.take() else {
                return self.decode_response(response).await;
            };
            if attempt > 1 {
                // Already retried once after a refresh — surface the 401.
                return self.decode_response(response).await;
            }
            tracing::debug!("client: 401 on {method} {path_and_query}, single-flight refresh + retry");
            bundle = Some(
                self.session
                    .refresh_single_flight(&self.http, &self.config.base_url, Some(&stale.access_token))
                    .await?,
            );
        }
    }

    fn build_request(
        &self,
        method: &str,
        path_and_query: &str,
        body: &[u8],
        body_hash_override: Option<&str>,
        content_type: Option<&str>,
        bundle: Option<&crate::session::TokenBundle>,
    ) -> Result<reqwest::Request, CoreError> {
        let method_parsed = method
            .parse::<reqwest::Method>()
            .map_err(|e| CoreError::InvalidConfig(format!("bad method {method}: {e}")))?;
        let mut builder = self
            .http
            .request(method_parsed, self.url(path_and_query))
            .header(HEADER_CLIENT_SURFACE, CLIENT_SURFACE_NATIVE)
            .header(HEADER_OPERATOR_CONTEXT_TYPE, OPERATOR_CONTEXT_TYPE_MOBILE)
            .header(HEADER_OPERATOR_CONTEXT_ID, &self.device_id);
        if let Some(ct) = content_type {
            builder = builder.header(reqwest::header::CONTENT_TYPE, ct);
        }
        if let Some(bundle) = bundle {
            builder = builder.header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", bundle.access_token),
            );
            if signing_required(path_and_query) {
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|e| CoreError::Network(e.to_string()))?
                    .as_secs() as i64;
                let nonce = signing::fresh_nonce();
                let SignatureHeaders {
                    timestamp,
                    nonce,
                    body_sha256,
                    signature,
                } = match body_hash_override {
                    Some(precomputed) => {
                        // Same payload format as signing.rs, but with the
                        // incrementally-computed body hash (multipart upload).
                        sign_with_body_hash(
                            method,
                            path_and_query,
                            precomputed,
                            &bundle.session_secret,
                            timestamp,
                            &nonce,
                        )
                    }
                    None => signing::sign_request(
                        method,
                        path_and_query,
                        body,
                        &bundle.session_secret,
                        timestamp,
                        &nonce,
                    ),
                };
                builder = builder
                    .header("X-Request-Timestamp", timestamp)
                    .header("X-Request-Nonce", nonce)
                    .header("X-Request-Body-SHA256", body_sha256)
                    .header("X-Request-Signature", signature);
            }
        }
        if !body.is_empty() {
            builder = builder.body(body.to_vec());
        }
        builder
            .build()
            .map_err(|e| CoreError::Network(format!("build request: {e}")))
    }

    /// Map a non-retryable response to bytes or a typed error. Error bodies
    /// are parsed for `message`/`request_id` when they carry the envelope.
    async fn decode_response(&self, response: reqwest::Response) -> Result<Vec<u8>, CoreError> {
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|e| CoreError::Network(format!("read body: {e}")))?;
        if status.is_success() {
            return Ok(bytes.to_vec());
        }
        let parsed = serde_json::from_slice::<GenericApiResponse<serde_json::Value>>(&bytes).ok();
        let message = parsed
            .as_ref()
            .and_then(|e| e.message.clone().or_else(|| e.error.as_ref().map(|v| v.to_string())))
            .unwrap_or_else(|| format!("HTTP {status}"));
        let request_id = parsed.as_ref().and_then(|e| e.request_id.clone());
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(CoreError::Auth(format!("unauthorized: {message}")));
        }
        Err(CoreError::Api { message, request_id })
    }
}

/// Sign with a precomputed (incrementally hashed) body hash. Payload format
/// identical to `signing::sign_request` — see its doc comment.
fn sign_with_body_hash(
    method: &str,
    path_and_query: &str,
    body_hash: &str,
    session_secret: &str,
    timestamp: i64,
    nonce: &str,
) -> SignatureHeaders {
    use hmac::{Hmac, Mac};
    let payload = format!(
        "{}:{}:{}:{}:{}",
        method.to_uppercase(),
        path_and_query,
        timestamp,
        nonce,
        body_hash
    );
    let mut mac = Hmac::<Sha256>::new_from_slice(session_secret.as_bytes())
        .expect("HMAC-SHA256 accepts keys of any length");
    mac.update(payload.as_bytes());
    SignatureHeaders {
        timestamp: timestamp.to_string(),
        nonce: nonce.to_string(),
        body_sha256: body_hash.to_string(),
        signature: hex::encode(mac.finalize().into_bytes()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::TokenBundle;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;
    use tokio::sync::Mutex;

    #[derive(Debug, Clone)]
    struct CapturedRequest {
        method: String,
        path: String,
        headers: HashMap<String, String>,
        body: Vec<u8>,
    }

    /// Route table: exact path → sequence of (status, body) responses. Each
    /// hit pops the next response (last one repeats).
    type Routes = Arc<Mutex<HashMap<String, Vec<(u16, String)>>>>;

    struct MockServer {
        base_url: String,
        captured: Arc<Mutex<Vec<CapturedRequest>>>,
        refresh_hits: Arc<AtomicUsize>,
    }

    async fn spawn_mock(routes: Vec<(&str, Vec<(u16, &str)>)>) -> MockServer {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let captured = Arc::new(Mutex::new(Vec::new()));
        let refresh_hits = Arc::new(AtomicUsize::new(0));
        let routes: Routes = Arc::new(Mutex::new(
            routes
                .into_iter()
                .map(|(k, v)| {
                    (
                        k.to_string(),
                        v.into_iter()
                            .map(|(s, b)| (s, b.to_string()))
                            .collect::<Vec<_>>(),
                    )
                })
                .collect(),
        ));
        {
            let captured = Arc::clone(&captured);
            let refresh_hits = Arc::clone(&refresh_hits);
            let routes = Arc::clone(&routes);
            tokio::spawn(async move {
                loop {
                    let (mut socket, _) = listener.accept().await.unwrap();
                    let captured = Arc::clone(&captured);
                    let refresh_hits = Arc::clone(&refresh_hits);
                    let routes = Arc::clone(&routes);
                    tokio::spawn(async move {
                        let (read_half, mut write_half) = socket.split();
                        let mut reader = BufReader::new(read_half);
                        let mut request_line = String::new();
                        if reader.read_line(&mut request_line).await.unwrap_or(0) == 0 {
                            return;
                        }
                        let mut parts = request_line.split_whitespace();
                        let method = parts.next().unwrap_or("").to_string();
                        let path = parts.next().unwrap_or("").to_string();
                        let mut headers = HashMap::new();
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
                                let name = name.trim().to_ascii_lowercase();
                                let value = value.trim().to_string();
                                if name == "content-length" {
                                    content_length = value.parse().unwrap_or(0);
                                }
                                headers.insert(name, value);
                            }
                        }
                        let mut body = vec![0u8; content_length];
                        if content_length > 0
                            && reader.read_exact(&mut body).await.is_err()
                        {
                            return;
                        }
                        if path == "/api/v2/auth/refresh" {
                            refresh_hits.fetch_add(1, Ordering::SeqCst);
                        }
                        captured.lock().await.push(CapturedRequest {
                            method: method.clone(),
                            path: path.clone(),
                            headers,
                            body,
                        });
                        let mut table = routes.lock().await;
                        let path_key = path.split('?').next().unwrap_or(&path).to_string();
                        let (status, resp_body) = match table.get_mut(&path_key) {
                            Some(seq) if seq.len() > 1 => seq.remove(0),
                            Some(seq) if !seq.is_empty() => seq[0].clone(),
                            _ => (404, r#"{"success":false,"message":"not found"}"#.to_string()),
                        };
                        drop(table);
                        let reason = match status {
                            200 => "OK",
                            401 => "Unauthorized",
                            _ => "Error",
                        };
                        let response = format!(
                            "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{resp_body}",
                            resp_body.len()
                        );
                        let _ = write_half.write_all(response.as_bytes()).await;
                    });
                }
            });
        }
        MockServer {
            base_url: format!("http://{addr}"),
            captured,
            refresh_hits,
        }
    }

    fn now_epoch() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    fn make_client(base_url: &str) -> (ApiClient, SessionManager) {
        let session = SessionManager::new();
        let client = ApiClient::new(
            ApiConfig::new(base_url, true).unwrap(),
            session.clone(),
            "test-device-001",
        );
        (client, session)
    }

    fn active_bundle() -> TokenBundle {
        TokenBundle {
            access_token: "access-1".to_string(),
            refresh_token: "refresh-1".to_string(),
            session_secret: "deadbeef".to_string(),
            access_expire_at: now_epoch() + 3600,
        }
    }

    #[test]
    fn path_classification_matches_backend_lists() {
        // Public: exact + prefix matches (anti_replay.rs L71-101).
        for p in [
            "/api/v2/auth/login",
            "/api/v2/auth/register",
            "/api/v2/auth/refresh",
            "/api/ping",
            "/api/v2/ping",
            "/api/v2/health/ping",
            "/api/v2/system/runtime/health/ping",
            "/api/v2/system/health",
            "/",
            "/favicon.ico",
            "/docs",
            "/docs/anything",
            "/static/app.js",
        ] {
            assert!(is_public_path(p), "public: {p}");
            assert!(!signing_required(p), "no signing: {p}");
        }
        // Stream paths (anti_replay.rs L103-114).
        for p in [
            "/api/v2/sse/stream",
            "/api/v2/flights/stream",
            "/api/v2/ai/events/stream",
            "/api/v2/notifications/stream",
            "/api/v2/dispatch/collaboration/stream",
            "/api/v2/kpi/stream",
            "/api/v2/anomalies/stream",
        ] {
            assert!(is_stream_path(p), "stream: {p}");
            assert!(!signing_required(p), "no signing: {p}");
        }
        // dispatch-chat is deliberately NOT skipped.
        assert!(signing_required("/api/v2/dispatch-chat/stream"));
        assert!(signing_required("/api/v2/mobile/workbench"));
        assert!(signing_required("/api/v2/dispatch-orders/my"));
        // Query strings are stripped before classification.
        assert!(!signing_required("/api/v2/sse/stream?topics=flights"));
        assert!(signing_required("/api/v2/mobile/workbench?x=1"));
        // Prefix matching is `starts_with` in the backend too, so trailing
        // junk still counts as a stream path there — keep behavior identical.
        assert!(!signing_required("/api/v2/flights/streamx"));
        // Close-but-different public path: exact list, no prefix → signed.
        assert!(signing_required("/api/v2/auth/loginx"));
    }

    #[tokio::test]
    async fn injects_fixed_bearer_and_signature_headers() {
        let server = spawn_mock(vec![(
            "/api/v2/protected",
            vec![(200, r#"{"value":42}"#)],
        )])
        .await;
        let (client, session) = make_client(&server.base_url);
        session.restore_tokens(active_bundle()).await;

        let value: serde_json::Value = client
            .call_raw("POST", "/api/v2/protected", Some(&serde_json::json!({"a":1})))
            .await
            .unwrap();
        assert_eq!(value["value"], 42);

        let captured = server.captured.lock().await;
        let req = &captured[0];
        assert_eq!(req.method, "POST");
        assert_eq!(req.headers["x-client-surface"], "native");
        assert_eq!(req.headers["x-operator-context-type"], "mobile_device");
        assert_eq!(req.headers["x-operator-context-id"], "test-device-001");
        assert_eq!(req.headers["authorization"], "Bearer access-1");
        // Recompute the signature with the locked signing.rs algorithm using
        // the captured timestamp/nonce/body-hash.
        let expected = signing::sign_request(
            "POST",
            "/api/v2/protected",
            br#"{"a":1}"#,
            "deadbeef",
            req.headers["x-request-timestamp"].parse().unwrap(),
            &req.headers["x-request-nonce"],
        );
        assert_eq!(req.headers["x-request-body-sha256"], expected.body_sha256);
        assert_eq!(req.headers["x-request-signature"], expected.signature);
    }

    #[tokio::test]
    async fn get_uses_empty_body_hash() {
        let server = spawn_mock(vec![("/api/v2/protected", vec![(200, r#"{"ok":true}"#)])]).await;
        let (client, session) = make_client(&server.base_url);
        session.restore_tokens(active_bundle()).await;

        let _: serde_json::Value = client
            .call_raw::<serde_json::Value, ()>("GET", "/api/v2/protected?x=1", None)
            .await
            .unwrap();
        let captured = server.captured.lock().await;
        let req = &captured[0];
        assert_eq!(
            req.headers["x-request-body-sha256"],
            signing::EMPTY_BODY_SHA256
        );
        let expected = signing::sign_request(
            "GET",
            "/api/v2/protected?x=1",
            b"",
            "deadbeef",
            req.headers["x-request-timestamp"].parse().unwrap(),
            &req.headers["x-request-nonce"],
        );
        assert_eq!(req.headers["x-request-signature"], expected.signature);
    }

    #[tokio::test]
    async fn public_path_skips_signing() {
        let server = spawn_mock(vec![(
            "/api/v2/auth/login",
            vec![(
                200,
                r#"{"access_token":"a","token_type":"bearer","expires_in":3600,"refresh_token":"r","sse_token":null,"sse_expires_in":null,"session_secret":"s"}"#,
            )],
        )])
        .await;
        let (client, _session) = make_client(&server.base_url);
        // Anonymous login call must work and carry no signature headers.
        let _: serde_json::Value = client
            .call_raw(
                "POST",
                "/api/v2/auth/login",
                Some(&serde_json::json!({"username":"u","password":"p"})),
            )
            .await
            .unwrap();
        let captured = server.captured.lock().await;
        let req = &captured[0];
        assert_eq!(req.headers["x-client-surface"], "native");
        assert!(!req.headers.contains_key("x-request-signature"));
        assert!(!req.headers.contains_key("authorization"));
    }

    #[tokio::test]
    async fn retry_after_401_via_single_flight_refresh() {
        let server = spawn_mock(vec![
            (
                "/api/v2/auth/refresh",
                vec![(
                    200,
                    r#"{"access_token":"access-2","token_type":"bearer","expires_in":3600,"refresh_token":"refresh-2","sse_token":null,"sse_expires_in":null,"session_secret":"cafe"}"#,
                )],
            ),
            (
                "/api/v2/data",
                vec![(401, r#"{"success":false,"message":"expired"}"#), (200, r#"{"value":7}"#)],
            ),
        ])
        .await;
        let (client, session) = make_client(&server.base_url);
        session.restore_tokens(active_bundle()).await;

        let value: serde_json::Value = client
            .call_raw::<serde_json::Value, ()>("GET", "/api/v2/data", None)
            .await
            .unwrap();
        assert_eq!(value["value"], 7);
        assert_eq!(server.refresh_hits.load(Ordering::SeqCst), 1);

        let captured = server.captured.lock().await;
        let data_reqs: Vec<_> = captured
            .iter()
            .filter(|r| r.path == "/api/v2/data")
            .collect();
        assert_eq!(data_reqs.len(), 2);
        assert_eq!(data_reqs[0].headers["authorization"], "Bearer access-1");
        // Retried with the NEW token and signed with the NEW session_secret.
        assert_eq!(data_reqs[1].headers["authorization"], "Bearer access-2");
        let expected = signing::sign_request(
            "GET",
            "/api/v2/data",
            b"",
            "cafe",
            data_reqs[1].headers["x-request-timestamp"].parse().unwrap(),
            &data_reqs[1].headers["x-request-nonce"],
        );
        assert_eq!(data_reqs[1].headers["x-request-signature"], expected.signature);
        // Session state now holds the refreshed bundle.
        let bundle = session.current_token_bundle().await.unwrap();
        assert_eq!(bundle.access_token, "access-2");
        assert_eq!(bundle.session_secret, "cafe");
    }

    #[tokio::test]
    async fn refresh_401_clears_state_and_returns_auth_error() {
        let server = spawn_mock(vec![
            (
                "/api/v2/auth/refresh",
                vec![(401, r#"{"detail":"refresh invalid"}"#)],
            ),
            (
                "/api/v2/data",
                vec![(401, r#"{"success":false,"message":"expired"}"#)],
            ),
        ])
        .await;
        let (client, session) = make_client(&server.base_url);
        session.restore_tokens(active_bundle()).await;

        let err = client
            .call_raw::<serde_json::Value, ()>("GET", "/api/v2/data", None)
            .await
            .unwrap_err();
        assert!(matches!(err, CoreError::Auth(_)), "unexpected: {err:?}");
        assert_eq!(session.current_token_bundle().await, None);
        assert_eq!(server.refresh_hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn envelope_success_false_maps_to_api_error() {
        let server = spawn_mock(vec![(
            "/api/v2/mobile/workbench",
            vec![(
                200,
                r#"{"success":false,"data":null,"message":"boom","error":null,"request_id":"req-9"}"#,
            )],
        )])
        .await;
        let (client, session) = make_client(&server.base_url);
        session.restore_tokens(active_bundle()).await;

        let err = client
            .call_with_envelope::<serde_json::Value, ()>("GET", "/api/v2/mobile/workbench", None)
            .await
            .unwrap_err();
        match err {
            CoreError::Api { message, request_id } => {
                assert_eq!(message, "boom");
                assert_eq!(request_id.as_deref(), Some("req-9"));
            }
            other => panic!("expected Api error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn upload_sends_multipart_and_signed_headers() {
        let server = spawn_mock(vec![(
            "/api/v2/mobile/uploads",
            vec![(
                200,
                r#"{"success":true,"data":{"upload_id":"u1","original_filename":"a.txt","content_type":"text/plain","file_size":5,"checksum_sha256":null,"created_at":"2024-01-01T00:00:00Z","attachment_url":"/x","metadata":{}},"message":"ok","error":null,"request_id":null}"#,
            )],
        )])
        .await;
        let (client, session) = make_client(&server.base_url);
        session.restore_tokens(active_bundle()).await;

        let asset = client
            .upload("dispatch_issue", "a.txt", "text/plain", b"hello")
            .await
            .unwrap();
        assert_eq!(asset.upload_id, "u1");

        let captured = server.captured.lock().await;
        let req = &captured[0];
        assert!(req.headers["content-type"].starts_with("multipart/form-data; boundary="));
        // Signed with the incrementally computed hash of the exact wire body.
        let expected_hash = signing::body_hash_hex(&req.body);
        assert_eq!(req.headers["x-request-body-sha256"], expected_hash);
        let body = String::from_utf8_lossy(&req.body);
        assert!(body.contains("name=\"category\""));
        assert!(body.contains("dispatch_issue"));
        assert!(body.contains("name=\"file\"; filename=\"a.txt\""));
        assert!(body.contains("hello"));
    }

    #[tokio::test]
    async fn get_with_body_is_rejected() {
        let (client, session) = make_client("http://127.0.0.1:1");
        session.restore_tokens(active_bundle()).await;
        let err = client
            .call_raw::<serde_json::Value, _>("GET", "/api/v2/x", Some(&serde_json::json!({"a":1})))
            .await
            .unwrap_err();
        assert!(matches!(err, CoreError::BodyNotAllowed));
    }
}
