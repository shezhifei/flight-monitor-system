"""Unified command entrypoint for flight write operations."""

from __future__ import annotations

from dataclasses import dataclass, field
from time import perf_counter
from typing import Any

from src.application.mappers.flight_mapper import mapper as flight_mapper
from src.application.services.flight.flight_command_context import FlightCommandContext
from src.application.services.flight.flight_policy_metrics import record_policy_decision, record_policy_denied
from src.domain.exceptions.business import BusinessRuleException
from src.domain.exceptions.validation import ValidationException
from src.infrastructure.logging.core import get_logger

logger = get_logger(__name__)


class FlightCommandGatewayValidationError(ValueError):
    """Raised when write payloads or command inputs are invalid."""


class FlightCommandGatewayPermissionError(PermissionError):
    """Raised when command execution is denied by write policy."""


class FlightCommandGatewayNotFoundError(LookupError):
    """Raised when a target flight is missing for a write command."""


@dataclass(frozen=True)
class FlightCreateCommandRequest:
    payload: Any
    command_context: FlightCommandContext
    created_by: str
    profile: bool = False


@dataclass(frozen=True)
class FlightUpdateCommandRequest:
    flight_id: str
    payload: Any
    command_context: FlightCommandContext
    updated_by: str
    enforce_permission_policy: bool = True


@dataclass(frozen=True)
class FlightCommandExecutionResult:
    flight: Any
    changed_fields: tuple[str, ...] = ()
    timings_ms: dict[str, float] = field(default_factory=dict)


class FlightCommandGateway:
    """Application-layer command gateway for flight writes.

    Owns the route-facing lifecycle: payload normalization, policy checks,
    command dispatch, post-write lookup, and timing capture.
    """

    def __init__(self, *, async_flight_service: Any, flight_query_service: Any):
        self._async_flight_service = async_flight_service
        self._flight_query_service = flight_query_service

    @staticmethod
    def _mark(timings: dict[str, float], name: str, started_at: float) -> None:
        timings[name] = round((perf_counter() - started_at) * 1000, 2)

    @staticmethod
    def _merge_write_profile(timings: dict[str, float], aggregate: Any) -> None:
        write_side_effect_profile = getattr(aggregate, "_write_side_effect_profile", None)
        if isinstance(write_side_effect_profile, dict):
            timings.update(
                {
                    key: float(value)
                    for key, value in write_side_effect_profile.items()
                    if isinstance(value, (int, float))
                }
            )

    async def execute_create(self, request: FlightCreateCommandRequest) -> FlightCommandExecutionResult:
        total_start = perf_counter()
        timings: dict[str, float] = {}

        if not request.payload:
            raise FlightCommandGatewayValidationError("请求数据不能为空")

        map_started = perf_counter()
        try:
            domain_flight_data = flight_mapper.api_to_domain_create(request.payload)
        except ValidationException as exc:
            logger.warning(f"Flight create payload validation failed: {exc}")
            raise FlightCommandGatewayValidationError("数据验证失败") from exc
        except Exception as exc:
            logger.error(f"Flight create payload mapping failed: {exc}", exc_info=True)
            raise FlightCommandGatewayValidationError("数据转换失败") from exc
        self._mark(timings, "map_ms", map_started)

        service_started = perf_counter()
        try:
            created_aggregate = await self._async_flight_service.create_flight(
                inbound_leg=domain_flight_data.get("inbound_leg"),
                outbound_leg=domain_flight_data.get("outbound_leg"),
                scheduled_arrival_time=domain_flight_data.get("scheduled_arrival"),
                scheduled_departure_time=domain_flight_data.get("scheduled_departure"),
                status=domain_flight_data.get("status"),
                aircraft_type_detail=domain_flight_data.get("aircraft_type_detail"),
                registration=domain_flight_data.get("registration"),
                airline_code=domain_flight_data.get("airline_code"),
                flight_number=domain_flight_data.get("flight_number"),
                estimated_departure=domain_flight_data.get("estimated_departure"),
                estimated_arrival=domain_flight_data.get("estimated_arrival"),
                stand=domain_flight_data.get("stand"),
                gate=domain_flight_data.get("gate"),
                terminal=domain_flight_data.get("terminal"),
                position=domain_flight_data.get("position"),
                baggage_carousel=domain_flight_data.get("baggage_carousel"),
                is_quick_turnaround=domain_flight_data.get("is_quick_turnaround"),
                has_boarding_restriction=domain_flight_data.get("has_boarding_restriction", False),
                is_commercial_signed=domain_flight_data.get("is_commercial_signed", True),
                actual_departure=domain_flight_data.get("actual_departure"),
                actual_arrival=domain_flight_data.get("actual_arrival"),
                flight_remarks=domain_flight_data.get("flight_remarks"),
                load_planning_remarks=domain_flight_data.get("load_planning_remarks"),
                aircraft_maintenance_remarks=domain_flight_data.get("aircraft_maintenance_remarks"),
                aircraft_check_remarks=domain_flight_data.get("aircraft_check_remarks"),
                created_by=request.created_by,
            )
        except ValueError as exc:
            raise FlightCommandGatewayValidationError(str(exc)) from exc
        self._mark(timings, "service_create_ms", service_started)

        self._merge_write_profile(timings, created_aggregate)

        lookup_started = perf_counter()
        new_flight = getattr(created_aggregate, "flight", None)
        flight_id_val = getattr(new_flight, "flight_id", None)
        if new_flight is None and self._flight_query_service is not None and flight_id_val is not None:
            new_flight = await self._flight_query_service.find_by_flight_id(flight_id_val.value)
        if not new_flight:
            raise RuntimeError("航班创建后获取失败")
        self._mark(timings, "post_lookup_ms", lookup_started)

        timings["total_ms"] = round((perf_counter() - total_start) * 1000, 2)
        return FlightCommandExecutionResult(
            flight=new_flight,
            changed_fields=("create",),
            timings_ms=timings,
        )

    async def execute_update(self, request: FlightUpdateCommandRequest) -> FlightCommandExecutionResult:
        flight = await self._flight_query_service.find_by_flight_id(request.flight_id)
        if not flight:
            raise FlightCommandGatewayNotFoundError("航班未找到")
        if not request.payload:
            raise FlightCommandGatewayValidationError("No data provided")

        try:
            domain_flight_data = flight_mapper.api_to_domain_update(request.payload)
        except ValidationException as exc:
            logger.warning(f"Flight update payload validation failed: {exc}")
            raise FlightCommandGatewayValidationError("数据验证失败") from exc
        except Exception as exc:
            logger.error(f"Flight update payload mapping failed: {exc}", exc_info=True)
            raise FlightCommandGatewayValidationError("数据转换失败") from exc

        if not domain_flight_data:
            raise FlightCommandGatewayValidationError("未提供任何更新字段")

        try:
            policy_forbidden_fields = self._async_flight_service.evaluate_write_denied_fields(
                fields=domain_flight_data,
                command_context=request.command_context,
            )
        except ValueError as exc:
            raise FlightCommandGatewayValidationError(str(exc)) from exc

        allowed = not policy_forbidden_fields
        record_policy_decision(path="api.flight.update", mode="policy_v2", allowed=allowed)
        if policy_forbidden_fields:
            record_policy_denied(
                path="api.flight.update",
                mode="policy_v2",
                denied_fields=policy_forbidden_fields,
            )
            fields_str = ", ".join(sorted(policy_forbidden_fields))
            raise FlightCommandGatewayPermissionError(f"权限不足：非管理员禁止修改外部同步受控字段: [{fields_str}]")

        try:
            success, changed_fields = await self._async_flight_service.update_flight_fields(
                flight_id=flight.flight_id.value,
                fields=domain_flight_data,
                updated_by=request.updated_by,
                command_context=request.command_context,
                enforce_permission_policy=request.enforce_permission_policy,
            )
        except BusinessRuleException as exc:
            raise FlightCommandGatewayPermissionError(getattr(exc, "message", str(exc))) from exc
        except ValueError as exc:
            raise FlightCommandGatewayValidationError(str(exc)) from exc

        if not success:
            raise RuntimeError("航班更新失败")

        updated_flight = await self._flight_query_service.find_by_flight_id(flight.flight_id.value)
        if not updated_flight:
            raise RuntimeError("航班更新后获取失败")

        return FlightCommandExecutionResult(
            flight=updated_flight,
            changed_fields=tuple(changed_fields or []),
            timings_ms={},
        )
