"""Query tool executor for natural-language flight queries."""

from __future__ import annotations

import logging
from datetime import datetime
from typing import Any

from src.infrastructure.ai.monitoring.metrics import record_query_route

from ..base import ToolExecutionError, ToolExecutionStatus
from ..query_tools import QueryToolName

logger = logging.getLogger(__name__)


class _HandlersInsightsMixin:
    """QueryToolExecutor mixin."""

    async def _handle_query_unified(self, args: dict[str, object]) -> dict[str, object]:
        intent = str(args.get("intent") or "search").strip().lower()
        dataset = str(args.get("dataset") or "flights").strip().lower()
        filters = args.get("filters")
        if not isinstance(filters, dict):
            filters = {}
        metrics = args.get("metrics")
        if not isinstance(metrics, list):
            metrics = []
        group_by = args.get("group_by")
        if not isinstance(group_by, list):
            group_by = []
        limit = self._safe_int(args.get("limit"), default=50) or 50
        limit = max(1, min(limit, 500))

        if dataset not in {"flights", "alerts", "tasks", "ops"}:
            record_query_route(
                intent=intent,
                dataset=dataset,
                adapter="none",
                status=ToolExecutionStatus.VALIDATION_ERROR.value,
                misroute=True,
                reason="unsupported_dataset",
            )
            raise ToolExecutionError(
                f"Unsupported dataset for QUERY: {dataset}",
                ToolExecutionStatus.VALIDATION_ERROR,
            )

        time_range = args.get("time_range")
        time_from: str | None = None
        time_to: str | None = None
        if isinstance(time_range, dict):
            time_from = str(time_range.get("from") or "").strip() or None
            time_to = str(time_range.get("to") or "").strip() or None

        if dataset == "alerts":
            return await self._handle_query_alerts_unified(
                intent=intent,
                filters=filters,
                limit=limit,
                time_from=time_from,
                time_to=time_to,
            )

        if dataset == "tasks":
            return await self._handle_query_tasks_unified(
                intent=intent,
                filters=filters,
                limit=limit,
                time_from=time_from,
                time_to=time_to,
            )

        if dataset == "ops":
            return await self._handle_query_ops_unified(
                intent=intent,
                filters=filters,
                limit=limit,
                time_from=time_from,
                time_to=time_to,
            )

        if intent in {"search", "detail"}:
            adapted_args = dict(filters)
            adapted_args["limit"] = limit
            if time_from:
                adapted_args["start_time"] = time_from
            if time_to:
                adapted_args["end_time"] = time_to

            if "min_delay_minutes" in adapted_args:
                payload = await self._handle_get_delayed_flights(adapted_args)
                adapter = QueryToolName.GET_DELAYED_FLIGHTS.value
            elif adapted_args.get("has_open_anomaly") is True:
                payload = await self._handle_get_abnormal_flights(adapted_args)
                adapter = QueryToolName.GET_ABNORMAL_FLIGHTS.value
            elif time_from and time_to:
                payload = await self._handle_get_flights_by_time_range(adapted_args)
                adapter = QueryToolName.GET_FLIGHTS_BY_TIME_RANGE.value
            else:
                payload = await self._handle_search_flights_advanced(adapted_args)
                adapter = QueryToolName.SEARCH_FLIGHTS_ADVANCED.value

            payload["meta"] = {
                **(payload.get("meta") or {}),
                "router": "QUERY",
                "intent": intent,
                "dataset": dataset,
                "legacy_adapter": adapter,
            }
            record_query_route(
                intent=intent,
                dataset=dataset,
                adapter=adapter,
                status=ToolExecutionStatus.SUCCESS.value,
                misroute=False,
            )
            return payload

        if intent == "timeseries":
            if not (time_from and time_to):
                now = datetime.now()
                time_from = now.replace(hour=0, minute=0, second=0, microsecond=0).isoformat()
                time_to = now.replace(hour=23, minute=59, second=59, microsecond=0).isoformat()

            start_date = self._parse_datetime(time_from, "time_range.from")
            end_date = self._parse_datetime(time_to, "time_range.to")
            granularity = self._resolve_timeseries_granularity(
                filters=filters,
                start_date=start_date,
                end_date=end_date,
            )
            series, series_total, series_mode, series_partial = await self._query_flight_timeseries_series(
                filters=filters,
                start_date=start_date,
                end_date=end_date,
                granularity=granularity,
                scan_limit=max(200, min(max(limit, 1), 500)),
            )

            preview_limit = max(1, min(limit, 200))
            payload = await self._handle_get_flights_by_time_range(
                {
                    "start_time": time_from,
                    "end_time": time_to,
                    "limit": preview_limit,
                    **filters,
                }
            )
            adapter = QueryToolName.GET_FLIGHTS_BY_TIME_RANGE.value
            payload["total"] = int(series_total)
            payload["series"] = series
            payload["granularity"] = granularity
            payload["time_range"] = {
                "from": self._to_utc_aware(start_date).isoformat(),
                "to": self._to_utc_aware(end_date).isoformat(),
            }
            payload["partial"] = bool(series_partial)
            payload["source_availability"] = {
                "flights": {
                    "available": True,
                    "mode": series_mode,
                    "partial": bool(series_partial),
                }
            }
            self._attach_query_meta(
                payload,
                intent=intent,
                dataset=dataset,
                adapter=adapter,
                extra_meta={
                    "partial": bool(series_partial),
                    "bucket_count": len(series),
                    "source_availability": payload.get("source_availability") or {},
                },
            )
            record_query_route(
                intent=intent,
                dataset=dataset,
                adapter=adapter,
                status=ToolExecutionStatus.SUCCESS.value,
                misroute=False,
            )
            return payload

        if intent in {"aggregate", "compare"}:
            normalized_group_by = {str(item).strip().lower() for item in group_by if str(item).strip()}
            normalized_metrics = {str(item).strip().lower() for item in metrics if str(item).strip()}
            if "turnaround" in normalized_metrics:
                payload = await self._handle_get_turnaround_stats(dict(filters))
                adapter = QueryToolName.GET_TURNAROUND_STATS.value
                payload["meta"] = {
                    **(payload.get("meta") or {}),
                    "router": "QUERY",
                    "intent": intent,
                    "dataset": dataset,
                    "legacy_adapter": adapter,
                }
                record_query_route(
                    intent=intent,
                    dataset=dataset,
                    adapter=adapter,
                    status=ToolExecutionStatus.SUCCESS.value,
                    misroute=False,
                )
                return payload

            count_args = dict(filters)
            if time_from and not count_args.get("date_from"):
                count_args["date_from"] = time_from
            if time_to and not count_args.get("date_to"):
                count_args["date_to"] = time_to
            payload = await self._handle_count_flights_by_status(count_args)
            adapter = QueryToolName.COUNT_FLIGHTS_BY_STATUS.value
            payload["meta"] = {
                **(payload.get("meta") or {}),
                "router": "QUERY",
                "intent": intent,
                "dataset": dataset,
                "group_by": sorted(normalized_group_by),
                "legacy_adapter": adapter,
            }
            record_query_route(
                intent=intent,
                dataset=dataset,
                adapter=adapter,
                status=ToolExecutionStatus.SUCCESS.value,
                misroute=False,
            )
            return payload

        record_query_route(
            intent=intent,
            dataset=dataset,
            adapter="none",
            status=ToolExecutionStatus.VALIDATION_ERROR.value,
            misroute=True,
            reason="unsupported_intent",
        )
        raise ToolExecutionError(
            f"Unsupported QUERY intent: {intent}",
            ToolExecutionStatus.VALIDATION_ERROR,
        )

    async def _handle_query_alerts_unified(
        self,
        *,
        intent: str,
        filters: dict[str, Any],
        limit: int,
        time_from: str | None,
        time_to: str | None,
    ) -> dict[str, object]:
        if intent in {"search", "detail"}:
            items, adapter = await self._query_alert_items(
                filters=filters,
                limit=limit,
                time_from=time_from,
                time_to=time_to,
            )
            payload: dict[str, object] = {
                "total": len(items),
                "items": items,
            }
            self._attach_query_meta(payload, intent=intent, dataset="alerts", adapter=adapter)
            record_query_route(
                intent=intent,
                dataset="alerts",
                adapter=adapter,
                status=ToolExecutionStatus.SUCCESS.value,
                misroute=False,
            )
            return payload

        if intent == "timeseries":
            payload = await self._query_alert_timeseries(
                filters=filters,
                limit=limit,
                time_from=time_from,
                time_to=time_to,
            )
            self._attach_query_meta(
                payload,
                intent=intent,
                dataset="alerts",
                adapter="alerts_timeseries",
                extra_meta={
                    "partial": bool(payload.get("partial", False)),
                    "bucket_count": len(payload.get("series") or []),
                    "source_availability": payload.get("source_availability") or {},
                },
            )
            record_query_route(
                intent=intent,
                dataset="alerts",
                adapter="alerts_timeseries",
                status=ToolExecutionStatus.SUCCESS.value,
                misroute=False,
            )
            return payload

        if intent in {"aggregate", "compare"}:
            start_date, end_date = self._resolve_datetime_range(
                filters=filters,
                time_from=time_from,
                time_to=time_to,
            )
            stats = await self._query_alert_stats(start_date=start_date, end_date=end_date)
            payload = {
                "total": int(stats.get("total") or 0),
                "stats": stats,
            }
            self._attach_query_meta(payload, intent=intent, dataset="alerts", adapter="get_anomaly_stats")
            record_query_route(
                intent=intent,
                dataset="alerts",
                adapter="get_anomaly_stats",
                status=ToolExecutionStatus.SUCCESS.value,
                misroute=False,
            )
            return payload

        record_query_route(
            intent=intent,
            dataset="alerts",
            adapter="none",
            status=ToolExecutionStatus.VALIDATION_ERROR.value,
            misroute=True,
            reason="unsupported_intent",
        )
        raise ToolExecutionError(
            f"Unsupported QUERY intent for alerts dataset: {intent}",
            ToolExecutionStatus.VALIDATION_ERROR,
        )

    async def _handle_query_tasks_unified(
        self,
        *,
        intent: str,
        filters: dict[str, Any],
        limit: int,
        time_from: str | None,
        time_to: str | None,
    ) -> dict[str, object]:
        if intent in {"search", "detail"}:
            items, adapter = await self._query_task_items(
                filters=filters,
                limit=limit,
                time_from=time_from,
                time_to=time_to,
            )
            payload: dict[str, object] = {
                "total": len(items),
                "items": items,
            }
            self._attach_query_meta(payload, intent=intent, dataset="tasks", adapter=adapter)
            record_query_route(
                intent=intent,
                dataset="tasks",
                adapter=adapter,
                status=ToolExecutionStatus.SUCCESS.value,
                misroute=False,
            )
            return payload

        if intent == "timeseries":
            payload = await self._query_task_timeseries(
                filters=filters,
                limit=limit,
                time_from=time_from,
                time_to=time_to,
            )
            self._attach_query_meta(
                payload,
                intent=intent,
                dataset="tasks",
                adapter="tasks_timeseries",
                extra_meta={
                    "partial": bool(payload.get("partial", False)),
                    "bucket_count": len(payload.get("series") or []),
                    "source_availability": payload.get("source_availability") or {},
                },
            )
            record_query_route(
                intent=intent,
                dataset="tasks",
                adapter="tasks_timeseries",
                status=ToolExecutionStatus.SUCCESS.value,
                misroute=False,
            )
            return payload

        if intent in {"aggregate", "compare"}:
            stats = await self._query_task_stats(filters=filters)
            payload = {
                "total": int(stats.get("total") or 0),
                "group_by_status": stats.get("status_stats", {}),
                "stats": stats,
            }
            self._attach_query_meta(payload, intent=intent, dataset="tasks", adapter="get_todo_stats")
            record_query_route(
                intent=intent,
                dataset="tasks",
                adapter="get_todo_stats",
                status=ToolExecutionStatus.SUCCESS.value,
                misroute=False,
            )
            return payload

        record_query_route(
            intent=intent,
            dataset="tasks",
            adapter="none",
            status=ToolExecutionStatus.VALIDATION_ERROR.value,
            misroute=True,
            reason="unsupported_intent",
        )
        raise ToolExecutionError(
            f"Unsupported QUERY intent for tasks dataset: {intent}",
            ToolExecutionStatus.VALIDATION_ERROR,
        )

    async def _handle_query_ops_unified(
        self,
        *,
        intent: str,
        filters: dict[str, Any],
        limit: int,
        time_from: str | None,
        time_to: str | None,
    ) -> dict[str, object]:
        if intent not in {"search", "detail", "timeseries", "aggregate", "compare"}:
            record_query_route(
                intent=intent,
                dataset="ops",
                adapter="none",
                status=ToolExecutionStatus.VALIDATION_ERROR.value,
                misroute=True,
                reason="unsupported_intent",
            )
            raise ToolExecutionError(
                f"Unsupported QUERY intent for ops dataset: {intent}",
                ToolExecutionStatus.VALIDATION_ERROR,
            )

        if intent == "timeseries":
            payload = await self._query_ops_timeseries(
                filters=filters,
                limit=limit,
                time_from=time_from,
                time_to=time_to,
            )
            self._attach_query_meta(
                payload,
                intent=intent,
                dataset="ops",
                adapter="ops_timeseries",
                extra_meta={
                    "partial": bool(payload.get("partial", False)),
                    "bucket_count": len(payload.get("series") or []),
                    "source_availability": payload.get("source_availability") or {},
                },
            )
            record_query_route(
                intent=intent,
                dataset="ops",
                adapter="ops_timeseries",
                status=ToolExecutionStatus.SUCCESS.value,
                misroute=False,
            )
            return payload

        start_date, end_date = self._resolve_datetime_range(
            filters=filters,
            time_from=time_from,
            time_to=time_to,
        )
        flights_snapshot = await self._safe_ops_flight_status_snapshot(
            filters=filters,
            time_from=time_from,
            time_to=time_to,
        )
        alerts_snapshot = await self._safe_ops_alert_stats(
            start_date=start_date,
            end_date=end_date,
        )
        tasks_snapshot = await self._safe_ops_task_stats(
            filters=filters,
        )

        flights_total = int((flights_snapshot.get("total") or 0) if isinstance(flights_snapshot, dict) else 0)
        alerts_total = int((alerts_snapshot.get("total") or 0) if isinstance(alerts_snapshot, dict) else 0)
        tasks_total = int((tasks_snapshot.get("total") or 0) if isinstance(tasks_snapshot, dict) else 0)

        payload = {
            "total": flights_total + alerts_total + tasks_total,
            "items": [
                {
                    "flights": flights_snapshot,
                    "alerts": alerts_snapshot,
                    "tasks": tasks_snapshot,
                }
            ],
        }
        self._attach_query_meta(payload, intent=intent, dataset="ops", adapter="ops_snapshot")
        record_query_route(
            intent=intent,
            dataset="ops",
            adapter="ops_snapshot",
            status=ToolExecutionStatus.SUCCESS.value,
            misroute=False,
        )
        return payload

    async def _query_alert_items(
        self,
        *,
        filters: dict[str, Any],
        limit: int,
        time_from: str | None,
        time_to: str | None,
    ) -> tuple[list[dict[str, Any]], str]:
        self._ensure_anomaly_repository()

        anomaly_id = self._first_non_empty(filters, "anomaly_id", "id", "alert_id")
        if anomaly_id:
            anomaly = await self._anomaly_repository.find_by_id(anomaly_id)
            if anomaly is None:
                raise ToolExecutionError(
                    f"Anomaly not found: {anomaly_id}",
                    ToolExecutionStatus.NOT_FOUND,
                )
            return [self._anomaly_to_dict(anomaly)], "get_anomaly_detail"

        start_date, end_date = self._resolve_datetime_range(
            filters=filters,
            time_from=time_from,
            time_to=time_to,
        )
        status = self._first_non_empty(filters, "status")
        anomaly_type = self._first_non_empty(filters, "anomaly_type", "type")
        flight_id = self._first_non_empty(filters, "flight_id")
        severity = self._first_non_empty(filters, "severity")
        anomalies = await self._anomaly_repository.list_anomalies(
            status=status,
            anomaly_type=anomaly_type,
            start_date=start_date,
            end_date=end_date,
            limit=max(1, min(limit, 500)),
            offset=0,
        )

        items = [self._anomaly_to_dict(item) for item in anomalies]
        if flight_id:
            normalized_flight_id = str(flight_id).strip().upper()
            items = [item for item in items if str(item.get("flight_id") or "").strip().upper() == normalized_flight_id]
        if severity:
            normalized_severity = str(severity).strip().lower()
            items = [item for item in items if str(item.get("severity") or "").strip().lower() == normalized_severity]

        return items[: max(1, min(limit, 500))], "list_anomalies"

    async def _query_alert_stats(
        self,
        *,
        start_date: datetime | None,
        end_date: datetime | None,
    ) -> dict[str, Any]:
        self._ensure_anomaly_repository()
        raw_stats = await self._anomaly_repository.get_stats(
            start_date=start_date,
            end_date=end_date,
        )
        return self._normalize_alert_stats(raw_stats)

    async def _query_task_items(
        self,
        *,
        filters: dict[str, Any],
        limit: int,
        time_from: str | None,
        time_to: str | None,
    ) -> tuple[list[dict[str, Any]], str]:
        self._ensure_todo_service()

        todo_id = self._first_non_empty(filters, "todo_id", "task_id", "id")
        if todo_id:
            aggregate = await self._get_todo_by_identifier(todo_id)
            if aggregate is None:
                raise ToolExecutionError(
                    f"Todo not found: {todo_id}",
                    ToolExecutionStatus.NOT_FOUND,
                )
            return [self._todo_to_dict(aggregate)], "get_todo"

        options = self._build_todo_query_options(limit=limit, filters=filters)
        query_text = self._first_non_empty(filters, "query", "keyword", "q")
        if query_text:
            aggregates = await self._todo_service.search_todos(query_text, options)
            return [self._todo_to_dict(item) for item in aggregates], "search_todos"

        if bool(filters.get("overdue_only")) and hasattr(self._todo_service, "get_overdue_todos"):
            aggregates = await self._todo_service.get_overdue_todos(options)
            return [self._todo_to_dict(item) for item in aggregates], "list_todos"

        if bool(filters.get("due_today")) and hasattr(self._todo_service, "get_due_today_todos"):
            aggregates = await self._todo_service.get_due_today_todos(options)
            return [self._todo_to_dict(item) for item in aggregates], "list_todos"

        if bool(filters.get("high_priority_only")) and hasattr(self._todo_service, "get_high_priority_todos"):
            aggregates = await self._todo_service.get_high_priority_todos(options)
            return [self._todo_to_dict(item) for item in aggregates], "list_todos"

        start_date, end_date = self._resolve_datetime_range(
            filters=filters,
            time_from=time_from,
            time_to=time_to,
        )
        if (start_date is not None or end_date is not None) and not hasattr(self._todo_service, "list_todos"):
            raise ToolExecutionError("Todo service does not support list_todos", ToolExecutionStatus.ERROR)

        aggregates = await self._todo_service.list_todos(options)
        items = [self._todo_to_dict(item) for item in aggregates]
        if start_date is not None or end_date is not None:
            items = [
                item for item in items if self._todo_in_time_window(item=item, start_date=start_date, end_date=end_date)
            ]
        return items, "list_todos"

    async def _query_task_stats(self, *, filters: dict[str, Any]) -> dict[str, Any]:
        self._ensure_todo_service()
        criteria: dict[str, Any] = {}
        assignee = self._first_non_empty(filters, "assignee", "owner")
        if assignee:
            criteria["assignee"] = assignee
        raw_stats = await self._todo_service.get_todo_stats(criteria)
        return self._normalize_todo_stats(raw_stats)

    async def _safe_ops_flight_status_snapshot(
        self,
        *,
        filters: dict[str, Any],
        time_from: str | None,
        time_to: str | None,
    ) -> dict[str, Any]:
        count_args = dict(filters)
        if time_from and not count_args.get("date_from"):
            count_args["date_from"] = time_from
        if time_to and not count_args.get("date_to"):
            count_args["date_to"] = time_to
        try:
            payload = await self._handle_count_flights_by_status(count_args)
            return {
                "available": True,
                "total": int(payload.get("total") or 0),
                "group_by_status": payload.get("group_by_status") or {},
            }
        except Exception as exc:  # noqa: BLE001 - recovery handler must catch all errors
            return {
                "available": False,
                "total": 0,
                "group_by_status": {},
                "error": str(exc),
            }

    async def _safe_ops_alert_stats(
        self,
        *,
        start_date: datetime | None,
        end_date: datetime | None,
    ) -> dict[str, Any]:
        if self._anomaly_repository is None:
            return {"available": False, "total": 0}
        try:
            stats = await self._query_alert_stats(start_date=start_date, end_date=end_date)
            return {"available": True, **stats}
        except Exception as exc:  # noqa: BLE001 - recovery handler must catch all errors
            return {
                "available": False,
                "total": 0,
                "error": str(exc),
            }

    async def _safe_ops_task_stats(self, *, filters: dict[str, Any]) -> dict[str, Any]:
        if self._todo_service is None:
            return {"available": False, "total": 0}
        try:
            stats = await self._query_task_stats(filters=filters)
            return {"available": True, **stats}
        except Exception as exc:  # noqa: BLE001 - recovery handler must catch all errors
            return {
                "available": False,
                "total": 0,
                "error": str(exc),
            }
