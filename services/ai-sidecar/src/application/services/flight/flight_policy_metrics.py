"""Flight policy observability helpers."""

from __future__ import annotations

from collections.abc import Iterable

from src.infrastructure.ai.monitoring.metrics import metrics


def _labels(path: str, mode: str) -> dict[str, str]:
    return {
        "path": str(path or "unknown"),
        "mode": str(mode or "unknown"),
    }


def record_policy_decision(*, path: str, mode: str, allowed: bool) -> None:
    labels = {
        **_labels(path, mode),
        "allowed": "true" if allowed else "false",
    }
    metrics.inc_counter("flight_policy_decision_total", 1, labels)


def record_policy_denied(*, path: str, mode: str, denied_fields: Iterable[str]) -> None:
    denied = [str(field).strip() for field in (denied_fields or []) if str(field).strip()]
    labels = {
        **_labels(path, mode),
        "field_count": str(len(denied)),
    }
    metrics.inc_counter("flight_policy_denied_total", 1, labels)

    for field_name in sorted(set(denied)):
        metrics.inc_counter(
            "flight_policy_denied_field_total",
            1,
            {
                **_labels(path, mode),
                "field": field_name,
            },
        )


__all__ = [
    "record_policy_decision",
    "record_policy_denied",
]
