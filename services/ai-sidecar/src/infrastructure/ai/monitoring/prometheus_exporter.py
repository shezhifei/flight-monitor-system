"""Prometheus export surface for the AI sidecar metrics.

This module owns the Prometheus metric objects and the default
``prometheus_client.REGISTRY``. It intentionally does NOT import
``src.infrastructure.ai.monitoring.metrics`` back, so the existing JSON
``MetricsCollector`` can call into this module without creating an import
cycle.

The existing in-memory ``MetricsCollector`` (``metrics.py``) remains the
source of truth for JSON observability; the functions here bridge those
recorders into Prometheus counters/histograms so both surfaces stay
consistent.
"""

from __future__ import annotations

from typing import Any

try:
    from prometheus_client import REGISTRY, Counter, Histogram, generate_latest

    _PROM_AVAILABLE = True
except Exception:  # noqa: BLE001 - prometheus_client may be absent at import time
    REGISTRY = None  # type: ignore[assignment]
    Counter = None  # type: ignore[assignment]
    Histogram = None  # type: ignore[assignment]
    generate_latest = None  # type: ignore[assignment]
    _PROM_AVAILABLE = False


class _NoopMetric:
    """Fallback metric used when prometheus_client is not installed."""

    def labels(self, *args: Any, **kwargs: Any) -> _NoopMetric:
        return self

    def inc(self, amount: float = 1.0) -> None:
        return None

    def observe(self, amount: float) -> None:
        return None

    def set(self, amount: float) -> None:  # pragma: no cover - defensive
        return None


def _make_counter(name: str, help_text: str, labelnames: list[str] | None = None) -> Any:
    if not _PROM_AVAILABLE:
        return _NoopMetric()
    return Counter(name, help_text, labelnames or [])


def _make_histogram(
    name: str,
    help_text: str,
    labelnames: list[str] | None = None,
    buckets: tuple[float, ...] | None = None,
) -> Any:
    if not _PROM_AVAILABLE:
        return _NoopMetric()
    kwargs: dict[str, Any] = {"labelnames": labelnames or []}
    if buckets is not None:
        kwargs["buckets"] = list(buckets)
    return Histogram(name, help_text, **kwargs)


# ---------------------------------------------------------------------------
# Metric definitions
# ---------------------------------------------------------------------------

_LATENCY_BUCKETS = (
    0.005,
    0.01,
    0.025,
    0.05,
    0.1,
    0.25,
    0.5,
    1.0,
    2.5,
    5.0,
    10.0,
    30.0,
    60.0,
)

_DURATION_BUCKETS = (
    0.005,
    0.01,
    0.025,
    0.05,
    0.1,
    0.25,
    0.5,
    1.0,
    2.5,
    5.0,
    10.0,
    30.0,
    60.0,
    120.0,
)

fms_ai_llm_calls_total: Any = _make_counter(
    "fms_ai_llm_calls_total",
    "Total number of LLM invocations issued by the runtime.",
    ["model"],
)

fms_ai_tokens_total: Any = _make_counter(
    "fms_ai_tokens_total",
    "Total tokens consumed, labelled by model and type (prompt|completion).",
    ["model", "type"],
)

fms_ai_tool_calls_total: Any = _make_counter(
    "fms_ai_tool_calls_total",
    "Total tool invocations labelled by tool and status.",
    ["tool", "status"],
)

fms_ai_tool_duration_seconds: Any = _make_histogram(
    "fms_ai_tool_duration_seconds",
    "Tool execution duration in seconds, labelled by tool.",
    ["tool"],
    _DURATION_BUCKETS,
)

fms_ai_request_latency_seconds: Any = _make_histogram(
    "fms_ai_request_latency_seconds",
    "End-to-end request latency in seconds, labelled by operation.",
    ["operation"],
    _LATENCY_BUCKETS,
)

fms_ai_errors_total: Any = _make_counter(
    "fms_ai_errors_total",
    "Total errors labelled by type.",
    ["type"],
)

fms_ai_mq_gate_decisions_total: Any = _make_counter(
    "fms_ai_mq_gate_decisions_total",
    "Total MQ gate authorization decisions labelled by decision branch.",
    ["decision"],
)


# ---------------------------------------------------------------------------
# Bridge helpers (called by metrics.py recorders and the MQ gate)
# ---------------------------------------------------------------------------


def inc_llm_call(model: str = "unknown") -> None:
    fms_ai_llm_calls_total.labels(model=str(model or "unknown")).inc()


def observe_tokens(model: str, prompt_tokens: int, completion_tokens: int) -> None:
    model = str(model or "unknown")
    if prompt_tokens:
        fms_ai_tokens_total.labels(model=model, type="prompt").inc(prompt_tokens)
    if completion_tokens:
        fms_ai_tokens_total.labels(model=model, type="completion").inc(completion_tokens)


def inc_tool_call(tool: str, status: str) -> None:
    fms_ai_tool_calls_total.labels(tool=str(tool or "unknown"), status=str(status or "unknown")).inc()


def observe_tool_duration(tool: str, duration_seconds: float) -> None:
    fms_ai_tool_duration_seconds.labels(tool=str(tool or "unknown")).observe(max(0.0, float(duration_seconds)))


def observe_request_latency(latency_seconds: float, operation: str = "unknown") -> None:
    fms_ai_request_latency_seconds.labels(operation=str(operation or "unknown")).observe(
        max(0.0, float(latency_seconds))
    )


def inc_error(error_type: str) -> None:
    fms_ai_errors_total.labels(type=str(error_type or "unknown")).inc()


def inc_mq_gate_decision(decision: str) -> None:
    fms_ai_mq_gate_decisions_total.labels(decision=str(decision or "unknown")).inc()
