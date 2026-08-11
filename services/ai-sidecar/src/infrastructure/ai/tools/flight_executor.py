"""航班工具执行器。"""

import re
from typing import TYPE_CHECKING, Any

from pydantic import BaseModel, Field, field_validator

from src.domain.ports.service_interfaces import FlightServiceInterface

from .base import BaseToolExecutor, ToolCategory, ToolExecutionError, ToolExecutionStatus
from .flight_tools import FlightToolName

if TYPE_CHECKING:
    from src.domain.aggregates.flight import FlightAggregate


class GetFlightDetailsArgs(BaseModel):
    flight_id: str = Field(..., min_length=1)


class SearchFlightsByNumberArgs(BaseModel):
    flight_number: str = Field(..., min_length=3)

    @field_validator("flight_number")
    @classmethod
    def validate_flight_number(cls, value: str) -> str:
        cleaned = value.strip().upper()
        if not re.match(r"^[A-Z0-9]{2,3}\d{1,4}$", cleaned):
            raise ValueError(f"无效的航班号格式: {value}")
        return cleaned


class FlightToolExecutor(BaseToolExecutor):
    """将航班工具调用路由到航班服务。"""

    def __init__(self, flight_service: FlightServiceInterface | None = None, default_user: str = "AI_Assistant"):
        super().__init__(default_user)
        self._service = flight_service  # 使用基类的通用服务引用

    def _register_handlers(self) -> None:
        """注册工具处理器"""
        self._handlers = {
            FlightToolName.GET_FLIGHT_DETAILS.value: self._handle_get_flight_details,
            FlightToolName.SEARCH_FLIGHTS_BY_NUMBER.value: self._handle_search_flights_by_number,
        }

    def get_category(self) -> ToolCategory:
        """返回此执行器处理的工具类别"""
        return ToolCategory.FLIGHT

    def set_flight_service(self, flight_service: FlightServiceInterface) -> None:
        """设置航班服务（向后兼容）"""
        self._service = flight_service

    async def _handle_get_flight_details(self, args: dict[str, Any]) -> dict[str, Any]:
        """处理获取航班详情请求。"""
        self._ensure_service()

        validated_args = self._validate_args(GetFlightDetailsArgs, args)

        flight_id = validated_args.flight_id
        aggregate = await self._service.get_flight(flight_id)

        if not aggregate:
            raise ToolExecutionError(f"航班不存在: {flight_id}", ToolExecutionStatus.NOT_FOUND)

        return self._flight_to_dict(aggregate)

    async def _handle_search_flights_by_number(self, args: dict[str, Any]) -> dict[str, Any]:
        """处理按航班号搜索请求。"""
        self._ensure_service()

        validated_args = self._validate_args(SearchFlightsByNumberArgs, args)

        flight_number = validated_args.flight_number
        aggregate = await self._service.find_flight_by_number(flight_number)

        if not aggregate:
            return {"found": False, "flight_number": flight_number, "message": f"未找到航班号为 {flight_number} 的航班"}

        flight_data = self._flight_to_dict(aggregate)
        flight_data["found"] = True
        return flight_data

    def _flight_to_dict(self, aggregate: "FlightAggregate") -> dict[str, Any]:
        """将航班聚合根转换为工具返回字典。"""
        flight = self._unwrap(aggregate, "get_flight")
        inbound_leg = self._serialize_leg(getattr(flight, "inbound_leg", None))
        outbound_leg = self._serialize_leg(getattr(flight, "outbound_leg", None))
        primary_flight_number = self._extract_value(getattr(flight, "flight_number", None))
        if not primary_flight_number:
            primary_flight_number = (outbound_leg or {}).get("flight_no") or (inbound_leg or {}).get("flight_no")

        return {
            "flight_id": self._extract_value(flight.flight_id),
            "flight_number": primary_flight_number,
            "inbound_leg": inbound_leg,
            "outbound_leg": outbound_leg,
            "anomaly_summary": getattr(flight, "anomaly_summary", None)
            or {
                "has_open_anomaly": False,
                "open_count": 0,
                "acknowledged_count": 0,
            },
            "status": self._extract_value(flight.status),
            "gate": self._extract_value(flight.gate),
            "stand": self._extract_value(flight.stand),
            "aircraft_type_detail": self._extract_value(flight.aircraft_type_detail),
            "operation_date": self._derive_operation_date(flight),
            # 时间字段
            "scheduled_arrival_time": self._to_iso(flight.scheduled_arrival),
            "scheduled_departure_time": self._to_iso(flight.scheduled_departure),
            "estimated_arrival_time": self._to_iso(flight.estimated_arrival),
            "estimated_departure_time": self._to_iso(flight.estimated_departure),
            "actual_arrival_time": self._to_iso(flight.actual_arrival),
            "actual_departure_time": self._to_iso(flight.actual_departure),
            # 业务标记
            "is_quick_turnaround": getattr(flight, "is_quick_turnaround", False),
            "boarding_restriction": getattr(flight, "has_boarding_restriction", None),
        }

    @classmethod
    def _serialize_leg(cls, leg: Any) -> dict[str, Any] | None:
        if leg is None:
            return None
        return {
            "leg_type": getattr(leg, "leg_type", None),
            "flight_no": cls._extract_value(getattr(leg, "flight_no", None)),
            "flight_type": getattr(leg, "flight_type", None),
            "mission": getattr(leg, "mission", None),
            "origin_stations": [
                {"code": getattr(station, "code", None), "name": getattr(station, "name", None)}
                for station in getattr(leg, "origin_stations", []) or []
            ],
            "destination_stations": [
                {"code": getattr(station, "code", None), "name": getattr(station, "name", None)}
                for station in getattr(leg, "destination_stations", []) or []
            ],
            "is_vip": bool(getattr(leg, "is_vip", False)),
            "stand_type": getattr(leg, "stand_type", None),
            "scheduled_time": cls._to_iso(getattr(leg, "scheduled_time", None)),
        }

    @staticmethod
    def _stringify_temporal(value: Any) -> str | None:
        if value is None:
            return None
        return value.isoformat() if hasattr(value, "isoformat") else str(value)

    @classmethod
    def _derive_operation_date(cls, flight: Any) -> str | None:
        for attr in ("scheduled_departure", "estimated_departure", "scheduled_arrival", "estimated_arrival"):
            value = getattr(flight, attr, None)
            if value is None:
                continue
            text = cls._stringify_temporal(value)
            if not text:
                continue
            return text.split("T", 1)[0]
        return None


__all__ = [
    "FlightToolExecutor",
]
