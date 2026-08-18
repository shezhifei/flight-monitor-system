import time
from collections import deque
from threading import Lock
from typing import Any

from src.infrastructure.ai.monitoring.prometheus_exporter import (
    inc_error,
    inc_tool_call,
    observe_first_progress,
    observe_request_latency,
    observe_tokens,
    observe_tool_duration,
)
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)

FIRST_PROGRESS_TARGET_MS = 1500.0
EVENT_INTERVAL_TARGET_MS = 3000.0


class MetricsCollector:
    """
    指标收集器

    收集和聚合 AI 系统运行指标，支持：
    - 请求计数 (Counter)
    - 延迟统计 (Histogram/Summary)
    - Token 使用量 (Counter)
    - 错误率 (Counter)
    - 并发数 (Gauge)

    设计目标是兼容 Prometheus 格式。
    """

    _instance = None
    _lock = Lock()
    MAX_HISTOGRAM_SAMPLES = 2048
    MAX_SERIES_PER_METRIC = 64
    IDLE_SERIES_TTL_SECONDS = 3600.0

    def __new__(cls):
        with cls._lock:
            if cls._instance is None:
                cls._instance = super().__new__(cls)
                cls._instance._initialized = False
            return cls._instance

    def __init__(self):
        if self._initialized:
            return

        # 指标存储
        self._counters: dict[str, float] = {}
        self._histograms: dict[str, deque] = {}
        self._gauges: dict[str, float] = {}
        self._counter_last_updated: dict[str, float] = {}
        self._histogram_last_updated: dict[str, float] = {}
        self._gauge_last_updated: dict[str, float] = {}

        # 标签处理 (metric_name -> {label_hash -> value})
        pass  # 简化版暂不实现完全的带标签存储，仅支持 metric_name key

        # 简化存储：key = metric_name + labels_str

        self._initialized = True
        logger.info("MetricsCollector initialized")

    def _get_key(self, name: str, labels: dict[str, str]) -> str:
        if not labels:
            return name
        sorted_labels = sorted(labels.items())
        label_str = ",".join([f"{k}={v}" for k, v in sorted_labels])
        return f"{name}{{{label_str}}}"

    @staticmethod
    def _percentile(values: list[float], percentile: float) -> float:
        """Calculate percentile using linear interpolation on sorted samples."""
        if not values:
            return 0.0

        ordered = sorted(values)
        if len(ordered) == 1:
            return float(ordered[0])

        pct = max(0.0, min(100.0, float(percentile)))
        rank = (pct / 100.0) * (len(ordered) - 1)
        lower_index = int(rank)
        upper_index = min(lower_index + 1, len(ordered) - 1)

        lower_value = float(ordered[lower_index])
        upper_value = float(ordered[upper_index])
        if upper_index == lower_index:
            return lower_value

        fraction = rank - lower_index
        return lower_value + (upper_value - lower_value) * fraction

    def inc_counter(self, name: str, value: float = 1.0, labels: dict[str, str] | None = None):
        """增加计数器"""
        key = self._get_key(name, labels or {})
        with self._lock:
            self._compact_series(self._counters, self._counter_last_updated, name, key)
            self._counters[key] = self._counters.get(key, 0.0) + value
            self._counter_last_updated[key] = time.time()

    def observe_histogram(self, name: str, value: float, labels: dict[str, str] | None = None):
        """记录直方图数据"""
        key = self._get_key(name, labels or {})
        with self._lock:
            self._compact_series(self._histograms, self._histogram_last_updated, name, key)
            if key not in self._histograms:
                self._histograms[key] = deque(maxlen=self.MAX_HISTOGRAM_SAMPLES)
            self._histograms[key].append(value)
            self._histogram_last_updated[key] = time.time()
            # 使用固定窗口限制内存占用

    def set_gauge(self, name: str, value: float, labels: dict[str, str] | None = None):
        """设置仪表盘值"""
        key = self._get_key(name, labels or {})
        with self._lock:
            self._compact_series(self._gauges, self._gauge_last_updated, name, key)
            self._gauges[key] = value
            self._gauge_last_updated[key] = time.time()

    @staticmethod
    def _metric_name_from_key(metric_key: str) -> str:
        name, _labels = _parse_metric_key(metric_key)
        return name

    def _compact_series(
        self,
        store: dict[str, Any],
        last_updated: dict[str, float],
        metric_name: str,
        incoming_key: str,
    ) -> None:
        now = time.time()

        for key in list(last_updated.keys()):
            if key == incoming_key:
                continue
            if now - float(last_updated.get(key, now)) <= self.IDLE_SERIES_TTL_SECONDS:
                continue
            store.pop(key, None)
            last_updated.pop(key, None)

        if incoming_key in store:
            return

        metric_keys = [key for key in store if self._metric_name_from_key(key) == metric_name]
        if len(metric_keys) < self.MAX_SERIES_PER_METRIC:
            return

        evictable_keys = sorted(
            metric_keys,
            key=lambda key: float(last_updated.get(key, 0.0)),
        )
        for key in evictable_keys[: max(1, len(metric_keys) - self.MAX_SERIES_PER_METRIC + 1)]:
            store.pop(key, None)
            last_updated.pop(key, None)

    def get_metrics(self) -> dict[str, Any]:
        """获取所有指标快照"""
        with self._lock:
            return {
                "counters": self._counters.copy(),
                "gauges": self._gauges.copy(),
                # histograms 仅返回基本的 count/sum/avg
                "histograms": {
                    k: {
                        "count": len(v),
                        "sum": sum(v),
                        "avg": sum(v) / len(v) if v else 0,
                        "p95": self._percentile(list(v), 95),
                    }
                    for k, v in self._histograms.items()
                },
            }

    def reset(self):
        """重置指标 (测试用)"""
        with self._lock:
            self._counters.clear()
            self._histograms.clear()
            self._gauges.clear()
            self._counter_last_updated.clear()
            self._histogram_last_updated.clear()
            self._gauge_last_updated.clear()


# 全局实例
metrics = MetricsCollector()


# 常用便捷方法
def record_latency(operation: str, latency_ms: float, entity_id: str = "unknown"):
    metrics.observe_histogram("ai_request_latency_ms", latency_ms, {"operation": str(operation or "unknown")})
    observe_request_latency(latency_ms / 1000.0, operation)


def record_tokens(model: str, prompt_tokens: int, completion_tokens: int, entity_id: str = "unknown"):
    labels = {"model": str(model or "unknown")}
    metrics.inc_counter("ai_tokens_total", prompt_tokens + completion_tokens, labels)
    metrics.inc_counter("ai_prompt_tokens", prompt_tokens, labels)
    metrics.inc_counter("ai_completion_tokens", completion_tokens, labels)
    observe_tokens(model, prompt_tokens, completion_tokens)


def record_error(error_type: str, entity_id: str = "unknown"):
    metrics.inc_counter("ai_errors_total", 1, {"type": str(error_type or "unknown")})
    inc_error(error_type)


def record_tool_usage(tool_name: str, status: str, duration_ms: float):
    metrics.inc_counter("ai_tool_calls_total", 1, {"tool": tool_name, "status": status})
    metrics.observe_histogram("ai_tool_duration_ms", duration_ms, {"tool": tool_name})
    inc_tool_call(tool_name, status)
    observe_tool_duration(tool_name, duration_ms / 1000.0)


def record_query_route(
    *,
    intent: str,
    dataset: str,
    adapter: str,
    status: str = "success",
    misroute: bool = False,
    reason: str = "none",
):
    labels = {
        "intent": str(intent or "unknown"),
        "dataset": str(dataset or "unknown"),
        "adapter": str(adapter or "unknown"),
        "status": str(status or "unknown"),
        "misroute": "true" if misroute else "false",
        "reason": str(reason or "none"),
    }
    metrics.inc_counter("ai_query_route_total", 1, labels)
    if misroute:
        metrics.inc_counter(
            "ai_query_misroute_total",
            1,
            {
                "intent": str(intent or "unknown"),
                "dataset": str(dataset or "unknown"),
                "reason": str(reason or "unknown"),
            },
        )


def record_query_tool_selection(
    *,
    status: str,
    mismatch: bool,
    tool_name: str = "QUERY",
    reason: str = "none",
):
    labels = {
        "tool": str(tool_name or "QUERY"),
        "status": str(status or "unknown"),
        "mismatch": "true" if mismatch else "false",
        "reason": str(reason or "none"),
    }
    metrics.inc_counter("ai_query_selection_total", 1, labels)
    if mismatch:
        metrics.inc_counter(
            "ai_query_misselection_total",
            1,
            {
                "tool": str(tool_name or "QUERY"),
                "status": str(status or "unknown"),
                "reason": str(reason or "unknown"),
            },
        )


def record_report_schema_validation(
    *,
    schema_valid: bool,
    mode: str,
    report_type: str = "unknown",
    error_count: int = 0,
):
    labels = {
        "schema_valid": "true" if schema_valid else "false",
        "mode": str(mode or "unknown"),
        "report_type": str(report_type or "unknown"),
    }
    metrics.inc_counter("ai_report_schema_validation_total", 1, labels)
    metrics.observe_histogram(
        "ai_report_schema_validation_error_count",
        max(0, int(error_count or 0)),
        labels,
    )


def record_execution_visibility_sample(
    *,
    event: str,
    status: str,
    phase: str,
    first_progress_latency_ms: float | None = None,
    event_interval_ms: float | None = None,
) -> None:
    metrics.inc_counter(
        "ai_execution_event_total",
        1,
        {
            "event": str(event or "unknown"),
            "status": str(status or "unknown"),
            "phase": str(phase or "unknown"),
        },
    )

    if first_progress_latency_ms is not None:
        latency = max(0.0, float(first_progress_latency_ms))
        metrics.observe_histogram("ai_execution_first_progress_latency_ms", latency)
        metrics.inc_counter("ai_execution_first_progress_total", 1)
        # J1: Prometheus histogram sliceable by task_type (bound by the run
        # entry point); alerts on p95 > 3s live in fms-slo-alerts.yml.
        observe_first_progress(latency / 1000.0)
        if latency > FIRST_PROGRESS_TARGET_MS:
            metrics.inc_counter("ai_execution_first_progress_violation_total", 1)

    if event_interval_ms is not None:
        interval = max(0.0, float(event_interval_ms))
        metrics.observe_histogram("ai_execution_event_interval_ms", interval)
        metrics.inc_counter("ai_execution_event_interval_total", 1)
        if interval > EVENT_INTERVAL_TARGET_MS:
            metrics.inc_counter("ai_execution_event_interval_violation_total", 1)


def _parse_metric_key(metric_key: str) -> tuple[str, dict[str, str]]:
    text = str(metric_key or "")
    if "{" not in text or not text.endswith("}"):
        return text, {}
    name, rest = text.split("{", 1)
    labels_text = rest[:-1]
    labels: dict[str, str] = {}
    for part in labels_text.split(","):
        if "=" not in part:
            continue
        k, v = part.split("=", 1)
        key = str(k or "").strip()
        value = str(v or "").strip()
        if key:
            labels[key] = value
    return name, labels


def get_query_observability_snapshot() -> dict[str, Any]:
    snapshot = metrics.get_metrics()
    counters: dict[str, float] = snapshot.get("counters", {}) or {}
    route_total = 0.0
    misroute_total = 0.0
    selection_total = 0.0
    misselection_total = 0.0
    reason_buckets: dict[str, float] = {}

    for key, value in counters.items():
        name, labels = _parse_metric_key(key)
        numeric = float(value or 0.0)
        if name == "ai_query_route_total":
            route_total += numeric
            reason = labels.get("reason", "none")
            reason_buckets[reason] = reason_buckets.get(reason, 0.0) + numeric
        elif name == "ai_query_misroute_total":
            misroute_total += numeric
        elif name == "ai_query_selection_total":
            selection_total += numeric
        elif name == "ai_query_misselection_total":
            misselection_total += numeric

    top_reasons = sorted(
        ({"reason": reason, "count": int(count)} for reason, count in reason_buckets.items()),
        key=lambda item: item["count"],
        reverse=True,
    )[:10]

    return {
        "query_route_total": int(route_total),
        "query_misroute_total": int(misroute_total),
        "query_misroute_rate": float(misroute_total / route_total) if route_total > 0 else 0.0,
        "query_selection_total": int(selection_total),
        "query_misselection_total": int(misselection_total),
        "query_misselection_rate": float(misselection_total / selection_total) if selection_total > 0 else 0.0,
        "top_reasons": top_reasons,
    }


def get_report_schema_validation_snapshot() -> dict[str, Any]:
    snapshot = metrics.get_metrics()
    counters: dict[str, float] = snapshot.get("counters", {}) or {}
    total = 0.0
    invalid = 0.0
    modes: dict[str, float] = {}

    for key, value in counters.items():
        name, labels = _parse_metric_key(key)
        if name != "ai_report_schema_validation_total":
            continue
        numeric = float(value or 0.0)
        total += numeric
        if labels.get("schema_valid", "").lower() == "false":
            invalid += numeric
        mode = labels.get("mode", "unknown")
        modes[mode] = modes.get(mode, 0.0) + numeric

    mode_breakdown = sorted(
        ({"mode": mode, "count": int(count)} for mode, count in modes.items()),
        key=lambda item: item["count"],
        reverse=True,
    )

    return {
        "schema_validation_total": int(total),
        "schema_validation_invalid_total": int(invalid),
        "schema_validation_invalid_rate": float(invalid / total) if total > 0 else 0.0,
        "mode_breakdown": mode_breakdown,
    }


def _sum_counter_by_name(counters: dict[str, float], metric_name: str) -> float:
    total = 0.0
    for key, value in (counters or {}).items():
        name, _labels = _parse_metric_key(key)
        if name == metric_name:
            total += float(value or 0.0)
    return total


def _histogram_stat(histograms: dict[str, dict[str, Any]], key: str) -> dict[str, Any]:
    item = histograms.get(key, {}) if isinstance(histograms, dict) else {}
    count = int(item.get("count", 0) or 0)
    avg = float(item.get("avg", 0.0) or 0.0)
    p95 = float(item.get("p95", 0.0) or 0.0)
    return {
        "count": count,
        "avg": avg,
        "p95": p95,
    }


def get_execution_visibility_snapshot() -> dict[str, Any]:
    snapshot = metrics.get_metrics()
    counters: dict[str, float] = snapshot.get("counters", {}) or {}
    histograms: dict[str, dict[str, Any]] = snapshot.get("histograms", {}) or {}

    event_total = _sum_counter_by_name(counters, "ai_execution_event_total")
    first_progress_total = _sum_counter_by_name(counters, "ai_execution_first_progress_total")
    first_progress_violations = _sum_counter_by_name(counters, "ai_execution_first_progress_violation_total")
    interval_total = _sum_counter_by_name(counters, "ai_execution_event_interval_total")
    interval_violations = _sum_counter_by_name(counters, "ai_execution_event_interval_violation_total")

    first_progress_stats = _histogram_stat(histograms, "ai_execution_first_progress_latency_ms")
    interval_stats = _histogram_stat(histograms, "ai_execution_event_interval_ms")

    first_progress_target_met = (
        first_progress_stats["count"] > 0 and first_progress_stats["p95"] < FIRST_PROGRESS_TARGET_MS
    )
    interval_target_met = interval_stats["count"] > 0 and int(interval_violations) == 0

    return {
        "execution_event_total": int(event_total),
        "first_progress_latency_ms": {
            **first_progress_stats,
            "target_p95_lt_ms": FIRST_PROGRESS_TARGET_MS,
            "violation_total": int(first_progress_violations),
            "target_met": first_progress_target_met,
        },
        "event_interval_ms": {
            **interval_stats,
            "target_lte_ms": EVENT_INTERVAL_TARGET_MS,
            "violation_total": int(interval_violations),
            "target_met": interval_target_met,
        },
        "coverage": {
            "first_progress_samples": int(first_progress_total),
            "event_interval_samples": int(interval_total),
        },
    }
