use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use hmac::{Hmac, Mac};
use rand::Rng;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, Method};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

static PROCESS_CTX: OnceLock<(u32, u32)> = OnceLock::new();

fn process_ctx() -> &'static (u32, u32) {
    PROCESS_CTX.get_or_init(|| {
        let pid = std::process::id();
        let seed = pid
            ^ (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u32);
        (pid, seed)
    })
}

#[derive(Debug, Clone, Deserialize)]
struct ScenarioFile {
    name: String,
    #[serde(default)]
    endpoints: Vec<ScenarioEndpoint>,
}

#[derive(Debug, Clone, Deserialize)]
struct ScenarioEndpoint {
    name: String,
    method: String,
    path: String,
    weight: u32,
    #[serde(default = "default_true")]
    auth: bool,
    #[serde(default)]
    body: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone)]
struct Config {
    base_url: String,
    scenario_path: Option<String>,
    token: Option<String>,
    anti_replay_secret: Option<String>,
    flight_ids: Vec<String>,
    concurrency: usize,
    duration_secs: u64,
    timeout_ms: u64,
    accept_invalid_certs: bool,
    gzip: bool,
}

impl Config {
    fn from_args() -> Self {
        let mut config = Self {
            base_url: "https://localhost:18443".to_string(),
            scenario_path: None,
            token: None,
            anti_replay_secret: None,
            flight_ids: Vec::new(),
            concurrency: 512,
            duration_secs: 30,
            timeout_ms: 5000,
            accept_invalid_certs: false,
            gzip: true,
        };
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            if matches!(arg.as_str(), "--help" | "-h") {
                print_help();
                std::process::exit(0);
            }
            let Some(value) = args.next() else {
                eprintln!("missing value for {arg}");
                std::process::exit(2);
            };
            match arg.as_str() {
                "--base-url" => config.base_url = value.trim_end_matches('/').to_string(),
                "--scenario" => config.scenario_path = Some(value),
                "--token" => config.token = Some(value),
                "--anti-replay-secret" => config.anti_replay_secret = Some(value),
                "--flight-id" => config.flight_ids.push(value),
                "--flight-ids" => {
                    config.flight_ids.extend(
                        value
                            .split(',')
                            .map(|item| item.trim().to_string())
                            .filter(|item| !item.is_empty()),
                    );
                }
                "--concurrency" => config.concurrency = parse_arg(&arg, &value),
                "--duration-sec" => config.duration_secs = parse_arg(&arg, &value),
                "--timeout-ms" => config.timeout_ms = parse_arg(&arg, &value),
                "--insecure" => config.accept_invalid_certs = parse_bool(&value),
                "--gzip" => config.gzip = parse_bool(&value),
                _ => {
                    eprintln!("unknown argument: {arg}");
                    print_help();
                    std::process::exit(2);
                }
            }
        }
        config.concurrency = config.concurrency.max(1);
        config.duration_secs = config.duration_secs.max(1);
        config.timeout_ms = config.timeout_ms.max(1);
        config
    }
}

fn parse_arg<T: std::str::FromStr>(name: &str, value: &str) -> T {
    value.parse::<T>().unwrap_or_else(|_| {
        eprintln!("invalid value for {name}: {value}");
        std::process::exit(2);
    })
}

fn parse_bool(value: &str) -> bool {
    matches!(value, "1" | "true" | "TRUE" | "yes" | "YES")
}

fn print_help() {
    eprintln!(
        "Usage: mixed_qps_client --base-url URL --scenario FILE --concurrency N --duration-sec S [--token T] [--anti-replay-secret HEX] [--flight-id ID] [--insecure true|false] [--gzip true|false]"
    );
}

fn builtin_scenario() -> ScenarioFile {
    ScenarioFile {
        name: "airport_ops".to_string(),
        endpoints: vec![
            ep(
                "flights_list",
                "GET",
                "/api/v2/flights?page=1&page_size=20",
                48,
                true,
                None,
            ),
            ep(
                "monitor_rows",
                "GET",
                "/api/v2/flights/monitor-rows?page=1&page_size=20",
                10,
                true,
                None,
            ),
            ep("auth_me", "GET", "/api/v2/auth/me", 8, true, None),
            ep(
                "notifications_unread",
                "GET",
                "/api/v2/notifications/unread-count",
                8,
                true,
                None,
            ),
            ep("todos_list", "GET", "/api/v2/todos?page=1&size=20", 6, true, None),
            ep(
                "dispatch_orders",
                "GET",
                "/api/v2/dispatch-orders?page=1&page_size=20",
                5,
                true,
                None,
            ),
            ep("health_ping", "GET", "/api/v2/health/ping", 5, false, None),
            ep(
                "todo_create",
                "POST",
                "/api/v2/todos",
                6,
                true,
                Some(r#"{"title":"perf-todo-{seq}","priority":"low","category":"ops"}"#.to_string()),
            ),
            ep(
                "flight_patch_remarks",
                "PATCH",
                "/api/v2/flights/{flight_id}",
                4,
                true,
                Some(r#"{"flight_remarks":"perf-{seq}"}"#.to_string()),
            ),
        ],
    }
}

fn ep(name: &str, method: &str, path: &str, weight: u32, auth: bool, body: Option<String>) -> ScenarioEndpoint {
    ScenarioEndpoint {
        name: name.to_string(),
        method: method.to_string(),
        path: path.to_string(),
        weight,
        auth,
        body,
    }
}

fn load_scenario(path: Option<&str>) -> ScenarioFile {
    let Some(path) = path else {
        return builtin_scenario();
    };
    let text = std::fs::read_to_string(path).unwrap_or_else(|error| {
        eprintln!("failed to read scenario {path}: {error}");
        std::process::exit(2);
    });
    serde_json::from_str(&text).unwrap_or_else(|error| {
        eprintln!("failed to parse scenario {path}: {error}");
        std::process::exit(2);
    })
}

struct EndpointRuntime {
    name: String,
    method: Method,
    path: String,
    weight: u32,
    auth: bool,
    body: Option<String>,
    needs_flight: bool,
    metrics: EndpointMetrics,
}

#[derive(Default)]
struct EndpointMetrics {
    total: AtomicU64,
    success: AtomicU64,
    non_success: AtomicU64,
    errors: AtomicU64,
}

#[derive(Default)]
struct Metrics {
    total: AtomicU64,
    success: AtomicU64,
    non_success: AtomicU64,
    errors: AtomicU64,
    bytes: AtomicU64,
    gzip_responses: AtomicU64,
    status_2xx: AtomicU64,
    status_4xx: AtomicU64,
    status_5xx: AtomicU64,
}

#[derive(Serialize)]
struct EndpointSummary {
    name: String,
    total: u64,
    success: u64,
    non_success: u64,
    errors: u64,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let config = Config::from_args();
    let scenario = load_scenario(config.scenario_path.as_deref());
    let endpoints: Vec<EndpointRuntime> = scenario
        .endpoints
        .into_iter()
        .filter(|endpoint| endpoint.weight > 0)
        .map(|endpoint| {
            let method = endpoint.method.parse::<Method>().unwrap_or_else(|_| {
                eprintln!("invalid method for {}: {}", endpoint.name, endpoint.method);
                std::process::exit(2);
            });
            EndpointRuntime {
                needs_flight: endpoint.path.contains("{flight_id}")
                    || endpoint.body.as_deref().unwrap_or("").contains("{flight_id}"),
                name: endpoint.name,
                method,
                path: endpoint.path,
                weight: endpoint.weight,
                auth: endpoint.auth,
                body: endpoint.body,
                metrics: EndpointMetrics::default(),
            }
        })
        .collect();
    if endpoints.is_empty() {
        eprintln!("scenario has no weighted endpoints");
        std::process::exit(2);
    }
    let total_weight: u32 = endpoints.iter().map(|endpoint| endpoint.weight).sum();
    if config.flight_ids.is_empty() && endpoints.iter().any(|endpoint| endpoint.needs_flight) {
        eprintln!("warning: scenario has {{flight_id}} endpoints but no --flight-id; those requests will be skipped");
    }

    let mut default_headers = HeaderMap::new();
    default_headers.insert("Accept", HeaderValue::from_static("application/json"));
    default_headers.insert("User-Agent", HeaderValue::from_static("fms-mixed-qps-client"));
    if config.gzip {
        default_headers.insert("Accept-Encoding", HeaderValue::from_static("gzip"));
    }
    if let Some(token) = config.token.as_deref() {
        let value = format!("Bearer {token}");
        default_headers.insert(
            "Authorization",
            HeaderValue::from_str(&value).unwrap_or_else(|error| {
                eprintln!("invalid token header: {error}");
                std::process::exit(2);
            }),
        );
    }

    let client = Client::builder()
        .pool_max_idle_per_host(config.concurrency)
        .tcp_nodelay(true)
        .tcp_keepalive(Duration::from_secs(30))
        .timeout(Duration::from_millis(config.timeout_ms))
        .default_headers(default_headers)
        .danger_accept_invalid_certs(config.accept_invalid_certs)
        .build()
        .expect("failed to build HTTP client");

    let metrics = Arc::new(Metrics::default());
    let endpoints = Arc::new(endpoints);
    let flight_ids = Arc::new(config.flight_ids.clone());
    let flight_cursor = Arc::new(AtomicUsize::new(0));
    let started = Instant::now();
    let deadline = started + Duration::from_secs(config.duration_secs);

    let reporter_metrics = metrics.clone();
    let reporter = tokio::spawn(async move {
        let mut last_total = 0;
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            let total = reporter_metrics.total.load(Ordering::Relaxed);
            let elapsed = started.elapsed().as_secs_f64();
            eprintln!(
                "elapsed={elapsed:.1}s total={} qps_1s={} success={} non_success={} errors={} 2xx={} 4xx={} 5xx={}",
                total,
                total.saturating_sub(last_total),
                reporter_metrics.success.load(Ordering::Relaxed),
                reporter_metrics.non_success.load(Ordering::Relaxed),
                reporter_metrics.errors.load(Ordering::Relaxed),
                reporter_metrics.status_2xx.load(Ordering::Relaxed),
                reporter_metrics.status_4xx.load(Ordering::Relaxed),
                reporter_metrics.status_5xx.load(Ordering::Relaxed),
            );
            last_total = total;
        }
    });

    let mut tasks = Vec::with_capacity(config.concurrency);
    for _ in 0..config.concurrency {
        let client = client.clone();
        let base_url = config.base_url.clone();
        let secret = config.anti_replay_secret.clone();
        let metrics = metrics.clone();
        let endpoints = endpoints.clone();
        let flight_ids = flight_ids.clone();
        let flight_cursor = flight_cursor.clone();
        tasks.push(tokio::spawn(async move {
            let mut latencies_us = Vec::new();
            while Instant::now() < deadline {
                let pick = rand::thread_rng().gen_range(0..total_weight);
                let mut acc = 0;
                let mut index = 0;
                for (endpoint_index, endpoint) in endpoints.iter().enumerate() {
                    acc += endpoint.weight;
                    if pick < acc {
                        index = endpoint_index;
                        break;
                    }
                }
                let endpoint = &endpoints[index];
                if endpoint.needs_flight && flight_ids.is_empty() {
                    continue;
                }
                let seq = next_seq();
                let flight_id = if endpoint.needs_flight {
                    let cursor = flight_cursor.fetch_add(1, Ordering::Relaxed);
                    flight_ids[cursor % flight_ids.len()].clone()
                } else {
                    String::new()
                };
                let path = substitute(&endpoint.path, seq, &flight_id);
                let body = endpoint
                    .body
                    .as_ref()
                    .map(|template| substitute(template, seq, &flight_id));
                let url = format!("{base_url}{path}");
                let request_started = Instant::now();
                let mut request = client.request(endpoint.method.clone(), &url);
                if let Some(body) = body.as_ref() {
                    request = request.header("Content-Type", "application/json").body(body.clone());
                }
                if endpoint.auth {
                    if let Some(secret) = secret.as_deref() {
                        for (name, value) in anti_replay_headers(&endpoint.method, &url, body.as_deref(), secret) {
                            request = request.header(name, value);
                        }
                    }
                }
                match request.send().await {
                    Ok(response) => {
                        let status = response.status();
                        let success = status.is_success();
                        let code = status.as_u16();
                        let gzipped = response
                            .headers()
                            .get("content-encoding")
                            .and_then(|value| value.to_str().ok())
                            .map(|value| value.split(',').any(|part| part.trim().eq_ignore_ascii_case("gzip")))
                            .unwrap_or(false);
                        match response.bytes().await {
                            Ok(bytes) => {
                                metrics.bytes.fetch_add(bytes.len() as u64, Ordering::Relaxed);
                                if gzipped {
                                    metrics.gzip_responses.fetch_add(1, Ordering::Relaxed);
                                }
                                if success {
                                    metrics.success.fetch_add(1, Ordering::Relaxed);
                                    endpoint.metrics.success.fetch_add(1, Ordering::Relaxed);
                                } else {
                                    metrics.non_success.fetch_add(1, Ordering::Relaxed);
                                    endpoint.metrics.non_success.fetch_add(1, Ordering::Relaxed);
                                }
                                match code {
                                    200..=299 => {
                                        metrics.status_2xx.fetch_add(1, Ordering::Relaxed);
                                    }
                                    400..=499 => {
                                        metrics.status_4xx.fetch_add(1, Ordering::Relaxed);
                                    }
                                    500..=599 => {
                                        metrics.status_5xx.fetch_add(1, Ordering::Relaxed);
                                    }
                                    _ => {}
                                }
                            }
                            Err(_) => {
                                metrics.errors.fetch_add(1, Ordering::Relaxed);
                                endpoint.metrics.errors.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }
                    Err(_) => {
                        metrics.errors.fetch_add(1, Ordering::Relaxed);
                        endpoint.metrics.errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
                metrics.total.fetch_add(1, Ordering::Relaxed);
                endpoint.metrics.total.fetch_add(1, Ordering::Relaxed);
                latencies_us.push(request_started.elapsed().as_micros() as u64);
            }
            latencies_us
        }));
    }

    let mut latencies_us = Vec::new();
    for task in tasks {
        match task.await {
            Ok(mut worker_latencies) => latencies_us.append(&mut worker_latencies),
            Err(_) => {
                metrics.errors.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    reporter.abort();
    latencies_us.sort_unstable();
    let elapsed_secs = started.elapsed().as_secs_f64();
    let total = metrics.total.load(Ordering::Relaxed);
    let qps = if elapsed_secs > 0.0 {
        total as f64 / elapsed_secs
    } else {
        0.0
    };
    let endpoint_summaries: Vec<EndpointSummary> = endpoints
        .iter()
        .map(|endpoint| EndpointSummary {
            name: endpoint.name.clone(),
            total: endpoint.metrics.total.load(Ordering::Relaxed),
            success: endpoint.metrics.success.load(Ordering::Relaxed),
            non_success: endpoint.metrics.non_success.load(Ordering::Relaxed),
            errors: endpoint.metrics.errors.load(Ordering::Relaxed),
        })
        .collect();
    let p50 = percentile_ms(&latencies_us, 50.0);
    let p95 = percentile_ms(&latencies_us, 95.0);
    let p99 = percentile_ms(&latencies_us, 99.0);
    let max_ms = latencies_us.last().copied().unwrap_or_default() as f64 / 1000.0;
    let bytes = metrics.bytes.load(Ordering::Relaxed);
    let gzip_responses = metrics.gzip_responses.load(Ordering::Relaxed);
    let avg_bytes = if total > 0 { bytes as f64 / total as f64 } else { 0.0 };
    let mbps = if elapsed_secs > 0.0 {
        (bytes as f64 * 8.0) / elapsed_secs / 1_000_000.0
    } else {
        0.0
    };
    println!(
        "summary scenario={} concurrency={} duration_sec={:.3} total={} success={} non_success={} errors={} bytes={} gzip_responses={} gzip={} avg_bytes={:.0} mbps={:.1} qps={:.2} p50_ms={:.3} p95_ms={:.3} p99_ms={:.3} max_ms={:.3} status_2xx={} status_4xx={} status_5xx={}",
        scenario.name,
        config.concurrency,
        elapsed_secs,
        total,
        metrics.success.load(Ordering::Relaxed),
        metrics.non_success.load(Ordering::Relaxed),
        metrics.errors.load(Ordering::Relaxed),
        bytes,
        gzip_responses,
        config.gzip,
        avg_bytes,
        mbps,
        qps,
        p50,
        p95,
        p99,
        max_ms,
        metrics.status_2xx.load(Ordering::Relaxed),
        metrics.status_4xx.load(Ordering::Relaxed),
        metrics.status_5xx.load(Ordering::Relaxed),
    );
    let jsonl = serde_json::json!({
        "type": "mixed_summary",
        "scenario": scenario.name,
        "base_url": config.base_url,
        "concurrency": config.concurrency,
        "duration_sec": elapsed_secs,
        "total": total,
        "success": metrics.success.load(Ordering::Relaxed),
        "non_success": metrics.non_success.load(Ordering::Relaxed),
        "errors": metrics.errors.load(Ordering::Relaxed),
        "bytes": bytes,
        "gzip_responses": gzip_responses,
        "gzip": config.gzip,
        "avg_bytes": avg_bytes,
        "mbps": mbps,
        "qps": qps,
        "p50_ms": p50,
        "p95_ms": p95,
        "p99_ms": p99,
        "max_ms": max_ms,
        "status_2xx": metrics.status_2xx.load(Ordering::Relaxed),
        "status_4xx": metrics.status_4xx.load(Ordering::Relaxed),
        "status_5xx": metrics.status_5xx.load(Ordering::Relaxed),
        "endpoints": endpoint_summaries,
    });
    println!("jsonl={}", serde_json::to_string(&jsonl).unwrap_or_default());
}

fn substitute(template: &str, seq: u64, flight_id: &str) -> String {
    template
        .replace("{seq}", &seq.to_string())
        .replace("{flight_id}", flight_id)
}

fn next_seq() -> u64 {
    static SEQ: AtomicU64 = AtomicU64::new(1);
    SEQ.fetch_add(1, Ordering::Relaxed)
}

fn anti_replay_headers(
    method: &Method,
    url: &str,
    body: Option<&str>,
    session_secret: &str,
) -> [(HeaderName, String); 4] {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock")
        .as_secs()
        .to_string();
    let (pid, seed) = process_ctx();
    let random_part: u128 = rand::thread_rng().gen();
    let nonce = format!("mix-{}-{}-{:032x}-{}", pid, seed, random_part, next_seq());
    let body_hash = sha256_hex(body.unwrap_or("").as_bytes());
    let request_uri = request_uri(url);
    let payload = format!(
        "{}:{}:{}:{}:{}",
        method.as_str().to_ascii_uppercase(),
        request_uri,
        timestamp,
        nonce,
        body_hash
    );
    let signature = hmac_sha256_hex(session_secret.as_bytes(), payload.as_bytes());
    [
        (HeaderName::from_static("x-request-timestamp"), timestamp),
        (HeaderName::from_static("x-request-nonce"), nonce),
        (HeaderName::from_static("x-request-body-sha256"), body_hash),
        (HeaderName::from_static("x-request-signature"), signature),
    ]
}

fn request_uri(url: &str) -> String {
    match reqwest::Url::parse(url) {
        Ok(parsed) => match parsed.query() {
            Some(query) => format!("{}?{}", parsed.path(), query),
            None => parsed.path().to_string(),
        },
        Err(_) => url.to_string(),
    }
}

fn sha256_hex(input: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input);
    hex::encode(hasher.finalize())
}

fn hmac_sha256_hex(secret: &[u8], payload: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(payload);
    hex::encode(mac.finalize().into_bytes())
}

fn percentile_ms(sorted_values_us: &[u64], percentile: f64) -> f64 {
    if sorted_values_us.is_empty() {
        return 0.0;
    }
    let rank = (percentile / 100.0) * (sorted_values_us.len().saturating_sub(1) as f64);
    let index = rank.round() as usize;
    sorted_values_us[index] as f64 / 1000.0
}
