use dashmap::DashMap;
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

const MAX_LATENCY_SAMPLES: usize = 1000;
const LATENCY_SAMPLE_SHARDS: usize = 10;
const LATENCY_SAMPLES_PER_SHARD: usize = MAX_LATENCY_SAMPLES / LATENCY_SAMPLE_SHARDS;

#[derive(Debug, Clone, Serialize, Default)]
pub struct RequestLatencySnapshot {
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
    pub avg: f64,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct AuthMetricsSnapshot {
    pub login_success: u64,
    pub login_failure: u64,
    pub login_total: u64,
    pub login_success_rate_pct: f64,
    pub refresh_success: u64,
    pub refresh_failure: u64,
    pub refresh_total: u64,
    pub refresh_success_rate_pct: f64,
    pub session_lost: u64,
    pub logout_total: u64,
    pub heartbeat_total: u64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct NotificationDeliveryMetricsSnapshot {
    pub push_attempts: u64,
    pub push_success: u64,
    pub push_success_rate_pct: f64,
    pub sse_attempts: u64,
    pub sse_success: u64,
    pub sse_success_rate_pct: f64,
    pub external_attempts: u64,
    pub external_success: u64,
    pub in_app_attempts: u64,
    pub in_app_success: u64,
    pub backfill_pending: u64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct MobileRealtimeMetricsSnapshot {
    pub sse_reconnects: u64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct PerformanceMetricsSnapshot {
    pub requests: RequestLatencySnapshot,
    pub auth: AuthMetricsSnapshot,
    pub notification_delivery: NotificationDeliveryMetricsSnapshot,
    pub mobile_realtime: MobileRealtimeMetricsSnapshot,
}

pub struct PerformanceMetricsService {
    latency_samples_ms: Vec<Mutex<VecDeque<f64>>>,
    next_latency_sample_shard: AtomicUsize,
    counters: DashMap<String, AtomicU64>,
}

impl PerformanceMetricsService {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            latency_samples_ms: (0..LATENCY_SAMPLE_SHARDS)
                .map(|_| Mutex::new(VecDeque::with_capacity(LATENCY_SAMPLES_PER_SHARD)))
                .collect(),
            next_latency_sample_shard: AtomicUsize::new(0),
            counters: DashMap::new(),
        })
    }

    pub fn record_request_latency(&self, latency_ms: f64) {
        if !latency_ms.is_finite() || latency_ms < 0.0 {
            return;
        }

        let shard_index =
            self.next_latency_sample_shard.fetch_add(1, Ordering::Relaxed) % self.latency_samples_ms.len();
        let mut state = self.latency_samples_ms[shard_index]
            .lock()
            .expect("performance metrics latency shard lock poisoned");
        if state.len() >= LATENCY_SAMPLES_PER_SHARD {
            state.pop_front();
        }
        state.push_back(latency_ms);
    }

    pub fn record_auth_login(&self, success: bool) {
        self.increment_counter(if success {
            "auth.login.success"
        } else {
            "auth.login.failure"
        });
    }

    pub fn record_auth_refresh(&self, success: bool, session_lost: bool) {
        self.increment_counter(if success {
            "auth.refresh.success"
        } else {
            "auth.refresh.failure"
        });
        if session_lost {
            self.increment_counter("auth.session_lost");
        }
    }

    pub fn record_auth_logout(&self) {
        self.increment_counter("auth.logout");
    }

    pub fn record_auth_heartbeat(&self) {
        self.increment_counter("auth.heartbeat");
    }

    pub fn record_notification_delivery(&self, channel: &str, success: bool) {
        let channel = normalize_channel(channel);
        self.increment_counter(&format!("notification.{channel}.attempts"));
        self.increment_counter(&format!(
            "notification.{channel}.{}",
            if success { "success" } else { "failure" }
        ));
    }

    pub fn record_notification_backfill_pending(&self) {
        self.increment_counter("notification.backfill.pending");
    }

    pub fn record_sse_reconnect(&self) {
        self.increment_counter("sse.reconnects");
    }

    pub fn record_latency(&self, key: &str, duration_ms: f64) {
        if !duration_ms.is_finite() || duration_ms < 0.0 {
            return;
        }
        self.increment_counter(&format!("{key}.count"));
        let total_key = format!("{key}.total_ms");
        let total = (duration_ms * 1000.0).round() as u64;
        if let Some(counter) = self.counters.get(&total_key) {
            counter.value().fetch_add(total, Ordering::Relaxed);
        } else {
            self.counters.entry(total_key).or_insert_with(|| AtomicU64::new(total));
        }
    }

    pub fn snapshot(&self) -> PerformanceMetricsSnapshot {
        let latency_samples = {
            let mut samples = VecDeque::with_capacity(MAX_LATENCY_SAMPLES);
            for shard in &self.latency_samples_ms {
                let state = shard.lock().expect("performance metrics latency shard lock poisoned");
                samples.extend(state.iter().copied());
            }
            samples
        };
        PerformanceMetricsSnapshot {
            requests: snapshot_request_latency(&latency_samples),
            auth: snapshot_auth_metrics(&self.counters),
            notification_delivery: snapshot_notification_metrics(&self.counters),
            mobile_realtime: MobileRealtimeMetricsSnapshot {
                sse_reconnects: counter(&self.counters, "sse.reconnects"),
            },
        }
    }

    pub fn increment_counter(&self, key: &str) {
        if let Some(counter) = self.counters.get(key) {
            counter.value().fetch_add(1, Ordering::Relaxed);
        } else {
            self.counters
                .entry(key.to_string())
                .or_insert_with(|| AtomicU64::new(0))
                .value()
                .fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn normalize_channel(channel: &str) -> &'static str {
    match channel.trim().to_ascii_lowercase().as_str() {
        "push" => "push",
        "sse" => "sse",
        "external" => "external",
        "in_app" => "in_app",
        _ => "unknown",
    }
}

fn snapshot_request_latency(samples: &VecDeque<f64>) -> RequestLatencySnapshot {
    if samples.is_empty() {
        return RequestLatencySnapshot::default();
    }

    let mut sorted = samples.iter().copied().collect::<Vec<_>>();
    sorted.sort_by(f64::total_cmp);
    let count = sorted.len() as u64;
    let avg = sorted.iter().sum::<f64>() / (count as f64);

    RequestLatencySnapshot {
        p50: round_to_2(percentile(&sorted, 50)),
        p95: round_to_2(percentile(&sorted, 95)),
        p99: round_to_2(percentile(&sorted, 99)),
        avg: round_to_2(avg),
        count,
    }
}

fn snapshot_auth_metrics(counters: &DashMap<String, AtomicU64>) -> AuthMetricsSnapshot {
    let login_success = counter(counters, "auth.login.success");
    let login_failure = counter(counters, "auth.login.failure");
    let refresh_success = counter(counters, "auth.refresh.success");
    let refresh_failure = counter(counters, "auth.refresh.failure");
    let login_total = login_success + login_failure;
    let refresh_total = refresh_success + refresh_failure;

    AuthMetricsSnapshot {
        login_success,
        login_failure,
        login_total,
        login_success_rate_pct: safe_rate(login_success, login_total),
        refresh_success,
        refresh_failure,
        refresh_total,
        refresh_success_rate_pct: safe_rate(refresh_success, refresh_total),
        session_lost: counter(counters, "auth.session_lost"),
        logout_total: counter(counters, "auth.logout"),
        heartbeat_total: counter(counters, "auth.heartbeat"),
    }
}

fn snapshot_notification_metrics(counters: &DashMap<String, AtomicU64>) -> NotificationDeliveryMetricsSnapshot {
    let push_attempts = counter(counters, "notification.push.attempts");
    let push_success = counter(counters, "notification.push.success");
    let sse_attempts = counter(counters, "notification.sse.attempts");
    let sse_success = counter(counters, "notification.sse.success");

    NotificationDeliveryMetricsSnapshot {
        push_attempts,
        push_success,
        push_success_rate_pct: safe_rate(push_success, push_attempts),
        sse_attempts,
        sse_success,
        sse_success_rate_pct: safe_rate(sse_success, sse_attempts),
        external_attempts: counter(counters, "notification.external.attempts"),
        external_success: counter(counters, "notification.external.success"),
        in_app_attempts: counter(counters, "notification.in_app.attempts"),
        in_app_success: counter(counters, "notification.in_app.success"),
        backfill_pending: counter(counters, "notification.backfill.pending"),
    }
}

fn counter(counters: &DashMap<String, AtomicU64>, key: &str) -> u64 {
    counters
        .get(key)
        .map(|v| v.value().load(Ordering::Relaxed))
        .unwrap_or(0)
}

fn percentile(sorted: &[f64], p: usize) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    if sorted.len() == 1 || p == 0 {
        return sorted[0];
    }
    if p >= 100 {
        return *sorted.last().unwrap_or(&0.0);
    }

    let rank = (p as f64 / 100.0) * ((sorted.len() - 1) as f64);
    let lower = rank.floor() as usize;
    let upper = rank.ceil() as usize;
    if lower == upper {
        return sorted[lower];
    }

    let fraction = rank - lower as f64;
    sorted[lower] + ((sorted[upper] - sorted[lower]) * fraction)
}

fn safe_rate(success: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    round_to_2((success as f64 / total as f64) * 100.0)
}

fn round_to_2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::PerformanceMetricsService;

    #[test]
    fn request_latency_snapshot_matches_python_percentiles() {
        let metrics = PerformanceMetricsService::new();
        for sample in [10.0, 20.0, 30.0, 40.0, 50.0] {
            metrics.record_request_latency(sample);
        }

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.requests.p50, 30.0);
        assert_eq!(snapshot.requests.p95, 48.0);
        assert_eq!(snapshot.requests.p99, 49.6);
        assert_eq!(snapshot.requests.avg, 30.0);
        assert_eq!(snapshot.requests.count, 5);
    }

    #[test]
    fn auth_and_notification_counters_follow_python_shape() {
        let metrics = PerformanceMetricsService::new();
        metrics.record_auth_login(true);
        metrics.record_auth_login(false);
        metrics.record_auth_refresh(true, false);
        metrics.record_auth_refresh(false, true);
        metrics.record_auth_logout();
        metrics.record_auth_heartbeat();
        metrics.record_notification_delivery("sse", true);
        metrics.record_notification_delivery("sse", false);
        metrics.record_notification_backfill_pending();
        metrics.record_sse_reconnect();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.auth.login_total, 2);
        assert_eq!(snapshot.auth.login_success_rate_pct, 50.0);
        assert_eq!(snapshot.auth.refresh_total, 2);
        assert_eq!(snapshot.auth.session_lost, 1);
        assert_eq!(snapshot.auth.logout_total, 1);
        assert_eq!(snapshot.auth.heartbeat_total, 1);
        assert_eq!(snapshot.notification_delivery.sse_attempts, 2);
        assert_eq!(snapshot.notification_delivery.sse_success, 1);
        assert_eq!(snapshot.notification_delivery.sse_success_rate_pct, 50.0);
        assert_eq!(snapshot.notification_delivery.backfill_pending, 1);
        assert_eq!(snapshot.mobile_realtime.sse_reconnects, 1);
    }

    #[test]
    fn request_latency_samples_remain_bounded_after_exceeding_limit() {
        let metrics = PerformanceMetricsService::new();
        for sample in 1..=(super::MAX_LATENCY_SAMPLES + 250) {
            metrics.record_request_latency(sample as f64);
        }

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.requests.count, super::MAX_LATENCY_SAMPLES as u64);
        assert!(snapshot.requests.avg.is_finite());
        assert!(snapshot.requests.p50.is_finite());
        assert!(snapshot.requests.p95.is_finite());
        assert!(snapshot.requests.p99.is_finite());
        assert!(snapshot.requests.p50 <= snapshot.requests.p95);
        assert!(snapshot.requests.p95 <= snapshot.requests.p99);
    }

    #[test]
    fn request_latency_snapshot_ignores_invalid_samples_with_shards() {
        let metrics = PerformanceMetricsService::new();
        metrics.record_request_latency(10.0);
        metrics.record_request_latency(f64::NAN);
        metrics.record_request_latency(f64::INFINITY);
        metrics.record_request_latency(-1.0);
        metrics.record_request_latency(30.0);

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.requests.count, 2);
        assert_eq!(snapshot.requests.p50, 20.0);
        assert_eq!(snapshot.requests.p95, 29.0);
        assert_eq!(snapshot.requests.p99, 29.8);
    }
}
