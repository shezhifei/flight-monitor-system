"""Eval jobs survive a process restart (Task G5).

Acceptance: "写 job → 新 service 实例读到同一 row". There is no in-memory
job registry anymore — a fresh :class:`EvaluationService` bound to the same
database must see every job, gate, and status transition written by a
previous instance. Runs against the real test database (``db_pool``).
"""

from __future__ import annotations

import uuid

import pytest

from src.application.services.ai.llm_eval_service import (
    EvaluationService,
    GateMetricsSummary,
)


@pytest.mark.asyncio
async def test_job_written_by_one_instance_is_read_by_a_fresh_one(db_pool) -> None:
    writer = EvaluationService(db_pool=db_pool)
    job = await writer.create_job(
        name="restart-check",
        dataset_path="docs/fixtures/agent_query_ops_eval.jsonl",
        metrics_config={"tool_accuracy_min": 0.95},
        description="Task G5 persistence check",
    )

    # Simulated process restart: a brand-new service instance, same database.
    reader = EvaluationService(db_pool=db_pool)

    items = await reader.list_jobs(limit=10)
    assert any(item["job_id"] == str(job.job_id) for item in items)

    detail = await reader.get_job_detail(job.job_id)
    assert detail is not None
    assert detail["name"] == "restart-check"
    assert detail["dataset_path"] == "docs/fixtures/agent_query_ops_eval.jsonl"
    assert detail["status"] == "pending"
    assert detail["metrics_config"] == {"tool_accuracy_min": 0.95}
    assert detail["gates"] == []


@pytest.mark.asyncio
async def test_gates_written_by_one_instance_are_read_by_a_fresh_one(db_pool) -> None:
    writer = EvaluationService(db_pool=db_pool)
    job = await writer.create_job(name="gate-check", dataset_path="", metrics_config={})
    await writer._persist_gate_metric(
        GateMetricsSummary(
            job_id=job.job_id,
            metric_name="tool_accuracy",
            value=1.0,
            threshold=0.95,
            status="pass",
            details={"direction": "minimum_required"},
        )
    )

    reader = EvaluationService(db_pool=db_pool)
    detail = await reader.get_job_detail(job.job_id)
    assert detail is not None
    assert [gate["metric_name"] for gate in detail["gates"]] == ["tool_accuracy"]
    assert detail["gates"][0]["status"] == "pass"
    assert detail["gates"][0]["value"] == pytest.approx(1.0)


@pytest.mark.asyncio
async def test_status_transition_from_a_fresh_instance(db_pool) -> None:
    writer = EvaluationService(db_pool=db_pool)
    job = await writer.create_job(name="cancel-check", dataset_path="", metrics_config={})

    reader = EvaluationService(db_pool=db_pool)
    cancelled = await reader.cancel_job(job.job_id)
    assert cancelled is not None
    assert cancelled["status"] == "failed"
    assert cancelled["error_message"] == "cancelled by user"

    # A second instance observes the transition too; cancelling again is a no-op.
    verifier = EvaluationService(db_pool=db_pool)
    detail = await verifier.get_job_detail(job.job_id)
    assert detail is not None
    assert detail["status"] == "failed"
    assert await verifier.cancel_job(job.job_id) is None


@pytest.mark.asyncio
async def test_get_job_detail_returns_none_for_unknown_job(db_pool) -> None:
    service = EvaluationService(db_pool=db_pool)
    assert await service.get_job_detail(uuid.uuid4()) is None
