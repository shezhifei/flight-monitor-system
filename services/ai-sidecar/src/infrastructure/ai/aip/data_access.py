"""
AIP 数据访问层

提供从实际数据源获取 Ontology 对象状态的能力。
支持从数据库、服务层获取对象数据。

使用方式:
    from src.infrastructure.ai.aip.data_access import ObjectDataAccessor

    accessor = ObjectDataAccessor()
    flight_state = await accessor.get_object_state("Flight", "CA1234_20240101")
"""

from __future__ import annotations

from typing import Any

from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


class ObjectDataAccessor:
    """
    对象数据访问器

    提供统一的接口从各种数据源获取对象状态。
    支持注入不同的数据访问实现。
    """

    def __init__(self):
        self._repositories: dict[str, Any] = {}
        self._initialized = False

    def register_repository(self, object_type: str, repository: Any) -> None:
        """注册对象仓储"""
        self._repositories[object_type] = repository
        logger.debug(f"Registered repository for {object_type}")

    async def get_object_state(self, object_type: str, object_id: str) -> dict[str, Any] | None:
        """
        获取对象状态

        Args:
            object_type: 对象类型 (Flight, Stand, Team, etc.)
            object_id: 对象ID

        Returns:
            对象状态字典，如果未找到则返回 None

        Raises:
            Exception: 数据库错误会向上传播
        """
        if object_type == "Flight":
            return await self._get_flight_state(object_id)
        elif object_type == "Stand":
            return await self._get_stand_state(object_id)
        elif object_type == "Team":
            return await self._get_team_state(object_id)
        elif object_type == "Equipment":
            return await self._get_equipment_state(object_id)
        elif object_type == "Anomaly":
            return await self._get_anomaly_state(object_id)
        elif object_type == "Todo":
            return await self._get_todo_state(object_id)
        else:
            logger.warning(f"Unknown object type: {object_type}")
            return None

    async def get_objects_by_query(self, object_type: str, filters: dict[str, Any]) -> list[dict[str, Any]]:
        """
        根据查询条件获取对象列表

        Args:
            object_type: 对象类型
            filters: 查询过滤器

        Returns:
            匹配的对象列表

        Raises:
            Exception: 数据库错误会向上传播
        """
        if object_type == "Flight":
            return await self._query_flights(filters)
        elif object_type == "Stand":
            return await self._query_stands(filters)
        elif object_type == "Team":
            return await self._query_teams(filters)
        else:
            return []

    async def _get_flight_state(self, flight_id: str) -> dict[str, Any] | None:
        """获取航班状态"""
        from src.application.services.flight.flight_query_service import FlightQueryService

        service = FlightQueryService()
        flight = await service.get_flight_by_id(flight_id)

        if not flight:
            return None

        return {
            "flight_id": flight.id,
            "flight_number": flight.flight_number,
            "flight_type": getattr(flight, "flight_type", "departure"),
            "status": getattr(flight, "status", "scheduled"),
            "stand": getattr(flight, "stand", None),
            "gate": getattr(flight, "gate", None),
            "aircraft_type": getattr(flight, "aircraft_type", None),
            "origin": getattr(flight, "origin", None),
            "destination": getattr(flight, "destination", None),
            "scheduled_departure": self._format_datetime(getattr(flight, "scheduled_departure", None)),
            "scheduled_arrival": self._format_datetime(getattr(flight, "scheduled_arrival", None)),
            "actual_departure": self._format_datetime(getattr(flight, "actual_departure", None)),
            "actual_arrival": self._format_datetime(getattr(flight, "actual_arrival", None)),
            "delay_minutes": getattr(flight, "delay_minutes", 0),
            "assigned_team_id": getattr(flight, "assigned_team_id", None),
        }

    async def _get_stand_state(self, stand_id: str) -> dict[str, Any] | None:
        """获取机位状态"""
        from src.application.services.dispatch.dispatch_query_service import DispatchQueryService

        service = DispatchQueryService()
        stand = await service.get_stand_by_id(stand_id)

        if not stand:
            return None

        return {
            "stand_id": stand.id,
            "stand_code": getattr(stand, "code", stand_id),
            "terminal": getattr(stand, "terminal", None),
            "zone": getattr(stand, "zone", None),
            "status": getattr(stand, "status", "available"),
            "size": getattr(stand, "size", "medium"),
            "max_wingspan": getattr(stand, "max_wingspan", None),
            "has_bridge": getattr(stand, "has_bridge", False),
            "current_flight_id": getattr(stand, "current_flight_id", None),
        }

    async def _get_team_state(self, team_id: str) -> dict[str, Any] | None:
        """获取班组状态"""
        from src.application.services.dispatch.dispatch_query_service import DispatchQueryService

        service = DispatchQueryService()
        team = await service.get_team_by_id(team_id)

        if not team:
            return None

        return {
            "team_id": team.id,
            "team_name": getattr(team, "name", team_id),
            "team_type": getattr(team, "team_type", "ground"),
            "status": getattr(team, "status", "off_duty"),
            "location": getattr(team, "location", None),
            "member_count": getattr(team, "member_count", 0),
            "shift_start": self._format_datetime(getattr(team, "shift_start", None)),
            "shift_end": self._format_datetime(getattr(team, "shift_end", None)),
        }

    async def _get_equipment_state(self, equipment_id: str) -> dict[str, Any] | None:
        """获取设备状态"""
        # Equipment data source not implemented yet
        # Return None instead of fabricated stub data
        return None

    async def _get_anomaly_state(self, anomaly_id: str) -> dict[str, Any] | None:
        """获取异常状态"""
        from src.application.services.anomaly.anomaly_query_service import AnomalyQueryService

        service = AnomalyQueryService()
        anomaly = await service.get_anomaly_by_id(anomaly_id)

        if not anomaly:
            return None

        return {
            "anomaly_id": anomaly.id,
            "anomaly_type": getattr(anomaly, "anomaly_type", "unknown"),
            "severity": getattr(anomaly, "severity", "medium"),
            "status": getattr(anomaly, "status", "detected"),
            "title": getattr(anomaly, "title", ""),
            "description": getattr(anomaly, "description", ""),
            "detected_at": self._format_datetime(getattr(anomaly, "detected_at", None)),
            "flight_id": getattr(anomaly, "flight_id", None),
            "stand_id": getattr(anomaly, "stand_id", None),
            "team_id": getattr(anomaly, "team_id", None),
        }

    async def _get_todo_state(self, todo_id: str) -> dict[str, Any] | None:
        """获取待办状态"""
        from src.application.services.async_todo_service import AsyncTodoService

        service = AsyncTodoService()
        todo = await service.get_todo_by_id(todo_id)

        if not todo:
            return None

        return {
            "todo_id": todo.id if hasattr(todo, "id") else todo_id,
            "title": getattr(todo, "title", ""),
            "description": getattr(todo, "description", ""),
            "status": getattr(todo, "status", "pending"),
            "priority": getattr(todo, "priority", "medium"),
            "assignee_id": getattr(todo, "assignee_id", None),
            "created_at": self._format_datetime(getattr(todo, "created_at", None)),
            "due_date": self._format_datetime(getattr(todo, "due_date", None)),
            "flight_id": getattr(todo, "flight_id", None),
        }

    async def _query_flights(self, filters: dict[str, Any]) -> list[dict[str, Any]]:
        """查询航班列表"""
        from src.application.services.flight.flight_query_service import FlightQueryService

        service = FlightQueryService()
        flights = await service.query_flights(filters)

        return [
            {
                "flight_id": f.id,
                "flight_number": f.flight_number,
                "status": getattr(f, "status", "scheduled"),
                "stand": getattr(f, "stand", None),
            }
            for f in flights
        ]

    async def _query_stands(self, filters: dict[str, Any]) -> list[dict[str, Any]]:
        """查询机位列表"""
        from src.application.services.dispatch.dispatch_query_service import DispatchQueryService

        service = DispatchQueryService()
        stands = await service.query_stands(filters)

        return [
            {
                "stand_id": s.id,
                "stand_code": getattr(s, "code", s.id),
                "status": getattr(s, "status", "available"),
                "size": getattr(s, "size", "medium"),
            }
            for s in stands
        ]

    async def _query_teams(self, filters: dict[str, Any]) -> list[dict[str, Any]]:
        """查询班组列表"""
        from src.application.services.dispatch.dispatch_query_service import DispatchQueryService

        service = DispatchQueryService()
        teams = await service.query_teams(filters)

        return [
            {
                "team_id": t.id,
                "team_name": getattr(t, "name", t.id),
                "status": getattr(t, "status", "off_duty"),
            }
            for t in teams
        ]

    @staticmethod
    def _format_datetime(dt: Any) -> str | None:
        """格式化日期时间"""
        if dt is None:
            return None
        if hasattr(dt, "isoformat"):
            return dt.isoformat()
        return str(dt)

    @staticmethod
    def _stub_flight_state(flight_id: str) -> dict[str, Any]:
        """航班状态存根"""
        return {
            "flight_id": flight_id,
            "flight_number": flight_id.split("_")[0] if "_" in flight_id else flight_id,
            "flight_type": "departure",
            "status": "scheduled",
            "stand": None,
            "gate": None,
            "aircraft_type": "B737",
            "origin": "PEK",
            "destination": "SHA",
            "scheduled_departure": None,
            "scheduled_arrival": None,
            "delay_minutes": 0,
            "assigned_team_id": None,
        }

    @staticmethod
    def _stub_stand_state(stand_id: str) -> dict[str, Any]:
        """机位状态存根"""
        return {
            "stand_id": stand_id,
            "stand_code": stand_id,
            "terminal": "T1",
            "zone": "North",
            "status": "available",
            "size": "medium",
            "max_wingspan": 36.0,
            "has_bridge": True,
            "current_flight_id": None,
        }

    @staticmethod
    def _stub_team_state(team_id: str) -> dict[str, Any]:
        """班组状态存根"""
        return {
            "team_id": team_id,
            "team_name": team_id,
            "team_type": "ground",
            "status": "on_duty",
            "location": "Office A",
            "member_count": 4,
            "shift_start": None,
            "shift_end": None,
        }

    @staticmethod
    def _stub_equipment_state(equipment_id: str) -> dict[str, Any]:
        """设备状态存根"""
        return {
            "equipment_id": equipment_id,
            "equipment_code": equipment_id,
            "equipment_type": "pushback_tractor",
            "status": "available",
            "location": "Equipment Yard",
            "fuel_level": 80,
        }

    @staticmethod
    def _stub_anomaly_state(anomaly_id: str) -> dict[str, Any]:
        """异常状态存根"""
        return {
            "anomaly_id": anomaly_id,
            "anomaly_type": "gate_stand_conflict",
            "severity": "medium",
            "status": "detected",
            "title": "Anomaly",
            "description": "",
            "detected_at": None,
            "flight_id": None,
            "stand_id": None,
            "team_id": None,
        }

    @staticmethod
    def _stub_todo_state(todo_id: str) -> dict[str, Any]:
        """待办状态存根"""
        return {
            "todo_id": todo_id,
            "title": "Task",
            "description": "",
            "status": "pending",
            "priority": "medium",
            "assignee_id": None,
            "created_at": None,
            "due_date": None,
            "flight_id": None,
        }


_accessor: ObjectDataAccessor | None = None


def get_object_accessor() -> ObjectDataAccessor:
    """获取全局对象访问器"""
    global _accessor
    if _accessor is None:
        _accessor = ObjectDataAccessor()
    return _accessor


def set_object_accessor(accessor: ObjectDataAccessor) -> None:
    """设置全局对象访问器"""
    global _accessor
    _accessor = accessor


__all__ = [
    "ObjectDataAccessor",
    "get_object_accessor",
    "set_object_accessor",
]
