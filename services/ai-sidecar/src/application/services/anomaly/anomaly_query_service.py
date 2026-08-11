"""Application-layer query/command facade for anomaly v2 routes.

Isolates Route from direct AnomalyRepository access per North Star §5.1.
"""

from __future__ import annotations

from datetime import datetime
from typing import Any

from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


class AnomalyQueryService:
    """Thin service that wraps the anomaly repository for route consumption."""

    def __init__(self, anomaly_repo: Any) -> None:
        self._repo = anomaly_repo

    # --- Anomaly CRUD ---

    async def list_anomalies(
        self,
        *,
        status: str | None = None,
        anomaly_type: str | None = None,
        start_date: datetime | None = None,
        end_date: datetime | None = None,
        limit: int = 100,
        offset: int = 0,
    ) -> list[Any]:
        return await self._repo.list_anomalies(
            status=status,
            anomaly_type=anomaly_type,
            start_date=start_date,
            end_date=end_date,
            limit=limit,
            offset=offset,
        )

    async def find_by_id(self, anomaly_id: str) -> Any | None:
        return await self._repo.find_by_id(anomaly_id)

    async def acknowledge(self, anomaly_id: str) -> bool:
        return await self._repo.acknowledge(anomaly_id)

    async def resolve(self, anomaly_id: str) -> bool:
        return await self._repo.resolve(anomaly_id)

    async def get_stats(
        self,
        *,
        start_date: datetime | None = None,
        end_date: datetime | None = None,
    ) -> dict[str, Any]:
        return await self._repo.get_stats(start_date=start_date, end_date=end_date)

    # --- Rule CRUD ---

    async def list_rules(self, *, enabled_only: bool = False) -> list[Any]:
        return await self._repo.list_rules(enabled_only=enabled_only)

    async def get_rule(self, rule_id: str) -> Any | None:
        return await self._repo.get_rule(rule_id)

    async def upsert_rule(self, rule: Any) -> Any:
        return await self._repo.upsert_rule(rule)

    async def create_rule_from_payload(self, payload: Any) -> Any:
        from src.domain.models.anomaly import AnomalySeverity
        from src.domain.models.anomaly_rule import AnomalyRule

        severity = AnomalySeverity(payload.severity)
        rule = AnomalyRule(
            rule_id=payload.rule_id,
            rule_type=payload.rule_type,
            name=payload.name,
            enabled=payload.enabled,
            config=payload.config,
            severity=severity,
            auto_create_todo=payload.auto_create_todo,
            todo_priority=payload.todo_priority,
            escalation_intervals=payload.escalation_intervals,
        )
        return await self._repo.upsert_rule(rule)

    async def update_rule_from_payload(self, rule_id: str, payload: Any) -> Any:
        from src.domain.models.anomaly import AnomalySeverity

        existing = await self._repo.get_rule(rule_id)
        if not existing:
            return None

        if payload.enabled is not None:
            existing.enabled = payload.enabled
        if payload.config is not None:
            existing.config = payload.config
        if payload.severity is not None:
            existing.severity = AnomalySeverity(payload.severity)
        if payload.auto_create_todo is not None:
            existing.auto_create_todo = payload.auto_create_todo
        if payload.todo_priority is not None:
            existing.todo_priority = payload.todo_priority
        if payload.escalation_intervals is not None:
            existing.escalation_intervals = payload.escalation_intervals

        return await self._repo.upsert_rule(existing)
