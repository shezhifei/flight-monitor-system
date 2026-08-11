use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use hmac::{Hmac, Mac};
use rand::Rng;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, Method};
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

#[derive(Debug, Clone)]
struct Config {
    url: String,
    method: Method,
    headers: Vec<(HeaderName, HeaderValue)>,
    body: Option<String>,
    anti_replay_secret: Option<String>,
    concurrency: usize,
    duration_secs: u64,
    timeout_ms: u64,
    accept_invalid_certs: bool,
}

impl Config {
    fn from_args() -> Self {
        let mut config = Self {
            url: "http://127.0.0.1:8000/api/v2/health/ping".to_string(),
            method: Method::GET,
            headers: Vec::new(),
            body: None,
            anti_replay_secret: None,
            concurrency: 100,
            duration_secs: 30,
            timeout_ms: 5000,
            accept_invalid_certs: false,
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
                "--url" => config.url = value,
                "--method" => {
                    config.method = value.parse::<Method>().unwrap_or_else(|_| {
                        eprintln!("invalid value for --method: {value}");
                        std::process::exit(2);
                    });
                }
                "--header" => config.headers.push(parse_header(&value)),
                "--body" => config.body = Some(value),
                "--anti-replay-secret" => config.anti_replay_secret = Some(value),
                "--concurrency" => config.concurrency = parse_arg(&arg, &value),
                "--duration-sec" => config.duration_secs = parse_arg(&arg, &value),
                "--timeout-ms" => config.timeout_ms = parse_arg(&arg, &value),
                "--insecure" => config.accept_invalid_certs = parse_arg(&arg, &value),
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

#[derive(Default)]
struct Metrics {
    total: AtomicU64,
    success: AtomicU64,
    non_success: AtomicU64,
    errors: AtomicU64,
    bytes: AtomicU64,
    status_2xx: AtomicU64,
    status_3xx: AtomicU64,
    status_4xx: AtomicU64,
    status_5xx: AtomicU64,
    status_200: AtomicU64,
    status_400: AtomicU64,
    status_401: AtomicU64,
    status_403: AtomicU64,
    status_404: AtomicU64,
    status_429: AtomicU64,
    status_500: AtomicU64,
    status_502: AtomicU64,
    status_503: AtomicU64,
}

fn parse_arg<T>(name: &str, value: &str) -> T
where
    T: std::str::FromStr,
{
    value.parse::<T>().unwrap_or_else(|_| {
        eprintln!("invalid value for {name}: {value}");
        std::process::exit(2);
    })
}

fn print_help() {
    eprintln!(
        "Usage: qps-load-client --url URL --concurrency N --duration-sec S [--method METHOD] [--header 'Name: value'] [--body JSON] [--anti-replay-secret HEX] [--timeout-ms MS] [--insecure true|false]"
    );
}

fn emit_jsonl_summary(config: &Config, metrics: &Metrics, latencies_us: &[u64], elapsed_secs: f64) {
    let total = metrics.total.load(Ordering::Relaxed);
    let qps = if elapsed_secs > 0.0 {
        total as f64 / elapsed_secs
    } else {
        0.0
    };
    let p50 = percentile_ms(latencies_us, 50.0);
    let p95 = percentile_ms(latencies_us, 95.0);
    let p99 = percentile_ms(latencies_us, 99.0);
    let max_ms = latencies_us.last().copied().unwrap_or_default() as f64 / 1000.0;

    let summary = serde_json::json!({
        "type": "summary",
        "url": config.url,
        "method": config.method.as_str(),
        "concurrency": config.concurrency,
        "duration_sec": elapsed_secs,
        "total": total,
        "success": metrics.success.load(Ordering::Relaxed),
        "non_success": metrics.non_success.load(Ordering::Relaxed),
        "errors": metrics.errors.load(Ordering::Relaxed),
        "bytes": metrics.bytes.load(Ordering::Relaxed),
        "qps": format!("{:.2}", qps),
        "p50_ms": format!("{:.3}", p50),
        "p95_ms": format!("{:.3}", p95),
        "p99_ms": format!("{:.3}", p99),
        "max_ms": format!("{:.3}", max_ms),
        "status_2xx": metrics.status_2xx.load(Ordering::Relaxed),
        "status_3xx": metrics.status_3xx.load(Ordering::Relaxed),
        "status_4xx": metrics.status_4xx.load(Ordering::Relaxed),
        "status_5xx": metrics.status_5xx.load(Ordering::Relaxed),
        "status_200": metrics.status_200.load(Ordering::Relaxed),
        "status_400": metrics.status_400.load(Ordering::Relaxed),
        "status_401": metrics.status_401.load(Ordering::Relaxed),
        "status_403": metrics.status_403.load(Ordering::Relaxed),
        "status_404": metrics.status_404.load(Ordering::Relaxed),
        "status_429": metrics.status_429.load(Ordering::Relaxed),
        "status_500": metrics.status_500.load(Ordering::Relaxed),
        "status_502": metrics.status_502.load(Ordering::Relaxed),
        "status_503": metrics.status_503.load(Ordering::Relaxed),
    });
    println!("jsonl={}", serde_json::to_string(&summary).unwrap_or_default());
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let config = Config::from_args();
    let metrics = Arc::new(Metrics::default());
    let default_headers = build_default_headers(&config.headers);
    let client = Client::builder()
        .pool_max_idle_per_host(config.concurrency)
        .tcp_keepalive(Duration::from_secs(30))
        .timeout(Duration::from_millis(config.timeout_ms))
        .default_headers(default_headers)
        .danger_accept_invalid_certs(config.accept_invalid_certs)
        .build()
        .expect("failed to build HTTP client");

    eprintln!("config={config:?}");

    let started = Instant::now();
    let deadline = started + Duration::from_secs(config.duration_secs);

    let reporter_metrics = metrics.clone();
    let reporter_started = started;
    let reporter = tokio::spawn(async move {
        let mut last_total = 0;
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            let total = reporter_metrics.total.load(Ordering::Relaxed);
            let elapsed = reporter_started.elapsed().as_secs_f64();
            eprintln!(
                "elapsed={elapsed:.1}s total={} qps_1s={} success={} non_success={} errors={} bytes={} 2xx={} 4xx={} 5xx={}",
                total,
                total.saturating_sub(last_total),
                reporter_metrics.success.load(Ordering::Relaxed),
                reporter_metrics.non_success.load(Ordering::Relaxed),
                reporter_metrics.errors.load(Ordering::Relaxed),
                reporter_metrics.bytes.load(Ordering::Relaxed),
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
        let url = config.url.clone();
        let method = config.method.clone();
        let body = config.body.clone();
        let anti_replay_secret = config.anti_replay_secret.clone();
        let metrics = metrics.clone();
        tasks.push(tokio::spawn(async move {
            let mut latencies_us = Vec::new();
            while Instant::now() < deadline {
                let request_started = Instant::now();
                let mut request = client.request(method.clone(), &url);
                if let Some(body) = body.as_ref() {
                    request = request.body(body.clone());
                }
                if let Some(secret) = anti_replay_secret.as_deref() {
                    for (name, value) in anti_replay_headers(&method, &url, body.as_deref(), secret) {
                        request = request.header(name, value);
                    }
                }
                match request.send().await {
                    Ok(response) => {
                        let status = response.status();
                        let status_success = status.is_success();
                        let status_code = status.as_u16();
                        match response.bytes().await {
                            Ok(bytes) => {
                                metrics.bytes.fetch_add(bytes.len() as u64, Ordering::Relaxed);
                                if status_success {
                                    metrics.success.fetch_add(1, Ordering::Relaxed);
                                } else {
                                    metrics.non_success.fetch_add(1, Ordering::Relaxed);
                                }
                                record_status_code(&metrics, status_code);
                            }
                            Err(_) => {
                                metrics.errors.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }
                    Err(_) => {
                        metrics.errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
                metrics.total.fetch_add(1, Ordering::Relaxed);
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
        };
    }
    reporter.abort();

    latencies_us.sort_unstable();
    let elapsed_secs = started.elapsed().as_secs_f64();
    let total = metrics.total.load(Ordering::Relaxed);
    let success = metrics.success.load(Ordering::Relaxed);
    let non_success = metrics.non_success.load(Ordering::Relaxed);
    let errors = metrics.errors.load(Ordering::Relaxed);
    let bytes = metrics.bytes.load(Ordering::Relaxed);

    let status_2xx = metrics.status_2xx.load(Ordering::Relaxed);
    let status_3xx = metrics.status_3xx.load(Ordering::Relaxed);
    let status_4xx = metrics.status_4xx.load(Ordering::Relaxed);
    let status_5xx = metrics.status_5xx.load(Ordering::Relaxed);

    println!(
        "summary url={} concurrency={} duration_sec={:.3} total={} success={} non_success={} errors={} bytes={} qps={:.2} p50_ms={:.3} p95_ms={:.3} p99_ms={:.3} max_ms={:.3} status_2xx={} status_3xx={} status_4xx={} status_5xx={} status_200={} status_400={} status_401={} status_403={} status_404={} status_429={} status_500={} status_502={} status_503={}",
        config.url,
        config.concurrency,
        elapsed_secs,
        total,
        success,
        non_success,
        errors,
        bytes,
        total as f64 / elapsed_secs,
        percentile_ms(&latencies_us, 50.0),
        percentile_ms(&latencies_us, 95.0),
        percentile_ms(&latencies_us, 99.0),
        latencies_us.last().copied().unwrap_or_default() as f64 / 1000.0,
        status_2xx,
        status_3xx,
        status_4xx,
        status_5xx,
        metrics.status_200.load(Ordering::Relaxed),
        metrics.status_400.load(Ordering::Relaxed),
        metrics.status_401.load(Ordering::Relaxed),
        metrics.status_403.load(Ordering::Relaxed),
        metrics.status_404.load(Ordering::Relaxed),
        metrics.status_429.load(Ordering::Relaxed),
        metrics.status_500.load(Ordering::Relaxed),
        metrics.status_502.load(Ordering::Relaxed),
        metrics.status_503.load(Ordering::Relaxed),
    );

    emit_jsonl_summary(&config, &metrics, &latencies_us, elapsed_secs);
}

fn record_status_code(metrics: &Metrics, code: u16) {
    match code {
        200 => {
            metrics.status_200.fetch_add(1, Ordering::Relaxed);
        }
        400 => {
            metrics.status_400.fetch_add(1, Ordering::Relaxed);
        }
        401 => {
            metrics.status_401.fetch_add(1, Ordering::Relaxed);
        }
        403 => {
            metrics.status_403.fetch_add(1, Ordering::Relaxed);
        }
        404 => {
            metrics.status_404.fetch_add(1, Ordering::Relaxed);
        }
        429 => {
            metrics.status_429.fetch_add(1, Ordering::Relaxed);
        }
        500 => {
            metrics.status_500.fetch_add(1, Ordering::Relaxed);
        }
        502 => {
            metrics.status_502.fetch_add(1, Ordering::Relaxed);
        }
        503 => {
            metrics.status_503.fetch_add(1, Ordering::Relaxed);
        }
        _ => {}
    };
    match code {
        200..=299 => {
            metrics.status_2xx.fetch_add(1, Ordering::Relaxed);
        }
        300..=399 => {
            metrics.status_3xx.fetch_add(1, Ordering::Relaxed);
        }
        400..=499 => {
            metrics.status_4xx.fetch_add(1, Ordering::Relaxed);
        }
        500..=599 => {
            metrics.status_5xx.fetch_add(1, Ordering::Relaxed);
        }
        _ => {}
    };
}

fn parse_header(value: &str) -> (HeaderName, HeaderValue) {
    let Some((name, header_value)) = value.split_once(':') else {
        eprintln!("invalid header, expected 'Name: value': {value}");
        std::process::exit(2);
    };
    let name = HeaderName::from_bytes(name.trim().as_bytes()).unwrap_or_else(|error| {
        eprintln!("invalid header name {name}: {error}");
        std::process::exit(2);
    });
    let header_value = HeaderValue::from_str(header_value.trim()).unwrap_or_else(|error| {
        eprintln!("invalid header value for {name}: {error}");
        std::process::exit(2);
    });
    (name, header_value)
}

fn build_default_headers(headers: &[(HeaderName, HeaderValue)]) -> HeaderMap {
    let mut header_map = HeaderMap::new();
    for (name, value) in headers {
        header_map.insert(name.clone(), value.clone());
    }
    header_map
}

fn anti_replay_headers(
    method: &Method,
    url: &str,
    body: Option<&str>,
    session_secret: &str,
) -> [(HeaderName, String); 4] {
    let timestamp = current_unix_timestamp().to_string();
    let (pid, seed) = process_ctx();
    let random_part: u128 = rand::thread_rng().gen();
    let nonce = format!("qps-{}-{}-{:032x}-{}", pid, seed, random_part, next_nonce());
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

fn current_unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_secs()
}

fn next_nonce() -> u64 {
    static NONCE_COUNTER: AtomicU64 = AtomicU64::new(1);
    NONCE_COUNTER.fetch_add(1, Ordering::Relaxed)
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
