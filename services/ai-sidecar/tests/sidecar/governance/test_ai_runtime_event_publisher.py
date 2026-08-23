"""Tests for the AI runtime event MQ publisher client."""

from __future__ import annotations

import httpx
import pytest

from src.infrastructure.ai.messaging import (
    AI_RUNTIME_EVENTS_TOPIC,
    SCHEMA_VERSION,
    AiRuntimeEventPublisher,
    AiRuntimeEventPublishError,
    PublishConfig,
    build_checkpoint,
    build_heartbeat,
    build_run_complete,
    build_run_fail,
    build_tool_call_requested,
    build_tool_result,
)


def _patch_transport(monkeypatch, handler):
    real_async_client = httpx.AsyncClient

    def _factory(*args, **kwargs):
        kwargs["transport"] = httpx.MockTransport(handler)
        return real_async_client(*args, **kwargs)

    monkeypatch.setattr("src.infrastructure.ai.messaging.ai_runtime_event_publisher.httpx.AsyncClient", _factory)


def _make_publisher(monkeypatch, handler, **config_kwargs) -> AiRuntimeEventPublisher:
    _patch_transport(monkeypatch, handler)
    config = PublishConfig(
        base_url="https://gw.example",
        timeout=2.0,
        **config_kwargs,
    )
    return AiRuntimeEventPublisher(config)


def test_topic_constant() -> None:
    assert AI_RUNTIME_EVENTS_TOPIC == "ai_runtime_events"


def test_build_tool_call_requested_envelope_shape() -> None:
    envelope = build_tool_call_requested(
        run_id="run-1",
        job_id="job-1",
        round_index=0,
        event_sequence=1,
        idempotency_key="run-1:0:call-1:flight_status_lookup:abc",
        tool_call_pk="tpc-1",
        tool_call_id="call-1",
        tool_name="flight_status_lookup",
        tool_type="builtin",
        args_hash="abc",
        args_summary={"flight_id": "CA1234"},
        authorization_mode="rust_pdp",
    )
    assert envelope["event_type"] == "tool_call_requested"
    assert envelope["event_id"]
    assert envelope["run_id"] == "run-1"
    assert envelope["job_id"] == "job-1"
    assert envelope["round_index"] == 0
    assert envelope["event_sequence"] == 1
    assert envelope["idempotency_key"] == "run-1:0:call-1:flight_status_lookup:abc"
    assert envelope["schema_version"] == SCHEMA_VERSION
    assert envelope["payload"]["tool_call_pk"] == "tpc-1"
    assert envelope["payload"]["tool_call_id"] == "call-1"
    assert envelope["payload"]["tool_name"] == "flight_status_lookup"
    assert envelope["payload"]["tool_type"] == "builtin"
    assert envelope["payload"]["authorization_mode"] == "rust_pdp"
    assert envelope["payload"]["args_hash"] == "abc"


def test_build_checkpoint_envelope_shape() -> None:
    envelope = build_checkpoint(
        run_id="run-1",
        job_id="job-1",
        round_index=2,
        event_sequence=7,
        idempotency_key="ckpt-1",
        checkpoint_id="ckpt-1",
        sequence_no=7,
        checkpoint_type="before_tool",
        snapshot_hash="snap-hash",
        snapshot={"tool_call_id": "call-1"},
        snapshot_size_bytes=42,
    )
    assert envelope["event_type"] == "checkpoint"
    assert envelope["event_sequence"] == 7
    assert envelope["payload"]["checkpoint_type"] == "before_tool"
    assert envelope["payload"]["snapshot_size_bytes"] == 42


def test_build_heartbeat_envelope_shape() -> None:
    envelope = build_heartbeat(
        run_id="run-1",
        job_id="job-1",
        round_index=0,
        event_sequence=3,
        idempotency_key="run-1:0:call-1:long_tool:hash",
        tool_call_pk="tpc-1",
        progress_pct=42,
    )
    assert envelope["event_type"] == "heartbeat"
    assert envelope["payload"]["tool_call_pk"] == "tpc-1"
    assert envelope["payload"]["progress_pct"] == 42


def test_build_run_complete_envelope_shape() -> None:
    envelope = build_run_complete(
        run_id="run-1",
        job_id="job-1",
        round_index=4,
        event_sequence=99,
        idempotency_key="run-complete-1",
        output_raw={"text": "ok"},
        proposal_ids=["p-1"],
    )
    assert envelope["event_type"] == "run_complete"
    assert envelope["payload"]["output_raw"] == {"text": "ok"}
    assert envelope["payload"]["proposal_ids"] == ["p-1"]


def test_build_run_fail_envelope_shape() -> None:
    envelope = build_run_fail(
        run_id="run-1",
        job_id="job-1",
        round_index=4,
        event_sequence=99,
        idempotency_key="run-fail-1",
        error_code="MODEL_TIMEOUT",
        error_message="stream aborted",
    )
    assert envelope["event_type"] == "run_fail"
    assert envelope["payload"]["error_code"] == "MODEL_TIMEOUT"
    assert envelope["payload"]["error_message"] == "stream aborted"
    assert "blocked_by" not in envelope["payload"]
    assert "rule" not in envelope["payload"]
    assert "detail" not in envelope["payload"]


def test_build_run_fail_includes_optional_block_fields() -> None:
    envelope = build_run_fail(
        run_id="run-1",
        job_id="job-1",
        round_index=0,
        event_sequence=1,
        idempotency_key="run-fail-snapshot",
        error_code="AI_TOOL_SNAPSHOT_MISSING",
        error_message="no resolved tool snapshot for this run",
        blocked_by="snapshot",
        rule="AI_TOOL_SNAPSHOT_MISSING",
        detail="no resolved tool snapshot for this run",
    )
    assert envelope["payload"]["blocked_by"] == "snapshot"
    assert envelope["payload"]["rule"] == "AI_TOOL_SNAPSHOT_MISSING"
    assert envelope["payload"]["detail"] == "no resolved tool snapshot for this run"


def test_build_tool_result_envelope_shape() -> None:
    envelope = build_tool_result(
        run_id="run-1",
        job_id="job-1",
        round_index=0,
        event_sequence=2,
        idempotency_key="run-1:0:call-1:lookup:hash",
        tool_call_pk="tpc-1",
        tool_call_id="call-1",
        tool_name="flight_status_lookup",
        status="succeeded",
        duration_ms=120,
        result_summary={"status": "ON_TIME"},
    )
    assert envelope["event_type"] == "tool_result"
    assert envelope["payload"]["status"] == "succeeded"
    assert envelope["payload"]["duration_ms"] == 120


@pytest.mark.asyncio
async def test_publish_sends_envelope_to_gateway_and_uses_run_id_as_message_key(monkeypatch) -> None:
    captured: dict = {}

    def handler(request: httpx.Request) -> httpx.Response:
        captured["url"] = str(request.url)
        captured["body"] = request.content
        captured["auth"] = request.headers.get("Authorization")
        return httpx.Response(200, json={"message_id": "MQ-12345"})

    publisher = _make_publisher(monkeypatch, handler, api_key="sk-test")
    envelope = build_tool_call_requested(
        run_id="run-1",
        job_id="job-1",
        round_index=0,
        event_sequence=1,
        idempotency_key="idem-1",
        tool_call_pk="tpc-1",
        tool_call_id="call-1",
        tool_name="flight_status_lookup",
        tool_type="builtin",
        args_hash="abc",
        args_summary={},
        authorization_mode="rust_pdp",
    )
    message_id = await publisher.publish(envelope)

    assert message_id == "MQ-12345"
    assert captured["url"] == "https://gw.example/mq/messages"
    assert captured["auth"] == "Bearer sk-test"
    body = captured["body"].decode("utf-8")
    assert '"topic":"ai_runtime_events"' in body
    assert '"tag":"tool.call.requested"' in body
    assert '"message_key":"run-1"' in body
    assert '"event_type":"tool_call_requested"' in body
    assert '"schema_version":1' in body


@pytest.mark.asyncio
async def test_publish_falls_back_to_header_for_message_id(monkeypatch) -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        return httpx.Response(200, headers={"X-MQ-Message-Id": "fallback-1"})

    publisher = _make_publisher(monkeypatch, handler)
    envelope = build_checkpoint(
        run_id="run-1",
        job_id="job-1",
        round_index=0,
        event_sequence=1,
        idempotency_key="idem-1",
        checkpoint_id="ckpt-1",
        sequence_no=1,
        checkpoint_type="run_input",
        snapshot_hash="h",
        snapshot={},
        snapshot_size_bytes=2,
    )
    assert await publisher.publish(envelope) == "fallback-1"


@pytest.mark.asyncio
async def test_publish_retries_on_transient_5xx(monkeypatch) -> None:
    attempts = {"count": 0}

    def handler(request: httpx.Request) -> httpx.Response:
        attempts["count"] += 1
        if attempts["count"] < 3:
            return httpx.Response(503, text="busy")
        return httpx.Response(200, json={"message_id": "ok-1"})

    publisher = _make_publisher(
        monkeypatch,
        handler,
        max_retries=3,
        backoff_seconds=0.001,
    )
    envelope = build_tool_call_requested(
        run_id="run-1",
        job_id="job-1",
        round_index=0,
        event_sequence=1,
        idempotency_key="idem-1",
        tool_call_pk="tpc-1",
        tool_call_id="call-1",
        tool_name="t",
        tool_type="builtin",
        args_hash="h",
        args_summary={},
        authorization_mode="rust_pdp",
    )
    assert await publisher.publish(envelope) == "ok-1"
    assert attempts["count"] == 3


@pytest.mark.asyncio
async def test_publish_retries_on_transport_error(monkeypatch) -> None:
    attempts = {"count": 0}

    def handler(request: httpx.Request) -> httpx.Response:
        attempts["count"] += 1
        if attempts["count"] < 2:
            raise httpx.ConnectError("boom", request=request)
        return httpx.Response(200, json={"message_id": "ok-2"})

    publisher = _make_publisher(
        monkeypatch,
        handler,
        max_retries=3,
        backoff_seconds=0.001,
    )
    envelope = build_tool_call_requested(
        run_id="run-1",
        job_id="job-1",
        round_index=0,
        event_sequence=1,
        idempotency_key="idem-1",
        tool_call_pk="tpc-1",
        tool_call_id="call-1",
        tool_name="t",
        tool_type="builtin",
        args_hash="h",
        args_summary={},
        authorization_mode="rust_pdp",
    )
    assert await publisher.publish(envelope) == "ok-2"
    assert attempts["count"] == 2


@pytest.mark.asyncio
async def test_publish_raises_after_exhausting_retries(monkeypatch) -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        return httpx.Response(503, text="busy")

    publisher = _make_publisher(
        monkeypatch,
        handler,
        max_retries=2,
        backoff_seconds=0.001,
    )
    envelope = build_tool_call_requested(
        run_id="run-1",
        job_id="job-1",
        round_index=0,
        event_sequence=1,
        idempotency_key="idem-1",
        tool_call_pk="tpc-1",
        tool_call_id="call-1",
        tool_name="t",
        tool_type="builtin",
        args_hash="h",
        args_summary={},
        authorization_mode="rust_pdp",
    )
    with pytest.raises(AiRuntimeEventPublishError, match="AI_MQ_HTTP_503"):
        await publisher.publish(envelope)


@pytest.mark.asyncio
async def test_publish_raises_on_4xx_without_retry(monkeypatch) -> None:
    attempts = {"count": 0}

    def handler(request: httpx.Request) -> httpx.Response:
        attempts["count"] += 1
        return httpx.Response(400, text="bad request")

    publisher = _make_publisher(
        monkeypatch,
        handler,
        max_retries=3,
        backoff_seconds=0.001,
    )
    envelope = build_tool_call_requested(
        run_id="run-1",
        job_id="job-1",
        round_index=0,
        event_sequence=1,
        idempotency_key="idem-1",
        tool_call_pk="tpc-1",
        tool_call_id="call-1",
        tool_name="t",
        tool_type="builtin",
        args_hash="h",
        args_summary={},
        authorization_mode="rust_pdp",
    )
    with pytest.raises(AiRuntimeEventPublishError, match="AI_MQ_HTTP_400"):
        await publisher.publish(envelope)
    assert attempts["count"] == 1
