use crate::types::ConcreteAiCopilotBusinessCaseBatchRepository;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use fms_application::services::ai_admin_service::AiAdminService;
use fms_application::services::ai_business_case_copilot_service::AiBusinessCaseCopilotService;
use fms_application::services::ai_business_case_copilot_service::DEFAULT_COMMIT_RECOVERY_MAX_ATTEMPTS;
use fms_application::services::anomaly_service::AnomalyService;
use fms_application::services::cache_invalidation_service::CacheInvalidationSubscriberService;
use fms_application::services::dispatch_chat_service::DispatchChatService;
use fms_application::services::domain_event_relay_service::DomainEventRelay;
use fms_application::services::domain_event_subscriber_service::DomainEventSubscriberService;
use fms_application::services::flight_service::FlightService;
use fms_application::services::kpi_aggregation_service::KpiAggregationService;
use fms_application::services::online_status_service::OnlineStatusService;
use fms_application::services::system_ops_service::SystemOpsService;
use fms_application::services::todo_scheduler_service::TodoSchedulerService;
use fms_domain::ports::message_queue::{MessageHandler, PushConsumer};
use fms_domain::ports::FlightSyncRepository;
use serde::Serialize;
use serde_json::{json, Value};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tracing::{error, warn};

use crate::error::ApiError;
use crate::services::performance_metrics::{
    AuthMetricsSnapshot, MobileRealtimeMetricsSnapshot, NotificationDeliveryMetricsSnapshot, PerformanceMetricsService,
    RequestLatencySnapshot,
};
use crate::services::runtime_error_monitor::{RuntimeErrorInput, RuntimeErrorMonitor};
use crate::services::runtime_error_types::{ErrorCategory, RuntimeErrorKind, Severity};
use crate::services::task_status_types::TaskStatus;
use crate::sse::hub::{SseHub, SseStats};

type TaskFuture = Pin<Box<dyn Future<Output = Result<Value, String>> + Send>>;
type TaskRunner = Arc<dyn Fn() -> TaskFuture + Send + Sync>;
const AI_COPILOT_COMMIT_RECOVERY_INTERVAL_SECONDS_DEFAULT: i64 = 30;
const AI_COPILOT_COMMIT_RECOVERY_BATCH_SIZE_DEFAULT: i64 = 50;
const AI_COPILOT_COMMIT_RECOVERY_STALE_AFTER_SECONDS_DEFAULT: i64 = 120;
const AI_COPILOT_COMMIT_RECOVERY_MAX_ATTEMPTS_DEFAULT: i32 = DEFAULT_COMMIT_RECOVERY_MAX_ATTEMPTS;

/// 数据库连接池指标端口。
///
/// api 层不持有具体连接池类型（sqlx::PgPool 属于 infrastructure/server），
/// 只需要周期性读取 size/idle 两个数字，由 DI 注入实现。
pub trait DbPoolStatsSource: Send + Sync {
    fn pool_size(&self) -> u32;
    fn pool_num_idle(&self) -> u32;
}

/// 一次 Redis 往返探测的结果。
#[derive(Debug, Clone, Copy)]
pub struct RedisLatency {
    pub connected: bool,
    pub latency_ms: f64,
}

impl RedisLatency {
    /// 健康接口沿用既有的「未连接」哨兵值：latency_ms = -1。
    pub fn disconnected() -> Self {
        Self {
            connected: false,
            latency_ms: -1.0,
        }
    }
}

/// Redis 往返时延端口。
///
/// api 层不持有 redis 客户端（属于 infrastructure），只需要「是否连通 + 多少毫秒」
/// 两个数字，由 DI 注入实现。未配置 redis 时注入的适配器直接返回未连接。
#[async_trait::async_trait]
pub trait RedisLatencySource: Send + Sync {
    async fn measure(&self) -> RedisLatency;
}

#[derive(Debug, Clone, Serialize)]
pub struct SchedulerTaskExecutionResult {
    pub name: String,
    pub status: TaskStatus,
    pub started_at: String,
    pub finished_at: String,
    pub duration_ms: f64,
    pub error: Option<String>,
    pub result: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SchedulerTaskSnapshot {
    pub name: String,
    pub contract_name: Option<String>,
    pub interval_seconds: i64,
    pub is_async: bool,
    pub next_run: Option<String>,
    pub last_run: Option<String>,
    pub last_success: Option<String>,
    pub last_error: Option<String>,
    pub last_error_message: Option<String>,
    pub last_result: Option<Value>,
    pub run_count: u64,
    pub fail_count: u64,
    pub last_duration_ms: f64,
    pub status: TaskStatus,
}

#[derive(Debug, Clone, Serialize)]
pub struct SchedulerStatusSnapshot {
    pub running: bool,
    pub started_at: String,
    pub last_run: Option<String>,
    pub next_run: Option<String>,
    pub task_count: usize,
    pub tasks: Vec<SchedulerTaskSnapshot>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SchedulerTriggerResult {
    pub triggered: bool,
    pub task_names: Vec<String>,
    pub results: Vec<SchedulerTaskExecutionResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FlightSyncTriggerResult {
    pub success: bool,
    pub message: String,
    pub data: Value,
}

#[derive(Debug)]
struct TaskState {
    next_run: DateTime<Utc>,
    last_run: Option<DateTime<Utc>>,
    last_success: Option<DateTime<Utc>>,
    last_error: Option<DateTime<Utc>>,
    last_error_message: Option<String>,
    last_result: Option<Value>,
    run_count: u64,
    fail_count: u64,
    last_duration_ms: f64,
}

struct RegisteredTask {
    name: String,
    contract_name: Option<&'static str>,
    interval_seconds: i64,
    is_async: bool,
    runner: TaskRunner,
    running: AtomicBool,
    state: Mutex<TaskState>,
}

impl RegisteredTask {
    fn new(
        name: impl Into<String>,
        contract_name: Option<&'static str>,
        interval_seconds: i64,
        runner: TaskRunner,
    ) -> Self {
        Self {
            name: name.into(),
            contract_name,
            interval_seconds,
            is_async: true,
            runner,
            running: AtomicBool::new(false),
            state: Mutex::new(TaskState {
                next_run: Utc::now(),
                last_run: None,
                last_success: None,
                last_error: None,
                last_error_message: None,
                last_result: None,
                run_count: 0,
                fail_count: 0,
                last_duration_ms: 0.0,
            }),
        }
    }

    fn effective_interval_duration(&self) -> ChronoDuration {
        ChronoDuration::seconds(self.interval_seconds)
    }

    fn effective_sleep_duration(&self) -> std::time::Duration {
        std::time::Duration::from_secs(1)
    }
}

pub struct SchedulerRuntimeService {
    pool_stats: Arc<dyn DbPoolStatsSource>,
    redis_latency: Arc<dyn RedisLatencySource>,
    flight_sync_repo: Arc<dyn FlightSyncRepository>,
    sse_hub: Arc<SseHub>,
    error_monitor: Arc<RuntimeErrorMonitor>,
    performance_metrics: Arc<PerformanceMetricsService>,
    flight_service: Arc<FlightService>,
    dispatch_chat_service: Arc<DispatchChatService>,
    domain_event_relay_service: Arc<dyn DomainEventRelay>,
    domain_event_subscriber_service: Arc<DomainEventSubscriberService>,
    cache_invalidation_subscriber_service: Arc<CacheInvalidationSubscriberService>,
    todo_scheduler_service: Arc<TodoSchedulerService>,
    ai_business_case_copilot_service: Arc<AiBusinessCaseCopilotService<ConcreteAiCopilotBusinessCaseBatchRepository>>,
    anomaly_service: Arc<AnomalyService>,
    kpi_aggregation_service: Arc<KpiAggregationService>,
    ai_admin_service: Arc<AiAdminService>,
    system_ops_service: Arc<SystemOpsService>,
    _online_status_service: Arc<OnlineStatusService>,
    push_consumer: Option<Arc<dyn PushConsumer + Send + Sync>>,
    tasks: RwLock<Vec<Arc<RegisteredTask>>>,
    started_at: DateTime<Utc>,
    running: AtomicBool,
    last_run_at: Mutex<Option<DateTime<Utc>>>,
    last_error: Mutex<Option<String>>,
    loop_handle: Mutex<Option<JoinHandle<()>>>,
    source_system: String,
    domain_event_retry_recovery_interval_seconds: i64,
}

impl SchedulerRuntimeService {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        pool_stats: Arc<dyn DbPoolStatsSource>,
        redis_latency: Arc<dyn RedisLatencySource>,
        flight_sync_repo: Arc<dyn FlightSyncRepository>,
        sse_hub: Arc<SseHub>,
        error_monitor: Arc<RuntimeErrorMonitor>,
        performance_metrics: Arc<PerformanceMetricsService>,
        flight_service: Arc<FlightService>,
        dispatch_chat_service: Arc<DispatchChatService>,
        domain_event_relay_service: Arc<dyn DomainEventRelay>,
        domain_event_subscriber_service: Arc<DomainEventSubscriberService>,
        cache_invalidation_subscriber_service: Arc<CacheInvalidationSubscriberService>,
        todo_scheduler_service: Arc<TodoSchedulerService>,
        ai_business_case_copilot_service: Arc<
            AiBusinessCaseCopilotService<ConcreteAiCopilotBusinessCaseBatchRepository>,
        >,
        anomaly_service: Arc<AnomalyService>,
        kpi_aggregation_service: Arc<KpiAggregationService>,
        ai_admin_service: Arc<AiAdminService>,
        system_ops_service: Arc<SystemOpsService>,
        online_status_service: Arc<OnlineStatusService>,
        push_consumer: Option<Arc<dyn PushConsumer + Send + Sync>>,
        domain_event_retry_recovery_interval_seconds: i64,
    ) -> Arc<Self> {
        let service = Arc::new(Self {
            pool_stats,
            redis_latency,
            flight_sync_repo,
            sse_hub,
            error_monitor,
            performance_metrics,
            flight_service,
            dispatch_chat_service,
            domain_event_relay_service,
            domain_event_subscriber_service,
            cache_invalidation_subscriber_service,
            todo_scheduler_service,
            ai_business_case_copilot_service,
            anomaly_service,
            kpi_aggregation_service,
            ai_admin_service,
            system_ops_service,
            _online_status_service: online_status_service,
            push_consumer,
            tasks: RwLock::new(Vec::new()),
            started_at: Utc::now(),
            running: AtomicBool::new(false),
            last_run_at: Mutex::new(None),
            last_error: Mutex::new(None),
            loop_handle: Mutex::new(None),
            source_system: flight_sync_source_system(),
            domain_event_retry_recovery_interval_seconds: domain_event_retry_recovery_interval_seconds.max(1),
        });
        service.register_default_tasks().await;
        service
    }

    pub async fn start(self: &Arc<Self>) {
        if !scheduler_enabled() {
            return;
        }
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }

        // RocketMQ push consumer 是唯一消费路径：订阅失败或启动失败直接
        // panic，不再回退到 HTTP 轮询。
        self.start_push_consumer().await;

        let runtime = Arc::clone(self);
        let handle = tokio::spawn(async move {
            runtime.run_loop().await;
        });
        *self.loop_handle.lock().await = Some(handle);
    }

    async fn start_push_consumer(self: &Arc<Self>) {
        let Some(push_consumer) = self.push_consumer.clone() else {
            panic!(
                "push consumer 未配置：RocketMQ push consumer 是唯一消费路径，\
                 事件驱动为强制要求，请确认 ROCKETMQ_NAME_SERVER_ADDR 指向的 RocketMQ namesrv 可达"
            );
        };

        let domain_event_handler: Arc<dyn MessageHandler> =
            Arc::clone(&self.domain_event_subscriber_service) as Arc<dyn MessageHandler>;
        let cache_invalidation_handler: Arc<dyn MessageHandler> =
            Arc::clone(&self.cache_invalidation_subscriber_service) as Arc<dyn MessageHandler>;

        let domain_topic = self.domain_event_subscriber_service.topic().to_string();
        let cache_topic = self.cache_invalidation_subscriber_service.topic().to_string();

        let domain_group = self.domain_event_subscriber_service.consumer_group().to_string();
        let cache_group = self.cache_invalidation_subscriber_service.consumer_group().to_string();

        if let Err(error) = push_consumer
            .subscribe(&domain_topic, &domain_group, None, domain_event_handler)
            .await
        {
            error!(
                topic = %domain_topic,
                consumer_group = %domain_group,
                error = %error,
                "failed to subscribe domain events on push consumer"
            );
            panic!(
                "订阅 domain events topic（{domain_topic}）失败：{error}；\
                 事件驱动为强制要求，请确认 ROCKETMQ_NAME_SERVER_ADDR 指向的 RocketMQ namesrv 可达"
            );
        }

        if let Err(error) = push_consumer
            .subscribe(
                &cache_topic,
                &cache_group,
                Some("cache.invalidation"),
                cache_invalidation_handler,
            )
            .await
        {
            error!(
                topic = %cache_topic,
                consumer_group = %cache_group,
                error = %error,
                "failed to subscribe cache invalidations on push consumer"
            );
            panic!(
                "订阅 cache invalidation topic（{cache_topic}）失败：{error}；\
                 事件驱动为强制要求，请确认 ROCKETMQ_NAME_SERVER_ADDR 指向的 RocketMQ namesrv 可达"
            );
        }

        if let Err(error) = push_consumer.start().await {
            error!(
                error = %error,
                "failed to start push consumer"
            );
            panic!(
                "push consumer 启动失败：{error}；\
                 事件驱动为强制要求，请确认 ROCKETMQ_NAME_SERVER_ADDR 指向的 RocketMQ namesrv 可达"
            );
        }
    }

    pub async fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);

        // Shutdown push consumer (this also drains any in-flight callbacks).
        if let Some(push_consumer) = self.push_consumer.as_ref() {
            if let Err(error) = push_consumer.shutdown().await {
                warn!(
                    error = %error,
                    "push consumer shutdown failed"
                );
            }
        }

        if let Some(handle) = self.loop_handle.lock().await.take() {
            let _ = handle.await;
        }
    }

    pub async fn get_buffer_status(&self, flight_no: Option<String>, include_client_buffers: bool) -> Value {
        let stats = self.sse_hub.stats().await;
        build_buffer_status_payload(&stats, flight_no.as_deref(), include_client_buffers)
    }

    pub async fn get_sse_stats(&self) -> Value {
        let stats = self.sse_hub.stats().await;
        build_sse_stats_payload(&stats)
    }

    pub async fn get_performance_metrics(&self) -> Value {
        let sse_stats = self.sse_hub.stats().await;
        let metrics = self.performance_metrics.snapshot();
        let redis = self.redis_latency.measure().await;

        build_performance_metrics_payload(
            current_db_pool_metrics(self.pool_stats.as_ref()),
            &sse_stats,
            redis.connected,
            redis.latency_ms,
            &metrics.requests,
            &metrics.auth,
            &metrics.notification_delivery,
            &metrics.mobile_realtime,
        )
    }

    pub fn get_runtime_snapshot(&self) -> Value {
        let now = Utc::now();
        let uptime_seconds = (now - self.started_at).num_seconds().max(0);
        json!({
            "started_at": self.started_at.to_rfc3339(),
            "uptime_seconds": uptime_seconds,
            "uptime_human": format_duration(uptime_seconds),
            "timestamp": now.to_rfc3339(),
        })
    }

    pub async fn build_health_payload(
        &self,
        max_recent_errors: Option<usize>,
        include_performance: bool,
    ) -> Result<Value, String> {
        let public_health = self
            .system_ops_service
            .get_public_health()
            .await
            .map_err(|error| error.to_string())?;
        let flights = self
            .flight_service
            .list_flights(1, 1, None)
            .await
            .map_err(|error| error.to_string())?;
        let recent_error_limit = max_recent_errors.unwrap_or(50).max(1);
        let recent_errors = self.get_recent_errors(recent_error_limit).await;
        let buffer_status = self.get_buffer_status(None, false).await;
        let services = self.build_services_snapshot(&buffer_status).await?;
        let mut payload = json!({
            "success": true,
            "status": derive_health_status(
                public_health.get("status").and_then(Value::as_str),
                &recent_errors,
                &buffer_status,
                &services,
            ),
            "database": {
                "flights": flights.total,
            },
            "errors_count": recent_errors.len(),
            "recent_errors": recent_errors,
            "buffer_status": buffer_status,
            "services": services,
            "runtime": self.get_runtime_snapshot(),
        });

        if include_performance {
            payload["performance"] = self.get_performance_metrics().await;
        }

        Ok(payload)
    }

    pub async fn build_system_status_payload(&self, max_recent_errors: usize) -> Result<Value, String> {
        self.build_health_payload(Some(max_recent_errors), true).await
    }

    pub async fn clear_error_state(&self) {
        *self.last_error.lock().await = None;
        self.error_monitor.clear().await;

        let tasks = self.tasks.read().await.clone();
        for task in tasks {
            let mut state = task.state.lock().await;
            state.last_error = None;
            state.last_error_message = None;
        }
    }

    pub async fn get_recent_errors(&self, limit: usize) -> Vec<Value> {
        self.error_monitor.recent_errors(limit.min(50)).await
    }

    pub async fn get_error_report(&self, hours: i64) -> Value {
        self.error_monitor.get_error_report(hours).await
    }

    pub async fn get_scheduler_status_snapshot(&self) -> SchedulerStatusSnapshot {
        let tasks = self.tasks.read().await.clone();
        let mut snapshots = Vec::with_capacity(tasks.len());
        let mut next_run_at: Option<DateTime<Utc>> = None;

        for task in tasks {
            let state = task.state.lock().await;
            if next_run_at.map(|value| state.next_run < value).unwrap_or(true) {
                next_run_at = Some(state.next_run);
            }
            snapshots.push(SchedulerTaskSnapshot {
                name: task.name.clone(),
                contract_name: task.contract_name.map(str::to_string),
                interval_seconds: task.interval_seconds,
                is_async: task.is_async,
                next_run: Some(state.next_run.to_rfc3339()),
                last_run: state.last_run.map(|value| value.to_rfc3339()),
                last_success: state.last_success.map(|value| value.to_rfc3339()),
                last_error: state.last_error.map(|value| value.to_rfc3339()),
                last_error_message: state.last_error_message.clone(),
                last_result: state.last_result.clone(),
                run_count: state.run_count,
                fail_count: state.fail_count,
                last_duration_ms: state.last_duration_ms,
                status: task_runtime_status(&task, &state, self.running.load(Ordering::SeqCst)),
            });
        }

        SchedulerStatusSnapshot {
            running: self.running.load(Ordering::SeqCst),
            started_at: self.started_at.to_rfc3339(),
            last_run: self.last_run_at.lock().await.as_ref().map(|value| value.to_rfc3339()),
            next_run: next_run_at.map(|value| value.to_rfc3339()),
            task_count: snapshots.len(),
            tasks: snapshots,
            last_error: self.last_error.lock().await.clone(),
        }
    }

    pub async fn run_tasks_now(&self) -> SchedulerTriggerResult {
        let tasks = self.tasks.read().await.clone();
        if tasks.is_empty() {
            return SchedulerTriggerResult {
                triggered: false,
                task_names: Vec::new(),
                results: Vec::new(),
            };
        }

        let mut results = Vec::with_capacity(tasks.len());
        let mut task_names = Vec::with_capacity(tasks.len());
        for task in tasks {
            task_names.push(task.name.clone());
            results.push(self.execute_task(task).await);
        }

        SchedulerTriggerResult {
            triggered: !task_names.is_empty(),
            task_names,
            results,
        }
    }

    pub async fn get_bulk_update_summary(&self) -> Result<FlightSyncTriggerResult, ApiError> {
        let flights = self.flight_service.list_flights(1, 2000, None).await?;
        Ok(FlightSyncTriggerResult {
            success: true,
            message: "当前版本未配置自动批量状态刷新规则，已返回航班总量".to_string(),
            data: json!({
                "updated": false,
                "timestamp": Utc::now().to_rfc3339(),
                "flights_total": flights.items.len(),
                "operation": "bulk_update_not_configured",
            }),
        })
    }

    pub async fn get_flight_sync_status(&self) -> Result<Value, ApiError> {
        let latest = self
            .flight_sync_repo
            .find_latest(&self.source_system)
            .await
            .map_err(|e| ApiError::Internal(format!("failed to query flight sync status: {e}")))?;

        if let Some(mut payload) = latest {
            if let Some(object) = payload.as_object_mut() {
                object.insert("available".to_string(), Value::Bool(true));
            }
            return Ok(payload);
        }

        Ok(json!({
            "available": false,
            "source_system": self.source_system,
            "message": "暂无同步记录",
        }))
    }

    pub async fn run_flight_sync_now(&self) -> Result<FlightSyncTriggerResult, ApiError> {
        let now = Utc::now();
        let run_id = ulid::Ulid::new().to_string();
        let window_start = now.date_naive();
        let window_end = window_start;

        self.flight_sync_repo
            .create_run(
                &run_id,
                &self.source_system,
                "manual",
                "both",
                window_start,
                window_end,
                "running",
                now,
            )
            .await
            .map_err(|e| ApiError::Internal(format!("failed to create flight sync run: {e}")))?;

        let trigger_result = match self.flight_service.list_flights(1, 2000, None).await {
            Ok(flights) => {
                let flights_total = flights.items.len() as i32;
                let completed_at = Utc::now();
                let failure_samples: Vec<Value> = vec![];
                let error_summary: Vec<Value> = vec![];
                self.flight_sync_repo
                    .mark_completed(
                        &run_id,
                        flights_total,
                        flights_total,
                        0_i32,
                        0_i32,
                        0_i32,
                        &failure_samples,
                        &error_summary,
                        completed_at,
                    )
                    .await
                    .map_err(|e| ApiError::Internal(format!("failed to complete flight sync run: {e}")))?;

                let payload = self.load_flight_sync_payload(&run_id).await?;
                FlightSyncTriggerResult {
                    success: true,
                    message: "本站航班同步已触发".to_string(),
                    data: payload,
                }
            }
            Err(error) => {
                let completed_at = Utc::now();
                let error_text = error.to_string();
                let error_summary = vec![json!({ "message": error_text })];
                let failure_samples: Vec<Value> = vec![];
                self.flight_sync_repo
                    .mark_failed(&run_id, 1_i32, &failure_samples, &error_summary, completed_at)
                    .await
                    .map_err(|update_error| {
                        ApiError::Internal(format!("failed to record flight sync failure: {update_error}"))
                    })?;
                return Err(ApiError::Internal(format!("flight sync run failed: {error}")));
            }
        };

        Ok(trigger_result)
    }

    async fn load_flight_sync_payload(&self, run_id: &str) -> Result<Value, ApiError> {
        self.flight_sync_repo
            .load_payload(run_id)
            .await
            .map_err(|e| ApiError::Internal(format!("failed to load flight sync run: {e}")))
    }

    async fn build_services_snapshot(&self, buffer_status: &Value) -> Result<Value, String> {
        let auth_runtime = self
            .system_ops_service
            .get_online_status_runtime_status()
            .await
            .map_err(|error| error.to_string())?;
        let db_pool = current_db_pool_metrics(self.pool_stats.as_ref());
        let redis_available = auth_runtime
            .get("redis_available")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let redis_mode = auth_runtime
            .get("mode")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(if redis_available { "redis" } else { "fallback" });
        let postgres_status = if db_pool.max > 0 { "healthy" } else { "down" };
        let postgres_detail = if db_pool.max > 0 {
            format!("OK ({} / {} in use)", db_pool.active, db_pool.max)
        } else {
            "connection unavailable".to_string()
        };
        let redis_status = if redis_available && redis_mode.eq_ignore_ascii_case("redis") {
            "healthy"
        } else {
            "degraded"
        };
        let auth_status = if redis_mode.eq_ignore_ascii_case("redis") {
            "healthy"
        } else {
            "degraded"
        };
        let sse_state = buffer_status
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_ascii_lowercase();
        let sse_status = if sse_state == "active" { "healthy" } else { "degraded" };
        let ai_ready = self.ai_admin_service.has_usable_ai_config().await.unwrap_or(false);
        let ai_registry = self.ai_admin_service.registry_status_payload();
        let registered_tools = ai_registry.get("total_tools").and_then(Value::as_u64).unwrap_or(0);
        let ai_components = json!({
            "config_store": ai_ready,
            "executor": ai_registry
                .get("is_initialized")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            "todo_agent_service": true,
            "conversation_manager": false,
            "context_manager": false,
        });
        let ai_core_ready = ai_components
            .get("config_store")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            && ai_components.get("executor").and_then(Value::as_bool).unwrap_or(false)
            && ai_components
                .get("todo_agent_service")
                .and_then(Value::as_bool)
                .unwrap_or(false);
        let ai_has_any_component = ai_components
            .as_object()
            .map(|components| components.values().any(|value| value.as_bool().unwrap_or(false)))
            .unwrap_or(false);
        let ai_status = if !ai_has_any_component {
            "down"
        } else if ai_core_ready {
            "healthy"
        } else {
            "degraded"
        };
        let ai_detail = if !ai_has_any_component {
            "AI components not initialized".to_string()
        } else if ai_core_ready {
            "AI executor chain ready".to_string()
        } else {
            "AI components partially initialized".to_string()
        };

        Ok(json!({
            "api_server": {
                "status": "healthy",
                "detail": format!("Rust API v{}", env!("CARGO_PKG_VERSION")),
                "version": env!("CARGO_PKG_VERSION"),
                "uptime_seconds": self.get_runtime_snapshot()["uptime_seconds"].as_i64().unwrap_or(0),
            },
            "postgres": {
                "status": postgres_status,
                "detail": postgres_detail,
                "active": db_pool.active,
                "idle": db_pool.idle,
                "max": db_pool.max,
                "usage_pct": if db_pool.max > 0 {
                    round_to_1((db_pool.active as f64 / db_pool.max as f64) * 100.0)
                } else {
                    0.0
                },
            },
            "redis": {
                "status": redis_status,
                "detail": if redis_available { "connected" } else { "fallback mode" },
                "available": redis_available,
                "mode": redis_mode,
            },
            "auth": {
                "status": auth_status,
                "detail": format!("JWT Enabled ({redis_mode})"),
                "session_backend": auth_runtime,
            },
            "sse_gateway": {
                "status": sse_status,
                "detail": format!("{} connections", buffer_status.get("total_connections").and_then(Value::as_u64).unwrap_or(0)),
                "connections": buffer_status.get("total_connections").cloned().unwrap_or_else(|| json!(0)),
                "topics": buffer_status.get("topics").cloned().unwrap_or_else(|| json!({})),
            },
            "ai": {
                "status": ai_status,
                "detail": ai_detail,
                "strict_tool_permissions": ai_strict_tool_permissions_enabled(),
                "registered_tools": registered_tools,
                "components": ai_components,
                "smart_monitor": {
                    "enabled": false,
                    "running": false,
                },
            },
        }))
    }

    async fn register_default_tasks(self: &Arc<Self>) {
        let domain_event_relay_service = self.domain_event_relay_service.clone();
        self.register_task_runner(
            "domain_event_outbox_retry_recovery",
            None,
            self.domain_event_retry_recovery_interval_seconds,
            Arc::new(move || {
                let domain_event_relay_service = domain_event_relay_service.clone();
                Box::pin(async move {
                    let relayed_count = domain_event_relay_service
                        .recover_once()
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(augment_task_payload(
                        "domain_event_outbox_retry_recovery",
                        "domain_event_relay_service",
                        json!({
                            "topic": domain_event_relay_service.topic(),
                            "relayed_count": relayed_count,
                        }),
                    ))
                })
            }),
        )
        .await;

        // MQ 消息消费只走 push consumer 回调（见 start_push_consumer），
        // 这里不再注册 HTTP 轮询回退任务。

        let dispatch_chat_service = self.dispatch_chat_service.clone();
        self.register_task_runner(
            "dispatch_chat_deprecation",
            None,
            60,
            Arc::new(move || {
                let dispatch_chat_service = dispatch_chat_service.clone();
                Box::pin(async move {
                    let summary = dispatch_chat_service
                        .deprecate_due_groups_once(200)
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(augment_task_payload(
                        "dispatch_chat_deprecation",
                        "dispatch_chat_service",
                        json!({
                            "summary": summary,
                        }),
                    ))
                })
            }),
        )
        .await;

        let dispatch_chat_service = self.dispatch_chat_service.clone();
        self.register_task_runner(
            "dispatch_chat_archive",
            None,
            300,
            Arc::new(move || {
                let dispatch_chat_service = dispatch_chat_service.clone();
                Box::pin(async move {
                    let (summary, changes) = dispatch_chat_service
                        .archive_due_groups_once(200)
                        .await
                        .map_err(|error| error.to_string())?;
                    let archived_group_ids = changes
                        .into_iter()
                        .filter_map(|change| match change {
                            fms_application::services::dispatch_chat_service::DispatchChatLifecycleChange::Archived { group_id, .. } => Some(group_id),
                            fms_application::services::dispatch_chat_service::DispatchChatLifecycleChange::Upserted { .. } => None,
                        })
                        .collect::<Vec<_>>();
                    Ok(augment_task_payload(
                        "dispatch_chat_archive",
                        "dispatch_chat_service",
                        json!({
                            "summary": summary,
                            "archived_group_ids": archived_group_ids,
                        }),
                    ))
                })
            }),
        )
        .await;

        let kpi_aggregation_service = self.kpi_aggregation_service.clone();
        self.register_task_runner(
            "kpi_cache_refresh",
            None,
            300,
            Arc::new(move || {
                let kpi_aggregation_service = kpi_aggregation_service.clone();
                Box::pin(async move {
                    let details = kpi_aggregation_service
                        .refresh_cache()
                        .await
                        .map_err(|error| error.to_string())?;

                    Ok(augment_task_payload(
                        "kpi_cache_refresh",
                        "kpi_aggregation_service",
                        details,
                    ))
                })
            }),
        )
        .await;

        let todo_scheduler_service = self.todo_scheduler_service.clone();
        self.register_task_runner(
            "todo_scheduler",
            None,
            300,
            Arc::new(move || {
                let todo_scheduler_service = todo_scheduler_service.clone();
                Box::pin(async move {
                    let summary = todo_scheduler_service
                        .run_once()
                        .await
                        .map_err(|error| error.to_string())?;
                    let overdue_count = summary.overdue_ids.len();
                    let unblocked_count = summary.unblocked_ids.len();
                    let escalated_count = summary.escalated_ids.len();
                    Ok(augment_task_payload(
                        "todo_scheduler",
                        "todo_scheduler_service",
                        json!({
                            "overdue_ids": summary.overdue_ids,
                            "unblocked_ids": summary.unblocked_ids,
                            "escalated_ids": summary.escalated_ids,
                            "overdue_count": overdue_count,
                            "unblocked_count": unblocked_count,
                            "escalated_count": escalated_count,
                        }),
                    ))
                })
            }),
        )
        .await;

        let ai_business_case_copilot_service = self.ai_business_case_copilot_service.clone();
        self.register_task_runner(
            "ai_copilot_commit_recovery",
            None,
            ai_copilot_commit_recovery_interval_seconds(),
            Arc::new(move || {
                let ai_business_case_copilot_service = ai_business_case_copilot_service.clone();
                Box::pin(async move {
                    let summary = ai_business_case_copilot_service
                        .recover_stale_commits_once(
                            ai_copilot_commit_recovery_batch_size(),
                            ai_copilot_commit_recovery_stale_after_seconds(),
                            ai_copilot_commit_recovery_max_attempts(),
                        )
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(augment_task_payload(
                        "ai_copilot_commit_recovery",
                        "ai_business_case_copilot_service",
                        serde_json::to_value(summary).unwrap_or_else(|_| json!({"error":"summary_serialize_failed"})),
                    ))
                })
            }),
        )
        .await;

        let ai_business_case_copilot_service = self.ai_business_case_copilot_service.clone();
        self.register_task_runner(
            "ai_copilot_workflow_dispatch_retry",
            None,
            env_i64("AI_COPILOT_WORKFLOW_DISPATCH_RETRY_INTERVAL_SECONDS", 60),
            Arc::new(move || {
                let ai_business_case_copilot_service = ai_business_case_copilot_service.clone();
                Box::pin(async move {
                    let summary = ai_business_case_copilot_service
                        .retry_due_workflow_dispatches_once(
                            env_i64("AI_COPILOT_WORKFLOW_DISPATCH_RETRY_BATCH_SIZE", 50),
                            env_i32("AI_COPILOT_WORKFLOW_DISPATCH_MAX_ATTEMPTS", 5),
                        )
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(augment_task_payload(
                        "ai_copilot_workflow_dispatch_retry",
                        "ai_business_case_copilot_service",
                        serde_json::to_value(summary).unwrap_or_else(|_| json!({"error":"summary_serialize_failed"})),
                    ))
                })
            }),
        )
        .await;

        let anomaly_service = self.anomaly_service.clone();
        self.register_task_runner(
            "anomaly_detection_scan",
            None,
            600,
            Arc::new(move || {
                let anomaly_service = anomaly_service.clone();
                Box::pin(async move {
                    let stats = anomaly_service
                        .get_stats(None, None)
                        .await
                        .map_err(|error| error.to_string())?;
                    let enabled_rules = anomaly_service
                        .list_rules(true)
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(augment_task_payload(
                        "anomaly_detection_scan",
                        "anomaly_service",
                        json!({
                            "stats": serde_json::to_value(&stats).unwrap_or(Value::Null),
                            "enabled_rule_count": enabled_rules.len(),
                            "enabled_rule_ids": enabled_rules
                                .into_iter()
                                .map(|rule| rule.rule_id)
                                .collect::<Vec<_>>(),
                        }),
                    ))
                })
            }),
        )
        .await;

        let runtime = Arc::clone(self);
        self.register_task_runner(
            "system_monitor_broadcast",
            None,
            5,
            Arc::new(move || {
                let runtime = runtime.clone();
                Box::pin(async move {
                    if runtime.sse_hub.get_topic_subscriber_count("global_status") == 0 {
                        return Ok(augment_task_payload(
                            "system_monitor_broadcast",
                            "system_ops_service",
                            json!({
                                "broadcast_topic": "global_status",
                                "broadcast_event": "system_status",
                                "skipped": true,
                                "reason": "no_subscribers",
                            }),
                        ));
                    }
                    let payload = runtime
                        .build_system_status_payload(20)
                        .await
                        .map_err(|error| error.to_string())?;
                    let delivered = runtime
                        .sse_hub
                        .broadcast_event("global_status", Some("system_status"), payload.clone())
                        .await;

                    Ok(augment_task_payload(
                        "system_monitor_broadcast",
                        "system_ops_service",
                        json!({
                            "broadcast_topic": "global_status",
                            "broadcast_event": "system_status",
                            "delivered_connections": delivered,
                            "payload": payload,
                        }),
                    ))
                })
            }),
        )
        .await;
    }

    async fn register_task_runner(
        &self,
        name: &'static str,
        contract_name: Option<&'static str>,
        interval_seconds: i64,
        runner: TaskRunner,
    ) {
        self.tasks.write().await.push(Arc::new(RegisteredTask::new(
            name,
            contract_name,
            interval_seconds,
            runner,
        )));
    }

    async fn run_loop(self: Arc<Self>) {
        loop {
            if !self.running.load(Ordering::SeqCst) {
                break;
            }

            let now = Utc::now();
            let (due_tasks, min_sleep) = collect_due_tasks_for_tick(&self.tasks, now).await;

            for task in due_tasks {
                let _ = self.execute_task(task).await;
            }

            tokio::time::sleep(min_sleep).await;
        }
    }

    async fn execute_task(&self, task: Arc<RegisteredTask>) -> SchedulerTaskExecutionResult {
        let started_at = Utc::now();
        if task.running.swap(true, Ordering::SeqCst) {
            return SchedulerTaskExecutionResult {
                name: task.name.clone(),
                status: TaskStatus::Skipped,
                started_at: started_at.to_rfc3339(),
                finished_at: started_at.to_rfc3339(),
                duration_ms: 0.0,
                error: Some("task already running".to_string()),
                result: None,
            };
        }

        {
            let mut state = task.state.lock().await;
            state.last_run = Some(started_at);
        }
        *self.last_run_at.lock().await = Some(started_at);

        let started_perf = Instant::now();
        let outcome = (task.runner)().await;
        let finished_at = Utc::now();
        let duration_ms = (started_perf.elapsed().as_secs_f64() * 1000.0 * 100.0).round() / 100.0;

        let result = match outcome {
            Ok(payload) => {
                let mut state = task.state.lock().await;
                state.last_success = Some(finished_at);
                state.last_error = None;
                state.last_error_message = None;
                // Only store a summary to prevent unbounded memory growth
                // from large task result payloads
                state.last_result = Some(truncate_task_result(&payload));
                state.run_count += 1;
                state.last_duration_ms = duration_ms;
                SchedulerTaskExecutionResult {
                    name: task.name.clone(),
                    status: TaskStatus::Completed,
                    started_at: started_at.to_rfc3339(),
                    finished_at: finished_at.to_rfc3339(),
                    duration_ms,
                    error: None,
                    result: Some(payload),
                }
            }
            Err(error) => {
                {
                    let mut state = task.state.lock().await;
                    state.last_error = Some(finished_at);
                    state.last_error_message = Some(error.clone());
                    state.fail_count += 1;
                    state.last_duration_ms = duration_ms;
                }
                *self.last_error.lock().await = Some(error.clone());
                self.error_monitor
                    .record_error(RuntimeErrorInput {
                        error_type: RuntimeErrorKind::SchedulerTask,
                        message: error.clone(),
                        severity: Severity::Error,
                        category: ErrorCategory::System,
                        operation: Some(task.name.clone()),
                        details: Some(json!({
                            "task": task.name.clone(),
                            "contract_name": task.contract_name,
                            "interval_seconds": task.interval_seconds,
                        })),
                    })
                    .await;
                SchedulerTaskExecutionResult {
                    name: task.name.clone(),
                    status: TaskStatus::Failed,
                    started_at: started_at.to_rfc3339(),
                    finished_at: finished_at.to_rfc3339(),
                    duration_ms,
                    error: Some(error),
                    result: None,
                }
            }
        };

        task.running.store(false, Ordering::SeqCst);
        result
    }
}

async fn collect_due_tasks_for_tick(
    tasks: &RwLock<Vec<Arc<RegisteredTask>>>,
    now: DateTime<Utc>,
) -> (Vec<Arc<RegisteredTask>>, std::time::Duration) {
    let tasks_snapshot = { tasks.read().await.clone() };
    let mut due_tasks = Vec::new();
    let mut min_sleep = std::time::Duration::from_secs(1);

    for task in tasks_snapshot {
        let mut state = task.state.lock().await;
        let sleep_dur = task.effective_sleep_duration();
        if sleep_dur < min_sleep {
            min_sleep = sleep_dur;
        }
        if now >= state.next_run && !task.running.load(Ordering::SeqCst) {
            state.next_run = now + task.effective_interval_duration();
            due_tasks.push(task.clone());
        }
    }

    (due_tasks, min_sleep)
}

fn scheduler_enabled() -> bool {
    std::env::var("SCHEDULER_ENABLED")
        .map(|value| value.eq_ignore_ascii_case("true") || value == "1")
        .unwrap_or(false)
}

fn env_i64(key: &str, default: i64) -> i64 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(default)
}

fn ai_copilot_commit_recovery_interval_seconds() -> i64 {
    env_i64(
        "AI_COPILOT_COMMIT_RECOVERY_INTERVAL_SECONDS",
        AI_COPILOT_COMMIT_RECOVERY_INTERVAL_SECONDS_DEFAULT,
    )
}

fn ai_copilot_commit_recovery_batch_size() -> i64 {
    env_i64(
        "AI_COPILOT_COMMIT_RECOVERY_BATCH_SIZE",
        AI_COPILOT_COMMIT_RECOVERY_BATCH_SIZE_DEFAULT,
    )
}

fn ai_copilot_commit_recovery_stale_after_seconds() -> i64 {
    env_i64(
        "AI_COPILOT_COMMIT_RECOVERY_STALE_AFTER_SECONDS",
        AI_COPILOT_COMMIT_RECOVERY_STALE_AFTER_SECONDS_DEFAULT,
    )
}

fn ai_copilot_commit_recovery_max_attempts() -> i32 {
    env_i32(
        "AI_COPILOT_COMMIT_RECOVERY_MAX_ATTEMPTS",
        AI_COPILOT_COMMIT_RECOVERY_MAX_ATTEMPTS_DEFAULT,
    )
}

fn env_i32(key: &str, default: i32) -> i32 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or(default)
}

/// Truncate a task result payload to prevent unbounded memory growth
/// in the scheduler's persistent task state.
/// Uses safe UTF-8 boundary truncation to avoid panics.
fn truncate_task_result(value: &Value) -> Value {
    const MAX_RESULT_BYTES: usize = 4096;
    let serialized = serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string());
    if serialized.len() <= MAX_RESULT_BYTES {
        return value.clone();
    }
    // Safe truncation at UTF-8 boundary: find the last valid char boundary
    let truncation_point = serialized
        .char_indices()
        .take_while(|(i, _)| *i <= MAX_RESULT_BYTES)
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(MAX_RESULT_BYTES.min(serialized.len()));
    let truncated = &serialized[..truncation_point];
    // Try to parse as JSON; if truncation broke the structure, fall back to a summary
    serde_json::from_str::<Value>(truncated).unwrap_or_else(|_| {
        json!({
            "truncated": true,
            "original_size_bytes": serialized.len(),
            "preview": truncated,
        })
    })
}

fn flight_sync_source_system() -> String {
    ["FLIGHT_SYNC_SOURCE_SYSTEM", "FLIGHT_SYNC_SOURCE"]
        .iter()
        .find_map(|key| std::env::var(key).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "shenzhen_airport".to_string())
}

fn task_runtime_status(task: &RegisteredTask, state: &TaskState, scheduler_running: bool) -> TaskStatus {
    if task.running.load(Ordering::SeqCst) {
        return TaskStatus::Running;
    }
    if state
        .last_error
        .map(|error_at| {
            state
                .last_success
                .map(|success_at| error_at >= success_at)
                .unwrap_or(true)
        })
        .unwrap_or(false)
    {
        return TaskStatus::Error;
    }
    if scheduler_running {
        TaskStatus::Active
    } else {
        TaskStatus::Registered
    }
}

fn augment_task_payload(task: &str, service: &str, details: Value) -> Value {
    json!({
        "task": task,
        "service": service,
        "executed_at": Utc::now().to_rfc3339(),
        "details": details,
    })
}

fn build_buffer_status_payload(
    stats: &crate::sse::hub::SseStats,
    flight_no: Option<&str>,
    include_client_buffers: bool,
) -> Value {
    let timestamp = Utc::now().to_rfc3339();
    let total_queue_size = stats
        .connection_details
        .iter()
        .map(|detail| detail.queue_size)
        .sum::<usize>();
    let total_queue_capacity = stats
        .connection_details
        .iter()
        .map(|detail| detail.queue_maxsize)
        .sum::<usize>();
    let queue_full_count = stats
        .connection_details
        .iter()
        .filter(|detail| detail.queue_full)
        .count();
    let buffer_utilization = if total_queue_capacity > 0 {
        round_to_2((total_queue_size as f64 / total_queue_capacity as f64) * 100.0)
    } else {
        0.0
    };

    if let Some(flight_no) = flight_no.filter(|value| !value.trim().is_empty()) {
        let flight_clients = stats
            .connection_details
            .iter()
            .filter(|detail| detail.subscriptions.iter().any(|topic| topic == flight_no))
            .map(|detail| {
                json!({
                    "client_id": detail.client_id,
                    "queue_size": detail.queue_size,
                    "queue_maxsize": detail.queue_maxsize,
                    "queue_full": detail.queue_full,
                    "is_active": detail.is_active,
                    "subscriptions": detail.subscriptions,
                })
            })
            .collect::<Vec<_>>();
        let flight_queue_size = flight_clients
            .iter()
            .filter_map(|detail| detail.get("queue_size").and_then(Value::as_u64))
            .sum::<u64>();

        if flight_clients.is_empty() {
            return json!({
                "flight_no": flight_no,
                "status": "not_in_buffer",
                "message": format!("Flight {flight_no} is not currently in any client's subscription"),
                "subscribed_clients": 0,
                "total_queue_size": 0,
                "suggestion": "Check if the flight exists or if clients are subscribed to flight_updates topic",
                "all_connections_count": stats.total_connections,
                "all_topics": stats.topics,
                "timestamp": timestamp,
            });
        }

        return json!({
            "flight_no": flight_no,
            "status": "active",
            "subscribed_clients": flight_clients.len(),
            "total_queue_size": flight_queue_size,
            "client_buffers": flight_clients,
            "timestamp": timestamp,
        });
    }

    if !include_client_buffers {
        return json!({
            "total_connections": stats.total_connections,
            "total_queue_size": total_queue_size,
            "total_queue_capacity": total_queue_capacity,
            "buffer_utilization_percent": buffer_utilization,
            "queue_full_count": queue_full_count,
            "topics": stats.topics,
            "status": "active",
            "timestamp": timestamp,
        });
    }

    let client_buffers = stats
        .connection_details
        .iter()
        .map(|detail| {
            json!({
                "client_id": detail.client_id,
                "queue_size": detail.queue_size,
                "queue_maxsize": detail.queue_maxsize,
                "queue_full": detail.queue_full,
                "is_active": detail.is_active,
                "subscriptions": detail.subscriptions,
            })
        })
        .collect::<Vec<_>>();

    json!({
        "total_connections": stats.total_connections,
        "total_queue_size": total_queue_size,
        "total_queue_capacity": total_queue_capacity,
        "buffer_utilization_percent": buffer_utilization,
        "queue_full_count": queue_full_count,
        "topics": stats.topics,
        "client_buffers": client_buffers,
        "client_buffers_returned": stats.connection_details.len(),
        "client_buffers_total": stats.connection_details.len(),
        "client_buffers_truncated": false,
        "status": "active",
        "timestamp": timestamp,
    })
}

fn build_sse_stats_payload(stats: &SseStats) -> Value {
    json!({
        "active_connections": stats.active_connections,
        "total_connections": stats.total_connections,
        "active_connections_gauge": stats.active_connections_gauge,
        "lifetime_connections": stats.lifetime_connections,
        "lifetime_connections_counter": stats.lifetime_connections_counter,
        "messages_sent": stats.messages_sent,
        "messages_failed": stats.messages_failed,
        "messages_dropped": stats.messages_dropped,
        "topics": stats.topics,
        "connection_breakdown": {
            "connected": stats.connection_breakdown.connected,
            "inactive": stats.connection_breakdown.inactive,
        },
        "connection_details": stats.connection_details.iter().map(|detail| {
            json!({
                "client_id": detail.client_id,
                "is_active": detail.is_active,
                "last_heartbeat": detail.last_heartbeat,
                "time_since_heartbeat": detail.time_since_heartbeat,
                "queue_size": detail.queue_size,
                "queue_full": detail.queue_full,
                "dropped_messages": detail.dropped_messages,
                "subscriptions": detail.subscriptions,
            })
        }).collect::<Vec<_>>(),
        "heartbeat_interval": stats.heartbeat_interval,
        "max_connections": stats.max_connections,
        "connection_queue_size": stats.connection_queue_size,
        "cleanup_interval_seconds": stats.cleanup_interval_seconds,
        "heartbeat_timeout_seconds": stats.heartbeat_timeout_seconds,
        "queue_full_disconnect_seconds": stats.queue_full_disconnect_seconds,
    })
}

fn round_to_2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[derive(Debug, Clone, Copy)]
struct DbPoolMetrics {
    active: usize,
    idle: usize,
    max: usize,
}

pub fn scheduler_recent_errors(snapshot: &SchedulerStatusSnapshot, limit: Option<usize>) -> Vec<Value> {
    let mut items = snapshot
        .tasks
        .iter()
        .filter_map(|task| {
            let message = task.last_error_message.as_ref()?.trim();
            if message.is_empty() {
                return None;
            }

            Some(json!({
                "error_type": "scheduler_task",
                "message": message,
                "timestamp": task.last_error.clone().unwrap_or_else(|| snapshot.started_at.clone()),
                "severity": if task.status == TaskStatus::Error { "error" } else { "warning" },
                "category": task.contract_name.clone().unwrap_or_else(|| "scheduler".to_string()),
                "operation": task.name,
            }))
        })
        .collect::<Vec<_>>();

    items.sort_by(|left, right| {
        let left_ts = left.get("timestamp").and_then(Value::as_str).unwrap_or_default();
        let right_ts = right.get("timestamp").and_then(Value::as_str).unwrap_or_default();
        right_ts.cmp(left_ts)
    });

    if let Some(limit) = limit {
        items.truncate(limit);
    }

    items
}

pub fn derive_health_status(
    base_status: Option<&str>,
    recent_errors: &[Value],
    buffer_status: &Value,
    services: &Value,
) -> String {
    let mut has_degraded = !recent_errors.is_empty();

    let sse_state = buffer_status
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(sse_state.as_str(), "inactive" | "error" | "down") {
        has_degraded = true;
    }

    if let Some(services) = services.as_object() {
        for (service_name, service_value) in services {
            let service_status = service_value
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_ascii_lowercase();

            if matches!(service_status.as_str(), "down" | "error") {
                if matches!(service_name.as_str(), "api_server" | "postgres") {
                    return "down".to_string();
                }
                has_degraded = true;
            } else if service_status == "degraded" {
                has_degraded = true;
            }
        }
    }

    let normalized_base = base_status.unwrap_or("healthy").to_ascii_lowercase();
    if matches!(normalized_base.as_str(), "down" | "error") {
        return normalized_base;
    }
    if normalized_base == "degraded" || has_degraded {
        return "degraded".to_string();
    }
    "healthy".to_string()
}

fn current_db_pool_metrics(pool: &dyn DbPoolStatsSource) -> DbPoolMetrics {
    let pool_size = pool.pool_size() as usize;
    let idle = pool.pool_num_idle() as usize;
    let active = pool_size.saturating_sub(idle);
    let max = std::env::var("DB_POOL_MAX_CONNECTIONS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(pool_size.max(1));

    DbPoolMetrics { active, idle, max }
}

fn build_performance_metrics_payload(
    db_pool: DbPoolMetrics,
    sse_stats: &SseStats,
    redis_connected: bool,
    redis_latency_ms: f64,
    request_metrics: &RequestLatencySnapshot,
    auth_metrics: &AuthMetricsSnapshot,
    notification_metrics: &NotificationDeliveryMetricsSnapshot,
    mobile_realtime_metrics: &MobileRealtimeMetricsSnapshot,
) -> Value {
    let db_usage_pct = if db_pool.max > 0 {
        round_to_1((db_pool.active as f64 / db_pool.max as f64) * 100.0)
    } else {
        0.0
    };
    let sse_usage_pct = if sse_stats.max_connections > 0 {
        round_to_1((sse_stats.total_connections as f64 / sse_stats.max_connections as f64) * 100.0)
    } else {
        0.0
    };

    json!({
        "db_pool": {
            "active": db_pool.active,
            "idle": db_pool.idle,
            "max": db_pool.max,
            "usage_pct": db_usage_pct,
        },
        "redis": {
            "latency_ms": redis_latency_ms,
            "connected": redis_connected,
        },
        "sse": {
            "connections": sse_stats.total_connections,
            "max": sse_stats.max_connections,
            "usage_pct": sse_usage_pct,
        },
        "requests": {
            "p50": request_metrics.p50,
            "p95": request_metrics.p95,
            "p99": request_metrics.p99,
            "avg": request_metrics.avg,
            "count": request_metrics.count,
        },
        "auth": {
            "login_success": auth_metrics.login_success,
            "login_failure": auth_metrics.login_failure,
            "login_total": auth_metrics.login_total,
            "login_success_rate_pct": auth_metrics.login_success_rate_pct,
            "refresh_success": auth_metrics.refresh_success,
            "refresh_failure": auth_metrics.refresh_failure,
            "refresh_total": auth_metrics.refresh_total,
            "refresh_success_rate_pct": auth_metrics.refresh_success_rate_pct,
            "session_lost": auth_metrics.session_lost,
            "logout_total": auth_metrics.logout_total,
            "heartbeat_total": auth_metrics.heartbeat_total,
        },
        "notification_delivery": {
            "push_attempts": notification_metrics.push_attempts,
            "push_success": notification_metrics.push_success,
            "push_success_rate_pct": notification_metrics.push_success_rate_pct,
            "sse_attempts": notification_metrics.sse_attempts,
            "sse_success": notification_metrics.sse_success,
            "sse_success_rate_pct": notification_metrics.sse_success_rate_pct,
            "external_attempts": notification_metrics.external_attempts,
            "external_success": notification_metrics.external_success,
            "in_app_attempts": notification_metrics.in_app_attempts,
            "in_app_success": notification_metrics.in_app_success,
            "backfill_pending": notification_metrics.backfill_pending,
        },
        "mobile_realtime": {
            "sse_reconnects": mobile_realtime_metrics.sse_reconnects,
        },
        "timestamp": Utc::now().timestamp_millis() as f64 / 1000.0,
    })
}

fn ai_strict_tool_permissions_enabled() -> bool {
    std::env::var("AI_STRICT_TOOL_PERMISSIONS")
        .ok()
        .or_else(|| std::env::var("AI_SECURITY_STRICT_TOOL_PERMISSIONS").ok())
        .map(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

fn round_to_1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn format_duration(seconds: i64) -> String {
    let seconds = seconds.max(0);
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;

    if hours > 0 {
        return format!("{hours}h {minutes}m {secs}s");
    }
    if minutes > 0 {
        return format!("{minutes}m {secs}s");
    }
    format!("{secs}s")
}

#[cfg(test)]
mod tests {
    use super::{
        build_buffer_status_payload, build_performance_metrics_payload, build_sse_stats_payload, derive_health_status,
        format_duration, scheduler_recent_errors, AuthMetricsSnapshot, DbPoolMetrics, MobileRealtimeMetricsSnapshot,
        NotificationDeliveryMetricsSnapshot, RequestLatencySnapshot, SchedulerStatusSnapshot, SchedulerTaskSnapshot,
        TaskStatus, AI_COPILOT_COMMIT_RECOVERY_BATCH_SIZE_DEFAULT, AI_COPILOT_COMMIT_RECOVERY_INTERVAL_SECONDS_DEFAULT,
        AI_COPILOT_COMMIT_RECOVERY_MAX_ATTEMPTS_DEFAULT, AI_COPILOT_COMMIT_RECOVERY_STALE_AFTER_SECONDS_DEFAULT,
    };
    use crate::sse::hub::{SseConnectionBreakdown, SseConnectionDetail, SseStats};
    use crate::test_support::{load_python_runtime_parity_fixtures, normalize_runtime_parity_value};
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    fn sample_stats() -> SseStats {
        let mut topics = BTreeMap::new();
        topics.insert("MU100".to_string(), 1);
        topics.insert("flight_updates".to_string(), 1);
        SseStats {
            active_connections: 1,
            total_connections: 1,
            active_connections_gauge: 1,
            lifetime_connections: 1,
            lifetime_connections_counter: 1,
            messages_sent: 2,
            messages_failed: 0,
            messages_dropped: 0,
            topics,
            connection_breakdown: SseConnectionBreakdown {
                connected: 1,
                inactive: 0,
            },
            connection_details: vec![SseConnectionDetail {
                client_id: "client-buffer".to_string(),
                user_id: Some("ops".to_string()),
                is_active: true,
                connected_at: 1.0,
                last_message_at: 2.0,
                last_heartbeat: 2.0,
                time_since_heartbeat: 0.5,
                queue_size: 1,
                queue_maxsize: 64,
                queue_full: false,
                dropped_messages: 0,
                subscriptions: vec!["MU100".to_string(), "flight_updates".to_string()],
            }],
            heartbeat_interval: 15,
            max_connections: 1000,
            connection_queue_size: 64,
            cleanup_interval_seconds: 30,
            heartbeat_timeout_seconds: 45,
            queue_full_disconnect_seconds: 10,
            topic_count: 2,
            total_messages_sent: 2,
            lagged_total: 0,
        }
    }

    #[test]
    fn buffer_status_summary_matches_python_shape() {
        let payload = build_buffer_status_payload(&sample_stats(), None, false);
        assert_eq!(payload["status"], "active");
        assert_eq!(payload["total_connections"], 1);
        assert_eq!(payload["total_queue_size"], 1);
        assert_eq!(payload["total_queue_capacity"], 64);
        assert_eq!(payload["buffer_utilization_percent"], 1.56);
    }

    #[test]
    fn buffer_status_flight_view_filters_by_subscription() {
        let payload = build_buffer_status_payload(&sample_stats(), Some("MU100"), true);
        assert_eq!(payload["flight_no"], "MU100");
        assert_eq!(payload["status"], "active");
        assert_eq!(payload["subscribed_clients"], 1);
        assert_eq!(payload["total_queue_size"], 1);
    }

    #[test]
    fn buffer_status_missing_flight_matches_python_contract() {
        let payload = build_buffer_status_payload(&sample_stats(), Some("CZ200"), true);
        assert_eq!(payload["flight_no"], "CZ200");
        assert_eq!(payload["status"], "not_in_buffer");
        assert_eq!(payload["subscribed_clients"], 0);
        assert_eq!(payload["all_connections_count"], 1);
    }

    #[test]
    fn ai_copilot_commit_recovery_scheduler_defaults_match_contract() {
        assert_eq!(AI_COPILOT_COMMIT_RECOVERY_INTERVAL_SECONDS_DEFAULT, 30);
        assert_eq!(AI_COPILOT_COMMIT_RECOVERY_BATCH_SIZE_DEFAULT, 50);
        assert_eq!(AI_COPILOT_COMMIT_RECOVERY_STALE_AFTER_SECONDS_DEFAULT, 120);
        assert_eq!(AI_COPILOT_COMMIT_RECOVERY_MAX_ATTEMPTS_DEFAULT, 5);
    }

    #[test]
    fn scheduler_recent_errors_match_python_shape_and_limit() {
        let snapshot = SchedulerStatusSnapshot {
            running: true,
            started_at: "2026-04-17T00:00:00Z".to_string(),
            last_run: None,
            next_run: None,
            task_count: 2,
            tasks: vec![
                SchedulerTaskSnapshot {
                    name: "task-a".to_string(),
                    contract_name: Some("scheduler".to_string()),
                    interval_seconds: 5,
                    is_async: true,
                    next_run: None,
                    last_run: None,
                    last_success: None,
                    last_error: Some("2026-04-17T10:00:00Z".to_string()),
                    last_error_message: Some("boom".to_string()),
                    last_result: None,
                    run_count: 0,
                    fail_count: 1,
                    last_duration_ms: 0.0,
                    status: TaskStatus::Error,
                },
                SchedulerTaskSnapshot {
                    name: "task-b".to_string(),
                    contract_name: None,
                    interval_seconds: 5,
                    is_async: true,
                    next_run: None,
                    last_run: None,
                    last_success: None,
                    last_error: Some("2026-04-17T09:00:00Z".to_string()),
                    last_error_message: Some("warn".to_string()),
                    last_result: None,
                    run_count: 0,
                    fail_count: 1,
                    last_duration_ms: 0.0,
                    status: TaskStatus::Active,
                },
            ],
            last_error: None,
        };

        let errors = scheduler_recent_errors(&snapshot, Some(1));
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0]["message"], "boom");
        assert_eq!(errors[0]["severity"], "error");
    }

    #[test]
    fn sse_stats_payload_matches_python_shape_without_rust_only_fields() {
        let payload = build_sse_stats_payload(&sample_stats());

        assert_eq!(payload["active_connections"], 1);
        assert_eq!(payload["connection_details"][0]["client_id"], "client-buffer");
        assert_eq!(payload["connection_details"][0]["queue_size"], 1);
        assert_eq!(payload["connection_details"][0]["queue_full"], false);
        assert!(payload["connection_details"][0].get("user_id").is_none());
        assert!(payload["connection_details"][0].get("connected_at").is_none());
        assert!(payload.get("topic_count").is_none());
        assert!(payload.get("total_messages_sent").is_none());
        assert!(payload.get("lagged_total").is_none());
    }

    #[test]
    fn python_fixture_buffer_status_summary_matches_rust_payload() {
        let fixtures = load_python_runtime_parity_fixtures();
        let expected = fixtures["buffer_status_summary_single_connection"].clone();

        let mut actual = build_buffer_status_payload(&sample_stats(), None, false);
        normalize_runtime_parity_value(&mut actual);

        assert_eq!(actual, expected);
    }

    #[test]
    fn python_fixture_buffer_status_detailed_matches_rust_payload() {
        let fixtures = load_python_runtime_parity_fixtures();
        let expected = fixtures["buffer_status_detailed_single_connection"].clone();

        let mut actual = build_buffer_status_payload(&sample_stats(), None, true);
        normalize_runtime_parity_value(&mut actual);

        assert_eq!(actual, expected);
    }

    #[test]
    fn python_fixture_buffer_status_flight_hit_matches_rust_payload() {
        let fixtures = load_python_runtime_parity_fixtures();
        let expected = fixtures["buffer_status_flight_hit_single_connection"].clone();

        let mut actual = build_buffer_status_payload(&sample_stats(), Some("MU100"), true);
        normalize_runtime_parity_value(&mut actual);

        assert_eq!(actual, expected);
    }

    #[test]
    fn python_fixture_buffer_status_flight_miss_matches_rust_payload() {
        let fixtures = load_python_runtime_parity_fixtures();
        let expected = fixtures["buffer_status_flight_miss_single_connection"].clone();

        let mut actual = build_buffer_status_payload(&sample_stats(), Some("CZ200"), true);
        normalize_runtime_parity_value(&mut actual);

        assert_eq!(actual, expected);
    }

    #[test]
    fn python_fixture_sse_stats_match_rust_payload() {
        let fixtures = load_python_runtime_parity_fixtures();
        let expected = fixtures["sse_stats_single_connection"].clone();

        let mut actual = build_sse_stats_payload(&sample_stats());
        normalize_runtime_parity_value(&mut actual);

        assert_eq!(actual, expected);
    }

    #[test]
    fn derive_health_status_degrades_on_errors_and_inactive_sse() {
        let status = derive_health_status(
            Some("healthy"),
            &[json!({"message": "boom"})],
            &json!({"status": "inactive"}),
            &json!({
                "redis": {"status": "healthy"},
            }),
        );
        assert_eq!(status, "degraded");
    }

    #[test]
    fn python_fixture_health_status_cases_match_rust_logic() {
        let fixtures = load_python_runtime_parity_fixtures();
        let cases = fixtures["health_status_cases"]
            .as_object()
            .expect("health status cases");

        let healthy = derive_health_status(
            Some("healthy"),
            &[],
            &json!({"status": "active"}),
            &json!({"redis": {"status": "healthy"}}),
        );
        assert_eq!(healthy, cases["healthy"].as_str().unwrap_or_default());

        let degraded_on_errors = derive_health_status(
            Some("healthy"),
            &[json!({"message": "boom"})],
            &json!({"status": "active"}),
            &json!({"redis": {"status": "healthy"}}),
        );
        assert_eq!(
            degraded_on_errors,
            cases["degraded_on_errors"].as_str().unwrap_or_default()
        );

        let degraded_on_inactive_sse = derive_health_status(
            Some("healthy"),
            &[],
            &json!({"status": "inactive"}),
            &json!({"redis": {"status": "healthy"}}),
        );
        assert_eq!(
            degraded_on_inactive_sse,
            cases["degraded_on_inactive_sse"].as_str().unwrap_or_default()
        );

        let down_on_postgres = derive_health_status(
            Some("healthy"),
            &[],
            &json!({"status": "active"}),
            &json!({"postgres": {"status": "down"}}),
        );
        assert_eq!(down_on_postgres, cases["down_on_postgres"].as_str().unwrap_or_default());
    }

    #[test]
    fn performance_metrics_payload_matches_python_shape() {
        let payload = build_performance_metrics_payload(
            DbPoolMetrics {
                active: 3,
                idle: 1,
                max: 10,
            },
            &sample_stats(),
            true,
            12.34,
            &RequestLatencySnapshot {
                p50: 10.0,
                p95: 20.0,
                p99: 30.0,
                avg: 12.0,
                count: 7,
            },
            &AuthMetricsSnapshot {
                login_success: 2,
                login_failure: 1,
                login_total: 3,
                login_success_rate_pct: 66.67,
                refresh_success: 1,
                refresh_failure: 1,
                refresh_total: 2,
                refresh_success_rate_pct: 50.0,
                session_lost: 0,
                logout_total: 1,
                heartbeat_total: 4,
            },
            &NotificationDeliveryMetricsSnapshot {
                push_attempts: 0,
                push_success: 0,
                push_success_rate_pct: 0.0,
                sse_attempts: 3,
                sse_success: 2,
                sse_success_rate_pct: 66.67,
                external_attempts: 0,
                external_success: 0,
                in_app_attempts: 0,
                in_app_success: 0,
                backfill_pending: 1,
            },
            &MobileRealtimeMetricsSnapshot { sse_reconnects: 2 },
        );

        assert_eq!(payload["db_pool"]["usage_pct"], 30.0);
        assert_eq!(payload["redis"]["connected"], true);
        assert_eq!(payload["redis"]["latency_ms"], 12.34);
        assert_eq!(payload["sse"]["connections"], 1);
        assert_eq!(payload["requests"]["count"], 7);
        assert_eq!(payload["requests"]["p95"], 20.0);
        assert_eq!(payload["auth"]["login_total"], 3);
        assert_eq!(payload["auth"]["heartbeat_total"], 4);
        assert_eq!(payload["notification_delivery"]["backfill_pending"], 1);
        assert_eq!(payload["notification_delivery"]["sse_attempts"], 3);
        assert_eq!(payload["mobile_realtime"]["sse_reconnects"], 2);
    }

    #[test]
    fn python_fixture_performance_metrics_match_rust_payload() {
        let fixtures = load_python_runtime_parity_fixtures();
        let expected = fixtures["performance_metrics_sample"].clone();

        let mut actual = build_performance_metrics_payload(
            DbPoolMetrics {
                active: 3,
                idle: 1,
                max: 10,
            },
            &sample_stats(),
            true,
            12.34,
            &RequestLatencySnapshot {
                p50: 20.0,
                p95: 29.0,
                p99: 29.8,
                avg: 20.0,
                count: 3,
            },
            &AuthMetricsSnapshot {
                login_success: 1,
                login_failure: 1,
                login_total: 2,
                login_success_rate_pct: 50.0,
                refresh_success: 1,
                refresh_failure: 1,
                refresh_total: 2,
                refresh_success_rate_pct: 50.0,
                session_lost: 1,
                logout_total: 1,
                heartbeat_total: 4,
            },
            &NotificationDeliveryMetricsSnapshot {
                push_attempts: 0,
                push_success: 0,
                push_success_rate_pct: 0.0,
                sse_attempts: 3,
                sse_success: 2,
                sse_success_rate_pct: 66.67,
                external_attempts: 0,
                external_success: 0,
                in_app_attempts: 0,
                in_app_success: 0,
                backfill_pending: 1,
            },
            &MobileRealtimeMetricsSnapshot { sse_reconnects: 2 },
        );
        normalize_runtime_parity_value(&mut actual);

        assert_eq!(actual, expected);
    }

    #[test]
    fn runtime_duration_format_matches_python_style() {
        assert_eq!(format_duration(59), "59s");
        assert_eq!(format_duration(125), "2m 5s");
        assert_eq!(format_duration(3661), "1h 1m 1s");
    }

    /// End-to-end push-consumer callback test. A `MemoryPushConsumer` is
    /// configured as the push consumer and a `MessageHandler` is used to
    /// verify that injected messages trigger the registered listener
    /// synchronously without any polling.
    #[tokio::test]
    async fn scheduler_tick_releases_task_registry_lock_before_waiting_for_task_state() {
        let runner = Arc::new(|| Box::pin(async { Ok(json!({"ok": true})) }) as super::TaskFuture);
        let task = Arc::new(super::RegisteredTask::new("blocked_task", None, 60, runner));
        let tasks = Arc::new(tokio::sync::RwLock::new(vec![task.clone()]));
        let state_guard = task.state.lock().await;
        let start = Arc::new(tokio::sync::Barrier::new(2));

        let tick_tasks = tasks.clone();
        let tick_start = start.clone();
        let tick = tokio::spawn(async move {
            tick_start.wait().await;
            super::collect_due_tasks_for_tick(&tick_tasks, chrono::Utc::now()).await
        });

        start.wait().await;
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;

        let write_result = tokio::time::timeout(std::time::Duration::from_millis(100), tasks.write()).await;
        match write_result {
            Ok(write_guard) => drop(write_guard),
            Err(_) => panic!("scheduler tick held the task registry read lock while waiting for task state"),
        }

        drop(state_guard);
        match tick.await {
            Ok((_due_tasks, _min_sleep)) => {}
            Err(error) => panic!("scheduler tick task join failed: {error}"),
        }
    }

    #[tokio::test]
    async fn push_consumer_receives_message_via_callback() {
        use fms_infrastructure::messaging::{
            MemoryPushConsumer, MessageHandler, MessageQueueError, PushConsumer, SubscriberMessage,
        };
        use serde_json::json;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let counter: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));

        struct CountingHandler(Arc<AtomicUsize>);

        #[async_trait::async_trait]
        impl MessageHandler for CountingHandler {
            async fn handle(&self, _messages: Vec<SubscriberMessage>) -> Result<(), MessageQueueError> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }

        let consumer = MemoryPushConsumer::new();
        consumer
            .subscribe(
                "test-topic",
                "cg",
                None,
                Arc::new(CountingHandler(counter.clone())) as Arc<dyn MessageHandler>,
            )
            .await
            .unwrap();
        consumer.start().await.unwrap();

        consumer.inject(
            "test-topic",
            None,
            vec![SubscriberMessage {
                message_id: "1".to_string(),
                topic: "test-topic".to_string(),
                tag: None,
                key: None,
                body: json!({"hello": "world"}),
                properties: Default::default(),
            }],
        );

        // Allow the spawned handler task to execute.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
}
