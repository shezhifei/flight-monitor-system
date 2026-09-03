"""Freshness PostToolUse hook (Task H1).

Read-only query tools must carry fresh, timestamped evidence. A result
without ``as_of`` (or older than the per-tool threshold from
``shadow_mode_config.TOOL_FRESHNESS_LIMITS``) is rewritten in place to
``{ok: false, error_code: EVIDENCE_STALE, ...}`` so the model sees the
failure and can retry; the hook itself still returns True and records the
failure in the run's working-memory evidence chain.
"""

from __future__ import annotations

from datetime import UTC, datetime, timedelta

import pytest

from src.infrastructure.ai.hooks.pipeline import FreshnessCheckHook, HookContext
from src.infrastructure.ai.templates import shadow_mode_config
from src.infrastructure.ai.working_memory import WorkingMemory


def _ctx(
    *,
    tool_name: str,
    tool_result: dict,
    tool_args: dict | None = None,
    working_memory: WorkingMemory | None = None,
) -> HookContext:
    return HookContext(
        phase="PostToolUse",
        run_id="run-freshness",
        tool_name=tool_name,
        tool_args=tool_args,
        tool_result=tool_result,
        working_memory=working_memory,
    )


def _fresh_as_of() -> str:
    return datetime.now(UTC).isoformat()


# ---------------------------------------------------------------------------
# Non-query tools are never gated
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_non_query_tools_are_skipped() -> None:
    hook = FreshnessCheckHook()
    for tool_name in ("update_plan", "ontology.propose_action", "load_skill"):
        result: dict = {"anything": "no as_of"}
        ctx = _ctx(tool_name=tool_name, tool_result=result)
        assert await hook.execute(ctx) is True
        assert result == {"anything": "no as_of"}


# ---------------------------------------------------------------------------
# Missing as_of → EVIDENCE_STALE rewrite + evidence failure record
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_missing_as_of_is_rewritten_and_recorded() -> None:
    hook = FreshnessCheckHook()
    memory = WorkingMemory(run_id="run-freshness")
    result: dict = {"flight_id": "CA1832", "status": "delayed", "source": "ai_query.v_flights"}
    ctx = _ctx(tool_name="flight_status_lookup", tool_result=result, working_memory=memory)

    assert await hook.execute(ctx) is True

    assert result["ok"] is False
    assert result["error_code"] == "EVIDENCE_STALE"
    assert "missing as_of" in result["detail"]
    # The stale payload must not survive the rewrite.
    assert "flight_id" not in result

    evidence = memory.read_evidence()
    assert len(evidence) == 1
    assert evidence[0]["source"] == "flight_status_lookup"
    assert "EVIDENCE_STALE" in evidence[0]["summary"]


@pytest.mark.asyncio
async def test_missing_as_of_without_working_memory_still_rewrites() -> None:
    hook = FreshnessCheckHook()
    result: dict = {"flights": []}
    ctx = _ctx(tool_name="get_delayed_flights", tool_result=result)
    assert await hook.execute(ctx) is True
    assert result["error_code"] == "EVIDENCE_STALE"


@pytest.mark.asyncio
async def test_as_of_inside_evidence_block_is_accepted() -> None:
    hook = FreshnessCheckHook()
    result: dict = {
        "entity": {"flight_id": "CA1832"},
        "evidence": {"source": "ontology.lookup", "as_of": _fresh_as_of()},
    }
    ctx = _ctx(
        tool_name="ontology.lookup",
        tool_args={"entity_id": "flight:CA1832"},
        tool_result=result,
    )
    assert await hook.execute(ctx) is True
    assert "error_code" not in result


# ---------------------------------------------------------------------------
# Per-tool thresholds (shared dict with shadow mode)
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_stale_flight_lookup_is_rejected_with_threshold_details() -> None:
    hook = FreshnessCheckHook()
    stale = (datetime.now(UTC) - timedelta(seconds=31)).isoformat()
    result: dict = {"status": "on_time", "as_of": stale}
    ctx = _ctx(tool_name="flight_status_lookup", tool_result=result)

    assert await hook.execute(ctx) is True
    assert result["ok"] is False
    assert result["error_code"] == "EVIDENCE_STALE"
    assert result["freshness_seconds"] >= 31
    assert result["max_age"] == 30


@pytest.mark.asyncio
async def test_ontology_lookup_uses_entity_namespace_thresholds() -> None:
    hook = FreshnessCheckHook()
    stale = (datetime.now(UTC) - timedelta(seconds=11)).isoformat()

    # stand lookups are stricter than flight lookups (10s vs 30s).
    stand_result: dict = {"evidence": {"as_of": stale}}
    stand_ctx = _ctx(
        tool_name="ontology.lookup",
        tool_args={"entity_id": "stand:A12"},
        tool_result=stand_result,
    )
    assert await hook.execute(stand_ctx) is True
    assert stand_result["error_code"] == "EVIDENCE_STALE"
    assert stand_result["max_age"] == 10

    # The same age is fine for a flight lookup.
    flight_result: dict = {"evidence": {"as_of": stale}}
    flight_ctx = _ctx(
        tool_name="ontology.lookup",
        tool_args={"entity_id": "flight:CA1832"},
        tool_result=flight_result,
    )
    assert await hook.execute(flight_ctx) is True
    assert "error_code" not in flight_result


@pytest.mark.asyncio
async def test_fresh_result_passes_untouched() -> None:
    hook = FreshnessCheckHook()
    result: dict = {
        "entity": {"flight_id": "CA1832"},
        "evidence": {"source": "ontology.lookup", "as_of": _fresh_as_of()},
    }
    ctx = _ctx(
        tool_name="ontology.lookup",
        tool_args={"entity_id": "flight:CA1832"},
        tool_result=result,
    )
    assert await hook.execute(ctx) is True
    assert result["entity"] == {"flight_id": "CA1832"}


# ---------------------------------------------------------------------------
# The shared threshold dict is keyed by real production tool names
# ---------------------------------------------------------------------------


def test_freshness_limits_are_keyed_by_real_tool_names() -> None:
    limits = shadow_mode_config.TOOL_FRESHNESS_LIMITS
    # The old shadow-only key must be gone.
    assert "flights.lookup" not in limits
    # Real production query surfaces.
    for key in ("flight_status_lookup", "get_delayed_flights", "ontology.lookup.flight"):
        assert key in limits
    assert limits["ontology.lookup.flight"] == 30
    assert limits["ontology.lookup.stand"] == 10
    assert shadow_mode_config.resolve_freshness_limit("ontology.lookup", {"entity_id": "dispatch:DO-1"}) == 60
    assert shadow_mode_config.resolve_freshness_limit("get_dispatch_by_flight") == 60
