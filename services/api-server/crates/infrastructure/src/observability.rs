//! Process-exported performance metrics helpers (Prometheus via `metrics`).

use std::time::{Duration, Instant};

use serde::Serialize;

/// Parse a boolean-like environment flag (`1`/`true`/`yes`/`on`).
pub fn env_flag(name: &str, default: bool) -> bool {
    match std::env::var(name) {
        Ok(raw) => matches!(raw.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"),
        Err(_) => default,
    }
}

pub fn redis_pipeline_enabled() -> bool {
    env_flag("REDIS_PIPELINE_ENABLED", false)
}

pub fn shadow_mode_enabled() -> bool {
    env_flag("ENABLE_SHADOW_MODE", false)
}

pub fn profiling_enabled() -> bool {
    env_flag("ENABLE_PROFILING", false)
}

pub fn record_serialization_duration(seconds: f64) {
    metrics::histogram!("fms_serialization_duration_seconds").record(seconds);
    metrics::counter!("fms_serialization_total").increment(1);
}

pub fn serialize_json<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    let started = Instant::now();
    let encoded = serde_json::to_string(value);
    record_serialization_duration(started.elapsed().as_secs_f64());
    encoded
}

pub fn serialize_json_pretty<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    let started = Instant::now();
    let encoded = serde_json::to_string_pretty(value);
    record_serialization_duration(started.elapsed().as_secs_f64());
    encoded
}

pub fn record_shadow_comparison(query_type: &str, old_latency: Duration, new_latency: Duration) {
    let old_latency_ms = old_latency.as_millis() as i64;
    let new_latency_ms = new_latency.as_millis() as i64;
    tracing::info!(query_type, old_latency_ms, new_latency_ms, "shadow mode comparison");
    metrics::counter!(
        "fms_shadow_comparisons_total",
        "query_type" => query_type.to_string()
    )
    .increment(1);
    metrics::histogram!(
        "fms_shadow_old_latency_seconds",
        "query_type" => query_type.to_string()
    )
    .record(old_latency.as_secs_f64());
    metrics::histogram!(
        "fms_shadow_new_latency_seconds",
        "query_type" => query_type.to_string()
    )
    .record(new_latency.as_secs_f64());
}

#[cfg(test)]
mod tests {
    use super::{env_flag, serialize_json};
    use serde::Serialize;

    #[test]
    fn env_flag_treats_common_truthy_values_as_enabled() {
        let key = "FMS_TEST_ENV_FLAG_TRUTHY";
        std::env::set_var(key, "true");
        assert!(env_flag(key, false));
        std::env::set_var(key, "0");
        assert!(!env_flag(key, true));
        std::env::remove_var(key);
        assert!(env_flag(key, true));
        assert!(!env_flag(key, false));
    }

    #[derive(Serialize)]
    struct Sample {
        id: u32,
        name: String,
    }

    #[test]
    fn serialize_json_returns_serde_output_and_is_valid_json() {
        let sample = Sample {
            id: 7,
            name: "copilot".to_string(),
        };
        let encoded = serialize_json(&sample).expect("serialize");
        let parsed: serde_json::Value = serde_json::from_str(&encoded).expect("round-trip");
        assert_eq!(parsed["id"], 7);
        assert_eq!(parsed["name"], "copilot");
    }
}
