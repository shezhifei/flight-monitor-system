"""Eval sampling from the control-plane ledger (Task G4).

Production runs are not traced twice: the eval harness folds the existing
``ai_run_checkpoints`` rows (written by the run loop over MQ) into an
:class:`EvalRunResult`. These tests lock the folding rules:

1. ``after_tool`` snapshots carry the executed tool names (``results[*].tool_name``)
   and the working-memory ``evidence.json`` object ids.
2. Governance-blocked results count as unauthorized attempts.
3. ``after_completion`` marks the run successful and carries the final answer.
4. ``ingest_run_from_ledger`` reads ``ai_run_checkpoints`` by run_id and fails
   closed (no fabricated sample) when the run has no checkpoints.
"""

from __future__ import annotations

import json

import pytest

from src.application.services.ai.llm_eval_service import (
    EvalRunResult,
    EvaluationService,
)
from src.application.services.ai.llm_eval_service.service import (
    build_eval_result_from_checkpoints,
)


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------


def _after_tool_row(
    *,
    results: list[dict],
    evidence: list[dict] | None = None,
    plan_state: dict | None = None,
    timestamp: float = 1000.0,
) -> dict:
    """One ``after_tool`` checkpoint row in ledger shape."""
    snapshot = {
        "round_index": 1,
        "tool_calls_executed": len(results),
        "results": results,
        "messages_count": 4,
        "working_memory": {
            "run_id": "run-1",
            "evidence.json": evidence or [],
            "plan_state": plan_state,
        },
        "timestamp": timestamp,
    }
    return {
        "checkpoint_id": "cp-1",
        "run_id": "run-1",
        "checkpoint_type": "after_tool",
        "sequence_no": 2,
        "snapshot": snapshot,
    }


def _completion_row(answer: str = "航班CA1832当前状态为延误。") -> dict:
    return {
        "checkpoint_id": "cp-9",
        "run_id": "run-1",
        "checkpoint_type": "after_completion",
        "sequence_no": 9,
        "snapshot": {
            "round_index": 2,
            "final_result": {"text": answer, "tool_calls_count": 0},
            "messages_count": 6,
            "timestamp": 1004.5,
        },
    }


# ---------------------------------------------------------------------------
# 1. called_tools / evidence_object_ids come out of one after_tool snapshot
# ---------------------------------------------------------------------------


def test_extracts_called_tools_and_evidence_from_after_tool_checkpoint() -> None:
    row = _after_tool_row(
        results=[
            {"tool_call_id": "tc-1", "tool_name": "ontology.lookup", "result": {"object_id": "flight-1832"}},
            {"tool_call_id": "tc-2", "tool_name": "get_delayed_flights", "result": {"flights": []}},
        ],
        evidence=[
            {"evidence_id": "ev-0001", "source": "ontology.lookup", "object_id": "flight-1832"},
            {"evidence_id": "ev-0002", "source": "get_delayed_flights", "object_id": ""},
        ],
    )

    result = build_eval_result_from_checkpoints([row])

    assert isinstance(result, EvalRunResult)
    assert result.called_tools == ["ontology.lookup", "get_delayed_flights"]
    assert result.evidence_object_ids == ["flight-1832"]  # empty object_id dropped
    assert result.total_tool_rounds == 1
    assert result.success is False  # no after_completion checkpoint yet


# ---------------------------------------------------------------------------
# 2. Multiple rounds, plan state, and evidence dedupe
# ---------------------------------------------------------------------------


def test_multiple_rounds_plan_state_and_evidence_dedupe() -> None:
    round_one = _after_tool_row(
        results=[{"tool_call_id": "tc-1", "tool_name": "update_plan", "result": {"ok": True}}],
        evidence=[{"evidence_id": "ev-0001", "source": "ontology.lookup", "object_id": "flight-1832"}],
        timestamp=1000.0,
    )
    round_two = _after_tool_row(
        results=[{"tool_call_id": "tc-2", "tool_name": "ontology.lookup", "result": {"object_id": "flight-1832"}}],
        evidence=[{"evidence_id": "ev-0002", "source": "ontology.lookup", "object_id": "flight-1832"}],
        plan_state={"description": "查询航班状态", "steps": []},
        timestamp=1002.0,
    )

    result = build_eval_result_from_checkpoints([round_one, round_two])

    assert result.total_tool_rounds == 2
    assert result.plan_present is True
    assert result.evidence_object_ids == ["flight-1832"]  # deduped, order kept


# ---------------------------------------------------------------------------
# 3. after_completion marks success and feeds answer extraction
# ---------------------------------------------------------------------------


def test_completion_marks_success_and_answer_ids_extracted() -> None:
    rows = [
        _after_tool_row(results=[{"tool_call_id": "tc-1", "tool_name": "ontology.lookup", "result": {}}]),
        _completion_row("航班CA1832当前状态为延误，证据见flight-1832。"),
    ]

    result = build_eval_result_from_checkpoints(rows)

    assert result.success is True
    assert result.agent_response.startswith("航班CA1832")
    assert result.extracted_ids == ["CA1832", "flight-1832"]
    assert result.duration_ms == 4500  # 1004.5 - 1000.0 seconds


# ---------------------------------------------------------------------------
# 4. Governance-blocked results count as unauthorized attempts
# ---------------------------------------------------------------------------


def test_blocked_results_count_as_unauthorized_attempts() -> None:
    row = _after_tool_row(
        results=[
            {"tool_call_id": "tc-1", "tool_name": "sql_query_readonly", "error": "blocked",
             "blocked_by": "template", "rule": "QueryOpsDenySql"},
            {"tool_call_id": "tc-2", "tool_name": "ontology.lookup", "result": {}},
        ],
    )

    result = build_eval_result_from_checkpoints([row])

    assert result.unauthorized_attempts == 1
    assert "sql_query_readonly" in result.called_tools


# ---------------------------------------------------------------------------
# 5. Snapshots may arrive as JSON strings (asyncpg jsonb without codec)
# ---------------------------------------------------------------------------


def test_snapshot_as_json_string_is_decoded() -> None:
    row = _after_tool_row(results=[{"tool_call_id": "tc-1", "tool_name": "ontology.lookup", "result": {}}])
    row["snapshot"] = json.dumps(row["snapshot"])

    result = build_eval_result_from_checkpoints([row])

    assert result.called_tools == ["ontology.lookup"]


# ---------------------------------------------------------------------------
# 6. ingest_run_from_ledger reads ai_run_checkpoints by run_id
# ---------------------------------------------------------------------------


class _LedgerConn:
    def __init__(self, rows: list[dict], queries: list[tuple[str, tuple]]):
        self._rows = rows
        self._queries = queries

    async def fetch(self, query: str, *args):
        self._queries.append((query, args))
        return self._rows

    async def execute(self, query: str, *args) -> str:
        return "OK"


class _LedgerPool:
    def __init__(self, rows: list[dict]):
        self.rows = rows
        self.queries: list[tuple[str, tuple]] = []

    def acquire(self):
        pool = self

        class _Ctx:
            async def __aenter__(self_inner):
                return _LedgerConn(pool.rows, pool.queries)

            async def __aexit__(self_inner, *exc):
                return False

        return _Ctx()


@pytest.mark.asyncio
async def test_ingest_run_from_ledger_reads_checkpoints_table() -> None:
    rows = [
        _after_tool_row(
            results=[{"tool_call_id": "tc-1", "tool_name": "ontology.lookup", "result": {}}],
            evidence=[{"evidence_id": "ev-0001", "source": "ontology.lookup", "object_id": "flight-1832"}],
        ),
        _completion_row(),
    ]
    pool = _LedgerPool(rows)
    service = EvaluationService(db_pool=pool)

    result = await service.ingest_run_from_ledger("run-1")

    query, args = pool.queries[0]
    assert "ai_run_checkpoints" in query
    assert "run_id" in query
    assert args == ("run-1",)
    assert result.success is True
    assert result.called_tools == ["ontology.lookup"]
    assert result.evidence_object_ids == ["flight-1832"]


@pytest.mark.asyncio
async def test_ingest_fails_closed_when_run_has_no_checkpoints() -> None:
    service = EvaluationService(db_pool=_LedgerPool([]))

    with pytest.raises(LookupError):
        await service.ingest_run_from_ledger("run-missing")
