"""Data models and module-level constants for NL query service."""

from __future__ import annotations

from dataclasses import dataclass

INSIGHT_TOOL_NAMES = {
    "generate_flight_history_report",
    "generate_flight_event_journey",
}

SQL_READ_TOOL_NAME = "sql_query_readonly"

LEGACY_QUERY_TOOL_NAMES = {
    "QUERY",
    "search_flights_advanced",
    "count_flights_by_status",
    "get_delayed_flights",
    "get_flights_by_time_range",
    "get_abnormal_flights",
    "get_turnaround_stats",
}


@dataclass
class NLQueryResult:
    query: str
    interpretation: str
    structured_data: object
    visualization_hint: str | None
    summary: str
    conversation_id: str
    duration_ms: int

    def to_dict(self) -> dict[str, object]:
        return {
            "query": self.query,
            "interpretation": self.interpretation,
            "structured_data": self.structured_data,
            "visualization_hint": self.visualization_hint,
            "summary": self.summary,
            "conversation_id": self.conversation_id,
            "duration_ms": self.duration_ms,
        }
