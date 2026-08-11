from collections.abc import Callable
from typing import Any

from src.infrastructure.logging.core import get_logger

from .metrics import metrics

logger = get_logger(__name__)


class AIAlertSystem:
    """
    AI 告警系统

    定期检查指标是否超过阈值，并触发告警。
    """

    def __init__(self):
        self._handlers: list[Callable[[str, dict], None]] = []
        self._thresholds: dict[str, float] = {
            "error_rate_5m": 0.05,  # 5分钟内错误率 > 5%
            "latency_p95": 5000.0,  # P95延迟 > 5秒
        }

    def add_alert_handler(self, handler: Callable[[str, dict], None]):
        """注册告警处理器 (如发送 Slack, 邮件)"""
        self._handlers.append(handler)

    @staticmethod
    def _sum_counter_with_prefix(counters: dict[str, Any], metric_prefix: str) -> float:
        total = 0.0
        for name, value in (counters or {}).items():
            if str(name).startswith(metric_prefix):
                try:
                    total += float(value)
                except (TypeError, ValueError):
                    continue
        return total

    @staticmethod
    def _max_histogram_p95(histograms: dict[str, Any], metric_prefix: str) -> float:
        max_p95 = 0.0
        for name, stats in (histograms or {}).items():
            if not str(name).startswith(metric_prefix):
                continue
            if not isinstance(stats, dict):
                continue
            try:
                max_p95 = max(max_p95, float(stats.get("p95", 0.0) or 0.0))
            except (TypeError, ValueError):
                continue
        return max_p95

    def check_alerts(self):
        """检查所有告警规则 (应由定时任务调用)"""
        metrics_data = metrics.get_metrics()
        counters = metrics_data.get("counters", {})
        histograms = metrics_data.get("histograms", {})
        triggered_alerts: list[str] = []

        # 1) 错误率告警（在当前 MetricsCollector 能力范围内，使用累计计数做近似）
        error_total = self._sum_counter_with_prefix(counters, "ai_errors_total")
        request_total = self._sum_counter_with_prefix(counters, "ai_requests_total")
        if request_total <= 0:
            # 兼容当前埋点：若未上报 ai_requests_total，则使用工具调用数作为请求近似
            request_total = self._sum_counter_with_prefix(counters, "ai_tool_calls_total")

        if request_total > 0:
            error_rate = error_total / request_total
            threshold = float(self._thresholds.get("error_rate_5m", 0.05))
            if error_rate > threshold:
                self.trigger_alert(
                    alert_name="ai_error_rate_high",
                    severity="critical",
                    details={
                        "error_rate": round(error_rate, 4),
                        "threshold": threshold,
                        "errors": int(error_total),
                        "requests": int(request_total),
                    },
                )
                triggered_alerts.append("ai_error_rate_high")

        # 2) 延迟告警（基于 p95）
        p95_latency = self._max_histogram_p95(histograms, "ai_request_latency_ms")
        latency_threshold = float(self._thresholds.get("latency_p95", 5000.0))
        if p95_latency > latency_threshold:
            self.trigger_alert(
                alert_name="ai_latency_p95_high",
                severity="warning",
                details={
                    "latency_p95_ms": round(p95_latency, 2),
                    "threshold_ms": latency_threshold,
                },
            )
            triggered_alerts.append("ai_latency_p95_high")

        return triggered_alerts

    def trigger_alert(self, alert_name: str, severity: str, details: dict[str, Any]):
        """手动触发告警"""
        alert_payload = {
            "alert": alert_name,
            "severity": severity,
            "details": details,
        }
        logger.warning(f"ALERT TRIGGERED: {alert_name}", extra=alert_payload)

        for handler in self._handlers:
            try:
                handler(alert_name, alert_payload)
            except Exception as e:  # noqa: BLE001 - alert handler callbacks must not propagate failures
                logger.error(f"Failed to execute alert handler: {e}")


# 全局实例
alert_system = AIAlertSystem()
