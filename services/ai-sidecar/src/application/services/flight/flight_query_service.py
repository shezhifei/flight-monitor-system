"""Read-side flight query service.

This service owns query concerns for flights so command services can stay
focused on state transitions.
"""

from __future__ import annotations

from datetime import date, datetime
from typing import Any

from src.domain.models.flight import FlightStatus
from src.domain.utils.time_utils import utc_now
from src.infrastructure.logging.core import get_logger
from src.infrastructure.repositories.flight_repository import AsyncFlightRepository

logger = get_logger(__name__)


class FlightQueryService:
    """Query-only service for flight read models."""

    def __init__(self, flight_repository: AsyncFlightRepository):
        self._flight_repository = flight_repository
        self._terminal_statuses = {
            FlightStatus.DEPARTED,
            FlightStatus.NEXT_ARRIVED,
            FlightStatus.CANCELLED,
        }

    async def get_flight(self, flight_id: str) -> Any | None:
        return await self._flight_repository.find_by_id(flight_id)

    async def get_flight_by_id(self, flight_id: str) -> Any | None:
        return await self.get_flight(flight_id)

    async def find_by_flight_id(self, flight_id: str) -> Any | None:
        aggregate = await self._flight_repository.find_by_id(flight_id)
        return self._extract_flight(aggregate) if aggregate else None

    async def get_all_flights(self, limit: int = 100, offset: int = 0) -> list[Any]:
        safe_limit = max(1, int(limit or 1))
        safe_offset = max(0, int(offset or 0))
        return await self._flight_repository.find_all(limit=safe_limit, offset=safe_offset)

    async def get_flights_count(self) -> int:
        try:
            return int(await self._flight_repository.count_all())
        except Exception as exc:  # noqa: BLE001 - repository count may fail in various ways
            logger.warning(f"count_all() failed, falling back to find_all: {exc}")

        total = 0
        batch_size = 2000
        offset = 0
        while True:
            page = await self._flight_repository.find_all(limit=batch_size, offset=offset)
            if not page:
                break
            total += len(page)
            if len(page) < batch_size:
                break
            offset += batch_size
        return total

    async def get_active_flights(self, limit: int = 100) -> list[Any]:
        normalized_limit = max(int(limit or 1), 1)
        batch_size = max(normalized_limit * 2, 100)

        active_flights: list[Any] = []
        offset = 0
        scanned_batches = 0
        max_scan_batches = 20

        while len(active_flights) < normalized_limit and scanned_batches < max_scan_batches:
            page = await self._flight_repository.find_all(limit=batch_size, offset=offset)
            if not page:
                break

            for item in page:
                if self._is_active_flight(item):
                    active_flights.append(item)
                    if len(active_flights) >= normalized_limit:
                        break

            if len(page) < batch_size:
                break

            offset += batch_size
            scanned_batches += 1

        return active_flights[:normalized_limit]

    async def search_flights(
        self,
        criteria: dict[str, Any] | None = None,
        limit: int = 100,
        offset: int = 0,
        **kwargs: Any,
    ) -> list[Any]:
        merged_criteria = dict(criteria or {})
        if kwargs:
            merged_criteria.update(kwargs)

        if "flight_number" in merged_criteria and "flight_no" not in merged_criteria:
            merged_criteria["flight_no"] = merged_criteria.get("flight_number")
        if "stand_id" in merged_criteria and "stand" not in merged_criteria:
            merged_criteria["stand"] = merged_criteria.get("stand_id")
        if "gate_id" in merged_criteria and "gate" not in merged_criteria:
            merged_criteria["gate"] = merged_criteria.get("gate_id")

        safe_limit = max(1, int(limit or 1))
        safe_offset = max(0, int(offset or 0))
        return await self._flight_repository.search(
            criteria=merged_criteria,
            limit=safe_limit,
            offset=safe_offset,
        )

    async def find_flight_by_number(self, flight_number: str) -> Any | None:
        normalized_number = str(flight_number or "").strip().upper()
        if not normalized_number:
            return None

        candidates = await self.search_flights({"flight_no": normalized_number}, limit=100, offset=0)
        if not candidates:
            return None

        for candidate in candidates:
            flight = self._extract_flight(candidate)
            if self._match_flight_number(flight, normalized_number):
                return candidate

        return candidates[0]

    async def find_by_leg_natural_key(
        self,
        *,
        leg_type: str,
        flight_no: str,
        operation_date: date,
        limit: int = 5,
    ) -> list[Any]:
        return await self._flight_repository.find_by_leg_natural_key(
            leg_type=leg_type,
            flight_no=str(flight_no or "").strip().upper(),
            operation_date=operation_date,
            limit=max(1, int(limit or 1)),
        )

    async def batch_get_business_cases(self, flight_ids: list[str]) -> dict[str, list[Any]]:
        if not flight_ids:
            return {}

        cases = await self._flight_repository.find_business_cases_by_flight_ids(flight_ids)
        result: dict[str, list[Any]] = {flight_id: [] for flight_id in flight_ids}
        for case in cases:
            case_flight_id = getattr(case, "flight_id", None)
            if case_flight_id in result:
                result[case_flight_id].append(case)
        return result

    async def batch_get_latest_dispatch_timeline_snapshot(self, flight_ids: list[str]) -> dict[str, dict[str, Any]]:
        normalized_ids = [str(item).strip() for item in dict.fromkeys(flight_ids) if str(item).strip()]
        if not normalized_ids:
            return {}

        finder = getattr(self._flight_repository, "find_latest_dispatch_timeline_snapshot", None)
        if callable(finder):
            return await finder(normalized_ids)

        result: dict[str, dict[str, Any]] = {flight_id: {} for flight_id in normalized_ids}
        getter = getattr(self._flight_repository, "get_dispatch_timeline_events", None)
        if not callable(getter):
            return result

        for flight_id in normalized_ids:
            items = await getter(flight_id)
            latest_by_milestone: dict[str, Any] = {}
            for item in items:
                milestone_code = str(item.get("milestone_code") or "").strip()
                if milestone_code and milestone_code not in latest_by_milestone:
                    latest_by_milestone[milestone_code] = item.get("occurred_at")
            result[flight_id] = latest_by_milestone
        return result

    async def batch_get_gate_map(self, flight_ids: list[str]) -> dict[str, str | None]:
        normalized_ids = [str(item).strip() for item in dict.fromkeys(flight_ids) if str(item).strip()]
        if not normalized_ids:
            return {}

        finder = getattr(self._flight_repository, "find_gate_map", None)
        if callable(finder):
            return await finder(normalized_ids)

        result: dict[str, str | None] = {flight_id: None for flight_id in normalized_ids}
        for flight_id in normalized_ids:
            aggregate = await self._flight_repository.find_by_id(flight_id)
            flight = self._extract_flight(aggregate) if aggregate else None
            result[flight_id] = getattr(flight, "gate", None) if flight is not None else None
        return result

    def _is_active_flight(self, item: Any) -> bool:
        flight = self._extract_flight(item)
        status_raw = getattr(flight, "status", None)
        status = FlightStatus.from_any(status_raw)

        if status is None:
            return True
        if status not in self._terminal_statuses:
            return True
        return self._is_today_flight(flight)

    @staticmethod
    def _extract_flight(item: Any) -> Any:
        if hasattr(item, "get_flight") and callable(item.get_flight):
            try:
                return item.get_flight()
            except Exception as exc:  # noqa: BLE001 - get_flight() may fail in various ways
                logger.warning("get_flight() call failed; falling back to raw item: %s", exc)
                return item
        return getattr(item, "flight", item)

    @staticmethod
    def _is_today_flight(flight: Any) -> bool:
        today = utc_now().date()

        for attr in (
            "scheduled_departure",
            "scheduled_arrival",
            "estimated_departure",
            "estimated_arrival",
            "actual_departure",
            "actual_arrival",
        ):
            value = getattr(flight, attr, None)
            if isinstance(value, datetime) and value.date() == today:
                return True

        return False

    @staticmethod
    def _match_flight_number(flight: Any, normalized_number: str) -> bool:
        for leg_attr in ("outbound_leg", "inbound_leg"):
            leg = getattr(flight, leg_attr, None)
            if leg is None:
                continue
            leg_no = getattr(leg, "flight_no", None)
            if leg_no and str(leg_no).strip().upper() == normalized_number:
                return True

        for attr in ("flight_number",):
            value = getattr(flight, attr, None)
            if value is None:
                continue
            raw = value.value if hasattr(value, "value") else value
            if str(raw).strip().upper() == normalized_number:
                return True
        return False
