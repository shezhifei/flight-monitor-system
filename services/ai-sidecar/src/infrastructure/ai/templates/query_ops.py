"""Query-ops task template (hybrid agent Task A4).

Policy-only module for ``task_type=query_ops``: facts-only answering with
cited evidence, a read-only tool face, and a 6-round budget. Slot extraction
(date/status/flight-number) goes through the query tools' parameters — there
is no hand-written SQL path (docs/plans/2026-08-14-hybrid-agent-architecture.md,
Task A4).
"""

from __future__ import annotations

from src.infrastructure.ai.tools.tool_executor import WRITE_ACTION_TOOLS

from .base import TaskTemplate

QUERY_OPS_TEMPLATE = TaskTemplate(
    task_type="query_ops",
    display_name="运营查询",
    system_prompt_addendum=(
        "# Task Template: query_ops (read-only operations query)\n"
        "- State facts only. Every claim must cite tool evidence: name the tool and the object id(s) it returned.\n"
        "- If the data is missing or you are not sure, say so explicitly; never invent flight ids, flight numbers, or statuses.\n"
        "- Extract query slots (dates, statuses, flight numbers) into the query tool parameters; do not write SQL yourself.\n"
        "- Write actions are forbidden for this task type: do not call them and do not propose them."
    ),
    # Read-only categories only: query catalog (search_flights_* / count_* /
    # get_delayed_flights / ...), the flight adapter, and anomaly read tools.
    allowed_tool_categories=frozenset({"query", "flight", "anomaly"}),
    denied_tools=frozenset(WRITE_ACTION_TOOLS),
    default_max_tool_rounds=6,
)


__all__ = ["QUERY_OPS_TEMPLATE"]
