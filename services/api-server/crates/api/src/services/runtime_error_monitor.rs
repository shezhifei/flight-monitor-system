use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::future::Future;
use std::sync::{Arc, OnceLock, Weak};
use tokio::sync::Mutex;

use crate::services::runtime_error_types::{ErrorCategory, RuntimeErrorKind, Severity};
use crate::sse::hub::SseHub;

const ERROR_HISTORY_MAX_LEN: usize = 1000;
const RECENT_ERROR_MESSAGE_MAX_LEN: usize = 200;

/// OnceLock 本身是线程安全的（内部使用 Mutex），直接存储 Option<Weak>
/// 避免了额外的锁层和可能的死锁。
static GLOBAL_RUNTIME_ERROR_MONITOR: OnceLock<Option<Weak<RuntimeErrorMonitor>>> = OnceLock::new();
tokio::task_local! {
    static REQUEST_ERROR_RECORDING_CONTEXT: RefCell<RequestErrorRecordingContext>;
}

#[derive(Debug, Default, Clone)]
struct RequestErrorRecordingContext {
    operation: Option<String>,
    suppress_next_api_error_recording: bool,
}

#[derive(Debug, Clone)]
pub struct RuntimeErrorInput {
    pub error_type: RuntimeErrorKind,
    pub message: String,
    pub severity: Severity,
    pub category: ErrorCategory,
    pub operation: Option<String>,
    pub details: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeErrorRecord {
    pub error_id: String,
    pub timestamp: String,
    pub error_type: RuntimeErrorKind,
    pub message: String,
    pub severity: Severity,
    pub category: ErrorCategory,
    pub operation: Option<String>,
    pub details: Option<Value>,
}

#[derive(Debug)]
struct RuntimeErrorMetrics {
    start_time: DateTime<Utc>,
    total_errors: usize,
    total_requests: usize,
    error_counts: BTreeMap<String, usize>,
    severity_counts: BTreeMap<String, usize>,
    category_counts: BTreeMap<String, usize>,
    error_history: VecDeque<RuntimeErrorRecord>,
}

impl RuntimeErrorMetrics {
    fn new() -> Self {
        Self {
            start_time: Utc::now(),
            total_errors: 0,
            total_requests: 0,
            error_counts: BTreeMap::new(),
            severity_counts: BTreeMap::new(),
            category_counts: BTreeMap::new(),
            error_history: VecDeque::with_capacity(ERROR_HISTORY_MAX_LEN),
        }
    }

    fn clear(&mut self) {
        self.total_errors = 0;
        self.error_counts.clear();
        self.severity_counts.clear();
        self.category_counts.clear();
        self.error_history.clear();
    }

    fn error_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.total_errors as f64 / self.total_requests as f64
        }
    }
}

pub struct RuntimeErrorMonitor {
    metrics: Mutex<RuntimeErrorMetrics>,
    sse_hub: Option<Arc<SseHub>>,
}

impl RuntimeErrorMonitor {
    pub fn new(sse_hub: Option<Arc<SseHub>>) -> Arc<Self> {
        Arc::new(Self {
            metrics: Mutex::new(RuntimeErrorMetrics::new()),
            sse_hub,
        })
    }

    pub async fn record_request(&self) {
        let mut metrics = self.metrics.lock().await;
        metrics.total_requests += 1;
    }

    pub async fn record_error(&self, input: RuntimeErrorInput) -> RuntimeErrorRecord {
        let (record, total_errors) = {
            let mut metrics = self.metrics.lock().await;
            metrics.total_errors += 1;
            *metrics.error_counts.entry(input.error_type.label()).or_insert(0) += 1;
            *metrics.severity_counts.entry(input.severity.to_string()).or_insert(0) += 1;
            *metrics.category_counts.entry(input.category.to_string()).or_insert(0) += 1;

            let record = RuntimeErrorRecord {
                error_id: format!("err_{}_{}", Utc::now().timestamp_millis(), metrics.total_errors),
                timestamp: Utc::now().to_rfc3339(),
                error_type: input.error_type,
                message: input.message,
                severity: input.severity,
                category: input.category,
                operation: input.operation,
                details: input.details,
            };

            if metrics.error_history.len() == ERROR_HISTORY_MAX_LEN {
                metrics.error_history.pop_front();
            }
            metrics.error_history.push_back(record.clone());
            (record, metrics.total_errors)
        };

        self.publish_realtime_error(&record, total_errors).await;
        record
    }

    pub async fn recent_errors(&self, limit: usize) -> Vec<Value> {
        let metrics = self.metrics.lock().await;
        metrics
            .error_history
            .iter()
            .rev()
            .take(limit.max(1))
            .map(recent_error_payload)
            .collect()
    }

    pub async fn get_error_report(&self, hours: i64) -> Value {
        let cutoff = Utc::now() - ChronoDuration::hours(hours.max(1));
        let metrics = self.metrics.lock().await;
        let recent_errors = metrics
            .error_history
            .iter()
            .filter(|record| parse_timestamp(&record.timestamp).is_some_and(|ts| ts > cutoff))
            .cloned()
            .collect::<Vec<_>>();

        let mut hourly_counts = BTreeMap::<String, usize>::new();
        let mut top_errors = BTreeMap::<String, usize>::new();
        let mut severity_distribution = BTreeMap::<String, usize>::new();

        for record in &recent_errors {
            if let Some(timestamp) = parse_timestamp(&record.timestamp) {
                let hour_key = timestamp.format("%Y-%m-%d %H:00").to_string();
                *hourly_counts.entry(hour_key).or_insert(0) += 1;
            }
            *top_errors.entry(record.error_type.label()).or_insert(0) += 1;
            *severity_distribution.entry(record.severity.to_string()).or_insert(0) += 1;
        }

        let mut top_errors = top_errors.into_iter().collect::<Vec<_>>();
        top_errors.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
        let top_errors = top_errors
            .into_iter()
            .take(10)
            .map(|(error_type, count)| json!({ "error_type": error_type, "count": count }))
            .collect::<Vec<_>>();
        let trend = hourly_counts
            .into_iter()
            .map(|(hour, count)| json!({ "hour": hour, "count": count }))
            .collect::<Vec<_>>();

        json!({
            "report_period_hours": hours.max(1),
            "metrics": metrics_payload(&metrics),
            "trend": trend,
            "top_errors": top_errors,
            "severity_distribution": severity_distribution,
        })
    }

    pub async fn clear(&self) {
        self.metrics.lock().await.clear();
    }

    pub async fn metrics_payload(&self) -> Value {
        let metrics = self.metrics.lock().await;
        metrics_payload(&metrics)
    }

    async fn publish_realtime_error(&self, record: &RuntimeErrorRecord, total_errors: usize) {
        let Some(hub) = &self.sse_hub else {
            return;
        };
        if hub.get_topic_subscriber_count("error_events") == 0 {
            return;
        }

        let emitted_at = Utc::now();
        let payload = json!({
            "message_type": "error_log",
            "errors_count": total_errors,
            "status": "degraded",
            "error_event": {
                "error_id": record.error_id,
                "timestamp": record.timestamp,
                "error_type": record.error_type,
                "message": truncate_message(&record.message, 500),
                "severity": record.severity,
                "category": record.category,
                "operation": record.operation,
                "emitted_at": emitted_at.to_rfc3339(),
                "emitted_at_ms": emitted_at.timestamp_millis(),
            }
        });

        let _ = hub.broadcast_event("error_events", Some("error_log"), payload).await;
    }
}

pub fn set_global_runtime_error_monitor(monitor: &Arc<RuntimeErrorMonitor>) {
    // OnceLock 本身是线程安全的，set 操作是原子的
    let _ = GLOBAL_RUNTIME_ERROR_MONITOR.set(Some(Arc::downgrade(monitor)));
}

pub fn global_runtime_error_monitor() -> Option<Arc<RuntimeErrorMonitor>> {
    GLOBAL_RUNTIME_ERROR_MONITOR
        .get()
        .and_then(|opt| opt.as_ref())
        .and_then(|weak| Weak::upgrade(weak))
}

pub fn record_runtime_error_background(input: RuntimeErrorInput) {
    if let Some(monitor) = global_runtime_error_monitor() {
        actix_web::rt::spawn(async move {
            monitor.record_error(input).await;
        });
    }
}

pub fn record_service_unavailable_background(
    message: impl Into<String>,
    operation: impl Into<String>,
    category: impl Into<String>,
) {
    use std::str::FromStr;
    let category_str: String = category.into();
    let category = ErrorCategory::from_str(&category_str).unwrap_or(ErrorCategory::Other);
    let operation_str: String = operation.into();
    record_runtime_error_background(RuntimeErrorInput {
        error_type: RuntimeErrorKind::ApiServiceUnavailable,
        message: message.into(),
        severity: Severity::High,
        category,
        operation: Some(operation_str),
        details: None,
    });
}

pub async fn scope_request_error_recording<F, R>(operation: Option<String>, future: F) -> R
where
    F: Future<Output = R>,
{
    REQUEST_ERROR_RECORDING_CONTEXT
        .scope(
            RefCell::new(RequestErrorRecordingContext {
                operation,
                suppress_next_api_error_recording: false,
            }),
            future,
        )
        .await
}

pub fn current_request_operation() -> Option<String> {
    REQUEST_ERROR_RECORDING_CONTEXT
        .try_with(|context| context.borrow().operation.clone())
        .ok()
        .flatten()
}

pub fn suppress_next_api_error_recording() {
    let _ = REQUEST_ERROR_RECORDING_CONTEXT.try_with(|context| {
        context.borrow_mut().suppress_next_api_error_recording = true;
    });
}

pub fn take_api_error_recording_suppressed() -> bool {
    REQUEST_ERROR_RECORDING_CONTEXT
        .try_with(|context| {
            let mut context = context.borrow_mut();
            let suppressed = context.suppress_next_api_error_recording;
            context.suppress_next_api_error_recording = false;
            suppressed
        })
        .unwrap_or(false)
}

fn metrics_payload(metrics: &RuntimeErrorMetrics) -> Value {
    let uptime = (Utc::now() - metrics.start_time).num_seconds().max(0) as f64;
    let total_errors = metrics.total_errors.max(1) as f64;
    let error_rate_by_category = metrics
        .category_counts
        .iter()
        .map(|(category, count)| (category.clone(), (*count as f64) / total_errors))
        .collect::<BTreeMap<_, _>>();

    json!({
        "total_errors": metrics.total_errors,
        "total_requests": metrics.total_requests,
        "error_rate": metrics.error_rate(),
        "uptime": uptime,
        "error_counts": metrics.error_counts,
        "severity_counts": metrics.severity_counts,
        "category_counts": metrics.category_counts,
        "error_rate_by_category": error_rate_by_category,
    })
}

fn recent_error_payload(record: &RuntimeErrorRecord) -> Value {
    json!({
        "error_type": record.error_type,
        "message": truncate_message(&record.message, RECENT_ERROR_MESSAGE_MAX_LEN),
        "timestamp": record.timestamp,
        "severity": record.severity,
        "category": record.category,
        "operation": record.operation,
    })
}

fn truncate_message(message: &str, max_len: usize) -> String {
    if message.chars().count() <= max_len {
        return message.to_string();
    }

    message.chars().take(max_len).collect::<String>()
}

fn parse_timestamp(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|timestamp| timestamp.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::{RuntimeErrorInput, RuntimeErrorMonitor, RECENT_ERROR_MESSAGE_MAX_LEN};
    use crate::services::runtime_error_types::{ErrorCategory, RuntimeErrorKind, Severity};

    #[actix_web::test]
    async fn recent_errors_are_returned_newest_first_and_truncated() {
        let monitor = RuntimeErrorMonitor::new(None);
        monitor.record_request().await;
        monitor.record_request().await;

        monitor
            .record_error(RuntimeErrorInput {
                error_type: RuntimeErrorKind::from_label("FirstError"),
                message: "first".to_string(),
                severity: Severity::Warning,
                category: ErrorCategory::System,
                operation: Some("first_op".to_string()),
                details: None,
            })
            .await;
        monitor
            .record_error(RuntimeErrorInput {
                error_type: RuntimeErrorKind::from_label("SecondError"),
                message: "x".repeat(250),
                severity: Severity::Error,
                category: ErrorCategory::Infrastructure,
                operation: Some("second_op".to_string()),
                details: None,
            })
            .await;

        let errors = monitor.recent_errors(10).await;
        assert_eq!(errors.len(), 2);
        assert_eq!(errors[0]["error_type"], "SecondError");
        assert_eq!(errors[1]["error_type"], "FirstError");
        assert_eq!(
            errors[0]["message"].as_str().map(str::len),
            Some(RECENT_ERROR_MESSAGE_MAX_LEN)
        );
    }

    #[actix_web::test]
    async fn error_report_matches_python_shape() {
        let monitor = RuntimeErrorMonitor::new(None);
        monitor.record_request().await;
        monitor.record_request().await;
        monitor.record_request().await;
        monitor.record_request().await;

        monitor
            .record_error(RuntimeErrorInput {
                error_type: RuntimeErrorKind::from_label("DatabaseError"),
                message: "db timeout".to_string(),
                severity: Severity::High,
                category: ErrorCategory::Infrastructure,
                operation: Some("scheduler:sync".to_string()),
                details: None,
            })
            .await;
        monitor
            .record_error(RuntimeErrorInput {
                error_type: RuntimeErrorKind::from_label("DatabaseError"),
                message: "db timeout".to_string(),
                severity: Severity::High,
                category: ErrorCategory::Infrastructure,
                operation: Some("scheduler:sync".to_string()),
                details: None,
            })
            .await;
        monitor
            .record_error(RuntimeErrorInput {
                error_type: RuntimeErrorKind::ApiInternalError,
                message: "route exploded".to_string(),
                severity: Severity::Critical,
                category: ErrorCategory::System,
                operation: Some("GET /api/v2/test".to_string()),
                details: None,
            })
            .await;

        let report = monitor.get_error_report(24).await;
        assert_eq!(report["report_period_hours"], 24);
        assert_eq!(report["metrics"]["total_errors"], 3);
        assert_eq!(report["metrics"]["total_requests"], 4);
        assert_eq!(report["severity_distribution"]["high"], 2);
        assert_eq!(report["severity_distribution"]["critical"], 1);
        assert_eq!(report["top_errors"][0]["error_type"], "DatabaseError");
        assert_eq!(report["top_errors"][0]["count"], 2);
        assert!(report["trend"].is_array());
    }

    #[actix_web::test]
    async fn clear_resets_errors_but_preserves_total_requests() {
        let monitor = RuntimeErrorMonitor::new(None);
        monitor.record_request().await;
        monitor.record_request().await;
        monitor
            .record_error(RuntimeErrorInput {
                error_type: RuntimeErrorKind::ApiInternalError,
                message: "boom".to_string(),
                severity: Severity::High,
                category: ErrorCategory::System,
                operation: Some("GET /boom".to_string()),
                details: None,
            })
            .await;

        monitor.clear().await;

        let metrics = monitor.metrics_payload().await;
        assert_eq!(metrics["total_errors"], 0);
        assert_eq!(metrics["total_requests"], 2);
    }
}
