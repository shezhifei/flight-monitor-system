"""Task J4 — Phase J exit-criteria evidence.

Proves, against a fake gateway runner (no real LLM), that:

* ``fms_ai_run_cost_usd`` grows from the price table when a run reports
  token usage;
* ``fms_ai_run_stops_total`` distinguishes ``completed`` from
  ``budget_exhausted`` terminal runs;
* both counters are sliceable by ``task_type`` through the run metric
  context bound around the run.

The value assertions only run when ``prometheus_client`` is installed.
"""

from __future__ import annotations

import pytest

from src.infrastructure.ai.monitoring import prometheus_exporter as exporter
from src.infrastructure.ai.monitoring.model_prices import MODEL_PRICES_PER_1M

PROM = exporter._PROM_AVAILABLE

pytestmark = pytest.mark.skipif(not PROM, reason="prometheus_client not installed")


def _chunk(usage: dict | None, choices: list[dict]):
    from src.infrastructure.ai.openai_client import ChatCompletionChunk

    return ChatCompletionChunk(
        id="j4",
        object="chat.completion.chunk",
        created=0,
        model="gpt-4o",
        choices=choices,
        usage=usage,
    )


def _fake_gateway(usage: dict):
    from unittest.mock import AsyncMock, MagicMock

    gateway = MagicMock()
    gateway.config = MagicMock()
    gateway.config.default_model = "gpt-4o"

    async def async_iter():
        # Real DTO chunks only: the runner's isinstance gate skips mocks, and
        # the usage field must ride the DTO or cost accounting sees 0 tokens.
        yield _chunk(None, [{"index": 0, "delta": {}, "finish_reason": "stop"}])
        yield _chunk(usage, [])

    gateway.chat_completion = AsyncMock(return_value=async_iter())
    return gateway


@pytest.mark.asyncio
async def test_cost_and_stop_counters_grow_in_fake_runner():
    from src.infrastructure.ai.llm_stream_runner import LLMStreamRunner
    from src.infrastructure.ai.openai_client import Message

    cost_labels = dict(task_type="query_ops", entity_id="J4-FLT")
    cost_before = exporter.fms_ai_run_cost_usd.labels(**cost_labels)._value.get()
    stops_before = exporter.fms_ai_run_stops_total.labels(reason="completed")._value.get()
    llm_labels = dict(model="gpt-4o", task_type="query_ops", entity_id="J4-FLT", status="ok")
    llm_before = exporter.fms_ai_llm_calls_total.labels(**llm_labels)._value.get()

    runner = LLMStreamRunner(
        _fake_gateway({"prompt_tokens": 1_000_000, "completion_tokens": 500_000, "total_tokens": 1_500_000})
    )
    with exporter.bind_metric_context(task_type="query_ops", entity_id="J4-FLT"):
        async for _event in runner.stream_chat_with_tools(
            messages=[Message(role="user", content="just answer, no tools")],
            model="gpt-4o",
            tools=[],
            run_id="j4_fake_run_cost",
        ):
            pass

    prompt_price, completion_price = MODEL_PRICES_PER_1M["gpt-4o"]
    expected_cost = prompt_price * 1.0 + completion_price * 0.5
    assert exporter.fms_ai_run_cost_usd.labels(**cost_labels)._value.get() == pytest.approx(cost_before + expected_cost)
    assert exporter.fms_ai_run_stops_total.labels(reason="completed")._value.get() == stops_before + 1
    assert exporter.fms_ai_llm_calls_total.labels(**llm_labels)._value.get() == llm_before + 1


@pytest.mark.asyncio
async def test_budget_exhausted_stop_reason_counted_in_fake_runner():
    from src.infrastructure.ai.llm_stream_runner import LLMStreamRunner
    from src.infrastructure.ai.openai_client import Message

    stops_before = exporter.fms_ai_run_stops_total.labels(reason="budget_exhausted")._value.get()

    class _FailingToolClient:
        """Every round the model requests the same unregistered tool."""

        config = None

        async def chat_completion(self, *args, **kwargs):
            async def _iter():
                yield _chunk(
                    None,
                    [
                        {
                            "index": 0,
                            "delta": {
                                "tool_calls": [
                                    {
                                        "index": 0,
                                        "id": "call-j4",
                                        "type": "function",
                                        "function": {"name": "bad_tool", "arguments": "{}"},
                                    }
                                ]
                            },
                            "finish_reason": "tool_calls",
                        }
                    ],
                )

            return _iter()

    runner = LLMStreamRunner(_FailingToolClient())
    saw_budget_exhausted = False
    async for event in runner.stream_chat_with_tools(
        messages=[Message(role="user", content="fail repeatedly")],
        model="gpt-4o",
        tools=[{"function": {"name": "bad_tool"}}],
        run_id="j4_fake_run_budget",
        consecutive_failure_threshold=2,
    ):
        saw_budget_exhausted = saw_budget_exhausted or event.type == "budget_exhausted"

    assert saw_budget_exhausted
    assert exporter.fms_ai_run_stops_total.labels(reason="budget_exhausted")._value.get() == stops_before + 1
