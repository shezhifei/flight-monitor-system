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

from contextlib import contextmanager
from contextvars import ContextVar
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


# ---------------------------------------------------------------------------
# Run metric context (Task J1)
#
# ``stream_run_with_tools`` binds the current run's task_type / entity_id
# here; bridge helpers fall back to these values when callers do not pass
# labels explicitly, so every per-run metric is sliceable by task type.
# ---------------------------------------------------------------------------

_task_type_var: ContextVar[str] = ContextVar("fms_ai_metric_task_type", default="unknown")
_entity_id_var: ContextVar[str] = ContextVar("fms_ai_metric_entity_id", default="unknown")


def _label(value: Any, fallback: str = "unknown") -> str:
    text = str(value or "").strip()
    return text or fallback


@contextmanager
def bind_metric_context(task_type: str | None = None, entity_id: str | None = None):
    """Bind the run-scoped metric labels for the current context."""
    tt_token = _task_type_var.set(_label(task_type))
    ei_token = _entity_id_var.set(_label(entity_id))
    try:
        yield
    finally:
        _task_type_var.reset(tt_token)
        _entity_id_var.reset(ei_token)


def current_task_type() -> str:
    return _task_type_var.get()


def current_entity_id() -> str:
    return _entity_id_var.get()


def begin_metric_context(task_type: str | None = None, entity_id: str | None = None) -> tuple[Any, Any]:
    """Bind run-scoped metric labels; returns tokens for :func:`end_metric_context`.

    Used by async generators where a ``with`` block cannot span early
    returns (``stream_run_with_tools``).
    """
    return (
        _task_type_var.set(_label(task_type)),
        _entity_id_var.set(_label(entity_id)),
    )


def end_metric_context(tokens: tuple[Any, Any]) -> None:
    """Restore the labels bound before :func:`begin_metric_context`."""
    _task_type_var.reset(tokens[0])
    _entity_id_var.reset(tokens[1])


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
    ["model", "task_type", "entity_id", "status"],
)

fms_ai_tokens_total: Any = _make_counter(
    "fms_ai_tokens_total",
    "Total tokens consumed, labelled by model, type (prompt|completion) and task type.",
    ["model", "type", "task_type"],
)

fms_ai_tool_calls_total: Any = _make_counter(
    "fms_ai_tool_calls_total",
    "Total tool invocations labelled by tool, task type, status and blocking gate.",
    ["tool", "task_type", "status", "blocked_by"],
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

fms_ai_run_cost_usd: Any = _make_counter(
    "fms_ai_run_cost_usd",
    "Estimated LLM spend in USD per run, labelled by task type and entity.",
    ["task_type", "entity_id"],
)

fms_ai_price_missing_total: Any = _make_counter(
    "fms_ai_price_missing_total",
    "LLM calls whose model had no price-table entry (recorded cost is 0).",
    [],
)

fms_ai_first_progress_seconds: Any = _make_histogram(
    "fms_ai_first_progress_seconds",
    "Time to first progress event of a run, labelled by task type (target 1.5s).",
    ["task_type"],
    _LATENCY_BUCKETS,
)

fms_ai_resume_total: Any = _make_counter(
    "fms_ai_resume_total",
    "Checkpoint resume attempts labelled by outcome status.",
    ["status"],
)

fms_ai_proposal_total: Any = _make_counter(
    "fms_ai_proposal_total",
    "Pending-action lifecycle events labelled by action and status.",
    ["action", "status"],
)


# ---------------------------------------------------------------------------
# Bridge helpers (called by metrics.py recorders and the MQ gate)
# ---------------------------------------------------------------------------


def inc_llm_call(
    model: str = "unknown",
    *,
    status: str = "ok",
    task_type: str | None = None,
    entity_id: str | None = None,
) -> None:
    fms_ai_llm_calls_total.labels(
        model=_label(model),
        task_type=_label(task_type, current_task_type()),
        entity_id=_label(entity_id, current_entity_id()),
        status=_label(status),
    ).inc()


def observe_tokens(
    model: str,
    prompt_tokens: int,
    completion_tokens: int,
    *,
    task_type: str | None = None,
) -> None:
    model = _label(model)
    task_type = _label(task_type, current_task_type())
    if prompt_tokens:
        fms_ai_tokens_total.labels(model=model, type="prompt", task_type=task_type).inc(prompt_tokens)
    if completion_tokens:
        fms_ai_tokens_total.labels(model=model, type="completion", task_type=task_type).inc(completion_tokens)


def observe_run_cost(
    model: str,
    prompt_tokens: int,
    completion_tokens: int,
    *,
    task_type: str | None = None,
    entity_id: str | None = None,
) -> None:
    """Record the estimated USD cost of one LLM call (Task J1)."""
    from src.infrastructure.ai.monitoring.model_prices import estimate_run_cost_usd

    cost, price_missing = estimate_run_cost_usd(model, prompt_tokens, completion_tokens)
    fms_ai_run_cost_usd.labels(
        task_type=_label(task_type, current_task_type()),
        entity_id=_label(entity_id, current_entity_id()),
    ).inc(max(0.0, float(cost)))
    if price_missing:
        # Unlabelled counter: prometheus_client forbids .labels() on it.
        fms_ai_price_missing_total.inc()


def inc_tool_call(
    tool: str,
    status: str,
    *,
    task_type: str | None = None,
    blocked_by: str | None = None,
) -> None:
    fms_ai_tool_calls_total.labels(
        tool=_label(tool),
        task_type=_label(task_type, current_task_type()),
        status=_label(status),
        blocked_by=_label(blocked_by, "none"),
    ).inc()


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


def observe_first_progress(latency_seconds: float, *, task_type: str | None = None) -> None:
    fms_ai_first_progress_seconds.labels(task_type=_label(task_type, current_task_type())).observe(
        max(0.0, float(latency_seconds))
    )


def inc_resume(status: str) -> None:
    fms_ai_resume_total.labels(status=_label(status)).inc()


def inc_proposal(action: str, status: str) -> None:
    fms_ai_proposal_total.labels(action=_label(action), status=_label(status)).inc()
