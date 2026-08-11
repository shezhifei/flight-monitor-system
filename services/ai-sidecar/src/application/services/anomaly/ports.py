"""Port abstractions and default adapters for anomaly subdomain integrations."""

from __future__ import annotations

from dataclasses import asdict
from typing import Any, Protocol

from src.application.services.todo_priority import normalize_todo_priority
from src.domain.models.anomaly import Anomaly
from src.domain.models.anomaly_rule import AnomalyRule
from src.domain.utils.time_utils import utc_now


class AnomalyFlightReadPort(Protocol):
    async def get_active_flights(self, limit: int = 500) -> list[Any]: ...

    async def get_flight(self, flight_id: str) -> Any | None: ...

    async def search_flights(
        self,
        criteria: dict[str, Any],
        limit: int = 100,
        offset: int = 0,
    ) -> list[Any]: ...


class AnomalyTodoWritePort(Protocol):
    async def create_anomaly_todo(self, anomaly: Anomaly, rule: AnomalyRule) -> str | None: ...


class AnomalyNotifyPort(Protocol):
    async def publish(self, topic: str, payload: dict[str, Any]) -> None: ...


class FlightServiceReadAdapter:
    """Adapter from existing flight query/application services to anomaly read port."""

    def __init__(self, flight_service: Any):
        self._flight_service = flight_service

    async def get_active_flights(self, limit: int = 500) -> list[Any]:
        if not self._flight_service:
            return []
        getter = getattr(self._flight_service, "get_active_flights", None)
        if not callable(getter):
            return []
        return await getter(limit=limit)

    async def get_flight(self, flight_id: str) -> Any | None:
        if not self._flight_service:
            return None
        getter = getattr(self._flight_service, "get_flight", None)
        if not callable(getter):
            return None
        return await getter(flight_id)

    async def search_flights(
        self,
        criteria: dict[str, Any],
        limit: int = 100,
        offset: int = 0,
    ) -> list[Any]:
        if not self._flight_service:
            return []
        searcher = getattr(self._flight_service, "search_flights", None)
        if not callable(searcher):
            return []
        return await searcher(criteria, limit=limit, offset=offset)


class TodoServiceWriteAdapter:
    """Adapter from existing todo app service to anomaly todo write port."""

    def __init__(self, todo_service: Any):
        self._todo_service = todo_service

    async def create_anomaly_todo(self, anomaly: Anomaly, rule: AnomalyRule) -> str | None:
        if not self._todo_service:
            return None

        create_todo = getattr(self._todo_service, "create_todo", None)
        if not callable(create_todo):
            return None

        from src.application.services.async_todo_service import CreateTodoCommand

        priority = normalize_todo_priority(rule.todo_priority, default="高")
        command = CreateTodoCommand(
            title=f"[Anomaly] {anomaly.title}",
            description=anomaly.description,
            priority=priority,
            created_by="AnomalyDetector",
            source_type="anomaly_detection",
            source_id=anomaly.anomaly_id,
        )
        todo_id = await create_todo(command)
        return getattr(todo_id, "value", str(todo_id) if todo_id else None)


class SSEAnomalyNotifyAdapter:
    """Adapter from SSE hub to anomaly notify port."""

    def __init__(self, sse_hub: Any):
        self._sse_hub = sse_hub

    async def publish(self, topic: str, payload: dict[str, Any]) -> None:
        if not self._sse_hub:
            return
        await self._sse_hub.broadcast_to_topic(topic, payload)


def build_anomaly_payload(event_type: str, anomaly: Anomaly) -> dict[str, Any]:
    """Build canonical anomaly realtime payload envelope."""
    return {
        "type": event_type,
        "anomaly": {
            **asdict(anomaly),
            "anomaly_type": anomaly.anomaly_type.value,
            "severity": anomaly.severity.value,
            "status": anomaly.status.value,
            "detected_at": anomaly.detected_at.isoformat() if anomaly.detected_at else None,
            "resolved_at": anomaly.resolved_at.isoformat() if anomaly.resolved_at else None,
            "last_escalated_at": anomaly.last_escalated_at.isoformat() if anomaly.last_escalated_at else None,
            "created_at": anomaly.created_at.isoformat() if anomaly.created_at else None,
            "updated_at": anomaly.updated_at.isoformat() if anomaly.updated_at else None,
        },
        "timestamp": utc_now().isoformat(),
    }


__all__ = [
    "AnomalyFlightReadPort",
    "AnomalyNotifyPort",
    "AnomalyTodoWritePort",
    "FlightServiceReadAdapter",
    "SSEAnomalyNotifyAdapter",
    "TodoServiceWriteAdapter",
    "build_anomaly_payload",
]
