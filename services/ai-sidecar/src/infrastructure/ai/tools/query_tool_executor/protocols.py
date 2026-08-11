"""Protocol definitions for query tool executor dependencies."""

from __future__ import annotations

from datetime import datetime
from typing import Any, Literal, Protocol

QueryScope = Literal["active", "archive", "all"]


class FlightReader(Protocol):
    async def find_flight_by_number(self, flight_number: str) -> object | None: ...


class FlightAIQueryRepositoryReader(Protocol):
    async def search_flights(
        self,
        *,
        criteria: dict[str, Any] | None,
        scope: QueryScope = "active",
        limit: int = 100,
        offset: int = 0,
        sort_by: str = "scheduled_departure",
        sort_order: str = "desc",
    ) -> list[dict[str, Any]]: ...

    async def count_by_status(
        self,
        *,
        criteria: dict[str, Any] | None,
        scope: QueryScope = "active",
    ) -> dict[str, int]: ...

    async def get_turnaround_stats(
        self,
        *,
        criteria: dict[str, Any] | None,
        scope: QueryScope = "active",
    ) -> dict[str, float | None]: ...

    async def find_flight_id_by_number(
        self,
        *,
        flight_number: str,
        scope: QueryScope = "active",
    ) -> str | None: ...

    async def count_departures_by_time_buckets(
        self,
        *,
        criteria: dict[str, Any] | None,
        start_time: datetime,
        end_time: datetime,
        granularity: str = "hour",
        scope: QueryScope = "active",
    ) -> list[dict[str, Any]]: ...


class FlightInsightServiceReader(Protocol):
    async def generate_history_report(
        self,
        flight_id: str,
        hours: int,
        incident_type: str | None,
        user_id: str,
    ) -> dict[str, Any]: ...

    async def generate_event_journey(
        self,
        flight_id: str,
        hours: int,
        user_id: str,
    ) -> dict[str, Any]: ...


class AnomalyRepositoryReader(Protocol):
    async def list_anomalies(
        self,
        *,
        status: str | None = None,
        anomaly_type: str | None = None,
        start_date: datetime | None = None,
        end_date: datetime | None = None,
        limit: int = 200,
        offset: int = 0,
    ) -> list[Any]: ...

    async def find_by_id(self, anomaly_id: str) -> Any | None: ...

    async def get_stats(
        self,
        *,
        start_date: datetime | None = None,
        end_date: datetime | None = None,
    ) -> dict[str, Any]: ...

    async def count_by_time_buckets(
        self,
        *,
        start_date: datetime,
        end_date: datetime,
        granularity: str = "hour",
        status: str | None = None,
        anomaly_type: str | None = None,
        flight_id: str | None = None,
        severity: str | None = None,
    ) -> list[dict[str, Any]]: ...


class TodoServiceReader(Protocol):
    async def get_todo(self, todo_id: Any) -> Any | None: ...

    async def list_todos(self, options: Any) -> list[Any]: ...

    async def search_todos(self, query: str, options: Any) -> list[Any]: ...

    async def get_todo_stats(self, criteria: dict[str, Any]) -> Any: ...

    async def count_by_time_buckets(
        self,
        *,
        start_time: datetime,
        end_time: datetime,
        granularity: str = "hour",
        filters: dict[str, Any] | None = None,
    ) -> list[dict[str, Any]]: ...
