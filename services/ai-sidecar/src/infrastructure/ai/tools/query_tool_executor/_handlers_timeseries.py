"""Query tool executor for natural-language flight queries."""

from __future__ import annotations

import logging
from datetime import datetime
from typing import Any

logger = logging.getLogger(__name__)


class _HandlersTimeseriesMixin:
    """QueryToolExecutor mixin."""

    async def _query_alert_timeseries_series(
        self,
        *,
        filters: dict[str, Any],
        start_date: datetime,
        end_date: datetime,
        granularity: str,
        scan_limit: int,
    ) -> tuple[list[dict[str, Any]], int, str, bool]:
        self._ensure_anomaly_repository()

        bucket_reader = getattr(self._anomaly_repository, "count_by_time_buckets", None)
        if callable(bucket_reader):
            rows = await bucket_reader(
                start_date=start_date,
                end_date=end_date,
                granularity=granularity,
                status=self._first_non_empty(filters, "status"),
                anomaly_type=self._first_non_empty(filters, "anomaly_type", "type"),
                flight_id=self._first_non_empty(filters, "flight_id"),
                severity=self._first_non_empty(filters, "severity"),
            )
            series = self._build_timeseries_series_from_bucket_rows(
                rows=rows,
                start_date=start_date,
                end_date=end_date,
                granularity=granularity,
            )
            return series, self._sum_series_counts(series), "db_aggregate", False

        safe_scan_limit = max(200, min(max(scan_limit, 1), 500))
        items, _ = await self._query_alert_items(
            filters=filters,
            limit=safe_scan_limit,
            time_from=start_date.isoformat(),
            time_to=end_date.isoformat(),
        )
        series = self._build_timeseries_series_from_items(
            items=items,
            timestamp_fields=("detected_at", "resolved_at"),
            start_date=start_date,
            end_date=end_date,
            granularity=granularity,
        )
        partial = len(items) >= safe_scan_limit
        return series, len(items), "item_scan", partial

    async def _query_task_timeseries_series(
        self,
        *,
        filters: dict[str, Any],
        start_date: datetime,
        end_date: datetime,
        granularity: str,
        scan_limit: int,
    ) -> tuple[list[dict[str, Any]], int, str, bool]:
        self._ensure_todo_service()

        bucket_reader = getattr(self._todo_service, "count_by_time_buckets", None)
        if callable(bucket_reader):
            rows = await bucket_reader(
                start_time=start_date,
                end_time=end_date,
                granularity=granularity,
                filters=filters,
            )
            series = self._build_timeseries_series_from_bucket_rows(
                rows=rows,
                start_date=start_date,
                end_date=end_date,
                granularity=granularity,
            )
            return series, self._sum_series_counts(series), "db_aggregate", False

        safe_scan_limit = max(200, min(max(scan_limit, 1), 500))
        items, _ = await self._query_task_items(
            filters=filters,
            limit=safe_scan_limit,
            time_from=start_date.isoformat(),
            time_to=end_date.isoformat(),
        )
        series = self._build_timeseries_series_from_items(
            items=items,
            timestamp_fields=("due_date", "created_at", "updated_at"),
            start_date=start_date,
            end_date=end_date,
            granularity=granularity,
        )
        partial = len(items) >= safe_scan_limit
        return series, len(items), "item_scan", partial

    async def _query_flight_timeseries_series(
        self,
        *,
        filters: dict[str, Any],
        start_date: datetime,
        end_date: datetime,
        granularity: str,
        scan_limit: int,
    ) -> tuple[list[dict[str, Any]], int, str, bool]:
        self._ensure_query_repository()

        criteria: dict[str, Any] = dict(filters)
        criteria["scheduled_departure_from"] = start_date
        criteria["scheduled_departure_to"] = end_date
        scope = self._resolve_scope({"data_scope": filters.get("data_scope")}, criteria)

        bucket_reader = getattr(self._flight_ai_query_repository, "count_departures_by_time_buckets", None)
        if callable(bucket_reader):
            rows = await bucket_reader(
                criteria=criteria,
                start_time=start_date,
                end_time=end_date,
                granularity=granularity,
                scope=scope,
            )
            series = self._build_timeseries_series_from_bucket_rows(
                rows=rows,
                start_date=start_date,
                end_date=end_date,
                granularity=granularity,
            )
            return series, self._sum_series_counts(series), "db_aggregate", False

        safe_scan_limit = max(200, min(max(scan_limit, 1), 500))
        rows = await self._flight_ai_query_repository.search_flights(
            criteria=criteria,
            scope=scope,
            limit=safe_scan_limit,
            offset=0,
            sort_by="scheduled_departure",
            sort_order="asc",
        )
        items = [self._row_to_dict(row) for row in rows]
        series = self._build_timeseries_series_from_items(
            items=items,
            timestamp_fields=("scheduled_departure", "estimated_departure"),
            start_date=start_date,
            end_date=end_date,
            granularity=granularity,
        )
        partial = len(items) >= safe_scan_limit
        return series, len(items), "item_scan", partial

    async def _query_alert_timeseries(
        self,
        *,
        filters: dict[str, Any],
        limit: int,
        time_from: str | None,
        time_to: str | None,
    ) -> dict[str, Any]:
        start_date, end_date = self._resolve_datetime_range_for_timeseries(
            filters=filters,
            time_from=time_from,
            time_to=time_to,
        )
        granularity = self._resolve_timeseries_granularity(
            filters=filters,
            start_date=start_date,
            end_date=end_date,
        )
        preview_limit = max(1, min(limit, 200))
        preview_items, _ = await self._query_alert_items(
            filters=filters,
            limit=preview_limit,
            time_from=start_date.isoformat(),
            time_to=end_date.isoformat(),
        )
        series, total, source_mode, partial = await self._query_alert_timeseries_series(
            filters=filters,
            start_date=start_date,
            end_date=end_date,
            granularity=granularity,
            scan_limit=max(200, min(max(limit, 1), 500)),
        )
        return {
            "total": int(total),
            "items": preview_items[:preview_limit],
            "series": series,
            "granularity": granularity,
            "time_range": {
                "from": self._to_utc_aware(start_date).isoformat(),
                "to": self._to_utc_aware(end_date).isoformat(),
            },
            "partial": bool(partial),
            "source_availability": {
                "alerts": {
                    "available": True,
                    "mode": source_mode,
                    "partial": bool(partial),
                }
            },
        }

    async def _query_task_timeseries(
        self,
        *,
        filters: dict[str, Any],
        limit: int,
        time_from: str | None,
        time_to: str | None,
    ) -> dict[str, Any]:
        start_date, end_date = self._resolve_datetime_range_for_timeseries(
            filters=filters,
            time_from=time_from,
            time_to=time_to,
        )
        granularity = self._resolve_timeseries_granularity(
            filters=filters,
            start_date=start_date,
            end_date=end_date,
        )
        preview_limit = max(1, min(limit, 200))
        preview_items, _ = await self._query_task_items(
            filters=filters,
            limit=preview_limit,
            time_from=start_date.isoformat(),
            time_to=end_date.isoformat(),
        )
        series, total, source_mode, partial = await self._query_task_timeseries_series(
            filters=filters,
            start_date=start_date,
            end_date=end_date,
            granularity=granularity,
            scan_limit=max(200, min(max(limit, 1), 500)),
        )
        return {
            "total": int(total),
            "items": preview_items[:preview_limit],
            "series": series,
            "granularity": granularity,
            "time_range": {
                "from": self._to_utc_aware(start_date).isoformat(),
                "to": self._to_utc_aware(end_date).isoformat(),
            },
            "partial": bool(partial),
            "source_availability": {
                "tasks": {
                    "available": True,
                    "mode": source_mode,
                    "partial": bool(partial),
                }
            },
        }

    async def _query_ops_timeseries(
        self,
        *,
        filters: dict[str, Any],
        limit: int,
        time_from: str | None,
        time_to: str | None,
    ) -> dict[str, Any]:
        start_date, end_date = self._resolve_datetime_range_for_timeseries(
            filters=filters,
            time_from=time_from,
            time_to=time_to,
        )
        granularity = self._resolve_timeseries_granularity(
            filters=filters,
            start_date=start_date,
            end_date=end_date,
        )

        scan_limit = max(200, min(max(limit, 1), 500))

        base_series = self._build_empty_timeseries_series(
            start_date=start_date,
            end_date=end_date,
            granularity=granularity,
        )
        flights_series = list(base_series)
        alerts_series = list(base_series)
        tasks_series = list(base_series)
        source_availability: dict[str, dict[str, Any]] = {
            "flights": {
                "available": False,
                "mode": "unavailable",
                "partial": True,
            },
            "alerts": {
                "available": False,
                "mode": "unavailable",
                "partial": True,
            },
            "tasks": {
                "available": False,
                "mode": "unavailable",
                "partial": True,
            },
        }

        if self._flight_ai_query_repository is not None:
            try:
                flights_series, _, mode, partial = await self._query_flight_timeseries_series(
                    filters=filters,
                    start_date=start_date,
                    end_date=end_date,
                    granularity=granularity,
                    scan_limit=scan_limit,
                )
                source_availability["flights"] = {
                    "available": True,
                    "mode": mode,
                    "partial": bool(partial),
                }
            except Exception as exc:  # noqa: BLE001 - recovery handler must catch all errors
                source_availability["flights"] = {
                    "available": False,
                    "mode": "error",
                    "partial": True,
                    "error": str(exc),
                }

        if self._anomaly_repository is not None:
            try:
                alerts_series, _, mode, partial = await self._query_alert_timeseries_series(
                    filters=filters,
                    start_date=start_date,
                    end_date=end_date,
                    granularity=granularity,
                    scan_limit=scan_limit,
                )
                source_availability["alerts"] = {
                    "available": True,
                    "mode": mode,
                    "partial": bool(partial),
                }
            except Exception as exc:  # noqa: BLE001 - recovery handler must catch all errors
                source_availability["alerts"] = {
                    "available": False,
                    "mode": "error",
                    "partial": True,
                    "error": str(exc),
                }

        if self._todo_service is not None:
            try:
                tasks_series, _, mode, partial = await self._query_task_timeseries_series(
                    filters=filters,
                    start_date=start_date,
                    end_date=end_date,
                    granularity=granularity,
                    scan_limit=scan_limit,
                )
                source_availability["tasks"] = {
                    "available": True,
                    "mode": mode,
                    "partial": bool(partial),
                }
            except Exception as exc:  # noqa: BLE001 - recovery handler must catch all errors
                source_availability["tasks"] = {
                    "available": False,
                    "mode": "error",
                    "partial": True,
                    "error": str(exc),
                }

        series_index: dict[str, dict[str, Any]] = {
            str(entry.get("time") or ""): {
                "time": entry.get("time"),
                "flights": 0,
                "alerts": 0,
                "tasks": 0,
                "total": 0,
            }
            for entry in base_series
            if entry.get("time")
        }

        for entry in flights_series:
            key = str(entry.get("time") or "")
            if key and key in series_index:
                series_index[key]["flights"] = int(entry.get("count") or 0)
        for entry in alerts_series:
            key = str(entry.get("time") or "")
            if key and key in series_index:
                series_index[key]["alerts"] = int(entry.get("count") or 0)
        for entry in tasks_series:
            key = str(entry.get("time") or "")
            if key and key in series_index:
                series_index[key]["tasks"] = int(entry.get("count") or 0)

        merged_series: list[dict[str, Any]] = []
        for key in sorted(series_index.keys()):
            bucket = series_index[key]
            bucket["total"] = int(bucket.get("flights", 0)) + int(bucket.get("alerts", 0)) + int(bucket.get("tasks", 0))
            merged_series.append(bucket)

        flights_snapshot = await self._safe_ops_flight_status_snapshot(
            filters=filters,
            time_from=start_date.isoformat(),
            time_to=end_date.isoformat(),
        )
        alerts_snapshot = await self._safe_ops_alert_stats(
            start_date=start_date,
            end_date=end_date,
        )
        tasks_snapshot = await self._safe_ops_task_stats(filters=filters)

        flights_total = int((flights_snapshot.get("total") or 0) if isinstance(flights_snapshot, dict) else 0)
        alerts_total = int((alerts_snapshot.get("total") or 0) if isinstance(alerts_snapshot, dict) else 0)
        tasks_total = int((tasks_snapshot.get("total") or 0) if isinstance(tasks_snapshot, dict) else 0)

        partial = any(
            not bool(snapshot.get("available", False)) or bool(snapshot.get("partial", False))
            for snapshot in source_availability.values()
        )

        return {
            "total": flights_total + alerts_total + tasks_total,
            "items": [
                {
                    "flights": flights_snapshot,
                    "alerts": alerts_snapshot,
                    "tasks": tasks_snapshot,
                }
            ],
            "series": merged_series,
            "granularity": granularity,
            "time_range": {
                "from": self._to_utc_aware(start_date).isoformat(),
                "to": self._to_utc_aware(end_date).isoformat(),
            },
            "partial": bool(partial),
            "source_availability": source_availability,
        }
