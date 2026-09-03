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
        "- Write actions are forbidden for this task type: do not call them and do not propose them.\n"
        "- Use ontology.lookup(entity_id) to retrieve entity relationships and constraints before answering questions about specific flights/gates.\n"
        "- Use ontology.explain_constraints(entity_type, proposed_change) to verify any constraint violations.\n"
        "## Evidence Freshness Validation (P1-1-B)\n"
        "- **Check freshness**: Before using data, verify each tool result's `as_of` timestamp and `freshness_seconds` field.\n"
        "- **Freshness thresholds**:\n"
        "  - Flight status evidence: max_age=30s\n"
        "  - Current stand/gate assignment evidence: max_age=10s\n"
        "  - Dispatch order evidence: max_age=60s\n"
        '- **Reject stale data**: If evidence exceeds its freshness threshold, report "数据过期，需要重新查询" and retry the tool.\n'
        "- **Confidence scoring (P1-1-C)**:\n"
        '  - If confidence < 0.7, return "uncertain, need human review" marker in answer.\n'
        '  - List missing fields clearly: "Missing required fields: {field1}, {field2}".\n'
        "## Shadow Mode Constraints\n"
        "- Shadow runs are strictly read-only; never propose or execute write actions.\n"
        "- Answer within a linear tool chain (fanout depth 0); do not branch into parallel investigations.\n"
        "- Every factual claim must include evidence coverage: source, object_id, as_of.\n"
    ),
    allowed_tool_categories=frozenset({"query", "flight", "anomaly", "ontology"}),
    # Task F5: SQL exits the production query face — query slots go through
    # the query tool parameters, never through hand-written SQL.
    denied_tools=frozenset(WRITE_ACTION_TOOLS) | {"sql_query_readonly"},
    default_max_tool_rounds=6,
    hard_max_tool_rounds=8,
    # Task B3: query 对话保持线性短链，默认不打开 LLM 摘要（可逐条对话覆盖）。
    default_llm_summary=False,
)


__all__ = ["QUERY_OPS_TEMPLATE"]
