#![cfg(test)]

use actix_web::{http::header, web, App, HttpRequest, HttpResponse, HttpServer};
use futures_util::future::{AbortHandle, Abortable};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub(crate) struct FakeSidecarRequest {
    pub method: String,
    pub path: String,
    pub has_service_identity: bool,
    pub service_identity_token: Option<String>,
    pub body: Value,
}

#[cfg(test)]
pub(crate) fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .and_then(Path::parent)
        .expect("repository root")
        .to_path_buf()
}

#[cfg(not(test))]
pub(crate) fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .and_then(Path::parent)
        .expect("repository root")
        .to_path_buf()
}

pub(crate) fn workspace_python_executable() -> PathBuf {
    repository_root().join(".venv").join("Scripts").join("python.exe")
}

pub(crate) fn temp_json_path(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{unique}.json"))
}

pub(crate) fn load_python_runtime_parity_fixtures() -> Value {
    let repo_root = repository_root();
    let python_exe = workspace_python_executable();
    let script = repo_root
        .join("scripts")
        .join("tools")
        .join("export_python_runtime_parity_fixtures.py");
    let output_path = temp_json_path("python-runtime-parity-fixtures");

    let output = Command::new(&python_exe)
        .arg(&script)
        .arg("--output")
        .arg(&output_path)
        .current_dir(&repo_root)
        .output()
        .unwrap_or_else(|error| panic!("failed to run python runtime parity fixture exporter: {error}"));

    assert!(
        output.status.success(),
        "python runtime parity fixture exporter failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let raw = fs::read_to_string(&output_path)
        .unwrap_or_else(|error| panic!("failed to read python runtime parity fixtures: {error}"));
    let _ = fs::remove_file(&output_path);

    serde_json::from_str(&raw).unwrap_or_else(|error| panic!("invalid python runtime parity fixture json: {error}"))
}

/// Load the shared Rust/Python contract fixture from tests/fixtures/runtime_contract.json.
/// This fixture contains a canonical ContextEnvelope and AiStructuredOutput that both
/// sides agree on, preventing schema drift.
pub(crate) fn load_shared_runtime_contract_fixture() -> Value {
    let path = repository_root()
        .join("services")
        .join("ai-sidecar")
        .join("tests")
        .join("fixtures")
        .join("runtime_contract.json");
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read shared runtime contract fixture: {error}"));
    serde_json::from_str(&raw).unwrap_or_else(|error| panic!("invalid shared runtime contract fixture json: {error}"))
}

/// Load the shared cross-language contract field manifest from
/// tests/fixtures/contract_field_manifest.json. This manifest enumerates the wire
/// field set of every contract type; both the Rust round-trip test and the Python
/// introspection test assert against it so field drift on either side fails CI.
pub(crate) fn load_contract_field_manifest() -> Value {
    let path = repository_root()
        .join("services")
        .join("ai-sidecar")
        .join("tests")
        .join("fixtures")
        .join("contract_field_manifest.json");
    let raw =
        fs::read_to_string(&path).unwrap_or_else(|error| panic!("failed to read contract field manifest: {error}"));
    serde_json::from_str(&raw).unwrap_or_else(|error| panic!("invalid contract field manifest json: {error}"))
}

pub(crate) fn normalize_runtime_parity_value(value: &mut Value) {
    normalize_runtime_parity_value_for_key(None, value);
}

fn normalize_runtime_parity_value_for_key(key: Option<&str>, value: &mut Value) {
    match value {
        Value::Object(map) => {
            let keys = map.keys().cloned().collect::<Vec<_>>();
            for child_key in keys {
                if let Some(child_value) = map.get_mut(&child_key) {
                    if is_timestamp_key(&child_key) {
                        if !child_value.is_null() {
                            *child_value = Value::String("<timestamp>".to_string());
                        }
                        continue;
                    }
                    if is_duration_key(&child_key) {
                        if !child_value.is_null() {
                            *child_value = Value::String("<duration>".to_string());
                        }
                        continue;
                    }
                    normalize_runtime_parity_value_for_key(Some(child_key.as_str()), child_value);
                }
            }
        }
        Value::Array(items) => {
            for item in items.iter_mut() {
                normalize_runtime_parity_value_for_key(None, item);
            }

            match key {
                Some("subscriptions") => {
                    items.sort_by(|left, right| {
                        left.as_str()
                            .unwrap_or_default()
                            .cmp(right.as_str().unwrap_or_default())
                    });
                }
                Some("client_buffers") | Some("connection_details") => {
                    items.sort_by(|left, right| {
                        let left_id = left.get("client_id").and_then(Value::as_str).unwrap_or_default();
                        let right_id = right.get("client_id").and_then(Value::as_str).unwrap_or_default();
                        left_id.cmp(right_id)
                    });
                }
                _ => {}
            }
        }
        _ => {}
    }
}

fn is_timestamp_key(key: &str) -> bool {
    matches!(
        key,
        "timestamp"
            | "last_heartbeat"
            | "started_at"
            | "completed_at"
            | "last_run"
            | "next_run"
            | "connected_at"
            | "last_message_at"
    )
}

fn is_duration_key(key: &str) -> bool {
    key == "time_since_heartbeat"
}

/// Fake sidecar for deterministic nl-query integration tests.
/// Spawns a lightweight actix-web HTTP server on a random port.
/// The caller receives the URL to inject into `AiRuntimeClient::with_base_url()`.
/// Shuts down automatically when the test runtime ends.
pub(crate) struct FakeSidecar {
    url: String,
    requests: Arc<Mutex<Vec<FakeSidecarRequest>>>,
    shutdown: Option<AbortHandle>,
}

impl FakeSidecar {
    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn requests(&self) -> Vec<FakeSidecarRequest> {
        self.requests.lock().unwrap().clone()
    }

    #[allow(dead_code)]
    pub fn take_requests(&self) -> Vec<FakeSidecarRequest> {
        std::mem::take(&mut *self.requests.lock().unwrap())
    }
}

impl Drop for FakeSidecar {
    fn drop(&mut self) {
        if let Some(handle) = self.shutdown.take() {
            handle.abort();
        }
    }
}

struct FakeSidecarState {
    status: u16,
    body: Value,
    requests: Arc<Mutex<Vec<FakeSidecarRequest>>>,
}

async fn fake_sidecar_handler(
    req: HttpRequest,
    body: web::Bytes,
    state: web::Data<Arc<Mutex<FakeSidecarState>>>,
) -> HttpResponse {
    let json_body = serde_json::from_slice::<Value>(&body).unwrap_or(Value::Null);
    let request_run_id = request_run_id(&json_body);
    let has_service_identity = req
        .headers()
        .contains_key(header::HeaderName::from_static("x-service-identity"));
    let service_identity_token = req
        .headers()
        .get(header::HeaderName::from_static("x-service-identity"))
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());
    let method = req.method().to_string();
    let path = req.path().to_string();

    let (status, response_body, requests) = {
        let guard = state.lock().unwrap();
        (guard.status, guard.body.clone(), guard.requests.clone())
    };

    requests.lock().unwrap().push(FakeSidecarRequest {
        method,
        path,
        has_service_identity,
        service_identity_token,
        body: json_body,
    });

    let response_body = with_request_run_id(response_body, request_run_id.as_deref());
    HttpResponse::build(actix_web::http::StatusCode::from_u16(status).unwrap()).json(response_body)
}

/// Start a fake Python AI runtime sidecar that returns the given status + JSON body.
/// The handler records each request and asserts every request carries an `X-Service-Identity` header.
pub(crate) async fn start_fake_sidecar(status: u16, body: Value) -> FakeSidecar {
    let requests = Arc::new(Mutex::new(Vec::<FakeSidecarRequest>::new()));
    let state = Arc::new(Mutex::new(FakeSidecarState {
        status,
        body,
        requests: requests.clone(),
    }));

    let server = HttpServer::new(move || {
        let state = state.clone();
        App::new()
            .app_data(web::Data::new(state))
            .service(web::resource("/internal/ai/v1/runs").route(web::post().to(fake_sidecar_handler)))
    })
    .workers(1)
    .bind("127.0.0.1:0")
    .expect("Failed to bind fake sidecar");

    let addr = server.addrs()[0];
    let url = format!("http://127.0.0.1:{}", addr.port());
    let (shutdown, reg) = AbortHandle::new_pair();
    let fut = Abortable::new(server.run(), reg);
    tokio::spawn(async move {
        let _ = fut.await;
    });

    FakeSidecar {
        url,
        requests,
        shutdown: Some(shutdown),
    }
}

struct FakeSidecarSseState {
    status: u16,
    events: Vec<String>,
    requests: Arc<Mutex<Vec<FakeSidecarRequest>>>,
}

async fn fake_sidecar_sse_handler(
    req: HttpRequest,
    body: web::Bytes,
    state: web::Data<Arc<Mutex<FakeSidecarSseState>>>,
) -> HttpResponse {
    let json_body = serde_json::from_slice::<Value>(&body).unwrap_or(Value::Null);
    let request_run_id = request_run_id(&json_body);
    let has_service_identity = req
        .headers()
        .contains_key(header::HeaderName::from_static("x-service-identity"));
    let service_identity_token = req
        .headers()
        .get(header::HeaderName::from_static("x-service-identity"))
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());
    let method = req.method().to_string();
    let path = req.path().to_string();

    let (status, events, requests) = {
        let guard = state.lock().unwrap();
        (guard.status, guard.events.clone(), guard.requests.clone())
    };

    requests.lock().unwrap().push(FakeSidecarRequest {
        method,
        path,
        has_service_identity,
        service_identity_token,
        body: json_body,
    });

    if status == 200 {
        let body_text = with_request_run_id_text(&events.join(""), request_run_id.as_deref());
        HttpResponse::Ok()
            .content_type("text/event-stream")
            .insert_header(("Cache-Control", "no-cache"))
            .body(body_text)
    } else {
        HttpResponse::build(actix_web::http::StatusCode::from_u16(status).unwrap()).finish()
    }
}

fn request_run_id(json_body: &Value) -> Option<String> {
    json_body.get("run_id").and_then(Value::as_str).map(str::to_string)
}

fn with_request_run_id(mut value: Value, request_run_id: Option<&str>) -> Value {
    let Some(run_id) = request_run_id else {
        return value;
    };
    replace_placeholder_run_ids_in_value(&mut value, run_id);
    value
}

fn replace_placeholder_run_ids_in_value(value: &mut Value, run_id: &str) {
    match value {
        Value::String(text) if is_run_id_placeholder(text) => {
            *text = run_id.to_string();
        }
        Value::Array(items) => {
            for item in items {
                replace_placeholder_run_ids_in_value(item, run_id);
            }
        }
        Value::Object(map) => {
            for item in map.values_mut() {
                replace_placeholder_run_ids_in_value(item, run_id);
            }
        }
        _ => {}
    }
}

fn with_request_run_id_text(text: &str, request_run_id: Option<&str>) -> String {
    let Some(run_id) = request_run_id else {
        return text.to_string();
    };
    text.replace("run_fake_ignored", run_id).replace("ignored_id", run_id)
}

fn is_run_id_placeholder(value: &str) -> bool {
    matches!(value, "run_fake_ignored" | "ignored_id" | "run_fixture_001")
}

pub(crate) async fn start_fake_sidecar_sse(events: Vec<String>) -> FakeSidecar {
    start_fake_sidecar_sse_with_status(200, events).await
}

pub(crate) async fn start_fake_sidecar_sse_with_status(status: u16, events: Vec<String>) -> FakeSidecar {
    let requests = Arc::new(Mutex::new(Vec::<FakeSidecarRequest>::new()));
    let state = Arc::new(Mutex::new(FakeSidecarSseState {
        status,
        events,
        requests: requests.clone(),
    }));

    let server = HttpServer::new(move || {
        let state = state.clone();
        App::new()
            .app_data(web::Data::new(state))
            .service(web::resource("/internal/ai/v1/runs/stream").route(web::post().to(fake_sidecar_sse_handler)))
    })
    .workers(1)
    .bind("127.0.0.1:0")
    .expect("Failed to bind fake SSE sidecar");

    let addr = server.addrs()[0];
    let url = format!("http://127.0.0.1:{}", addr.port());
    let (shutdown, reg) = AbortHandle::new_pair();
    let fut = Abortable::new(server.run(), reg);
    tokio::spawn(async move {
        let _ = fut.await;
    });

    FakeSidecar {
        url,
        requests,
        shutdown: Some(shutdown),
    }
}

struct FakeSidecarDelayedSseState {
    status: u16,
    chunks: Vec<(String, std::time::Duration)>,
    requests: Arc<Mutex<Vec<FakeSidecarRequest>>>,
}

async fn fake_sidecar_delayed_sse_handler(
    req: HttpRequest,
    body: web::Bytes,
    state: web::Data<Arc<Mutex<FakeSidecarDelayedSseState>>>,
) -> HttpResponse {
    let json_body = serde_json::from_slice::<Value>(&body).unwrap_or(Value::Null);
    let has_service_identity = req
        .headers()
        .contains_key(header::HeaderName::from_static("x-service-identity"));
    let service_identity_token = req
        .headers()
        .get(header::HeaderName::from_static("x-service-identity"))
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());
    let method = req.method().to_string();
    let path = req.path().to_string();

    let (status, chunks, requests) = {
        let guard = state.lock().unwrap();
        (guard.status, guard.chunks.clone(), guard.requests.clone())
    };

    requests.lock().unwrap().push(FakeSidecarRequest {
        method,
        path,
        has_service_identity,
        service_identity_token,
        body: json_body,
    });

    if status == 200 {
        let stream = async_stream::stream! {
            for (chunk, delay) in chunks {
                tokio::time::sleep(delay).await;
                yield Ok::<web::Bytes, actix_web::Error>(web::Bytes::from(chunk));
            }
        };
        HttpResponse::Ok()
            .content_type("text/event-stream")
            .insert_header(("Cache-Control", "no-cache"))
            .streaming(stream)
    } else {
        HttpResponse::build(actix_web::http::StatusCode::from_u16(status).unwrap()).finish()
    }
}

/// Start a fake Python AI runtime sidecar that returns SSE chunks with configurable delays.
/// Each tuple is (sse_chunk_text, delay_before_yielding).
/// This is useful for testing client disconnect scenarios where the server must continue
/// consuming the stream after the client drops.
pub(crate) async fn start_fake_sidecar_delayed_sse(chunks: Vec<(String, std::time::Duration)>) -> FakeSidecar {
    let requests = Arc::new(Mutex::new(Vec::<FakeSidecarRequest>::new()));
    let state = Arc::new(Mutex::new(FakeSidecarDelayedSseState {
        status: 200,
        chunks,
        requests: requests.clone(),
    }));

    let server = HttpServer::new(move || {
        let state = state.clone();
        App::new().app_data(web::Data::new(state)).service(
            web::resource("/internal/ai/v1/runs/stream").route(web::post().to(fake_sidecar_delayed_sse_handler)),
        )
    })
    .workers(1)
    .bind("127.0.0.1:0")
    .expect("Failed to bind fake delayed SSE sidecar");

    let addr = server.addrs()[0];
    let url = format!("http://127.0.0.1:{}", addr.port());
    let (shutdown, reg) = AbortHandle::new_pair();
    let fut = Abortable::new(server.run(), reg);
    tokio::spawn(async move {
        let _ = fut.await;
    });

    FakeSidecar {
        url,
        requests,
        shutdown: Some(shutdown),
    }
}

struct FakeSidecarErrorStreamState {
    status: u16,
    initial_chunks: Vec<String>,
    error_message: String,
    requests: Arc<Mutex<Vec<FakeSidecarRequest>>>,
}

async fn fake_sidecar_error_stream_handler(
    req: HttpRequest,
    body: web::Bytes,
    state: web::Data<Arc<Mutex<FakeSidecarErrorStreamState>>>,
) -> HttpResponse {
    let json_body = serde_json::from_slice::<Value>(&body).unwrap_or(Value::Null);
    let has_service_identity = req
        .headers()
        .contains_key(header::HeaderName::from_static("x-service-identity"));
    let service_identity_token = req
        .headers()
        .get(header::HeaderName::from_static("x-service-identity"))
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());
    let method = req.method().to_string();
    let path = req.path().to_string();

    let (status, initial_chunks, error_message, requests) = {
        let guard = state.lock().unwrap();
        (
            guard.status,
            guard.initial_chunks.clone(),
            guard.error_message.clone(),
            guard.requests.clone(),
        )
    };

    requests.lock().unwrap().push(FakeSidecarRequest {
        method,
        path,
        has_service_identity,
        service_identity_token,
        body: json_body,
    });

    if status == 200 {
        let abort_chunk = format!(
            "event: transport.abort\ndata: {}\n\n",
            serde_json::json!({ "message": error_message })
        );
        let stream = async_stream::stream! {
            for chunk in initial_chunks {
                yield Ok::<web::Bytes, actix_web::Error>(web::Bytes::from(chunk));
            }
            yield Ok::<web::Bytes, actix_web::Error>(web::Bytes::from(abort_chunk));
        };
        HttpResponse::Ok()
            .content_type("text/event-stream")
            .insert_header(("Cache-Control", "no-cache"))
            .streaming(stream)
    } else {
        HttpResponse::build(actix_web::http::StatusCode::from_u16(status).unwrap()).finish()
    }
}

pub(crate) async fn start_fake_sidecar_error_stream(initial_chunks: Vec<String>, error_message: String) -> FakeSidecar {
    let requests = Arc::new(Mutex::new(Vec::<FakeSidecarRequest>::new()));
    let state = Arc::new(Mutex::new(FakeSidecarErrorStreamState {
        status: 200,
        initial_chunks,
        error_message,
        requests: requests.clone(),
    }));

    let server = HttpServer::new(move || {
        let state = state.clone();
        App::new().app_data(web::Data::new(state)).service(
            web::resource("/internal/ai/v1/runs/stream").route(web::post().to(fake_sidecar_error_stream_handler)),
        )
    })
    .workers(1)
    .bind("127.0.0.1:0")
    .expect("Failed to bind fake error SSE sidecar");

    let addr = server.addrs()[0];
    let url = format!("http://127.0.0.1:{}", addr.port());
    let (shutdown, reg) = AbortHandle::new_pair();
    let fut = Abortable::new(server.run(), reg);
    tokio::spawn(async move {
        let _ = fut.await;
    });

    FakeSidecar {
        url,
        requests,
        shutdown: Some(shutdown),
    }
}

// ---------------------------------------------------------------------------
// Shared test helpers (EnvGuard, DB pool, JWT)
// ---------------------------------------------------------------------------

/// Guard an environment variable, restoring on drop.
pub(crate) struct EnvGuard {
    key: &'static str,
    previous: Option<String>,
}

impl EnvGuard {
    pub fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var(key).ok();
        std::env::set_var(key, value);
        Self { key, previous }
    }

    pub fn remove(key: &'static str) -> Self {
        let previous = std::env::var(key).ok();
        std::env::remove_var(key);
        Self { key, previous }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        if let Some(value) = &self.previous {
            std::env::set_var(self.key, value);
        } else {
            std::env::remove_var(self.key);
        }
    }
}

/// Returns true when `TEST_DATABASE_URL` is set.
pub(crate) fn has_test_db() -> bool {
    std::env::var("TEST_DATABASE_URL").is_ok()
}

/// Create a PgPool from `TEST_DATABASE_URL`.
pub(crate) async fn create_test_pool() -> sqlx::PgPool {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    sqlx::PgPool::connect(&url).await.expect("test db pool")
}

/// Build a JWT string for integration tests.
pub(crate) fn make_test_jwt(permissions: &[&str]) -> String {
    use chrono::Utc;
    use jsonwebtoken::{encode, EncodingKey, Header};
    let now = Utc::now().timestamp();
    let claims = serde_json::json!({
        "sub": "test_user",
        "username": "tester",
        "permissions": permissions,
        "department_id": null,
        "is_admin": false,
        "iat": now,
        "exp": now + 3600,
        "type": "access",
    });
    encode(&Header::default(), &claims, &EncodingKey::from_secret(b"test-secret")).expect("jwt encoding")
}

// ---------------------------------------------------------------------------

/// Start a fake Python AI runtime sidecar that serves SSE on
/// `/internal/ai/v1/runs/stream-with-tools` (for P2.4-alpha tool streaming tests).
pub(crate) async fn start_fake_sidecar_sse_for_stream_with_tools(events: Vec<String>) -> FakeSidecar {
    start_fake_sidecar_sse_for_stream_with_tools_with_status(200, events).await
}

pub(crate) async fn start_fake_sidecar_sse_for_stream_with_tools_with_status(
    status: u16,
    events: Vec<String>,
) -> FakeSidecar {
    let requests = Arc::new(Mutex::new(Vec::<FakeSidecarRequest>::new()));
    let state = Arc::new(Mutex::new(FakeSidecarSseState {
        status,
        events,
        requests: requests.clone(),
    }));

    let server = HttpServer::new(move || {
        let state = state.clone();
        App::new().app_data(web::Data::new(state)).service(
            web::resource("/internal/ai/v1/runs/stream-with-tools").route(web::post().to(fake_sidecar_sse_handler)),
        )
    })
    .workers(1)
    .bind("127.0.0.1:0")
    .expect("Failed to bind fake SSE sidecar for stream-with-tools");

    let addr = server.addrs()[0];
    let url = format!("http://127.0.0.1:{}", addr.port());
    let (shutdown, reg) = AbortHandle::new_pair();
    let fut = Abortable::new(server.run(), reg);
    tokio::spawn(async move {
        let _ = fut.await;
    });

    FakeSidecar {
        url,
        requests,
        shutdown: Some(shutdown),
    }
}

// ---------------------------------------------------------------------------
// SQL test helpers for centralized DB operations (TD-16)
//
// These helpers concentrate every `sqlx::query*` call made by integration
// tests so the SQL text lives in one place. Test files call these helpers
// instead of writing inline SQL, which keeps the tests decoupled from the
// raw SQL and makes schema changes easier to propagate.
// ---------------------------------------------------------------------------

/// Insert a smoke `ai_action_proposals` row with an explicit `created_at`.
///
/// Used by readiness cleanup tests that need proposals at a specific point in
/// time (e.g., 30 minutes ago or 2 hours ago) to exercise the cleanup window.
pub(crate) async fn insert_smoke_proposal_with_created_at(
    pool: &sqlx::PgPool,
    proposal_id: &str,
    job_id: &str,
    run_id: &str,
    object_id: &str,
    metadata: serde_json::Value,
    created_at: chrono::DateTime<chrono::Utc>,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO ai_action_proposals (proposal_id, job_id, run_id, ontology_version, object_type, object_id, action_name, status, risk_level, metadata, created_at) \
         VALUES ($1, $2, $3, 'v1', 'Todo', $4, 'create', 2, 1, $5, $6)",
    )
    .bind(proposal_id)
    .bind(job_id)
    .bind(run_id)
    .bind(object_id)
    .bind(metadata)
    .bind(created_at)
    .execute(pool)
    .await
    .map(|_| ())
}

/// Insert a smoke `ai_action_proposals` row relying on the DB default for
/// `created_at` (i.e., the proposal is considered "new" for cleanup logic).
pub(crate) async fn insert_smoke_proposal_default_created_at(
    pool: &sqlx::PgPool,
    proposal_id: &str,
    job_id: &str,
    run_id: &str,
    object_id: &str,
    metadata: serde_json::Value,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO ai_action_proposals (proposal_id, job_id, run_id, ontology_version, object_type, object_id, action_name, status, risk_level, metadata) \
         VALUES ($1, $2, $3, 'v1', 'Todo', $4, 'create', 2, 1, $5)",
    )
    .bind(proposal_id)
    .bind(job_id)
    .bind(run_id)
    .bind(object_id)
    .bind(metadata)
    .execute(pool)
    .await
    .map(|_| ())
}

/// Insert a pre-existing proposal that carries an idempotency key in its
/// `metadata`. Used to simulate a conflicting prior proposal for the
/// idempotency-conflict integration test (status = 6 = Executed, with an
/// `expires_at` set one hour in the future).
pub(crate) async fn insert_idempotent_conflict_proposal(
    pool: &sqlx::PgPool,
    proposal_id: &str,
    metadata: serde_json::Value,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO ai_action_proposals (proposal_id, job_id, run_id, ontology_version, object_type, object_id, action_name, arguments, status, metadata, created_at, expires_at) \
         VALUES ($1, 'job', 'run', 'flight-ops.v1', 'Flight', 'FL123', 'change_stand', '{}', 6, $2, NOW(), NOW() + INTERVAL '1 hour')",
    )
    .bind(proposal_id)
    .bind(metadata)
    .execute(pool)
    .await
    .map(|_| ())
}

/// Return whether an `ai_action_proposals` row with the given id exists.
pub(crate) async fn proposal_exists_by_id(pool: &sqlx::PgPool, proposal_id: &str) -> sqlx::Result<bool> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM ai_action_proposals WHERE proposal_id = $1)")
        .bind(proposal_id)
        .fetch_one(pool)
        .await
}

/// Return the numeric `status` of a proposal, or `None` if it does not exist.
pub(crate) async fn proposal_status_by_id(pool: &sqlx::PgPool, proposal_id: &str) -> sqlx::Result<Option<i16>> {
    let row: Option<(i16,)> = sqlx::query_as("SELECT status FROM ai_action_proposals WHERE proposal_id = $1")
        .bind(proposal_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.0))
}

/// Delete a proposal by id. Safe to call even if the row does not exist.
pub(crate) async fn cleanup_proposal_by_id(pool: &sqlx::PgPool, proposal_id: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM ai_action_proposals WHERE proposal_id = $1")
        .bind(proposal_id)
        .execute(pool)
        .await
        .map(|_| ())
}

/// Insert the canonical `test_user` row used to satisfy foreign keys, using
/// `ON CONFLICT (id) DO NOTHING` so it is safe to call repeatedly.
pub(crate) async fn ensure_test_user(pool: &sqlx::PgPool) -> sqlx::Result<()> {
    // 不指定冲突目标：既容忍历史残留的 username='tester' 行，也容忍并行测试的 username 唯一约束竞争。
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, is_active, is_verified) \
         VALUES ($1, $2, $3, $4, TRUE, TRUE) ON CONFLICT DO NOTHING",
    )
    .bind("test_user")
    .bind("tester")
    .bind("tester@example.com")
    .bind("hashed_password")
    .execute(pool)
    .await
    .map(|_| ())
}

/// Remove the canonical `test_user` row by id or username.
pub(crate) async fn cleanup_test_user(pool: &sqlx::PgPool) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM users WHERE username = 'tester' OR id = 'test_user'")
        .execute(pool)
        .await
        .map(|_| ())
}

/// Return whether any `todos` row references the given `source_id`.
pub(crate) async fn todo_exists_by_source_id(pool: &sqlx::PgPool, source_id: &str) -> sqlx::Result<bool> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM todos WHERE source_id = $1)")
        .bind(source_id)
        .fetch_one(pool)
        .await
}

/// Return whether any `todos` row matches the given `(source_type, source_id)`.
pub(crate) async fn todo_exists_by_source(
    pool: &sqlx::PgPool,
    source_type: &str,
    source_id: &str,
) -> sqlx::Result<bool> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM todos WHERE source_type = $1 AND source_id = $2)")
        .bind(source_type)
        .bind(source_id)
        .fetch_one(pool)
        .await
}

/// Count `todos` rows matching the given `(source_type, source_id)`.
pub(crate) async fn todo_count_by_source(pool: &sqlx::PgPool, source_type: &str, source_id: &str) -> sqlx::Result<i64> {
    sqlx::query_scalar("SELECT COUNT(*) FROM todos WHERE source_type = $1 AND source_id = $2")
        .bind(source_type)
        .bind(source_id)
        .fetch_one(pool)
        .await
}

/// Fetch the `title` of the todo with the given `source_id`.
pub(crate) async fn todo_title_by_source_id(pool: &sqlx::PgPool, source_id: &str) -> sqlx::Result<String> {
    sqlx::query_scalar("SELECT title FROM todos WHERE source_id = $1")
        .bind(source_id)
        .fetch_one(pool)
        .await
}

/// Delete `todos` rows by `todo_id`.
pub(crate) async fn cleanup_todo_by_id(pool: &sqlx::PgPool, todo_id: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM todos WHERE todo_id = $1")
        .bind(todo_id)
        .execute(pool)
        .await
        .map(|_| ())
}

/// Count `domain_event_outbox` rows for a given `aggregate_id`.
pub(crate) async fn outbox_count_by_aggregate_id(pool: &sqlx::PgPool, aggregate_id: &str) -> sqlx::Result<i64> {
    sqlx::query_scalar("SELECT COUNT(*) FROM domain_event_outbox WHERE aggregate_id = $1")
        .bind(aggregate_id)
        .fetch_one(pool)
        .await
}

/// Count `domain_event_outbox` rows for a given `(event_type, aggregate_id)`.
pub(crate) async fn outbox_count_by_event_type(
    pool: &sqlx::PgPool,
    event_type: &str,
    aggregate_id: &str,
) -> sqlx::Result<i64> {
    sqlx::query_scalar("SELECT COUNT(*) FROM domain_event_outbox WHERE event_type = $1 AND aggregate_id = $2")
        .bind(event_type)
        .bind(aggregate_id)
        .fetch_one(pool)
        .await
}

/// Delete `domain_event_outbox` rows by `aggregate_id`.
pub(crate) async fn cleanup_outbox_by_aggregate_id(pool: &sqlx::PgPool, aggregate_id: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM domain_event_outbox WHERE aggregate_id = $1")
        .bind(aggregate_id)
        .execute(pool)
        .await
        .map(|_| ())
}

/// Return the ordered list of `event_type` values from `ai_run_events` for the
/// given `(job_id, run_id)`, ordered by `created_at` ascending.
pub(crate) async fn ai_run_event_types(pool: &sqlx::PgPool, job_id: &str, run_id: &str) -> sqlx::Result<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT event_type FROM ai_run_events WHERE job_id = $1 AND run_id = $2 ORDER BY created_at ASC",
    )
    .bind(job_id)
    .bind(run_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|row| row.0).collect())
}

pub(crate) async fn seed_ai_runtime_test_flights(pool: &sqlx::PgPool) {
    ensure_ai_runtime_test_flight_tables(pool).await;

    for (flight_id, flight_number) in [("FL123", "FL123"), ("CA1234", "CA1234")] {
        sqlx::query(
            r#"
                INSERT INTO flights (
                    flight_id, airline_code, flight_number, registration,
                    aircraft_type_detail, status,
                    scheduled_departure, scheduled_arrival,
                    estimated_departure, estimated_arrival,
                    actual_departure, actual_arrival,
                    cobt_time, codt,
                    gate, stand, terminal, position, baggage_carousel,
                    has_boarding_restriction, is_quick_turnaround, is_commercial_signed,
                    created_at, updated_at, version,
                    flight_remarks, load_planning_remarks,
                    aircraft_maintenance_remarks, aircraft_check_remarks
                ) VALUES (
                    $1, 'CA', $2, NULL,
                    NULL, 0,
                    NOW(), NOW() + INTERVAL '2 hours',
                    NULL, NULL,
                    NULL, NULL,
                    NULL, NULL,
                    'A12', 'S1', 'T1', NULL, NULL,
                    FALSE, FALSE, TRUE,
                    NOW(), NOW(), 1,
                    NULL, NULL, NULL, NULL
                )
                ON CONFLICT (flight_id) DO UPDATE SET
                    airline_code = EXCLUDED.airline_code,
                    flight_number = EXCLUDED.flight_number,
                    gate = EXCLUDED.gate,
                    stand = EXCLUDED.stand,
                    terminal = EXCLUDED.terminal,
                    updated_at = NOW()
                "#,
        )
        .bind(flight_id)
        .bind(flight_number)
        .execute(pool)
        .await
        .expect("seed AI runtime test flight");
    }
}

async fn ensure_ai_runtime_test_flight_tables(pool: &sqlx::PgPool) {
    sqlx::query(
        r#"
            CREATE TABLE IF NOT EXISTS flights (
                flight_id VARCHAR(26) PRIMARY KEY,
                airline_code VARCHAR(8),
                flight_number VARCHAR(16),
                registration VARCHAR(32),
                aircraft_type_detail VARCHAR(64),
                status SMALLINT NOT NULL DEFAULT 0,
                scheduled_departure TIMESTAMPTZ,
                scheduled_arrival TIMESTAMPTZ,
                estimated_departure TIMESTAMPTZ,
                estimated_arrival TIMESTAMPTZ,
                actual_departure TIMESTAMPTZ,
                actual_arrival TIMESTAMPTZ,
                cobt_time TIMESTAMPTZ,
                codt TIMESTAMPTZ,
                gate VARCHAR(32),
                stand VARCHAR(32),
                terminal VARCHAR(32),
                position VARCHAR(32),
                baggage_carousel VARCHAR(32),
                has_boarding_restriction BOOLEAN NOT NULL DEFAULT FALSE,
                is_quick_turnaround BOOLEAN NOT NULL DEFAULT FALSE,
                is_commercial_signed BOOLEAN NOT NULL DEFAULT TRUE,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                version INTEGER NOT NULL DEFAULT 1,
                labels JSONB,
                flight_remarks TEXT,
                load_planning_remarks TEXT,
                aircraft_maintenance_remarks TEXT,
                aircraft_check_remarks TEXT
            )
            "#,
    )
    .execute(pool)
    .await
    .expect("ensure AI runtime test flights table");

    sqlx::query(
        r#"
            CREATE TABLE IF NOT EXISTS flight_legs (
                leg_id VARCHAR(26) PRIMARY KEY,
                flight_id VARCHAR(26) NOT NULL,
                leg_type VARCHAR(16) NOT NULL,
                flight_no VARCHAR(16) NOT NULL,
                flight_type VARCHAR(16) NOT NULL DEFAULT 'domestic',
                mission SMALLINT,
                origin_stations JSONB,
                destination_stations JSONB,
                is_vip BOOLEAN NOT NULL DEFAULT FALSE,
                stand_type VARCHAR(32),
                scheduled_time TIMESTAMPTZ,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                UNIQUE (flight_id, leg_type)
            )
            "#,
    )
    .execute(pool)
    .await
    .expect("ensure AI runtime test flight_legs table");

    sqlx::query(
        r#"
            CREATE TABLE IF NOT EXISTS anomalies (
                anomaly_id VARCHAR(26) PRIMARY KEY,
                flight_id VARCHAR(26),
                status VARCHAR(32) NOT NULL DEFAULT 'open'
            )
            "#,
    )
    .execute(pool)
    .await
    .expect("ensure AI runtime test anomalies table");
}

// ---------------------------------------------------------------------------
// Mock repositories for unit tests (TD-16)
// ---------------------------------------------------------------------------

pub mod mock_repositories {
    use fms_domain::error::DomainError;
    use serde_json::Value;

    pub struct MockFlightSyncRepository {
        pub latest_result: Option<Value>,
        pub create_run_called: std::sync::atomic::AtomicBool,
    }

    impl MockFlightSyncRepository {
        pub fn new() -> Self {
            Self {
                latest_result: None,
                create_run_called: std::sync::atomic::AtomicBool::new(false),
            }
        }

        pub fn with_latest(mut self, result: Value) -> Self {
            self.latest_result = Some(result);
            self
        }
    }

    impl Default for MockFlightSyncRepository {
        fn default() -> Self {
            Self::new()
        }
    }

    #[async_trait::async_trait]
    impl fms_domain::ports::flight_sync_repository::FlightSyncRepository for MockFlightSyncRepository {
        async fn find_latest(&self, _source_system: &str) -> Result<Option<Value>, DomainError> {
            Ok(self.latest_result.clone())
        }

        async fn create_run(
            &self,
            _run_id: &str,
            _source_system: &str,
            _trigger: &str,
            _direction: &str,
            _window_start: chrono::NaiveDate,
            _window_end: chrono::NaiveDate,
            _status: &str,
            _started_at: chrono::DateTime<chrono::Utc>,
        ) -> Result<(), DomainError> {
            self.create_run_called.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }

        async fn mark_completed(
            &self,
            _run_id: &str,
            _processed_count: i32,
            _success_count: i32,
            _failure_count: i32,
            _created_count: i32,
            _updated_count: i32,
            _failure_samples: &[Value],
            _error_summary: &[Value],
            _completed_at: chrono::DateTime<chrono::Utc>,
        ) -> Result<(), DomainError> {
            Ok(())
        }

        async fn mark_failed(
            &self,
            _run_id: &str,
            _failure_count: i32,
            _failure_samples: &[Value],
            _error_summary: &[Value],
            _completed_at: chrono::DateTime<chrono::Utc>,
        ) -> Result<(), DomainError> {
            Ok(())
        }

        async fn load_payload(&self, _run_id: &str) -> Result<Value, DomainError> {
            Ok(serde_json::json!({}))
        }
    }
}

#[cfg(test)]
mod mock_tests {
    use super::mock_repositories::MockFlightSyncRepository;
    use fms_domain::ports::flight_sync_repository::FlightSyncRepository;

    #[tokio::test]
    async fn mock_flight_sync_repository_returns_configured_latest() {
        let mock = MockFlightSyncRepository::new().with_latest(serde_json::json!({"id": "test"}));
        let result = mock.find_latest("test-system").await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap()["id"], "test");
    }

    #[tokio::test]
    async fn mock_flight_sync_repository_tracks_create_run_calls() {
        let mock = MockFlightSyncRepository::new();
        assert!(!mock.create_run_called.load(std::sync::atomic::Ordering::SeqCst));
        mock.create_run(
            "run-1",
            "system",
            "trigger",
            "inbound",
            chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            chrono::NaiveDate::from_ymd_opt(2026, 1, 2).unwrap(),
            "running",
            chrono::Utc::now(),
        )
        .await
        .unwrap();
        assert!(mock.create_run_called.load(std::sync::atomic::Ordering::SeqCst));
    }
}
