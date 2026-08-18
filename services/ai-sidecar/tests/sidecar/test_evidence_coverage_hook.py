"""Evidence coverage Stop hook (Task H2).

The final answer of a ``query_ops`` run must not present identifiers that
have no backing evidence: every critical ID (flight number, four-digit
flight number, UUID) extracted from the last assistant text must appear in
the run's working-memory ``evidence.json`` (``object_id`` or ``content``).
Uncovered IDs trigger a fixed Chinese degradation rewrite — the hook locks
the "rewrite" option: it returns False and exposes ``final_text_override``
which the runner must use instead of the original text.
"""

from __future__ import annotations

from types import SimpleNamespace

import pytest

from src.infrastructure.ai.hooks.pipeline import (
    CRITICAL_ID_PATTERNS,
    EvidenceCoverageHook,
    HookContext,
    extract_critical_ids,
)
from src.infrastructure.ai.working_memory import WorkingMemory


def _ctx(
    *,
    final_text: str,
    task_type: str = "query_ops",
    working_memory: WorkingMemory | None = None,
) -> HookContext:
    return HookContext(
        phase="Stop",
        run_id="run-coverage",
        messages=[
            {"role": "user", "content": "查询航班状态"},
            {"role": "assistant", "content": final_text},
        ],
        envelope=SimpleNamespace(task=SimpleNamespace(task_type=task_type)),
        working_memory=working_memory,
    )


# ---------------------------------------------------------------------------
# CRITICAL_ID_PATTERNS covers the real identifier shapes (Task H2 extension)
# ---------------------------------------------------------------------------


def test_patterns_cover_ca_flight_numbers_four_digits_and_uuids() -> None:
    text = (
        "航班 CA1832 与 F1234 延误，四位数字航班 8899，"
        "flight_id 为 3f2a9c1e-8b4d-4a67-9e2f-1c0d5b7a8e91，异常 ANOMALY-GT1。"
    )
    ids = extract_critical_ids([{"role": "assistant", "content": text}])
    assert "CA1832" in ids
    assert "F1234" in ids
    assert "8899" in ids
    assert "3f2a9c1e-8b4d-4a67-9e2f-1c0d5b7a8e91" in ids
    assert "ANOMALY-GT1" in ids
    # Digits glued to letters are not bare four-digit numbers.
    assert "1832" not in ids


def test_patterns_do_not_double_count_letter_prefixed_digits() -> None:
    ids = extract_critical_ids([{"role": "assistant", "content": "MU5102 已起飞"}])
    assert "MU5102" in ids
    assert "5102" not in ids


# ---------------------------------------------------------------------------
# Grounded answers pass; ungrounded answers degrade
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_covered_ids_pass_untouched() -> None:
    memory = WorkingMemory(run_id="run-coverage")
    memory.add_evidence(
        source="ontology.lookup",
        object_id="CA1832",
        summary="flight delayed",
        content='{"flight_id": "CA1832", "status": "delayed"}',
    )
    ctx = _ctx(final_text="航班 CA1832 当前延误 45 分钟。", working_memory=memory)

    hook = EvidenceCoverageHook()
    assert await hook.execute(ctx) is True
    assert ctx.final_text_override is None


@pytest.mark.asyncio
async def test_uncovered_id_triggers_fixed_degradation() -> None:
    memory = WorkingMemory(run_id="run-coverage")
    memory.add_evidence(
        source="ontology.lookup",
        object_id="CA1832",
        summary="flight delayed",
        content='{"flight_id": "CA1832"}',
    )
    # MU5102 has no evidence at all.
    ctx = _ctx(final_text="航班 CA1832 延误；MU5102 正常起飞。", working_memory=memory)

    hook = EvidenceCoverageHook()
    assert await hook.execute(ctx) is False
    assert ctx.final_text_override is not None
    assert "缺少工具证据" in ctx.final_text_override
    assert "MU5102" in ctx.final_text_override
    # The grounded ID must not be degraded away.
    assert "CA1832" not in ctx.final_text_override.replace("缺少工具证据", "")
    assert ctx.errors


@pytest.mark.asyncio
async def test_id_covered_by_evidence_content_counts() -> None:
    memory = WorkingMemory(run_id="run-coverage")
    memory.add_evidence(
        source="get_delayed_flights",
        object_id="",
        summary="delayed list",
        content='{"flights": [{"flight_id": "3f2a9c1e-8b4d-4a67-9e2f-1c0d5b7a8e91"}]}',
    )
    ctx = _ctx(
        final_text="航班 3f2a9c1e-8b4d-4a67-9e2f-1c0d5b7a8e91 延误。",
        working_memory=memory,
    )
    hook = EvidenceCoverageHook()
    assert await hook.execute(ctx) is True


@pytest.mark.asyncio
async def test_no_evidence_at_all_degrades_every_id() -> None:
    ctx = _ctx(final_text="航班 CA1832 已取消。", working_memory=WorkingMemory(run_id="run-coverage"))
    hook = EvidenceCoverageHook()
    assert await hook.execute(ctx) is False
    assert "CA1832" in (ctx.final_text_override or "")


# ---------------------------------------------------------------------------
# Task-type policy: query_ops enforces; ops templates with hypothesis
# paragraphs are left to NoPromisesHook
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_dispatch_ops_and_anomaly_ops_are_not_grounding_gated() -> None:
    hook = EvidenceCoverageHook()
    for task_type in ("dispatch_ops", "anomaly_ops"):
        ctx = _ctx(
            final_text="假设 MU5102 需要调整机位。",
            task_type=task_type,
            working_memory=WorkingMemory(run_id="run-coverage"),
        )
        assert await hook.execute(ctx) is True
        assert ctx.final_text_override is None


@pytest.mark.asyncio
async def test_no_critical_ids_means_nothing_to_check() -> None:
    ctx = _ctx(final_text="今天整体运行平稳，无异常。")
    hook = EvidenceCoverageHook()
    assert await hook.execute(ctx) is True


def test_critical_patterns_include_extended_shapes() -> None:
    joined = "\n".join(CRITICAL_ID_PATTERNS)
    assert "F[0-9]" in joined  # legacy shape preserved
    assert "{8}" in joined  # UUID shape present
