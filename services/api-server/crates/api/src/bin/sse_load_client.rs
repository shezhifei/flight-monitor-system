use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use reqwest::Client;
use tokio::sync::Semaphore;

#[derive(Debug, Clone)]
struct Config {
    url: String,
    connections: usize,
    ramp_step: usize,
    ramp_delay_ms: u64,
    duration_secs: u64,
    connect_concurrency: usize,
}

impl Config {
    fn from_args() -> Self {
        let mut config = Self {
            url: "http://127.0.0.1:19080/sse?topic=flights".to_string(),
            connections: 1000,
            ramp_step: 250,
            ramp_delay_ms: 1000,
            duration_secs: 60,
            connect_concurrency: 512,
        };

        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            let Some(value) = args.next() else {
                eprintln!("missing value for {arg}");
                std::process::exit(2);
            };
            match arg.as_str() {
                "--url" => config.url = value,
                "--connections" => config.connections = parse_arg(&arg, &value),
                "--ramp-step" => config.ramp_step = parse_arg(&arg, &value),
                "--ramp-delay-ms" => config.ramp_delay_ms = parse_arg(&arg, &value),
                "--duration-sec" => config.duration_secs = parse_arg(&arg, &value),
                "--connect-concurrency" => config.connect_concurrency = parse_arg(&arg, &value),
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                _ => {
                    eprintln!("unknown argument: {arg}");
                    print_help();
                    std::process::exit(2);
                }
            }
        }

        config.ramp_step = config.ramp_step.max(1);
        config.connect_concurrency = config.connect_concurrency.max(1);
        config
    }
}

#[derive(Default)]
struct Metrics {
    attempted: AtomicUsize,
    connected: AtomicUsize,
    failed: AtomicUsize,
    disconnected: AtomicUsize,
    bytes: AtomicU64,
    chunks: AtomicU64,
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
        "Usage: sse-load-client --url URL --connections N --duration-sec S [--ramp-step N] [--ramp-delay-ms MS] [--connect-concurrency N]"
    );
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let config = Config::from_args();
    let deadline = Instant::now() + Duration::from_secs(config.duration_secs);
    let metrics = Arc::new(Metrics::default());
    let client = Client::builder()
        .pool_max_idle_per_host(0)
        .tcp_keepalive(Duration::from_secs(30))
        .timeout(Duration::from_secs(config.duration_secs + 30))
        .build()
        .expect("failed to build HTTP client");
    let connect_limit = Arc::new(Semaphore::new(config.connect_concurrency));

    eprintln!("config={config:?}");

    let reporter_metrics = metrics.clone();
    let reporter = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            eprintln!(
                "attempted={} connected={} failed={} disconnected={} chunks={} bytes={}",
                reporter_metrics.attempted.load(Ordering::Relaxed),
                reporter_metrics.connected.load(Ordering::Relaxed),
                reporter_metrics.failed.load(Ordering::Relaxed),
                reporter_metrics.disconnected.load(Ordering::Relaxed),
                reporter_metrics.chunks.load(Ordering::Relaxed),
                reporter_metrics.bytes.load(Ordering::Relaxed)
            );
        }
    });

    let mut tasks = Vec::with_capacity(config.connections);
    for start in (0..config.connections).step_by(config.ramp_step) {
        let end = (start + config.ramp_step).min(config.connections);
        for index in start..end {
            let client = client.clone();
            let url = with_client_id(&config.url, index);
            let metrics = metrics.clone();
            let connect_limit = connect_limit.clone();
            tasks.push(tokio::spawn(async move {
                let _permit = connect_limit.acquire_owned().await.expect("semaphore should stay open");
                let response = open_connection(&client, &url, &metrics).await;
                drop(_permit);
                if let Some(response) = response {
                    drain_connection(response, deadline, metrics).await;
                }
            }));
        }
        tokio::time::sleep(Duration::from_millis(config.ramp_delay_ms)).await;
    }

    for task in tasks {
        let _ = task.await;
    }
    reporter.abort();

    println!(
        "summary attempted={} connected={} failed={} disconnected={} chunks={} bytes={}",
        metrics.attempted.load(Ordering::Relaxed),
        metrics.connected.load(Ordering::Relaxed),
        metrics.failed.load(Ordering::Relaxed),
        metrics.disconnected.load(Ordering::Relaxed),
        metrics.chunks.load(Ordering::Relaxed),
        metrics.bytes.load(Ordering::Relaxed)
    );
}

fn with_client_id(base_url: &str, index: usize) -> String {
    let separator = if base_url.contains('?') { '&' } else { '?' };
    format!("{base_url}{separator}user_id=load-{index}")
}

async fn open_connection(client: &Client, url: &str, metrics: &Metrics) -> Option<reqwest::Response> {
    metrics.attempted.fetch_add(1, Ordering::Relaxed);
    match client.get(url).send().await {
        Ok(response) if response.status().is_success() => response,
        Ok(_) | Err(_) => {
            metrics.failed.fetch_add(1, Ordering::Relaxed);
            return None;
        }
    }
    .into()
}

async fn drain_connection(response: reqwest::Response, deadline: Instant, metrics: Arc<Metrics>) {
    metrics.connected.fetch_add(1, Ordering::Relaxed);
    let mut stream = response.bytes_stream();
    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)) => {
                break;
            }
            chunk = stream.next() => {
                match chunk {
                    Some(Ok(bytes)) => {
                        metrics.chunks.fetch_add(1, Ordering::Relaxed);
                        metrics.bytes.fetch_add(bytes.len() as u64, Ordering::Relaxed);
                    }
                    Some(Err(_)) | None => {
                        metrics.disconnected.fetch_add(1, Ordering::Relaxed);
                        break;
                    }
                }
            }
        }
    }
}
