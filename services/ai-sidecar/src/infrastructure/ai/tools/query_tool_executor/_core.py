"""Query tool executor for natural-language flight queries."""

from __future__ import annotations

import logging

from ..base import ToolCategory, ToolExecutionError, ToolExecutionStatus
from ..query_tools import QueryToolName
from .protocols import (
    AnomalyRepositoryReader,
    FlightAIQueryRepositoryReader,
    FlightInsightServiceReader,
    FlightReader,
    TodoServiceReader,
)

logger = logging.getLogger(__name__)


class _CoreMixin:
    """QueryToolExecutor mixin."""

    def __init__(
        self,
        flight_service: FlightReader | None = None,
        flight_insight_service: FlightInsightServiceReader | None = None,
        flight_ai_query_repository: FlightAIQueryRepositoryReader | None = None,
        anomaly_repository: AnomalyRepositoryReader | None = None,
        todo_service: TodoServiceReader | None = None,
        default_user: str = "AI_Assistant",
    ):
        super().__init__(default_user)
        self._service = flight_service
        self._flight_insight_service = flight_insight_service
        self._flight_ai_query_repository = flight_ai_query_repository
        self._anomaly_repository = anomaly_repository
        self._todo_service = todo_service

    def _register_handlers(self) -> None:
        self._handlers = {
            QueryToolName.QUERY.value: self._handle_query_unified,
            QueryToolName.SEARCH_FLIGHTS_ADVANCED.value: self._handle_search_flights_advanced,
            QueryToolName.COUNT_FLIGHTS_BY_STATUS.value: self._handle_count_flights_by_status,
            QueryToolName.GET_DELAYED_FLIGHTS.value: self._handle_get_delayed_flights,
            QueryToolName.GET_FLIGHTS_BY_TIME_RANGE.value: self._handle_get_flights_by_time_range,
            QueryToolName.GET_ABNORMAL_FLIGHTS.value: self._handle_get_abnormal_flights,
            QueryToolName.GET_TURNAROUND_STATS.value: self._handle_get_turnaround_stats,
            QueryToolName.GENERATE_FLIGHT_HISTORY_REPORT.value: self._handle_generate_flight_history_report,
            QueryToolName.GENERATE_FLIGHT_EVENT_JOURNEY.value: self._handle_generate_flight_event_journey,
        }

    def get_category(self) -> ToolCategory:
        return ToolCategory.QUERY

    def _ensure_insight_service(self) -> None:
        if self._flight_insight_service is None:
            raise ToolExecutionError(
                "航班洞察服务未初始化，无法生成报表或事件经过",
                ToolExecutionStatus.ERROR,
            )

    def _ensure_query_repository(self) -> None:
        if self._flight_ai_query_repository is None:
            raise ToolExecutionError(
                "航班查询仓储未初始化，无法执行数据库筛选查询",
                ToolExecutionStatus.ERROR,
            )

    def _ensure_anomaly_repository(self) -> None:
        if self._anomaly_repository is None:
            raise ToolExecutionError(
                "异常查询仓储未初始化，无法执行 alerts 数据集查询",
                ToolExecutionStatus.ERROR,
            )

    def _ensure_todo_service(self) -> None:
        if self._todo_service is None:
            raise ToolExecutionError(
                "待办服务未初始化，无法执行 tasks 数据集查询",
                ToolExecutionStatus.ERROR,
            )
