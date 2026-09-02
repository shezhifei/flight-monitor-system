//! 派工预排冲突预警服务
//!
//! 非阻断预警:不改变工单状态,不释放/锁定人员,不阻止派工、发布、重排。
//! 实际完成回报仍是人员进入空闲状态的唯一事实依据;预计完成时间只作为
//! 排程信号,用于预测共享人员的冲突。
//!
//! 检测条件:
//! - 当前单: `in_progress` 且 `actual_end_time IS NULL`
//! - 下一单: `pending` 或 `assigned`,且有 `planned_start_time`
//! - 两单至少共享一名实际指派人员
//! - `now >= next.planned_start_time - effective_lead_minutes` 进入预警窗口
//!
//! 有效提前量优先级:工单级值(单次覆盖/规则快照) > 当前生成规则值(部门) >
//! 系统默认 5 分钟。有 ETA 时计算预计冲突分钟;无 ETA 时仅标记 `eta_missing`,
//! 不伪造持续时间。
//!
//! 生命周期:同一"当前单 → 下一单"按 `dedupe_key` 幂等写入,持续冲突只保留
//! 一条告警;冲突关闭后再次出现时复用键、递增 `occurrence_count`、清空确认并
//! 重新通知;冲突消失后自动关闭。

use chrono::{DateTime, Duration, Utc};
use fms_domain::broadcaster::Broadcaster;
use fms_runtime::spawn_tracked::spawn_tracked;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration as StdDuration, Instant};
use tracing::{info, warn};

use fms_domain::error::DomainError;
use fms_domain::models::dispatch::{
    dispatch_overrun_dedupe_key, resolve_completion_warning_lead_minutes, AlertSeverity, AssigneeType,
    CompletionWarningLeadSource, DepartmentRuleStatus, DispatchAlert, DispatchOrder, DispatchOrderStatus, LegScope,
};
use fms_domain::ports::dispatch_repository::{
    DispatchAlertRepository, DispatchOrderRepository, FlightGenerationRuleRepository,
};

/// 单次扫描的汇总统计。
#[derive(Debug, Clone, Default)]
pub struct ScanSummary {
    /// 已评估的相邻单对数。
    pub evaluated_pairs: usize,
    /// 需要发布通知的次数(新建或重新打开)。
    pub notifications: usize,
    /// 当前仍处于未关闭状态的告警数。
    pub active_alerts: usize,
    /// 自动关闭的告警数。
    pub auto_resolved: usize,
    /// 缺少预计完成时间的冲突数。
    pub eta_missing: usize,
}

/// 一次检测命中的预排冲突。
#[derive(Debug, Clone)]
pub struct OverrunWarningCandidate {
    pub current_order: DispatchOrder,
    pub next_order: DispatchOrder,
    /// 两张工单共享的实际指派人员。
    pub shared_personnel: Vec<String>,
    /// 距下一单计划开始时间的倒计时(分钟,不小于 0)。
    pub countdown_minutes: i64,
    pub lead_minutes: i32,
    pub lead_source: CompletionWarningLeadSource,
    /// 当前单未回报预计完成时间时为 true,此时不推算冲突分钟。
    pub eta_missing: bool,
    /// 预计冲突分钟 `max(0, estimated_completion_time - next.planned_start_time)`。
    pub predicted_conflict_minutes: Option<i64>,
}

/// 一次"当前单 → 下一单"评估的产出。
#[derive(Debug, Clone)]
pub struct OverrunEvaluationOutcome {
    pub candidate: Option<OverrunWarningCandidate>,
    /// 幂等写入后的告警(新建/更新/重新打开)。
    pub alert: Option<DispatchAlert>,
    /// 本次需要发布一次通知(新建或重新打开)。
    pub notify: bool,
    /// 冲突消失后自动关闭的告警 id。
    pub auto_resolved_alert_id: Option<String>,
}

/// 进程内原子指标。
#[derive(Debug, Default)]
struct OverrunMetrics {
    detections: AtomicU64,
    notifications: AtomicU64,
    auto_resolved: AtomicU64,
    eta_missing: AtomicU64,
    scan_duration_ms: AtomicU64,
    event_failures: AtomicU64,
}

/// 预排冲突预警应用服务。
pub struct DispatchOverrunWarningService {
    order_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
    alert_repo: Arc<dyn DispatchAlertRepository + Send + Sync>,
    generation_rule_repo: Option<Arc<dyn FlightGenerationRuleRepository + Send + Sync>>,
    broadcaster: Option<Arc<dyn Broadcaster + Send + Sync>>,
    /// `DISPATCH_OVERRUN_WARNING_ENABLED` — false 时 evaluate/scan 不落库。
    warning_enabled: bool,
    /// `DISPATCH_OVERRUN_SSE_ENABLED` — false 时不广播。
    sse_enabled: bool,
    scan_interval: StdDuration,
    scan_timeout: StdDuration,
    scanner_running: AtomicBool,
    metrics: OverrunMetrics,
    now_provider: Box<dyn Fn() -> DateTime<Utc> + Send + Sync>,
}

/// 扫描器前瞻窗口(分钟):覆盖下一单计划开始前 `now + look_ahead` 的全部工单。
const SCAN_LOOK_AHEAD_MINUTES: i64 = 15;
/// 扫描器回看窗口(分钟):覆盖在途单的实际开始时间回溯。
const SCAN_LOOK_BACK_MINUTES: i64 = 240;
/// 单次扫描上限,防止异常数据拖垮扫描任务。
const SCAN_MAX_ORDERS: usize = 5000;
const DEFAULT_SCAN_INTERVAL_SECS: u64 = 30;
const DEFAULT_SCAN_TIMEOUT_SECS: u64 = 25;
const SSE_TOPIC: &str = "dispatch_alerts";
const SSE_EVENT: &str = "dispatch_overrun_warning";

impl DispatchOverrunWarningService {
    pub fn new(
        order_repo: Arc<dyn DispatchOrderRepository + Send + Sync>,
        alert_repo: Arc<dyn DispatchAlertRepository + Send + Sync>,
    ) -> Self {
        Self {
            order_repo,
            alert_repo,
            generation_rule_repo: None,
            broadcaster: None,
            warning_enabled: true,
            sse_enabled: true,
            scan_interval: StdDuration::from_secs(DEFAULT_SCAN_INTERVAL_SECS),
            scan_timeout: StdDuration::from_secs(DEFAULT_SCAN_TIMEOUT_SECS),
            scanner_running: AtomicBool::new(false),
            metrics: OverrunMetrics::default(),
            now_provider: Box::new(Utc::now),
        }
    }

    pub fn with_generation_rule_repo(
        mut self,
        generation_rule_repo: Arc<dyn FlightGenerationRuleRepository + Send + Sync>,
    ) -> Self {
        self.generation_rule_repo = Some(generation_rule_repo);
        self
    }

    pub fn with_broadcaster(mut self, broadcaster: Arc<dyn Broadcaster + Send + Sync>) -> Self {
        self.broadcaster = Some(broadcaster);
        self
    }

    /// DI 在构造时注入环境开关,避免每次评估读 env。
    pub fn with_feature_flags(mut self, warning_enabled: bool, sse_enabled: bool) -> Self {
        self.warning_enabled = warning_enabled;
        self.sse_enabled = sse_enabled;
        self
    }

    pub fn with_scan_interval(mut self, interval: StdDuration) -> Self {
        if !interval.is_zero() {
            self.scan_interval = interval;
        }
        self
    }

    /// 测试用时钟注入。
    pub fn with_clock(mut self, now_provider: impl Fn() -> DateTime<Utc> + Send + Sync + 'static) -> Self {
        self.now_provider = Box::new(now_provider);
        self
    }

    pub fn is_warning_enabled(&self) -> bool {
        self.warning_enabled
    }

    pub fn is_sse_enabled(&self) -> bool {
        self.sse_enabled
    }

    /// 列出未关闭告警(功能开关关闭时仍可用,便于 API 查看历史未处理项)。
    pub async fn list_unresolved(&self, flight_id: Option<&str>) -> Result<Vec<DispatchAlert>, DomainError> {
        self.alert_repo.find_unresolved(flight_id).await
    }

    /// 调度员确认;确认不等于关闭。
    pub async fn acknowledge(&self, id: &str, by: &str) -> Result<DispatchAlert, DomainError> {
        let ok = self.alert_repo.acknowledge(id, by).await?;
        if !ok {
            return Err(DomainError::NotFound {
                entity_type: "DispatchAlert",
                id: id.to_string(),
            });
        }
        self.alert_repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "DispatchAlert",
                id: id.to_string(),
            })
    }

    /// 人工关闭告警。
    pub async fn resolve(&self, id: &str, by: &str, notes: Option<&str>) -> Result<DispatchAlert, DomainError> {
        let ok = self.alert_repo.resolve(id, by, notes).await?;
        if !ok {
            return Err(DomainError::NotFound {
                entity_type: "DispatchAlert",
                id: id.to_string(),
            });
        }
        self.alert_repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| DomainError::NotFound {
                entity_type: "DispatchAlert",
                id: id.to_string(),
            })
    }

    pub fn metrics_snapshot(&self) -> serde_json::Value {
        serde_json::json!({
            "detections": self.metrics.detections.load(Ordering::Relaxed),
            "notifications": self.metrics.notifications.load(Ordering::Relaxed),
            "auto_resolved": self.metrics.auto_resolved.load(Ordering::Relaxed),
            "eta_missing": self.metrics.eta_missing.load(Ordering::Relaxed),
            "scan_duration_ms": self.metrics.scan_duration_ms.load(Ordering::Relaxed),
            "event_failures": self.metrics.event_failures.load(Ordering::Relaxed),
            "warning_enabled": self.warning_enabled,
            "sse_enabled": self.sse_enabled,
            "scanner_running": self.scanner_running.load(Ordering::Acquire),
        })
    }

    pub fn record_event_failure(&self) {
        self.metrics.event_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// 评估整条航班的相邻工单链;事件触发与扫描器共用此入口。
    pub async fn evaluate_flight(&self, flight_id: &str) -> Result<Vec<OverrunEvaluationOutcome>, DomainError> {
        if !self.warning_enabled {
            return Ok(Vec::new());
        }
        let orders = self.order_repo.find_by_flight(flight_id).await?;
        self.evaluate_orders(orders).await
    }

    /// 评估一组工单(通常来自同一航班)的相邻对。
    pub async fn evaluate_orders(
        &self,
        mut orders: Vec<DispatchOrder>,
    ) -> Result<Vec<OverrunEvaluationOutcome>, DomainError> {
        if !self.warning_enabled {
            return Ok(Vec::new());
        }
        orders.sort_by_key(|order| order.planned_start_time);
        let mut outcomes = Vec::new();
        for window in orders.windows(2) {
            outcomes.push(self.evaluate_pair(window[0].clone(), window[1].clone()).await?);
        }
        self.finalize_outcomes(&outcomes).await;
        Ok(outcomes)
    }

    /// 评估单个工单受影响的链路(事件触发入口):加载其航班并整链评估。
    pub async fn evaluate_order(&self, order_id: &str) -> Result<Vec<OverrunEvaluationOutcome>, DomainError> {
        if !self.warning_enabled {
            return Ok(Vec::new());
        }
        let Some(order) = self.order_repo.find_by_id(order_id, false, None).await? else {
            return Ok(Vec::new());
        };
        self.evaluate_flight(&order.flight_id).await
    }

    /// 进程级恢复扫描:查找窗口内所有未完成工单,按航班分组后整链评估。
    /// 事件触发失败或遗漏时,由 30 秒扫描器兜底。
    pub async fn scan_once(&self) -> Result<ScanSummary, DomainError> {
        if !self.warning_enabled {
            return Ok(ScanSummary::default());
        }
        let started = Instant::now();
        let now = (self.now_provider)();
        let window_start = now - Duration::minutes(SCAN_LOOK_BACK_MINUTES);
        let window_end = now + Duration::minutes(SCAN_LOOK_AHEAD_MINUTES + 60);
        let orders = self
            .order_repo
            .find_orders_in_window(
                window_start,
                window_end,
                &["in_progress", "pending", "assigned", "cancelled"],
                None,
                None,
                None,
                true,
            )
            .await?;
        let mut by_flight: std::collections::BTreeMap<String, Vec<DispatchOrder>> = std::collections::BTreeMap::new();
        for order in orders.into_iter().take(SCAN_MAX_ORDERS) {
            by_flight.entry(order.flight_id.clone()).or_default().push(order);
        }
        let mut summary = ScanSummary::default();
        for (_flight_id, orders) in by_flight {
            let outcomes = self.evaluate_orders(orders).await?;
            for outcome in outcomes {
                summary.evaluated_pairs += 1;
                if outcome.notify {
                    summary.notifications += 1;
                }
                if let Some(alert) = &outcome.alert {
                    summary.active_alerts += usize::from(!alert.is_resolved);
                }
                if outcome.auto_resolved_alert_id.is_some() {
                    summary.auto_resolved += 1;
                }
                if outcome
                    .candidate
                    .as_ref()
                    .map(|candidate| candidate.eta_missing)
                    .unwrap_or(false)
                {
                    summary.eta_missing += 1;
                }
            }
        }
        let elapsed_ms = started.elapsed().as_millis() as u64;
        self.metrics.scan_duration_ms.store(elapsed_ms, Ordering::Relaxed);
        Ok(summary)
    }

    /// 启动进程级 30s 扫描器(单例;可安全重复调用)。
    pub fn start_scanner(self: Arc<Self>) {
        if !self.warning_enabled {
            info!("dispatch overrun warning disabled; scanner not started");
            return;
        }
        if self
            .scanner_running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let me = Arc::clone(&self);
        spawn_tracked("dispatch_overrun_warning_scanner", async move {
            me.run_scanner_loop().await;
        });
    }

    pub fn stop_scanner(&self) {
        self.scanner_running.store(false, Ordering::Release);
    }

    async fn run_scanner_loop(self: Arc<Self>) {
        let mut ticker = tokio::time::interval(self.scan_interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        info!(
            interval_secs = self.scan_interval.as_secs(),
            "dispatch overrun warning scanner started"
        );
        loop {
            ticker.tick().await;
            if !self.scanner_running.load(Ordering::Acquire) {
                break;
            }
            if !self.warning_enabled {
                continue;
            }
            match tokio::time::timeout(self.scan_timeout, self.scan_once()).await {
                Ok(Ok(summary)) => {
                    info!(
                        evaluated_pairs = summary.evaluated_pairs,
                        notifications = summary.notifications,
                        active_alerts = summary.active_alerts,
                        auto_resolved = summary.auto_resolved,
                        eta_missing = summary.eta_missing,
                        scan_duration_ms = self.metrics.scan_duration_ms.load(Ordering::Relaxed),
                        "dispatch overrun warning scan complete"
                    );
                }
                Ok(Err(error)) => {
                    warn!(error = %error, "dispatch overrun warning scan failed");
                }
                Err(_) => {
                    warn!(
                        timeout_secs = self.scan_timeout.as_secs(),
                        "dispatch overrun warning scan timed out"
                    );
                }
            }
        }
        info!("dispatch overrun warning scanner stopped");
    }

    async fn finalize_outcomes(&self, outcomes: &[OverrunEvaluationOutcome]) {
        for outcome in outcomes {
            if outcome.candidate.is_some() {
                self.metrics.detections.fetch_add(1, Ordering::Relaxed);
            }
            if outcome.notify {
                self.metrics.notifications.fetch_add(1, Ordering::Relaxed);
            }
            if outcome.auto_resolved_alert_id.is_some() {
                self.metrics.auto_resolved.fetch_add(1, Ordering::Relaxed);
            }
            if outcome
                .candidate
                .as_ref()
                .map(|candidate| candidate.eta_missing)
                .unwrap_or(false)
            {
                self.metrics.eta_missing.fetch_add(1, Ordering::Relaxed);
            }

            let should_broadcast = outcome.notify || outcome.auto_resolved_alert_id.is_some();
            if should_broadcast {
                if let Some(alert) = outcome.alert.as_ref() {
                    self.broadcast_alert(alert).await;
                }
            }
        }
    }

    async fn broadcast_alert(&self, alert: &DispatchAlert) {
        if !self.sse_enabled {
            return;
        }
        let Some(broadcaster) = self.broadcaster.as_ref() else {
            return;
        };
        broadcaster
            .broadcast_event(SSE_TOPIC, Some(SSE_EVENT), overrun_alert_to_json(alert))
            .await;
    }

    async fn evaluate_pair(
        &self,
        current: DispatchOrder,
        next: DispatchOrder,
    ) -> Result<OverrunEvaluationOutcome, DomainError> {
        let (lead_minutes, lead_source) = self.resolve_effective_lead(&next).await?;
        let dedupe_key = dispatch_overrun_dedupe_key(&current.id, &next.id);
        let now = (self.now_provider)();
        let candidate = detect_overrun(&current, &next, lead_minutes, lead_source, now);
        let existing = self.find_active_alert(&next.flight_id, &dedupe_key).await?;
        match candidate {
            Some(candidate) => {
                let alert = build_overrun_alert(&candidate, &dedupe_key);
                let upserted = self.alert_repo.upsert_overrun(&alert).await?;
                Ok(OverrunEvaluationOutcome {
                    candidate: Some(candidate),
                    alert: Some(upserted.alert),
                    notify: upserted.inserted || upserted.reopened,
                    auto_resolved_alert_id: None,
                })
            }
            None => {
                if let Some(mut alert) = existing {
                    let alert_id = alert.id.clone();
                    self.alert_repo.auto_resolve(&alert_id).await?;
                    alert.is_resolved = true;
                    alert.resolved_at = Some(Utc::now());
                    alert.resolved_by = None;
                    alert.resolution_notes = Some("auto".to_string());
                    Ok(OverrunEvaluationOutcome {
                        candidate: None,
                        alert: Some(alert),
                        notify: false,
                        auto_resolved_alert_id: Some(alert_id),
                    })
                } else {
                    Ok(OverrunEvaluationOutcome {
                        candidate: None,
                        alert: None,
                        notify: false,
                        auto_resolved_alert_id: None,
                    })
                }
            }
        }
    }

    async fn find_active_alert(&self, flight_id: &str, dedupe_key: &str) -> Result<Option<DispatchAlert>, DomainError> {
        let alerts = self.alert_repo.find_unresolved(Some(flight_id)).await?;
        Ok(alerts
            .into_iter()
            .find(|alert| alert.dedupe_key.as_deref() == Some(dedupe_key)))
    }

    /// 解析下一单的生效提前量:工单级 > 当前生成规则(部门) > 系统默认 5。
    async fn resolve_effective_lead(
        &self,
        order: &DispatchOrder,
    ) -> Result<(i32, CompletionWarningLeadSource), DomainError> {
        let department_value = match (order.department_id.as_deref(), self.generation_rule_repo.as_ref()) {
            (Some(department_id), Some(rule_repo)) => {
                let rules = rule_repo.list_rules(department_id, None).await?;
                rules
                    .iter()
                    .filter(|rule| rule.status == DepartmentRuleStatus::Published)
                    .filter(|rule| {
                        rule.task_type == order.task_type && leg_scope_value(rule.leg_scope) == order.leg_scope
                    })
                    .find_map(|rule| rule.completion_warning_lead_minutes)
            }
            _ => None,
        };
        resolve_completion_warning_lead_minutes(order.completion_warning_lead_minutes, department_value)
    }
}

/// 将预排冲突告警序列化为 API/SSE JSON。
pub fn overrun_alert_to_json(alert: &DispatchAlert) -> serde_json::Value {
    let severity = match alert.severity {
        AlertSeverity::Info => "info",
        AlertSeverity::Warning => "warning",
        AlertSeverity::Critical => "critical",
    };
    serde_json::json!({
        "id": alert.id,
        "flight_id": alert.flight_id,
        "task_type": alert.task_type,
        "alert_type": alert.alert_type,
        "severity": severity,
        "message": alert.message,
        "is_resolved": alert.is_resolved,
        "resolved_at": alert.resolved_at,
        "resolved_by": alert.resolved_by,
        "resolution_notes": alert.resolution_notes,
        "notify_users": alert.notify_users,
        "created_at": alert.created_at,
        "dedupe_key": alert.dedupe_key,
        "current_order_id": alert.current_order_id,
        "next_order_id": alert.next_order_id,
        "last_detected_at": alert.last_detected_at,
        "occurrence_count": alert.occurrence_count,
        "acknowledged_at": alert.acknowledged_at,
        "acknowledged_by": alert.acknowledged_by,
        "details": if alert.details.is_null() {
            serde_json::json!({})
        } else {
            alert.details.clone()
        },
    })
}

fn leg_scope_value(scope: LegScope) -> &'static str {
    match scope {
        LegScope::Inbound => "inbound",
        LegScope::Outbound => "outbound",
        LegScope::None => "none",
    }
}

/// 提取工单实际指派人员:个人单取 `individual_user_id`,班组单取活跃成员。
pub fn actual_person_ids(order: &DispatchOrder) -> HashSet<String> {
    let mut ids: HashSet<String> = order.individual_user_id.iter().cloned().collect();
    ids.extend(
        order
            .members
            .iter()
            .filter(|member| member.is_active)
            .map(|member| member.user_id.clone()),
    );
    ids
}

/// 纯检测:满足全部条件时返回冲突详情,否则返回 `None`。
pub fn detect_overrun(
    current: &DispatchOrder,
    next: &DispatchOrder,
    lead_minutes: i32,
    lead_source: CompletionWarningLeadSource,
    now: DateTime<Utc>,
) -> Option<OverrunWarningCandidate> {
    if current.status != DispatchOrderStatus::InProgress || current.actual_end_time.is_some() {
        return None;
    }
    if !matches!(
        next.status,
        DispatchOrderStatus::Pending | DispatchOrderStatus::Assigned
    ) {
        return None;
    }
    let next_start = next.planned_start_time?;
    let mut shared_personnel: Vec<String> = actual_person_ids(current)
        .intersection(&actual_person_ids(next))
        .cloned()
        .collect();
    if shared_personnel.is_empty() {
        return None;
    }
    shared_personnel.sort();
    let window_start = next_start - Duration::minutes(i64::from(lead_minutes));
    if now < window_start {
        return None;
    }
    let countdown_minutes = (next_start - now).num_minutes().max(0);
    let (eta_missing, predicted_conflict_minutes) = match current.estimated_completion_time {
        Some(eta) => (false, Some((eta - next_start).num_minutes().max(0))),
        None => (true, None),
    };
    Some(OverrunWarningCandidate {
        current_order: current.clone(),
        next_order: next.clone(),
        shared_personnel,
        countdown_minutes,
        lead_minutes,
        lead_source,
        eta_missing,
        predicted_conflict_minutes,
    })
}

/// 由冲突候选构造待幂等写入的告警。
pub fn build_overrun_alert(candidate: &OverrunWarningCandidate, dedupe_key: &str) -> DispatchAlert {
    let message = match (candidate.eta_missing, candidate.predicted_conflict_minutes) {
        (false, Some(minutes)) => format!(
            "工单 {} 预计完成时间晚于下一单 {} 的计划开始时间,预计冲突 {minutes} 分钟",
            candidate.current_order.id, candidate.next_order.id
        ),
        _ => format!(
            "工单 {} 仍在执行且未回报预计完成时间,下一单 {} 可能无法按时开始",
            candidate.current_order.id, candidate.next_order.id
        ),
    };
    let details = serde_json::json!({
        "current_order_id": candidate.current_order.id,
        "next_order_id": candidate.next_order.id,
        "shared_personnel": candidate.shared_personnel,
        "countdown_minutes": candidate.countdown_minutes,
        "lead_minutes": candidate.lead_minutes,
        "lead_source": match candidate.lead_source {
            CompletionWarningLeadSource::Order => "order",
            CompletionWarningLeadSource::Department => "department",
            CompletionWarningLeadSource::System => "system",
        },
        "eta_missing": candidate.eta_missing,
        "predicted_conflict_minutes": candidate.predicted_conflict_minutes,
    });
    DispatchAlert {
        id: ulid::Ulid::new().to_string(),
        flight_id: Some(candidate.next_order.flight_id.clone()),
        task_type: Some(candidate.next_order.task_type.clone()),
        alert_type: "dispatch_schedule_overrun".to_string(),
        severity: AlertSeverity::Warning,
        message,
        is_resolved: false,
        resolved_at: None,
        resolved_by: None,
        resolution_notes: None,
        notify_users: candidate.shared_personnel.clone(),
        created_at: Some(Utc::now()),
        dedupe_key: Some(dedupe_key.to_string()),
        current_order_id: Some(candidate.current_order.id.clone()),
        next_order_id: Some(candidate.next_order.id.clone()),
        last_detected_at: Some(Utc::now()),
        occurrence_count: 1,
        acknowledged_at: None,
        acknowledged_by: None,
        details,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fms_domain::models::dispatch::{DispatchOrderMember, MemberRole};

    fn fixed_now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-08-08T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn order(id: &str, flight: &str, status: DispatchOrderStatus) -> DispatchOrder {
        DispatchOrder {
            id: id.to_string(),
            flight_id: flight.to_string(),
            task_type: "boarding".to_string(),
            stand_id: None,
            task_type_name: None,
            stand_code: None,
            terminal: None,
            department: Some("dept-1".to_string()),
            individual_user_id: None,
            individual_username: None,
            driver_type: None,
            driver_user_id: None,
            planned_start_time: None,
            planned_end_time: None,
            actual_start_time: None,
            actual_end_time: None,
            estimated_completion_time: None,
            estimated_completion_reported_by: None,
            estimated_completion_reported_at: None,
            estimated_completion_note: None,
            status,
            dispatch_type: fms_domain::models::dispatch::DispatchType::Auto,
            dispatched_at: None,
            dispatched_by: None,
            snapshot_assignee_position: None,
            snapshot_equipment_positions: None,
            estimated_arrival_minutes: None,
            process_instance_id: None,
            process_task_id: None,
            workflow_context: Default::default(),
            workflow_status: "pending_assignment".to_string(),
            source: "system".to_string(),
            schedule_source: fms_domain::models::dispatch::ScheduleSource::CurrentStatusFallback,
            lock_level: fms_domain::models::dispatch::DispatchLockLevel::Optimizable,
            publication_state: "prepublished".to_string(),
            source_type: "manual".to_string(),
            department_id: Some("dept-1".to_string()),
            leg_scope: "outbound".to_string(),
            generation_rule_id: None,
            generation_rule_version: None,
            generation_anchor_type: None,
            generation_anchor_time: None,
            completion_time_mode: None,
            completion_anchor_type: None,
            completion_anchor_time: None,
            completion_offset_minutes: None,
            completion_warning_lead_minutes: None,
            publish_trigger_mode: None,
            publish_at: None,
            turnaround_pair_key: None,
            turnaround_constraint_mode: None,
            department_rule_version: None,
            crew_requirement_snapshot: Vec::new(),
            equipment_requirement_snapshot: Vec::new(),
            task_crew: Default::default(),
            equipment_assignment: Vec::new(),
            qualification_gap: Vec::new(),
            equipment_gap: Vec::new(),
            availability_reason: None,
            score_breakdown: Default::default(),
            conflict_reason: None,
            recommended_assignees: Vec::new(),
            recommendation_score: None,
            supervisor_notified: false,
            supervisor_notified_at: None,
            assignment_deadline: None,
            attributes: serde_json::json!({}),
            completed_by: None,
            completion_notes: None,
            gate: None,
            created_at: None,
            updated_at: None,
            members: Vec::new(),
            equipment_list: Vec::new(),
        }
    }

    fn with_member(mut order: DispatchOrder, user_id: &str, active: bool) -> DispatchOrder {
        order.members.push(DispatchOrderMember {
            id: format!("m-{user_id}"),
            dispatch_order_id: order.id.clone(),
            user_id: user_id.to_string(),
            role: MemberRole::Member,
            source_type: AssigneeType::Team,
            source_team_id: None,
            slot_code: None,
            qualification_code: None,
            qualification_level_code: None,
            assigned_at: None,
            check_in_time: None,
            check_out_time: None,
            is_active: active,
            username: None,
        });
        order
    }

    fn in_progress_current(start: DateTime<Utc>, eta: Option<DateTime<Utc>>) -> DispatchOrder {
        let mut current = order("do-current", "fl-1", DispatchOrderStatus::InProgress);
        current.planned_start_time = Some(start);
        current.actual_start_time = Some(start);
        current.actual_end_time = None;
        current.estimated_completion_time = eta;
        current.individual_user_id = Some("user-1".to_string());
        current
    }

    fn pending_next(start: DateTime<Utc>) -> DispatchOrder {
        let mut next = order("do-next", "fl-1", DispatchOrderStatus::Pending);
        next.planned_start_time = Some(start);
        next.individual_user_id = Some("user-1".to_string());
        next
    }

    #[test]
    fn detects_conflict_inside_lead_window_with_eta() {
        let now = fixed_now();
        let current = in_progress_current(now - Duration::minutes(10), Some(now + Duration::minutes(20)));
        let next = pending_next(now + Duration::minutes(5));
        let candidate = detect_overrun(&current, &next, 5, CompletionWarningLeadSource::System, now)
            .expect("inside the 5-minute lead window must warn");
        assert_eq!(candidate.shared_personnel, vec!["user-1"]);
        assert_eq!(candidate.countdown_minutes, 5);
        assert!(!candidate.eta_missing);
        assert_eq!(candidate.predicted_conflict_minutes, Some(15));
        assert_eq!(candidate.lead_source, CompletionWarningLeadSource::System);
    }

    #[test]
    fn does_not_warn_outside_lead_window() {
        let now = fixed_now();
        let current = in_progress_current(now - Duration::minutes(10), Some(now + Duration::minutes(20)));
        let next = pending_next(now + Duration::minutes(10));
        assert!(detect_overrun(&current, &next, 5, CompletionWarningLeadSource::System, now).is_none());
    }

    #[test]
    fn lead_zero_triggers_only_at_planned_start() {
        let now = fixed_now();
        let current = in_progress_current(now - Duration::minutes(10), Some(now + Duration::minutes(20)));
        let next = pending_next(now + Duration::minutes(4));
        assert!(detect_overrun(&current, &next, 0, CompletionWarningLeadSource::Order, now).is_none());
        let next = pending_next(now);
        assert!(detect_overrun(&current, &next, 0, CompletionWarningLeadSource::Order, now).is_some());
    }

    #[test]
    fn no_warning_without_shared_actual_person() {
        let now = fixed_now();
        let mut current = in_progress_current(now - Duration::minutes(10), Some(now + Duration::minutes(20)));
        current.individual_user_id = Some("user-a".to_string());
        let next = pending_next(now + Duration::minutes(5));
        assert!(detect_overrun(&current, &next, 5, CompletionWarningLeadSource::System, now).is_none());
    }

    #[test]
    fn no_warning_when_current_is_completed_or_cancelled() {
        let now = fixed_now();
        let next = pending_next(now + Duration::minutes(5));
        let mut completed = in_progress_current(now - Duration::minutes(10), Some(now + Duration::minutes(20)));
        completed.status = DispatchOrderStatus::Completed;
        assert!(detect_overrun(&completed, &next, 5, CompletionWarningLeadSource::System, now).is_none());

        let mut cancelled = in_progress_current(now - Duration::minutes(10), Some(now + Duration::minutes(20)));
        cancelled.status = DispatchOrderStatus::Cancelled;
        assert!(detect_overrun(&cancelled, &next, 5, CompletionWarningLeadSource::System, now).is_none());
    }

    #[test]
    fn no_warning_when_next_is_completed_or_cancelled() {
        let now = fixed_now();
        let current = in_progress_current(now - Duration::minutes(10), Some(now + Duration::minutes(20)));
        let mut next = pending_next(now + Duration::minutes(5));
        next.status = DispatchOrderStatus::Completed;
        assert!(detect_overrun(&current, &next, 5, CompletionWarningLeadSource::System, now).is_none());
        next.status = DispatchOrderStatus::Cancelled;
        assert!(detect_overrun(&current, &next, 5, CompletionWarningLeadSource::System, now).is_none());
    }

    #[test]
    fn no_warning_when_next_has_no_planned_start() {
        let now = fixed_now();
        let current = in_progress_current(now - Duration::minutes(10), Some(now + Duration::minutes(20)));
        let mut next = pending_next(now + Duration::minutes(5));
        next.planned_start_time = None;
        assert!(detect_overrun(&current, &next, 5, CompletionWarningLeadSource::System, now).is_none());
    }

    #[test]
    fn eta_missing_flags_without_inventing_duration() {
        let now = fixed_now();
        let current = in_progress_current(now - Duration::minutes(10), None);
        let next = pending_next(now + Duration::minutes(5));
        let candidate = detect_overrun(&current, &next, 5, CompletionWarningLeadSource::Department, now)
            .expect("missing ETA must still warn inside the window");
        assert!(candidate.eta_missing);
        assert_eq!(candidate.predicted_conflict_minutes, None);
    }

    #[test]
    fn team_orders_share_active_members_only() {
        let now = fixed_now();
        let mut current = in_progress_current(now - Duration::minutes(10), Some(now + Duration::minutes(20)));
        current.individual_user_id = None;
        current = with_member(current, "shared", true);
        current = with_member(current, "inactive", false);

        let mut next = pending_next(now + Duration::minutes(5));
        next.individual_user_id = None;
        next = with_member(next, "shared", true);
        next = with_member(next, "different", true);

        let candidate = detect_overrun(&current, &next, 5, CompletionWarningLeadSource::System, now)
            .expect("active shared member must warn");
        assert_eq!(candidate.shared_personnel, vec!["shared"]);
    }

    #[test]
    fn overlapping_eta_after_next_start_yields_zero_conflict_minutes() {
        let now = fixed_now();
        let current = in_progress_current(now - Duration::minutes(10), Some(now + Duration::minutes(3)));
        let next = pending_next(now + Duration::minutes(5));
        let candidate = detect_overrun(&current, &next, 5, CompletionWarningLeadSource::System, now).unwrap();
        assert_eq!(candidate.predicted_conflict_minutes, Some(0));
    }

    // -----------------------------------------------------------------------
    // 生命周期:幂等写入与自动关闭
    // -----------------------------------------------------------------------

    use fms_domain::ports::dispatch_repository::{
        CreateDispatchOrderCommand, DispatchAlertRepository, DispatchOrderRepository, OverrunAlertUpsert,
    };

    #[derive(Default)]
    struct StubOrderRepo {
        orders_by_flight: std::sync::Mutex<std::collections::HashMap<String, Vec<DispatchOrder>>>,
    }

    impl StubOrderRepo {
        fn replace(&self, flight_id: &str, orders: Vec<DispatchOrder>) {
            self.orders_by_flight
                .lock()
                .unwrap()
                .insert(flight_id.to_string(), orders);
        }
    }

    #[async_trait::async_trait]
    impl DispatchOrderRepository for StubOrderRepo {
        async fn find_by_flight(&self, flight_id: &str) -> Result<Vec<DispatchOrder>, DomainError> {
            Ok(self
                .orders_by_flight
                .lock()
                .unwrap()
                .get(flight_id)
                .cloned()
                .unwrap_or_default())
        }
        async fn find_by_id(
            &self,
            id: &str,
            _load_members: bool,
            _department: Option<&str>,
        ) -> Result<Option<DispatchOrder>, DomainError> {
            Ok(self
                .orders_by_flight
                .lock()
                .unwrap()
                .values()
                .flatten()
                .find(|order| order.id == id)
                .cloned())
        }
        async fn save(&self, _order: &DispatchOrder) -> Result<(), DomainError> {
            unimplemented!("save")
        }
        async fn create_order_atomic(&self, _command: CreateDispatchOrderCommand) -> Result<(), DomainError> {
            unimplemented!("create_order_atomic")
        }
        async fn save_orders_atomic(&self, _commands: Vec<CreateDispatchOrderCommand>) -> Result<(), DomainError> {
            unimplemented!("save_orders_atomic")
        }
        async fn find_by_flight_with_filters(
            &self,
            _flight_id: &str,
            _status: Option<&str>,
            _source: Option<&str>,
            _department: Option<&str>,
            _limit: i64,
            _offset: i64,
        ) -> Result<Vec<DispatchOrder>, DomainError> {
            unimplemented!("find_by_flight_with_filters")
        }
        async fn find_by_user(&self, _user_id: &str, _status: Option<&str>) -> Result<Vec<DispatchOrder>, DomainError> {
            unimplemented!("find_by_user")
        }
        async fn find_all(
            &self,
            _status: Option<&str>,
            _department: Option<&str>,
            _limit: i64,
            _offset: i64,
        ) -> Result<Vec<DispatchOrder>, DomainError> {
            unimplemented!("find_all")
        }
        async fn find_all_filtered(
            &self,
            _status: Option<&str>,
            _source: Option<&str>,
            _department: Option<&str>,
            _limit: i64,
            _offset: i64,
        ) -> Result<Vec<DispatchOrder>, DomainError> {
            unimplemented!("find_all_filtered")
        }
        async fn find_orders_in_window(
            &self,
            window_start: DateTime<Utc>,
            window_end: DateTime<Utc>,
            statuses: &[&str],
            _source: Option<&str>,
            _department: Option<&str>,
            _terminal: Option<&str>,
            include_cancelled: bool,
        ) -> Result<Vec<DispatchOrder>, DomainError> {
            let statuses = statuses.iter().map(|s| s.trim().to_string()).collect::<Vec<_>>();
            Ok(self
                .orders_by_flight
                .lock()
                .unwrap()
                .values()
                .flatten()
                .filter(|order| {
                    let Some(start) = order.planned_start_time else {
                        return false;
                    };
                    let end = order.planned_end_time.unwrap_or(start);
                    end >= window_start && start <= window_end
                })
                .filter(|order| {
                    let in_status =
                        statuses.is_empty() || statuses.iter().any(|status| status == order.status.as_ref());
                    in_status || (include_cancelled && order.status == DispatchOrderStatus::Cancelled)
                })
                .cloned()
                .collect())
        }
        async fn find_overlapping_orders(
            &self,
            _window_start: DateTime<Utc>,
            _window_end: DateTime<Utc>,
            _individual_user_id: Option<&str>,
            _stand_id: Option<&str>,
            _exclude_order_id: Option<&str>,
        ) -> Result<Vec<DispatchOrder>, DomainError> {
            unimplemented!("find_overlapping_orders")
        }
        async fn find_equipment_conflicts(
            &self,
            _equipment_ids: &[String],
            _window_start: DateTime<Utc>,
            _window_end: DateTime<Utc>,
            _exclude_order_id: Option<&str>,
        ) -> Result<Vec<serde_json::Value>, DomainError> {
            unimplemented!("find_equipment_conflicts")
        }
        async fn list_logs(
            &self,
            _dispatch_order_id: &str,
            _limit: i64,
        ) -> Result<Vec<serde_json::Value>, DomainError> {
            unimplemented!("list_logs")
        }
        async fn find_pending_for_flight(&self, _flight_id: &str) -> Result<Vec<DispatchOrder>, DomainError> {
            unimplemented!("find_pending_for_flight")
        }
        async fn find_publishable_orders(
            &self,
            _as_of: DateTime<Utc>,
            _limit: i64,
        ) -> Result<Vec<DispatchOrder>, DomainError> {
            unimplemented!("find_publishable_orders")
        }
        async fn update_status(
            &self,
            _id: &str,
            _status: &str,
            _actor_id: Option<&str>,
            _enforce_actor_assignment: bool,
        ) -> Result<bool, DomainError> {
            unimplemented!("update_status")
        }
        async fn start_order(
            &self,
            _id: &str,
            _actual_start: DateTime<Utc>,
            _actor_id: &str,
        ) -> Result<bool, DomainError> {
            unimplemented!("start_order")
        }
        async fn complete_order(
            &self,
            _id: &str,
            _actual_end: DateTime<Utc>,
            _actor_id: &str,
            _notes: Option<&str>,
        ) -> Result<bool, DomainError> {
            unimplemented!("complete_order")
        }
        async fn append_log(
            &self,
            _dispatch_order_id: &str,
            _action: &str,
            _actor_id: Option<&str>,
            _details: Option<serde_json::Value>,
        ) -> Result<(), DomainError> {
            unimplemented!("append_log")
        }
        async fn append_log_once(
            &self,
            _dispatch_order_id: &str,
            _action: &str,
            _actor_id: Option<&str>,
            _details: serde_json::Value,
        ) -> Result<bool, DomainError> {
            unimplemented!("append_log_once")
        }
        async fn has_logged_action(
            &self,
            _dispatch_order_id: &str,
            _action: &str,
            _actor_id: Option<&str>,
            _client_action_id: Option<&str>,
        ) -> Result<bool, DomainError> {
            unimplemented!("has_logged_action")
        }
        async fn report_estimated_completion(
            &self,
            _id: &str,
            _estimated_time: DateTime<Utc>,
            _actor_id: &str,
            _note: Option<&str>,
        ) -> Result<bool, DomainError> {
            unimplemented!("report_estimated_completion")
        }
        async fn update_planned_times(
            &self,
            _id: &str,
            _planned_start: DateTime<Utc>,
            _planned_end: DateTime<Utc>,
        ) -> Result<bool, DomainError> {
            unimplemented!("update_planned_times")
        }
        async fn replace_order_equipment_assignments(
            &self,
            _id: &str,
            _equipment_ids: &[String],
        ) -> Result<(), DomainError> {
            unimplemented!("replace_order_equipment_assignments")
        }
    }

    #[derive(Default)]
    struct StubAlertRepo {
        alerts: std::sync::Mutex<Vec<DispatchAlert>>,
    }

    #[async_trait::async_trait]
    impl DispatchAlertRepository for StubAlertRepo {
        async fn save(&self, _alert: &DispatchAlert) -> Result<(), DomainError> {
            unimplemented!("save")
        }
        async fn find_by_id(&self, id: &str) -> Result<Option<DispatchAlert>, DomainError> {
            Ok(self.alerts.lock().unwrap().iter().find(|alert| alert.id == id).cloned())
        }
        async fn find_unresolved(&self, flight_id: Option<&str>) -> Result<Vec<DispatchAlert>, DomainError> {
            Ok(self
                .alerts
                .lock()
                .unwrap()
                .iter()
                .filter(|alert| {
                    !alert.is_resolved && flight_id.is_none_or(|flight| alert.flight_id.as_deref() == Some(flight))
                })
                .cloned()
                .collect())
        }
        async fn resolve(&self, id: &str, resolved_by: &str, notes: Option<&str>) -> Result<bool, DomainError> {
            let mut guards = self.alerts.lock().unwrap();
            let Some(alert) = guards.iter_mut().find(|alert| alert.id == id) else {
                return Ok(false);
            };
            if alert.is_resolved {
                return Ok(false);
            }
            alert.is_resolved = true;
            alert.resolved_at = Some(Utc::now());
            alert.resolved_by = Some(resolved_by.to_string());
            alert.resolution_notes = notes.map(str::to_string);
            Ok(true)
        }
        async fn upsert_overrun(&self, alert: &DispatchAlert) -> Result<OverrunAlertUpsert, DomainError> {
            let dedupe_key = alert.dedupe_key.clone().expect("dedupe_key required");
            let mut guards = self.alerts.lock().unwrap();
            let existing = guards
                .iter()
                .find(|item| item.dedupe_key.as_deref() == Some(dedupe_key.as_str()));
            let outcome = match existing {
                Some(current) => {
                    let reopened = current.is_resolved;
                    let mut updated = current.clone();
                    updated.message = alert.message.clone();
                    updated.details = alert.details.clone();
                    updated.last_detected_at = alert.last_detected_at;
                    if reopened {
                        updated.is_resolved = false;
                        updated.resolved_at = None;
                        updated.resolved_by = None;
                        updated.resolution_notes = None;
                        updated.acknowledged_at = None;
                        updated.acknowledged_by = None;
                        updated.occurrence_count += 1;
                    }
                    let index = guards
                        .iter()
                        .position(|item| item.id == updated.id)
                        .expect("must exist");
                    guards[index] = updated.clone();
                    OverrunAlertUpsert {
                        alert: updated,
                        inserted: false,
                        reopened,
                    }
                }
                None => {
                    guards.push(alert.clone());
                    OverrunAlertUpsert {
                        alert: alert.clone(),
                        inserted: true,
                        reopened: false,
                    }
                }
            };
            Ok(outcome)
        }
        async fn acknowledge(&self, id: &str, acknowledged_by: &str) -> Result<bool, DomainError> {
            let mut guards = self.alerts.lock().unwrap();
            let Some(alert) = guards.iter_mut().find(|alert| alert.id == id) else {
                return Ok(false);
            };
            if alert.is_resolved {
                return Ok(false);
            }
            alert.acknowledged_at = Some(Utc::now());
            alert.acknowledged_by = Some(acknowledged_by.to_string());
            Ok(true)
        }
        async fn auto_resolve(&self, id: &str) -> Result<bool, DomainError> {
            let mut guards = self.alerts.lock().unwrap();
            let Some(alert) = guards.iter_mut().find(|alert| alert.id == id) else {
                return Ok(false);
            };
            if alert.is_resolved {
                return Ok(false);
            }
            alert.is_resolved = true;
            alert.resolved_at = Some(Utc::now());
            alert.resolution_notes = Some("auto".to_string());
            Ok(true)
        }
    }

    fn flight_chain(current_start: DateTime<Utc>, next_start: DateTime<Utc>) -> Vec<DispatchOrder> {
        vec![
            in_progress_current(current_start, Some(current_start + Duration::minutes(20))),
            pending_next(next_start),
        ]
    }

    fn flight_chain_for(
        flight_id: &str,
        current_start: DateTime<Utc>,
        next_start: DateTime<Utc>,
    ) -> Vec<DispatchOrder> {
        let mut orders = flight_chain(current_start, next_start);
        for (index, order) in orders.iter_mut().enumerate() {
            order.flight_id = flight_id.to_string();
            order.id = format!("do-{flight_id}-{index}");
        }
        orders
    }

    #[tokio::test]
    async fn lifecycle_upserts_idempotently_and_does_not_notify_twice() {
        let now = fixed_now();
        let order_repo = Arc::new(StubOrderRepo::default());
        order_repo.replace(
            "fl-1",
            flight_chain(now - Duration::minutes(10), now + Duration::minutes(5)),
        );
        let alert_repo = Arc::new(StubAlertRepo::default());
        let service =
            DispatchOverrunWarningService::new(order_repo.clone(), alert_repo.clone()).with_clock(move || now);

        let first = service.evaluate_flight("fl-1").await.unwrap();
        assert_eq!(first.len(), 1);
        let outcome = &first[0];
        assert!(outcome.notify, "first detection must notify");
        let alert_id = outcome.alert.as_ref().unwrap().id.clone();

        let second = service.evaluate_flight("fl-1").await.unwrap();
        let outcome = &second[0];
        assert!(!outcome.notify, "identical active conflict must not notify twice");
        assert_eq!(outcome.alert.as_ref().unwrap().id, alert_id);
        assert_eq!(outcome.alert.as_ref().unwrap().occurrence_count, 1);
    }

    #[tokio::test]
    async fn lifecycle_auto_resolves_when_conflict_disappears() {
        let now = fixed_now();
        let order_repo = Arc::new(StubOrderRepo::default());
        order_repo.replace(
            "fl-1",
            flight_chain(now - Duration::minutes(10), now + Duration::minutes(5)),
        );
        let alert_repo = Arc::new(StubAlertRepo::default());
        let service =
            DispatchOverrunWarningService::new(order_repo.clone(), alert_repo.clone()).with_clock(move || now);

        service.evaluate_flight("fl-1").await.unwrap();
        assert_eq!(alert_repo.find_unresolved(None).await.unwrap().len(), 1);

        // 当前单完成后,冲突消失,告警自动关闭。
        let mut current = in_progress_current(now - Duration::minutes(10), Some(now + Duration::minutes(20)));
        current.status = DispatchOrderStatus::Completed;
        order_repo.replace("fl-1", vec![current, pending_next(now + Duration::minutes(5))]);

        let outcome = service.evaluate_flight("fl-1").await.unwrap();
        assert!(outcome[0].auto_resolved_alert_id.is_some());
        assert!(alert_repo.find_unresolved(None).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn lifecycle_reopens_resolved_alert_with_new_notification() {
        let now = fixed_now();
        let order_repo = Arc::new(StubOrderRepo::default());
        order_repo.replace(
            "fl-1",
            flight_chain(now - Duration::minutes(10), now + Duration::minutes(5)),
        );
        let alert_repo = Arc::new(StubAlertRepo::default());
        let service =
            DispatchOverrunWarningService::new(order_repo.clone(), alert_repo.clone()).with_clock(move || now);

        let first = service.evaluate_flight("fl-1").await.unwrap();
        let alert_id = first[0].alert.as_ref().unwrap().id.clone();
        assert!(alert_repo.auto_resolve(&alert_id).await.unwrap());
        assert!(alert_repo.find_unresolved(None).await.unwrap().is_empty());

        // 冲突重新出现:复用同一告警,递增 occurrence,再次通知。
        let outcome = service.evaluate_flight("fl-1").await.unwrap();
        assert!(outcome[0].notify, "reappearing conflict must notify again");
        let alert = outcome[0].alert.as_ref().unwrap();
        assert_eq!(alert.id, alert_id);
        assert_eq!(alert.occurrence_count, 2);
    }

    #[tokio::test]
    async fn scan_covers_all_flights_and_counts_summary() {
        let now = fixed_now();
        let order_repo = Arc::new(StubOrderRepo::default());
        order_repo.replace(
            "fl-1",
            flight_chain_for("fl-1", now - Duration::minutes(10), now + Duration::minutes(5)),
        );
        order_repo.replace(
            "fl-2",
            flight_chain_for("fl-2", now - Duration::minutes(20), now + Duration::minutes(3)),
        );
        // 窗口外的航班不参与扫描。
        order_repo.replace(
            "fl-3",
            flight_chain_for("fl-3", now - Duration::hours(6), now - Duration::hours(5)),
        );
        let alert_repo = Arc::new(StubAlertRepo::default());
        let service =
            DispatchOverrunWarningService::new(order_repo.clone(), alert_repo.clone()).with_clock(move || now);

        let summary = service.scan_once().await.unwrap();
        assert_eq!(summary.evaluated_pairs, 2);
        assert_eq!(summary.notifications, 2);
        assert_eq!(summary.active_alerts, 2);
        assert_eq!(summary.auto_resolved, 0);
        assert_eq!(summary.eta_missing, 0);
    }

    #[tokio::test]
    async fn scan_auto_resolves_stale_conflict() {
        let now = fixed_now();
        let order_repo = Arc::new(StubOrderRepo::default());
        order_repo.replace(
            "fl-1",
            flight_chain_for("fl-1", now - Duration::minutes(10), now + Duration::minutes(5)),
        );
        let alert_repo = Arc::new(StubAlertRepo::default());
        let service =
            DispatchOverrunWarningService::new(order_repo.clone(), alert_repo.clone()).with_clock(move || now);

        let first = service.evaluate_flight("fl-1").await.unwrap();
        assert!(first[0].notify);
        assert_eq!(alert_repo.find_unresolved(None).await.unwrap().len(), 1);

        // 下一单被取消后,扫描器应自动关闭告警。
        let mut orders = flight_chain_for("fl-1", now - Duration::minutes(10), now + Duration::minutes(5));
        orders[1].status = DispatchOrderStatus::Cancelled;
        order_repo.replace("fl-1", orders);

        let summary = service.scan_once().await.unwrap();
        assert_eq!(summary.auto_resolved, 1);
        assert_eq!(summary.active_alerts, 0);
        assert!(alert_repo.find_unresolved(None).await.unwrap().is_empty());
    }
}
