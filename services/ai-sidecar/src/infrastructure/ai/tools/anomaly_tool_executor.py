"""Anomaly tool executor."""

from typing import Any

from .anomaly_tools import AnomalyToolName
from .base import BaseToolExecutor, ToolCategory, ToolExecutionError, ToolExecutionStatus


class AnomalyToolExecutor(BaseToolExecutor):
    """Route anomaly tool calls to anomaly repository."""

    def __init__(self, anomaly_repository: Any = None, default_user: str = "AI_Assistant"):
        super().__init__(default_user)
        self._service = anomaly_repository

    def _register_handlers(self) -> None:
        self._handlers = {
            AnomalyToolName.LIST_ANOMALIES.value: self._handle_list_anomalies,
            AnomalyToolName.GET_ANOMALY_DETAIL.value: self._handle_get_anomaly_detail,
            AnomalyToolName.GET_ANOMALY_STATS.value: self._handle_get_anomaly_stats,
        }

    def get_category(self) -> ToolCategory:
        return ToolCategory.ANOMALY

    async def _handle_list_anomalies(self, args: dict[str, Any]) -> dict[str, Any]:
        self._ensure_service()
        limit = int(args.get("limit", 20) or 20)
        items = await self._service.list_anomalies(
            status=args.get("status"),
            anomaly_type=args.get("anomaly_type"),
            limit=max(1, min(limit, 200)),
            offset=0,
        )
        return {
            "total": len(items),
            "items": [self._anomaly_to_dict(item) for item in items],
        }

    async def _handle_get_anomaly_detail(self, args: dict[str, Any]) -> dict[str, Any]:
        self._ensure_service()
        anomaly_id = self._require_arg(args, "anomaly_id")
        item = await self._service.find_by_id(anomaly_id)
        if not item:
            raise ToolExecutionError(
                f"Anomaly not found: {anomaly_id}",
                ToolExecutionStatus.NOT_FOUND,
            )
        return self._anomaly_to_dict(item)

    async def _handle_get_anomaly_stats(self, args: dict[str, Any]) -> dict[str, Any]:
        self._ensure_service()
        return await self._service.get_stats()

    @staticmethod
    def _anomaly_to_dict(item: Any) -> dict[str, Any]:
        return {
            "anomaly_id": item.anomaly_id,
            "flight_id": item.flight_id,
            "anomaly_type": item.anomaly_type.value,
            "severity": item.severity.value,
            "status": item.status.value,
            "title": item.title,
            "description": item.description,
            "detected_at": item.detected_at.isoformat() if item.detected_at else None,
            "resolved_at": item.resolved_at.isoformat() if item.resolved_at else None,
            "escalation_level": item.escalation_level,
            "linked_todo_id": item.linked_todo_id,
            "rule_id": item.rule_id,
            "context_data": item.context_data or {},
        }
