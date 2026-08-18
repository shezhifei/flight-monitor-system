"""EvaluationService runs through an injectable agent runner (Task G2).

The E3 rewrite deletes the old "assemble a dict and assert" hollow tests and
locks the behaviors that make the eval harness falsifiable:

1. ``run_job`` executes dataset samples through the injected
   :class:`EvalAgentRunner` — the runner receives the sample's
   user_query/task_type/entity_id and its structured result lands on the span.
2. The production assembly path fails closed without a runner: it must never
   return ``{"success": True}``.
3. The JSONL dataset loader reads real fixture lines.
4. :class:`RuntimeServiceEvalRunner` folds ``stream_run_with_tools`` SSE
   events into an :class:`EvalRunResult` (no real LLM involved).
"""

from __future__ import annotations

import json
from typing import Any

import pytest

from src.application.services.ai.llm_eval_service import (
    EvalRunnerUnavailableError,
    EvalRunResult,
    EvaluationService,
    RuntimeServiceEvalRunner,
)


# ---------------------------------------------------------------------------
# Fakes
# ---------------------------------------------------------------------------


class _FakeConn:
    def __init__(self, store: dict[str, Any]):
        self._store = store

    async def execute(self, query: str, *args) -> str:
        if "ai_eval_spans" in query:
            self._store["spans"].append(args)
        elif "ai_eval_jobs" in query:
            self._store["job_writes"].append((query.split()[0], args))
        elif "ai_eval_metrics_summary" in query:
            self._store["gates"].append(args)
        return "OK"

    async def fetch(self, query: str, *args) -> list:
        return []

    async def fetchrow(self, query: str, *args):
        return None


class _FakePool:
    def __init__(self) -> None:
        self.store: dict[str, Any] = {"spans": [], "job_writes": [], "gates": []}

    def acquire(self):
        pool = self

        class _Ctx:
            async def __aenter__(self_inner):
                return _FakeConn(pool.store)

            async def __aexit__(self_inner, *exc):
                return False

        return _Ctx()


class _FakeRunner:
    def __init__(self, result: EvalRunResult) -> None:
        self._result = result
        self.calls: list[dict[str, str]] = []

    async def run(self, *, user_query: str, task_type: str, entity_id: str) -> EvalRunResult:
        self.calls.append({"user_query": user_query, "task_type": task_type, "entity_id": entity_id})
        return self._result


def _result(**overrides: Any) -> EvalRunResult:
    base = dict(
        success=True,
        agent_response="航班CA1832当前状态为延误。",
        called_tools=["ontology.lookup", "search_flights_advanced"],
        evidence_object_ids=["flight-1832"],
        extracted_ids=["CA1832"],
        total_tool_rounds=2,
        plan_present=False,
        unauthorized_attempts=0,
        tokens={"total_tokens": 120},
        duration_ms=350,
    )
    base.update(overrides)
    return EvalRunResult(**base)


def _dataset(tmp_path) -> str:
    path = tmp_path / "dataset.jsonl"
    lines = [
        {"id": "query-delay-001", "task_type": "query_ops", "entity_id": "default",
         "user_query": "今天延误超过30分钟的航班有哪些？",
         "expected": {"allowed_tools": ["get_delayed_flights"], "forbidden_tools": ["sql_query_readonly"],
                      "required_object_ids": [], "plan_required": False}},
        {"id": "query-status-002", "task_type": "query_ops", "entity_id": "default",
         "user_query": "航班CA1832现在的状态是什么？",
         "expected": {"allowed_tools": ["ontology.lookup"], "forbidden_tools": ["sql_query_readonly"],
                      "required_object_ids": [], "plan_required": False}},
    ]
    path.write_text("\n".join(json.dumps(line, ensure_ascii=False) for line in lines), encoding="utf-8")
    return str(path)


# ---------------------------------------------------------------------------
# 1. run_job goes through the injected runner
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_run_job_executes_samples_through_injected_runner(tmp_path) -> None:
    runner = _FakeRunner(_result())
    service = EvaluationService(db_pool=_FakePool(), agent_runner=runner)

    job = await service.create_job(
        name="g2-runner-test",
        dataset_path=_dataset(tmp_path),
        metrics_config={"tool_accuracy_min": 0.95},
    )
    finished = await service.run_job(job)

    assert [call["user_query"] for call in runner.calls] == [
        "今天延误超过30分钟的航班有哪些？",
        "航班CA1832现在的状态是什么？",
    ]
    assert runner.calls[0]["task_type"] == "query_ops"
    assert runner.calls[0]["entity_id"] == "default"
    assert finished.total_runs == 2
    assert finished.completed_runs == 2


@pytest.mark.asyncio
async def test_runner_result_lands_on_the_span(tmp_path) -> None:
    pool = _FakePool()
    runner = _FakeRunner(_result())
    service = EvaluationService(db_pool=pool, agent_runner=runner)

    job = await service.create_job(
        name="g2-span-test",
        dataset_path=_dataset(tmp_path),
        metrics_config={},
    )
    await service.run_job(job)

    assert len(pool.store["spans"]) == 2
    first_span = pool.store["spans"][0]
    persisted_result = json.loads(first_span[8])  # result column
    assert persisted_result["called_tools"] == ["ontology.lookup", "search_flights_advanced"]
    assert persisted_result["evidence_object_ids"] == ["flight-1832"]
    persisted_metrics = json.loads(first_span[9])
    assert persisted_metrics["total_tool_rounds"] == 2
    assert persisted_metrics["constraint_violations"] == 0


# ---------------------------------------------------------------------------
# 2. Fail-closed without a runner
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_missing_runner_fails_closed_instead_of_faking_success(tmp_path) -> None:
    service = EvaluationService(db_pool=_FakePool(), agent_runner=None)

    job = await service.create_job(
        name="g2-fail-closed",
        dataset_path=_dataset(tmp_path),
        metrics_config={},
    )
    with pytest.raises(EvalRunnerUnavailableError):
        await service.run_job(job)

    assert job.status == "failed"
    assert "no agent runner" in (job.error_message or "")


@pytest.mark.asyncio
async def test_run_agent_on_query_never_returns_stub_success() -> None:
    service = EvaluationService(db_pool=_FakePool(), agent_runner=None)
    with pytest.raises(EvalRunnerUnavailableError):
        await service._run_agent_on_query(
            user_query="今天延误航班？", task_type="query_ops", entity_id="default"
        )


# ---------------------------------------------------------------------------
# 3. Dataset loader reads real JSONL
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_load_test_dataset_reads_jsonl_lines(tmp_path) -> None:
    service = EvaluationService(db_pool=_FakePool())
    samples = await service._load_test_dataset(_dataset(tmp_path))
    assert [sample["id"] for sample in samples] == ["query-delay-001", "query-status-002"]


@pytest.mark.asyncio
async def test_load_test_dataset_missing_file_raises() -> None:
    service = EvaluationService(db_pool=_FakePool())
    with pytest.raises(FileNotFoundError):
        await service._load_test_dataset("does/not/exist.jsonl")


# ---------------------------------------------------------------------------
# 4. RuntimeServiceEvalRunner folds SSE events (no real LLM)
# ---------------------------------------------------------------------------


class _FakeRuntimeService:
    def __init__(self, events: list[dict[str, Any]]) -> None:
        self._events = events
        self.envelopes = []

    async def stream_run_with_tools(self, envelope):
        self.envelopes.append(envelope)
        for event in self._events:
            yield event


@pytest.mark.asyncio
async def test_runtime_runner_collects_tools_evidence_and_tokens() -> None:
    runtime = _FakeRuntimeService(
        [
            {"event": "tool.call", "data": {"tool_name": "ontology.lookup"}},
            {"event": "tool.result", "data": {}},
            {"event": "tool.call", "data": {"tool_name": "update_plan"}},
            {"event": "tool.result", "data": {}},
            {
                "event": "run.complete",
                "data": {
                    "answer": "航班CA1832延误，证据见flight-1832。",
                    "evidence": [{"source": "ontology.lookup", "object_type": "Flight", "object_id": "flight-1832"}],
                    "token_usage": {"prompt_tokens": 100, "completion_tokens": 20, "total_tokens": 120},
                },
            },
        ]
    )
    runner = RuntimeServiceEvalRunner(runtime)

    result = await runner.run(user_query="CA1832状态？", task_type="query_ops", entity_id="default")

    assert result.success is True
    assert result.called_tools == ["ontology.lookup", "update_plan"]
    assert result.total_tool_rounds == 2
    assert result.plan_present is True
    assert result.evidence_object_ids == ["flight-1832"]
    assert "CA1832" in result.extracted_ids
    assert result.tokens["total_tokens"] == 120
    envelope = runtime.envelopes[0]
    assert envelope.task.task_type == "query_ops"
    assert envelope.task.user_message == "CA1832状态？"


@pytest.mark.asyncio
async def test_runtime_runner_marks_failed_runs() -> None:
    runtime = _FakeRuntimeService(
        [{"event": "run.fail", "data": {"answer": "ONTOLOGY_CLIENT_NOT_CONFIGURED", "error_code": "X"}}]
    )
    runner = RuntimeServiceEvalRunner(runtime)

    result = await runner.run(user_query="随便", task_type="query_ops", entity_id="default")

    assert result.success is False
    assert result.called_tools == []
    assert result.plan_present is False


def test_extract_answer_ids_covers_flight_numbers_and_prefixed_ids() -> None:
    from src.application.services.ai.llm_eval_service.service import extract_answer_ids

    ids = extract_answer_ids("CA1832 已停在 stand-12，关联 dispatch-9 与 flight-77。")
    assert set(ids) == {"CA1832", "stand-12", "dispatch-9", "flight-77"}
