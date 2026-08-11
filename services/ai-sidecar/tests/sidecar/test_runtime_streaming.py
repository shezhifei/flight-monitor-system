"""Unit tests for RuntimeService.stream_run (P2.0 real token streaming)."""

from __future__ import annotations

import asyncio
import os

import pytest

from src.infrastructure.ai.context_envelope import (
    ContextEnvelope,
    EnvelopeContext,
    EnvelopeLimits,
    EnvelopeObject,
    EnvelopeOntology,
    EnvelopeRequester,
    EnvelopeTask,
)
from src.infrastructure.ai.runtime_llm import FakeStreamingLlmClient, LlmStreamError
from src.infrastructure.ai.runtime_service import (
    STATUS_FAILED,
    STATUS_SUCCEEDED,
    RuntimeService,
    structured_output_to_response_dict,
)


def _sample_envelope(**overrides) -> ContextEnvelope:
    base = ContextEnvelope(
        contract_version="ai-runtime.v1",
        job_id="job_stream_unit",
        run_id="run_stream_unit",
        correlation_id="corr_stream_unit",
        requester=EnvelopeRequester(user_id="user_1", roles=["ai:chat"]),
        ontology=EnvelopeOntology(
            version="flight-ops.v1",
            allowed_object_types=["Flight"],
            allowed_actions=["Flight.add_note"],
            risk_ceiling="medium",
        ),
        context=EnvelopeContext(
            objects=[
                EnvelopeObject(
                    object_type="Flight",
                    object_id="FL123",
                    data={"flight_number": "CA1234", "status": "scheduled"},
                )
            ],
            limits=EnvelopeLimits(),
        ),
        task=EnvelopeTask(task_type="nl_query", user_message="What is the status of CA1234?"),
    )
    for key, value in overrides.items():
        setattr(base, key, value)
    return base


def _collect_events(service: RuntimeService, envelope: ContextEnvelope):
    # stream_run is now an async generator (resolver-aware preamble); collect it
    # synchronously so the existing sync test methods stay unchanged.
    async def _run():
        return [evt async for evt in service.stream_run(envelope)]

    return asyncio.run(_run())


class TestStreamRunMockLlm:
    def test_mock_streaming_emits_tokens_and_run_complete(self):
        tokens = ["航班 ", "CA1234 ", "状态正常"]
        service = RuntimeService(
            llm_client=None,
            streaming_llm_client=FakeStreamingLlmClient(tokens=tokens),
        )
        events = _collect_events(service, _sample_envelope())

        event_types = [e["event"] for e in events]
        assert "progress" in event_types
        assert event_types.count("token") == len(tokens)
        assert "run.complete" in event_types
        assert "transport.abort" not in event_types

        deltas = [e["data"]["delta"] for e in events if e["event"] == "token"]
        assert "".join(deltas) == "".join(tokens)

        complete = next(e for e in events if e["event"] == "run.complete")
        data = complete["data"]
        assert data["status"] == STATUS_SUCCEEDED
        assert data["answer"] == "".join(tokens)
        assert data["run_id"] == "run_stream_unit"
        for key in (
            "contract_version",
            "reasoning_steps",
            "evidence",
            "proposals",
            "limitations",
            "metrics",
        ):
            assert key in data

    def test_mid_stream_error_emits_transport_abort_not_complete(self):
        service = RuntimeService(
            streaming_llm_client=FakeStreamingLlmClient(
                tokens=["part1", "part2", "part3"],
                raise_after_tokens=1,
            ),
        )
        events = _collect_events(service, _sample_envelope())
        event_types = [e["event"] for e in events]

        assert "token" in event_types
        assert "transport.abort" in event_types
        assert "run.complete" not in event_types
        assert "run.fail" not in event_types

        abort = next(e for e in events if e["event"] == "transport.abort")
        assert abort["data"]["run_id"] == "run_stream_unit"
        assert abort["data"]["message"]
        assert "OPENAI_API_KEY" not in abort["data"]["message"]

    def test_pre_stream_error_degrades_to_heuristic_succeeded(self):
        service = RuntimeService(
            streaming_llm_client=FakeStreamingLlmClient(
                tokens=[],
                raise_on_start=LlmStreamError("provider connection refused"),
            ),
        )
        events = _collect_events(service, _sample_envelope())
        complete = next(e for e in events if e["event"] == "run.complete")
        data = complete["data"]
        assert data["status"] == STATUS_SUCCEEDED
        assert data.get("degraded") is True or data["limitations"]
        assert "token" in [e["event"] for e in events]

    def test_invalid_envelope_run_complete_failed_not_500_shape(self):
        service = RuntimeService()
        events = _collect_events(service, _sample_envelope(run_id="", job_id=""))
        complete = next(e for e in events if e["event"] == "run.complete")
        assert complete["data"]["status"] == STATUS_FAILED


class TestStreamRunNoApiKey:
    def test_no_openai_key_heuristic_degraded(self, monkeypatch: pytest.MonkeyPatch):
        monkeypatch.delenv("OPENAI_API_KEY", raising=False)
        service = RuntimeService()
        events = _collect_events(service, _sample_envelope())
        complete = next(e for e in events if e["event"] == "run.complete")
        data = complete["data"]
        assert data["status"] == STATUS_SUCCEEDED
        assert data["limitations"]
        assert data.get("degraded") is True or data["metrics"]["model"] == "heuristic-runtime-v1"
        payload = structured_output_to_response_dict(asyncio.run(service.execute_run(_sample_envelope())))
        assert payload["status"] == STATUS_SUCCEEDED


@pytest.mark.skipif(
    os.environ.get("RUN_LIVE_OPENAI_STREAM_SMOKE", "0") != "1" or not os.environ.get("OPENAI_API_KEY", "").strip(),
    reason="Set RUN_LIVE_OPENAI_STREAM_SMOKE=1 and OPENAI_API_KEY for live OpenAI stream smoke",
)
class TestLiveOpenAiStreamSmoke:
    def test_live_stream_has_tokens_and_complete(self):
        import time

        from .test_live_smoke_summary import build_live_smoke_summary, format_live_smoke_summary

        service = RuntimeService()
        start_time = time.time()
        events = _collect_events(service, _sample_envelope())
        duration_ms = int((time.time() - start_time) * 1000)

        assert any(e["event"] == "token" for e in events)
        complete = next(e for e in events if e["event"] == "run.complete")
        assert complete["data"]["status"] == STATUS_SUCCEEDED

        # Verify content exists but don't print or leak it
        assert "answer" in complete["data"]
        assert len(complete["data"]["answer"]) > 0

        # Gather token usage
        metrics = complete["data"].get("metrics", {})
        token_usage = metrics.get("token_usage", {})
        model_name = metrics.get("model", "unknown")

        summary = build_live_smoke_summary(events, model_name, duration_ms, token_usage)
        formatted = format_live_smoke_summary(summary)

        import os

        summary_path = os.path.join(os.path.dirname(__file__), ".smoke_summary.json")
        with open(summary_path, "w", encoding="utf-8") as f:
            f.write(formatted)
