"""Todo graph pilot operational snapshot helpers."""

from __future__ import annotations

from collections.abc import Iterable
from datetime import datetime, timedelta
from typing import Any

from src.domain.ai.todo_graph_pilot import (
    DEFAULT_TODO_GRAPH_PILOT_ENTITY_ID,
    DEFAULT_TODO_GRAPH_PILOT_WINDOW_HOURS,
)
from src.domain.utils.time_utils import utc_now

_THRESHOLDS: dict[str, Any] = {
    "ready_graph_requested_total_min": 30,
    "ready_completion_rate_min": 0.95,
    "ready_graph_fallback_rate_max": 0.05,
    "ready_graph_resume_total_min": 5,
    "ready_graph_resume_success_rate_min": 0.95,
    "ready_duplicate_tool_execution_total_max": 0,
    "ready_duplicate_tool_execution_blocked_total_max": 0,
    "ready_stale_pending_total_max": 0,
    "rollback_graph_requested_total_min": 10,
    "rollback_graph_fallback_rate_gt": 0.20,
    "rollback_graph_resume_total_min": 5,
    "rollback_graph_resume_success_rate_lt": 0.90,
}


def _normalize_runtime_text(value: Any) -> str | None:
    if value is None:
        return None
    normalized = str(value).strip().lower()
    return normalized or None


def _parse_timestamp(value: Any) -> datetime | None:
    if value is None:
        return None
    if isinstance(value, datetime):
        return value
    text = str(value).strip()
    if not text:
        return None
    if text.endswith("Z"):
        text = f"{text[:-1]}+00:00"
    try:
        return datetime.fromisoformat(text)
    except ValueError:
        return None


def _safe_int(value: Any) -> int:
    try:
        return max(0, int(value or 0))
    except (TypeError, ValueError):
        return 0


def _safe_float(value: Any) -> float | None:
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def _percentile(values: list[float], percentile: float) -> float | None:
    if not values:
        return None
    if len(values) == 1:
        return round(values[0], 2)

    normalized = sorted(values)
    rank = (len(normalized) - 1) * max(0.0, min(1.0, percentile))
    lower = int(rank)
    upper = min(lower + 1, len(normalized) - 1)
    fraction = rank - lower
    interpolated = normalized[lower] + (normalized[upper] - normalized[lower]) * fraction
    return round(interpolated, 2)


def _distribution(values: list[float]) -> dict[str, Any]:
    normalized = [value for value in values if value is not None and value >= 0]
    return {
        "sample_size": len(normalized),
        "p50": _percentile(normalized, 0.50),
        "p95": _percentile(normalized, 0.95),
    }


def _extract_execution_duration_ms(execution: dict[str, Any]) -> float | None:
    if not isinstance(execution, dict):
        return None

    metadata = execution.get("metadata") if isinstance(execution.get("metadata"), dict) else {}
    runtime = execution.get("runtime") if isinstance(execution.get("runtime"), dict) else {}
    for source in (runtime, metadata):
        duration_ms = _safe_float(source.get("duration_ms")) if isinstance(source, dict) else None
        if duration_ms is not None and duration_ms >= 0:
            return duration_ms

    started_at = _parse_timestamp(execution.get("started_at"))
    finished_at = _parse_timestamp(execution.get("finished_at"))
    if started_at is None or finished_at is None:
        return None
    return round(max(0.0, (finished_at - started_at).total_seconds() * 1000), 2)


def _extract_approval_response_time_ms(action: dict[str, Any]) -> float | None:
    if not isinstance(action, dict):
        return None

    created_at = _parse_timestamp(action.get("created_at"))
    if created_at is None:
        return None

    for field_name in ("approved_at", "rejected_at", "updated_at"):
        resolved_at = _parse_timestamp(action.get(field_name))
        if resolved_at is not None:
            return round(max(0.0, (resolved_at - created_at).total_seconds() * 1000), 2)

    return None


def _build_value_metrics(
    *,
    executions: list[dict[str, Any]],
    pending_actions: list[dict[str, Any]],
) -> dict[str, Any]:
    graph_requested_run_ids = {
        str(item.get("run_id") or "").strip()
        for item in executions
        if isinstance(item, dict)
        and (
            _normalize_runtime_text((item.get("runtime") or {}).get("requested_path")) == "graph"
            or _normalize_runtime_text((item.get("metadata") or {}).get("runtime_path_requested")) == "graph"
        )
        and str(item.get("run_id") or "").strip()
    }

    execution_duration_ms = _distribution(
        [
            value
            for value in (
                _extract_execution_duration_ms(execution)
                for execution in executions
                if _normalize_runtime_text((execution or {}).get("status")) == "completed"
            )
            if value is not None
        ]
    )

    approval_response_time_ms = _distribution(
        [
            value
            for value in (
                _extract_approval_response_time_ms(action)
                for action in pending_actions
                if _normalize_runtime_text((action or {}).get("status")) != "pending"
            )
            if value is not None
        ]
    )

    approval_run_ids = {
        str(action.get("correlation_id") or "").strip()
        for action in pending_actions
        if isinstance(action, dict)
        and str(action.get("correlation_id") or "").strip()
        and (
            str(action.get("correlation_id") or "").strip() in graph_requested_run_ids
            or _normalize_runtime_text((action.get("execution_receipt") or {}).get("resume_mode")) == "graph"
        )
    }

    approval_rate = float(len(approval_run_ids) / len(graph_requested_run_ids)) if graph_requested_run_ids else 0.0

    return {
        "execution_duration_ms": execution_duration_ms,
        "approval_response_time_ms": approval_response_time_ms,
        "human_approval_rate": approval_rate,
        "graph_approval_run_total": len(approval_run_ids),
    }


def _top_fallback_reasons(executions: Iterable[dict[str, Any]]) -> list[dict[str, Any]]:
    buckets: dict[str, int] = {}
    for item in executions:
        runtime = item.get("runtime") if isinstance(item, dict) else None
        reason = None
        if isinstance(runtime, dict):
            reason = runtime.get("fallback_reason")
        if reason is None and isinstance(item, dict):
            reason = (item.get("metadata") or {}).get("runtime_fallback_reason")
        normalized = str(reason or "").strip()
        if not normalized:
            continue
        buckets[normalized] = buckets.get(normalized, 0) + 1

    return sorted(
        ({"reason": reason, "count": count} for reason, count in buckets.items()),
        key=lambda entry: entry["count"],
        reverse=True,
    )[:10]


def _scan_duplicate_tool_calls(steps_by_run_id: dict[str, list[dict[str, Any]]]) -> dict[str, int]:
    duplicate_total = 0
    duplicate_runs = 0
    for run_id, steps in (steps_by_run_id or {}).items():
        if not str(run_id or "").strip():
            continue
        occurrences: dict[str, int] = {}
        for step in steps or []:
            if not isinstance(step, dict):
                continue
            tool_calls = step.get("tool_calls")
            if not isinstance(tool_calls, list):
                continue
            for tool_call in tool_calls:
                if not isinstance(tool_call, dict):
                    continue
                tool_call_id = str(tool_call.get("tool_call_id") or "").strip()
                if not tool_call_id:
                    continue
                occurrences[tool_call_id] = occurrences.get(tool_call_id, 0) + 1

        duplicates_for_run = sum(max(0, count - 1) for count in occurrences.values())
        if duplicates_for_run > 0:
            duplicate_total += duplicates_for_run
            duplicate_runs += 1

    return {
        "duplicate_tool_execution_total": duplicate_total,
        "duplicate_tool_execution_runs": duplicate_runs,
    }


def _aggregate_guardrail_metrics(executions: list[dict[str, Any]]) -> dict[str, int]:
    duplicate_total = 0
    duplicate_runs = 0
    duplicate_blocked_total = 0
    duplicate_blocked_runs = 0

    for execution in executions:
        metadata = execution.get("metadata") if isinstance(execution, dict) else None
        guardrails = (metadata or {}).get("graph_runtime_guardrails") if isinstance(metadata, dict) else None
        if not isinstance(guardrails, dict):
            continue

        duplicates_for_run = _safe_int(guardrails.get("duplicate_tool_execution_total"))
        blocked_for_run = _safe_int(guardrails.get("duplicate_tool_execution_blocked_total"))

        duplicate_total += duplicates_for_run
        duplicate_blocked_total += blocked_for_run
        if duplicates_for_run > 0:
            duplicate_runs += 1
        if blocked_for_run > 0:
            duplicate_blocked_runs += 1

    return {
        "duplicate_tool_execution_total": duplicate_total,
        "duplicate_tool_execution_runs": duplicate_runs,
        "duplicate_tool_execution_blocked_total": duplicate_blocked_total,
        "duplicate_tool_execution_blocked_runs": duplicate_blocked_runs,
    }


def _build_verdict(
    *,
    entity_id: str | None,
    executions: dict[str, Any],
    approvals: dict[str, Any],
    guardrails: dict[str, Any],
) -> dict[str, Any] | None:
    if not str(entity_id or "").strip():
        return None

    graph_requested_total = _safe_int(executions.get("graph_requested_total"))
    completion_rate = float(executions.get("completion_rate") or 0.0)
    fallback_rate = float(executions.get("graph_fallback_rate") or 0.0)
    graph_resume_total = _safe_int(approvals.get("graph_resume_total"))
    graph_resume_success_rate = float(approvals.get("graph_resume_success_rate") or 0.0)
    stale_pending_total = _safe_int(approvals.get("stale_pending_total"))
    duplicate_total = _safe_int(guardrails.get("duplicate_tool_execution_total"))
    duplicate_blocked_total = _safe_int(guardrails.get("duplicate_tool_execution_blocked_total"))

    rollback_reasons: list[str] = []
    if duplicate_total > 0:
        rollback_reasons.append("duplicate tool execution detected")
    if (
        graph_requested_total >= _THRESHOLDS["rollback_graph_requested_total_min"]
        and fallback_rate > _THRESHOLDS["rollback_graph_fallback_rate_gt"]
    ):
        rollback_reasons.append("graph fallback rate exceeded rollback threshold")
    if (
        graph_resume_total >= _THRESHOLDS["rollback_graph_resume_total_min"]
        and graph_resume_success_rate < _THRESHOLDS["rollback_graph_resume_success_rate_lt"]
    ):
        rollback_reasons.append("graph resume success rate fell below rollback threshold")

    if rollback_reasons:
        return {"status": "rollback_recommended", "reasons": rollback_reasons}

    insufficient_reasons: list[str] = []
    if graph_requested_total < _THRESHOLDS["ready_graph_requested_total_min"]:
        insufficient_reasons.append("graph requested sample size below readiness threshold")
    if graph_resume_total < _THRESHOLDS["ready_graph_resume_total_min"]:
        insufficient_reasons.append("graph resume sample size below readiness threshold")

    if insufficient_reasons:
        return {"status": "insufficient_data", "reasons": insufficient_reasons}

    hold_reasons: list[str] = []
    if completion_rate < _THRESHOLDS["ready_completion_rate_min"]:
        hold_reasons.append("completion rate below readiness threshold")
    if fallback_rate > _THRESHOLDS["ready_graph_fallback_rate_max"]:
        hold_reasons.append("graph fallback rate above readiness threshold")
    if graph_resume_success_rate < _THRESHOLDS["ready_graph_resume_success_rate_min"]:
        hold_reasons.append("graph resume success rate below readiness threshold")
    if stale_pending_total > _THRESHOLDS["ready_stale_pending_total_max"]:
        hold_reasons.append("stale pending approvals detected")
    if duplicate_total > _THRESHOLDS["ready_duplicate_tool_execution_total_max"]:
        hold_reasons.append("duplicate tool execution detected")
    if duplicate_blocked_total > _THRESHOLDS["ready_duplicate_tool_execution_blocked_total_max"]:
        hold_reasons.append("duplicate execution attempts were blocked")

    if hold_reasons:
        return {"status": "hold", "reasons": hold_reasons}

    return {"status": "ready_to_expand", "reasons": []}


def build_todo_graph_pilot_thresholds() -> dict[str, Any]:
    return dict(_THRESHOLDS)


def build_todo_graph_pilot_snapshot(
    *,
    executions: list[dict[str, Any]],
    steps_by_run_id: dict[str, list[dict[str, Any]]] | None = None,
    pending_actions: list[dict[str, Any]],
    pending_total: int,
    pending_stale_after_minutes: int,
    entity_id: str | None = None,
    window_hours: int = DEFAULT_TODO_GRAPH_PILOT_WINDOW_HOURS,
    window_started_at: datetime | None = None,
    window_ended_at: datetime | None = None,
) -> dict[str, Any]:
    normalized_executions = [item for item in executions if isinstance(item, dict)]
    normalized_steps = steps_by_run_id if isinstance(steps_by_run_id, dict) else {}
    resolved_window_ended_at = window_ended_at or utc_now()
    resolved_window_started_at = window_started_at or (
        resolved_window_ended_at - timedelta(hours=max(1, int(window_hours or DEFAULT_TODO_GRAPH_PILOT_WINDOW_HOURS)))
    )

    execution_total = len(normalized_executions)
    completed_total = 0
    failed_total = 0
    cancelled_total = 0
    pending_execution_total = 0
    graph_requested_total = 0
    graph_actual_total = 0
    graph_fallback_total = 0

    for execution in normalized_executions:
        runtime = execution.get("runtime") if isinstance(execution.get("runtime"), dict) else {}
        status = _normalize_runtime_text(execution.get("status"))
        runtime_path = _normalize_runtime_text(runtime.get("path")) or _normalize_runtime_text(
            execution.get("runtime_path")
        )
        requested_path = _normalize_runtime_text(runtime.get("requested_path"))

        if requested_path == "graph":
            graph_requested_total += 1
        if runtime_path == "graph":
            graph_actual_total += 1
        if requested_path == "graph" and runtime_path == "legacy":
            graph_fallback_total += 1

        if status == "completed":
            completed_total += 1
        elif status == "failed":
            failed_total += 1
        elif status == "cancelled":
            cancelled_total += 1
        elif status == "pending":
            pending_execution_total += 1

    stale_threshold = resolved_window_ended_at - timedelta(minutes=max(1, int(pending_stale_after_minutes or 30)))
    normalized_pending = [item for item in pending_actions if isinstance(item, dict)]
    stale_pending_total = 0
    graph_resume_total = 0
    graph_resume_success_total = 0

    for action in normalized_pending:
        status = _normalize_runtime_text(action.get("status"))
        created_at = _parse_timestamp(action.get("created_at"))
        if status == "pending" and created_at is not None and created_at <= stale_threshold:
            stale_pending_total += 1

        receipt = action.get("execution_receipt")
        if not isinstance(receipt, dict):
            continue
        if _normalize_runtime_text(receipt.get("resume_mode")) != "graph":
            continue

        graph_resume_total += 1
        receipt_status = _normalize_runtime_text(receipt.get("status"))
        if receipt_status == "applied":
            graph_resume_success_total += 1

    guardrail_metrics = _aggregate_guardrail_metrics(normalized_executions)
    step_scan_metrics = _scan_duplicate_tool_calls(normalized_steps)
    guardrail_metrics["duplicate_tool_execution_total"] = max(
        guardrail_metrics["duplicate_tool_execution_total"],
        step_scan_metrics["duplicate_tool_execution_total"],
    )
    guardrail_metrics["duplicate_tool_execution_runs"] = max(
        guardrail_metrics["duplicate_tool_execution_runs"],
        step_scan_metrics["duplicate_tool_execution_runs"],
    )
    value_metrics_block = _build_value_metrics(
        executions=normalized_executions,
        pending_actions=normalized_pending,
    )

    completion_rate = float(completed_total / execution_total) if execution_total > 0 else 0.0
    fallback_rate = float(graph_fallback_total / graph_requested_total) if graph_requested_total > 0 else 0.0
    graph_resume_success_rate = (
        float(graph_resume_success_total / graph_resume_total) if graph_resume_total > 0 else 0.0
    )

    executions_block = {
        "total": execution_total,
        "completed_total": completed_total,
        "failed_total": failed_total,
        "cancelled_total": cancelled_total,
        "pending_total": pending_execution_total,
        "completion_rate": completion_rate,
        "graph_requested_total": graph_requested_total,
        "graph_actual_total": graph_actual_total,
        "graph_fallback_total": graph_fallback_total,
        "graph_fallback_rate": fallback_rate,
        "top_fallback_reasons": _top_fallback_reasons(normalized_executions),
    }
    approvals_block = {
        "pending_total": int(max(0, pending_total)),
        "stale_pending_total": stale_pending_total,
        "graph_resume_total": graph_resume_total,
        "graph_resume_success_total": graph_resume_success_total,
        "graph_resume_success_rate": graph_resume_success_rate,
    }
    guardrails_block = {
        **guardrail_metrics,
        "duplicate_tool_execution_backstop_total": step_scan_metrics["duplicate_tool_execution_total"],
        "duplicate_tool_execution_backstop_runs": step_scan_metrics["duplicate_tool_execution_runs"],
        "duplicate_tool_execution_instrumented": True,
    }

    return {
        "scope": {
            "entity_id": str(entity_id or "").strip() or None,
            "cohort_mode": "entity" if str(entity_id or "").strip() else "global_snapshot",
            "window_hours": max(1, int(window_hours or DEFAULT_TODO_GRAPH_PILOT_WINDOW_HOURS)),
            "window_started_at": resolved_window_started_at.isoformat(),
            "window_ended_at": resolved_window_ended_at.isoformat(),
            "default_pilot_entity_id": DEFAULT_TODO_GRAPH_PILOT_ENTITY_ID,
        },
        "thresholds": build_todo_graph_pilot_thresholds(),
        "verdict": _build_verdict(
            entity_id=entity_id,
            executions=executions_block,
            approvals=approvals_block,
            guardrails=guardrails_block,
        ),
        "window": {
            "execution_sample_size": execution_total,
            "approval_sample_size": len(normalized_pending),
            "pending_stale_after_minutes": max(1, int(pending_stale_after_minutes or 30)),
        },
        "executions": executions_block,
        "approvals": approvals_block,
        "guardrails": guardrails_block,
        "value_metrics": value_metrics_block,
    }


__all__ = [
    "DEFAULT_TODO_GRAPH_PILOT_ENTITY_ID",
    "DEFAULT_TODO_GRAPH_PILOT_WINDOW_HOURS",
    "build_todo_graph_pilot_snapshot",
    "build_todo_graph_pilot_thresholds",
]
