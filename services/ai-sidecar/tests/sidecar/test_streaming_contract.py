"""SSE streaming contract tests for POST /internal/ai/v1/runs/stream.

Coverage:
- No X-Service-Identity -> 401
- Path mismatch token -> 403
- Valid envelope -> SSE contains progress/token/run.complete
- No OPENAI_API_KEY -> final status=succeeded with degraded/limitations
- Invalid envelope -> final failed event, NOT HTTP 500
"""

from __future__ import annotations

import json
import os
import time
from pathlib import Path
from typing import Any

import jwt
import pytest
from fastapi.testclient import TestClient

_project_root = Path(__file__).parent.parent.parent
os.environ["JWT_SECRET"] = os.environ.get("JWT_SECRET", "test-secret-for-streaming-tests")

from scripts.host.ai_sidecar_entrypoint import app

from src.infrastructure.ai.service_identity import (
    SERVICE_AUDIENCE,
    SERVICE_IDENTITY_HEADER,
    SERVICE_ISSUER,
    SERVICE_SUBJECT,
)


@pytest.fixture(scope="module")
def client() -> TestClient:
    return TestClient(app)


def _create_token(path: str, secret: str | None = None, expired: bool = False) -> str:
    secret = secret or os.environ["JWT_SECRET"]
    now = int(time.time())
    payload = {
        "iss": SERVICE_ISSUER,
        "sub": SERVICE_SUBJECT,
        "aud": SERVICE_AUDIENCE,
        "iat": now,
        "exp": now - 10 if expired else now + 300,
        "path": path,
    }
    return jwt.encode(payload, secret, algorithm="HS256")


def _valid_envelope_body(**overrides: Any) -> dict[str, Any]:
    base: dict[str, Any] = {
        "contract_version": "ai-runtime.v1",
        "job_id": "job_stream_001",
        "run_id": "run_stream_001",
        "correlation_id": "corr_stream",
        "requester": {"user_id": "user_1", "roles": ["ai:chat"]},
        "ontology": {
            "version": "flight-ops.v1",
            "allowed_object_types": ["Flight"],
            "allowed_actions": ["Flight.add_note"],
            "risk_ceiling": "medium",
        },
        "context": {
            "objects": [
                {
                    "object_type": "Flight",
                    "object_id": "FL123",
                    "data": {"flight_number": "CA1234", "status": "scheduled"},
                }
            ],
            "limits": {},
        },
        "task": {"task_type": "nl_query", "user_message": "What is the status of CA1234?"},
    }
    base.update(overrides)
    return base


def _parse_sse_events(raw: str) -> list[tuple[str, dict[str, Any]]]:
    events: list[tuple[str, dict[str, Any]]] = []
    current_event = ""
    current_data = ""
    for line in raw.split("\n"):
        if line.startswith("event: "):
            current_event = line[7:].strip()
        elif line.startswith("data: "):
            current_data = line[6:]
        elif line == "" and current_event and current_data:
            try:
                parsed = json.loads(current_data)
            except json.JSONDecodeError:
                parsed = {"raw": current_data}
            events.append((current_event, parsed))
            current_event = ""
            current_data = ""
    return events


class TestStreamAuth:
    def test_stream_no_identity_returns_401(self, client: TestClient) -> None:
        resp = client.post(
            "/internal/ai/v1/runs/stream",
            json=_valid_envelope_body(),
        )
        assert resp.status_code == 401
        detail = resp.json().get("detail", {})
        assert detail.get("code") == "MISSING_SERVICE_IDENTITY"

    def test_stream_wrong_path_identity_returns_403(self, client: TestClient) -> None:
        token = _create_token("/different/path")
        resp = client.post(
            "/internal/ai/v1/runs/stream",
            json=_valid_envelope_body(),
            headers={SERVICE_IDENTITY_HEADER: token},
        )
        assert resp.status_code == 403
        detail = resp.json().get("detail", {})
        assert detail.get("code") == "PATH_MISMATCH"

    def test_stream_expired_identity_returns_401(self, client: TestClient) -> None:
        token = _create_token("/internal/ai/v1/runs/stream", expired=True)
        resp = client.post(
            "/internal/ai/v1/runs/stream",
            json=_valid_envelope_body(),
            headers={SERVICE_IDENTITY_HEADER: token},
        )
        assert resp.status_code == 401


class TestStreamSuccessPath:
    def test_stream_valid_identity_returns_sse_with_progress_token_complete(self, client: TestClient) -> None:
        token = _create_token("/internal/ai/v1/runs/stream")
        with client.stream(
            "POST",
            "/internal/ai/v1/runs/stream",
            json=_valid_envelope_body(),
            headers={SERVICE_IDENTITY_HEADER: token},
        ) as resp:
            assert resp.status_code == 200
            assert "text/event-stream" in resp.headers.get("content-type", "")

            raw = ""
            for chunk in resp.iter_text():
                raw += chunk

            events = _parse_sse_events(raw)
            event_types = [e[0] for e in events]

            assert "progress" in event_types, f"Expected progress event, got: {event_types}"
            assert "token" in event_types, f"Expected token event, got: {event_types}"
            assert "run.complete" in event_types, f"Expected run.complete event, got: {event_types}"

            complete_events = [e[1] for e in events if e[0] == "run.complete"]
            assert len(complete_events) == 1
            data = complete_events[0]
            assert data["status"] == "succeeded"
            assert data["run_id"] == "run_stream_001"
            assert data["answer"]
            assert "contract_version" in data
            assert "reasoning_steps" in data
            assert "evidence" in data
            assert "proposals" in data

    def test_stream_token_deltas_concat_to_answer(self, client: TestClient) -> None:
        token = _create_token("/internal/ai/v1/runs/stream")
        with client.stream(
            "POST",
            "/internal/ai/v1/runs/stream",
            json=_valid_envelope_body(),
            headers={SERVICE_IDENTITY_HEADER: token},
        ) as resp:
            raw = ""
            for chunk in resp.iter_text():
                raw += chunk

            events = _parse_sse_events(raw)
            token_deltas = "".join(e[1].get("delta", "") for e in events if e[0] == "token")
            complete_events = [e[1] for e in events if e[0] == "run.complete"]
            answer = complete_events[0]["answer"]
            assert token_deltas == answer, "Token deltas must concatenate to final answer"


class TestStreamDegradedPath:
    def test_stream_no_openai_key_returns_succeeded_degraded(
        self, client: TestClient, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        monkeypatch.delenv("OPENAI_API_KEY", raising=False)
        import src.infrastructure.ai.runtime_service as rs_mod

        rs_mod._default_runtime_service = None

        token = _create_token("/internal/ai/v1/runs/stream")
        with client.stream(
            "POST",
            "/internal/ai/v1/runs/stream",
            json=_valid_envelope_body(),
            headers={SERVICE_IDENTITY_HEADER: token},
        ) as resp:
            assert resp.status_code == 200
            raw = ""
            for chunk in resp.iter_text():
                raw += chunk

            events = _parse_sse_events(raw)
            complete_events = [e[1] for e in events if e[0] == "run.complete"]
            assert len(complete_events) == 1
            data = complete_events[0]
            assert data["status"] == "succeeded", "degraded must still be succeeded"
            assert data["limitations"], "degraded path must have limitations"
            assert data["metrics"]["model"] == "heuristic-runtime-v1"


class TestStreamInvalidEnvelope:
    def test_stream_invalid_envelope_returns_run_complete_failed(self, client: TestClient) -> None:
        token = _create_token("/internal/ai/v1/runs/stream")
        with client.stream(
            "POST",
            "/internal/ai/v1/runs/stream",
            json=_valid_envelope_body(run_id="", job_id=""),
            headers={SERVICE_IDENTITY_HEADER: token},
        ) as resp:
            assert resp.status_code == 200, "must NOT return HTTP 500"
            assert "text/event-stream" in resp.headers.get("content-type", "")

            raw = ""
            for chunk in resp.iter_text():
                raw += chunk

            events = _parse_sse_events(raw)
            event_types = [e[0] for e in events]
            assert "run.complete" in event_types, f"Expected run.complete for invalid envelope, got: {event_types}"

            complete_events = [e[1] for e in events if e[0] == "run.complete"]
            data = complete_events[0]
            assert data["status"] == "failed"

    def test_stream_malformed_body_returns_run_fail(self, client: TestClient) -> None:
        token = _create_token("/internal/ai/v1/runs/stream")
        with client.stream(
            "POST",
            "/internal/ai/v1/runs/stream",
            json={"contract_version": "ai-runtime.v1"},
            headers={SERVICE_IDENTITY_HEADER: token},
        ) as resp:
            assert resp.status_code == 200, "must NOT return HTTP 500"
            assert "text/event-stream" in resp.headers.get("content-type", "")

            raw = ""
            for chunk in resp.iter_text():
                raw += chunk

            events = _parse_sse_events(raw)
            event_types = [e[0] for e in events]
            assert "run.fail" in event_types, f"Expected run.fail for malformed body, got: {event_types}"

            fail_events = [e[1] for e in events if e[0] == "run.fail"]
            data = fail_events[0]
            assert data["status"] == "failed"


class TestStreamTransportAbort:
    def test_mid_stream_provider_error_emits_transport_abort(
        self, client: TestClient, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        from src.infrastructure.ai.runtime_llm import FakeStreamingLlmClient
        from src.infrastructure.ai.runtime_service import RuntimeService

        failing = FakeStreamingLlmClient(
            tokens=["alpha", "beta"],
            raise_after_tokens=1,
        )
        monkeypatch.setattr(
            "src.infrastructure.ai.api_routes.get_runtime_service",
            lambda: RuntimeService(streaming_llm_client=failing),
        )

        token = _create_token("/internal/ai/v1/runs/stream")
        with client.stream(
            "POST",
            "/internal/ai/v1/runs/stream",
            json=_valid_envelope_body(),
            headers={SERVICE_IDENTITY_HEADER: token},
        ) as resp:
            assert resp.status_code == 200
            raw = ""
            for chunk in resp.iter_text():
                raw += chunk

            events = _parse_sse_events(raw)
            event_types = [e[0] for e in events]
            assert "transport.abort" in event_types
            assert "run.complete" not in event_types
            abort_msg = next(e[1]["message"] for e in events if e[0] == "transport.abort")
            assert abort_msg
            assert "sk-" not in abort_msg.lower()


class TestStreamSSEFormat:
    def test_stream_events_follow_sse_protocol(self, client: TestClient) -> None:
        token = _create_token("/internal/ai/v1/runs/stream")
        with client.stream(
            "POST",
            "/internal/ai/v1/runs/stream",
            json=_valid_envelope_body(),
            headers={SERVICE_IDENTITY_HEADER: token},
        ) as resp:
            raw = ""
            for chunk in resp.iter_text():
                raw += chunk

            assert "event: progress\n" in raw
            assert "event: token\n" in raw
            assert "event: run.complete\n" in raw
            for line in raw.split("\n"):
                if line.startswith("data: "):
                    json_str = line[6:]
                    parsed = json.loads(json_str)
                    assert isinstance(parsed, dict)

    def test_stream_cache_control_no_cache(self, client: TestClient) -> None:
        token = _create_token("/internal/ai/v1/runs/stream")
        with client.stream(
            "POST",
            "/internal/ai/v1/runs/stream",
            json=_valid_envelope_body(),
            headers={SERVICE_IDENTITY_HEADER: token},
        ) as resp:
            cache_control = resp.headers.get("cache-control", "")
            assert "no-cache" in cache_control


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
